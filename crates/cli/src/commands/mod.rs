//! One module per CLI command. Commands return an explicit exit code so
//! evidence gates (exit 3) are distinguishable from operational failures.

pub mod capabilities;
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
