//! Public FC Live Editor Lua API compatibility matrix.
//!
//! The matrix is deliberately explicit: unsupported functions raise a
//! compatibility error instead of returning fabricated data or pretending a
//! write occurred.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiStatus {
    SupportedTested,
    PartiallySupported,
    Unimplemented,
    RequiresRe,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiEntry {
    pub name: &'static str,
    pub signature: &'static str,
    pub returns: &'static str,
    pub required_state: &'static str,
    pub capability: &'static str,
    pub status: ApiStatus,
    pub evidence: &'static str,
    pub tests: &'static str,
    pub limitations: &'static str,
}

macro_rules! e {
    ($name:literal, $signature:literal, $returns:literal, $state:literal, $cap:literal, $status:ident, $evidence:literal, $tests:literal, $lim:literal) => {
        ApiEntry {
            name: $name,
            signature: $signature,
            returns: $returns,
            required_state: $state,
            capability: $cap,
            status: ApiStatus::$status,
            evidence: $evidence,
            tests: $tests,
            limitations: $lim,
        }
    };
}

pub static API_MATRIX: &[ApiEntry] = &[
    e!(
        "LE_VERSION",
        "string global",
        "string",
        "Lua state ready",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "FC27LE version string, not the original Live Editor version."
    ),
    e!(
        "LE",
        "table",
        "table",
        "Lua state ready",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "Only the fields implemented below are present."
    ),
    e!(
        "LE.version",
        "string field",
        "string",
        "Lua state ready",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "Compatibility layer version."
    ),
    e!(
        "LE.data",
        "string field",
        "string",
        "Lua state ready",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "Empty unless the caller supplies a data path."
    ),
    e!(
        "LE.game_base",
        "integer field",
        "integer",
        "Connected",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "FC27.exe module base."
    ),
    e!(
        "LE.game_size",
        "integer field",
        "integer",
        "Connected",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "FC27.exe module size."
    ),
    e!(
        "LE.game_name",
        "string field",
        "string",
        "Connected",
        "read",
        SupportedTested,
        "WP6 Lua runtime",
        "lua_compat::tests::le_version_and_db_lookup",
        "FC27.exe."
    ),
    e!(
        "LE.db",
        "table object",
        "table",
        "Connected + metadata",
        "read",
        SupportedTested,
        "local lua/libs/v2/imports/t3db/db.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Read-only database facade."
    ),
    e!(
        "LE.db.tables_count",
        "integer field",
        "integer",
        "Connected + metadata",
        "read",
        PartiallySupported,
        "local db.lua, t3db.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Reports the active profile table count: 4 in the embedded profile, 268 with external XML in the tested build."
    ),
    e!(
        "LE.db:GetTable",
        "table GetTable(string name)",
        "table|nil",
        "Connected + metadata",
        "read",
        SupportedTested,
        "local db.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Unknown names return nil, matching Live Editor."
    ),
    e!(
        "LE.db:GetTableByShortname",
        "table GetTableByShortname(string 4cc)",
        "table|nil",
        "Connected + metadata",
        "read",
        SupportedTested,
        "local db.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Unknown 4CC returns nil."
    ),
    e!(
        "LE.db:GetTableNames",
        "table GetTableNames()",
        "array<string>",
        "Connected + metadata",
        "read",
        SupportedTested,
        "WP6 addition",
        "lua_compat::tests::le_version_and_db_lookup",
        "FC27LE convenience; not present in every Live Editor version."
    ),
    e!(
        "LE.db:Refresh",
        "void Refresh()",
        "none",
        "Connected + metadata",
        "read",
        SupportedTested,
        "local db.lua Reset semantics",
        "lua_compat::tests::refresh_invalidates_old_handles",
        "Clears snapshots and increments the handle generation."
    ),
    e!(
        "LE.db:Reset",
        "void Reset()",
        "none",
        "Connected + metadata",
        "read",
        SupportedTested,
        "local db.lua",
        "lua_compat::tests::refresh_invalidates_old_handles",
        "Alias for Refresh."
    ),
    e!(
        "LE.db:Load",
        "void Load()",
        "none",
        "Connected + metadata",
        "read",
        SupportedTested,
        "local db.lua",
        "lua_compat::tests::refresh_invalidates_old_handles",
        "Alias for Refresh."
    ),
    e!(
        "table.name",
        "string field",
        "string",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Metadata name."
    ),
    e!(
        "table.shortname",
        "string field",
        "string",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "4CC descriptor code."
    ),
    e!(
        "table.written_records",
        "integer field",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Slot high-water mark from descriptor +0x7C."
    ),
    e!(
        "table.record_size",
        "integer field",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Bytes per slot."
    ),
    e!(
        "table.last_record_idx",
        "integer field",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "written_records - 1."
    ),
    e!(
        "table.current_record_idx",
        "integer field",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::iteration_reads_valid_records_and_skips_invalid",
        "Current cursor; updated by iteration."
    ),
    e!(
        "table:GetFirstRecord",
        "number GetFirstRecord()",
        "number",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::iteration_reads_valid_records_and_skips_invalid",
        "Returns a positive numeric token or 0. Tokens are not process pointers."
    ),
    e!(
        "table:GetNextValidRecord",
        "number GetNextValidRecord()",
        "number",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::iteration_reads_valid_records_and_skips_invalid",
        "Skips invalid markers; returns 0 at end."
    ),
    e!(
        "table:GetNextRecord",
        "number GetNextRecord()",
        "number",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::iteration_reads_valid_records_and_skips_invalid",
        "Advances even over invalid slots."
    ),
    e!(
        "table:GetRecordFieldValue",
        "value GetRecordFieldValue(number record, string field)",
        "integer|float|string",
        "GetTable succeeded + valid handle",
        "read",
        PartiallySupported,
        "local t3db/table.lua + field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Integer/date, float and inline string fields work. Compressed string fields (type 13) raise an explicit error."
    ),
    e!(
        "table:SetRecordFieldValue",
        "void SetRecordFieldValue(number record, string field, value)",
        "error",
        "GetTable succeeded",
        "write",
        Unimplemented,
        "read-only milestone",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Raises a compatibility error; no memory is modified."
    ),
    e!(
        "table:GetField",
        "field GetField(string name)",
        "field|nil",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Unknown field returns nil, matching the Lua layer."
    ),
    e!(
        "table:GetFieldsCount",
        "integer GetFieldsCount()",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "local t3db/table.lua",
        "lua_compat::tests::le_version_and_db_lookup",
        "Field entry count."
    ),
    e!(
        "table:GetName",
        "string GetName()",
        "string",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "WP6 convenience",
        "lua_compat::tests::le_version_and_db_lookup",
        "Metadata name."
    ),
    e!(
        "table:GetShortName",
        "string GetShortName()",
        "string",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "WP6 convenience",
        "lua_compat::tests::le_version_and_db_lookup",
        "4CC code."
    ),
    e!(
        "table:GetRecordSize",
        "integer GetRecordSize()",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "WP6 convenience",
        "lua_compat::tests::le_version_and_db_lookup",
        "Bytes per slot."
    ),
    e!(
        "table:GetWrittenRecords",
        "integer GetWrittenRecords()",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "WP6 convenience",
        "lua_compat::tests::le_version_and_db_lookup",
        "Slot count."
    ),
    e!(
        "table:GetLastRecordIdx",
        "integer GetLastRecordIdx()",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "WP6 convenience",
        "lua_compat::tests::le_version_and_db_lookup",
        "Slot count - 1."
    ),
    e!(
        "table:GetCurrentRecordIdx",
        "integer GetCurrentRecordIdx()",
        "integer",
        "GetTable succeeded",
        "read",
        SupportedTested,
        "WP6 convenience",
        "lua_compat::tests::iteration_reads_valid_records_and_skips_invalid",
        "Current cursor."
    ),
    e!(
        "field.name",
        "string field",
        "string",
        "GetField succeeded",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Metadata field name."
    ),
    e!(
        "field.shortname",
        "string field",
        "string",
        "GetField succeeded",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Field 4CC."
    ),
    e!(
        "field.type",
        "integer field",
        "integer",
        "GetField succeeded",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Live field type (0/3/4/13)."
    ),
    e!(
        "field.offset",
        "integer field",
        "integer",
        "GetField succeeded",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Byte offset."
    ),
    e!(
        "field.startbit",
        "integer field",
        "integer",
        "GetField succeeded",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Bit offset inside the byte."
    ),
    e!(
        "field.sz",
        "integer field",
        "integer",
        "GetField succeeded",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Bit width."
    ),
    e!(
        "field.type_name",
        "string field",
        "string",
        "GetField succeeded",
        "read",
        SupportedTested,
        "metadata schema",
        "lua_compat::tests::field_lookup_and_read",
        "Metadata type name."
    ),
    e!(
        "field:GetValue",
        "value GetValue(number record)",
        "integer|float|string",
        "GetField succeeded + valid handle",
        "read",
        SupportedTested,
        "local t3db/field.lua",
        "lua_compat::tests::field_lookup_and_read",
        "Same decoder as GetRecordFieldValue."
    ),
    e!(
        "field:SetValue",
        "void SetValue(number record, value)",
        "error",
        "GetField succeeded",
        "write",
        Unimplemented,
        "read-only milestone",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Raises a compatibility error; no memory is modified."
    ),
    e!(
        "LOGGER",
        "table object",
        "table",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LUA-API-v2-LOGGER",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "Writes to the GUI Lua console."
    ),
    e!(
        "LOGGER:LogDebug",
        "void LogDebug(string)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LOGGER",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "GUI log only."
    ),
    e!(
        "LOGGER:LogInfo",
        "void LogInfo(string)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LOGGER",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "GUI log only."
    ),
    e!(
        "LOGGER:LogWarn",
        "void LogWarn(string)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LOGGER",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "GUI log only."
    ),
    e!(
        "LOGGER:LogWarning",
        "void LogWarning(string)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "WP6 alias",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "Alias of LogWarn."
    ),
    e!(
        "LOGGER:LogError",
        "void LogError(string)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LOGGER",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "GUI log only."
    ),
    e!(
        "LOGGER:LogFatal",
        "void LogFatal(string)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LOGGER",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "GUI log only."
    ),
    e!(
        "MessageBox",
        "void MessageBox(string title, string text)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "wiki LUA-API",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "Shown as a GUI modal/log event, not a native Win32 dialog."
    ),
    e!(
        "FC27LE_EnsureDirectory",
        "bool FC27LE_EnsureDirectory(string path)",
        "bool",
        "Lua state ready",
        "filesystem",
        SupportedTested,
        "WP10 player-field and export helpers",
        "lua_compat::player_fields::player_field_helpers_and_export_path_are_first_party",
        "FC27LE extension, not a Live Editor API. Creates one directory selected by the script; no game memory or file writes."
    ),
    e!(
        "print",
        "void print(...)",
        "none",
        "Lua state ready",
        "ui",
        SupportedTested,
        "Lua 5.4",
        "lua_compat::tests::logger_print_messagebox_and_script_error",
        "Captured into the GUI log."
    ),
    e!(
        "AddEventHandler",
        "void AddEventHandler(string, function)",
        "error",
        "Lua state ready",
        "event",
        RequiresRe,
        "wiki LUA-API-v2-Events",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "No verified game-event source; raises an explicit error and registers nothing."
    ),
    e!(
        "GetEventHandlers",
        "Event[] GetEventHandlers(string)",
        "error",
        "Lua state ready",
        "event",
        RequiresRe,
        "wiki Events",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "No event registry is exposed."
    ),
    e!(
        "RemoveEventHandler",
        "void RemoveEventHandler(string, int)",
        "error",
        "Lua state ready",
        "event",
        RequiresRe,
        "wiki Events",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "No event registry is exposed."
    ),
    e!(
        "ClearEventHandlersForEvent",
        "void ClearEventHandlersForEvent(string)",
        "error",
        "Lua state ready",
        "event",
        RequiresRe,
        "wiki Events",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "No event registry is exposed."
    ),
    e!(
        "LEInitDoneEvent",
        "game event",
        "event",
        "Live Editor startup",
        "event",
        RequiresRe,
        "wiki Events",
        "n/a",
        "FC27LE creates the Lua state on demand; no startup event source."
    ),
    e!(
        "CareerModeEvent",
        "game event",
        "event",
        "Career game function",
        "event",
        RequiresRe,
        "wiki Events",
        "n/a",
        "Requires game-function hooking, which is explicitly out of scope."
    ),
    e!(
        "MEMORY",
        "table object",
        "table",
        "Lua state ready",
        "read/write",
        RequiresRe,
        "wiki LUA-API-v2-MEMORY",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "Proxy raises explicit errors for every member; raw memory access is not exposed."
    ),
    e!(
        "HTTP",
        "table object",
        "table",
        "Lua state ready",
        "network",
        RequiresRe,
        "wiki HTTP Requests",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "Proxy raises explicit errors; no network implementation in this milestone."
    ),
    e!(
        "REQUEST",
        "table object",
        "table",
        "Lua state ready",
        "network",
        RequiresRe,
        "wiki HTTP Requests",
        "lua_compat::tests::unsupported_events_and_memory_are_explicit",
        "Proxy raises explicit errors."
    ),
    e!(
        "LE.gameplay_attribulator_manager",
        "manager object",
        "error",
        "Lua state ready",
        "gameplay",
        RequiresRe,
        "wiki Gameplay Attribulator",
        "n/a",
        "Not implemented; no manager object is created."
    ),
    e!(
        "LE.player_development_manager",
        "manager object",
        "error",
        "Lua state ready",
        "career",
        RequiresRe,
        "wiki Player Development",
        "n/a",
        "Not implemented; no manager object is created."
    ),
    e!(
        "LE.aardvark_manager",
        "manager object",
        "error",
        "Lua state ready",
        "career",
        RequiresRe,
        "wiki Aardvark",
        "n/a",
        "Not implemented; no manager object is created."
    ),
    e!(
        "LE.game_localization_manager",
        "manager object",
        "error",
        "Lua state ready",
        "localization",
        RequiresRe,
        "wiki Game Localization",
        "n/a",
        "Not implemented; no manager object is created."
    ),
    e!(
        "LE.players_manager",
        "manager object",
        "error",
        "Lua state ready",
        "career",
        RequiresRe,
        "local live_editor.lua",
        "n/a",
        "Not implemented; no manager object is created."
    ),
    e!(
        "IsInCM",
        "bool IsInCM()",
        "boolean",
        "Career mode",
        "read",
        SupportedTested,
        "WP8 career core path; live career session",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Uses the verified core object path. The proprietary DLL's private event byte is not used."
    ),
    e!(
        "GetCurrentDate",
        "table GetCurrentDate()",
        "table",
        "Career mode",
        "read",
        SupportedTested,
        "WP8 manager type 24 calendar object; live career session",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Reads +0x34/+0x38/+0x3C; outside career returns the documented 1 July 2021 default."
    ),
    e!(
        "GetPlayerPrimaryPositionName",
        "string GetPlayerPrimaryPositionName(int)",
        "string",
        "Lua state ready",
        "read",
        SupportedTested,
        "WP8 independent 0..27 position map",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Independent compatibility mapping; invalid IDs return INVALID."
    ),
    e!(
        "GetSaveUID",
        "string GetSaveUID()",
        "error",
        "Career mode",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Not implemented in this milestone."
    ),
    e!(
        "GetCompetitionNameByID",
        "string GetCompetitionNameByID(int)",
        "error",
        "Lua state ready",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Requires a verified competition-id map; not needed by the season exporter."
    ),
    e!(
        "GetCompetitionNameByObjID",
        "string GetCompetitionNameByObjID(int)",
        "string",
        "Lua state ready",
        "read",
        PartiallySupported,
        "WP8 live active-competition tags + onMQ league table; optional id_map.json",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Domestic leagues resolve from the game's own table. Cups and unknown IDs use the documented COBJ<id> fallback; an id_map.json overrides both."
    ),
    e!(
        "GetTeamIdFromPlayerId",
        "int GetTeamIdFromPlayerId(int)",
        "integer",
        "Lua state ready",
        "read",
        PartiallySupported,
        "WP8 season-stat team records",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Derived from the player's season rows; returns 0 when no row exists. The original career-player object path is not called."
    ),
    e!(
        "GetTeamName",
        "string GetTeamName(int)",
        "string",
        "Lua state ready",
        "read",
        PartiallySupported,
        "WP8 live tWYe mCXg/vIys team table; 703 validated names",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Clubs resolve from the game's own table. Missing/national team IDs use TEAM<id>."
    ),
    e!(
        "GetPlayerName",
        "string GetPlayerName(int)",
        "string",
        "Lua state ready",
        "read",
        SupportedTested,
        "WP5 playernames/dcplayernames/editedplayernames resolver; live 21814/21814",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Missing IDs use the original API's PLAYER_<id> fallback."
    ),
    e!(
        "GetPlayersStats",
        "array GetPlayersStats()",
        "array of tables",
        "Career mode",
        "read",
        SupportedTested,
        "WP8 live statistics vector; 22939 valid records in the tested session",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Reads the current career vector only. Competition names are partial as described above; no statistic is inferred."
    ),
    e!(
        "GetPlayerStats",
        "array GetPlayerStats(int)",
        "array of tables",
        "Career mode",
        "read",
        SupportedTested,
        "WP8 live statistics vector, filtered by playerid",
        "lua_compat::tests::career_apis_read_fixture_data",
        "Same fields and limitations as GetPlayersStats."
    ),
    e!(
        "PlayerExists",
        "bool PlayerExists(int)",
        "error",
        "Lua state ready",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Can be added later via the verified players snapshot."
    ),
    e!(
        "GetDBTablesNames",
        "array GetDBTablesNames()",
        "error",
        "Lua state ready",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "LE.db:GetTableNames is the supported read-only equivalent."
    ),
    e!(
        "GetDBTableFields",
        "array GetDBTableFields(string)",
        "error",
        "Lua state ready",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Use table:GetField(s) instead."
    ),
    e!(
        "GetDBTableRows",
        "array GetDBTableRows(string)",
        "error",
        "Lua state ready",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Use LE.db:GetTable and iteration instead."
    ),
    e!(
        "SetPlayerSharpness",
        "void SetPlayerSharpness(int, int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "SetPlayerMorale",
        "void SetPlayerMorale(int, int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "SetPlayerForm",
        "void SetPlayerForm(int, int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "SetPlayerFitness",
        "void SetPlayerFitness(int, int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "PlayerHasDevelopementPlan",
        "bool PlayerHasDevelopementPlan(int)",
        "error",
        "Career mode",
        "read",
        RequiresRe,
        "wiki LUA-API",
        "n/a",
        "Development-plan table not verified."
    ),
    e!(
        "PlayerSetValueInDevelopementPlan",
        "void PlayerSetValueInDevelopementPlan(int, string, int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "DeletePlayer",
        "void DeletePlayer(int, int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "CreatePlayer",
        "int CreatePlayer(int, table)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "TransferPlayer",
        "void TransferPlayer(...)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "cTransferPlayer",
        "void cTransferPlayer(...)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "LoanPlayer",
        "void LoanPlayer(...)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "cLoanPlayer",
        "void cLoanPlayer(...)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "ReleasePlayerFromTeam",
        "void ReleasePlayerFromTeam(int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "TerminateLoan",
        "void TerminateLoan(int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "DeletePresignedContract",
        "void DeletePresignedContract(int)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "IsPlayerLoanedOut",
        "bool IsPlayerLoanedOut(int)",
        "error",
        "Career mode",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Career loan tables not implemented."
    ),
    e!(
        "IsPlayerPresigned",
        "bool IsPlayerPresigned(int)",
        "error",
        "Career mode",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Career presigned-contract table not implemented."
    ),
    e!(
        "IsPlayerTransferListed",
        "bool IsPlayerTransferListed(int, int=0)",
        "error",
        "Career mode",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Career list state not implemented."
    ),
    e!(
        "IsPlayerLoanListed",
        "bool IsPlayerLoanListed(int, int=0)",
        "error",
        "Career mode",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Career list state not implemented."
    ),
    e!(
        "AddPlayerToTransferList",
        "void AddPlayerToTransferList(int, int=0)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "AddPlayerToLoanList",
        "void AddPlayerToLoanList(int, int=0)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "RemovePlayerFromTransferList",
        "void RemovePlayerFromTransferList(int, int=0)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "RemovePlayerFromLoanList",
        "void RemovePlayerFromLoanList(int, int=0)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "RemovePlayerFromLists",
        "void RemovePlayerFromLists(int, int=0)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "InsertDBTableRow",
        "DBRow InsertDBTableRow(string, table)",
        "error",
        "Lua state ready",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "DeleteDBTableRow",
        "void DeleteDBTableRow(string, int)",
        "error",
        "Lua state ready",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "EditDBTableField",
        "void EditDBTableField(...)",
        "error",
        "Lua state ready",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "SetTransferBudget",
        "void SetTransferBudget(...)",
        "error",
        "Career mode",
        "write",
        Unimplemented,
        "wiki LUA-API",
        "lua_compat::tests::write_operations_are_explicitly_rejected",
        "Read-only milestone: explicit error, no memory write."
    ),
    e!(
        "GetTransferBudget",
        "int GetTransferBudget(...)",
        "error",
        "Career mode",
        "read",
        Unimplemented,
        "wiki LUA-API",
        "n/a",
        "Career budget table not verified."
    ),
    e!(
        "PlayerCapture*",
        "multiple functions",
        "error",
        "Game capture",
        "game",
        NotApplicable,
        "wiki LUA-API",
        "n/a",
        "Screen capture/game-function integration is outside FC27LE's scope."
    ),
];

pub fn compatibility_matrix() -> &'static [ApiEntry] {
    API_MATRIX
}

pub fn status_counts() -> (usize, usize, usize, usize) {
    let mut supported = 0;
    let mut partial = 0;
    let mut unimplemented = 0;
    let mut requires_re = 0;
    for entry in API_MATRIX {
        match entry.status {
            ApiStatus::SupportedTested => supported += 1,
            ApiStatus::PartiallySupported => partial += 1,
            ApiStatus::Unimplemented | ApiStatus::NotApplicable => unimplemented += 1,
            ApiStatus::RequiresRe => requires_re += 1,
        }
    }
    (supported, partial, unimplemented, requires_re)
}
