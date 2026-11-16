//! Native Linux process inspection via `/proc`.
//!
//! The backend is strictly read-only: it opens `/proc/<pid>/mem` with
//! `O_RDONLY` and never exposes `ptrace`, `process_vm_writev`, `mprotect` or
//! injection primitives.

#![forbid(unsafe_code)]

pub mod policy;
pub mod process;
pub mod procfs;
pub mod wine;

pub use policy::{InspectionPolicy, inspect_policy};
pub use process::{LinuxProcess, LinuxProvider};
