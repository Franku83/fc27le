//! Minimal embedded schema profile for the verified FC 27 build.
//!
//! This is deliberately **not** a conversion of the proprietary metadata XML.
//! It contains only the table/field identifiers and decoding bounds required by
//! the read-only player browser and the player-name resolver:
//!
//! * `players`: eight fields used by [`crate::player_store`].
//! * `playernames`, `dcplayernames`, `editedplayernames`: the fields used by
//!   [`crate::name_pool`].
//!
//! The identifiers, bit widths and field types are checked against the live
//! table descriptors on every load. The profile is accepted only for the exact
//! SHA-256 build recorded in [`PROFILE_BUILD_SHA256`]. External XML metadata
//! remains supported and is still required for full-table/Lua coverage or any
//! other build.
//!
//! Provenance: the field set was derived from the minimum runtime facts needed
//! by the verified reader and re-checked against the live descriptor layout
//! during WP4/WP5. Ranges are the project's safe decoding bounds, not a copy of
//! an XML document. No XML, table dump, player dump or third-party file is
//! embedded.

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
        assert_eq!(profile.table_by_name("players").unwrap().fields.len(), 9);
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
