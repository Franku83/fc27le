//! Static FC 27 profile data.
//!
//! The hash below was observed locally and cross-checked against the public
//! research notes. A hash match identifies a file, not the correctness of any
//! memory structure.

use serde::Serialize;

pub const GAME_EXE: &str = "FC27.exe";

#[derive(Debug, Clone, Serialize)]
pub struct KnownBuild {
    pub sha256: &'static str,
    pub label: &'static str,
    pub size_bytes: u64,
    pub source: &'static str,
    pub notes: &'static str,
}

/// Builds that have been observed by this project.
///
/// A hash match identifies the file, not the accuracy of any structure. No
/// structure is trusted because of a build match.
pub const KNOWN_BUILDS: &[KnownBuild] = &[KnownBuild {
    sha256: "d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9",
    label: "Tested FC 27 build, 2026-11-12",
    size_bytes: 235_283_352,
    source: "local verification; public research cross-check",
    notes: "Hash match only. Other game builds and distributions are untested.",
}];

pub fn identify_build(sha256: &str) -> Option<&'static KnownBuild> {
    let needle = sha256.to_ascii_lowercase();
    KNOWN_BUILDS
        .iter()
        .find(|build| build.sha256.eq_ignore_ascii_case(&needle))
}
