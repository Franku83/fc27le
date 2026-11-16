//! Verified, read-only enumeration of the FC 27 `players` table.
//!
//! # Evidence
//!
//! The layout is not inferred from a single sample. It matches three
//! independent descriptions:
//!
//! 1. The documented Live Editor Lua API behavior:
//!    first record at descriptor `+0x30`, record size at `+0x44`, slot count
//!    (`written_records`) as a `u16` at `+0x7C`, and a record is skipped when
//!    the high bit (`0x80`) of its last byte is set.
//! 2. The documented bit-field reader: a field is an LSB-first bit slice
//!    at descriptor field entry `+0x4`, and the metadata low bound is added to
//!    the stored value.
//! 3. Independent static analysis of the table iteration logic: the loop ends at
//!    `data + (count - 1) * record_size` and skips records whose final byte has
//!    bit 7 set.
//!
//! Live cross-checks across all 273 descriptors in the tested build: every
//! non-empty table (232) satisfies `slot_count <= capacity` (`u16` at `+0x78`)
//! and `invalid_rows == u16 at +0x7E`. For `CZUM`, `+0x7C = 22098`,
//! `+0x78 = 27000`, `+0x7E = 284`, and `22098 - 284 = 21814`, which equals the
//! table's playerid-index count at index-object `+0x20`.
//!
//! Nothing here writes memory, calls a game function, or scans the heap. Every
//! read is bounded to the verified record range and split into chunks no larger
//! than [`fc27_process::MAX_SINGLE_READ`].

use std::collections::BTreeMap;

use fc27_core::{Error, Result};
use fc27_process::{MAX_SINGLE_READ, ProcessHandle};
use serde::Serialize;

use crate::db_metadata::{Metadata, MetadataField};
use crate::db_service::{DbField, DbTable, read_table_fields};

/// Metadata table name that was resolved to 4CC `CZUM` in WP3.
pub const PLAYERS_TABLE: &str = "players";
/// Fields decoded by the verified player reader. The first three are name IDs
/// resolved through the name tables; the rest are the numeric output.
pub const PLAYER_FIELDS: [&str; 8] = [
    "playerid",
    "firstnameid",
    "lastnameid",
    "commonnameid",
    "overallrating",
    "potential",
    "height",
    "weight",
];
/// Safety cap for a single record; the verified players record is 156 bytes.
pub const MAX_RECORD_SIZE: u32 = 4096;
/// Safety cap for one full table snapshot (64 MiB).
pub const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
/// Deleted/free marker in the final byte of a slot.
pub const INVALID_MARKER: u8 = 0x80;

