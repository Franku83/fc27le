//! Minimal embedded schema profile for the verified FC 27 build.
//!
//! This is deliberately **not** a conversion of the proprietary metadata XML.
//! It contains only the table/field identifiers and decoding bounds required by
//! the read-only player browser, the player-name resolver and the first-party
//! player-field helpers:
//!
//! * `players`: the fields used by [`crate::player_store`] plus the fields
//!   independently verified for the player-field helpers.
//! * `playernames`, `dcplayernames`, `editedplayernames`: the fields used by
//!   [`crate::name_pool`].
//!
//! The identifiers, bit widths and field types are checked against the live
//! table descriptors on every load. The profile is accepted only for the exact
//! SHA-256 build recorded in [`PROFILE_BUILD_SHA256`]. External XML metadata
//! remains supported and is still required for full-table/Lua coverage or any
//! other build.
//!
//! Provenance: the original field set was derived from the minimum runtime
//! facts needed by the verified reader and re-checked against the live
//! descriptor layout during WP4/WP5. The player fields were added in WP10 after
//! correlating live FC 27 values with known player birthdays, nationalities,
//! preferred feet, contracts and PlayStyles, and with the FC 26 column schema
//! as an independent cross-check. Ranges are the project's safe decoding
//! bounds, not a copy of an XML document. No XML, table dump, player dump or
//! third-party file is embedded.

use crate::db_metadata::{Metadata, MetadataField, MetadataTable};

/// Stable identifier for this compact profile.
pub const PROFILE_ID: &str = "fc27le-embedded-v1";
/// Build for which this profile is accepted.
pub const PROFILE_BUILD_SHA256: &str =
    "d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9";
/// Tables represented by the compact profile.
pub const PROFILE_TABLE_COUNT: usize = 4;

const INTEGER: &str = "DBOFIELDTYPE_INTEGER";
const STRING: &str = "DBOFIELDTYPE_STRING";

fn field(
    name: &str,
    shortname: &str,
    type_name: &str,
    depth: u16,
    low: i64,
    high: i64,
) -> MetadataField {
    MetadataField {
        name: name.to_string(),
        shortname: shortname.to_string(),
        type_name: type_name.to_string(),
        depth,
        nullable: false,
        is_key: false,
        range_high: Some(high),
        range_low: Some(low),
        foreign_table: None,
    }
}

fn table(name: &str, shortname: &str, fields: Vec<MetadataField>) -> MetadataTable {
    MetadataTable {
        name: name.to_string(),
        shortname: shortname.to_string(),
        savegroups: Vec::new(),
        fields,
        partial: true,
    }
}

