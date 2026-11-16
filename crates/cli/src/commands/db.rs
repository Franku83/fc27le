use fc27_core::{Result, units::format_bytes};
use fc27_game::anchors::ScanOptions;
use fc27_game::db_metadata::{MetadataValidation, validate};
use fc27_game::db_service::inspect;
use fc27_process::ProcessHandle;
use serde::Serialize;

use crate::cli::DbArgs;
use crate::commands::{load_metadata, open_process, require_module};
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct MetadataOutput {
    path: String,
    size_bytes: u64,
    sha256: String,
    validation: MetadataValidation,
}

#[derive(Debug, Serialize)]
struct DbOutput {
    pid: fc27_process::Pid,
    module: String,
    report: fc27_game::db_service::DbServiceReport,
    metadata: Option<MetadataOutput>,
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

    let metadata = if let Some(path) = &args.metadata {
        let loaded = load_metadata(path)?;
        let validation = validate(&loaded.parsed, &report.tables, &process, &args.focus_table)?;
        Some(MetadataOutput {
            path: path.clone(),
            size_bytes: loaded.size_bytes,
            sha256: loaded.sha256,
            validation,
        })
    } else {
        None
    };

    let output = DbOutput {
        pid: args.pid,
        module: module.name.clone(),
        report,
        metadata,
    };

    if json {
        print_json(&output)?;
        return Ok(exit_code(&output));
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

    if let Some(metadata) = &output.metadata {
        let validation = &metadata.validation;
        println!("\nmetadata:        {}", metadata.path);
        println!(
            "  size/sha256:   {} / {}",
            metadata.size_bytes, metadata.sha256
        );
        println!(
            "  coverage:      {} table(s); {}/{} matched, {}/{} verified",
            validation.metadata_table_count,
            validation.matched_tables,
            validation.metadata_table_count,
            validation.verified_tables,
            validation.metadata_table_count
        );
        if let Some(focus) = &validation.focus {
            println!(
                "  focus {}:     {} at {}",
                focus.name,
                focus.shortname,
                focus
                    .live_address
                    .map(|address| format!("{address:#x}"))
                    .unwrap_or_else(|| "missing".to_string())
            );
            println!(
                "    fields:      {} metadata / {} live; widths {}/{}; row_size {} bytes; \
                 depth {} bits; padding {} bits",
                focus.metadata_field_count,
                focus.live_field_count.unwrap_or(0),
                focus.width_matches,
                focus.metadata_field_count,
                focus.row_size.unwrap_or(0),
                focus.sum_depth,
                focus.padding_bits.unwrap_or(-1)
            );
            println!(
                "    data:        {:#x}; status: {}",
                focus.data_pointer.unwrap_or(0),
                if focus.verified { "VERIFIED" } else { "FAILED" }
            );
        } else {
            println!(
                "  focus {}:     not present in metadata",
                validation.focus_table
            );
        }
        for note in &validation.notes {
            println!("  note:          {note}");
        }
    } else {
        println!(
            "\nplayers table: NOT RESOLVED (pass --metadata FILE to resolve names to 4CC codes)"
        );
    }

    Ok(exit_code(&output))
}

fn exit_code(output: &DbOutput) -> u8 {
    if !output.report.all_checks_passed {
        return 1;
    }
    if let Some(metadata) = &output.metadata {
        if !metadata.validation.focus_verified {
            return 1;
        }
    }
    0
}
