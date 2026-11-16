//! Parser and live-layout validator for EA FC database metadata XML.
//!
//! The metadata file is proprietary game data and is **not** bundled in this
//! repository. The CLI accepts a caller-supplied file, parses it here, and
//! validates the resulting name -> 4CC mappings against the live table
//! descriptors. This module contains only format logic and synthetic tests.

use std::collections::BTreeMap;
use std::path::Path;

use fc27_core::{Error, Result};
use fc27_process::ProcessHandle;
use roxmltree::{Document, Node};
use serde::Serialize;

use crate::build_identity::sha256_file;
use crate::db_service::{DbField, DbTable, read_table_fields};

#[derive(Debug, Clone, Serialize)]
pub struct MetadataField {
    pub name: String,
    pub shortname: String,
    pub type_name: String,
    pub depth: u16,
    pub nullable: bool,
    pub is_key: bool,
    pub range_high: Option<i64>,
    pub range_low: Option<i64>,
    pub foreign_table: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataTable {
    pub name: String,
    pub shortname: String,
    pub savegroups: Vec<String>,
    pub fields: Vec<MetadataField>,
    /// True for the compact embedded profile: only the listed fields are
    /// validated, while the live table may contain additional fields.
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub database_name: String,
    pub database_shortname: String,
    pub tables: Vec<MetadataTable>,
}

#[derive(Debug, Clone)]
pub struct LoadedMetadata {
    pub parsed: Metadata,
    pub size_bytes: u64,
    pub sha256: String,
}

/// Read, parse and hash a caller-supplied metadata XML with a 16 MiB safety cap.
///
/// The file is never copied into the repository. This is shared by the CLI,
/// GUI and Lua compatibility runtime so all three see the same parsed data.
pub fn load_metadata_file(path: &Path) -> Result<LoadedMetadata> {
    let size_bytes = std::fs::metadata(path)
        .map_err(|error| Error::io_context(format!("stat {}", path.display()), error))?
        .len();
    if size_bytes > 16 * 1024 * 1024 {
        return Err(Error::InvalidInput(format!(
            "metadata file {} is larger than the 16 MiB safety cap",
            path.display()
        )));
    }
    let xml = std::fs::read_to_string(path)
        .map_err(|error| Error::io_context(format!("read {}", path.display()), error))?;
    let parsed = parse_metadata_xml(&xml)?;
    let sha256 = sha256_file(path)?;
    Ok(LoadedMetadata {
        parsed,
        size_bytes,
        sha256,
    })
}

impl Metadata {
    pub fn table_by_name(&self, name: &str) -> Option<&MetadataTable> {
        self.tables.iter().find(|table| table.name == name)
    }

    pub fn table_by_shortname(&self, shortname: &str) -> Option<&MetadataTable> {
        self.tables
            .iter()
            .find(|table| table.shortname == shortname)
    }
}

/// Parse the `<database>` / `<table>` / `<fields>` / `<field>` descriptor.
///
/// Names and shortnames are required; a shortname must be four ASCII
/// characters. Malformed descriptors fail closed instead of being guessed.
pub fn parse_metadata_xml(xml: &str) -> Result<Metadata> {
    let document = Document::parse(xml)
        .map_err(|error| Error::parse("database metadata XML", error.to_string()))?;
    let root = document.root_element();
    if root.tag_name().name() != "database" {
        return Err(Error::parse(
            "database metadata XML",
            format!(
                "root element is <{}>, expected <database>",
                root.tag_name().name()
            ),
        ));
    }

    let database_name = required_attr(root, "name")?;
    let database_shortname = required_attr(root, "shortname")?;
    validate_shortname(&database_shortname, "database")?;

    let mut tables = Vec::new();
    for table_node in root
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "table")
    {
        let name = required_attr(table_node, "name")?;
        let shortname = required_attr(table_node, "shortname")?;
        validate_shortname(&shortname, &format!("table {name}"))?;

        let savegroups = table_node
            .attribute("savegroups")
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let mut fields = Vec::new();
        for fields_node in table_node
            .children()
            .filter(|node| node.is_element() && node.tag_name().name() == "fields")
        {
            for field_node in fields_node
                .children()
                .filter(|node| node.is_element() && node.tag_name().name() == "field")
            {
                fields.push(parse_field(field_node, &name)?);
            }
        }

        tables.push(MetadataTable {
            name,
            shortname,
            savegroups,
            fields,
            partial: false,
        });
    }

