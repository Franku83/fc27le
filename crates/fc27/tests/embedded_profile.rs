use std::collections::BTreeMap;

use fc27_core::{Error, Result};
use fc27_game::db_metadata::validate;
use fc27_game::db_service::DbTable;
use fc27_game::embedded_profile::{PROFILE_BUILD_SHA256, metadata};
use fc27_game::player_store::{resolve_layout, scan_players};
use fc27_process::{MemoryMapping, ModuleInfo, Pid, ProcessHandle, ProcessInfo, ProcessRole};

struct MockProcess {
    info: ProcessInfo,
    memory: BTreeMap<u64, u8>,
}

impl MockProcess {
    fn new() -> Self {
        let info = ProcessInfo {
            pid: Pid(1),
            ppid: None,
            name: "mock".to_string(),
            exe_path: None,
            cmdline: Vec::new(),
            cwd: None,
            uid: None,
            state: None,
            threads: None,
            rss_bytes: None,
            role: ProcessRole::NativeApplication,
            wine: None,
        };
        Self {
            info,
            memory: BTreeMap::new(),
        }
    }

    fn write(&mut self, address: u64, bytes: &[u8]) {
        for (offset, byte) in bytes.iter().enumerate() {
            self.memory.insert(address + offset as u64, *byte);
        }
    }

    fn write_u16(&mut self, address: u64, value: u16) {
        self.write(address, &value.to_le_bytes());
    }

    fn write_u64(&mut self, address: u64, value: u64) {
        self.write(address, &value.to_le_bytes());
    }
}

impl ProcessHandle for MockProcess {
    fn pid(&self) -> Pid {
        self.info.pid
    }

    fn info(&self) -> &ProcessInfo {
        &self.info
    }

    fn mappings(&self) -> Result<Vec<MemoryMapping>> {
        Ok(Vec::new())
    }

    fn modules(&self) -> Result<Vec<ModuleInfo>> {
        Ok(Vec::new())
    }

    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(length);
        for offset in 0..length {
            let Some(byte) = self.memory.get(&(address + offset as u64)) else {
                return Err(Error::UnmappedRange {
                    pid: self.info.pid.0,
                    start: address,
                    end: address + length as u64,
                    len: length,
                });
            };
            bytes.push(*byte);
        }
        Ok(bytes)
    }
}

fn field_code(shortname: &str) -> u32 {
    let bytes = shortname.as_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn write_field(
    mock: &mut MockProcess,
    base: u64,
    index: usize,
    shortname: &str,
    offset: u32,
    width: u32,
) {
    let entry = base + 0x84 + index as u64 * 0x10;
    let first = (u64::from(offset) << 32) | 3;
    let second = (u64::from(width) << 32) | u64::from(field_code(shortname));
    mock.write_u64(entry, first);
    mock.write_u64(entry + 8, second);
}

fn set_bits(row: &mut [u8], bit_offset: u32, bit_width: u32, value: u64) {
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

#[test]
fn embedded_profile_decodes_a_verified_shape_snapshot() {
    assert_eq!(PROFILE_BUILD_SHA256.len(), 64);
    let metadata = metadata();
    let mut mock = MockProcess::new();
    let table_base = 0x1000;
    let data_base = 0x2000;
    mock.write_u64(table_base + 0x18, 0);
    mock.write_u16(table_base + 0x78, 4);
    mock.write_u16(table_base + 0x7C, 2);
    mock.write_u16(table_base + 0x7E, 1);

    for (index, (name, shortname, offset, width)) in [
        ("playerid", "ykFq", 1040, 19),
        ("firstnameid", "tHlO", 0, 16),
        ("lastnameid", "QCfa", 16, 16),
        ("commonnameid", "HDYx", 48, 16),
        ("overallrating", "UERs", 1106, 7),
        ("potential", "mpuH", 370, 7),
        ("height", "ypBQ", 514, 7),
        ("weight", "sNZW", 655, 7),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            metadata
                .table_by_name("players")
                .unwrap()
                .fields
                .iter()
                .any(|f| f.name == name)
        );
        write_field(&mut mock, table_base, index, shortname, offset, width);
    }

    let mut rows = vec![0u8; 156 * 2];
    set_bits(&mut rows[0..156], 1040, 19, 2); // playerid = 1 after bias -1
    set_bits(&mut rows[0..156], 1106, 7, 0); // overall = 1
    set_bits(&mut rows[0..156], 370, 7, 0); // potential = 1
    set_bits(&mut rows[0..156], 514, 7, 0); // height = 130
    set_bits(&mut rows[0..156], 655, 7, 0); // weight = 30
    rows[156 + 155] |= 0x80; // second row is invalid

    mock.write(data_base, &rows);
    let live = DbTable {
        database: "mock".to_string(),
        shortname: "CZUM".to_string(),
        shortname_hex: field_code("CZUM"),
        address: table_base,
        field_count_byte: 8,
        data_pointer: data_base,
        header_44: 156,
        header_48: 1247,
        header_4c: 6,
        header_50: 0,
    };

    let validation = validate(&metadata, std::slice::from_ref(&live), &mock, "players").unwrap();
    assert!(validation.focus_verified);
    assert_eq!(validation.metadata_table_count, 4);

    let layout = resolve_layout(&mock, &metadata, std::slice::from_ref(&live)).unwrap();
    assert_eq!(layout.record_size, 156);
    assert_eq!(layout.slot_count, 2);
    assert_eq!(layout.fields.len(), 8);

    let scan = scan_players(&mock, &metadata, &[live], usize::MAX).unwrap();
    assert!(scan.all_checks_passed);
    assert_eq!(scan.valid_slots, 1);
    assert_eq!(scan.invalid_slots, 1);
    assert_eq!(scan.records.len(), 1);
    assert_eq!(scan.records[0].player_id, 1);
    assert_eq!(scan.records[0].overall_rating, 1);
    assert_eq!(scan.records[0].potential, 1);
    assert_eq!(scan.records[0].height_cm, 130);
    assert_eq!(scan.records[0].weight_kg, 30);
}
