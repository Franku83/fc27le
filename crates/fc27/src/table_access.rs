//! Generic, read-only table snapshots for the Lua compatibility layer.
//!
//! This is the same verified storage model used by the player reader:
//! `+0x30` first record, `+0x44` record size, `+0x7C` slot count, `+0x78`
//! capacity, `+0x7E` invalid-marker count, and the high bit of each record's
//! last byte as the invalid predicate. Field entries come from the live
//! descriptor and are matched to metadata names by 4CC.
//!
//! The snapshot is immutable. A record handle is a logical `(table_id,
//! generation, slot)` token, never a raw process pointer.

use std::collections::BTreeMap;

use fc27_core::{Error, Result};
use fc27_process::ProcessHandle;
use serde::Serialize;

use crate::db_metadata::{Metadata, MetadataField};
use crate::db_service::{DbField, DbTable, read_table_fields};
use crate::player_store::{read_bits, read_range};

/// Safety cap for one generic table snapshot (64 MiB).
pub const MAX_TABLE_BYTES: u64 = 64 * 1024 * 1024;
/// Player record invalid/deleted marker.
pub const INVALID_MARKER: u8 = 0x80;

#[derive(Debug, Clone, Serialize)]
pub struct FieldInfo {
    pub name: String,
    pub shortname: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub low: i64,
    pub high: i64,
    pub field_type: u32,
    pub type_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableSnapshotInfo {
    pub table_id: u16,
    pub table_name: String,
    pub shortname: String,
    pub descriptor: u64,
    pub data_pointer: u64,
    pub record_size: u32,
    pub slot_count: u32,
    pub capacity: u32,
    pub invalid_marker_count: u32,
    pub field_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FieldValue {
    Integer(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone)]
pub struct TableSnapshot {
    info: TableSnapshotInfo,
    data: Vec<u8>,
    fields: BTreeMap<String, FieldInfo>,
}

impl TableSnapshot {
    /// Load a generic snapshot from the live process using verified metadata.
    pub fn load(
        handle: &dyn ProcessHandle,
        metadata: &Metadata,
        live_tables: &[DbTable],
        table_name: &str,
        table_id: u16,
    ) -> Result<Self> {
        let metadata_table = metadata.table_by_name(table_name).ok_or_else(|| {
            Error::InsufficientEvidence(format!("metadata has no table `{table_name}`"))
        })?;
        let live = live_tables
            .iter()
            .find(|table| table.shortname == metadata_table.shortname)
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!(
                    "live table manager has no descriptor for `{table_name}` ({})",
                    metadata_table.shortname
                ))
            })?;
        if live.field_count_byte as usize != metadata_table.fields.len() {
            return Err(Error::InsufficientEvidence(format!(
                "`{table_name}` field count mismatch: metadata {} vs live {}",
                metadata_table.fields.len(),
                live.field_count_byte
            )));
        }
        let live_fields = read_table_fields(handle, live)?;
        let mut live_by_short: BTreeMap<&str, &DbField> = BTreeMap::new();
        for field in &live_fields {
            if live_by_short
                .insert(field.shortname.as_str(), field)
                .is_some()
            {
                return Err(Error::InsufficientEvidence(format!(
                    "`{table_name}` has duplicate live field 4CCs"
                )));
            }
        }
        let mut fields = BTreeMap::new();
        for metadata_field in &metadata_table.fields {
            let live_field = live_by_short
                .get(metadata_field.shortname.as_str())
                .ok_or_else(|| {
                    Error::InsufficientEvidence(format!(
                        "`{table_name}.{}` ({}) has no live field entry",
                        metadata_field.name, metadata_field.shortname
                    ))
                })?;
            let info = field_info(metadata_field, live_field, live.header_44)?;
            if fields.insert(info.name.clone(), info).is_some() {
                return Err(Error::InsufficientEvidence(format!(
                    "`{table_name}` has duplicate metadata field names"
                )));
            }
        }

