//! Verified read-only player-name resolution for FC 27.
//!
//! # Name storage model (WP5)
//!
//! `playernames` (`BGwe`) is a 12-byte fixed-row table:
//!
//! ```text
//! +0x00  u32  byte offset into the string pool
//! +0x04  u16  nameid
//! +0x06  u16  commentaryid
//! +0x08  u32  zero / unused
//! ```
//!
//! The pool base is the table descriptor's `u64` at `+0x38`. The pool starts
//! with a breadth-first serialized binary Huffman tree:
//!
//! ```text
//! node n: u16 left, u16 right       (4 bytes per node, root = node 0)
//! internal child: node index (high byte 0)
//! leaf child:     UTF-8 byte << 8   (high byte = character, low byte 0)
//! ```
//!
//! For `BGwe` the tree is 552 bytes / 138 nodes; the first string offset
//! (0x228) equals the tree size. A string entry is:
//!
//! ```text
//! u8  output_byte_count
//! ... Huffman bitstream, MSB-first, no padding between entries
//! ```
//!
//! The output is a UTF-8 byte sequence. Decoding stops after
//! `output_byte_count` bytes. The next name's offset gives a strict upper bound
//! for the current bitstream.
//!
//! `dcplayernames` (`bneD`) covers name IDs `46000..`:
//! `nameid = stored + 46000` (13 bits at bit 304), `name` is a null-terminated
//! inline string in the first 38 bytes of the 40-byte row.
//!
//! `editedplayernames` (`nQVU`) overrides names by `playerid`:
//! `playerid` (19 bits, bias -1) plus four 45-byte inline strings
//! (firstname, commonname, playerjerseyname, surname).
//!
//! No proprietary metadata or bulk name dump is embedded in this module.
//! Tests build synthetic trees and rows.

use std::collections::BTreeMap;

use fc27_core::{Error, Result};
use fc27_process::ProcessHandle;
use serde::Serialize;

use crate::db_metadata::{Metadata, MetadataTable};
use crate::db_service::{DbField, DbTable, read_table_fields};
use crate::player_store::{read_bits, read_range};

pub const PLAYERNAMES_TABLE: &str = "playernames";
pub const DCPLAYERNAMES_TABLE: &str = "dcplayernames";
pub const EDITED_PLAYERNAMES_TABLE: &str = "editedplayernames";

/// Maximum UTF-8 bytes in one name (the length prefix is a `u8`).
pub const MAX_NAME_BYTES: usize = 255;
/// Safety bound for a single compressed name bitstream.
pub const MAX_COMPRESSED_NAME_BYTES: usize = 512;
/// Safety bound for the whole string pool.
pub const MAX_POOL_BYTES: usize = 16 * 1024 * 1024;
/// Safety bound for the serialized Huffman tree.
pub const MAX_TREE_BYTES: usize = 64 * 1024;
/// Safety bound for one name-table snapshot.
pub const MAX_NAME_TABLE_BYTES: u64 = 64 * 1024 * 1024;
/// `dcplayernames` IDs start here.
pub const DC_NAME_ID_BASE: i64 = 46000;

