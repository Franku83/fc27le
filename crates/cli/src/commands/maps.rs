use fc27_core::{Error, Result, units::format_bytes};
use fc27_process::{MemoryMapping, ProcessHandle};
use serde::Serialize;

use crate::cli::MapsArgs;
use crate::commands::open_process;
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct MapsOutput {
    pid: fc27_process::Pid,
    total: usize,
    returned: usize,
    filters: Vec<String>,
    mappings: Vec<MemoryMapping>,
}

pub fn run(args: &MapsArgs, json: bool) -> Result<u8> {
    let process = open_process(args.pid)?;
    let all = process.mappings()?;

    let permission_filter = match &args.perms {
        Some(text) => {
            let text = text.to_ascii_lowercase();
            if !text.chars().all(|ch| "rwxsp-".contains(ch)) {
                return Err(Error::InvalidInput(format!(
                    "--perms may only contain r, w, x, s, p or '-': {text:?}"
                )));
            }
            Some(text)
        }
        None => None,
    };

    let mut filters = Vec::new();
    if let Some(path) = &args.path_contains {
        filters.push(format!("path contains {path:?}"));
    }
    if let Some(module) = &args.module {
        filters.push(format!("file name contains {module:?}"));
    }
    if let Some(perms) = &args.perms {
        filters.push(format!("permissions contain {perms:?}"));
    }
    if args.readable {
        filters.push("readable".to_string());
    }
    if args.writable {
        filters.push("writable".to_string());
    }
    if args.anonymous {
        filters.push("anonymous".to_string());
    }

    let filtered: Vec<MemoryMapping> = all
        .into_iter()
        .filter(|mapping| {
            if let Some(path) = &args.path_contains {
                let lower = path.to_ascii_lowercase();
                if !mapping
                    .path
                    .as_deref()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(&lower))
                {
                    return false;
                }
            }
            if let Some(module) = &args.module {
                let lower = module.to_ascii_lowercase();
                if !mapping
                    .file_name()
                    .is_some_and(|value| value.to_ascii_lowercase().contains(&lower))
                {
                    return false;
                }
            }
            if let Some(perms) = &permission_filter {
                let actual = mapping.permissions.to_string();
                if !perms
                    .chars()
                    .filter(|ch| *ch != '-')
                    .all(|ch| actual.contains(ch))
                {
                    return false;
                }
            }
            if args.readable && !mapping.permissions.read {
                return false;
            }
            if args.writable && !mapping.permissions.write {
                return false;
            }
            if args.anonymous && mapping.path.is_some() {
                return false;
            }
            true
        })
        .collect();

    let total = filtered.len();
    let mut returned = filtered;
    if args.limit > 0 {
        returned.truncate(args.limit);
    }

    let output = MapsOutput {
        pid: args.pid,
        total,
        returned: returned.len(),
        filters,
        mappings: returned,
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    println!(
        "pid {}: {} mapping(s) matched, showing {}",
        output.pid, output.total, output.returned
    );
    if let Some(filters) = (!output.filters.is_empty()).then(|| output.filters.join(", ")) {
        println!("filters: {filters}");
    }
    if args.module.is_some() {
        println!(
            "note: Wine PE images may have anonymous sections not listed by --module; \
             `modules` reports the full SizeOfImage."
        );
    }
    println!();
    for mapping in &output.mappings {
        println!(
            "{:016x}-{:016x} {:<5} {:>10} off={:08x} {}",
            mapping.start,
            mapping.end,
            mapping.permissions.to_string(),
            format_bytes(mapping.size()),
            mapping.offset,
            mapping.path.as_deref().unwrap_or("[anonymous]"),
        );
    }

    Ok(0)
}
