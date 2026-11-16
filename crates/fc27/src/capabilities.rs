//! Capability matrix derived from the existing research and this project's
//! verified implementation.
//!
//! No row claims a capability that has not been demonstrated. "Blocked on
//! evidence" means the capability is understood but the required game-side
//! layout has not been verified.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Implemented and exercised in this workspace.
    Implemented,
    /// Implemented as a read-only verifier or diagnostic.
    ImplementedReadOnly,
    /// Useful behaviour can be reproduced, but game data is not yet resolved.
    BlockedOnEvidence,
    /// Researched only; not part of this project's scope.
    ResearchOnly,
    /// Deliberately excluded by project policy.
    OutOfScope,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub id: &'static str,
    pub name: &'static str,
    pub status: CapabilityStatus,
    pub evidence: &'static str,
    pub note: &'static str,
}

pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: "process-discovery",
        name: "Discover FC 27 under Wine/Proton",
        status: CapabilityStatus::Implemented,
        evidence: "prior research project/reports/discovery.md; runtime_evidence.md; local Phase 0/3",
        note: "Score-based /proc discovery with explicit reasons; verified against a live process.",
    },
    Capability {
        id: "module-enumeration",
        name: "Enumerate PE/ELF modules and PE sections",
        status: CapabilityStatus::Implemented,
        evidence: "local Phase 2 implementation; Wine mapping behaviour observed in Phase 0",
        note: "Wine maps PE images as rwxp with anonymous gaps; SizeOfImage is used to bound modules.",
    },
    Capability {
        id: "bounded-memory-read",
        name: "Bounded, read-only process memory reads",
        status: CapabilityStatus::Implemented,
        evidence: "local Phase 2 implementation + controlled selftest",
        note: "1 MiB per-read cap, mapping validation, no writes and no ptrace attach.",
    },
    Capability {
        id: "build-identity",
        name: "Identify the game build by size, SHA-256 and PE metadata",
        status: CapabilityStatus::Implemented,
        evidence: "local Phase 3; reports/runtime_evidence.md build hash",
        note: "Hash matches identify a file, not a structure layout.",
    },
    Capability {
        id: "verified-pattern-scan",
        name: "Verify research anchors against live memory",
        status: CapabilityStatus::ImplementedReadOnly,
        evidence: "press_any_key_analysis.md §2.4; proprietary editor DLL internal function; local live verification",
        note: "Two anchors: the event dispatcher and the database service-object initializer.",
    },
    Capability {
        id: "db-service-verification",
        name: "Verify the game database service path and table manager",
        status: CapabilityStatus::ImplementedReadOnly,
        evidence: "proprietary editor DLL internal function / internal function / internal function; live `fc27le db` checks",
        note: "Reaches the 273-table manager and two databases by pointer traversal; no game function is called.",
    },
    Capability {
        id: "db-metadata-parser",
        name: "Parse EA FC database metadata XML (caller-supplied)",
        status: CapabilityStatus::ImplementedReadOnly,
        evidence: "crates/fc27/src/db_metadata.rs; live `fc27le db --metadata` validation",
        note: "Parses table/field names and 4CC shortnames, validates against live descriptors. \
               The proprietary XML is not bundled.",
    },
    Capability {
        id: "player-listing",
        name: "Read verified player records and names from process memory",
        status: CapabilityStatus::ImplementedReadOnly,
        evidence: "crates/fc27/src/player_store.rs + name_pool.rs; lua/.../t3db/table.lua + field.lua; \
                   proprietary editor DLL internal function/internal function; live `fc27le players --metadata`",
        note: "Resolves players = CZUM, validates count/capacity/invalid markers, requires two \
               identical snapshots, and emits numeric fields plus player names. Names come from \
               playernames (Huffman), dcplayernames (IDs 46000+) and editedplayernames overrides; \
               missing names stay null and are reported.",
    },
    Capability {
        id: "player-name-resolution",
        name: "Decode the compressed player-name pools",
        status: CapabilityStatus::ImplementedReadOnly,
        evidence: "crates/fc27/src/name_pool.rs; docs/research/name_storage.md; live 21814/21814 \
                   player-name validation",
        note: "Shared 138-node Huffman tree, length-prefixed MSB-first bitstreams, UTF-8 output, \
               inline 46000+ names and per-player edited overrides. No name is guessed.",
    },
    Capability {
        id: "team-and-competition-names",
        name: "Resolve team, league and competition names",
        status: CapabilityStatus::BlockedOnEvidence,
        evidence: "lua/DOC.MD GetTeamName / GetCompetitionNameByID; FC27.exe schema strings",
        note: "Player names are resolved, but team/league/competition names use different \
               tables/string fields and are not yet exposed.",
    },
    Capability {
        id: "career-save-parsing",
        name: "Parse career save files directly",
        status: CapabilityStatus::ResearchOnly,
        evidence: "no prior report covers the save container",
        note: "Would require new research; not assumed to match any FIFA-era format.",
    },
    Capability {
        id: "memory-writes",
        name: "Modify player values in memory",
        status: CapabilityStatus::OutOfScope,
        evidence: "task policy",
        note: "The project is read-only. No write primitive exists in the workspace.",
    },
    Capability {
        id: "dll-injection-and-hooks",
        name: "DLL injection, detours or runtime hooks",
        status: CapabilityStatus::OutOfScope,
        evidence: "task policy; hook_dependency_audit.md",
        note: "Excluded. The research audit already warns that the hook bundle is not proven safe.",
    },
    Capability {
        id: "overlay-ui",
        name: "In-game overlay / ImGui UI",
        status: CapabilityStatus::OutOfScope,
        evidence: "discovery.md (ImGui launcher/DLL responsibilities)",
        note: "A future UI would be external and read-only, not an injected overlay.",
    },
    Capability {
        id: "live-editor-binary-patching",
        name: "Patch proprietary editor DLL or Launcher.exe",
        status: CapabilityStatus::OutOfScope,
        evidence: "compatibility_fix.md; verification.md",
        note: "Previous-project artifacts are research references only; this project does not modify them.",
    },
    Capability {
        id: "lua-scripting",
        name: "Reimplement the Live Editor Lua API",
        status: CapabilityStatus::OutOfScope,
        evidence: "lua/DOC.MD",
        note: "A scripting layer is only meaningful after a verified data model exists.",
    },
    Capability {
        id: "windows-backend",
        name: "Windows process backend",
        status: CapabilityStatus::ResearchOnly,
        evidence: "fc27-platform-windows stub",
        note: "Boundary exists; no Windows build has been compiled or tested.",
    },
];

pub fn capability(id: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().find(|capability| capability.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids = std::collections::BTreeSet::new();
        for capability in CAPABILITIES {
            assert!(ids.insert(capability.id));
        }
    }

    #[test]
    fn player_listing_is_read_only() {
        assert_eq!(
            capability("player-listing").unwrap().status,
            CapabilityStatus::ImplementedReadOnly
        );
        assert_eq!(
            capability("player-name-resolution").unwrap().status,
            CapabilityStatus::ImplementedReadOnly
        );
    }

    #[test]
    fn writes_are_out_of_scope() {
        assert_eq!(
            capability("memory-writes").unwrap().status,
            CapabilityStatus::OutOfScope
        );
    }
}
