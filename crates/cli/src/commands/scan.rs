use fc27_core::{Error, Pattern, Result, units::format_bytes};
use fc27_game::anchors::{ScanOptions, scan_module};
use fc27_process::{Pid, ProcessHandle};
use serde::Serialize;

use crate::cli::ScanArgs;
use crate::commands::{open_process, require_module};
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct MatchInfo {
    rva: u64,
    va: u64,
    section: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScanOutput {
    pid: Pid,
    module: String,
    module_base: u64,
    module_size: u64,
    pattern: String,
    executable_sections_only: bool,
    max_bytes: u64,
    scanned_bytes: u64,
    truncated: bool,
    match_count: usize,
    matches: Vec<MatchInfo>,
}

pub fn run(args: &ScanArgs, json: bool) -> Result<u8> {
    let pattern = Pattern::parse(&args.pattern)?;
    if pattern.len() > 4096 {
        return Err(Error::InvalidPattern(
            "pattern is longer than 4096 bytes".to_string(),
        ));
    }

    let process = open_process(args.pid)?;
    let modules = process.modules()?;
    let module = require_module(&modules, &args.module)?.clone();

    let max_bytes = args.max_mib.saturating_mul(1024 * 1024);
    let options = ScanOptions {
        max_bytes,
        max_matches: args.limit.max(1),
        executable_sections_only: !args.all_sections,
    };

    let (rvas, scanned_bytes, truncated) = scan_module(&process, &module, &pattern, options)?;

    let matches: Vec<MatchInfo> = rvas
        .iter()
        .map(|rva| MatchInfo {
            rva: *rva,
            va: module.base + rva,
            section: module
                .section_containing_rva(*rva)
                .map(|section| section.name.clone()),
        })
        .collect();

    let output = ScanOutput {
        pid: args.pid,
        module: module.name.clone(),
        module_base: module.base,
        module_size: module.size,
        pattern: pattern.as_str().to_string(),
        executable_sections_only: options.executable_sections_only,
        max_bytes,
        scanned_bytes,
        truncated,
        match_count: matches.len(),
        matches,
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    println!(
        "scanned {} of {} in {} (cap {})",
        format_bytes(output.scanned_bytes),
        format_bytes(output.module_size),
        output.module,
        format_bytes(output.max_bytes)
    );
    println!("pattern: {} ({} bytes)", output.pattern, pattern.len());
    println!(
        "matches: {}{}",
        output.match_count,
        if output.truncated {
            " (scan truncated by cap)"
        } else {
            ""
        }
    );
    for hit in &output.matches {
        println!(
            "  {:#x}  va={:#x}  {}",
            hit.rva,
            hit.va,
            hit.section.as_deref().unwrap_or("-")
        );
    }

    Ok(0)
}
