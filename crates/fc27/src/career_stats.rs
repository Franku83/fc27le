//! Read-only career-mode state and season statistics.
//!
//! # Evidence
//!
//! The layouts below are derived from the documented Live Editor Lua API and
//! independent static analysis of `GetPlayersStats`, `GetPlayerStats`,
//! `IsInCM`, `GetCurrentDate` and `GetTeamName` in the FC 27 build. The live
//! validation was performed against a running Career Mode save:
//!
//! * the career core service is registered under key `0x1297F047`;
//! * the statistics interface is registered under key `0x0A613B9A`;
//! * the statistics vector is reached by
//!   `interface + 0x18 -> + 0x10 -> + 0x08 -> deref + 0x00 -> + 0x90`;
//! * each statistics record is `0x5C` bytes;
//! * the calendar manager is manager type `24` in the mode-manager table;
//! * season records are valid when `playerid` is in `1..=u32::MAX-1` and the
//!   byte at `+0x1B` is non-zero.
//!
//! Nothing here calls a game function, writes memory or scans the heap. All
//! reads are bounded and all pointer chains are validated before use.

use std::collections::BTreeMap;

use fc27_core::{Error, Result};
use fc27_process::ProcessHandle;
use serde::Serialize;

use crate::db_service::{DbTable, read_table_fields};
use crate::player_store::{read_bits, read_range};

/// Key for the career core service in the game-service registry.
pub const CAREER_CORE_KEY: u32 = 0x1297_F047;
/// Key for the live `FCEInterface` object in the game-service registry.
pub const STATS_INTERFACE_KEY: u32 = 0x0A61_3B9A;
/// Game-service registry offsets, shared with [`crate::db_service`].
pub const SERVICE_MAP_OFFSET: u64 = 0x48;
pub const SERVICE_MAP_FLAG_OFFSET: u64 = 0x32C0;
pub const SERVICE_MAP_ALT_OFFSET: u64 = 0x1960;
pub const SERVICE_VALUE_OFFSET: u64 = 0x878;
pub const SERVICE_MAP_CAPACITY: usize = 0x21D;
/// Manager type id for `ENUM_FCEGameModesFCECareerModeCalendarManager`.
pub const CALENDAR_MANAGER_TYPE: u64 = 24;
/// Manager type id for `ENUM_FCEGameModesFCECareerModeActiveCompetitionsManager`.
pub const ACTIVE_COMPETITIONS_MANAGER_TYPE: u64 = 20;
/// Verified 4CC for the live league table in the tested build.
pub const LEAGUE_TABLE_SHORTNAME: &str = "onMQ";
/// Verified 4CC for the league id field in `onMQ`.
pub const LEAGUE_ID_FIELD: &str = "aQrQ";
/// Verified 4CC for the league name field in `onMQ`.
pub const LEAGUE_NAME_FIELD: &str = "HEQX";
/// Safety cap for the active-competitions object scan.
pub const MAX_ACTIVE_COMPETITION_BYTES: usize = 0x4000;
/// One season-statistics record.
pub const STATS_RECORD_SIZE: usize = 0x5C;
/// Hard cap for a single statistics snapshot (200k records, about 11 MiB).
pub const MAX_STATS_RECORDS: usize = 200_000;
/// Maximum bytes in one team table snapshot.
pub const MAX_TEAM_TABLE_BYTES: u64 = 64 * 1024 * 1024;
/// Verified 4CC for the live team-name table in the tested build.
pub const TEAM_TABLE_SHORTNAME: &str = "tWYe";
/// Verified 4CC for the career team id field in `tWYe`.
pub const TEAM_CAREER_ID_FIELD: &str = "mCXg";
/// Verified 4CC for the display-name field in `tWYe`.
pub const TEAM_NAME_FIELD: &str = "vIys";
/// Verified 4CC for the database team id field in `tWYe`.
pub const TEAM_DB_ID_FIELD: &str = "CkjP";

/// In-game date returned by `GetCurrentDate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GameDate {
    pub day: u32,
    pub month: u32,
    pub year: u32,
}

impl Default for GameDate {
    fn default() -> Self {
        Self {
            day: 1,
            month: 7,
            year: 2021,
        }
    }
}