        let slot_count = u32::from(handle.read_u16(offset(live.address, 0x7C)?)?);
        let capacity = u32::from(handle.read_u16(offset(live.address, 0x78)?)?);
        let invalid_marker_count = u32::from(handle.read_u16(offset(live.address, 0x7E)?)?);
        if slot_count > capacity {
            return Err(Error::InsufficientEvidence(format!(
                "`{table_name}` slot count {slot_count} exceeds capacity {capacity}"
            )));
        }
        if live.header_44 == 0 || live.header_44 > 4096 {
            return Err(Error::InsufficientEvidence(format!(
                "`{table_name}` record size {} is outside 1..=4096",
                live.header_44
            )));
        }
        if live.data_pointer == 0 && slot_count != 0 {
            return Err(Error::InsufficientEvidence(format!(
                "`{table_name}` data pointer is null"
            )));
        }
        let bytes = u64::from(slot_count)
            .checked_mul(u64::from(live.header_44))
            .ok_or_else(|| Error::parse("table snapshot", "size overflow"))?;
        if bytes > MAX_TABLE_BYTES {
            return Err(Error::InsufficientEvidence(format!(
                "`{table_name}` snapshot would read {bytes} bytes, above the {MAX_TABLE_BYTES} cap"
            )));
        }
        let data = if bytes == 0 {
            Vec::new()
        } else {
            read_range(handle, live.data_pointer, bytes as usize)?
        };

        let info = TableSnapshotInfo {
            table_id,
            table_name: metadata_table.name.clone(),
            shortname: metadata_table.shortname.clone(),
            descriptor: live.address,
            data_pointer: live.data_pointer,
            record_size: live.header_44,
            slot_count,
            capacity,
            invalid_marker_count,
            field_count: fields.len() as u32,
        };
        Self::from_parts(info, fields.into_values().collect(), data)
    }

    /// Build a snapshot from already-read parts. Used by tests and fixtures.
    pub fn from_parts(
        info: TableSnapshotInfo,
        fields: Vec<FieldInfo>,
        data: Vec<u8>,
    ) -> Result<Self> {
        let expected = (info.record_size as usize)
            .checked_mul(info.slot_count as usize)
            .ok_or_else(|| Error::parse("table snapshot", "size overflow"))?;
        if data.len() != expected {
            return Err(Error::ShortRead {
                address: info.data_pointer,
                requested: expected,
                received: data.len(),
            });
        }
        if info.slot_count > info.capacity {
            return Err(Error::InsufficientEvidence(format!(
                "`{}` slot count {} exceeds capacity {}",
                info.table_name, info.slot_count, info.capacity
            )));
        }
        if info.record_size == 0 || info.record_size > 4096 {
            return Err(Error::InsufficientEvidence(format!(
                "`{}` record size {} is outside 1..=4096",
                info.table_name, info.record_size
            )));
        }
        let mut field_map = BTreeMap::new();
        for field in fields {
            if field.bit_width == 0 || field.bit_width > 64 {
                return Err(Error::InsufficientEvidence(format!(
                    "`{}.{}` width {} is outside 1..=64",
                    info.table_name, field.name, field.bit_width
                )));
            }
            let required = if field.field_type == 13 {
                32.min(field.bit_width)
            } else {
                field.bit_width
            };
            if field.bit_offset as u64 + u64::from(required) > u64::from(info.record_size) * 8 {
                return Err(Error::InsufficientEvidence(format!(
                    "`{}.{}` ends beyond the {}-byte record",
                    info.table_name, field.name, info.record_size
                )));
            }
            if field.low > field.high {
                return Err(Error::InsufficientEvidence(format!(
                    "`{}.{}` has inverted range {}/{}",
                    info.table_name, field.name, field.low, field.high
                )));
            }
            if field_map.insert(field.name.clone(), field).is_some() {
                return Err(Error::InsufficientEvidence(format!(
                    "`{}` has duplicate field `{}`",
                    info.table_name,
                    field_map
                        .keys()
                        .last()
                        .map(String::as_str)
                        .unwrap_or("<unknown>")
                )));
            }
        }
        Ok(Self {
            info,
            data,
            fields: field_map,
        })
    }

    pub fn info(&self) -> &TableSnapshotInfo {
        &self.info
    }

    pub fn fields(&self) -> impl Iterator<Item = &FieldInfo> {
        self.fields.values()
    }

    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.get(name)
    }

    pub fn is_valid(&self, index: u32) -> Result<bool> {
        let row = self.row(index)?;
        Ok(row[self.info.record_size as usize - 1] & INVALID_MARKER == 0)
    }

    pub fn read_field(&self, index: u32, field_name: &str) -> Result<FieldValue> {
        let field = self.fields.get(field_name).ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "table `{}` has no field `{field_name}`",
                self.info.table_name
            ))
        })?;
        let row = self.row(index)?;
        let raw = read_bits(row, field.bit_offset, field.bit_width)?;
        match field.field_type {
            3 => {
                let value = raw as i64;
                let value = value.checked_add(field.low).ok_or_else(|| {
                    Error::parse("table field", format!("{} bias overflow", field.name))
                })?;
                if value < field.low || value > field.high {
                    return Err(Error::Unsupported(format!(
                        "`{}.{}` decoded {value} outside [{}, {}]",
                        self.info.table_name, field.name, field.low, field.high
                    )));
                }
                Ok(FieldValue::Integer(value))
            }
            4 => {
                if field.bit_width > 32 {
                    return Err(Error::Unsupported(format!(
                        "`{}.{}` float width {} is unsupported",
                        self.info.table_name, field.name, field.bit_width
                    )));
                }
                Ok(FieldValue::Float(f32::from_bits(raw as u32) as f64))
            }
            0 => Ok(FieldValue::String(read_inline_string(row, field)?)),
            13 => Err(Error::Unsupported(format!(
                "`{}.{}` is a compressed string; use the playernames name resolver",
                self.info.table_name, field.name
            ))),
            other => Err(Error::Unsupported(format!(
                "`{}.{}` has unsupported live type {other}",
                self.info.table_name, field.name
            ))),
        }
    }

    fn row(&self, index: u32) -> Result<&[u8]> {
        if index >= self.info.slot_count {
            return Err(Error::InvalidInput(format!(
                "record index {index} is outside table `{}` ({} slots)",
                self.info.table_name, self.info.slot_count
            )));
        }
        let size = self.info.record_size as usize;
        let start = index as usize * size;
        self.data.get(start..start + size).ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "record {index} is outside the `{}` snapshot",
                self.info.table_name
            ))
        })
    }
}

