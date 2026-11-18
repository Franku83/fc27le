//! Local diagnostic: run the Linux season exporter against a live FC 27 PID.
//!
//! Usage:
//!   FC27LE_PID=174907 FC27LE_EXPORT_DIR=/tmp/export \
//!     cargo run -p fc27-lua-compat --example live_export
//!
//! This example is read-only. It is intentionally not part of the shipped
//! binary and is useful for validating a new game build.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

use fc27_game::anchors::ScanOptions;
use fc27_game::career_stats::{self, TeamDirectory};
use fc27_game::db_metadata::validate;
use fc27_game::db_service::inspect;
use fc27_game::embedded_profile::metadata as embedded_metadata;
use fc27_game::name_pool::PlayerNameResolver;
use fc27_game::player_store::{PlayerRecord, scan_players};
use fc27_lua_compat::api::{ScriptJob, ScriptSession, build_live_context, run_script};
use fc27_platform_linux::LinuxProcess;
use fc27_process::Pid;
use fc27_process::ProcessHandle;
use fc27_process::modules::find_module;

fn main() -> Result<(), String> {
    let pid = std::env::var("FC27LE_PID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| "set FC27LE_PID to the running FC27.exe process id".to_string())?;
    let script_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts/lua/export_season_stats_linux.lua")
        });
    let process = LinuxProcess::open(Pid(pid)).map_err(|error| error.to_string())?;
    let modules = process.modules().map_err(|error| error.to_string())?;
    let module = find_module(&modules, "FC27.exe")
        .cloned()
        .ok_or_else(|| "FC27.exe is not loaded".to_string())?;
    let report = inspect(
        &process,
        &module,
        ScanOptions {
            max_bytes: 160 * 1024 * 1024,
            max_matches: 4,
            executable_sections_only: true,
        },
    )
    .map_err(|error| error.to_string())?;
    eprintln!(
        "db: service {:#x}, tables {}, checks {}",
        report.service_object_va, report.table_count, report.all_checks_passed
    );
    let metadata = embedded_metadata();
    let validation = validate(&metadata, &report.tables, &process, "players")
        .map_err(|error| error.to_string())?;
    eprintln!(
        "players: verified {}, focus {}",
        validation.verified_tables, validation.focus_verified
    );
    let stats = career_stats::inspect(&process, report.service_object_va)
        .map_err(|error| format!("career inspect: {error}"))?;
    eprintln!(
        "career: in_career {}, date {:?} ({}), stats {} ({})",
        stats.in_career,
        stats.date,
        stats.date_source,
        stats.stats.len(),
        stats.stats_source
    );
    if let Some(note) = &stats.stats_note {
        eprintln!("career note: {note}");
    }
    match TeamDirectory::load(&process, &report.tables) {
        Ok(directory) => eprintln!("teams: {} names", directory.len()),
        Err(error) => eprintln!("teams: unavailable: {error}"),
    }

    let mut scan = scan_players(&process, &metadata, &report.tables, usize::MAX)
        .map_err(|error| error.to_string())?;
    let names = PlayerNameResolver::load(&process, &metadata, &report.tables)
        .map_err(|error| error.to_string())?;
    for record in &mut scan.records {
        let resolved = names.resolve_player(
            record.player_id,
            record.first_name_id,
            record.last_name_id,
            record.common_name_id,
        );
        record.first_name = resolved.first_name;
        record.last_name = resolved.last_name;
        record.common_name = resolved.common_name;
        record.display_name = resolved.display_name;
        record.name_source = Some(resolved.source.to_string());
        record.name_error = resolved.error;
    }
    let records: Vec<PlayerRecord> = scan.records;
    let id_map_path = std::env::var_os("FC27LE_ID_MAP").map(PathBuf::from);
    let session = ScriptSession {
        pid: Pid(pid),
        module_name: module.name.clone(),
        metadata,
        data_path: std::env::var("FC27LE_DATA_PATH").unwrap_or_else(|_| "embedded".to_string()),
        max_db_mib: 160,
        player_records: Arc::new(records),
        id_map_path,
    };
    let (events_tx, events_rx) = mpsc::channel();
    let context = build_live_context(&session, Arc::new(AtomicBool::new(false)), events_tx)
        .map_err(|error| error.to_string())?;
    eprintln!("lua: tables {}", context.tables);
    let source = std::fs::read_to_string(&script_path)
        .map_err(|error| format!("script {}: {error}", script_path.display()))?;
    let script_dir = script_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let job = ScriptJob {
        name: script_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "live_export.lua".to_string()),
        source,
        script_dirs: vec![script_dir],
        timeout: Duration::from_secs(120),
        memory_limit_bytes: 256 * 1024 * 1024,
    };
    let result = run_script(&context.lua, &job, Arc::new(AtomicBool::new(false)));
    while let Ok(event) = events_rx.try_recv() {
        eprintln!("event: {event:?}");
    }
    result.map_err(|error| format!("script: {error}"))?;
    Ok(())
}
