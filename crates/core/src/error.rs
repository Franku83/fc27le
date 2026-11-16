use std::io;

/// Workspace-wide error type.
///
/// Variants carry enough context to produce an actionable CLI message. The
/// `Unsupported` and `InsufficientEvidence` variants exist so that missing
/// evidence can never silently turn into a fabricated implementation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("process {pid} was not found")]
    ProcessNotFound { pid: u32 },

    #[error("permission denied while inspecting process {pid} ({context})")]
    PermissionDenied {
        pid: u32,
        context: String,
        #[source]
        source: io::Error,
    },

    #[error("process {pid} is not available: {reason}")]
    ProcessUnavailable { pid: u32, reason: String },

    #[error(
        "address range {start:#x}..{end:#x} ({len} bytes) is not fully contained in a single \
         mapping of process {pid}"
    )]
    UnmappedRange {
        pid: u32,
        start: u64,
        end: u64,
        len: usize,
    },

    #[error(
        "read of {len} bytes at {address:#x} exceeds the {limit}-byte safety limit \
         (use a smaller read or an explicit, documented limit)"
    )]
    ReadLimit {
        address: u64,
        len: usize,
        limit: usize,
    },

    #[error("short read at {address:#x}: requested {requested} bytes, received {received}")]
    ShortRead {
        address: u64,
        requested: usize,
        received: usize,
    },

    #[error("invalid address {input:?}: {reason}")]
    InvalidAddress { input: String, reason: String },

    #[error("invalid hexadecimal byte pattern: {0}")]
    InvalidPattern(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("could not parse {what}: {detail}")]
    Parse { what: String, detail: String },

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    /// The requested capability is understood, but the evidence required to
    /// implement it safely has not been collected yet.
    #[error("insufficient evidence: {0}")]
    InsufficientEvidence(String),

    #[error("{context}: {source}")]
    IoContext {
        context: String,
        #[source]
        source: io::Error,
    },

    #[error(transparent)]
    Io(#[from] io::Error),
}

impl Error {
    pub fn io_context(context: impl Into<String>, source: io::Error) -> Self {
        Self::IoContext {
            context: context.into(),
            source,
        }
    }

    pub fn parse(what: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::Parse {
            what: what.into(),
            detail: detail.into(),
        }
    }

    /// True when retrying with the same inputs cannot succeed.
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Self::PermissionDenied { .. })
    }

    pub fn is_insufficient_evidence(&self) -> bool {
        matches!(self, Self::InsufficientEvidence(_))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