fn field_info(metadata: &MetadataField, live: &DbField, record_size: u32) -> Result<FieldInfo> {
    if live.bit_width != u32::from(metadata.depth) {
        return Err(Error::InsufficientEvidence(format!(
            "field {} width mismatch: metadata {} vs live {}",
            metadata.name, metadata.depth, live.bit_width
        )));
    }
    let high = metadata.range_high.ok_or_else(|| {
        Error::InsufficientEvidence(format!("field {} has no rangehigh", metadata.name))
    })?;
    let low = metadata.range_low.unwrap_or(0);
    if low > high {
        return Err(Error::InsufficientEvidence(format!(
            "field {} has inverted range {low}..{high}",
            metadata.name
        )));
    }
    let required = if live.field_type == 13 {
        32.min(live.bit_width)
    } else {
        live.bit_width
    };
    if live.bit_offset as u64 + u64::from(required) > u64::from(record_size) * 8 {
        return Err(Error::InsufficientEvidence(format!(
            "field {} needs {required} bit(s) at offset {}, beyond the {record_size}-byte record",
            metadata.name, live.bit_offset
        )));
    }
    Ok(FieldInfo {
        name: metadata.name.clone(),
        shortname: metadata.shortname.clone(),
        bit_offset: live.bit_offset,
        bit_width: live.bit_width,
        low,
        high,
        field_type: live.field_type,
        type_name: metadata.type_name.clone(),
    })
}

fn read_inline_string(row: &[u8], field: &FieldInfo) -> Result<String> {
    if field.bit_offset % 8 != 0 || field.bit_width % 8 != 0 {
        return Err(Error::Unsupported(format!(
            "inline string `{}` is not byte-aligned ({}/{})",
            field.name, field.bit_offset, field.bit_width
        )));
    }
    let start = (field.bit_offset / 8) as usize;
    let length = (field.bit_width / 8) as usize;
    let end = start
        .checked_add(length)
        .ok_or_else(|| Error::parse("table field", "string size overflow"))?;
    let bytes = row.get(start..end).ok_or_else(|| {
        Error::InsufficientEvidence(format!(
            "inline string `{}` exceeds the {}-byte row",
            field.name,
            row.len()
        ))
    })?;
    let stop = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let text = std::str::from_utf8(&bytes[..stop]).map_err(|error| {
        Error::InsufficientEvidence(format!(
            "inline string `{}` is not valid UTF-8: {error}",
            field.name
        ))
    })?;
    Ok(text.to_string())
}

