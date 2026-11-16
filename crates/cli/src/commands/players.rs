use fc27_core::Result;
use fc27_game::players::{capability_report, resolve};
use fc27_process::{Pid, ProcessHandle};
use serde::Serialize;

use crate::cli::PlayersArgs;
use crate::commands::{open_process, require_module};
use crate::output::print_json;

#[derive(Debug, Serialize)]
struct PlayersOutput {
    pid: Pid,
    module: String,
    module_present: bool,
    status: &'static str,
    summary: &'static str,
    known_from_research: Vec<String>,
    missing_evidence: Vec<fc27_game::players::MissingEvidence>,
    policy: &'static str,
    player_records: Vec<fc27_game::players::Player>,
}

pub fn run(args: &PlayersArgs, json: bool) -> Result<u8> {
    let report = capability_report();
    let process = open_process(args.pid)?;
    let modules = process.modules()?;
    let module = require_module(&modules, &args.module)?.clone();

    let player_records = match resolve(&process, &module) {
        Ok(players) => players,
        Err(error) if error.is_insufficient_evidence() => Vec::new(),
        Err(error) => return Err(error),
    };

    let implemented = !player_records.is_empty()
        || (report.status != "blocked_on_evidence" && report.missing_evidence.is_empty());

    let output = PlayersOutput {
        pid: args.pid,
        module: module.name.clone(),
        module_present: true,
        status: report.status,
        summary: report.summary,
        known_from_research: report.known_from_research,
        missing_evidence: report.missing_evidence,
        policy: report.policy,
        player_records,
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

        if args.explain || !implemented {
            println!("\nMissing evidence:");
            for (index, item) in output.missing_evidence.iter().enumerate() {
                println!("  {}. {}", index + 1, item.requirement);
                println!("     why: {}", item.why);
                println!("     collect: {}", item.how_to_collect);
            }
        } else {
            println!(
                "\nMissing evidence: {} item(s); use --explain to list them.",
                output.missing_evidence.len()
            );
        }
        println!("\npolicy: {}", output.policy);
        if output.player_records.is_empty() {
            println!("\nNo player records were produced.");
        } else {
            println!(
                "\nPlayers ({}): id / name / team / overall / potential / position",
                output.player_records.len()
            );
            for player in &output.player_records {
                let name = player
                    .common_name
                    .as_deref()
                    .or(player.last_name.as_deref())
                    .unwrap_or("-");
                println!(
                    "  {:>7}  {:<28} team={:<7} ovr={:<3} pot={:<3} pos={}",
                    player.player_id,
                    name,
                    player
                        .team_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".into()),
                    player
                        .overall
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".into()),
                    player
                        .potential
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".into()),
                    player.position.as_deref().unwrap_or("-")
                );
            }
        }
    }

    if implemented {
        Ok(0)
    } else {
        // Evidence gate, not an operational failure. Exit code 3 tells callers
        // that the report is complete but the capability is not available.
        Ok(3)
    }
}