#[derive(Debug, Clone, Serialize)]
pub struct NameCheck {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableSummary {
    pub table: String,
    pub shortname: String,
    pub descriptor: u64,
    pub data_pointer: u64,
    pub record_size: u32,
    pub rows: u32,
    pub capacity: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayernamesSummary {
    pub table: TableSummary,
    pub pool_base: u64,
    pub tree_bytes: u32,
    pub tree_nodes: u32,
    pub min_offset: u32,
    pub max_offset: u32,
    pub unique_name_ids: usize,
    pub decoded_entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditedSummary {
    pub table: TableSummary,
    pub overrides: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NameResolutionReport {
    pub status: &'static str,
    pub playernames: PlayernamesSummary,
    pub dcplayernames: Option<TableSummary>,
    pub editedplayernames: Option<EditedSummary>,
    pub emitted_with_names: usize,
    pub emitted_without_names: usize,
    pub name_errors: usize,
    pub checks: Vec<NameCheck>,
    pub notes: Vec<String>,
    pub all_checks_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedPlayerName {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub common_name: Option<String>,
    pub display_name: Option<String>,
    pub source: &'static str,
    pub error: Option<String>,
}

pub struct PlayerNameResolver {
    playernames: HuffmanNameTable,
    dcplayernames: Option<DcNameTable>,
    editedplayernames: Option<EditedNameTable>,
    checks: Vec<NameCheck>,
    notes: Vec<String>,
}

impl PlayerNameResolver {
    pub fn load(
        handle: &dyn ProcessHandle,
        metadata: &Metadata,
        live_tables: &[DbTable],
    ) -> Result<Self> {
        let playernames = HuffmanNameTable::load(handle, metadata, live_tables)?;
        let dcplayernames = DcNameTable::load_optional(handle, metadata, live_tables)?;
        let editedplayernames = EditedNameTable::load_optional(handle, metadata, live_tables)?;

        let mut checks = vec![
            check(
                "playernames_tree_valid",
                true,
                format!(
                    "{} node(s), {} bytes, pool {:#x}",
                    playernames.summary.tree_nodes,
                    playernames.summary.tree_bytes,
                    playernames.summary.pool_base
                ),
            ),
            check(
                "playernames_offsets_valid",
                true,
                format!(
                    "{} name ID(s), offsets {:#x}..{:#x}",
                    playernames.summary.unique_name_ids,
                    playernames.summary.min_offset,
                    playernames.summary.max_offset
                ),
            ),
            check(
                "playernames_entries_decoded",
                playernames.summary.decoded_entries == playernames.summary.unique_name_ids,
                format!(
                    "{} / {} entries decoded",
                    playernames.summary.decoded_entries, playernames.summary.unique_name_ids
                ),
            ),
        ];
        let mut notes = Vec::new();

        match &dcplayernames {
            Some(table) => checks.push(check(
                "dcplayernames_loaded",
                true,
                format!("{} row(s), IDs 46000..", table.summary.rows),
            )),
            None => notes.push(
                "dcplayernames is not loaded; name IDs >= 46000 will remain unresolved."
                    .to_string(),
            ),
        }
        match &editedplayernames {
            Some(table) => checks.push(check(
                "editedplayernames_loaded",
                true,
                format!("{} override(s)", table.summary.overrides),
            )),
            None => notes.push(
                "editedplayernames is not loaded; no per-player name overrides are applied."
                    .to_string(),
            ),
        }

        Ok(Self {
            playernames,
            dcplayernames,
            editedplayernames,
            checks,
            notes,
        })
    }

    pub fn resolve_player(
        &self,
        player_id: u32,
        first_id: u16,
        last_id: u16,
        common_id: u16,
    ) -> ResolvedPlayerName {
        let mut bg_used = false;
        let mut dc_used = false;

        let mut resolve = |id: u16| -> Option<String> {
            if id == 0 {
                return None;
            }
            match self.playernames.decode(id) {
                Ok(Some(name)) => {
                    bg_used = true;
                    return Some(name);
                }
                Ok(None) => {}
                Err(_) => {
                    return None;
                }
            }
            if let Some(dc) = &self.dcplayernames {
                if let Some(name) = dc.names.get(&id) {
                    dc_used = true;
                    return Some(name.clone());
                }
            }
            None
        };

        let base_first = resolve(first_id);
        let base_last = resolve(last_id);
        let base_common = resolve(common_id);

        let edited = self
            .editedplayernames
            .as_ref()
            .and_then(|table| table.by_player.get(&player_id))
            .cloned();
        let edited_used = edited.as_ref().is_some_and(|name| {
            name.first.is_some() || name.last.is_some() || name.common.is_some()
        });

        let first = edited
            .as_ref()
            .and_then(|name| name.first.clone())
            .or(base_first);
        let last = edited
            .as_ref()
            .and_then(|name| name.last.clone())
            .or(base_last);
        let common = edited
            .as_ref()
            .and_then(|name| name.common.clone())
            .or(base_common);

        // Rebuild errors from the final values so an edited override suppresses
        // a base-table miss for the same field.
        let mut final_errors = Vec::new();
        for (field, id, value) in [
            ("firstnameid", first_id, &first),
            ("lastnameid", last_id, &last),
            ("commonnameid", common_id, &common),
        ] {
            if id != 0 && value.is_none() {
                final_errors.push(format!("{field} {id} unresolved"));
            }
        }

        let display_name = common.clone().filter(|name| !name.is_empty()).or_else(|| {
            let mut parts = Vec::new();
            if let Some(name) = first.as_ref().filter(|name| !name.is_empty()) {
                parts.push(name.clone());
            }
            if let Some(name) = last.as_ref().filter(|name| !name.is_empty()) {
                parts.push(name.clone());
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        });

        let source = if edited_used {
            "editedplayernames"
        } else if bg_used && dc_used {
            "mixed"
        } else if dc_used {
            "dcplayernames"
        } else if bg_used {
            "playernames"
        } else {
            "none"
        };

        ResolvedPlayerName {
            first_name: first,
            last_name: last,
            common_name: common,
            display_name,
            source,
            error: if final_errors.is_empty() {
                None
            } else {
                Some(final_errors.join("; "))
            },
        }
    }

    pub fn report(&self, records: &[crate::player_store::PlayerRecord]) -> NameResolutionReport {
        let emitted_with_names = records
            .iter()
            .filter(|record| record.display_name.is_some())
            .count();
        let emitted_without_names = records.len().saturating_sub(emitted_with_names);
        let name_errors = records
            .iter()
            .filter(|record| record.name_error.is_some())
            .count();
        NameResolutionReport {
            status: "resolved",
            playernames: self.playernames.summary.clone(),
            dcplayernames: self
                .dcplayernames
                .as_ref()
                .map(|table| table.summary.clone()),
            editedplayernames: self
                .editedplayernames
                .as_ref()
                .map(|table| table.summary.clone()),
            emitted_with_names,
            emitted_without_names,
            name_errors,
            checks: self.checks.clone(),
            notes: self.notes.clone(),
            all_checks_passed: self.checks.iter().all(|check| check.passed),
        }
    }
}

struct HuffmanNameTable {
    summary: PlayernamesSummary,
    nodes: Vec<(u16, u16)>,
    pool: Vec<u8>,
    offset_by_id: BTreeMap<u16, u32>,
    offsets: Vec<u32>,
}

impl HuffmanNameTable {
    fn load(
        handle: &dyn ProcessHandle,
        metadata: &Metadata,
        live_tables: &[DbTable],
    ) -> Result<Self> {
        let (metadata_table, live) = require_table(metadata, live_tables, PLAYERNAMES_TABLE)?;
        let fields = field_map(handle, live)?;
        let nameid = field_spec(metadata_table, &fields, "nameid", live.header_44, None)?;
        let name = field_spec(metadata_table, &fields, "name", live.header_44, Some(32))?;
        if nameid.live_type != 3 {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERNAMES_TABLE}.nameid` is not an integer field (live type {})",
                nameid.live_type
            )));
        }
        if name.live_type != 13 {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERNAMES_TABLE}.name` is not a compressed string (live type {})",
                name.live_type
            )));
        }
        if name.bit_offset != 0 || name.bit_width < 32 {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERNAMES_TABLE}.name` has unsupported offset/width {}/{}",
                name.bit_offset, name.bit_width
            )));
        }
        // The declared string width is a capacity, not a fixed row width; only
        // the 4-byte pool reference must fit.
        if name.bit_offset as u64 + 32 > u64::from(live.header_44) * 8 {
            return Err(Error::InsufficientEvidence(format!(
                "`{PLAYERNAMES_TABLE}.name` reference exceeds the {}-byte row",
                live.header_44
            )));
        }

        let summary_base = table_summary(handle, metadata_table, live, false)?;
        let rows = read_rows(handle, &summary_base)?;

        let mut offset_by_id = BTreeMap::new();
        for index in 0..summary_base.rows as usize {
            let start = index * summary_base.record_size as usize;
            let row = &rows[start..start + summary_base.record_size as usize];
            let id = nameid.read_int(row)?;
            if !(0..=i64::from(u16::MAX)).contains(&id) {
                return Err(Error::InsufficientEvidence(format!(
                    "{PLAYERNAMES_TABLE} row {index}: nameid {id} outside u16"
                )));
            }
            let offset = read_bits(row, name.bit_offset, 32)? as u32;
            if offset == u32::MAX {
                continue;
            }
            if offset < 4 || offset as usize >= MAX_POOL_BYTES {
                return Err(Error::InsufficientEvidence(format!(
                    "{PLAYERNAMES_TABLE} row {index}: pool offset {offset:#x} outside bounds"
                )));
            }
            if offset_by_id.insert(id as u16, offset).is_some() {
                return Err(Error::InsufficientEvidence(format!(
                    "{PLAYERNAMES_TABLE} has duplicate nameid {id}"
                )));
            }
        }
        if offset_by_id.is_empty() {
            return Err(Error::InsufficientEvidence(format!(
                "{PLAYERNAMES_TABLE} has no usable string-pool offsets"
            )));
        }

        let min_offset = *offset_by_id
            .values()
            .min()
            .ok_or_else(|| Error::parse("playernames", "no minimum offset"))?;
        let max_offset = *offset_by_id
            .values()
            .max()
            .ok_or_else(|| Error::parse("playernames", "no maximum offset"))?;
        if min_offset < 4 || min_offset as usize > MAX_TREE_BYTES || min_offset % 4 != 0 {
            return Err(Error::InsufficientEvidence(format!(
                "{PLAYERNAMES_TABLE} first string offset {min_offset:#x} is not a valid tree size"
            )));
        }
        let pool_len = (max_offset as usize)
            .checked_add(MAX_COMPRESSED_NAME_BYTES)
            .ok_or_else(|| Error::parse("playernames", "pool size overflow"))?;
        if pool_len > MAX_POOL_BYTES {
            return Err(Error::InsufficientEvidence(format!(
                "{PLAYERNAMES_TABLE} pool needs {pool_len} bytes, above the {MAX_POOL_BYTES} cap"
            )));
        }
        let pool_base = handle.read_u64(offset(live.address, 0x38)?)?;
        if pool_base == 0 {
            return Err(Error::InsufficientEvidence(format!(
                "{PLAYERNAMES_TABLE} has no string-pool pointer at +0x38"
            )));
        }
        let pool = read_range(handle, pool_base, pool_len)?;

        if max_offset as usize >= pool.len() {
            return Err(Error::InsufficientEvidence(format!(
                "{PLAYERNAMES_TABLE} offset {max_offset:#x} exceeds the pool snapshot"
            )));
        }

        let tree_bytes = min_offset as usize;
        let node_count = tree_bytes / 4;
        if !(2..=4096).contains(&node_count) {
            return Err(Error::InsufficientEvidence(format!(
                "{PLAYERNAMES_TABLE} tree has {node_count} node(s), outside 2..=4096"
            )));
        }
        let mut nodes = Vec::with_capacity(node_count);
        for index in 0..node_count {
            let start = index * 4;
            let left = u16::from_le_bytes([pool[start], pool[start + 1]]);
            let right = u16::from_le_bytes([pool[start + 2], pool[start + 3]]);
            nodes.push((left, right));
        }
        validate_tree(&nodes)?;

        let mut offsets: Vec<u32> = offset_by_id.values().copied().collect();
        offsets.sort_unstable();
        offsets.dedup();
        for offset in offset_by_id.values() {
            if *offset < min_offset || *offset as usize >= pool.len() {
                return Err(Error::InsufficientEvidence(format!(
                    "{PLAYERNAMES_TABLE} offset {offset:#x} outside the pool tree/data bounds"
                )));
            }
        }

        let mut table = Self {
            summary: PlayernamesSummary {
                table: summary_base,
                pool_base,
                tree_bytes: tree_bytes as u32,
                tree_nodes: node_count as u32,
                min_offset,
                max_offset,
                unique_name_ids: offset_by_id.len(),
                decoded_entries: 0,
            },
            nodes,
            pool,
            offset_by_id,
            offsets,
        };
        let decoded = table.validate_entries()?;
        table.summary.decoded_entries = decoded;
        Ok(table)
    }

    fn validate_entries(&self) -> Result<usize> {
        let mut decoded = 0usize;
        for id in self.offset_by_id.keys() {
            if self.decode(*id)?.is_some() {
                decoded += 1;
            }
        }
        Ok(decoded)
    }

    fn decode(&self, id: u16) -> Result<Option<String>> {
        let Some(&offset) = self.offset_by_id.get(&id) else {
            return Ok(None);
        };
        let next = match self.offsets.binary_search(&offset) {
            Ok(index) => self.offsets.get(index + 1).copied(),
            Err(index) => self.offsets.get(index).copied(),
        };
        decode_huffman_string(&self.pool, &self.nodes, offset, next).map(Some)
    }
}

struct DcNameTable {
    summary: TableSummary,
    names: BTreeMap<u16, String>,
}

impl DcNameTable {
    fn load_optional(
        handle: &dyn ProcessHandle,
        metadata: &Metadata,
        live_tables: &[DbTable],
    ) -> Result<Option<Self>> {
        let Some((metadata_table, live)) = find_table(metadata, live_tables, DCPLAYERNAMES_TABLE)
        else {
            return Ok(None);
        };
        let fields = field_map(handle, live)?;
        let nameid = field_spec(metadata_table, &fields, "nameid", live.header_44, None)?;
        let name = field_spec(metadata_table, &fields, "name", live.header_44, None)?;
        if nameid.live_type != 3 || name.live_type != 0 {
            return Err(Error::InsufficientEvidence(format!(
                "`{DCPLAYERNAMES_TABLE}` has unexpected field types {}/{}",
                nameid.live_type, name.live_type
            )));
        }
        let summary = table_summary(handle, metadata_table, live, true)?;
        let rows = read_rows(handle, &summary)?;
        let mut names = BTreeMap::new();
        for index in 0..summary.rows as usize {
            let start = index * summary.record_size as usize;
            let row = &rows[start..start + summary.record_size as usize];
            let id = nameid.read_int(row)?;
            if !(DC_NAME_ID_BASE..=i64::from(u16::MAX)).contains(&id) {
                return Err(Error::InsufficientEvidence(format!(
                    "{DCPLAYERNAMES_TABLE} row {index}: nameid {id} outside 46000..=65535"
                )));
            }
            let value = name.read_inline_string(row)?.unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            if names.insert(id as u16, value).is_some() {
                return Err(Error::InsufficientEvidence(format!(
                    "{DCPLAYERNAMES_TABLE} has duplicate nameid {id}"
                )));
            }
        }
        Ok(Some(Self { summary, names }))
    }
}

#[derive(Clone, Default)]
struct EditedName {
    first: Option<String>,
    last: Option<String>,
    common: Option<String>,
}

struct EditedNameTable {
    summary: EditedSummary,
    by_player: BTreeMap<u32, EditedName>,
}

impl EditedNameTable {
    fn load_optional(
        handle: &dyn ProcessHandle,
        metadata: &Metadata,
        live_tables: &[DbTable],
    ) -> Result<Option<Self>> {
        let Some((metadata_table, live)) =
            find_table(metadata, live_tables, EDITED_PLAYERNAMES_TABLE)
        else {
            return Ok(None);
        };
        let fields = field_map(handle, live)?;
        let playerid = field_spec(metadata_table, &fields, "playerid", live.header_44, None)?;
        let firstname = field_spec(metadata_table, &fields, "firstname", live.header_44, None)?;
        let surname = field_spec(metadata_table, &fields, "surname", live.header_44, None)?;
        let commonname = field_spec(metadata_table, &fields, "commonname", live.header_44, None)?;
        for field in [&playerid, &firstname, &surname, &commonname] {
            if field.live_type != 3 && field.live_type != 0 {
                return Err(Error::InsufficientEvidence(format!(
                    "`{EDITED_PLAYERNAMES_TABLE}.{}` has unsupported live type {}",
                    field.name, field.live_type
                )));
            }
        }
        if playerid.live_type != 3
            || firstname.live_type != 0
            || surname.live_type != 0
            || commonname.live_type != 0
        {
            return Err(Error::InsufficientEvidence(format!(
                "`{EDITED_PLAYERNAMES_TABLE}` field types do not match the verified layout"
            )));
        }
        let summary_base = table_summary(handle, metadata_table, live, true)?;
        let rows = read_rows(handle, &summary_base)?;
        let mut by_player = BTreeMap::new();
        for index in 0..summary_base.rows as usize {
            let start = index * summary_base.record_size as usize;
            let row = &rows[start..start + summary_base.record_size as usize];
            let id = playerid.read_int(row)?;
            if !(1..=i64::from(u32::MAX)).contains(&id) {
                return Err(Error::InsufficientEvidence(format!(
                    "{EDITED_PLAYERNAMES_TABLE} row {index}: playerid {id} is invalid"
                )));
            }
            let value = EditedName {
                first: firstname.read_inline_string(row)?,
                last: surname.read_inline_string(row)?,
                common: commonname.read_inline_string(row)?,
            };
            if by_player.insert(id as u32, value).is_some() {
                return Err(Error::InsufficientEvidence(format!(
                    "{EDITED_PLAYERNAMES_TABLE} has duplicate playerid {id}"
                )));
            }
        }
        Ok(Some(Self {
            summary: EditedSummary {
                table: summary_base,
                overrides: by_player.len(),
            },
            by_player,
        }))
    }
}

#[derive(Clone)]
struct FieldSpec {
    name: String,
    bit_offset: u32,
    bit_width: u32,
    low: i64,
    high: i64,
    live_type: u32,
}

impl FieldSpec {
    fn read_int(&self, row: &[u8]) -> Result<i64> {
        let raw = read_bits(row, self.bit_offset, self.bit_width)? as i64;
        let value = raw
            .checked_add(self.low)
            .ok_or_else(|| Error::parse("name field", format!("{} bias overflow", self.name)))?;
        if value < self.low || value > self.high {
            return Err(Error::Unsupported(format!(
                "name field {} decoded {value} outside [{}, {}]",
                self.name, self.low, self.high
            )));
        }
        Ok(value)
    }

    fn read_inline_string(&self, row: &[u8]) -> Result<Option<String>> {
        if self.bit_offset % 8 != 0 || self.bit_width % 8 != 0 {
            return Err(Error::InsufficientEvidence(format!(
                "inline string {} is not byte-aligned ({}/{})",
                self.name, self.bit_offset, self.bit_width
            )));
        }
        let start = (self.bit_offset / 8) as usize;
        let length = (self.bit_width / 8) as usize;
        let end = start
            .checked_add(length)
            .ok_or_else(|| Error::parse("name field", "inline string size overflow"))?;
        let bytes = row.get(start..end).ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "inline string {} [{start}..{end}] exceeds the {}-byte row",
                self.name,
                row.len()
            ))
        })?;
        let stop = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        let text = std::str::from_utf8(&bytes[..stop]).map_err(|error| {
            Error::InsufficientEvidence(format!(
                "inline string {} is not valid UTF-8: {error}",
                self.name
            ))
        })?;
        validate_name_text(text)?;
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text.to_string()))
        }
    }
}

fn decode_huffman_string(
    pool: &[u8],
    nodes: &[(u16, u16)],
    offset: u32,
    next_offset: Option<u32>,
) -> Result<String> {
    let offset = offset as usize;
    let length = *pool.get(offset).ok_or_else(|| {
        Error::InsufficientEvidence(format!("name offset {offset:#x} is outside the pool"))
    })? as usize;
    if length == 0 {
        return Ok(String::new());
    }
    if length > MAX_NAME_BYTES {
        return Err(Error::InsufficientEvidence(format!(
            "name length {length} exceeds the {MAX_NAME_BYTES}-byte cap"
        )));
    }
    let max_bytes = next_offset
        .and_then(|next| next.checked_sub(offset as u32 + 1))
        .map(|value| (value as usize).min(MAX_COMPRESSED_NAME_BYTES))
        .unwrap_or(MAX_COMPRESSED_NAME_BYTES);
    if max_bytes == 0 {
        return Err(Error::InsufficientEvidence(format!(
            "name at {offset:#x} has no room for a bitstream"
        )));
    }

    if nodes.is_empty() {
        return Err(Error::InsufficientEvidence(
            "Huffman tree is empty".to_string(),
        ));
    }
    let mut node = 0usize;
    let mut output = Vec::with_capacity(length);
    let mut used = 0usize;
    'outer: while output.len() < length {
        if used >= max_bytes {
            return Err(Error::InsufficientEvidence(format!(
                "name at {offset:#x} exceeded its {max_bytes}-byte bitstream bound"
            )));
        }
        let byte = *pool.get(offset + 1 + used).ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "name at {offset:#x} bitstream exceeds the pool snapshot"
            ))
        })?;
        used += 1;
        for bit in (0..8).rev() {
            let (left, right) = nodes.get(node).ok_or_else(|| {
                Error::InsufficientEvidence(format!("Huffman tree node {node} is outside the tree"))
            })?;
            let child = if (byte >> bit) & 1 == 0 {
                *left
            } else {
                *right
            };
            if child & 0xFF00 != 0 {
                let character = (child >> 8) as u8;
                if character == 0 {
                    return Err(Error::InsufficientEvidence(
                        "Huffman leaf encodes a NUL byte".to_string(),
                    ));
                }
                output.push(character);
                node = 0;
                if output.len() == length {
                    break 'outer;
                }
            } else {
                let next = child as usize;
                if next == 0 || next >= nodes.len() {
                    return Err(Error::InsufficientEvidence(format!(
                        "Huffman tree references invalid node {next}"
                    )));
                }
                node = next;
            }
        }
    }

    let text = String::from_utf8(output).map_err(|error| {
        Error::InsufficientEvidence(format!("name at {offset:#x} is not valid UTF-8: {error}"))
    })?;
    validate_name_text(&text)?;
    Ok(text)
}

fn validate_name_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    if text
        .chars()
        .any(|character| character == '\0' || character.is_control())
    {
        return Err(Error::InsufficientEvidence(
            "decoded name contains a control character".to_string(),
        ));
    }
    Ok(())
}

fn validate_tree(nodes: &[(u16, u16)]) -> Result<()> {
    let mut state = vec![0u8; nodes.len()];
    state[0] = 1;
    let mut stack = vec![(0usize, 0u8)];
    let mut leaves = 0usize;
    while let Some((node, child_index)) = stack.pop() {
        if child_index == 2 {
            state[node] = 2;
            continue;
        }
        stack.push((node, child_index + 1));
        let child = if child_index == 0 {
            nodes[node].0
        } else {
            nodes[node].1
        };
        if child & 0xFF00 != 0 {
            if child >> 8 == 0 {
                return Err(Error::InsufficientEvidence(
                    "Huffman tree contains a NUL leaf".to_string(),
                ));
            }
            leaves += 1;
            continue;
        }
        let next = child as usize;
        if next == 0 || next >= nodes.len() {
            return Err(Error::InsufficientEvidence(format!(
                "Huffman tree references invalid node {next}"
            )));
        }
        match state[next] {
            0 => {
                state[next] = 1;
                stack.push((next, 0));
            }
            1 => {
                return Err(Error::InsufficientEvidence(
                    "Huffman tree contains a cycle".to_string(),
                ));
            }
            2 => {
                return Err(Error::InsufficientEvidence(
                    "Huffman tree reuses a node (not a tree)".to_string(),
                ));
            }
            _ => unreachable!(),
        }
    }
    if leaves == 0 {
        return Err(Error::InsufficientEvidence(
            "Huffman tree has no leaves".to_string(),
        ));
    }
    Ok(())
}

fn find_table<'a>(
    metadata: &'a Metadata,
    live_tables: &'a [DbTable],
    name: &str,
) -> Option<(&'a MetadataTable, &'a DbTable)> {
    let metadata_table = metadata.table_by_name(name)?;
    let live = live_tables
        .iter()
        .find(|table| table.shortname == metadata_table.shortname)?;
    Some((metadata_table, live))
}

fn require_table<'a>(
    metadata: &'a Metadata,
    live_tables: &'a [DbTable],
    name: &str,
) -> Result<(&'a MetadataTable, &'a DbTable)> {
    find_table(metadata, live_tables, name).ok_or_else(|| {
        Error::InsufficientEvidence(format!(
            "metadata/live table `{name}` is not available; name resolution is gated"
        ))
    })
}

fn field_map(handle: &dyn ProcessHandle, live: &DbTable) -> Result<BTreeMap<String, DbField>> {
    let fields = read_table_fields(handle, live)?;
    let mut map = BTreeMap::new();
    for field in fields {
        if map.insert(field.shortname.clone(), field).is_some() {
            return Err(Error::InsufficientEvidence(format!(
                "table {} has duplicate field 4CCs",
                live.shortname
            )));
        }
    }
    Ok(map)
}

fn field_spec(
    metadata: &MetadataTable,
    live_fields: &BTreeMap<String, DbField>,
    name: &str,
    row_size: u32,
    fit_bits: Option<u32>,
) -> Result<FieldSpec> {
    let metadata_field = metadata
        .fields
        .iter()
        .find(|field| field.name == name)
        .ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "metadata table `{}` has no `{name}` field",
                metadata.name
            ))
        })?;
    let live_field = live_fields.get(&metadata_field.shortname).ok_or_else(|| {
        Error::InsufficientEvidence(format!(
            "live table `{}` has no field {} for `{name}`",
            metadata.name, metadata_field.shortname
        ))
    })?;
    if live_field.bit_width != u32::from(metadata_field.depth) {
        return Err(Error::InsufficientEvidence(format!(
            "{}.{name} width mismatch: metadata {} vs live {}",
            metadata.name, metadata_field.depth, live_field.bit_width
        )));
    }
    let high = metadata_field.range_high.ok_or_else(|| {
        Error::InsufficientEvidence(format!("{}.{name} has no rangehigh", metadata.name))
    })?;
    let low = metadata_field.range_low.unwrap_or(0);
    if low > high {
        return Err(Error::InsufficientEvidence(format!(
            "{}.{name} has inverted range {low}..{high}",
            metadata.name
        )));
    }
    let required_bits = fit_bits.unwrap_or(live_field.bit_width);
    if live_field.bit_offset as u64 + u64::from(required_bits) > u64::from(row_size) * 8 {
        return Err(Error::InsufficientEvidence(format!(
            "{}.{name} needs {required_bits} bit(s) at offset {}, beyond the {row_size}-byte row",
            metadata.name, live_field.bit_offset
        )));
    }
    Ok(FieldSpec {
        name: name.to_string(),
        bit_offset: live_field.bit_offset,
        bit_width: live_field.bit_width,
        low,
        high,
        live_type: live_field.field_type,
    })
}

fn table_summary(
    handle: &dyn ProcessHandle,
    metadata: &MetadataTable,
    live: &DbTable,
    allow_empty: bool,
) -> Result<TableSummary> {
    let rows = u32::from(handle.read_u16(offset(live.address, 0x7C)?)?);
    let capacity = u32::from(handle.read_u16(offset(live.address, 0x78)?)?);
    if !allow_empty && rows == 0 {
        return Err(Error::InsufficientEvidence(format!(
            "`{}` has zero rows; name resolution is gated",
            metadata.name
        )));
    }
    if rows > capacity {
        return Err(Error::InsufficientEvidence(format!(
            "`{}` row count {rows} exceeds capacity {capacity}",
            metadata.name
        )));
    }
    if live.header_44 == 0 || live.header_44 > 4096 {
        return Err(Error::InsufficientEvidence(format!(
            "`{}` record size {} is outside 1..=4096",
            metadata.name, live.header_44
        )));
    }
    if live.data_pointer == 0 {
        return Err(Error::InsufficientEvidence(format!(
            "`{}` data pointer is null",
            metadata.name
        )));
    }
    let bytes = u64::from(rows)
        .checked_mul(u64::from(live.header_44))
        .ok_or_else(|| Error::parse("name table", "snapshot size overflow"))?;
    if bytes > MAX_NAME_TABLE_BYTES {
        return Err(Error::InsufficientEvidence(format!(
            "`{}` snapshot would read {bytes} bytes, above the {MAX_NAME_TABLE_BYTES} cap",
            metadata.name
        )));
    }
    Ok(TableSummary {
        table: metadata.name.clone(),
        shortname: metadata.shortname.clone(),
        descriptor: live.address,
        data_pointer: live.data_pointer,
        record_size: live.header_44,
        rows,
        capacity,
    })
}

fn read_rows(handle: &dyn ProcessHandle, summary: &TableSummary) -> Result<Vec<u8>> {
    let length = (summary.record_size as usize)
        .checked_mul(summary.rows as usize)
        .ok_or_else(|| Error::parse("name table", "row snapshot size overflow"))?;
    read_range(handle, summary.data_pointer, length)
}

fn offset(base: u64, delta: u64) -> Result<u64> {
    base.checked_add(delta)
        .ok_or_else(|| Error::parse("name table", "address overflow"))
}

fn check(name: &'static str, passed: bool, detail: String) -> NameCheck {
    NameCheck {
        name,
        passed,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(character: char) -> u16 {
        (character as u16) << 8
    }

    /// Build a tiny tree:
    /// root 0 -> left 1, right 2
    /// node 1 -> 'A', 'B'
    /// node 2 -> ' ', 'C'
    fn test_tree() -> Vec<(u16, u16)> {
        vec![(1, 2), (leaf('A'), leaf('B')), (leaf(' '), leaf('C'))]
    }

    fn encode_bits(value: &[u8]) -> Vec<u8> {
        // The test tree codes: A=00, B=01, space=10, C=11.
        let mut bits = Vec::new();
        for byte in value {
            let code = match *byte {
                b'A' => 0b00,
                b'B' => 0b01,
                b' ' => 0b10,
                b'C' => 0b11,
                _ => panic!("unexpected test character"),
            };
            bits.push((code >> 1) & 1);
            bits.push(code & 1);
        }
        while bits.len() % 8 != 0 {
            bits.push(0);
        }
        bits.chunks(8)
            .map(|chunk| chunk.iter().fold(0u8, |value, bit| (value << 1) | *bit))
            .collect()
    }

    fn encode_name(text: &str) -> Vec<u8> {
        let mut output = vec![text.len() as u8];
        output.extend(encode_bits(text.as_bytes()));
        output
    }

    #[test]
    fn decodes_shared_tree_names() {
        let tree = test_tree();
        let mut pool = vec![0u8; 0];
        for (left, right) in &tree {
            pool.extend_from_slice(&left.to_le_bytes());
            pool.extend_from_slice(&right.to_le_bytes());
        }
        // The pool's first offset is the tree size (12 bytes).
        let first = encode_name("AB");
        let second = encode_name("C A");
        let first_offset = pool.len() as u32;
        pool.extend_from_slice(&first);
        let second_offset = pool.len() as u32;
        pool.extend_from_slice(&second);

        assert_eq!(
            decode_huffman_string(&pool, &tree, first_offset, Some(second_offset)).unwrap(),
            "AB"
        );
        assert_eq!(
            decode_huffman_string(&pool, &tree, second_offset, None).unwrap(),
            "C A"
        );
    }

    #[test]
    fn rejects_truncated_bitstream() {
        let tree = test_tree();
        let mut pool = vec![0u8; 12];
        for (index, (left, right)) in tree.iter().enumerate() {
            pool[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            pool[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        // length 5 but only one byte of bits
        pool.push(5);
        pool.push(0b0001_0111);
        let error = decode_huffman_string(&pool, &tree, 12, None).unwrap_err();
        assert!(error.is_insufficient_evidence());
    }

    #[test]
    fn rejects_invalid_utf8() {
        // A leaf byte 0x80 is not a valid standalone UTF-8 sequence.
        let tree: Vec<(u16, u16)> = vec![(1, 1), (0x8000, 0x8000)];
        let mut pool = vec![0u8; 8];
        for (index, (left, right)) in tree.iter().enumerate() {
            pool[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            pool[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        pool.push(1);
        pool.push(0);
        let error = decode_huffman_string(&pool, &tree, 8, None).unwrap_err();
        assert!(error.is_insufficient_evidence());
    }

    #[test]
    fn rejects_tree_cycle_and_bad_child() {
        let cycle = vec![(1, 1), (0, leaf('A'))];
        assert!(validate_tree(&cycle).is_err());
        let bad = vec![(3, leaf('A'))];
        assert!(validate_tree(&bad).is_err());
    }

    #[test]
    fn rejects_zero_length_and_control_names() {
        // Reuse the shared tree test helpers to construct invalid strings.
        let tree = test_tree();
        let mut pool = vec![0u8; 12];
        for (index, (left, right)) in tree.iter().enumerate() {
            pool[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            pool[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        pool.push(0);
        assert_eq!(decode_huffman_string(&pool, &tree, 12, None).unwrap(), "");
    }

    #[test]
    fn resolves_edited_override_and_missing_ids() {
        // Build a resolver-shaped value without a live process by using the
        // pure pieces directly: the edited override is represented by the
        // final-field precedence in `resolve_player`, so test the helper logic
        // through a minimal table.
        let tree = test_tree();
        let mut pool = vec![0u8; 12];
        for (index, (left, right)) in tree.iter().enumerate() {
            pool[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            pool[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        let alpha = encode_name("AB");
        let alpha_offset = pool.len() as u32;
        pool.extend_from_slice(&alpha);

        let mut offset_by_id = BTreeMap::new();
        offset_by_id.insert(1u16, alpha_offset);
        let table = HuffmanNameTable {
            summary: PlayernamesSummary {
                table: TableSummary {
                    table: "playernames".to_string(),
                    shortname: "TEST".to_string(),
                    descriptor: 0,
                    data_pointer: 0,
                    record_size: 0,
                    rows: 1,
                    capacity: 1,
                },
                pool_base: 0,
                tree_bytes: 12,
                tree_nodes: 3,
                min_offset: 12,
                max_offset: alpha_offset,
                unique_name_ids: 1,
                decoded_entries: 1,
            },
            nodes: tree,
            pool,
            offset_by_id,
            offsets: vec![alpha_offset],
        };
        let resolver = PlayerNameResolver {
            playernames: table,
            dcplayernames: None,
            editedplayernames: None,
            checks: Vec::new(),
            notes: Vec::new(),
        };
        let resolved = resolver.resolve_player(7, 1, 0, 0);
        assert_eq!(resolved.first_name.as_deref(), Some("AB"));
        assert_eq!(resolved.display_name.as_deref(), Some("AB"));
        assert_eq!(resolved.error, None);

        let missing = resolver.resolve_player(8, 2, 0, 0);
        assert_eq!(missing.first_name, None);
        assert!(missing.error.is_some());
    }
    fn tiny_name_table() -> HuffmanNameTable {
        let tree = test_tree();
        let mut pool = vec![0u8; 12];
        for (index, (left, right)) in tree.iter().enumerate() {
            pool[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            pool[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        let alpha = encode_name("AB");
        let alpha_offset = pool.len() as u32;
        pool.extend_from_slice(&alpha);
        let mut offset_by_id = BTreeMap::new();
        offset_by_id.insert(1u16, alpha_offset);
        HuffmanNameTable {
            summary: PlayernamesSummary {
                table: dummy_summary("playernames"),
                pool_base: 0,
                tree_bytes: 12,
                tree_nodes: 3,
                min_offset: 12,
                max_offset: alpha_offset,
                unique_name_ids: 1,
                decoded_entries: 1,
            },
            nodes: tree,
            pool,
            offset_by_id,
            offsets: vec![alpha_offset],
        }
    }

    fn dummy_summary(table: &str) -> TableSummary {
        TableSummary {
            table: table.to_string(),
            shortname: "TEST".to_string(),
            descriptor: 0,
            data_pointer: 0,
            record_size: 0,
            rows: 0,
            capacity: 0,
        }
    }

    #[test]
    fn edited_override_wins_for_same_player() {
        let mut by_player = BTreeMap::new();
        by_player.insert(
            7u32,
            EditedName {
                first: Some("C".to_string()),
                last: Some("A".to_string()),
                common: None,
            },
        );
        let resolver = PlayerNameResolver {
            playernames: tiny_name_table(),
            dcplayernames: None,
            editedplayernames: Some(EditedNameTable {
                summary: EditedSummary {
                    table: dummy_summary("editedplayernames"),
                    overrides: 1,
                },
                by_player,
            }),
            checks: Vec::new(),
            notes: Vec::new(),
        };
        let resolved = resolver.resolve_player(7, 1, 0, 0);
        assert_eq!(resolved.display_name.as_deref(), Some("C A"));
        assert_eq!(resolved.source, "editedplayernames");
        assert_eq!(resolved.error, None);
    }

    #[test]
    fn dc_table_resolves_high_name_ids() {
        let mut names = BTreeMap::new();
        names.insert(46000u16, "Lutin".to_string());
        let resolver = PlayerNameResolver {
            playernames: tiny_name_table(),
            dcplayernames: Some(DcNameTable {
                summary: dummy_summary("dcplayernames"),
                names,
            }),
            editedplayernames: None,
            checks: Vec::new(),
            notes: Vec::new(),
        };
        let resolved = resolver.resolve_player(8, 46000, 0, 0);
        assert_eq!(resolved.display_name.as_deref(), Some("Lutin"));
        assert_eq!(resolved.source, "dcplayernames");
        assert_eq!(resolved.error, None);
    }

    #[test]
    fn rejects_control_character_leaf() {
        let tree = vec![(leaf('\u{0001}'), leaf('A'))];
        let mut pool = vec![0u8; 4];
        for (index, (left, right)) in tree.iter().enumerate() {
            pool[index * 4..index * 4 + 2].copy_from_slice(&left.to_le_bytes());
            pool[index * 4 + 2..index * 4 + 4].copy_from_slice(&right.to_le_bytes());
        }
        pool.push(1);
        pool.push(0);
        let error = decode_huffman_string(&pool, &tree, 4, None).unwrap_err();
        assert!(error.is_insufficient_evidence());
    }
}