fn offset(base: u64, delta: u64) -> Result<u64> {
    base.checked_add(delta)
        .ok_or_else(|| Error::parse("table snapshot", "address overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn field(
        name: &str,
        shortname: &str,
        bit_offset: u32,
        bit_width: u32,
        low: i64,
        high: i64,
        field_type: u32,
        type_name: &str,
    ) -> FieldInfo {
        FieldInfo {
            name: name.to_string(),
            shortname: shortname.to_string(),
            bit_offset,
            bit_width,
            low,
            high,
            field_type,
            type_name: type_name.to_string(),
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

    fn write_string(row: &mut [u8], bit_offset: u32, value: &str) {
        let start = (bit_offset / 8) as usize;
        row[start..start + value.len()].copy_from_slice(value.as_bytes());
    }

    fn fixture() -> TableSnapshot {
        let fields = vec![
            field(
                "playerid",
                "ykFq",
                0,
                16,
                0,
                65535,
                3,
                "DBOFIELDTYPE_INTEGER",
            ),
            field(
                "overallrating",
                "UERs",
                16,
                7,
                1,
                99,
                3,
                "DBOFIELDTYPE_INTEGER",
            ),
            field("height", "ypBQ", 23, 7, 130, 220, 3, "DBOFIELDTYPE_INTEGER"),
            field("teamname", "tnm1", 32, 40, 0, 0, 0, "DBOFIELDTYPE_STRING"),
            field("ratingf", "ratf", 72, 32, 0, 0, 4, "DBOFIELDTYPE_FLOAT"),
        ];
        let mut data = vec![0u8; 48];
        for (index, (id, ovr, height, name, rating, invalid)) in [
            (20801u64, 84u64, 187u64, "Alpha", 1.5f32, false),
            (158023, 88, 169, "Beta", 2.25, true),
            (231747, 91, 182, "Gamma", -0.5, false),
        ]
        .into_iter()
        .enumerate()
        {
            let row = &mut data[index * 16..(index + 1) * 16];
            write_bits(row, 0, 16, id);
            write_bits(row, 16, 7, ovr - 1);
            write_bits(row, 23, 7, height - 130);
            write_string(row, 32, name);
            write_bits(row, 72, 32, u64::from(rating.to_bits()));
            if invalid {
                row[15] |= INVALID_MARKER;
            }
        }
        TableSnapshot::from_parts(
            TableSnapshotInfo {
                table_id: 1,
                table_name: "players".to_string(),
                shortname: "TEST".to_string(),
                descriptor: 0,
                data_pointer: 0,
                record_size: 16,
                slot_count: 3,
                capacity: 3,
                invalid_marker_count: 1,
                field_count: 5,
            },
            fields,
            data,
        )
        .unwrap()
    }

    #[test]
    fn reads_integer_string_and_float_fields() {
        let table = fixture();
        assert!(table.is_valid(0).unwrap());
        assert!(!table.is_valid(1).unwrap());
        assert_eq!(
            table.read_field(0, "playerid").unwrap(),
            FieldValue::Integer(20801)
        );
        assert_eq!(
            table.read_field(0, "overallrating").unwrap(),
            FieldValue::Integer(84)
        );
        assert_eq!(
            table.read_field(0, "height").unwrap(),
            FieldValue::Integer(187)
        );
        assert_eq!(
            table.read_field(0, "teamname").unwrap(),
            FieldValue::String("Alpha".to_string())
        );
        assert_eq!(
            table.read_field(0, "ratingf").unwrap(),
            FieldValue::Float(1.5)
        );
        assert_eq!(
            table.read_field(1, "teamname").unwrap(),
            FieldValue::String("Beta".to_string())
        );
    }

    #[test]
    fn rejects_unknown_field_and_out_of_range_index() {
        let table = fixture();
        assert!(table.read_field(0, "missing").is_err());
        assert!(table.read_field(3, "playerid").is_err());
    }

    #[test]
    fn validates_snapshot_shape() {
        let table = fixture();
        let mut info = table.info().clone();
        info.capacity = 2;
        let fields = table.fields().cloned().collect();
        let error = TableSnapshot::from_parts(info, fields, table.data.clone()).unwrap_err();
        assert!(error.is_insufficient_evidence());
    }
}
