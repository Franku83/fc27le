# FC Live Editor Lua API v2 compatibility

This document is generated from `crates/lua-compat/src/matrix.rs`.
Regenerate it with:

```bash
cargo run -p fc27-lua-compat --example matrix > docs/lua_compatibility.md
```

## Status summary

| Status | Entries |
| --- | ---: |
| Supported and tested | 55 |
| Partially supported | 5 |
| Unimplemented / not applicable | 37 |
| Requires further reverse engineering | 15 |

The runtime is deliberately read-only. Every unsupported call raises an
explicit `FC27LE compatibility:` error. No call silently succeeds, invents
data, reports a fake event handler, or pretends a memory write occurred.

## Tested environment

* FC 27 `FC27.exe`, SHA-256
  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`,
  running under GE-Proton11-12.
* External FC 27 metadata XML (not redistributed): 268/268 metadata
  tables matched and 263 fixed-row layouts verified. The embedded
  profile covers 4 tables and revalidates its listed fields live.
* Live `players` table: 4CC `CZUM`, 152 fields, 156-byte records,
  22,098 slots, 27,000 capacity, 284 invalid markers, 21,814 valid rows.
* With external XML, the Lua runtime exposed 268 tables whose metadata
  matched the live descriptors. The embedded minimal profile exposes 4
  tables (`players` plus the three name tables). `LOGGER` output and
  `MessageBox` are shown in the GUI.

## Compatibility matrix

| API | Signature | Returns | Required state | Capability | Status | Evidence | Tests | Known limitations |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `LE_VERSION` | `string global` | string | Lua state ready | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | FC27LE version string, not the original Live Editor version. |
| `LE` | `table` | table | Lua state ready | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | Only the fields implemented below are present. |
| `LE.version` | `string field` | string | Lua state ready | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | Compatibility layer version. |
| `LE.data` | `string field` | string | Lua state ready | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | Empty unless the caller supplies a data path. |
| `LE.game_base` | `integer field` | integer | Connected | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | FC27.exe module base. |
| `LE.game_size` | `integer field` | integer | Connected | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | FC27.exe module size. |
| `LE.game_name` | `string field` | string | Connected | read | Supported and tested | WP6 Lua runtime | `lua_compat::tests::le_version_and_db_lookup` | FC27.exe. |
| `LE.db` | `table object` | table | Connected + metadata | read | Supported and tested | local lua/libs/v2/imports/t3db/db.lua | `lua_compat::tests::le_version_and_db_lookup` | Read-only database facade. |
| `LE.db.tables_count` | `integer field` | integer | Connected + metadata | read | Partially supported | local db.lua, t3db.lua | `lua_compat::tests::le_version_and_db_lookup` | Reports the active profile table count: 4 in the embedded profile, 268 with external XML in the tested build. |
| `LE.db:GetTable` | `table GetTable(string name)` | table\|nil | Connected + metadata | read | Supported and tested | local db.lua | `lua_compat::tests::le_version_and_db_lookup` | Unknown names return nil, matching Live Editor. |
| `LE.db:GetTableByShortname` | `table GetTableByShortname(string 4cc)` | table\|nil | Connected + metadata | read | Supported and tested | local db.lua | `lua_compat::tests::le_version_and_db_lookup` | Unknown 4CC returns nil. |
| `LE.db:GetTableNames` | `table GetTableNames()` | array<string> | Connected + metadata | read | Supported and tested | WP6 addition | `lua_compat::tests::le_version_and_db_lookup` | FC27LE convenience; not present in every Live Editor version. |
| `LE.db:Refresh` | `void Refresh()` | none | Connected + metadata | read | Supported and tested | local db.lua Reset semantics | `lua_compat::tests::refresh_invalidates_old_handles` | Clears snapshots and increments the handle generation. |
| `LE.db:Reset` | `void Reset()` | none | Connected + metadata | read | Supported and tested | local db.lua | `lua_compat::tests::refresh_invalidates_old_handles` | Alias for Refresh. |
| `LE.db:Load` | `void Load()` | none | Connected + metadata | read | Supported and tested | local db.lua | `lua_compat::tests::refresh_invalidates_old_handles` | Alias for Refresh. |
| `table.name` | `string field` | string | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::le_version_and_db_lookup` | Metadata name. |
| `table.shortname` | `string field` | string | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::le_version_and_db_lookup` | 4CC descriptor code. |
| `table.written_records` | `integer field` | integer | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::le_version_and_db_lookup` | Slot high-water mark from descriptor +0x7C. |
| `table.record_size` | `integer field` | integer | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::le_version_and_db_lookup` | Bytes per slot. |
| `table.last_record_idx` | `integer field` | integer | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::le_version_and_db_lookup` | written_records - 1. |
| `table.current_record_idx` | `integer field` | integer | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::iteration_reads_valid_records_and_skips_invalid` | Current cursor; updated by iteration. |
| `table:GetFirstRecord` | `number GetFirstRecord()` | number | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::iteration_reads_valid_records_and_skips_invalid` | Returns a positive numeric token or 0. Tokens are not process pointers. |
| `table:GetNextValidRecord` | `number GetNextValidRecord()` | number | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::iteration_reads_valid_records_and_skips_invalid` | Skips invalid markers; returns 0 at end. |
| `table:GetNextRecord` | `number GetNextRecord()` | number | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::iteration_reads_valid_records_and_skips_invalid` | Advances even over invalid slots. |
| `table:GetRecordFieldValue` | `value GetRecordFieldValue(number record, string field)` | integer\|float\|string | GetTable succeeded + valid handle | read | Partially supported | local t3db/table.lua + field.lua | `lua_compat::tests::field_lookup_and_read` | Integer/date, float and inline string fields work. Compressed string fields (type 13) raise an explicit error. |
| `table:SetRecordFieldValue` | `void SetRecordFieldValue(number record, string field, value)` | error | GetTable succeeded | write | Unimplemented | read-only milestone | `lua_compat::tests::write_operations_are_explicitly_rejected` | Raises a compatibility error; no memory is modified. |
| `table:GetField` | `field GetField(string name)` | field\|nil | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::field_lookup_and_read` | Unknown field returns nil, matching the Lua layer. |
| `table:GetFieldsCount` | `integer GetFieldsCount()` | integer | GetTable succeeded | read | Supported and tested | local t3db/table.lua | `lua_compat::tests::le_version_and_db_lookup` | Field entry count. |
| `table:GetName` | `string GetName()` | string | GetTable succeeded | read | Supported and tested | WP6 convenience | `lua_compat::tests::le_version_and_db_lookup` | Metadata name. |
| `table:GetShortName` | `string GetShortName()` | string | GetTable succeeded | read | Supported and tested | WP6 convenience | `lua_compat::tests::le_version_and_db_lookup` | 4CC code. |
| `table:GetRecordSize` | `integer GetRecordSize()` | integer | GetTable succeeded | read | Supported and tested | WP6 convenience | `lua_compat::tests::le_version_and_db_lookup` | Bytes per slot. |
| `table:GetWrittenRecords` | `integer GetWrittenRecords()` | integer | GetTable succeeded | read | Supported and tested | WP6 convenience | `lua_compat::tests::le_version_and_db_lookup` | Slot count. |
| `table:GetLastRecordIdx` | `integer GetLastRecordIdx()` | integer | GetTable succeeded | read | Supported and tested | WP6 convenience | `lua_compat::tests::le_version_and_db_lookup` | Slot count - 1. |
| `table:GetCurrentRecordIdx` | `integer GetCurrentRecordIdx()` | integer | GetTable succeeded | read | Supported and tested | WP6 convenience | `lua_compat::tests::iteration_reads_valid_records_and_skips_invalid` | Current cursor. |
| `field.name` | `string field` | string | GetField succeeded | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Metadata field name. |
| `field.shortname` | `string field` | string | GetField succeeded | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Field 4CC. |
| `field.type` | `integer field` | integer | GetField succeeded | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Live field type (0/3/4/13). |
| `field.offset` | `integer field` | integer | GetField succeeded | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Byte offset. |
| `field.startbit` | `integer field` | integer | GetField succeeded | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Bit offset inside the byte. |
| `field.sz` | `integer field` | integer | GetField succeeded | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Bit width. |
| `field.type_name` | `string field` | string | GetField succeeded | read | Supported and tested | metadata schema | `lua_compat::tests::field_lookup_and_read` | Metadata type name. |
| `field:GetValue` | `value GetValue(number record)` | integer\|float\|string | GetField succeeded + valid handle | read | Supported and tested | local t3db/field.lua | `lua_compat::tests::field_lookup_and_read` | Same decoder as GetRecordFieldValue. |
| `field:SetValue` | `void SetValue(number record, value)` | error | GetField succeeded | write | Unimplemented | read-only milestone | `lua_compat::tests::write_operations_are_explicitly_rejected` | Raises a compatibility error; no memory is modified. |
| `LOGGER` | `table object` | table | Lua state ready | ui | Supported and tested | wiki LUA-API-v2-LOGGER | `lua_compat::tests::logger_print_messagebox_and_script_error` | Writes to the GUI Lua console. |
| `LOGGER:LogDebug` | `void LogDebug(string)` | none | Lua state ready | ui | Supported and tested | wiki LOGGER | `lua_compat::tests::logger_print_messagebox_and_script_error` | GUI log only. |
| `LOGGER:LogInfo` | `void LogInfo(string)` | none | Lua state ready | ui | Supported and tested | wiki LOGGER | `lua_compat::tests::logger_print_messagebox_and_script_error` | GUI log only. |
| `LOGGER:LogWarn` | `void LogWarn(string)` | none | Lua state ready | ui | Supported and tested | wiki LOGGER | `lua_compat::tests::logger_print_messagebox_and_script_error` | GUI log only. |
| `LOGGER:LogWarning` | `void LogWarning(string)` | none | Lua state ready | ui | Supported and tested | WP6 alias | `lua_compat::tests::logger_print_messagebox_and_script_error` | Alias of LogWarn. |
| `LOGGER:LogError` | `void LogError(string)` | none | Lua state ready | ui | Supported and tested | wiki LOGGER | `lua_compat::tests::logger_print_messagebox_and_script_error` | GUI log only. |
| `LOGGER:LogFatal` | `void LogFatal(string)` | none | Lua state ready | ui | Supported and tested | wiki LOGGER | `lua_compat::tests::logger_print_messagebox_and_script_error` | GUI log only. |
| `MessageBox` | `void MessageBox(string title, string text)` | none | Lua state ready | ui | Supported and tested | wiki LUA-API | `lua_compat::tests::logger_print_messagebox_and_script_error` | Shown as a GUI modal/log event, not a native Win32 dialog. |
| `FC27LE_EnsureDirectory` | `bool FC27LE_EnsureDirectory(string path)` | bool | Lua state ready | filesystem | Supported and tested | WP10 player-field and export helpers | `lua_compat::player_fields::player_field_helpers_and_export_path_are_first_party` | FC27LE extension, not a Live Editor API. Creates one directory selected by the script; no game memory or file writes. |
| `print` | `void print(...)` | none | Lua state ready | ui | Supported and tested | Lua 5.4 | `lua_compat::tests::logger_print_messagebox_and_script_error` | Captured into the GUI log. |
| `AddEventHandler` | `void AddEventHandler(string, function)` | error | Lua state ready | event | Requires further reverse engineering | wiki LUA-API-v2-Events | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | No verified game-event source; raises an explicit error and registers nothing. |
| `GetEventHandlers` | `Event[] GetEventHandlers(string)` | error | Lua state ready | event | Requires further reverse engineering | wiki Events | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | No event registry is exposed. |
| `RemoveEventHandler` | `void RemoveEventHandler(string, int)` | error | Lua state ready | event | Requires further reverse engineering | wiki Events | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | No event registry is exposed. |
| `ClearEventHandlersForEvent` | `void ClearEventHandlersForEvent(string)` | error | Lua state ready | event | Requires further reverse engineering | wiki Events | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | No event registry is exposed. |
| `LEInitDoneEvent` | `game event` | event | Live Editor startup | event | Requires further reverse engineering | wiki Events | `n/a` | FC27LE creates the Lua state on demand; no startup event source. |
| `CareerModeEvent` | `game event` | event | Career game function | event | Requires further reverse engineering | wiki Events | `n/a` | Requires game-function hooking, which is explicitly out of scope. |
| `MEMORY` | `table object` | table | Lua state ready | read/write | Requires further reverse engineering | wiki LUA-API-v2-MEMORY | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | Proxy raises explicit errors for every member; raw memory access is not exposed. |
| `HTTP` | `table object` | table | Lua state ready | network | Requires further reverse engineering | wiki HTTP Requests | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | Proxy raises explicit errors; no network implementation in this milestone. |
| `REQUEST` | `table object` | table | Lua state ready | network | Requires further reverse engineering | wiki HTTP Requests | `lua_compat::tests::unsupported_events_and_memory_are_explicit` | Proxy raises explicit errors. |
| `LE.gameplay_attribulator_manager` | `manager object` | error | Lua state ready | gameplay | Requires further reverse engineering | wiki Gameplay Attribulator | `n/a` | Not implemented; no manager object is created. |
| `LE.player_development_manager` | `manager object` | error | Lua state ready | career | Requires further reverse engineering | wiki Player Development | `n/a` | Not implemented; no manager object is created. |
| `LE.aardvark_manager` | `manager object` | error | Lua state ready | career | Requires further reverse engineering | wiki Aardvark | `n/a` | Not implemented; no manager object is created. |
| `LE.game_localization_manager` | `manager object` | error | Lua state ready | localization | Requires further reverse engineering | wiki Game Localization | `n/a` | Not implemented; no manager object is created. |
| `LE.players_manager` | `manager object` | error | Lua state ready | career | Requires further reverse engineering | local live_editor.lua | `n/a` | Not implemented; no manager object is created. |
| `IsInCM` | `bool IsInCM()` | boolean | Career mode | read | Supported and tested | WP8 career core path; live career session | `lua_compat::tests::career_apis_read_fixture_data` | Uses the verified core object path. The proprietary DLL's private event byte is not used. |
| `GetCurrentDate` | `table GetCurrentDate()` | table | Career mode | read | Supported and tested | WP8 manager type 24 calendar object; live career session | `lua_compat::tests::career_apis_read_fixture_data` | Reads +0x34/+0x38/+0x3C; outside career returns the documented 1 July 2021 default. |
| `GetPlayerPrimaryPositionName` | `string GetPlayerPrimaryPositionName(int)` | string | Lua state ready | read | Supported and tested | WP8 independent 0..27 position map | `lua_compat::tests::career_apis_read_fixture_data` | Independent compatibility mapping; invalid IDs return INVALID. |
| `GetSaveUID` | `string GetSaveUID()` | error | Career mode | read | Unimplemented | wiki LUA-API | `n/a` | Not implemented in this milestone. |
| `GetCompetitionNameByID` | `string GetCompetitionNameByID(int)` | error | Lua state ready | read | Unimplemented | wiki LUA-API | `n/a` | Requires a verified competition-id map; not needed by the season exporter. |
| `GetCompetitionNameByObjID` | `string GetCompetitionNameByObjID(int)` | string | Lua state ready | read | Partially supported | WP8 live active-competition tags + onMQ league table; optional id_map.json | `lua_compat::tests::career_apis_read_fixture_data` | Domestic leagues resolve from the game's own table. Cups and unknown IDs use the documented COBJ<id> fallback; an id_map.json overrides both. |
| `GetTeamIdFromPlayerId` | `int GetTeamIdFromPlayerId(int)` | integer | Lua state ready | read | Partially supported | WP8 season-stat team records | `lua_compat::tests::career_apis_read_fixture_data` | Derived from the player's season rows; returns 0 when no row exists. The original career-player object path is not called. |
| `GetTeamName` | `string GetTeamName(int)` | string | Lua state ready | read | Partially supported | WP8 live tWYe mCXg/vIys team table; 703 validated names | `lua_compat::tests::career_apis_read_fixture_data` | Clubs resolve from the game's own table. Missing/national team IDs use TEAM<id>. |
| `GetPlayerName` | `string GetPlayerName(int)` | string | Lua state ready | read | Supported and tested | WP5 playernames/dcplayernames/editedplayernames resolver; live 21814/21814 | `lua_compat::tests::career_apis_read_fixture_data` | Missing IDs use the original API's PLAYER_<id> fallback. |
| `GetPlayersStats` | `array GetPlayersStats()` | array of tables | Career mode | read | Supported and tested | WP8 live statistics vector; 22939 valid records in the tested session | `lua_compat::tests::career_apis_read_fixture_data` | Reads the current career vector only. Competition names are partial as described above; no statistic is inferred. |
| `GetPlayerStats` | `array GetPlayerStats(int)` | array of tables | Career mode | read | Supported and tested | WP8 live statistics vector, filtered by playerid | `lua_compat::tests::career_apis_read_fixture_data` | Same fields and limitations as GetPlayersStats. |
| `PlayerExists` | `bool PlayerExists(int)` | error | Lua state ready | read | Unimplemented | wiki LUA-API | `n/a` | Can be added later via the verified players snapshot. |
| `GetDBTablesNames` | `array GetDBTablesNames()` | error | Lua state ready | read | Unimplemented | wiki LUA-API | `n/a` | LE.db:GetTableNames is the supported read-only equivalent. |
| `GetDBTableFields` | `array GetDBTableFields(string)` | error | Lua state ready | read | Unimplemented | wiki LUA-API | `n/a` | Use table:GetField(s) instead. |
| `GetDBTableRows` | `array GetDBTableRows(string)` | error | Lua state ready | read | Unimplemented | wiki LUA-API | `n/a` | Use LE.db:GetTable and iteration instead. |
| `SetPlayerSharpness` | `void SetPlayerSharpness(int, int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `SetPlayerMorale` | `void SetPlayerMorale(int, int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `SetPlayerForm` | `void SetPlayerForm(int, int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `SetPlayerFitness` | `void SetPlayerFitness(int, int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `PlayerHasDevelopementPlan` | `bool PlayerHasDevelopementPlan(int)` | error | Career mode | read | Requires further reverse engineering | wiki LUA-API | `n/a` | Development-plan table not verified. |
| `PlayerSetValueInDevelopementPlan` | `void PlayerSetValueInDevelopementPlan(int, string, int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `DeletePlayer` | `void DeletePlayer(int, int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `CreatePlayer` | `int CreatePlayer(int, table)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `TransferPlayer` | `void TransferPlayer(...)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `cTransferPlayer` | `void cTransferPlayer(...)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `LoanPlayer` | `void LoanPlayer(...)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `cLoanPlayer` | `void cLoanPlayer(...)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `ReleasePlayerFromTeam` | `void ReleasePlayerFromTeam(int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `TerminateLoan` | `void TerminateLoan(int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `DeletePresignedContract` | `void DeletePresignedContract(int)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `IsPlayerLoanedOut` | `bool IsPlayerLoanedOut(int)` | error | Career mode | read | Unimplemented | wiki LUA-API | `n/a` | Career loan tables not implemented. |
| `IsPlayerPresigned` | `bool IsPlayerPresigned(int)` | error | Career mode | read | Unimplemented | wiki LUA-API | `n/a` | Career presigned-contract table not implemented. |
| `IsPlayerTransferListed` | `bool IsPlayerTransferListed(int, int=0)` | error | Career mode | read | Unimplemented | wiki LUA-API | `n/a` | Career list state not implemented. |
| `IsPlayerLoanListed` | `bool IsPlayerLoanListed(int, int=0)` | error | Career mode | read | Unimplemented | wiki LUA-API | `n/a` | Career list state not implemented. |
| `AddPlayerToTransferList` | `void AddPlayerToTransferList(int, int=0)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `AddPlayerToLoanList` | `void AddPlayerToLoanList(int, int=0)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `RemovePlayerFromTransferList` | `void RemovePlayerFromTransferList(int, int=0)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `RemovePlayerFromLoanList` | `void RemovePlayerFromLoanList(int, int=0)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `RemovePlayerFromLists` | `void RemovePlayerFromLists(int, int=0)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `InsertDBTableRow` | `DBRow InsertDBTableRow(string, table)` | error | Lua state ready | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `DeleteDBTableRow` | `void DeleteDBTableRow(string, int)` | error | Lua state ready | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `EditDBTableField` | `void EditDBTableField(...)` | error | Lua state ready | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `SetTransferBudget` | `void SetTransferBudget(...)` | error | Career mode | write | Unimplemented | wiki LUA-API | `lua_compat::tests::write_operations_are_explicitly_rejected` | Read-only milestone: explicit error, no memory write. |
| `GetTransferBudget` | `int GetTransferBudget(...)` | error | Career mode | read | Unimplemented | wiki LUA-API | `n/a` | Career budget table not verified. |
| `PlayerCapture*` | `multiple functions` | error | Game capture | game | Not applicable | wiki LUA-API | `n/a` | Screen capture/game-function integration is outside FC27LE's scope. |

## Record handles

`GetFirstRecord` and `GetNextValidRecord` return positive numeric tokens,
not raw process pointers. A token encodes `(generation, table_id, slot+1)`.
Iteration returns `0` at the end. `LE.db:Refresh`, `Load` and `Reset`
invalidate every existing table/field/record handle; using one afterwards
raises a stale-handle error and requires a fresh `GetTable` call. This keeps
script-visible handles stable without exposing game addresses.

A `players` snapshot is immutable for its lifetime. A game-side database
change becomes visible only after the script calls `LE.db:Refresh` and
obtains a new table handle.

## Execution model

* Lua 5.4 is provided by `mlua` with `lua54` and `vendored` features.
* Scripts run on a background worker; the GUI render thread only polls
  events. Run and Stop remain interactive, and the window keeps repainting.
* Default limits: 30 s wall-clock time, 256 MiB Lua memory. A debug hook
  checks both every 10,000 instructions; Stop sets a cancellation flag.
  Live GUI test: Stop cancelled `while true do end` with
  `FC27LE: script cancelled` while the window stayed responsive.
* Helper modules are loaded from semicolon-separated directories configured
  in the GUI plus the open script's own directory.
* `os.execute`, `io.popen` and `package.loadlib` are removed. Standard Lua
  file I/O remains available because existing scripts may use it. Third-party
  scripts are untrusted; Lua 5.4 is **not** a security sandbox.
* Unsupported `MEMORY`, `HTTP` and `REQUEST` members raise compatibility
  errors. Raw process memory and network access are not exposed.

## Verified live read-only test

The GUI executed this read-only script against the live process:

```lua
local players = LE.db:GetTable("players")
local record = players:GetFirstRecord()
local count = 0
while record > 0 do
    local playerid = players:GetRecordFieldValue(record, "playerid")
    local overall = players:GetRecordFieldValue(record, "overallrating")
    count = count + 1
    record = players:GetNextValidRecord()
end
MessageBox("FC27LE", "Scanned " .. count .. " players")
```

Result: table `players` (`CZUM`) reported 22,098 written records; iteration
visited 21,814 valid records; the GUI logged the first IDs and displayed
`Scanned 21814 valid player records`. The same run was repeated with F5 and
with the GUI Run button while the window stayed responsive.

## Out of scope

No game-memory writes, DLL injection, hooks, game-function calls, DRM or
anti-cheat changes are implemented. Career-mode managers, write helpers,
game events and networking remain explicit compatibility errors until a
verified, read-only source exists.
