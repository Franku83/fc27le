//! Verified byte-pattern anchors.
//!
//! Every anchor must have a written source and a reproducible expectation. The
//! only anchor currently included is the game-event dispatcher from
//! `press_any_key_analysis.md`, whose pattern was independently re-verified
//! against the local `FC27.exe` (unique hit at RVA `0x1CD3B5C`, file offset
//! `0x1CD315C`, VA `0x141CD3B5C`).

use fc27_core::{Error, Pattern, Result};
use fc27_process::{ModuleInfo, ProcessHandle};
use serde::Serialize;

use crate::profile::GAME_EXE;

#[derive(Debug, Clone, Serialize)]
pub struct Anchor {
    pub id: &'static str,
    pub description: &'static str,
    pub module: &'static str,
    pub pattern: &'static str,
    pub expected_rva: u64,
    pub source: &'static str,
    pub confidence: &'static str,
    /// Per-anchor safety cap for the bytes scanned in executable sections.
    pub max_scan_bytes: u64,
}

pub const EVENT_DISPATCHER: Anchor = Anchor {
    id: "fc27.event-dispatcher",
    description: "Game event dispatcher associated with menu-state flags",
    module: GAME_EXE,
    // 36 bytes, wildcards as published in reports/press_any_key_analysis.md §2.4.
    pattern: "48 89 5C 24 ?? 48 89 74 24 ?? 57 48 83 EC 30 48 8B F1 49 8B D8 48 8D 0D ?? ?? ?? ?? \
              8B FA E8 ?? ?? ?? ?? 48 8B 06",
    expected_rva: 0x1CD3B5C,
    source: "independent pattern analysis; re-verified locally 2026-11-15",
    confidence: "high",
    max_scan_bytes: 64 * 1024 * 1024,
};

/// The game service-object initializer used to locate the game service registry.
///
/// The pattern is unique in the tested `FC27.exe` and was verified live on
/// 2026-11-16 at RVA `0x7242D5E`. The code around it references a global
/// service object; §`db_service` validates that chain read-only.
pub const DB_SERVICE_GLOBAL: Anchor = Anchor {
    id: "fc27.db-service-global",
    description: "Service-object initializer used to reach the game table manager",
    module: GAME_EXE,
    pattern: "C7 83 60 11 00 00 02 00 00 00 48 8D 0D ?? ?? ?? ?? \
              48 8B 05 ?? ?? ?? ?? BA E3 BB BD 0E FF 50 38",
    expected_rva: 0x7242D5E,
    source: "independent pattern analysis; static unique hit at file offset 0x724235e; \
             live verified 2026-11-16",
    confidence: "high",
    max_scan_bytes: 160 * 1024 * 1024,
};

pub const ANCHORS: &[Anchor] = &[EVENT_DISPATCHER, DB_SERVICE_GLOBAL];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Mismatch,
    PatternMissing,
    ModuleMissing,
    ScanTruncated,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnchorVerification {
    pub id: &'static str,
    pub description: &'static str,
    pub source: &'static str,
    pub status: VerificationStatus,
    pub module: &'static str,
    pub module_base: Option<u64>,
    pub expected_rva: Option<u64>,
    pub expected_va: Option<u64>,
    pub match_count: usize,
    pub match_rvas: Vec<u64>,
    pub match_vas: Vec<u64>,
    pub scanned_bytes: u64,
    pub truncated: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub max_bytes: u64,
    pub max_matches: usize,
    pub executable_sections_only: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_matches: 16,
            executable_sections_only: true,
        }
    }
}

const CHUNK_BYTES: usize = 1 << 20;

pub fn verify_anchor(
    handle: &dyn ProcessHandle,
    anchor: &Anchor,
    options: ScanOptions,
) -> AnchorVerification {
    match verify_anchor_inner(handle, anchor, options) {
        Ok(verification) => verification,
        Err(error) => AnchorVerification {
            id: anchor.id,
            description: anchor.description,
            source: anchor.source,
            status: VerificationStatus::Error,
            module: anchor.module,
            module_base: None,
            expected_rva: Some(anchor.expected_rva),
            expected_va: None,
            match_count: 0,
            match_rvas: Vec::new(),
            match_vas: Vec::new(),
            scanned_bytes: 0,
            truncated: false,
            message: error.to_string(),
        },
    }
}