/// One valid season-statistics row.
///
/// `rating_sum` is the raw value exposed by the Lua API as `avg`. The Lua
/// exporter divides it by appearances and by 10 before formatting. Keeping the
/// raw value preserves the original API semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CareerStat {
    pub team_id: u32,
    pub player_id: u32,
    pub competition_obj_id: u16,
    pub rating_sum: u16,
    pub goals: u16,
    pub two_yellow: u16,
    pub yellow: u8,
    pub red: u8,
    pub appearances: u8,
    pub assists: u8,
    pub motm: u8,
    pub saves: u8,
    pub goals_conceded: u8,
    pub clean_sheets: u8,
}

/// Career state and the raw season-statistics snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct CareerSnapshot {
    pub in_career: bool,
    pub date: GameDate,
    pub date_source: &'static str,
    pub stats: Vec<CareerStat>,
    pub stats_source: &'static str,
    pub stats_note: Option<String>,
}

/// Verified career team directory: career team id -> display name.
#[derive(Debug, Clone, Default)]
pub struct TeamDirectory {
    names: BTreeMap<u32, String>,
}

impl TeamDirectory {
    /// Load and validate the live team-name table.
    ///
    /// The table is accepted only when the verification rows match the known
    /// clubs in this build. A mismatch is an evidence failure, not a reason to
    /// emit guessed names.
    pub fn load(handle: &dyn ProcessHandle, live_tables: &[DbTable]) -> Result<Self> {
        let table = live_tables
            .iter()
            .find(|table| table.shortname == TEAM_TABLE_SHORTNAME)
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!(
                    "team table `{TEAM_TABLE_SHORTNAME}` is not present in the live table manager"
                ))
            })?;
        if table.header_44 == 0 || table.header_44 > 4096 {
            return Err(Error::InsufficientEvidence(format!(
                "team table record size {} is outside 1..=4096",
                table.header_44
            )));
        }
        let slot_count = u32::from(handle.read_u16(off(table.address, 0x7C)?)?);
        let capacity = u32::from(handle.read_u16(off(table.address, 0x78)?)?);
        if slot_count > capacity {
            return Err(Error::InsufficientEvidence(format!(
                "team table slot count {slot_count} exceeds capacity {capacity}"
            )));
        }
        let bytes = u64::from(slot_count)
            .checked_mul(u64::from(table.header_44))
            .ok_or_else(|| Error::parse("team table", "snapshot size overflow"))?;
        if bytes > MAX_TEAM_TABLE_BYTES {
            return Err(Error::InsufficientEvidence(format!(
                "team table snapshot would read {bytes} bytes, above the {MAX_TEAM_TABLE_BYTES} cap"
            )));
        }
        let fields = read_table_fields(handle, table)?;
        let career_id = fields
            .iter()
            .find(|field| field.shortname == TEAM_CAREER_ID_FIELD)
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!(
                    "team table has no `{TEAM_CAREER_ID_FIELD}` field"
                ))
            })?;
        let db_id = fields
            .iter()
            .find(|field| field.shortname == TEAM_DB_ID_FIELD)
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!("team table has no `{TEAM_DB_ID_FIELD}` field"))
            })?;
        let name = fields
            .iter()
            .find(|field| field.shortname == TEAM_NAME_FIELD)
            .ok_or_else(|| {
                Error::InsufficientEvidence(format!("team table has no `{TEAM_NAME_FIELD}` field"))
            })?;
        if career_id.field_type != 3 || db_id.field_type != 3 || name.field_type != 0 {
            return Err(Error::InsufficientEvidence(
                "team table field types do not match the verified layout".to_string(),
            ));
        }
        if name.bit_width < 8 || name.bit_width % 8 != 0 {
            return Err(Error::InsufficientEvidence(format!(
                "team name field width {} is not a whole number of bytes",
                name.bit_width
            )));
        }
        let data = if bytes == 0 {
            Vec::new()
        } else {
            read_range(handle, table.data_pointer, bytes as usize)?
        };
        let row_size = table.header_44 as usize;
        let mut names = BTreeMap::new();
        for slot in 0..slot_count as usize {
            let row = &data[slot * row_size..(slot + 1) * row_size];
            if row[row_size - 1] & 0x80 != 0 {
                continue;
            }
            let career = read_bits(row, career_id.bit_offset, career_id.bit_width)?;
            if career == 0 || career > u64::from(u32::MAX) {
                continue;
            }
            let start = (name.bit_offset / 8) as usize;
            let length = (name.bit_width / 8) as usize;
            if start + length > row.len() {
                continue;
            }
            let bytes = &row[start..start + length];
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            let Ok(text) = std::str::from_utf8(&bytes[..end]) else {
                continue;
            };
            names.insert(career as u32, text.to_string());
        }
        let expected = [
            (5, "Chelsea"),
            (9, "Liverpool"),
            (240, "Atlético de Madrid"),
            (243, "Real Madrid"),
            (1860, "Getafe CF"),
        ];
        for (team_id, expected_name) in expected {
            match names.get(&team_id) {
                Some(actual) if actual == expected_name => {}
                Some(actual) => {
                    return Err(Error::InsufficientEvidence(format!(
                        "team table validation failed: career team {team_id} is `{actual}`, \
                         expected `{expected_name}`"
                    )));
                }
                None => {
                    return Err(Error::InsufficientEvidence(format!(
                        "team table validation failed: career team {team_id} is missing"
                    )));
                }
            }
        }
        Ok(Self { names })
    }

    pub fn name(&self, team_id: u32) -> Option<&str> {
        self.names.get(&team_id).map(String::as_str)
    }

    pub fn names(&self) -> &BTreeMap<u32, String> {
        &self.names
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Resolve the current team id for a player from the season records.
///
/// The original API resolves the current club through a career player object.
/// This read-only implementation uses the team id recorded on the player's
/// season rows and chooses the most frequent one. Ties keep the first team
/// encountered. Players without season rows return `None`.
pub fn team_id_for_player(stats: &[CareerStat], player_id: u32) -> Option<u32> {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut first_seen: BTreeMap<u32, usize> = BTreeMap::new();
    for (index, stat) in stats.iter().enumerate() {
        if stat.player_id != player_id || stat.team_id == 0 || stat.team_id == u32::MAX {
            continue;
        }
        *counts.entry(stat.team_id).or_insert(0) += 1;
        first_seen.entry(stat.team_id).or_insert(index);
    }
    let mut candidates: Vec<(u32, usize, usize)> = counts
        .into_iter()
        .map(|(team_id, count)| {
            (
                team_id,
                count,
                first_seen.get(&team_id).copied().unwrap_or(usize::MAX),
            )
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.0.cmp(&right.0))
    });
    candidates.first().map(|(team_id, _, _)| *team_id)
}

