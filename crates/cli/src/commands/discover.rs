use fc27_core::Result;
use fc27_game::discovery::{DiscoveredProcess, analyze};
use fc27_platform_linux::LinuxProvider;
use fc27_process::ProcessProvider;
use serde::Serialize;

use crate::cli::DiscoverArgs;
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct ProcessSummary {
    pid: fc27_process::Pid,
    name: String,
    role: fc27_process::ProcessRole,
    cmdline: String,
}

#[derive(Debug, Serialize)]
struct DiscoverOutput {
    process_count: usize,
    game: Option<DiscoveredProcess>,
    candidates: Vec<DiscoveredProcess>,
    wine_helpers: Vec<DiscoveredProcess>,
    all_processes: Option<Vec<ProcessSummary>>,
    notes: Vec<String>,
}

pub fn run(args: &DiscoverArgs, json: bool) -> Result<u8> {
    let provider = LinuxProvider::new();
    let processes = provider.list()?;
    let report = analyze(&processes);
    tracing::info!(
        process_count = processes.len(),
        game_pid = ?report.game.as_ref().map(|game| game.pid.0),
        candidate_count = report.candidates.len(),
        "process discovery complete"
    );

    let mut candidates = report.candidates;
    if args.limit > 0 {
        candidates.truncate(args.limit);
    }

    let all_processes = args.all.then(|| {
        processes
            .iter()
            .map(|process| ProcessSummary {
                pid: process.pid,
                name: process.name.clone(),
                role: process.role,
                cmdline: process.command_line(),
            })
            .collect()
    });

    let output = DiscoverOutput {
        process_count: processes.len(),
        game: report.game.clone(),
        candidates,
        wine_helpers: report.wine_helpers,
        all_processes,
        notes: report.notes,
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    println!("Visible processes: {}", output.process_count);
    match &output.game {
        Some(game) => {
            println!(
                "\nFC 27 game: pid {} ({}, score {})",
                game.pid, game.name, game.score
            );
            print_process_line("  exe", game.exe_path.as_deref());
            print_process_line("  cwd", game.cwd.as_deref());
            print_process_line("  wine prefix", game.wine_prefix.as_deref());
            print_process_line("  proton root", game.proton_root.as_deref());
            println!("  command line: {}", game.cmdline.join(" "));
            println!("  reasons:");
            for reason in &game.reasons {
                println!("    - {reason}");
            }
        }
        None => println!("\nFC 27 game: not found"),
    }

    if !output.candidates.is_empty() {
        println!("\nCandidates:");
        for candidate in &output.candidates {
            println!(
                "  pid {:>7}  score {:>4}  {:<10} {}",
                candidate.pid,
                candidate.score,
                candidate.name,
                candidate.exe_path.as_deref().unwrap_or("-")
            );
        }
    }

    if !output.wine_helpers.is_empty() {
        println!("\nWine helpers ({}):", output.wine_helpers.len());
        for helper in output.wine_helpers.iter().take(12) {
            println!(
                "  pid {:>7}  {:<14} {}",
                helper.pid,
                helper.name,
                role_label(helper.role)
            );
        }
    }

    if let Some(processes) = &output.all_processes {
        println!("\nAll visible processes ({}):", processes.len());
        for process in processes {
            println!(
                "  pid {:>7}  {:<24} {}",
                process.pid,
                process.name,
                role_label(process.role)
            );
        }
    }

    for note in &output.notes {
        println!("\nnote: {note}");
    }

    Ok(0)
}

fn print_process_line(label: &str, value: Option<&str>) {
    if let Some(value) = value {
        println!("{label}: {value}");
    }
}

fn role_label(role: fc27_process::ProcessRole) -> &'static str {
    use fc27_process::ProcessRole;
    match role {
        ProcessRole::WineServer => "wineserver",
        ProcessRole::WineDevice => "winedevice",
        ProcessRole::WineApplication => "wine-app",
        ProcessRole::Steam => "steam",
        ProcessRole::NativeApplication => "native",
        ProcessRole::KernelThread => "kernel",
    }
}
