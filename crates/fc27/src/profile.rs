//! Static FC 27 profile data.
//!
//! Hashes and labels come from the existing research project and were re-checked
//! against the local installation during this project's Phase 3 reconnaissance.

use serde::Serialize;

pub const GAME_EXE: &str = "FC27.exe";
pub const EDITOR_DLL: &str = "proprietary editor DLL";
pub const PROTECTION_MODULE: &str = "activation module";
pub const EAAC_MODULE: &str = "EAAntiCheat.GameServiceLauncher.dll";
pub const THIRD_PARTY_MODULE: &str = "third-party-module.dll";

#[derive(Debug, Clone, Serialize)]
pub struct KnownBuild {
    pub sha256: &'static str,
    pub label: &'static str,
    pub size_bytes: u64,
    pub source: &'static str,
    pub notes: &'static str,
}

/// Builds that have been observed by the research project or this project.
///
/// A hash match identifies the file, not the accuracy of any structure. No
/// structure is trusted because of a build match.
pub const KNOWN_BUILDS: &[KnownBuild] = &[KnownBuild {
    sha256: "d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9",
    label: "Tested FC 27 build, 2026-11-12",
    size_bytes: 235_283_352,
    source: "prior research project/reports/runtime_evidence.md; re-verified locally 2026-11-15",
    notes: "Contains an .thirdparty PE section. The previous reports note that the other distribution file set \
            was not tested; do not generalize this hash to other distributions.",
}];

pub fn identify_build(sha256: &str) -> Option<&'static KnownBuild> {
    let needle = sha256.to_ascii_lowercase();
    KNOWN_BUILDS
        .iter()
        .find(|build| build.sha256.eq_ignore_ascii_case(&needle))
}