/// Inspect career state and read the season statistics vector.
pub fn inspect(handle: &dyn ProcessHandle, service_object: u64) -> Result<CareerSnapshot> {
    let core = registry_value(handle, service_object, CAREER_CORE_KEY)?;
    let in_career = core != 0 && career_core_ready(handle, core)?;
    let (date, date_source) = if in_career {
        match calendar_date(handle, core) {
            Ok(Some(date)) => (date, "career_calendar"),
            Ok(None) => (GameDate::default(), "default_outside_career"),
            Err(_) => (GameDate::default(), "default_unavailable"),
        }
    } else {
        (GameDate::default(), "default_outside_career")
    };
    if !in_career {
        return Ok(CareerSnapshot {
            in_career: false,
            date,
            date_source,
            stats: Vec::new(),
            stats_source: "career_mode_inactive",
            stats_note: Some(
                "career mode is not active; no season statistics were read".to_string(),
            ),
        });
    }
    let interface = registry_value(handle, service_object, STATS_INTERFACE_KEY)?;
    if interface == 0 {
        return Err(Error::InsufficientEvidence(
            "career mode is active but the statistics interface is not registered".to_string(),
        ));
    }
    let stats = read_stats(handle, interface)?;
    Ok(CareerSnapshot {
        in_career: true,
        date,
        date_source,
        stats_source: "career_season_stats_vector",
        stats_note: None,
        stats,
    })
}

