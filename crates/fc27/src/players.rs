//! Player-capability report and remaining evidence gates.
//!
//! Numeric player enumeration and player-name resolution are implemented in
//! [`crate::player_store`] and [`crate::name_pool`], and exposed by
//! `fc27le players <pid> --metadata FILE`. This module documents the pieces
//! that are still blocked: authoritative metadata extraction, team/competition
//! names, and a career-state check.

use serde::Serialize;

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
            why: "Player enumeration needs the metadata name -> 4CC map. A caller-supplied FC 27 \
                  descriptor resolves `players` to `CZUM` and validates 152/152 field widths, but \
                  the game's own data/db/fifa_ng_db-meta.xml is inside encrypted/compressed CFS. \
                  The CLI therefore requires --metadata FILE and never bundles the XML."
                .to_string(),
            how_to_collect: "Reverse the CFS codec/key for the metadata entry (contentsb.toc, \
                             offset 0x7E0B8A, size 0x158CB2 in the tested build) or document a \
                             reproducible read-only extraction path. Do not redistribute the XML."
                .to_string(),
        },
        MissingEvidence {
            requirement: "Team and competition names".to_string(),
            why: "Player names are resolved through `playernames` (Huffman), `dcplayernames` \
                  (inline 46000+ IDs) and `editedplayernames` (per-player overrides). Team, \
                  league and competition names use different tables/string fields and are not \
                  yet exposed by the CLI."
                .to_string(),
            how_to_collect: "Resolve the relevant table/field 4CCs from the metadata, identify \
                             whether each string is inline or pool-backed, and validate decoded \
                             names against independent competition data before emitting them."
                .to_string(),
        },
        MissingEvidence {
            requirement: "Career-mode context check".to_string(),
            why: "The table exists in the live database, but the tool does not yet verify that a \
                  career/squad context is active. A menu-mode or partially loaded database could \
                  expose stale rows."
                .to_string(),
            how_to_collect: "Locate the game-state object and the career-mode flag with the same \
                             anchor discipline used for the event dispatcher, then require it \
                             before enumeration."
                .to_string(),
        },
        MissingEvidence {
            requirement: "Build-specific revalidation".to_string(),
            why: "The verified storage model (count at +0x7C, capacity at +0x78, invalid marker \
                  at +0x7E, last-byte bit 7) was validated on one FC 27 build. Other builds must \
                  not reuse these offsets blindly."
                .to_string(),
            how_to_collect: "Gate the player reader on the known FC27.exe SHA-256 or rerun the \
                             cross-table invariant checks (count <= capacity, invalid rows == \
                             +0x7E) before emitting records."
                .to_string(),
        },
    ]
}

pub fn capability_report() -> PlayerCapabilityReport {
    PlayerCapabilityReport {
        status: "implemented_read_only",
        summary: "Player enumeration and name resolution are implemented and verified read-only. \
                  With a caller-supplied FC 27 metadata XML the CLI resolves `players` to CZUM, \
                  validates the 152-field layout, scans all 22098 slots, skips the 284 invalid \
                  markers, emits the five verified numeric fields, and resolves names through \
                  `playernames` (Huffman pool), `dcplayernames` (IDs 46000+) and \
                  `editedplayernames` overrides. A full live pass resolved 21814/21814 player \
                  names with zero errors. Missing names stay null and are reported; no name is \
                  invented.",
        known_from_research: vec![
            "Documented Live Editor Lua API: LE.db:GetTable(\"players\"), GetFirstRecord(), \
             GetNextValidRecord(), GetRecordFieldValue(record, \"playerid\"), \
             GetTeamIdFromPlayerId(playerid)."
                .to_string(),
            "The documented table reader uses descriptor +0x30 (first record), +0x44 (record \
             size), +0x7C (u16 slot count) and treats a record as invalid when the high bit of \
             its last byte is set. Source: lua/libs/v2/imports/t3db/table.lua."
                .to_string(),
            "The documented field reader extracts an LSB-first bit slice and adds the metadata \
             low bound. Source: lua/libs/v2/imports/t3db/field.lua."
                .to_string(),
            "Independent static analysis of the database iteration logic confirms the same \
             bound and invalid-marker predicate."
                .to_string(),
            "WP1 verified the runtime DB path: pattern RVA 0x7242D5E, registry key 0x0AE932D0, \
             two databases (g_db, a_mx), 273 table descriptors. Run `fc27le db <pid>`."
                .to_string(),
            "Metadata validation (caller-supplied FC 27 XML): 268/268 tables matched live \
             descriptors and 268/268 field-width sets matched; `players` = CZUM, 152 fields, \
             156-byte records, 1247-bit mask, 4 padding bits."
                .to_string(),
            "WP4 live cross-check: 232 non-empty tables satisfy slot_count <= capacity and \
             invalid rows == u16 at +0x7E. For CZUM: 22098 slots, 27000 capacity, 284 invalid, \
             21814 valid (equal to the playerid-index count at index-object +0x20)."
                .to_string(),
            "Famous-player spot check after applying metadata biases: 20801 = 187 cm / 85 kg / \
             ST; 200389 and 212831 decode as goalkeepers; 215914 = 168 cm / CDM."
                .to_string(),
            "WP5 name model: `playernames` rows carry a u32 pool offset at +0x00. The pool at \
             descriptor +0x38 begins with a 138-node breadth-first Huffman tree (4 bytes per \
             node; leaves are UTF-8 bytes shifted left 8), followed by length-prefixed, \
             MSB-first bitstreams. The first offset (0x228) equals the tree size."
                .to_string(),
            "`dcplayernames` stores IDs 46000+ as 13-bit keys (stored value + 46000) plus a \
             38-byte inline UTF-8 string. `editedplayernames` stores per-player 45-byte inline \
             first/surname/common overrides. In the live capture: 43066 playernames, 2279 \
             dcplayernames and 8 edited overrides."
                .to_string(),
            "Live name validation: all 43065 playernames entries decoded; 21814/21814 valid \
             players resolved with zero errors. Spot checks: 20801 Cristiano Ronaldo \
             (first C. Ronaldo, last dos Santos Aveiro, common Cristiano Ronaldo), 158023 Lionel \
             Messi, 212831 Alisson (common Alisson), 215914 N'Golo Kanté, 231747 Kylian Mbappé."
                .to_string(),
        ],
        missing_evidence: missing_evidence(),
        policy: "Do not fabricate offsets, pointer chains, names or record layouts. Read only the \
                 verified ranges, skip invalid slots, require two identical snapshots, decode \
                 names with strict bounds and UTF-8 checks, and leave unresolved names null with \
                 an explicit error. Stop at the evidence gate when a structural check fails.",
    }
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
    fn report_claims_verified_names() {
        let report = capability_report();
        assert_eq!(report.status, "implemented_read_only");
        assert!(report.summary.contains("name resolution"));
        assert!(report.summary.contains("21814/21814"));
        assert!(report.summary.contains("no name is invented"));
    }
}