#[derive(Debug, Clone, Serialize)]
pub struct PlayerField {
    pub name: String,
    pub shortname: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    /// Metadata `rangelow`; the stored bits are `value - low`.
    pub low: i64,
    pub high: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerIndexInfo {
    pub address: u64,
    pub shortname: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerTableLayout {
    pub table_name: String,
    pub shortname: String,
    pub descriptor: u64,
    pub data_pointer: u64,
    pub record_size: u32,
    /// Slots to scan: `u16` at descriptor `+0x7C`.
    pub slot_count: u32,
    /// Allocated slots: `u16` at descriptor `+0x78`.
    pub capacity: u32,
    /// Deleted/free slots claimed by `u16` at descriptor `+0x7E`.
    pub invalid_marker_count: u32,
    /// Optional playerid-index runtime (`QVtz`) at descriptor `+0x18`.
    pub index: Option<PlayerIndexInfo>,
    pub fields: Vec<PlayerField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerRecord {
    pub record_index: u32,
    pub player_id: u32,
    pub first_name_id: u16,
    pub last_name_id: u16,
    pub common_name_id: u16,
    pub overall_rating: u8,
    pub potential: u8,
    pub height_cm: u8,
    pub weight_kg: u8,
    /// Filled by the name resolver; null when names are disabled or unresolved.
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub common_name: Option<String>,
    pub display_name: Option<String>,
    pub name_source: Option<String>,
    pub name_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerCheck {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerScan {
    pub layout: PlayerTableLayout,
    pub records: Vec<PlayerRecord>,
    pub slots_scanned: u32,
    pub valid_slots: u32,
    pub invalid_slots: u32,
    pub truncated: bool,
    pub stable_reads: bool,
    pub checks: Vec<PlayerCheck>,
    pub all_checks_passed: bool,
}

impl PlayerScan {
    pub fn skipped_invalid(&self) -> u32 {
        self.invalid_slots
    }
}

/// Resolve the live `players` descriptor and validate every layout invariant
/// needed for enumeration.
pub fn resolve_layout(
    handle: &dyn ProcessHandle,
    metadata: &Metadata,
    live_tables: &[DbTable],
) -> Result<PlayerTableLayout> {
    let metadata_table = metadata.table_by_name(PLAYERS_TABLE).ok_or_else(|| {
        Error::InsufficientEvidence(format!(
            "metadata has no `{PLAYERS_TABLE}` table; refusing to guess a 4CC"
        ))
    })?;
    let live = live_tables
        .iter()
        .find(|table| table.shortname == metadata_table.shortname)
        .ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "live table manager has no descriptor for metadata table `{PLAYERS_TABLE}` \
                 ({})",
                metadata_table.shortname
            ))
        })?;

    if !metadata_table.partial && live.field_count_byte as usize != metadata_table.fields.len() {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` field count mismatch: metadata {} vs live {}",
            metadata_table.fields.len(),
            live.field_count_byte
        )));
    }
    if live.header_44 == 0 || live.header_44 > MAX_RECORD_SIZE {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` record size {} is outside the supported range 1..={MAX_RECORD_SIZE}",
            live.header_44
        )));
    }
    if live.data_pointer == 0 {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` data pointer is null; the table is not loaded"
        )));
    }

    let live_fields = read_table_fields(handle, live)?;
    let live_by_short: BTreeMap<&str, &DbField> = live_fields
        .iter()
        .map(|field| (field.shortname.as_str(), field))
        .collect();
    if live_by_short.len() != live_fields.len() {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` has duplicate live field 4CCs; refusing ambiguous layout"
        )));
    }
    let metadata_by_short: BTreeMap<&str, &MetadataField> = metadata_table
        .fields
        .iter()
        .map(|field| (field.shortname.as_str(), field))
        .collect();
    if metadata_by_short.len() != metadata_table.fields.len() {
        return Err(Error::InsufficientEvidence(format!(
            "metadata for `{PLAYERS_TABLE}` has duplicate field 4CCs"
        )));
    }

    let row_bits = live
        .header_44
        .checked_mul(8)
        .ok_or_else(|| Error::parse("players layout", "record size overflow"))?;
    let mut total_width = 0u32;
    for field in &metadata_table.fields {
        let live_field = live_by_short.get(field.shortname.as_str()).ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "metadata field {}.{} ({}) has no live descriptor entry",
                metadata_table.name, field.name, field.shortname
            ))
        })?;
        if live_field.bit_width != u32::from(field.depth) {
            return Err(Error::InsufficientEvidence(format!(
                "field {}.{} width mismatch: metadata {} vs live {}",
                metadata_table.name, field.name, field.depth, live_field.bit_width
            )));
        }
        if live_field.bit_width == 0 {
            return Err(Error::InsufficientEvidence(format!(
                "field {}.{} has zero width",
                metadata_table.name, field.name
            )));
        }
        let end = live_field
            .bit_offset
            .checked_add(live_field.bit_width)
            .ok_or_else(|| Error::parse("players layout", "field end overflow"))?;
        if end > row_bits {
            return Err(Error::InsufficientEvidence(format!(
                "field {}.{} ends at bit {end}, beyond the {row_bits}-bit record",
                metadata_table.name, field.name
            )));
        }
        total_width = total_width
            .checked_add(live_field.bit_width)
            .ok_or_else(|| Error::parse("players layout", "field width sum overflow"))?;
    }
    if total_width > row_bits {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` field widths total {total_width} bits, larger than the \
             {row_bits}-bit record"
        )));
    }
    let padding_bits = row_bits - total_width;
    if !metadata_table.partial && padding_bits > 7 {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` has {padding_bits} padding bits; the fixed-row layout requires \
             less than 8"
        )));
    }

    let mut fields = Vec::with_capacity(PLAYER_FIELDS.len());
    for name in PLAYER_FIELDS {
        let metadata_field = metadata_table
            .fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!(
                    "metadata for `{PLAYERS_TABLE}` has no `{name}` field"
                ))
            })?;
        if !metadata_field.type_name.contains("INTEGER")
            && !metadata_field.type_name.contains("DATE")
        {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERS_TABLE}.{name}` has unsupported type {}",
                metadata_field.type_name
            )));
        }
        let live_field = live_by_short
            .get(metadata_field.shortname.as_str())
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!(
                    "`{PLAYERS_TABLE}.{name}` ({}) has no live field entry",
                    metadata_field.shortname
                ))
            })?;
        let high = metadata_field.range_high.ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "`{PLAYERS_TABLE}.{name}` metadata has no rangehigh"
            ))
        })?;
        let low = metadata_field.range_low.unwrap_or(0);
        if low > high {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERS_TABLE}.{name}` has inverted range {low}..{high}"
            )));
        }
        let span = high
            .checked_sub(low)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| Error::parse("players layout", "field range overflow"))?;
        let capacity = 1u64
            .checked_shl(live_field.bit_width)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| Error::parse("players layout", "field width too large"))?;
        if span > capacity {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERS_TABLE}.{name}` range {low}..{high} does not fit in {} bits",
                live_field.bit_width
            )));
        }
        fields.push(PlayerField {
            name: name.to_string(),
            shortname: metadata_field.shortname.clone(),
            bit_offset: live_field.bit_offset,
            bit_width: live_field.bit_width,
            low,
            high,
        });
    }

    let slot_count = u32::from(handle.read_u16(offset(live.address, 0x7C)?)?);
    let capacity = u32::from(handle.read_u16(offset(live.address, 0x78)?)?);
    let invalid_marker_count = u32::from(handle.read_u16(offset(live.address, 0x7E)?)?);

    let index_pointer = handle.read_u64(offset(live.address, 0x18)?)?;
    let index = if index_pointer == 0 {
        None
    } else {
        let code = handle.read_u32(offset(index_pointer, 0x1C)?)?;
        Some(PlayerIndexInfo {
            address: index_pointer,
            shortname: fourcc(code),
            count: handle.read_u32(offset(index_pointer, 0x20)?)?,
        })
    };

    validate_counts(slot_count, capacity)?;

    let scan_bytes = u64::from(slot_count)
        .checked_mul(u64::from(live.header_44))
        .ok_or_else(|| Error::parse("players layout", "scan size overflow"))?;
    if scan_bytes > MAX_SCAN_BYTES {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` scan would read {scan_bytes} bytes, above the {MAX_SCAN_BYTES} \
             byte cap"
        )));
    }

    // Preflight both ends of the verified range. The full snapshot is read by
    // `scan_players`, but this gives an immediate, precise error for a stale or
    // unmapped data pointer.
    let record_size = live.header_44 as usize;
    if slot_count > 0 {
        handle.read(live.data_pointer, record_size)?;
        let last = live
            .data_pointer
            .checked_add(scan_bytes - u64::from(live.header_44))
            .ok_or_else(|| Error::parse("players layout", "last record address overflow"))?;
        handle.read(last, record_size)?;
    }

    Ok(PlayerTableLayout {
        table_name: metadata_table.name.clone(),
        shortname: metadata_table.shortname.clone(),
        descriptor: live.address,
        data_pointer: live.data_pointer,
        record_size: live.header_44,
        slot_count,
        capacity,
        invalid_marker_count,
        index,
        fields,
    })
}