/// Read and decode the live statistics vector.
pub fn read_stats(handle: &dyn ProcessHandle, interface: u64) -> Result<Vec<CareerStat>> {
    let x1 = handle.read_u64(off(interface, 0x18)?)?;
    if x1 == 0 {
        return Err(Error::InsufficientEvidence(
            "statistics interface career object is null".to_string(),
        ));
    }
    let x2 = handle.read_u64(off(x1, 0x10)?)?;
    if x2 == 0 {
        return Err(Error::InsufficientEvidence(
            "statistics career container is null".to_string(),
        ));
    }
    let x3 = handle.read_u64(off(x2, 0x08)?)?;
    if x3 == 0 {
        return Err(Error::InsufficientEvidence(
            "statistics manager is null".to_string(),
        ));
    }
    let x4 = handle.read_u64(x3)?;
    if x4 == 0 {
        return Err(Error::InsufficientEvidence(
            "statistics manager implementation is null".to_string(),
        ));
    }
    let vector = handle.read_u64(off(x4, 0x90)?)?;
    if vector == 0 {
        return Err(Error::InsufficientEvidence(
            "statistics interface has no statistics vector object".to_string(),
        ));
    }
    let begin = handle.read_u64(vector)?;
    let end = handle.read_u64(off(vector, 0x08)?)?;
    if begin == 0 && end == 0 {
        return Ok(Vec::new());
    }
    if begin == 0 || end <= begin {
        return Err(Error::InsufficientEvidence(format!(
            "statistics vector bounds are invalid: begin {begin:#x}, end {end:#x}"
        )));
    }
    let size = end - begin;
    if size % STATS_RECORD_SIZE as u64 != 0 {
        return Err(Error::InsufficientEvidence(format!(
            "statistics vector byte size {size} is not a multiple of {STATS_RECORD_SIZE}"
        )));
    }
    let count = (size / STATS_RECORD_SIZE as u64) as usize;
    if count > MAX_STATS_RECORDS {
        return Err(Error::InsufficientEvidence(format!(
            "statistics vector has {count} records, above the {MAX_STATS_RECORDS} safety cap"
        )));
    }
    let data = read_range(handle, begin, size as usize)?;
    let mut stats = Vec::with_capacity(count);
    for index in 0..count {
        let row = &data[index * STATS_RECORD_SIZE..(index + 1) * STATS_RECORD_SIZE];
        if let Some(stat) = decode_stat_record(row)? {
            stats.push(stat);
        }
    }
    Ok(stats)
}

/// Decode one `0x5C`-byte record.
///
/// Returns `None` for slots that the game marks invalid. The caller must pass
/// exactly [`STATS_RECORD_SIZE`] bytes.
pub fn decode_stat_record(row: &[u8]) -> Result<Option<CareerStat>> {
    if row.len() != STATS_RECORD_SIZE {
        return Err(Error::InvalidInput(format!(
            "statistics record must be {STATS_RECORD_SIZE} bytes, got {}",
            row.len()
        )));
    }
    let player_id = u32::from_le_bytes([row[0x04], row[0x05], row[0x06], row[0x07]]);
    if player_id == 0 || player_id == u32::MAX || row[0x1B] == 0 {
        return Ok(None);
    }
    let packed = u32::from_le_bytes([row[0x10], row[0x11], row[0x12], row[0x13]]);
    Ok(Some(CareerStat {
        team_id: u32::from_le_bytes([row[0x00], row[0x01], row[0x02], row[0x03]]),
        player_id,
        competition_obj_id: u16::from_le_bytes([row[0x08], row[0x09]]),
        rating_sum: u16::from_le_bytes([row[0x0C], row[0x0D]]),
        goals: (packed & 0x1FF) as u16,
        two_yellow: ((packed >> 9) & 0x7F) as u16,
        yellow: row[0x12],
        red: row[0x13],
        appearances: row[0x14],
        assists: row[0x16],
        motm: row[0x17],
        saves: row[0x18],
        goals_conceded: row[0x19],
        clean_sheets: row[0x1A],
    }))
}

