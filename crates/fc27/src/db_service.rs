//! Read-only verification of the game-side database service path.
//!
//! Live Editor's `internal function` scans for a 32-byte pattern to locate a game
//! service object. That object is the entry point used by
//! `LE::DatabaseManager::GetTableByShortname` to reach the game's table manager.
//!
//! This module verifies the whole chain **without calling any game function and
//! without writing memory**:
//!
//! pattern -> global object -> vtable thunk -> service registry
//!         -> registry key `0x0AE932D0` -> table-manager root -> tables
//!
//! The traversal stops at table descriptors (4CC shortname and header counts).
//! It does **not** map `players` to a 4CC and does not read player records. That
//! mapping comes from `data/db/fifa_ng_db-meta.xml` (loaded through CFS by Live
//! Editor) and is the next work package.

use std::collections::BTreeSet;

use fc27_core::{Error, Pattern, Result};
use fc27_process::{ModuleInfo, ProcessHandle};
use serde::Serialize;

use crate::anchors::{ScanOptions, scan_module};
use crate::profile::GAME_EXE;

/// Pattern published by Live Editor's DatabaseManager singleton (`internal function`).
pub const DB_SERVICE_PATTERN: &str = "C7 83 60 11 00 00 02 00 00 00 48 8D 0D ?? ?? ?? ?? \
                                      48 8B 05 ?? ?? ?? ?? BA E3 BB BD 0E FF 50 38";
/// Expected RVA in the tested build; independently found in `FC27.exe` and live.
pub const DB_SERVICE_EXPECTED_RVA: u64 = 0x7242D5E;
/// Service registry key used by the DLL (`0xAE932D0`).
pub const DB_REGISTRY_KEY: u32 = 0x0AE9_32D0;
pub const DB_REGISTRY_CAPACITY: usize = 0x21D;
pub const SERVICE_MAP_OFFSET: u64 = 0x48;
pub const SERVICE_MAP_FLAG_OFFSET: u64 = 0x32C0;
pub const SERVICE_MAP_ALT_OFFSET: u64 = 0x1960;
pub const SERVICE_VALUE_OFFSET: u64 = 0x878;
pub const MANAGER_INTERFACE_ADJUST: u64 = 8;
pub const MAX_DATABASES: usize = 64;
pub const MAX_TABLES: usize = 8192;

