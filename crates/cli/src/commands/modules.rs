use fc27_core::{Result, units::format_bytes};
use fc27_process::{ModuleInfo, ProcessHandle};
use serde::Serialize;

use crate::cli::ModulesArgs;
use crate::commands::open_process;
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct ModulesOutput {
    pid: fc27_process::Pid,
    total: usize,
    returned: usize,
    modules: Vec<ModuleInfo>,
}

pub fn run(args: &ModulesArgs, json: bool) -> Result<u8> {
    let process = open_process(args.pid)?;
    let modules = process.modules()?;

    let mut filtered: Vec<ModuleInfo> = modules
        .into_iter()
        .filter(|module| {
            args.filter.as_ref().is_none_or(|filter| {
                let filter = filter.to_ascii_lowercase();
                module.name.to_ascii_lowercase().contains(&filter)
                    || module.path.to_ascii_lowercase().contains(&filter)
            })
        })
        .collect();
    filtered.sort_by_key(|module| module.base);

    let output = ModulesOutput {
        pid: args.pid,
        total: filtered.len(),
        returned: filtered.len(),
        modules: filtered,
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    println!(
        "pid {}: {} module(s) with PE/ELF headers",
        output.pid, output.total
    );
    println!();
    for module in &output.modules {
        let kind = module.kind.as_str();
        println!(
            "{:016x}  {:>10}  {:<7} {:<24} {}",
            module.base,
            format_bytes(module.size),
            kind,
            module.name,
            module.path
        );

        if let Some(pe) = &module.pe {
            if pe.image_base != module.base {
                println!(
                    "  note: preferred image base {:#x} differs from mapped base {:#x}",
                    pe.image_base, module.base
                );
            }
            if args.sections {
                for section in &pe.sections {
                    println!(
                        "  [{:>2}] {:<10} rva={:#010x} vsize={:#010x} raw={:#010x} size={:#010x} {}{}{}",
                        section.index,
                        section.name,
                        section.virtual_address,
                        section.virtual_size,
                        section.raw_offset,
                        section.raw_size,
                        if section.readable { 'r' } else { '-' },
                        if section.writable { 'w' } else { '-' },
                        if section.executable { 'x' } else { '-' },
                    );
                }
            }
        }
    }

    Ok(0)
}
