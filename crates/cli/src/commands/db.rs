use fc27_core::{Result, units::format_bytes};
use fc27_game::anchors::ScanOptions;
use fc27_game::db_service::inspect;
use fc27_process::ProcessHandle;
use serde::Serialize;

use crate::cli::DbArgs;
use crate::commands::{open_process, require_module};
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct DbOutput {
    pid: fc27_process::Pid,
    module: String,
    report: fc27_game::db_service::DbServiceReport,
}

pub fn run(args: &DbArgs, json: bool) -> Result<u8> {
    let process = open_process(args.pid)?;
    let modules = process.modules()?;
    let module = require_module(&modules, &args.module)?.clone();

    let options = ScanOptions {
        max_bytes: args.max_mib.saturating_mul(1024 * 1024),
        max_matches: 4,
        executable_sections_only: true,
    };
    let report = inspect(&process, &module, options)?;

    let output = DbOutput {
        pid: args.pid,
        module: module.name.clone(),
        report,
    };

    if json {
        print_json(&output)?;
        return Ok(if output.report.all_checks_passed {
            0
        } else {
            1
        });
    }

    let report = &output.report;
    println!(
        "DB service path: {}",
        if report.all_checks_passed {
            "VERIFIED"
        } else {
            "FAILED CHECKS"
        }
    );
    println!(
        "pattern:         RVA {:#x} / VA {:#x} ({} match(es), scanned {})",
        report.pattern_rva,
        report.pattern_va,
        report.pattern_matches,
        format_bytes(report.scanned_bytes)
    );
    println!(
        "global object:   {:#x} (vtable {:#x}, RVA {})",
        report.global_va,
        report.vtable_va,
        report
            .vtable_rva
            .map(|rva| format!("{rva:#x}"))
            .unwrap_or_else(|| "outside module".into())
    );
    println!(
        "vtable thunk:    {:#x} (RVA {})",
        report.thunk_va,
        report
            .thunk_rva
            .map(|rva| format!("{rva:#x}"))
            .unwrap_or_else(|| "outside module".into())
    );
    println!(
        "service object:  {:#x} (global {:#x}, vtable {:#x})",
        report.service_object_va, report.service_global_va, report.service_vtable_va
    );
    println!(
        "service method:  {:#x} (RVA {})",
        report.service_method_va,
        report
            .service_method_rva
            .map(|rva| format!("{rva:#x}"))
            .unwrap_or_else(|| "outside module".into())
    );
    println!(
        "registry:        key {:#x} at index {} -> {:#x}",
        report.registry_key, report.registry_index, report.registry_value
    );
    println!("table manager:   {:#x}", report.table_manager_root);
    println!(
        "databases:       {}",
        report
            .databases
            .iter()
            .map(|db| format!(
                "{} ({} tables, node {:#x})",
                db.name, db.table_count, db.node_address
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "\ntables:          {} descriptor(s), showing {}",
        report.table_count,
        args.limit.min(report.table_count)
    );
    println!(
        "{:<6} {:<6} {:>5} {:>9} {:>9} {:>5} {:>5}  address",
        "db", "code", "flds", "h44", "h48", "h4c", "h50"
    );
    for table in report.tables.iter().take(args.limit) {
        println!(
            "{:<6} {:<6} {:>5} {:>9} {:>9} {:>5} {:>5}  {:#x}",
            table.database,
            table.shortname,
            table.field_count_byte,
            table.header_44,
            table.header_48,
            table.header_4c,
            table.header_50,
            table.address
        );
    }
    if report.table_count > args.limit {
        println!("... {} more", report.table_count - args.limit);
    }

    println!("\nchecks:");
    for check in &report.checks {
        println!(
            "  [{}] {:<28} {}",
            if check.passed { "ok" } else { "FAIL" },
            check.name,
            check.detail
        );
    }

    println!("\nnotes:");
    for note in &report.notes {
        println!("  - {note}");
    }
    println!("\nplayers table: NOT RESOLVED (full-name mapping is external XML metadata)");

    Ok(if report.all_checks_passed { 0 } else { 1 })
}