/// Return the compact profile used when no external XML is selected.
pub fn metadata() -> Metadata {
    let players = table(
        "players",
        "CZUM",
        vec![
            field("playerid", "ykFq", INTEGER, 19, -1, 500_000),
            field("firstnameid", "tHlO", INTEGER, 16, 0, 53_500),
            field("lastnameid", "QCfa", INTEGER, 16, 0, 53_500),
            field("commonnameid", "HDYx", INTEGER, 16, 0, 53_500),
            field("overallrating", "UERs", INTEGER, 7, 1, 99),
            field("potential", "mpuH", INTEGER, 7, 1, 99),
            field("height", "ypBQ", INTEGER, 7, 130, 220),
            field("weight", "sNZW", INTEGER, 7, 30, 150),
            // The exporter needs the same field the original script reads for
            // the `position` column. The width and 0..27 range were verified by
            // correlating the live values for known goalkeepers, defenders,
            // midfielders and forwards.
            field("preferredposition1", "wZQU", INTEGER, 5, 0, 27),
            // Verified against known players in the live FC 27 table. The
            // values are raw DB fields; see docs/player_field_coverage.md.
            field("birthdate", "WVIU", INTEGER, 20, 0, 1_048_575),
            field("gender", "EveZ", INTEGER, 1, 0, 1),
            field("nationality", "enmm", INTEGER, 12, 0, 4095),
            field("preferredfoot", "MDvm", INTEGER, 1, 0, 1),
            field("contractvaliduntil", "qvmK", INTEGER, 7, 0, 127),
            field("trait1", "juxV", INTEGER, 30, 0, 1_073_741_823),
            field("icontrait1", "OjQY", INTEGER, 30, 0, 1_073_741_823),
            // FC 27 stores goalkeeper PlayStyles in separate 17-bit fields.
            // Outfield PlayStyles use trait1/icontrait1 above.
            field("trait2", "inAr", INTEGER, 17, 0, 131_071),
            field("icontrait2", "PqtG", INTEGER, 17, 0, 131_071),
            // The live field is set for veterans at the end of the 2026/27
            // season (including Ronaldo, Modric, Neuer and Suarez); see the
            // docs/player_field_coverage.md for the live evidence.
            field("isretiring", "kvuF", INTEGER, 1, 0, 1),
            // The six face stats are position-dependent in FC: the same fields
            // hold PAC/SHO/PAS/DRI/DEF/PHY for outfielders and
            // DIV/HAN/KIC/REF/SPD/POS for goalkeepers.
            field("pacdiv", "aapy", INTEGER, 7, 0, 127),
            field("shohan", "NTFr", INTEGER, 7, 0, 127),
            field("paskic", "ceRf", INTEGER, 7, 0, 127),
            field("driref", "zNYP", INTEGER, 7, 0, 127),
            field("defspe", "aEqa", INTEGER, 7, 0, 127),
            field("phypos", "hdMV", INTEGER, 7, 0, 127),
            // Raw attributes used to reconstruct the face stats of generated
            // players. Every mapping was cross-checked against the corresponding
            // FC 26 column for known players and against the live generated
            // players, whose face-stat summary fields are zero.
            field("curve", "YFaA", INTEGER, 7, 0, 127),
            field("agility", "RRQB", INTEGER, 7, 0, 127),
            field("positioning", "XsFD", INTEGER, 7, 0, 127),
            field("standingtackle", "CsyD", INTEGER, 7, 0, 127),
            field("longpassing", "kerE", INTEGER, 7, 0, 127),
            field("penalties", "AGsE", INTEGER, 7, 0, 127),
            field("longshots", "CsBG", INTEGER, 7, 0, 127),
            field("gkdiving", "xrSG", INTEGER, 7, 0, 127),
            field("interceptions", "wWzG", INTEGER, 7, 0, 127),
            field("crossing", "wGOH", INTEGER, 7, 0, 127),
            field("gkreflexes", "eYFI", INTEGER, 7, 0, 127),
            field("finishing", "xJZL", INTEGER, 7, 0, 127),
            field("dribbling", "nEbM", INTEGER, 7, 0, 127),
            field("slidingtackle", "PhuM", INTEGER, 7, 0, 127),
            field("sprintspeed", "NrcP", INTEGER, 7, 0, 127),
            field("strength", "nmgT", INTEGER, 7, 0, 127),
            field("ballcontrol", "MgwU", INTEGER, 7, 0, 127),
            field("shotpower", "ohpV", INTEGER, 7, 0, 127),
            field("balance", "onkY", INTEGER, 7, 0, 127),
            field("vision", "ZoOK", INTEGER, 7, 0, 127),
            field("shortpassing", "vObb", INTEGER, 7, 0, 127),
            field("freekickaccuracy", "VgKc", INTEGER, 7, 0, 127),
            field("aggression", "iTce", INTEGER, 7, 0, 127),
            field("acceleration", "SPge", INTEGER, 7, 0, 127),
            field("headingaccuracy", "aReg", INTEGER, 7, 0, 127),
            field("gkhandling", "GBGj", INTEGER, 7, 0, 127),
            field("jumping", "URGo", INTEGER, 7, 0, 127),
            field("stamina", "XjDq", INTEGER, 7, 0, 127),
            field("gkpositioning", "yfhq", INTEGER, 7, 0, 127),
            field("defensiveawareness", "SJKz", INTEGER, 7, 0, 127),
            field("volleys", "Dydz", INTEGER, 7, 0, 127),
            field("gkkicking", "kqda", INTEGER, 7, 0, 127),
        ],
    );

    let playernames = table(
        "playernames",
        "BGwe",
        vec![
            field("nameid", "FuiB", INTEGER, 16, 0, 45_999),
            field("name", "vIys", STRING, 608, 0, 0),
        ],
    );

    let dcplayernames = table(
        "dcplayernames",
        "bneD",
        vec![
            field("nameid", "FuiB", INTEGER, 13, 46_000, 53_500),
            field("name", "vIys", STRING, 304, 0, 0),
        ],
    );

    let editedplayernames = table(
        "editedplayernames",
        "nQVU",
        vec![
            field("playerid", "ykFq", INTEGER, 19, -1, 500_000),
            field("firstname", "HdeP", STRING, 360, 0, 0),
            field("surname", "rREd", STRING, 360, 0, 0),
            field("commonname", "xnfZ", STRING, 360, 0, 0),
        ],
    );

    Metadata {
        database_name: "fc27le-embedded".to_string(),
        database_shortname: "g_db".to_string(),
        tables: vec![players, playernames, dcplayernames, editedplayernames],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_has_expected_tables_and_is_partial() {
        let profile = metadata();
        assert_eq!(profile.tables.len(), PROFILE_TABLE_COUNT);
        assert_eq!(profile.table_by_name("players").unwrap().shortname, "CZUM");
        assert!(profile.tables.iter().all(|table| table.partial));
        assert_eq!(profile.table_by_name("players").unwrap().fields.len(), 57);
        assert_eq!(
            profile.table_by_name("playernames").unwrap().fields.len(),
            2
        );
        assert_eq!(
            profile
                .table_by_name("editedplayernames")
                .unwrap()
                .fields
                .len(),
            4
        );
        let players = profile.table_by_name("players").unwrap();
        let expect = [
            ("playerid", "ykFq", 19, -1, 500_000),
            ("firstnameid", "tHlO", 16, 0, 53_500),
            ("lastnameid", "QCfa", 16, 0, 53_500),
            ("commonnameid", "HDYx", 16, 0, 53_500),
            ("overallrating", "UERs", 7, 1, 99),
            ("potential", "mpuH", 7, 1, 99),
            ("height", "ypBQ", 7, 130, 220),
            ("weight", "sNZW", 7, 30, 150),
            ("preferredposition1", "wZQU", 5, 0, 27),
            ("birthdate", "WVIU", 20, 0, 1_048_575),
            ("gender", "EveZ", 1, 0, 1),
            ("nationality", "enmm", 12, 0, 4095),
            ("preferredfoot", "MDvm", 1, 0, 1),
            ("contractvaliduntil", "qvmK", 7, 0, 127),
            ("trait1", "juxV", 30, 0, 1_073_741_823),
            ("icontrait1", "OjQY", 30, 0, 1_073_741_823),
            ("trait2", "inAr", 17, 0, 131_071),
            ("icontrait2", "PqtG", 17, 0, 131_071),
            ("isretiring", "kvuF", 1, 0, 1),
            ("pacdiv", "aapy", 7, 0, 127),
            ("shohan", "NTFr", 7, 0, 127),
            ("paskic", "ceRf", 7, 0, 127),
            ("driref", "zNYP", 7, 0, 127),
            ("defspe", "aEqa", 7, 0, 127),
            ("phypos", "hdMV", 7, 0, 127),
            ("curve", "YFaA", 7, 0, 127),
            ("agility", "RRQB", 7, 0, 127),
            ("positioning", "XsFD", 7, 0, 127),
            ("standingtackle", "CsyD", 7, 0, 127),
            ("longpassing", "kerE", 7, 0, 127),
            ("penalties", "AGsE", 7, 0, 127),
            ("longshots", "CsBG", 7, 0, 127),
            ("gkdiving", "xrSG", 7, 0, 127),
            ("interceptions", "wWzG", 7, 0, 127),
            ("crossing", "wGOH", 7, 0, 127),
            ("gkreflexes", "eYFI", 7, 0, 127),
            ("finishing", "xJZL", 7, 0, 127),
            ("dribbling", "nEbM", 7, 0, 127),
            ("slidingtackle", "PhuM", 7, 0, 127),
            ("sprintspeed", "NrcP", 7, 0, 127),
            ("strength", "nmgT", 7, 0, 127),
            ("ballcontrol", "MgwU", 7, 0, 127),
            ("shotpower", "ohpV", 7, 0, 127),
            ("balance", "onkY", 7, 0, 127),
            ("vision", "ZoOK", 7, 0, 127),
            ("shortpassing", "vObb", 7, 0, 127),
            ("freekickaccuracy", "VgKc", 7, 0, 127),
            ("aggression", "iTce", 7, 0, 127),
            ("acceleration", "SPge", 7, 0, 127),
            ("headingaccuracy", "aReg", 7, 0, 127),
            ("gkhandling", "GBGj", 7, 0, 127),
            ("jumping", "URGo", 7, 0, 127),
            ("stamina", "XjDq", 7, 0, 127),
            ("gkpositioning", "yfhq", 7, 0, 127),
            ("defensiveawareness", "SJKz", 7, 0, 127),
            ("volleys", "Dydz", 7, 0, 127),
            ("gkkicking", "kqda", 7, 0, 127),
        ];
        for (name, shortname, depth, low, high) in expect {
            let field = players
                .fields
                .iter()
                .find(|field| field.name == name)
                .unwrap();
            assert_eq!(field.shortname, shortname);
            assert_eq!(field.depth, depth);
            assert_eq!(field.range_low, Some(low));
            assert_eq!(field.range_high, Some(high));
        }
    }

    #[test]
    fn verified_build_hash_is_exact() {
        assert_eq!(PROFILE_BUILD_SHA256.len(), 64);
        assert!(
            PROFILE_BUILD_SHA256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }
}