fn verify_anchor_inner(
    handle: &dyn ProcessHandle,
    anchor: &Anchor,
    options: ScanOptions,
) -> Result<AnchorVerification> {
    let pattern = Pattern::parse(anchor.pattern)?;
    if pattern.is_empty() {
        return Err(Error::InvalidPattern("anchor pattern is empty".to_string()));
    }
    let options = ScanOptions {
        max_bytes: options.max_bytes.min(anchor.max_scan_bytes),
        ..options
    };

    let modules = handle.modules()?;
    let Some(module) = modules
        .iter()
        .find(|module| module.name.eq_ignore_ascii_case(anchor.module))
    else {
        return Ok(AnchorVerification {
            id: anchor.id,
            description: anchor.description,
            source: anchor.source,
            status: VerificationStatus::ModuleMissing,
            module: anchor.module,
            module_base: None,
            expected_rva: Some(anchor.expected_rva),
            expected_va: None,
            match_count: 0,
            match_rvas: Vec::new(),
            match_vas: Vec::new(),
            scanned_bytes: 0,
            truncated: false,
            message: format!("module {} is not loaded", anchor.module),
        });
    };

    let (match_rvas, scanned_bytes, truncated) = scan_module(handle, module, &pattern, options)?;
    let expected_va = module.address_of_rva(anchor.expected_rva);
    let match_vas = match_rvas
        .iter()
        .map(|rva| module.base.saturating_add(*rva))
        .collect::<Vec<_>>();

    let status = if match_rvas.contains(&anchor.expected_rva) {
        VerificationStatus::Verified
    } else if match_rvas.is_empty() && truncated {
        VerificationStatus::ScanTruncated
    } else if match_rvas.is_empty() {
        VerificationStatus::PatternMissing
    } else {
        VerificationStatus::Mismatch
    };

    let message = match status {
        VerificationStatus::Verified => format!(
            "expected location confirmed at RVA {:#x} (VA {:#x}); {} match(es) in the scanned range",
            anchor.expected_rva,
            module.base.saturating_add(anchor.expected_rva),
            match_rvas.len()
        ),
        VerificationStatus::Mismatch => format!(
            "pattern found at {} location(s), but not at expected RVA {:#x}",
            match_rvas.len(),
            anchor.expected_rva
        ),
        VerificationStatus::PatternMissing => format!(
            "pattern not found in the first {} bytes of executable sections",
            scanned_bytes
        ),
        VerificationStatus::ScanTruncated => format!(
            "scan cap of {} bytes reached before the pattern was found; increase max_bytes",
            options.max_bytes
        ),
        _ => String::new(),
    };

    Ok(AnchorVerification {
        id: anchor.id,
        description: anchor.description,
        source: anchor.source,
        status,
        module: anchor.module,
        module_base: Some(module.base),
        expected_rva: Some(anchor.expected_rva),
        expected_va,
        match_count: match_rvas.len(),
        match_rvas,
        match_vas,
        scanned_bytes,
        truncated,
        message,
    })
}

/// Scan a module's PE sections. Executable sections are selected by default;
/// scanning stops at `options.max_bytes`. Only explicitly bounded, read-only
/// reads are issued.
pub fn scan_module(
    handle: &dyn ProcessHandle,
    module: &ModuleInfo,
    pattern: &Pattern,
    options: ScanOptions,
) -> Result<(Vec<u64>, u64, bool)> {
    let sections = module.pe_sections();
    if sections.is_empty() {
        return Err(Error::Unsupported(format!(
            "module {} has no parsed PE section table",
            module.name
        )));
    }

    let mut matches = Vec::new();
    let mut scanned = 0u64;
    let mut truncated = false;

    for section in sections {
        if options.executable_sections_only && !section.executable {
            continue;
        }
        let section_size = u64::from(section.virtual_size.max(1));
        let section_rva = u64::from(section.virtual_address);
        let section_start = module
            .address_of_rva(section_rva)
            .ok_or_else(|| Error::Unsupported("section is outside the module".to_string()))?;

        let mut position = 0u64;
        let overlap = pattern.len().saturating_sub(1) as u64;
        let step = (CHUNK_BYTES as u64).saturating_sub(overlap).max(1);

        while position < section_size {
            if scanned >= options.max_bytes || matches.len() >= options.max_matches {
                truncated = position < section_size;
                break;
            }

            let want = (section_size - position).min(CHUNK_BYTES as u64) as usize;
            let remaining_budget = (options.max_bytes - scanned) as usize;
            let want = want.min(remaining_budget);
            if want < pattern.len() {
                truncated = true;
                break;
            }

            let bytes = handle.read(section_start + position, want)?;
            if bytes.len() != want {
                return Err(Error::ShortRead {
                    address: section_start + position,
                    requested: want,
                    received: bytes.len(),
                });
            }
            scanned += want as u64;

            for offset in pattern.find_all(&bytes, options.max_matches) {
                let rva = section_rva + position + offset as u64;
                if !matches.contains(&rva) && matches.len() < options.max_matches {
                    matches.push(rva);
                }
            }

            if position + want as u64 >= section_size {
                break;
            }
            position = position.saturating_add(step);
        }

        if truncated {
            break;
        }
    }

    matches.sort_unstable();
    Ok((matches, scanned, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_pattern_parses() {
        let pattern = Pattern::parse(EVENT_DISPATCHER.pattern).unwrap();
        assert_eq!(pattern.len(), 38);
        assert!(pattern.fixed_bytes() > 20);
    }

    #[test]
    fn anchors_have_unique_ids() {
        let mut ids = std::collections::BTreeSet::new();
        for anchor in ANCHORS {
            assert!(ids.insert(anchor.id), "duplicate anchor id {}", anchor.id);
            assert!(anchor.expected_rva > 0);
            assert!(!anchor.source.is_empty());
        }
    }
}