/// Read the verified range once. The process API caps a single read at
/// [`MAX_SINGLE_READ`], so longer ranges are chunked here.
pub fn read_range(handle: &dyn ProcessHandle, address: u64, length: usize) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(length);
    let mut offset = 0usize;
    while offset < length {
        let chunk = (length - offset).min(MAX_SINGLE_READ);
        let chunk_address = address
            .checked_add(offset as u64)
            .ok_or_else(|| Error::parse("player read", "address overflow"))?;
        let bytes = handle.read(chunk_address, chunk)?;
        if bytes.len() != chunk {
            return Err(Error::ShortRead {
                address: chunk_address,
                requested: chunk,
                received: bytes.len(),
            });
        }
        output.extend_from_slice(&bytes);
        offset += chunk;
    }
    Ok(output)
}

/// Resolve, snapshot twice, require identical reads, then decode.
pub fn scan_players(
    handle: &dyn ProcessHandle,
    metadata: &Metadata,
    live_tables: &[DbTable],
    limit: usize,
) -> Result<PlayerScan> {
    let layout = resolve_layout(handle, metadata, live_tables)?;
    let length = (layout.record_size as usize)
        .checked_mul(layout.slot_count as usize)
        .ok_or_else(|| Error::parse("player scan", "snapshot size overflow"))?;
    let first = read_range(handle, layout.data_pointer, length)?;
    let second = read_range(handle, layout.data_pointer, length)?;
    if first != second {
        return Err(Error::InsufficientEvidence(
            "the players table changed between two consecutive snapshots; refusing unstable data"
                .to_string(),
        ));
    }
    let mut scan = scan_snapshot(&first, &layout, limit)?;
    scan.stable_reads = true;

    // The playerid-index runtime is a second, independent count. It matched the
    // validity-bit count for every indexed table in the tested build.
    if let Some(index) = &layout.index {
        let current = handle.read_u32(offset(index.address, 0x20)?)?;
        let passed = current == scan.valid_slots;
        scan.checks.push(check(
            "index_count_matches",
            passed,
            format!(
                "index {} at {:#x} reports {current}, validity scan found {}",
                index.shortname, index.address, scan.valid_slots
            ),
        ));
        scan.all_checks_passed = scan.checks.iter().all(|check| check.passed);
        if !passed {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERS_TABLE}` index count {current} does not match {} valid slots;                  refusing inconsistent data",
                scan.valid_slots
            )));
        }
    }

    Ok(scan)
}

/// Pure snapshot scanner. `buffer` must contain exactly
/// `slot_count * record_size` bytes.
pub fn scan_snapshot(
    buffer: &[u8],
    layout: &PlayerTableLayout,
    limit: usize,
) -> Result<PlayerScan> {
    let record_size = layout.record_size as usize;
    let expected = record_size
        .checked_mul(layout.slot_count as usize)
        .ok_or_else(|| Error::parse("player scan", "snapshot size overflow"))?;
    if buffer.len() < expected {
        return Err(Error::ShortRead {
            address: layout.data_pointer,
            requested: expected,
            received: buffer.len(),
        });
    }

    let requested = limit.min(layout.slot_count as usize);
    let mut records = Vec::with_capacity(requested);
    let mut valid_slots = 0u32;
    let mut invalid_slots = 0u32;

    for index in 0..layout.slot_count {
        let start = index as usize * record_size;
        let row = &buffer[start..start + record_size];
        if row[record_size - 1] & INVALID_MARKER != 0 {
            invalid_slots += 1;
            continue;
        }
        valid_slots += 1;
        if records.len() < requested {
            let mut record = decode_record(row, layout).map_err(|error| {
                Error::InsufficientEvidence(format!(
                    "players row {index} failed semantic validation: {error}"
                ))
            })?;
            record.record_index = index;
            records.push(record);
        }
    }

    if invalid_slots != layout.invalid_marker_count {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` invalid-row count mismatch: scanned {invalid_slots}, descriptor \
             +0x7E says {}; the storage model is inconsistent",
            layout.invalid_marker_count
        )));
    }

    let checks = vec![
        check(
            "slot_count_nonzero",
            layout.slot_count > 0,
            format!("{} slot(s)", layout.slot_count),
        ),
        check(
            "slot_count_within_capacity",
            layout.slot_count <= layout.capacity,
            format!("{}/{} slot(s)", layout.slot_count, layout.capacity),
        ),
        check(
            "invalid_markers_match",
            invalid_slots == layout.invalid_marker_count,
            format!(
                "scanned {invalid_slots}, descriptor +0x7E {}",
                layout.invalid_marker_count
            ),
        ),
        check(
            "valid_plus_invalid",
            valid_slots + invalid_slots == layout.slot_count,
            format!("{valid_slots} + {invalid_slots} == {}", layout.slot_count),
        ),
        check(
            "records_decoded",
            records.len() == requested.min(valid_slots as usize),
            format!(
                "{} decoded of {} requested ({} valid slot(s))",
                records.len(),
                requested,
                valid_slots
            ),
        ),
    ];
    let all_checks_passed = checks.iter().all(|check| check.passed);

    Ok(PlayerScan {
        layout: layout.clone(),
        records,
        slots_scanned: layout.slot_count,
        valid_slots,
        invalid_slots,
        truncated: valid_slots as usize > requested,
        stable_reads: false,
        checks,
        all_checks_passed,
    })
}