#[derive(Debug, Clone, Serialize)]
pub struct DbCheck {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbDatabase {
    pub name: String,
    pub node_address: u64,
    pub table_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbTable {
    pub database: String,
    pub shortname: String,
    pub shortname_hex: u32,
    pub address: u64,
    pub field_count_byte: u8,
    pub header_44: u32,
    pub header_48: u32,
    pub header_4c: u32,
    pub header_50: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DbServiceReport {
    pub pattern_rva: u64,
    pub pattern_va: u64,
    pub pattern_matches: usize,
    pub scanned_bytes: u64,
    pub scan_truncated: bool,
    pub global_va: u64,
    pub vtable_va: u64,
    pub vtable_rva: Option<u64>,
    pub thunk_va: u64,
    pub thunk_rva: Option<u64>,
    pub service_global_va: u64,
    pub service_object_va: u64,
    pub service_vtable_va: u64,
    pub service_method_va: u64,
    pub service_method_rva: Option<u64>,
    pub registry_key: u32,
    pub registry_index: usize,
    pub registry_value: u64,
    pub table_manager_root: u64,
    pub databases: Vec<DbDatabase>,
    pub table_count: usize,
    pub tables: Vec<DbTable>,
    pub checks: Vec<DbCheck>,
    pub all_checks_passed: bool,
    pub notes: Vec<String>,
}

pub fn inspect(
    handle: &dyn ProcessHandle,
    module: &ModuleInfo,
    options: ScanOptions,
) -> Result<DbServiceReport> {
    if !module.name.eq_ignore_ascii_case(GAME_EXE) {
        return Err(Error::InvalidInput(format!(
            "database service inspection expects module {GAME_EXE}, got {}",
            module.name
        )));
    }

    let pattern = Pattern::parse(DB_SERVICE_PATTERN)?;
    let (matches, scanned_bytes, scan_truncated) = scan_module(handle, module, &pattern, options)?;
    if !matches.contains(&DB_SERVICE_EXPECTED_RVA) {
        return Err(Error::Unsupported(format!(
            "database service pattern is not at expected RVA {DB_SERVICE_EXPECTED_RVA:#x}; \
             found {} match(es) after scanning {scanned_bytes} bytes (truncated: {scan_truncated}). \
             This build is not verified for the database path.",
            matches.len()
        )));
    }
    let pattern_va = module.base.saturating_add(DB_SERVICE_EXPECTED_RVA);
    let body = handle.read(pattern_va, 32)?;

    // C7 83 ... | 48 8D 0D disp32 | 48 8B 05 disp32 | BA key | FF 50 38
    let lea_disp = i32::from_le_bytes(
        body[13..17]
            .try_into()
            .map_err(|_| Error::parse("DB pattern", "short LEA displacement"))?,
    );
    let lea_target = add_signed(pattern_va + 17, lea_disp)?;
    let mov_disp = i32::from_le_bytes(
        body[20..24]
            .try_into()
            .map_err(|_| Error::parse("DB pattern", "short MOV displacement"))?,
    );
    let mov_target = add_signed(pattern_va + 24, mov_disp)?;

    let global_va = lea_target;
    let vtable_va = handle.read_u64(global_va)?;
    let thunk_va = handle.read_u64(off(vtable_va, 0x30)?)?;
    let thunk_rva = module.rva_of(thunk_va);

    let thunk = handle.read(thunk_va, 16)?;
    let thunk_matches = thunk.len() == 16
        && thunk.starts_with(&[0x48, 0x8B, 0x0D])
        && thunk[7..].starts_with(&[0x48, 0x8B, 0x01])
        && thunk[10..].starts_with(&[0x48, 0xFF, 0x60]);
    let thunk_disp = if thunk.len() >= 7 {
        i32::from_le_bytes(
            thunk[3..7]
                .try_into()
                .map_err(|_| Error::parse("DB thunk", "short displacement"))?,
        )
    } else {
        0
    };
    let service_global_va = add_signed(off(thunk_va, 7)?, thunk_disp)?;

    let service_object_va = handle.read_u64(service_global_va)?;
    let service_vtable_va = handle.read_u64(service_object_va)?;
    let service_method_va = handle.read_u64(off(service_vtable_va, 0x40)?)?;
    let service_method_rva = module.rva_of(service_method_va);

    let (registry_index, registry_value) = locate_registry_entry(handle, service_object_va)?;
    let table_manager_root = registry_value
        .checked_sub(MANAGER_INTERFACE_ADJUST)
        .ok_or_else(|| Error::parse("DB registry", "manager interface adjustment underflow"))?;

    let (databases, tables) = traverse_table_manager(handle, table_manager_root)?;

    let global_in_module = module.rva_of(global_va).is_some();
    let vtable_in_module = module.rva_of(vtable_va).is_some();
    let thunk_in_module = thunk_rva.is_some();
    let method_in_module = service_method_rva.is_some();
    let database_names: Vec<&str> = databases.iter().map(|db| db.name.as_str()).collect();
    let expected_databases = database_names.contains(&"g_db") && database_names.contains(&"a_mx");

    let checks = vec![
        check(
            "pattern_unique",
            matches.len() == 1,
            format!(
                "{} match(es), scanned {} bytes",
                matches.len(),
                scanned_bytes
            ),
        ),
        check(
            "expected_rva",
            matches.contains(&DB_SERVICE_EXPECTED_RVA),
            format!("expected RVA {DB_SERVICE_EXPECTED_RVA:#x}"),
        ),
        check(
            "lea_mov_targets_agree",
            lea_target == mov_target,
            format!("LEA {lea_target:#x}, MOV {mov_target:#x}"),
        ),
        check(
            "global_in_module",
            global_in_module,
            format!("global {global_va:#x}"),
        ),
        check(
            "vtable_in_module",
            vtable_in_module,
            format!("vtable {vtable_va:#x}"),
        ),
        check(
            "thunk_in_module",
            thunk_in_module,
            format!("thunk {thunk_va:#x}"),
        ),
        check(
            "thunk_forwarder_shape",
            thunk_matches,
            format!("bytes {}", fc27_core::hex::to_hex(&thunk)),
        ),
        check(
            "service_method_in_module",
            method_in_module,
            format!("method {service_method_va:#x}"),
        ),
        check(
            "registry_key_found",
            registry_index < DB_REGISTRY_CAPACITY && registry_value != 0,
            format!(
                "key {DB_REGISTRY_KEY:#x} at index {registry_index}, value {registry_value:#x}"
            ),
        ),
        check(
            "manager_root_readable",
            table_manager_root != 0,
            format!("root {table_manager_root:#x}"),
        ),
        check(
            "expected_databases",
            expected_databases,
            format!("databases: {database_names:?}"),
        ),
        check(
            "tables_nonempty",
            !tables.is_empty(),
            format!("{} table descriptor(s)", tables.len()),
        ),
    ];
    let all_checks_passed = checks.iter().all(|check| check.passed);

    let mut notes = vec![
        "The table manager is reached by pointer traversal only; no game function is called and \
         no memory is written."
            .to_string(),
        "Table shortnames are 4CC game codes. The full-name -> 4CC map is loaded by Live Editor \
         from data/db/fifa_ng_db-meta.xml through CFS; no verified name->4CC map was found in \
         the game process."
            .to_string(),
        "The `players` table is not resolved here. See docs/research/db_interface.md for the \
         remaining evidence gate."
            .to_string(),
    ];
    if scan_truncated {
        notes.push(format!(
            "Pattern scan reported truncation after {scanned_bytes} bytes; the expected match was \
             still found."
        ));
    }

    Ok(DbServiceReport {
        pattern_rva: DB_SERVICE_EXPECTED_RVA,
        pattern_va,
        pattern_matches: matches.len(),
        scanned_bytes,
        scan_truncated,
        global_va,
        vtable_va,
        vtable_rva: module.rva_of(vtable_va),
        thunk_va,
        thunk_rva,
        service_global_va,
        service_object_va,
        service_vtable_va,
        service_method_va,
        service_method_rva,
        registry_key: DB_REGISTRY_KEY,
        registry_index,
        registry_value,
        table_manager_root,
        databases,
        table_count: tables.len(),
        tables,
        checks,
        all_checks_passed,
        notes,
    })
}

fn check(name: &'static str, passed: bool, detail: String) -> DbCheck {
    DbCheck {
        name,
        passed,
        detail,
    }
}

fn add_signed(base: u64, displacement: i32) -> Result<u64> {
    base.checked_add_signed(i64::from(displacement))
        .ok_or_else(|| Error::parse("RIP-relative address", "signed addition overflow"))
}

fn off(base: u64, offset: u64) -> Result<u64> {
    base.checked_add(offset)
        .ok_or_else(|| Error::parse("address arithmetic", format!("{base:#x} + {offset:#x}")))
}

fn locate_registry_entry(handle: &dyn ProcessHandle, service_object: u64) -> Result<(usize, u64)> {
    let map_base = off(service_object, SERVICE_MAP_OFFSET)?;
    let flag = handle.read_u32(off(map_base, SERVICE_MAP_FLAG_OFFSET)?)?;
    let key_base = if flag != 0 {
        off(map_base, SERVICE_MAP_ALT_OFFSET)?
    } else {
        map_base
    };
    let value_base = off(key_base, SERVICE_VALUE_OFFSET)?;

    for index in 0..DB_REGISTRY_CAPACITY {
        let key_address = off(key_base, (index as u64) * 4)?;
        let key = handle.read_u32(key_address)?;
        if key == DB_REGISTRY_KEY {
            let value_address = off(value_base, (index as u64) * 8)?;
            let value = handle.read_u64(value_address)?;
            return Ok((index, value));
        }
    }

    Err(Error::Unsupported(format!(
        "service registry key {DB_REGISTRY_KEY:#x} was not found in the {DB_REGISTRY_CAPACITY}-entry \
         map at {map_base:#x}; this build does not match the verified DB path"
    )))
}

fn traverse_table_manager(
    handle: &dyn ProcessHandle,
    root: u64,
) -> Result<(Vec<DbDatabase>, Vec<DbTable>)> {
    let p1 = handle.read_u64(off(root, 0x20)?)?;
    let p2 = handle.read_u64(off(p1, 8)?)?;
    let mut node = handle.read_u64(off(p2, 0x10)?)?;

    let mut databases = Vec::new();
    let mut tables = Vec::new();
    let mut seen_nodes = BTreeSet::new();
    let mut seen_tables = BTreeSet::new();
    let mut node_count = 0;

    while node != 0 && node_count < MAX_DATABASES && seen_nodes.insert(node) {
        let name = fourcc(&handle.read(off(node, 4)?, 4)?);
        let mut table = handle.read_u64(off(node, 0x10)?)?;
        let mut table_count = 0;

        while table != 0 && tables.len() < MAX_TABLES && seen_tables.insert(table) {
            let raw = handle.read(off(table, 0x40)?, 4)?;
            let shortname_hex = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let field_count_byte = handle.read(off(table, 0x82)?, 1)?[0];
            let header_44 = handle.read_u32(off(table, 0x44)?)?;
            let header_48 = handle.read_u32(off(table, 0x48)?)?;
            let header_4c = handle.read_u32(off(table, 0x4c)?)?;
            let header_50 = handle.read_u32(off(table, 0x50)?)?;

            tables.push(DbTable {
                database: name.clone(),
                shortname: fourcc(&raw),
                shortname_hex,
                address: table,
                field_count_byte,
                header_44,
                header_48,
                header_4c,
                header_50,
            });
            table_count += 1;
            table = handle.read_u64(off(table, 8)?)?;
        }

        databases.push(DbDatabase {
            name,
            node_address: node,
            table_count,
        });
        node_count += 1;
        node = handle.read_u64(off(node, 0x18)?)?;
    }

    Ok((databases, tables))
}

fn fourcc(bytes: &[u8]) -> String {
    bytes
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_parses() {
        let pattern = Pattern::parse(DB_SERVICE_PATTERN).unwrap();
        assert_eq!(pattern.len(), 32);
    }

    #[test]
    fn fourcc_is_printable_or_dotted() {
        assert_eq!(fourcc(b"SPRL"), "SPRL");
        assert_eq!(fourcc(&[0, 0x41, 0x42, 0x43]), ".ABC");
    }

    #[test]
    fn signed_rip_target() {
        assert_eq!(add_signed(0x1000, -0x10).unwrap(), 0xff0);
        assert!(add_signed(0, -1).is_err());
    }
}
