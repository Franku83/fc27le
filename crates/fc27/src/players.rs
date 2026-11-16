//! Evidence-gated player listing.
//!
//! The target chain is:
//! `Process Discovery -> Memory Reader -> Structure Resolver -> Player Data -> CLI`.
//!
//! Discovery, reading and build/anchor verification are implemented. The
//! structure resolver for the player table is **not** implemented because the
//! required game-side evidence does not exist in the previous research or in
//! this project's observations. Rather than inventing offsets, [`resolve`]
//! returns [`fc27_core::Error::InsufficientEvidence`] and
//! [`capability_report`] lists exactly what is missing.

use fc27_core::{Error, Result};
use fc27_process::{ModuleInfo, ProcessHandle};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Player {
    pub player_id: u32,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub common_name: Option<String>,
    pub team_id: Option<u32>,
    pub overall: Option<u16>,
    pub potential: Option<u16>,
    pub position: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingEvidence {
    pub requirement: String,
    pub why: String,
    pub how_to_collect: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerCapabilityReport {
    pub status: &'static str,
    pub summary: &'static str,
    pub known_from_research: Vec<String>,
    pub missing_evidence: Vec<MissingEvidence>,
    pub policy: &'static str,
}

pub fn missing_evidence() -> Vec<MissingEvidence> {
    vec![
        MissingEvidence {
            requirement: "Authoritative metadata extraction".to_string(),
            why: "A caller-supplied FC 27 metadata XML resolves `players` to 4CC `CZUM` and all \
                  152 field widths match the live descriptor. The game's own copy of \
                  data/db/fifa_ng_db-meta.xml is inside encrypted/compressed CFS; extracting it \
                  remains an unresolved dependency, so the CLI currently requires \
                  --metadata FILE."
                .to_string(),
            how_to_collect: "Reverse the CFS codec/key for the metadata entry (contentsb.toc, \
                             offset 0x7E0B8A, size 0x158CB2 in the tested build) or document a \
                             reproducible read-only extraction path. Do not bundle the XML."
                .to_string(),
        },
        MissingEvidence {
            requirement: "Players row count and data extent".to_string(),
            why: "The descriptor is resolved: CZUM, 152 fields, record size 156 bytes, bit mask \
                  1247, data pointer non-zero, 1244 field bits plus 4 padding bits. The table \
                  header has no verified row count/data length (+0x50 is zero), so enumeration \
                  bounds are not established."
                .to_string(),
            how_to_collect: "Locate the row count/data extent for the CZUM data buffer (or the \
                             containing DB allocation), validate it against record boundaries, \
                             and only then enable bounded enumeration."
                .to_string(),
        },
        MissingEvidence {
            requirement: "String storage model".to_string(),
            why: "Player names may be inline, pointer-based, interned or resolved through name \
                  tables. Guessing would produce plausible but false output."
                .to_string(),
            how_to_collect:
                "Identify the string allocator/table from the DB metadata and validate \
                             every resolved pointer against mapped, readable memory before use."
                    .to_string(),
        },
        MissingEvidence {
            requirement: "Career-mode context check".to_string(),
            why: "The table may exist in multiple modes or only after a career save is loaded. \
                  Without a verified state check the tool could read stale or menu-mode data."
                .to_string(),
            how_to_collect: "Locate the game-state object and the career-mode flag with the same \
                             anchor discipline used for the event dispatcher."
                .to_string(),
        },
    ]
}

pub fn capability_report() -> PlayerCapabilityReport {
    PlayerCapabilityReport {
        status: "blocked_on_evidence",
        summary: "Read-only player enumeration is not implemented. The database manager, the \
                  `players` descriptor (4CC CZUM), its 152 fields and its bit layout are verified \
                  with a caller-supplied metadata XML and live descriptor checks. Enumeration is \
                  gated on the row count/data extent and on extracting the game's own metadata \
                  from CFS.",
        known_from_research: vec![
            "Live Editor Lua API: LE.db:GetTable(\"players\"), GetFirstRecord(), \
             GetNextValidRecord(), GetRecordFieldValue(record, \"playerid\"), \
             GetTeamIdFromPlayerId(playerid). Source: lua/scripts/list_players.lua."
                .to_string(),
            "Field names used by scripts and SQL: playerid, firstname, surname, commonname, \
             overallrating, potential, teamid. Source: lua/DOC.MD, lua/scripts, FC27.exe strings."
                .to_string(),
            "A literal SQL query exists in FC27.exe at file offset 0xB20CB70 (VA 0x14B20CB70): \
             \"SELECT playerid, overallrating FROM players ORDER BY overallrating DESC\". \
             This is a string location, not a table locator."
                .to_string(),
            "WP1 verified the runtime DB path: pattern RVA 0x7242D5E, registry key 0x0AE932D0, \
             two databases (g_db, a_mx), 273 table descriptors. Run `fc27le db <pid>`."
                .to_string(),
            "Metadata validation (caller-supplied FC 27 XML): 268/268 tables matched live \
             descriptors and 268/268 field-width sets matched; 263/268 tables fully satisfy the \
             fixed-row fit check (the five exceptions are variable-string/commentary tables)."
                .to_string(),
            "Resolved players descriptor: CZUM, 152 fields, record size 156 bytes, bit mask 1247, \
             sum of field depths 1244 bits, 4 padding bits, data pointer non-zero. Source: \
             docs/research/db_metadata.md."
                .to_string(),
            "Resolved relationship table: teamplayerlinks is RrqT, 16 fields, record size 16 \
             bytes, bit mask 127. Source: metadata XML + live descriptor validation."
                .to_string(),
            "The metadata XML is loaded by Live Editor from data/db/fifa_ng_db-meta.xml (CFS \
             hash-list entry 1160535545681411833). The game's copy is inside CFS; the CLI accepts \
             a caller-supplied metadata file and does not bundle it."
                .to_string(),
        ],
        missing_evidence: missing_evidence(),
        policy: "Do not fabricate offsets, pointer chains or record layouts. Stop at the evidence \
                 gate and document what is missing.",
    }
}

/// Attempt to resolve the player table.
///
/// Always returns [`Error::InsufficientEvidence`] until the missing evidence is
/// collected. The signature exists so the future implementation has a stable
/// place to live.
pub fn resolve(_handle: &dyn ProcessHandle, _game_module: &ModuleInfo) -> Result<Vec<Player>> {
    Err(Error::InsufficientEvidence(
        "the player table resolver has no verified game-side structure. Run `players --explain` \
         to see the exact missing evidence."
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_evidence_is_actionable() {
        let items = missing_evidence();
        assert!(items.len() >= 4);
        for item in items {
            assert!(!item.requirement.is_empty());
            assert!(!item.how_to_collect.is_empty());
        }
    }

    #[test]
    fn report_does_not_claim_player_data() {
        let report = capability_report();
        assert_eq!(report.status, "blocked_on_evidence");
        assert!(report.summary.contains("not implemented"));
    }
}
