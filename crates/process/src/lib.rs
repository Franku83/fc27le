//! Platform-neutral abstractions for read-only process inspection.
//!
//! A backend implements [`ProcessProvider`] and [`ProcessHandle`]. The Linux
//! backend lives in `fc27-platform-linux`; the Windows backend is intentionally
//! a documented stub until it can be tested on Windows.

pub mod backend;
pub mod classify;
pub mod modules;
pub mod pe;
pub mod types;

pub use backend::{ProcessHandle, ProcessProvider};
pub use classify::classify_role;
pub use types::{
    MappingKind, MemoryMapping, MemoryPermissions, ModuleInfo, ModuleKind, PeSectionSummary,
    PeSummary, Pid, ProcessInfo, ProcessRole, WineContext,
};

/// Hard cap for a single read issued through the CLI or a resolver. Larger
/// reads must be split into explicitly bounded chunks by the caller.
pub const MAX_SINGLE_READ: usize = 1 << 20; // 1 MiB