/// Decode one row after the caller has already checked the invalid marker.
pub fn decode_record(row: &[u8], layout: &PlayerTableLayout) -> Result<PlayerRecord> {
    let field = |name: &str| {
        layout
            .fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| Error::InsufficientEvidence(format!("layout has no `{name}` field")))
    };
    let player_id = decode_field(row, field("playerid")?)?;
    if !(1..=i64::from(u32::MAX)).contains(&player_id) {
        return Err(Error::Unsupported(format!(
            "playerid {player_id} is outside the supported positive u32 range"
        )));
    }
    let first_name_id = decode_field(row, field("firstnameid")?)?;
    let last_name_id = decode_field(row, field("lastnameid")?)?;
    let common_name_id = decode_field(row, field("commonnameid")?)?;
    let overall = decode_field(row, field("overallrating")?)?;
    let potential = decode_field(row, field("potential")?)?;
    let height = decode_field(row, field("height")?)?;
    let weight = decode_field(row, field("weight")?)?;

    Ok(PlayerRecord {
        record_index: 0, // filled by the caller
        player_id: player_id as u32,
        first_name_id: to_u16("firstnameid", first_name_id)?,
        last_name_id: to_u16("lastnameid", last_name_id)?,
        common_name_id: to_u16("commonnameid", common_name_id)?,
        overall_rating: to_u8("overallrating", overall)?,
        potential: to_u8("potential", potential)?,
        height_cm: to_u8("height", height)?,
        weight_kg: to_u8("weight", weight)?,
        first_name: None,
        last_name: None,
        common_name: None,
        display_name: None,
        name_source: None,
        name_error: None,
    })
}

