use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

use fc27_game::career_stats::GameDate;
use fc27_game::table_access::{FieldInfo, TableSnapshot, TableSnapshotInfo};
use fc27_lua_compat::api::{
    FixtureTableProvider, GameData, LuaEvent, ScriptJob, build_fixture_context_with_game_data,
    run_script,
};

const FIELDS: &[(&str, &str, u32)] = &[
    ("playerid", "ykFq", 19),
    ("overallrating", "UERs", 7),
    ("potential", "mpuH", 7),
    ("preferredposition1", "wZQU", 5),
    ("birthdate", "WVIU", 20),
    ("nationality", "enmm", 12),
    ("gender", "EveZ", 1),
    ("preferredfoot", "MDvm", 1),
    ("contractvaliduntil", "qvmK", 7),
    ("trait1", "juxV", 30),
    ("icontrait1", "OjQY", 30),
    ("trait2", "inAr", 17),
    ("icontrait2", "PqtG", 17),
    ("isretiring", "kvuF", 1),
    ("pacdiv", "aapy", 7),
    ("shohan", "NTFr", 7),
    ("paskic", "ceRf", 7),
    ("driref", "zNYP", 7),
    ("defspe", "aEqa", 7),
    ("phypos", "hdMV", 7),
    ("curve", "YFaA", 7),
    ("agility", "RRQB", 7),
    ("positioning", "XsFD", 7),
    ("standingtackle", "CsyD", 7),
    ("longpassing", "kerE", 7),
    ("penalties", "AGsE", 7),
    ("longshots", "CsBG", 7),
    ("gkdiving", "xrSG", 7),
    ("interceptions", "wWzG", 7),
    ("crossing", "wGOH", 7),
    ("gkreflexes", "eYFI", 7),
    ("finishing", "xJZL", 7),
    ("dribbling", "nEbM", 7),
    ("slidingtackle", "PhuM", 7),
    ("sprintspeed", "NrcP", 7),
    ("strength", "nmgT", 7),
    ("ballcontrol", "MgwU", 7),
    ("shotpower", "ohpV", 7),
    ("balance", "onkY", 7),
    ("vision", "ZoOK", 7),
    ("shortpassing", "vObb", 7),
    ("freekickaccuracy", "VgKc", 7),
    ("aggression", "iTce", 7),
    ("acceleration", "SPge", 7),
    ("headingaccuracy", "aReg", 7),
    ("gkhandling", "GBGj", 7),
    ("jumping", "URGo", 7),
    ("stamina", "XjDq", 7),
    ("gkpositioning", "yfhq", 7),
    ("defensiveawareness", "SJKz", 7),
    ("volleys", "Dydz", 7),
    ("gkkicking", "kqda", 7),
];

fn field(name: &str, shortname: &str, bit_offset: u32, bit_width: u32) -> FieldInfo {
    FieldInfo {
        name: name.to_string(),
        shortname: shortname.to_string(),
        bit_offset,
        bit_width,
        low: 0,
        high: ((1u64 << bit_width) - 1) as i64,
        field_type: 3,
        type_name: "DBOFIELDTYPE_INTEGER".to_string(),
    }
}