/// Read a game-service registry value without calling a game function.
pub fn registry_value(handle: &dyn ProcessHandle, service_object: u64, key: u32) -> Result<u64> {
    if service_object == 0 {
        return Ok(0);
    }
    let map_base = off(service_object, SERVICE_MAP_OFFSET)?;
    let flag = handle.read_u32(off(map_base, SERVICE_MAP_FLAG_OFFSET)?)?;
    let key_base = if flag != 0 {
        off(map_base, SERVICE_MAP_ALT_OFFSET)?
    } else {
        map_base
    };
    let value_base = off(key_base, SERVICE_VALUE_OFFSET)?;
    for index in 0..SERVICE_MAP_CAPACITY {
        let key_address = off(key_base, (index as u64) * 4)?;
        if handle.read_u32(key_address)? == key {
            return handle.read_u64(off(value_base, (index as u64) * 8)?);
        }
    }
    Ok(0)
}

/// Verified career-mode predicate from the core object path.
pub fn career_core_ready(handle: &dyn ProcessHandle, core: u64) -> Result<bool> {
    if core == 0 {
        return Ok(false);
    }
    let p1 = handle.read_u64(off(core, 0x20)?)?;
    if p1 == 0 {
        return Ok(false);
    }
    let p2 = handle.read_u64(off(p1, 0x10)?)?;
    if p2 == 0 || handle.read_u32(off(p2, 0x250)?)? != 1 {
        return Ok(false);
    }
    let p3 = handle.read_u64(off(p2, 0x248)?)?;
    if p3 == 0 || handle.read_u32(off(p3, 0x10)?)? != 1 {
        return Ok(false);
    }
    let p4 = handle.read_u64(off(p2, 600)?)?;
    if p4 == 0 {
        return Ok(false);
    }
    Ok(handle.read_u64(p4)? != 0)
}

/// Read the calendar manager date (`day`, `month`, `year` at `+0x34/+0x38/+0x3C`).
pub fn calendar_date(handle: &dyn ProcessHandle, core: u64) -> Result<Option<GameDate>> {
    let p1 = handle.read_u64(off(core, 0x20)?)?;
    if p1 == 0 {
        return Ok(None);
    }
    let managers = handle.read_u64(off(p1, 0x10)?)?;
    if managers == 0 {
        return Ok(None);
    }
    let entry = off(managers, CALENDAR_MANAGER_TYPE * 0x20)?;
    if handle.read_u32(off(entry, 0x10)?)? != 1 {
        return Ok(None);
    }
    let type_object = handle.read_u64(off(entry, 0x08)?)?;
    if type_object == 0 || handle.read_u32(off(type_object, 0x10)?)? != 1 {
        return Ok(None);
    }
    let holder = handle.read_u64(off(entry, 0x18)?)?;
    if holder == 0 {
        return Ok(None);
    }
    let calendar = handle.read_u64(holder)?;
    if calendar == 0 {
        return Ok(None);
    }
    let date = GameDate {
        day: handle.read_u32(off(calendar, 0x34)?)?,
        month: handle.read_u32(off(calendar, 0x38)?)?,
        year: handle.read_u32(off(calendar, 0x3C)?)?,
    };
    if !(1..=31).contains(&date.day)
        || !(1..=12).contains(&date.month)
        || !(1900..=3000).contains(&date.year)
    {
        return Err(Error::InsufficientEvidence(format!(
            "calendar date {}/{}/{} is outside the plausible range",
            date.day, date.month, date.year
        )));
    }
    Ok(Some(date))
}

/// Resolve the competitions whose active-competition tag maps to a league row.
///
/// The game's active-competitions manager stores a `C<id>` tag next to each
/// competition object id. For domestic leagues that tag is one less than the
/// league id in the live `onMQ` table (for example 818 -> `C13` -> league 14 ->
/// `England Premier League (1)`). The mapping is accepted only after the league
/// table passes known-row checks. Cups and competitions without a matching
/// league row stay unresolved and are rendered as `COBJ<id>` by the Lua layer.
pub fn competition_names(
    handle: &dyn ProcessHandle,
    service_object: u64,
    live_tables: &[DbTable],
) -> Result<BTreeMap<u16, String>> {
    let leagues = league_names(handle, live_tables)?;
    let tags = active_competition_tags(handle, service_object)?;
    Ok(map_competition_names(&tags, &leagues))
}

