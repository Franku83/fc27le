use fc27_core::Result;
use fc27_platform_linux::inspect_policy;
use fc27_process::{Pid, ProcessHandle};
use serde::Serialize;

use crate::commands::open_process;
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct InfoOutput {
    process: fc27_process::ProcessInfo,
    mapping_count: usize,
    module_count: usize,
    policy: fc27_platform_linux::InspectionPolicy,
}

pub fn run(pid: Pid, json: bool) -> Result<u8> {
    let process = open_process(pid)?;
    let info = process.info().clone();
    let mapping_count = process.mappings()?.len();
    let module_count = process.modules()?.len();
    let policy = inspect_policy();

    let output = InfoOutput {
        process: info,
        mapping_count,
        module_count,
        policy,
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    let info = &output.process;
    println!("pid:            {}", info.pid);
    println!("name:           {}", info.name);
    println!("state:          {}", info.state.as_deref().unwrap_or("-"));
    println!(
        "parent pid:     {}",
        info.ppid
            .map(|pid| pid.0.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "user id:        {}",
        info.uid
            .map(|uid| uid.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!("threads:        {}", info.threads.unwrap_or(0));
    println!(
        "resident memory:{}",
        info.rss_bytes
            .map(|bytes| format!(" {}", fc27_core::units::format_bytes(bytes)))
            .unwrap_or_else(|| " -".into())
    );
    println!("role:           {:?}", info.role);
    println!(
        "exe:            {}",
        info.exe_path.as_deref().unwrap_or("-")
    );
    println!("cwd:            {}", info.cwd.as_deref().unwrap_or("-"));
    println!("command line:   {}", info.command_line());
    println!("mappings:       {mapping_count}");
    println!("modules:        {module_count}");

    if let Some(wine) = &info.wine {
        println!("\nWine/Proton:");
        println!("  prefix:        {}", wine.prefix.as_deref().unwrap_or("-"));
        println!(
            "  proton root:   {}",
            wine.proton_root.as_deref().unwrap_or("-")
        );
        println!(
            "  loader:        {}",
            wine.wine_loader.as_deref().unwrap_or("-")
        );
        println!(
            "  esync/fsync/ntsync: {:?}/{:?}/{:?}",
            wine.esync, wine.fsync, wine.ntsync
        );
        println!(
            "  environment:   {}",
            if wine.environment_readable {
                "readable".to_string()
            } else {
                format!(
                    "not readable ({})",
                    wine.environment_error.as_deref().unwrap_or("unknown")
                )
            }
        );
    }

    println!("\nInspection policy:");
    println!(
        "  yama ptrace_scope: {}",
        output
            .policy
            .yama_ptrace_scope
            .map(|value| value.to_string())
            .unwrap_or_else(|| "not present".into())
    );
    println!(
        "  perf_event_paranoid: {}",
        output
            .policy
            .perf_event_paranoid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "not present".into())
    );
    println!(
        "  effective caps:     {:#x}",
        output.policy.capabilities_effective.unwrap_or(0)
    );
    for note in &output.policy.notes {
        println!("  note: {note}");
    }

    Ok(0)
}