fn write_bits(row: &mut [u8], bit_offset: u32, bit_width: u32, value: u64) {
    for bit in 0..bit_width {
        let absolute = bit_offset + bit;
        let byte = &mut row[(absolute / 8) as usize];
        let mask = 1u8 << (absolute % 8);
        if (value >> bit) & 1 != 0 {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
    }
}

fn player(values: &[(&'static str, u64)]) -> BTreeMap<&'static str, u64> {
    values.iter().copied().collect()
}

fn synthetic_rows() -> Vec<BTreeMap<&'static str, u64>> {
    vec![
        player(&[
            ("playerid", 1001),
            ("overallrating", 80),
            ("potential", 85),
            ("preferredposition1", 25),
            ("birthdate", 141_429),
            ("nationality", 14),
            ("preferredfoot", 0),
            ("contractvaliduntil", 28),
            ("trait1", 1 | 8 | 2048),
            ("icontrait1", 33_554_432),
            ("pacdiv", 70),
            ("shohan", 65),
            ("paskic", 60),
            ("driref", 75),
            ("defspe", 40),
            ("phypos", 68),
        ]),
        player(&[
            ("playerid", 1002),
            ("overallrating", 75),
            ("potential", 80),
            ("preferredposition1", 0),
            ("birthdate", 152_385),
            ("nationality", 18),
            ("preferredfoot", 1),
            ("contractvaliduntil", 30),
            ("trait2", 1 | 16),
            ("icontrait2", 8),
            ("isretiring", 1),
            ("pacdiv", 80),
            ("shohan", 82),
            ("paskic", 75),
            ("driref", 84),
            ("defspe", 50),
            ("phypos", 81),
        ]),
        player(&[
            ("playerid", 1003),
            ("overallrating", 65),
            ("potential", 70),
            ("preferredposition1", 23),
            ("birthdate", 148_928),
            ("nationality", 52),
            ("preferredfoot", 0),
            ("contractvaliduntil", 0),
            ("trait1", 0),
            ("icontrait1", 0),
            ("pacdiv", 60),
            ("shohan", 55),
            ("paskic", 58),
            ("driref", 62),
            ("defspe", 30),
            ("phypos", 55),
        ]),
    ]
}

fn fixture() -> FixtureTableProvider {
    let mut fields = Vec::with_capacity(FIELDS.len());
    let mut layout = Vec::with_capacity(FIELDS.len());
    let mut bit_offset = 0u32;
    for (name, shortname, width) in FIELDS {
        fields.push(field(name, shortname, bit_offset, *width));
        layout.push((*name, bit_offset, *width));
        bit_offset += *width;
    }
    // Leave one padding byte so the high bit of the last record byte remains
    // the table's invalid-marker flag.
    let record_size = bit_offset.div_ceil(8) as usize + 1;
    let players = synthetic_rows();
    let mut data = vec![0u8; record_size * players.len()];
    for (index, values) in players.iter().enumerate() {
        let row = &mut data[index * record_size..(index + 1) * record_size];
        for (name, offset, width) in &layout {
            write_bits(row, *offset, *width, values.get(name).copied().unwrap_or(0));
        }
    }
    let snapshot = TableSnapshot::from_parts(
        TableSnapshotInfo {
            table_id: 1,
            table_name: "players".to_string(),
            shortname: "CZUM".to_string(),
            descriptor: 0,
            data_pointer: 0,
            record_size: record_size as u32,
            slot_count: players.len() as u32,
            capacity: players.len() as u32,
            invalid_marker_count: 0,
            field_count: fields.len() as u32,
        },
        fields,
        data,
    )
    .unwrap();
    FixtureTableProvider::new(vec![Arc::new(snapshot)])
}

fn game_data() -> GameData {
    let mut player_names = BTreeMap::new();
    player_names.insert(1001, "Alpha, \"Striker\"".to_string());
    player_names.insert(1002, "Beta Keeper".to_string());
    player_names.insert(1003, "Gamma Winger".to_string());

    let mut team_names = BTreeMap::new();
    team_names.insert(5, "Test Club, One".to_string());
    team_names.insert(6, "Test Keeper FC".to_string());

    let mut player_team_ids = BTreeMap::new();
    player_team_ids.insert(1001, 5);
    player_team_ids.insert(1002, 6);
    player_team_ids.insert(1003, 0);

    GameData {
        in_career: true,
        date: GameDate {
            day: 1,
            month: 7,
            year: 2027,
        },
        date_source: "fixture",
        stats: Vec::new(),
        stats_error: None,
        player_names,
        team_names,
        player_team_ids,
        competition_names: BTreeMap::new(),
        competition_source: "fixture",
        competition_error: None,
    }
}

fn parse_csv(contents: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = contents.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            '\r' if !quoted => {}
            _ => field.push(character),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn column(header: &[String], name: &str) -> usize {
    header
        .iter()
        .position(|column| column == name)
        .unwrap_or_else(|| panic!("missing column {name}"))
}

#[test]
fn player_field_helpers_and_export_path_are_first_party() {
    let base = std::env::temp_dir().join(format!(
        "fc27le-fields-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let nested = base.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let existing = nested.join("players.csv");
    std::fs::write(&existing, "existing\n").unwrap();

    let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/lua");
    let scripts = std::fs::canonicalize(scripts).unwrap();
    let source = format!(
        r#"
EXPORT_DIR = [==[{}]==]
local fields = require("player_fields")
local csv = require("csv")

local ok, err = pcall(FC27LE_EnsureDirectory, "")
assert(not ok and tostring(err):find("non-empty", 1, true) ~= nil)

assert(FC27LE_EnsureDirectory(EXPORT_DIR .. "/created/nested"))
local players = assert(LE.db:GetTable("players"))
local record = players:GetFirstRecord()
local rows = {{ csv.row({{
    "Name", "PreferredFoot", "ContractUntil", "Playstyles",
    "PlaystylePlus", "Pace", "Team", "Missing"
}}) }}
local count = 0
while record > 0 do
    local values, missing = fields.read_player(players, record)
    local name = GetPlayerName(values.playerid)
    local team = GetTeamName(GetTeamIdFromPlayerId(values.playerid))
    if not team or team == "" then team = "unknown" end
    local kind = "outfield"
    if values.preferredposition1 == 0 then kind = "goalkeeper" end
    local trait = values.trait1
    local trait_plus = values.icontrait1
    if kind == "goalkeeper" then
        trait = values.trait2
        trait_plus = values.icontrait2
    end
    rows[#rows + 1] = csv.row({{
        name,
        fields.decode_preferred_foot(values.preferredfoot),
        fields.normalize_contract_year(values.contractvaliduntil) or "unknown",
        fields.decode_playstyles(trait, kind) or "unknown",
        fields.decode_playstyles(trait_plus, kind) or "unknown",
        values.pacdiv or "unknown",
        team,
        #missing,
    }})
    count = count + 1
    record = players:GetNextValidRecord()
end
local path = csv.unique_path(EXPORT_DIR .. "/nested/players.csv")
csv.write_atomic(path, rows)
MessageBox("player fields", tostring(count) .. "|" .. path)
"#,
        base.display()
    );

    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let context =
        build_fixture_context_with_game_data(fixture(), game_data(), cancel.clone(), tx).unwrap();
    let job = ScriptJob {
        name: "player-fields".to_string(),
        source,
        script_dirs: vec![scripts],
        timeout: Duration::from_secs(5),
        memory_limit_bytes: 64 * 1024 * 1024,
    };
    run_script(&context.lua, &job, cancel).unwrap();

    let events = {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    };
    assert!(events.iter().any(|event| matches!(
        event,
        LuaEvent::MessageBox { title, text }
            if title == "player fields" && text.starts_with("3|")
    )));

    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "existing\n");
    assert!(base.join("created/nested").is_dir());
    let unique = nested.join("players_2.csv");
    let contents = std::fs::read_to_string(&unique).unwrap();
    assert!(contents.contains("\"Alpha, \"\"Striker\"\"\""));
    assert!(contents.contains("\"Test Club, One\""));

    let parsed = parse_csv(&contents);
    let header = &parsed[0];
    assert_eq!(header.len(), 8);
    assert_eq!(parsed.len(), 4);
    let name = column(header, "Name");
    let foot = column(header, "PreferredFoot");
    let contract = column(header, "ContractUntil");
    let playstyles = column(header, "Playstyles");
    let playstyles_plus = column(header, "PlaystylePlus");
    let pace = column(header, "Pace");
    let team = column(header, "Team");
    let missing = column(header, "Missing");

    assert_eq!(parsed[1][name], "Alpha, \"Striker\"");
    assert_eq!(parsed[1][foot], "Right");
    assert_eq!(parsed[1][contract], "2028");
    assert_eq!(parsed[1][playstyles], "Dead Ball;Finesse Shot;Tiki Taka");
    assert_eq!(parsed[1][playstyles_plus], "Quick Step");
    assert_eq!(parsed[1][pace], "70");
    assert_eq!(parsed[1][team], "Test Club, One");
    assert_eq!(parsed[1][missing], "0");

    assert_eq!(parsed[2][name], "Beta Keeper");
    assert_eq!(parsed[2][foot], "Left");
    assert_eq!(parsed[2][contract], "2030");
    assert_eq!(parsed[2][playstyles], "Far Reach;Far Throw");
    assert_eq!(parsed[2][playstyles_plus], "Rush Out");
    assert_eq!(parsed[2][pace], "80");
    assert_eq!(parsed[2][team], "Test Keeper FC");
    assert_eq!(parsed[2][missing], "0");

    assert_eq!(parsed[3][name], "Gamma Winger");
    assert_eq!(parsed[3][foot], "Right");
    assert_eq!(parsed[3][contract], "unknown");
    assert_eq!(parsed[3][playstyles], "");
    assert_eq!(parsed[3][playstyles_plus], "");
    assert_eq!(parsed[3][pace], "60");
    assert_eq!(parsed[3][team], "unknown");
    assert_eq!(parsed[3][missing], "0");

    std::fs::remove_dir_all(&base).ok();
}