/// Parse a four-byte `C<digits>` competition tag.
pub fn parse_competition_tag(tag: &[u8]) -> Option<u16> {
    if tag.len() < 2 || tag[0] != b'C' {
        return None;
    }
    let mut value = 0u32;
    let mut digits = 0usize;
    for byte in &tag[1..] {
        if *byte == 0 {
            break;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
        if value > u32::from(u16::MAX) {
            return None;
        }
        digits += 1;
    }
    if digits == 0 {
        None
    } else {
        Some(value as u16)
    }
}

/// Pure mapping step used by tests and the live resolver.
pub fn map_competition_names(
    tags: &BTreeMap<u16, u16>,
    leagues: &BTreeMap<u16, String>,
) -> BTreeMap<u16, String> {
    let mut result = BTreeMap::new();
    for (competition_obj_id, tag) in tags {
        if let Some(league_id) = tag.checked_add(1) {
            if let Some(name) = leagues.get(&league_id) {
                result.insert(*competition_obj_id, name.clone());
            }
        }
    }
    result
}

fn active_competition_tags(
    handle: &dyn ProcessHandle,
    service_object: u64,
) -> Result<BTreeMap<u16, u16>> {
    let core = registry_value(handle, service_object, CAREER_CORE_KEY)?;
    if core == 0 {
        return Err(Error::InsufficientEvidence(
            "career core is not available for the active-competitions manager".to_string(),
        ));
    }
    let p1 = handle.read_u64(off(core, 0x20)?)?;
    if p1 == 0 {
        return Err(Error::InsufficientEvidence(
            "career core container is null".to_string(),
        ));
    }
    let managers = handle.read_u64(off(p1, 0x10)?)?;
    if managers == 0 {
        return Err(Error::InsufficientEvidence(
            "career mode manager table is null".to_string(),
        ));
    }
    let entry = off(managers, ACTIVE_COMPETITIONS_MANAGER_TYPE * 0x20)?;
    if handle.read_u32(off(entry, 0x10)?)? != 1 {
        return Err(Error::InsufficientEvidence(
            "active-competitions manager entry is not instantiated".to_string(),
        ));
    }
    let type_object = handle.read_u64(off(entry, 0x08)?)?;
    if type_object == 0 || handle.read_u32(off(type_object, 0x10)?)? != 1 {
        return Err(Error::InsufficientEvidence(
            "active-competitions manager type check failed".to_string(),
        ));
    }
    let holder = handle.read_u64(off(entry, 0x18)?)?;
    if holder == 0 {
        return Err(Error::InsufficientEvidence(
            "active-competitions manager holder is null".to_string(),
        ));
    }
    let manager = handle.read_u64(holder)?;
    if manager == 0 {
        return Err(Error::InsufficientEvidence(
            "active-competitions manager object is null".to_string(),
        ));
    }
    let data = handle.read(manager, MAX_ACTIVE_COMPETITION_BYTES)?;
    let needle = b"TrophyName_Abbr15_";
    let mut tags = BTreeMap::new();
    for position in 0..data.len().saturating_sub(needle.len()) {
        if &data[position..position + needle.len()] != needle {
            continue;
        }
        if position < 0x18 {
            continue;
        }
        if data[position - 0x10..position - 0x0C] != 3u32.to_le_bytes() {
            continue;
        }
        let competition = u32::from_le_bytes([
            data[position - 0x14],
            data[position - 0x13],
            data[position - 0x12],
            data[position - 0x11],
        ]);
        if competition == 0 || competition > u32::from(u16::MAX) {
            continue;
        }
        let Some(tag) = parse_competition_tag(&data[position - 0x08..position - 0x04]) else {
            continue;
        };
        tags.entry(competition as u16).or_insert(tag);
    }
    if tags.is_empty() {
        return Err(Error::InsufficientEvidence(
            "active-competitions manager contained no recognised competition tags".to_string(),
        ));
    }
    Ok(tags)
}

fn league_names(
    handle: &dyn ProcessHandle,
    live_tables: &[DbTable],
) -> Result<BTreeMap<u16, String>> {
    let table = live_tables
        .iter()
        .find(|table| table.shortname == LEAGUE_TABLE_SHORTNAME)
        .ok_or_else(|| {
            Error::InsufficientEvidence(format!(
                "league table `{LEAGUE_TABLE_SHORTNAME}` is not present in the live table manager"
            ))
        })?;
    if table.header_44 == 0 || table.header_44 > 4096 {
        return Err(Error::InsufficientEvidence(format!(
            "league table record size {} is outside 1..=4096",
            table.header_44
        )));
    }
    let slot_count = u32::from(handle.read_u16(off(table.address, 0x7C)?)?);
    let capacity = u32::from(handle.read_u16(off(table.address, 0x78)?)?);
    if slot_count > capacity {
        return Err(Error::InsufficientEvidence(format!(
            "league table slot count {slot_count} exceeds capacity {capacity}"
        )));
    }
    let bytes = u64::from(slot_count)
        .checked_mul(u64::from(table.header_44))
        .ok_or_else(|| Error::parse("league table", "snapshot size overflow"))?;
    if bytes > MAX_TEAM_TABLE_BYTES {
        return Err(Error::InsufficientEvidence(format!(
            "league table snapshot would read {bytes} bytes, above the {MAX_TEAM_TABLE_BYTES} cap"
        )));
    }
    let fields = read_table_fields(handle, table)?;
    let id = fields
        .iter()
        .find(|field| field.shortname == LEAGUE_ID_FIELD)
        .ok_or_else(|| {
            Error::InsufficientEvidence(format!("league table has no `{LEAGUE_ID_FIELD}` field"))
        })?;
    let name = fields
        .iter()
        .find(|field| field.shortname == LEAGUE_NAME_FIELD)
        .ok_or_else(|| {
            Error::InsufficientEvidence(format!("league table has no `{LEAGUE_NAME_FIELD}` field"))
        })?;
    if id.field_type != 3 || name.field_type != 0 || name.bit_width < 8 || name.bit_width % 8 != 0 {
        return Err(Error::InsufficientEvidence(
            "league table field types do not match the verified layout".to_string(),
        ));
    }
    let data = if bytes == 0 {
        Vec::new()
    } else {
        read_range(handle, table.data_pointer, bytes as usize)?
    };
    let row_size = table.header_44 as usize;
    let mut result = BTreeMap::new();
    for slot in 0..slot_count as usize {
        let row = &data[slot * row_size..(slot + 1) * row_size];
        if row[row_size - 1] & 0x80 != 0 {
            continue;
        }
        let league_id = read_bits(row, id.bit_offset, id.bit_width)?;
        if league_id == 0 || league_id > u64::from(u16::MAX) {
            continue;
        }
        let start = (name.bit_offset / 8) as usize;
        let length = (name.bit_width / 8) as usize;
        if start + length > row.len() {
            continue;
        }
        let bytes = &row[start..start + length];
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        let Ok(text) = std::str::from_utf8(&bytes[..end]) else {
            continue;
        };
        result.insert(league_id as u16, text.to_string());
    }
    let checks = [
        (14u16, "Premier League"),
        (54, "Primera División"),
        (32, "Serie A"),
        (20, "Bundesliga"),
        (309, "Primeira Liga"),
        (40, "Major League Soccer"),
        (42, "Eliteserien"),
        (17, "Ligue 1"),
        (11, "Eredivisie"),
    ];
    for (league_id, needle) in checks {
        if !result
            .get(&league_id)
            .is_some_and(|name| name.contains(needle))
        {
            return Err(Error::InsufficientEvidence(format!(
                "league table validation failed: league {league_id} does not contain `{needle}`"
            )));
        }
    }
    Ok(result)
}

fn off(base: u64, offset: u64) -> Result<u64> {
    base.checked_add(offset)
        .ok_or_else(|| Error::parse("career statistics", "address overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Vec<u8> {
        let mut row = vec![0u8; STATS_RECORD_SIZE];
        row[0x00..0x04].copy_from_slice(&1860u32.to_le_bytes());
        row[0x04..0x08].copy_from_slice(&242534u32.to_le_bytes());
        row[0x08..0x0A].copy_from_slice(&1422u16.to_le_bytes());
        row[0x0C..0x0E].copy_from_slice(&1113u16.to_le_bytes());
        // goals = 3, two_yellow = 2, packed into bits 0..8 and 9..15.
        let packed = 3u32 | (2u32 << 9);
        row[0x10..0x14].copy_from_slice(&packed.to_le_bytes());
        row[0x12] = 4; // yellow overlaps the high byte of the u32, not the bitfields.
        row[0x13] = 1; // red
        row[0x14] = 18; // appearances
        row[0x16] = 5; // assists
        row[0x17] = 2; // motm
        row[0x18] = 1; // saves
        row[0x19] = 21; // conceded
        row[0x1A] = 4; // clean sheets
        row[0x1B] = 1; // valid
        row
    }

    #[test]
    fn decodes_verified_record_layout() {
        let stat = decode_stat_record(&record()).unwrap().unwrap();
        assert_eq!(stat.team_id, 1860);
        assert_eq!(stat.player_id, 242534);
        assert_eq!(stat.competition_obj_id, 1422);
        assert_eq!(stat.rating_sum, 1113);
        assert_eq!(stat.goals, 3);
        assert_eq!(stat.two_yellow, 2);
        assert_eq!(stat.yellow, 4);
        assert_eq!(stat.red, 1);
        assert_eq!(stat.appearances, 18);
        assert_eq!(stat.assists, 5);
        assert_eq!(stat.motm, 2);
        assert_eq!(stat.saves, 1);
        assert_eq!(stat.goals_conceded, 21);
        assert_eq!(stat.clean_sheets, 4);
    }

    #[test]
    fn skips_invalid_records() {
        let mut row = record();
        row[0x1B] = 0;
        assert!(decode_stat_record(&row).unwrap().is_none());
        row[0x1B] = 1;
        row[0x04..0x08].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_stat_record(&row).unwrap().is_none());
        row[0x04..0x08].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode_stat_record(&row).unwrap().is_none());
    }

    #[test]
    fn rejects_wrong_record_size() {
        assert!(decode_stat_record(&[0u8; STATS_RECORD_SIZE - 1]).is_err());
    }

    #[test]
    fn team_id_prefers_most_frequent_team() {
        let mut first = record();
        first[0x04..0x08].copy_from_slice(&7u32.to_le_bytes());
        first[0x00..0x04].copy_from_slice(&100u32.to_le_bytes());
        let mut second = first.clone();
        second[0x00..0x04].copy_from_slice(&200u32.to_le_bytes());
        let mut third = first.clone();
        third[0x00..0x04].copy_from_slice(&200u32.to_le_bytes());
        let stats = [
            decode_stat_record(&first).unwrap().unwrap(),
            decode_stat_record(&second).unwrap().unwrap(),
            decode_stat_record(&third).unwrap().unwrap(),
        ];
        assert_eq!(team_id_for_player(&stats, 7), Some(200));
        assert_eq!(team_id_for_player(&stats, 8), None);
    }

    #[test]
    fn game_date_default_is_documented_value() {
        assert_eq!(
            GameDate::default(),
            GameDate {
                day: 1,
                month: 7,
                year: 2021
            }
        );
    }

    #[test]
    fn team_directory_is_empty_by_default() {
        let directory = TeamDirectory::default();
        assert!(directory.is_empty());
        assert_eq!(directory.name(5), None);
    }

    #[test]
    fn parses_competition_tags() {
        assert_eq!(parse_competition_tag(b"C13\0"), Some(13));
        assert_eq!(parse_competition_tag(b"C206"), Some(206));
        assert_eq!(parse_competition_tag(b"C0\0\0"), Some(0));
        assert_eq!(parse_competition_tag(b"X13\0"), None);
        assert_eq!(parse_competition_tag(b"C"), None);
        assert_eq!(parse_competition_tag(b"C12x"), None);
        assert_eq!(parse_competition_tag(b"C99999"), None);
    }

    #[test]
    fn maps_tags_to_league_names() {
        let tags = BTreeMap::from([(818u16, 13u16), (1422u16, 53), (511u16, 201)]);
        let leagues = BTreeMap::from([
            (14u16, "England Premier League (1)".to_string()),
            (54, "Spain Primera División (1)".to_string()),
        ]);
        let mapped = map_competition_names(&tags, &leagues);
        assert_eq!(
            mapped.get(&818).map(String::as_str),
            Some("England Premier League (1)")
        );
        assert_eq!(
            mapped.get(&1422).map(String::as_str),
            Some("Spain Primera División (1)")
        );
        assert!(!mapped.contains_key(&511));
    }
}