/// LSB-first bit extraction from a byte slice.
///
/// For bit `k` the containing byte is `k / 8` and the bit position inside that
/// byte is `k % 8`; the first bit becomes the least significant bit of the
/// result. This matches the documented Live Editor field-reader semantics.
pub fn read_bits(row: &[u8], bit_offset: u32, bit_width: u32) -> Result<u64> {
    if bit_width == 0 || bit_width > 64 {
        return Err(Error::Unsupported(format!(
            "bit width {bit_width} is outside 1..=64"
        )));
    }
    let end = u64::from(bit_offset)
        .checked_add(u64::from(bit_width))
        .ok_or_else(|| Error::parse("bit field", "bit offset overflow"))?;
    if end > (row.len() as u64) * 8 {
        return Err(Error::InsufficientEvidence(format!(
            "bit field {bit_offset}+{bit_width} exceeds the {}-byte row",
            row.len()
        )));
    }
    let mut value = 0u64;
    for bit in 0..bit_width {
        let absolute = u64::from(bit_offset) + u64::from(bit);
        let byte = row[(absolute / 8) as usize];
        if (byte >> (absolute % 8)) & 1 != 0 {
            value |= 1u64 << bit;
        }
    }
    Ok(value)
}

fn decode_field(row: &[u8], field: &PlayerField) -> Result<i64> {
    let raw = read_bits(row, field.bit_offset, field.bit_width)? as i64;
    let value = raw
        .checked_add(field.low)
        .ok_or_else(|| Error::parse("bit field", format!("{} bias overflow", field.name)))?;
    if value < field.low || value > field.high {
        return Err(Error::Unsupported(format!(
            "field {} decoded {value} outside [{}, {}]",
            field.name, field.low, field.high
        )));
    }
    Ok(value)
}

fn to_u8(name: &str, value: i64) -> Result<u8> {
    u8::try_from(value)
        .map_err(|_| Error::Unsupported(format!("field {name} value {value} does not fit in u8")))
}

fn to_u16(name: &str, value: i64) -> Result<u16> {
    u16::try_from(value)
        .map_err(|_| Error::Unsupported(format!("field {name} value {value} does not fit in u16")))
}

