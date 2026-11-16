use fc27_core::{Error, Result};
use fc27_game::anchors::ScanOptions;
use fc27_game::db_metadata::validate;
use fc27_game::db_service::inspect;
use fc27_game::name_pool::{NameResolutionReport, PlayerNameResolver};
use fc27_game::player_store::{PlayerScan, scan_players};
use fc27_game::players::capability_report;
use fc27_process::{Pid, ProcessHandle};
use serde::Serialize;

use crate::cli::PlayersArgs;
use crate::commands::{load_metadata, open_process, require_module};
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct MetadataInfo {
    path: String,
    size_bytes: u64,
    sha256: String,
    table_count: usize,
    matched_tables: usize,
    verified_tables: usize,
    focus_verified: bool,
}

#[derive(Debug, Serialize)]
struct PlayersOutput {
    pid: Pid,
    module: String,
    module_base: u64,
    status: &'static str,
    metadata: MetadataInfo,
    scan: PlayerScan,
    name_resolution: Option<NameResolutionReport>,
    policy: &'static str,
}

pub fn run(args: &PlayersArgs, json: bool) -> Result<u8> {
    if args.explain || args.metadata.is_none() {
        return run_gate(args, json);
    }

    let process = open_process(args.pid)?;
    let modules = process.modules()?;
    let module = require_module(&modules, &args.module)?.clone();

    let options = ScanOptions {
        max_bytes: args.max_mib.saturating_mul(1024 * 1024),
        max_matches: 4,
        executable_sections_only: true,
    };
    let report = inspect(&process, &module, options)?;
    if !report.all_checks_passed {
        return Err(Error::InsufficientEvidence(
            "the database-service path did not pass its verification checks; refusing to read \
             player records"
                .to_string(),
        ));
    }

    let metadata_path = args
        .metadata
        .as_deref()
        .expect("metadata presence checked above");
    let loaded = load_metadata(metadata_path)?;
    let validation = validate(&loaded.parsed, &report.tables, &process, "players")?;
    if !validation.focus_verified {
        return Err(Error::InsufficientEvidence(
            "the `players` descriptor did not match the supplied metadata; refusing to enumerate"
                .to_string(),
        ));
    }

    let mut scan = scan_players(&process, &loaded.parsed, &report.tables, args.limit)?;

    let name_resolution = if args.no_names {
        None
    } else {
        let resolver = PlayerNameResolver::load(&process, &loaded.parsed, &report.tables)?;
        for record in &mut scan.records {
            let resolved = resolver.resolve_player(
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
        Some(resolver.report(&scan.records))
    };

    let output = PlayersOutput {
        pid: args.pid,
        module: module.name.clone(),
        module_base: module.base,
        status: if scan.all_checks_passed {
            "enumerated"
        } else {
            "insufficient_evidence"
        },
        metadata: MetadataInfo {
            path: metadata_path.to_string(),
            size_bytes: loaded.size_bytes,
            sha256: loaded.sha256,
            table_count: validation.metadata_table_count,
            matched_tables: validation.matched_tables,
            verified_tables: validation.verified_tables,
            focus_verified: validation.focus_verified,
        },
        scan,
        name_resolution,
        policy: "Read-only: no writes, no hooks, no DLL injection, no game-function calls. \
                 Invalid slots are skipped; only verified numeric fields and names from the \
                 verified name tables are emitted. Missing names stay null and are reported.",
    };

    if json {
        print_json(&output)?;
    } else {
        print_human(&output);
    }

    let names_ok = output
        .name_resolution
        .as_ref()
        .is_none_or(|names| names.all_checks_passed);
    if output.scan.all_checks_passed && names_ok {
        Ok(0)
    } else {
        Ok(3)
    }
}

fn print_human(output: &PlayersOutput) {
    let scan = &output.scan;
    let layout = &scan.layout;
    println!("players:          {}", output.status);
    println!(
        "module:           {} at {:#x}",
        output.module, output.module_base
    );
    println!(
        "metadata:         {} ({} bytes, sha256 {})",
        output.metadata.path, output.metadata.size_bytes, output.metadata.sha256
    );
    println!(
        "coverage:         {}/{} matched, {}/{} verified; players focus {}",
        output.metadata.matched_tables,
        output.metadata.table_count,
        output.metadata.verified_tables,
        output.metadata.table_count,
        if output.metadata.focus_verified {
            "VERIFIED"
        } else {
            "FAILED"
        }
    );
    println!(
        "descriptor:       {} / {} at {:#x}; record_size {} bytes",
        layout.table_name, layout.shortname, layout.descriptor, layout.record_size
    );
    println!(
        "storage:          data {:#x}; slots {}/{}; invalid markers {}; valid {}",
        layout.data_pointer,
        layout.slot_count,
        layout.capacity,
        layout.invalid_marker_count,
        scan.valid_slots
    );
    println!(
        "snapshot:         {}; scanned {} slot(s), skipped {} invalid",
        if scan.stable_reads {
            "two identical reads"
        } else {
            "single read"
        },
        scan.slots_scanned,
        scan.skipped_invalid()
    );
    if let Some(names) = &output.name_resolution {
        println!(
            "names:            resolved ({} playernames row(s), {} tree node(s); {} dcplayernames row(s); {} edited override(s))",
            names.playernames.table.rows,
            names.playernames.tree_nodes,
            names
                .dcplayernames
                .as_ref()
                .map(|table| table.rows.to_string())
                .unwrap_or_else(|| "not loaded".to_string()),
            names
                .editedplayernames
                .as_ref()
                .map(|table| table.overrides.to_string())
                .unwrap_or_else(|| "not loaded".to_string())
        );
        println!(
            "                  emitted {} with names, {} without; {} name error(s)",
            names.emitted_with_names, names.emitted_without_names, names.name_errors
        );
        println!(
            "                  name checks: {}/{} passed",
            names.checks.iter().filter(|check| check.passed).count(),
            names.checks.len()
        );
        for note in &names.notes {
            println!("  note:           {note}");
        }
    } else {
        println!("names:            disabled (--no-names)");
    }
    println!("\nfield layout:");
    for field in &layout.fields {
        println!(
            "  {:<14} bit {:>4} / width {:>2}  stored range {}..{} (value = bits + {})",
            field.name, field.bit_offset, field.bit_width, field.low, field.high, field.low
        );
    }
    println!("\nchecks:");
    for check in &scan.checks {
        println!(
            "  [{}] {:<24} {}",
            if check.passed { "ok" } else { "FAIL" },
            check.name,
            check.detail
        );
    }
    if scan.records.is_empty() {
        println!("\nNo player records were emitted.");
        return;
    }
    println!(
        "\nplayers ({} emitted, {} valid{}):",
        scan.records.len(),
        scan.valid_slots,
        if scan.truncated { ", truncated" } else { "" }
    );
    println!(
        "  {:<6} {:>8}  {:<30}  {:>3}  {:>3}  {:>6}  {:>6}",
        "slot", "playerid", "name", "ovr", "pot", "height", "weight"
    );
    for record in &scan.records {
        let name = record.display_name.as_deref().unwrap_or("-");
        let name: String = name.chars().take(30).collect();
        println!(
            "  {:<6} {:>8}  {:<30}  {:>3}  {:>3}  {:>6}  {:>6}",
            record.record_index,
            record.player_id,
            name,
            record.overall_rating,
            record.potential,
            record.height_cm,
            record.weight_kg
        );
    }
}

fn run_gate(args: &PlayersArgs, json: bool) -> Result<u8> {
    let report = capability_report();
    let process = open_process(args.pid)?;
    let modules = process.modules()?;
    let module = require_module(&modules, &args.module)?.clone();
    let output = GateOutput {
        pid: args.pid,
        module: module.name.clone(),
        module_present: true,
        status: report.status,
        summary: report.summary,
        known_from_research: report.known_from_research,
        missing_evidence: report.missing_evidence,
        policy: report.policy,
    };
    if json {
        print_json(&output)?;
    } else {
        println!("player listing: {}", output.status);
        println!("module:         {} at {:#x}", output.module, module.base);
        println!("\n{}", output.summary);
        println!("\nKnown from research:");
        for item in &output.known_from_research {
            println!("  - {item}");
        }
        println!("\nMissing evidence:");
        for (index, item) in output.missing_evidence.iter().enumerate() {
            println!("  {}. {}", index + 1, item.requirement);
            println!("     why: {}", item.why);
            println!("     collect: {}", item.how_to_collect);
        }
        println!("\npolicy: {}", output.policy);
        println!(
            "\nPass --metadata FILE to enumerate verified numeric player fields \
             (playerid, overallrating, potential, height, weight)."
        );
    }
    Ok(3)
}

#[derive(Debug, Serialize)]
struct GateOutput {
    pid: Pid,
    module: String,
    module_present: bool,
    status: &'static str,
    summary: &'static str,
    known_from_research: Vec<String>,
    missing_evidence: Vec<fc27_game::players::MissingEvidence>,
    policy: &'static str,
}
