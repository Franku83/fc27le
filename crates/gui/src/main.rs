mod app;
mod kwin;
mod session;
mod socket;
mod sort;

use std::path::PathBuf;

use clap::Parser;
use fc27_process::Pid;

use crate::app::{App, AppOptions};

#[derive(Debug, Parser)]
#[command(
    name = "fc27le-gui",
    version,
    about = "FC27LE — read-only FC 27 player browser and Live Editor Lua console"
)]
struct Args {
    /// Caller-supplied fifa_ng_db-meta.xml
    #[arg(long, value_name = "FILE")]
    metadata: Option<PathBuf>,

    /// FC27.exe pid; otherwise auto-discover
    #[arg(long)]
    pid: Option<u32>,

    /// Safety cap for the database-service pattern scan, in MiB
    #[arg(long, default_value_t = 160)]
    max_mib: u64,

    /// Initial player search
    #[arg(long)]
    filter: Option<String>,

    /// Load a Lua script into the editor at startup
    #[arg(long, value_name = "FILE")]
    script: Option<PathBuf>,

    /// Run the startup script automatically after the Lua session is ready
    #[arg(long)]
    run_script: bool,

    /// Do not auto-connect even if a game and metadata are available
    #[arg(long)]
    no_auto_connect: bool,

    /// Send toggle to a running FC27LE GUI and exit
    #[arg(long)]
    toggle: bool,

    /// Show a hidden FC27LE GUI and exit
    #[arg(long)]
    show: bool,

    /// Hide a running FC27LE GUI and exit
    #[arg(long)]
    hide: bool,
}

fn main() {
    let args = Args::parse();
    let selected = [args.toggle, args.show, args.hide]
        .iter()
        .filter(|value| **value)
        .count();
    if selected > 1 {
        eprintln!("fc27le-gui: choose only one of --toggle, --show, --hide");
        std::process::exit(2);
    }
    if args.toggle || args.show || args.hide {
        let command = if args.toggle {
            "toggle"
        } else if args.show {
            "show"
        } else {
            "hide"
        };
        if let Err(error) = socket::send_command(command) {
            eprintln!("fc27le-gui: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = run(args) {
        eprintln!("fc27le-gui: {error}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let app_options = AppOptions {
        metadata: args.metadata,
        pid: args.pid.map(Pid),
        max_mib: args.max_mib,
        filter: args.filter,
        script: args.script,
        run_script: args.run_script,
        no_auto_connect: args.no_auto_connect,
    };
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("FC27LE"),
        ..Default::default()
    };
    eframe::run_native(
        "FC27LE",
        native_options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, app_options)))),
    )
    .map_err(|error| error.to_string())
}