    if tables.is_empty() {
        return Err(Error::parse(
            "database metadata XML",
            "no <table> descriptors found",
        ));
    }

    Ok(Metadata {
        database_name,
        database_shortname,
        tables,
    })
}

fn parse_field(node: Node<'_, '_>, table_name: &str) -> Result<MetadataField> {
    let name = required_attr(node, "name")?;
    let shortname = required_attr(node, "shortname")?;
    validate_shortname(&shortname, &format!("field {table_name}.{name}"))?;
    let type_name = required_attr(node, "type")?;
    let depth = node
        .attribute("depth")
        .ok_or_else(|| {
            Error::parse(
                "database metadata XML",
                format!("field {table_name}.{name} has no depth"),
            )
        })?
        .parse::<u16>()
        .map_err(|error| {
            Error::parse(
                "database metadata XML",
                format!("field {table_name}.{name} depth: {error}"),
            )
        })?;

    let range_high = optional_i64(node, "rangehigh")?;
    let range_low = optional_i64(node, "rangelow")?;

    Ok(MetadataField {
        name,
        shortname,
        type_name,
        depth,
        nullable: node.attribute("null") == Some("True"),
        is_key: node.attribute("key") == Some("True"),
        range_high,
        range_low,
        foreign_table: node.attribute("fkparenttable").map(str::to_string),
    })
}

fn required_attr(node: Node<'_, '_>, name: &str) -> Result<String> {
    node.attribute(name).map(str::to_string).ok_or_else(|| {
        Error::parse(
            "database metadata XML",
            format!("<{}> is missing required {name:?}", node.tag_name().name()),
        )
    })
}

fn optional_i64(node: Node<'_, '_>, name: &str) -> Result<Option<i64>> {
    match node.attribute(name) {
        None => Ok(None),
        Some(value) => value.parse::<i64>().map(Some).map_err(|error| {
            Error::parse(
                "database metadata XML",
                format!("<{}> {name}={value:?}: {error}", node.tag_name().name()),
            )
        }),
    }
}

