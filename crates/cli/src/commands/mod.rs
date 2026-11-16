//! One module per CLI command. Commands return an explicit exit code so
//! evidence gates (exit 3) are distinguishable from operational failures.

pub mod capabilities;
pub mod db;
pub mod discover;
pub mod info;
pub mod maps;
pub mod modules;
pub mod players;
pub mod probe;
pub mod read;
pub mod scan;
pub mod selftest;

use fc27_core::{Error, Result};
use fc27_platform_linux::LinuxProcess;
use fc27_process::{ModuleInfo, Pid, modules::find_module};

pub struct LoadedMetadata {
    pub parsed: fc27_game::db_metadata::Metadata,
    pub size_bytes: u64,
    pub sha256: String,
}

/// Read, parse and hash a caller-supplied metadata XML.
///
/// The proprietary file is never copied into the repository; this only reads
/// the path the caller selected and enforces a 16 MiB safety cap.
pub fn load_metadata(path: &str) -> Result<LoadedMetadata> {
    let path_object = std::path::Path::new(path);
    let size_bytes = std::fs::metadata(path_object)
        .map_err(|error| Error::io_context(format!("stat {path}"), error))?
        .len();
    if size_bytes > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput(format!(
            "metadata file {path} is larger than the 16 MiB safety cap"
        )));
    }
    let xml = std::fs::read_to_string(path_object)
        .map_err(|error| Error::io_context(format!("read {path}"), error))?;
    let parsed = fc27_game::db_metadata::parse_metadata_xml(&xml)?;
    let sha256 = fc27_game::build_identity::sha256_file(path_object)?;
    Ok(LoadedMetadata {
        parsed,
        size_bytes,
        sha256,
    })
}

pub fn open_process(pid: Pid) -> Result<LinuxProcess> {
    LinuxProcess::open(pid)
}

pub fn require_module<'a>(modules: &'a [ModuleInfo], needle: &str) -> Result<&'a ModuleInfo> {
    find_module(modules, needle).ok_or_else(|| {
        Error::Unsupported(format!(
            "module {needle:?} is not loaded in this process; use `modules` to list what is loaded"
        ))
    })
}