fn validate_counts(slot_count: u32, capacity: u32) -> Result<()> {
    if capacity == 0 {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` capacity is zero; the game has not loaded player storage"
        )));
    }
    if slot_count > capacity {
        return Err(Error::InsufficientEvidence(format!(
            "`{PLAYERS_TABLE}` slot count {slot_count} exceeds capacity {capacity}"
        )));
    }
    Ok(())
}

fn fourcc(code: u32) -> String {
    code.to_le_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect()
}

fn offset(base: u64, delta: u64) -> Result<u64> {
    base.checked_add(delta)
        .ok_or_else(|| Error::parse("players layout", "address overflow"))
}

fn check(name: &'static str, passed: bool, detail: String) -> PlayerCheck {
    PlayerCheck {
        name,
        passed,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_layout() -> PlayerTableLayout {
        PlayerTableLayout {
            table_name: "players".to_string(),
            shortname: "TEST".to_string(),
            descriptor: 0x1000,
            data_pointer: 0x2000,
            record_size: 12,
            slot_count: 3,
            capacity: 4,
            invalid_marker_count: 1,
            index: None,
            fields: vec![
                PlayerField {
                    name: "playerid".to_string(),
                    shortname: "pid1".to_string(),
                    bit_offset: 0,
                    bit_width: 16,
                    low: 0,
                    high: 65535,
                },
                PlayerField {
                    name: "firstnameid".to_string(),
                    shortname: "fnm1".to_string(),
                    bit_offset: 16,
                    bit_width: 16,
                    low: 0,
                    high: 65535,
                },
                PlayerField {
                    name: "lastnameid".to_string(),
                    shortname: "lnm1".to_string(),
                    bit_offset: 32,
                    bit_width: 16,
                    low: 0,
                    high: 65535,
                },
                PlayerField {
                    name: "commonnameid".to_string(),
                    shortname: "cnm1".to_string(),
                    bit_offset: 48,
                    bit_width: 16,
                    low: 0,
                    high: 65535,
                },
                PlayerField {
                    name: "overallrating".to_string(),
                    shortname: "ovr1".to_string(),
                    bit_offset: 64,
                    bit_width: 7,
                    low: 1,
                    high: 99,
                },
                PlayerField {
                    name: "potential".to_string(),
                    shortname: "pot1".to_string(),
                    bit_offset: 71,
                    bit_width: 7,
                    low: 1,
                    high: 99,
                },
                PlayerField {
                    name: "height".to_string(),
                    shortname: "hgt1".to_string(),
                    bit_offset: 78,
                    bit_width: 7,
                    low: 130,
                    high: 220,
                },
                PlayerField {
                    name: "weight".to_string(),
                    shortname: "wgt1".to_string(),
                    bit_offset: 85,
                    bit_width: 7,
                    low: 30,
                    high: 150,
                },
            ],
        }
    }

    fn write_bits(row: &mut [u8], bit_offset: u32, bit_width: u32, value: u64) {
        for bit in 0..bit_width {
            let absolute = bit_offset + bit;
            let byte = &mut row[(absolute / 8) as usize];
            let mask = 1u8 << (absolute % 8);
            if (value >> bit) & 1 != 0 {
                *byte |= mask;
            } else {
                *byte &= !mask;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_row(
        player_id: u64,
        first_name_id: u64,
        last_name_id: u64,
        common_name_id: u64,
        overall: u64,
        potential: u64,
        height_cm: u64,
        weight_kg: u64,
        invalid: bool,
    ) -> [u8; 12] {
        let mut row = [0u8; 12];
        write_bits(&mut row, 0, 16, player_id);
        write_bits(&mut row, 16, 16, first_name_id);
        write_bits(&mut row, 32, 16, last_name_id);
        write_bits(&mut row, 48, 16, common_name_id);
        write_bits(&mut row, 64, 7, overall - 1);
        write_bits(&mut row, 71, 7, potential - 1);
        write_bits(&mut row, 78, 7, height_cm - 130);
        write_bits(&mut row, 85, 7, weight_kg - 30);
        if invalid {
            let last = row.len() - 1;
            row[last] |= INVALID_MARKER;
        }
        row
    }

    #[test]
    fn reads_lsb_first() {
        // bits 0..=8 -> 0b1_0101_0011 = 0x153
        let mut row = [0u8; 4];
        write_bits(&mut row, 0, 9, 0x153);
        assert_eq!(read_bits(&row, 0, 9).unwrap(), 0x153);
        assert_eq!(read_bits(&row, 2, 5).unwrap(), (0x153 >> 2) & 0x1f);
    }

    #[test]
    fn rejects_out_of_bounds_bits() {
        let row = [0u8; 4];
        assert!(read_bits(&row, 30, 5).is_err());
        assert!(read_bits(&row, 0, 0).is_err());
    }

    #[test]
    fn decodes_biased_fields() {
        let layout = test_layout();
        let row = encode_row(20801, 6183, 42468, 8211, 84, 94, 187, 85, false);
        let record = decode_record(&row, &layout).unwrap();
        assert_eq!(record.player_id, 20801);
        assert_eq!(record.first_name_id, 6183);
        assert_eq!(record.last_name_id, 42468);
        assert_eq!(record.common_name_id, 8211);
        assert_eq!(record.overall_rating, 84);
        assert_eq!(record.potential, 94);
        assert_eq!(record.height_cm, 187);
        assert_eq!(record.weight_kg, 85);
    }

    #[test]
    fn skips_invalid_marker_and_counts_it() {
        let layout = test_layout();
        let row0 = encode_row(1, 10, 20, 0, 50, 60, 180, 75, false);
        let row1 = encode_row(2, 11, 21, 0, 51, 61, 181, 76, true);
        let row2 = encode_row(3, 12, 22, 0, 52, 62, 182, 77, false);
        let snapshot = [row0, row1, row2].concat();
        let scan = scan_snapshot(&snapshot, &layout, 10).unwrap();
        assert_eq!(scan.slots_scanned, 3);
        assert_eq!(scan.valid_slots, 2);
        assert_eq!(scan.invalid_slots, 1);
        assert!(scan.all_checks_passed);
        assert_eq!(scan.records.len(), 2);
        assert_eq!(scan.records[0].player_id, 1);
        assert_eq!(scan.records[1].player_id, 3);
    }

    #[test]
    fn limit_truncates_records_but_still_scans_all_slots() {
        let layout = test_layout();
        let snapshot = [
            encode_row(1, 10, 20, 0, 50, 60, 180, 75, false),
            encode_row(2, 11, 21, 0, 51, 61, 181, 76, true),
            encode_row(3, 12, 22, 0, 52, 62, 182, 77, false),
        ]
        .concat();
        let scan = scan_snapshot(&snapshot, &layout, 1).unwrap();
        assert_eq!(scan.records.len(), 1);
        assert_eq!(scan.records[0].player_id, 1);
        assert!(scan.truncated);
    }

    #[test]
    fn rejects_invalid_marker_count_mismatch() {
        let layout = test_layout();
        let snapshot = [
            encode_row(1, 10, 20, 0, 50, 60, 180, 75, false),
            encode_row(2, 11, 21, 0, 51, 61, 181, 76, false),
            encode_row(3, 12, 22, 0, 52, 62, 182, 77, false),
        ]
        .concat();
        let error = scan_snapshot(&snapshot, &layout, 10).unwrap_err();
        assert!(error.is_insufficient_evidence());
    }

    #[test]
    fn rejects_semantic_out_of_range() {
        let layout = test_layout();
        let mut row = encode_row(1, 10, 20, 0, 50, 60, 180, 75, false);
        write_bits(&mut row, 78, 7, 100); // height 230, above the metadata maximum 220
        let error = decode_record(&row, &layout).unwrap_err();
        assert!(matches!(error, Error::Unsupported(_)));
    }

    #[test]
    fn rejects_slot_count_above_capacity() {
        assert!(validate_counts(101, 100).is_err());
        assert!(validate_counts(100, 100).is_ok());
        assert!(validate_counts(0, 100).is_ok());
        assert!(validate_counts(0, 0).is_err());
    }

    #[test]
    fn rejects_short_snapshot() {
        let layout = test_layout();
        let error = scan_snapshot(&[0u8; 8], &layout, 10).unwrap_err();
        assert!(matches!(error, Error::ShortRead { .. }));
    }
}
