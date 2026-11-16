//! Windows process backend.
//!
//! # Status
//!
//! This crate is a deliberate, documented stub. The Linux backend is the only
//! backend that has been exercised against FC 27. A Windows implementation must
//! be written and tested on Windows before any capability is claimed.

#![forbid(unsafe_code)]
//!
//! When `cfg(windows)` is active, [`WindowsProvider::open`] returns
//! [`fc27_core::Error::Unsupported`] with an explicit message. The module
//! boundaries are already in place so that a `windows-sys` implementation of
//! `OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION)`,
//! `ReadProcessMemory`, `VirtualQueryEx` and `EnumProcessModules` can be added
//! without touching callers.

use fc27_core::{Error, Result};
use fc27_process::{Pid, ProcessHandle, ProcessInfo, ProcessProvider};

/// Placeholder provider for the future Windows backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsProvider;

impl WindowsProvider {
    pub fn new() -> Self {
        Self
    }
}

/// The Windows backend has no verified capability yet.
pub const IMPLEMENTATION_STATUS: &str = "stub";

/// Statement returned by any attempt to use the backend.
pub const UNSUPPORTED_MESSAGE: &str = "the Windows backend is not implemented; only the Linux /proc backend has been verified \
     against FC 27. Do not claim Windows support until a Windows build has been tested.";

impl ProcessProvider for WindowsProvider {
    fn list(&self) -> Result<Vec<ProcessInfo>> {
        Err(Error::Unsupported(UNSUPPORTED_MESSAGE.to_string()))
    }

    fn info(&self, _pid: Pid) -> Result<ProcessInfo> {
        Err(Error::Unsupported(UNSUPPORTED_MESSAGE.to_string()))
    }

    fn open(&self, _pid: Pid) -> Result<Box<dyn ProcessHandle>> {
        Err(Error::Unsupported(UNSUPPORTED_MESSAGE.to_string()))
    }
}