fn validate_shortname(shortname: &str, what: &str) -> Result<()> {
    if shortname.len() != 4 || !shortname.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(Error::parse(
            "database metadata XML",
            format!("{what} shortname {shortname:?} is not exactly four ASCII graphics"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldMismatch {
    pub name: String,
    pub shortname: String,
    pub metadata_depth: u16,
    pub live_width: Option<u32>,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableValidation {
    pub name: String,
    pub shortname: String,
    pub metadata_field_count: usize,
    pub live_address: Option<u64>,
    pub live_field_count: Option<usize>,
    pub data_pointer: Option<u64>,
    pub row_size: Option<u32>,
    pub row_bits: Option<u32>,
    pub sum_depth: u32,
    pub padding_bits: Option<i64>,
    pub width_matches: usize,
    pub width_mismatch_count: usize,
    pub width_mismatches: Vec<FieldMismatch>,
    pub missing_live_fields: Vec<String>,
    pub extra_live_fields: Vec<String>,
    pub field_count_ok: bool,
    pub row_layout_ok: bool,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataValidation {
    pub metadata_database: String,
    pub metadata_table_count: usize,
    pub live_table_count: usize,
    pub matched_tables: usize,
    pub verified_tables: usize,
    pub missing_tables: Vec<String>,
    pub field_count_mismatches: Vec<String>,
    pub width_mismatch_tables: Vec<String>,
    pub focus_table: String,
    pub focus: Option<TableValidation>,
    pub all_tables_verified: bool,
    pub focus_verified: bool,
    pub notes: Vec<String>,
}

/// Validate every metadata table against the live shortname/field layout and
/// return detailed results for one focus table (normally `players`).
pub fn validate(
    metadata: &Metadata,
    live_tables: &[DbTable],
    handle: &dyn ProcessHandle,
    focus_table: &str,
) -> Result<MetadataValidation> {
    let live_by_short: BTreeMap<&str, &DbTable> = live_tables
        .iter()
        .map(|table| (table.shortname.as_str(), table))
        .collect();

    let mut matched_tables = 0;
    let mut verified_tables = 0;
    let mut missing_tables = Vec::new();
    let mut field_count_mismatches = Vec::new();
    let mut width_mismatch_tables = Vec::new();
    let mut focus = None;

    for table in &metadata.tables {
        let Some(live) = live_by_short.get(table.shortname.as_str()) else {
            missing_tables.push(format!("{} ({})", table.name, table.shortname));
            continue;
        };
        matched_tables += 1;

        let live_fields = read_table_fields(handle, live)?;
        let result = compare_table(table, live, &live_fields);
        if result.field_count_ok && result.width_mismatch_count == 0 && result.row_layout_ok {
            verified_tables += 1;
        }
        if !result.field_count_ok {
            field_count_mismatches.push(format!(
                "{} ({}) metadata={} live={}",
                table.name,
                table.shortname,
                result.metadata_field_count,
                result.live_field_count.unwrap_or(0)
            ));
        }
        if result.width_mismatch_count > 0 {
            width_mismatch_tables.push(format!(
                "{} ({}): {}",
                table.name, table.shortname, result.width_mismatch_count
            ));
        }
        if table.name == focus_table {
            focus = Some(result);
        }
    }

    let focus_verified = focus.as_ref().is_some_and(|table| table.verified);
    let all_tables_verified = matched_tables == metadata.tables.len()
        && verified_tables == metadata.tables.len()
        && missing_tables.is_empty()
        && field_count_mismatches.is_empty()
        && width_mismatch_tables.is_empty();

    let mut notes = vec![
        "The metadata file is caller-supplied and is not bundled with this project.".to_string(),
        "The game's own copy lives inside encrypted/compressed CFS; extracting it is a separate \
         dependency."
            .to_string(),
    ];
    if let Some(table) = &focus {
        notes.push(format!(
            "Focus table {} ({}) resolved to live address {}; verified={}.",
            table.name,
            table.shortname,
            table
                .live_address
                .map(|address| format!("{address:#x}"))
                .unwrap_or_else(|| "missing".to_string()),
            table.verified
        ));
    }

    Ok(MetadataValidation {
        metadata_database: metadata.database_name.clone(),
        metadata_table_count: metadata.tables.len(),
        live_table_count: live_tables.len(),
        matched_tables,
        verified_tables,
        missing_tables,
        field_count_mismatches,
        width_mismatch_tables,
        focus_table: focus_table.to_string(),
        focus,
        all_tables_verified,
        focus_verified,
        notes,
    })
}

/// Pure comparison for one table, usable in tests without a live process.
pub fn compare_table(
    metadata: &MetadataTable,
    live: &DbTable,
    live_fields: &[DbField],
) -> TableValidation {
    let live_by_short: BTreeMap<&str, &DbField> = live_fields
        .iter()
        .map(|field| (field.shortname.as_str(), field))
        .collect();
    let metadata_shortnames: BTreeMap<&str, &MetadataField> = metadata
        .fields
        .iter()
        .map(|field| (field.shortname.as_str(), field))
        .collect();

    let mut width_matches = 0;
    let mut width_mismatches = Vec::new();
    let mut missing_live_fields = Vec::new();

    for field in &metadata.fields {
        match live_by_short.get(field.shortname.as_str()) {
            Some(live_field) if live_field.bit_width == u32::from(field.depth) => {
                width_matches += 1;
            }
            Some(live_field) => {
                if width_mismatches.len() < 32 {
                    width_mismatches.push(FieldMismatch {
                        name: field.name.clone(),
                        shortname: field.shortname.clone(),
                        metadata_depth: field.depth,
                        live_width: Some(live_field.bit_width),
                        kind: "width",
                    });
                }
            }
            None => missing_live_fields.push(format!("{}.{}", field.name, field.shortname)),
        }
    }

    let extra_live_fields: Vec<String> = live_fields
        .iter()
        .filter(|field| !metadata_shortnames.contains_key(field.shortname.as_str()))
        .map(|field| field.shortname.clone())
        .collect();
    let sum_depth: u32 = metadata
        .fields
        .iter()
        .map(|field| u32::from(field.depth))
        .sum();
    let row_bits = live.header_44.checked_mul(8);
    let padding_bits = if metadata.partial {
        None
    } else {
        row_bits.map(|bits| i64::from(bits) - i64::from(sum_depth))
    };
    let field_count_ok = metadata.partial || metadata.fields.len() == live_fields.len();
    let row_layout_ok = if metadata.partial {
        // A partial profile lists only the fields needed by the player/name
        // reader. Every listed field is still bounds-checked above.
        true
    } else {
        padding_bits.is_some_and(|padding| (0..8).contains(&padding))
    };
    let field_widths_ok = width_mismatches.is_empty()
        && missing_live_fields.is_empty()
        && (metadata.partial || extra_live_fields.is_empty());
    let verified = field_count_ok && field_widths_ok && row_layout_ok && live.data_pointer != 0;

    TableValidation {
        name: metadata.name.clone(),
        shortname: metadata.shortname.clone(),
        metadata_field_count: metadata.fields.len(),
        live_address: Some(live.address),
        live_field_count: Some(live_fields.len()),
        data_pointer: Some(live.data_pointer),
        row_size: Some(live.header_44),
        row_bits,
        sum_depth,
        padding_bits,
        width_matches,
        width_mismatch_count: width_mismatches.len()
            + if live_fields.len() < metadata.fields.len() {
                metadata.fields.len() - live_fields.len()
            } else {
                0
            },
        width_mismatches,
        missing_live_fields,
        extra_live_fields,
        field_count_ok,
        row_layout_ok,
        verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_XML: &str = r#"<?xml version="1.0"?>
<database name="test_db" shortname="t_db">
  <table name="players" shortname="PLAY" savegroups="squads,career">
    <fields>
      <field name="playerid" shortname="pid1" type="DBOFIELDTYPE_INTEGER" depth="16" key="True" rangehigh="500000" rangelow="0"/>
      <field name="firstnameid" shortname="fid1" type="DBOFIELDTYPE_INTEGER" depth="15" rangehigh="50000" rangelow="0"/>
    </fields>
  </table>
  <table name="teams" shortname="TEAM">
    <fields>
      <field name="teamid" shortname="tid1" type="DBOFIELDTYPE_INTEGER" depth="16" key="True"/>
    </fields>
  </table>
</database>"#;

    fn live_table() -> DbTable {
        DbTable {
            database: "t_db".to_string(),
            shortname: "PLAY".to_string(),
            shortname_hex: 0x59414c50,
            address: 0x1000,
            field_count_byte: 2,
            data_pointer: 0x2000,
            header_44: 4,
            header_48: 31,
            header_4c: 2,
            header_50: 0,
        }
    }

    #[test]
    fn parses_minimal_metadata() {
        let metadata = parse_metadata_xml(TEST_XML).unwrap();
        assert_eq!(metadata.database_name, "test_db");
        assert_eq!(metadata.tables.len(), 2);
        let players = metadata.table_by_name("players").unwrap();
        assert_eq!(players.shortname, "PLAY");
        assert_eq!(players.fields.len(), 2);
        assert_eq!(players.fields[0].shortname, "pid1");
        assert_eq!(players.fields[0].depth, 16);
        assert!(players.fields[0].is_key);
        assert_eq!(players.savegroups, vec!["squads", "career"]);
    }

    #[test]
    fn rejects_bad_shortname() {
        let xml = r#"<database name="d" shortname="db"><table name="t" shortname="TOO_LONG"><fields/></table></database>"#;
        assert!(parse_metadata_xml(xml).is_err());
    }

    #[test]
    fn validates_synthetic_layout() {
        let metadata = parse_metadata_xml(TEST_XML).unwrap();
        let players = metadata.table_by_name("players").unwrap();
        let fields = vec![
            DbField {
                bit_offset: 0,
                field_type: 3,
                bit_width: 16,
                shortname: "pid1".to_string(),
                shortname_hex: 0x31646970,
            },
            DbField {
                bit_offset: 16,
                field_type: 3,
                bit_width: 15,
                shortname: "fid1".to_string(),
                shortname_hex: 0x31646966,
            },
        ];
        let validation = compare_table(players, &live_table(), &fields);
        assert!(validation.field_count_ok);
        assert_eq!(validation.padding_bits, Some(1));
        assert!(validation.verified);
    }

    #[test]
    fn detects_width_mismatch() {
        let metadata = parse_metadata_xml(TEST_XML).unwrap();
        let players = metadata.table_by_name("players").unwrap();
        let fields = vec![
            DbField {
                bit_offset: 0,
                field_type: 3,
                bit_width: 15,
                shortname: "pid1".to_string(),
                shortname_hex: 0x31646970,
            },
            DbField {
                bit_offset: 16,
                field_type: 3,
                bit_width: 15,
                shortname: "fid1".to_string(),
                shortname_hex: 0x31646966,
            },
        ];
        let validation = compare_table(players, &live_table(), &fields);
        assert!(!validation.verified);
        assert_eq!(validation.width_mismatch_count, 1);
    }
}
