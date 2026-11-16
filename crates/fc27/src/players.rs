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
            requirement: "Full-name -> 4CC metadata map".to_string(),
            why: "The game database manager and its 273 table descriptors are verified read-only \
                  (`fc27le db`), but table/field names are stored as 4CC codes. Live Editor maps \
                  `players` and its fields to those codes from data/db/fifa_ng_db-meta.xml, \
                  loaded through CFS. No verified full-name -> 4CC map was found in the game \
                  process."
                .to_string(),
            how_to_collect: "Extract data/db/fifa_ng_db-meta.xml read-only from the game archives \
                             (hash-list entry 1160535545681411833), parse the \
                             <database>/<table name shortname>/<fields>/<field> schema, and map \
                             `players` and its field names to 4CC codes."
                .to_string(),
        },
        MissingEvidence {
            requirement: "Players table descriptor and row count".to_string(),
            why: "The table manager exposes record size at +0x44, bit mask at +0x48 \
                  (`record_size*8-1`, confirmed for all 273 tables), field count at +0x82 and \
                  16-byte field entries at +0x84. Without the name map, the `players` 4CC and \
                  its row count/data state cannot be selected."
                .to_string(),
            how_to_collect:
                "Use the XML-derived 4CC to select the descriptor with `fc27le db`, then validate \
                             record size/bit mask, field count and row data availability before \
                             reading any record."
                    .to_string(),
        },
        MissingEvidence {
            requirement: "Player field names and record layout".to_string(),
            why: "Field entries give bit offset, type, bit width and 4CC, and internal function \
                  defines the bit-packed access semantics (types 0/3/4/0xD). The mapping from \
                  player field names (playerid, firstnameid, overallrating, ...) to those 4CCs \
                  and the row layout are not established."
                .to_string(),
            how_to_collect:
                "Combine the XML field map with the descriptor field entries, then validate \
                             values against natural record variation and independent range checks."
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
        summary: "Read-only player listing is not implemented. The process, memory and database \
                  manager layers are verified (`fc27le db`), but the full-name -> 4CC map, the \
                  players descriptor and the record layout are not established.",
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
            "Table descriptors store 4CC codes and a bit-packed layout: record size at +0x44, \
             bit mask at +0x48 (record_size*8-1, confirmed for all 273 tables), field count at \
             +0x82, 16-byte field entries at +0x84. Source: docs/research/db_interface.md."
                .to_string(),
            "The full-name -> 4CC map is loaded by Live Editor from \
             data/db/fifa_ng_db-meta.xml (CFS hash-list entry 1160535545681411833); no verified \
             equivalent map was found in the game process."
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
