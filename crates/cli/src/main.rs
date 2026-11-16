//! `fc27le` — read-only FC 27 process inspection.
//!
//! Safety boundaries are enforced in code, not just documentation:
//! * `/proc/<pid>/mem` is opened `O_RDONLY`,
//! * every read is bounded to 1 MiB and validated against the mapping snapshot,
//! * there is no write, ptrace, inject or hook code path in the workspace.

#![forbid(unsafe_code)]

mod cli;
mod commands;
mod output;

use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use fc27_core::Result;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(&cli);

    let command_name = cli.command.name();
    tracing::info!(command = command_name, "starting");
    let started = Instant::now();

    match dispatch(&cli) {
        Ok(code) => {
            tracing::info!(
                command = command_name,
                exit_code = code,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "finished"
            );
            ExitCode::from(code)
        }
        Err(error) => {
            tracing::error!(
                command = command_name,
                kind = output::error_kind(&error),
                error = %error,
                "failed"
            );
            output::print_error(&error, cli.json);
            ExitCode::from(output::exit_code(&error))
        }
    }
}

fn dispatch(cli: &Cli) -> Result<u8> {
    let json = cli.json;
    match &cli.command {
        Command::Discover(args) => commands::discover::run(args, json),
        Command::Info { pid } => commands::info::run(*pid, json),
        Command::Maps(args) => commands::maps::run(args, json),
        Command::Modules(args) => commands::modules::run(args, json),
        Command::Read(args) => commands::read::run(args, json),
        Command::Scan(args) => commands::scan::run(args, json),
        Command::Probe(args) => commands::probe::run(args, json),
        Command::Db(args) => commands::db::run(args, json),
        Command::Players(args) => commands::players::run(args, json),
        Command::Capabilities => commands::capabilities::run(json),
        Command::Selftest => commands::selftest::run(json),
        Command::SelftestChild => commands::selftest::run_child(),
    }
}

fn init_logging(cli: &Cli) {
    let filter = EnvFilter::try_new(&cli.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    if cli.log_json {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(false)
            .json()
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_target(false)
            .compact()
            .try_init();
    }
}
