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
            requirement: "Game-side database manager locator".to_string(),
            why: "The Lua API exposes LE.db:GetTable(\"players\"), but the previous research \
                  analysed proprietary editor DLL, not the game's database manager. No verified \
                  address, vtable, global or signature exists for it."
                .to_string(),
            how_to_collect: "Statically locate the game's DB interface using the observed SQL and \
                             schema strings in FC27.exe, then verify the resulting pattern against \
                             live memory with a read-only anchor test."
                .to_string(),
        },
        MissingEvidence {
            requirement: "Players table record layout".to_string(),
            why: "Field names are known at the API/SQL level (playerid, firstname, surname, \
                  overallrating, potential, teamid), but their in-memory sizes, order, padding and \
                  string representation are not established."
                .to_string(),
            how_to_collect:
                "Derive candidate layouts from the game's table metadata, then validate \
                             with cross-checks (monotonic record count, valid string pointers, \
                             overall/potential ranges, team id referential integrity)."
                    .to_string(),
        },
        MissingEvidence {
            requirement: "Verified record iteration primitive".to_string(),
            why:
                "The Live Editor API provides GetFirstRecord/GetNextValidRecord, but its internal \
                  record cursor belongs to the editor, not to an independently verified game \
                  structure."
                    .to_string(),
            how_to_collect:
                "Document the game's table header, record stride and validity flag from \
                             static and runtime evidence, then expose only a bounded iteration API."
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
        summary: "Read-only player listing is not implemented. The process and memory layers are \
                  verified, but no game-side player structure has been established.",
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
            "The previous project verified process/module discovery and the event-dispatcher \
             pattern, but not any player record layout."
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
