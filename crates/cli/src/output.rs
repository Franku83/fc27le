//! Output helpers. JSON goes to stdout; human text also goes to stdout; errors
//! and logs always go to stderr so `--json` pipelines stay valid.

use std::io::Write;

use fc27_core::{Error, Result};
use serde::Serialize;

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value).map_err(|error| {
        Error::io_context("serialize JSON output", std::io::Error::other(error))
    })?;
    writeln!(stdout).map_err(Error::Io)?;
    Ok(())
}

pub fn print_error(error: &Error, json: bool) {
    if json {
        let payload = serde_json::json!({
            "error": {
                "kind": error_kind(error),
                "message": error.to_string(),
            }
        });
        let _ = serde_json::to_writer_pretty(std::io::stderr(), &payload);
        let _ = writeln!(std::io::stderr());
    } else {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "error: {error}");
        let mut source = std::error::Error::source(error);
        while let Some(cause) = source {
            let _ = writeln!(stderr, "  caused by: {cause}");
            source = cause.source();
        }
    }
}

pub fn error_kind(error: &Error) -> &'static str {
    match error {
        Error::ProcessNotFound { .. } => "process_not_found",
        Error::PermissionDenied { .. } => "permission_denied",
        Error::ProcessUnavailable { .. } => "process_unavailable",
        Error::UnmappedRange { .. } => "unmapped_range",
        Error::ReadLimit { .. } => "read_limit",
        Error::ShortRead { .. } => "short_read",
        Error::InvalidAddress { .. } => "invalid_address",
        Error::InvalidPattern(_) => "invalid_pattern",
        Error::InvalidInput(_) => "invalid_input",
        Error::Parse { .. } => "parse_error",
        Error::Unsupported(_) => "unsupported",
        Error::InsufficientEvidence(_) => "insufficient_evidence",
        Error::IoContext { .. } | Error::Io(_) => "io_error",
    }
}

pub fn exit_code(error: &Error) -> u8 {
    match error {
        Error::InsufficientEvidence(_) => 3,
        Error::Unsupported(_) => 4,
        Error::PermissionDenied { .. } => 5,
        Error::ProcessNotFound { .. } => 6,
        Error::InvalidAddress { .. }
        | Error::InvalidPattern(_)
        | Error::InvalidInput(_)
        | Error::ReadLimit { .. } => 2,
        _ => 1,
    }
}

/// Parse a hexadecimal (`0x...`) or decimal address with optional underscores.
pub fn parse_u64(input: &str) -> Result<u64> {
    let cleaned = input.trim().replace('_', "");
    if cleaned.is_empty() {
        return Err(Error::InvalidAddress {
            input: input.to_string(),
            reason: "empty value".to_string(),
        });
    }
    let parsed = if let Some(hex) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        cleaned.parse::<u64>()
    };
    parsed.map_err(|error| Error::InvalidAddress {
        input: input.to_string(),
        reason: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addresses() {
        assert_eq!(parse_u64("0x140000000").unwrap(), 0x140000000);
        assert_eq!(parse_u64("4096").unwrap(), 4096);
        assert_eq!(parse_u64("0x1_0000").unwrap(), 0x10000);
        assert!(parse_u64("xyz").is_err());
        assert!(parse_u64("").is_err());
    }
}
