//! Shared, dependency-light primitives for the FC 27 Open Career Editor.
//!
//! This crate deliberately contains no platform or game knowledge. It provides
//! the error type used across the workspace, bounded byte-pattern matching and
//! small formatting helpers.

pub mod error;
pub mod hex;
pub mod pattern;
pub mod units;

pub use error::{Error, Result};
pub use pattern::Pattern;
