//! Command-line surface.
//!
//! Every command is read-only. The only file written by the tool is an optional
//! memory-dump output explicitly requested with `read --output`.

use clap::{Args, Parser, Subcommand};
use fc27_process::Pid;

#[derive(Debug, Parser)]
#[command(
    name = "fc27le",
    version,
    about = "Read-only FC 27 process inspection for Linux/Wine/Proton",
    long_about = "Native Linux inspection of a running FC 27 process under Wine/Proton. \
                  Discovery, module/section enumeration, bounded memory reads and verified \
                  anchor checks. No writes, injection, hooks or process modification."
)]
pub struct Cli {
    /// Emit machine-readable JSON on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Structured log level (error, warn, info, debug, trace).
    #[arg(long, global = true, default_value = "info", value_name = "LEVEL")]
    pub log_level: String,

    /// Emit logs as JSON lines on stderr.
    #[arg(long, global = true)]
    pub log_json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Find the running FC 27 process and its Wine/Proton helpers.
    Discover(DiscoverArgs),

    /// Show process metadata, mapping/module counts and policy.
    Info { pid: Pid },

    /// List memory mappings.
    Maps(MapsArgs),

    /// List loaded PE/ELF modules.
    Modules(ModulesArgs),

    /// Read a bounded, read-only range.
    Read(ReadArgs),

    /// Scan a module's PE sections for a bounded byte pattern.
    Scan(ScanArgs),

    /// Full read-only reconnaissance report for one process.
    Probe(ProbeArgs),

    /// Verify the game database service path and table manager (read-only).
    Db(DbArgs),

    /// Report the evidence gate for read-only player listing.
    Players(PlayersArgs),

    /// List implemented and out-of-scope capabilities.
    Capabilities,

    /// Spawn a controlled child process and verify the reader against it.
    Selftest,

    /// Internal selftest target.
    #[command(hide = true, name = "selftest-child")]
    SelftestChild,
}

#[derive(Debug, Args)]
pub struct DiscoverArgs {
    /// Include every visible process in the output.
    #[arg(long)]
    pub all: bool,

    /// Maximum number of scored candidates to print (0 = all).
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct MapsArgs {
    pub pid: Pid,

    /// Only mappings whose path contains this text (case-insensitive).
    #[arg(long)]
    pub path_contains: Option<String>,

    /// Only mappings whose file name contains this text (case-insensitive).
    #[arg(long)]
    pub module: Option<String>,

    /// Require all of these permission letters (for example "rx", "rwx", "---").
    #[arg(long)]
    pub perms: Option<String>,

    /// Only mappings that are readable.
    #[arg(long)]
    pub readable: bool,

    /// Only mappings that are writable.
    #[arg(long)]
    pub writable: bool,

    /// Only anonymous mappings.
    #[arg(long)]
    pub anonymous: bool,

    /// Maximum rows to print (0 = all).
    #[arg(long, default_value_t = 200)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct ModulesArgs {
    pub pid: Pid,

    /// Only modules whose name or path contains this text.
    #[arg(long)]
    pub filter: Option<String>,

    /// Include PE section tables in human output.
    #[arg(long)]
    pub sections: bool,
}

#[derive(Debug, Args)]
pub struct ReadArgs {
    pub pid: Pid,

    /// Absolute virtual address (hex or decimal).
    #[arg(long, conflicts_with_all = ["module", "offset"])]
    pub address: Option<String>,

    /// Module name (for example FC27.exe); requires --offset.
    #[arg(long, requires = "offset")]
    pub module: Option<String>,

    /// Module-relative offset (hex or decimal); requires --module.
    #[arg(long, requires = "module")]
    pub offset: Option<String>,

    /// Number of bytes to read (default 256, hard cap 1048576).
    #[arg(long, default_value_t = 256)]
    pub length: usize,

    /// Write the raw bytes to this file instead of only printing them.
    #[arg(long, value_name = "FILE")]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ScanArgs {
    pub pid: Pid,

    /// Module to scan.
    #[arg(long, default_value = "FC27.exe")]
    pub module: String,

    /// Hex byte pattern with ?? wildcards, space-separated.
    #[arg(long, value_name = "PATTERN")]
    pub pattern: String,

    /// Safety cap for total scanned bytes, in MiB.
    #[arg(long, default_value_t = 64)]
    pub max_mib: u64,

    /// Maximum number of matches to report.
    #[arg(long, default_value_t = 16)]
    pub limit: usize,

    /// Scan every PE section, not only executable ones (may read large data).
    #[arg(long)]
    pub all_sections: bool,
}

#[derive(Debug, Args)]
pub struct ProbeArgs {
    pub pid: Pid,

    /// Skip SHA-256 hashing of the 235 MiB game executable.
    #[arg(long)]
    pub no_hash: bool,

    /// Safety cap for anchor scans, in MiB.
    #[arg(long, default_value_t = 160)]
    pub anchor_max_mib: u64,

    /// Skip live anchor verification.
    #[arg(long)]
    pub skip_anchors: bool,
}

#[derive(Debug, Args)]
pub struct DbArgs {
    pub pid: Pid,

    /// Module to inspect.
    #[arg(long, default_value = "FC27.exe")]
    pub module: String,

    /// Safety cap for the database-service pattern scan, in MiB.
    #[arg(long, default_value_t = 160)]
    pub max_mib: u64,

    /// Caller-supplied database metadata XML to resolve names to 4CC codes.
    #[arg(long, value_name = "FILE")]
    pub metadata: Option<String>,

    /// Metadata table to resolve in detail (default players).
    #[arg(long, default_value = "players")]
    pub focus_table: String,

    /// Maximum table descriptors to print in human output.
    #[arg(long, default_value_t = 40)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct PlayersArgs {
    pub pid: Pid,

    /// Module that contains the player table (default FC27.exe).
    #[arg(long, default_value = "FC27.exe")]
    pub module: String,

    /// Caller-supplied database metadata XML used to resolve `players` to its 4CC.
    #[arg(long, value_name = "FILE")]
    pub metadata: Option<String>,

    /// Maximum player records to emit (the full store is still validated).
    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    /// Safety cap for the database-service pattern scan, in MiB.
    #[arg(long, default_value_t = 160)]
    pub max_mib: u64,

    /// Print the full missing-evidence list instead of enumerating.
    #[arg(long)]
    pub explain: bool,
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Discover(_) => "discover",
            Self::Info { .. } => "info",
            Self::Maps(_) => "maps",
            Self::Modules(_) => "modules",
            Self::Read(_) => "read",
            Self::Scan(_) => "scan",
            Self::Probe(_) => "probe",
            Self::Db(_) => "db",
            Self::Players(_) => "players",
            Self::Capabilities => "capabilities",
            Self::Selftest => "selftest",
            Self::SelftestChild => "selftest-child",
        }
    }
}
