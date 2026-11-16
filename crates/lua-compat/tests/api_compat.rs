use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use fc27_game::table_access::{FieldInfo, TableSnapshot, TableSnapshotInfo};
use fc27_lua_compat::api::{
    FixtureTableProvider, LogLevel, LuaEvent, ScriptJob, build_fixture_context, run_script,
};

#[allow(clippy::too_many_arguments)]
fn field(
    name: &str,
    shortname: &str,
    bit_offset: u32,
    bit_width: u32,
    low: i64,
    high: i64,
    field_type: u32,
    type_name: &str,
) -> FieldInfo {
    FieldInfo {
        name: name.to_string(),
        shortname: shortname.to_string(),
        bit_offset,
        bit_width,
        low,
        high,
        field_type,
        type_name: type_name.to_string(),
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

fn write_string(row: &mut [u8], bit_offset: u32, value: &str) {
    let start = (bit_offset / 8) as usize;
    row[start..start + value.len()].copy_from_slice(value.as_bytes());
}

fn fixture() -> FixtureTableProvider {
    let fields = vec![
        field(
            "playerid",
            "ykFq",
            0,
            16,
            0,
            65535,
            3,
            "DBOFIELDTYPE_INTEGER",
        ),
        field(
            "overallrating",
            "UERs",
            16,
            7,
            1,
            99,
            3,
            "DBOFIELDTYPE_INTEGER",
        ),
        field(
            "firstnameid",
            "tHlO",
            23,
            16,
            0,
            65535,
            3,
            "DBOFIELDTYPE_INTEGER",
        ),
        field("teamname", "tnm1", 40, 40, 0, 0, 0, "DBOFIELDTYPE_STRING"),
    ];
    let mut data = vec![0u8; 36];
    for (index, (id, ovr, first, name, invalid)) in [
        (20801u64, 84u64, 6183u64, "Alpha", false),
        (11111, 88, 22780, "Beta", true),
        (45678, 91, 21499, "Gamma", false),
    ]
    .into_iter()
    .enumerate()
    {
        let row = &mut data[index * 12..(index + 1) * 12];
        write_bits(row, 0, 16, id);
        write_bits(row, 16, 7, ovr - 1);
        write_bits(row, 23, 16, first);
        write_string(row, 40, name);
        if invalid {
            row[11] |= 0x80;
        }
    }
    let snapshot = TableSnapshot::from_parts(
        TableSnapshotInfo {
            table_id: 1,
            table_name: "players".to_string(),
            shortname: "TEST".to_string(),
            descriptor: 0,
            data_pointer: 0,
            record_size: 12,
            slot_count: 3,
            capacity: 3,
            invalid_marker_count: 1,
            field_count: 4,
        },
        fields,
        data,
    )
    .unwrap();
    FixtureTableProvider::new(vec![Arc::new(snapshot)])
}

fn run(source: &str) -> Result<(fc27_lua_compat::api::LuaContext, Vec<LuaEvent>), String> {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let context =
        build_fixture_context(fixture(), cancel.clone(), tx).map_err(|e| e.to_string())?;
    let job = ScriptJob {
        name: "test".to_string(),
        source: source.to_string(),
        script_dirs: Vec::new(),
        timeout: Duration::from_secs(2),
        memory_limit_bytes: 64 * 1024 * 1024,
    };
    let result = run_script(&context.lua, &job, cancel);
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    result.map(|()| (context, events))
}

fn logs(events: &[LuaEvent]) -> Vec<(LogLevel, String)> {
    events
        .iter()
        .filter_map(|event| match event {
            LuaEvent::Log { level, message } => Some((*level, message.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn le_version_and_db_lookup() {
    let (_ctx, _events) = run(r#"
        assert(type(LE_VERSION) == "string")
        assert(LE.version == LE_VERSION)
        assert(type(LE.data) == "string")
        assert(LE.game_name == "FC27.exe")
        assert(type(LE.game_base) == "number")
        assert(type(LE.game_size) == "number")
        assert(type(LE.db.tables_count) == "number")
        assert(LE.db.tables_count == 1)
        local names = LE.db:GetTableNames()
        assert(#names == 1 and names[1] == "players")
        local t = LE.db:GetTable("players")
        assert(t ~= nil)
        assert(t.name == "players")
        assert(t.shortname == "TEST")
        assert(t.written_records == 3)
        assert(t.record_size == 12)
        assert(t.last_record_idx == 2)
        assert(t.current_record_idx == 0)
        assert(t:GetFieldsCount() == 4)
        assert(t:GetName() == "players")
        assert(t:GetShortName() == "TEST")
        assert(t:GetRecordSize() == 12)
        assert(t:GetWrittenRecords() == 3)
        assert(t:GetLastRecordIdx() == 2)
        assert(t:GetCurrentRecordIdx() == 0)
        assert(LE.db:GetTable("missing") == nil)
        local by_short = LE.db:GetTableByShortname("TEST")
        assert(by_short ~= nil)
        assert(by_short.name == "players")
        "#)
    .unwrap();
}

#[test]
fn iteration_reads_valid_records_and_skips_invalid() {
    let (_ctx, events) = run(r#"
        local t = LE.db:GetTable("players")
        local r = t:GetFirstRecord()
        local count = 0
        local ids = {}
        while r > 0 do
            local id = t:GetRecordFieldValue(r, "playerid")
            local ovr = t:GetRecordFieldValue(r, "overallrating")
            local name = t:GetRecordFieldValue(r, "teamname")
            ids[#ids + 1] = id
            LOGGER:LogInfo(string.format("%d:%d:%s", id, ovr, name))
            count = count + 1
            r = t:GetNextValidRecord()
        end
        assert(count == 2, "expected 2 valid records, got " .. count)
        assert(ids[1] == 20801)
        assert(ids[2] == 45678, string.format("ids1=%s ids2=%s", tostring(ids[1]), tostring(ids[2])))
        assert(t.current_record_idx == 2, tostring(t.current_record_idx))
        assert(t:GetCurrentRecordIdx() == 2)

        -- GetNextRecord advances over invalid slots; GetNextValidRecord skips them.
        local first = t:GetFirstRecord()
        assert(first > 0)
        local second = t:GetNextRecord()
        assert(second > 0, "second handle " .. tostring(second))
        local second_id = t:GetRecordFieldValue(second, "playerid")
        assert(second_id == 11111, "second_id=" .. tostring(second_id))
        local third = t:GetNextValidRecord()
        assert(third > 0, "third handle " .. tostring(third))
        local third_id = t:GetRecordFieldValue(third, "playerid")
        assert(third_id == 45678, "third_id=" .. tostring(third_id))
        "#)
    .unwrap();
    let logs = logs(&events);
    assert!(
        logs.iter()
            .any(|(_, message)| message.contains("20801:84:Alpha"))
    );
    assert!(
        logs.iter()
            .any(|(_, message)| message.contains("45678:91:Gamma"))
    );
    assert!(!logs.iter().any(|(_, message)| message.contains("158023")));
}

#[test]
fn field_lookup_and_read() {
    let (_ctx, _events) = run(r#"
        local t = LE.db:GetTable("players")
        local r = t:GetFirstRecord()
        local f = t:GetField("overallrating")
        assert(f ~= nil)
        assert(f.name == "overallrating")
        assert(f.shortname == "UERs")
        assert(f.type == 3)
        assert(f.offset == 2)
        assert(f.startbit == 0)
        assert(f.sz == 7)
        assert(f.type_name == "DBOFIELDTYPE_INTEGER")
        assert(f:GetValue(r) == 84)
        assert(t:GetRecordFieldValue(r, "firstnameid") == 6183)
        assert(t:GetField("missing") == nil)
        local ok = pcall(function() t:GetRecordFieldValue(r, "missing") end)
        assert(not ok, "missing field must raise an error")
        "#)
    .unwrap();
}

#[test]
fn refresh_invalidates_old_handles() {
    let (_ctx, _events) = run(r#"
        local t = LE.db:GetTable("players")
        local r = t:GetFirstRecord()
        assert(t:GetRecordFieldValue(r, "playerid") == 20801)
        LE.db:Refresh()
        local ok, err = pcall(function() return t:GetRecordFieldValue(r, "playerid") end)
        assert(not ok, "stale handle must fail")
        assert(string.find(tostring(err), "stale"), tostring(err))
        local fresh = LE.db:GetTable("players")
        local fresh_r = fresh:GetFirstRecord()
        assert(fresh:GetRecordFieldValue(fresh_r, "playerid") == 20801)

        LE.db:Load()
        ok, err = pcall(function() return fresh:GetRecordFieldValue(fresh_r, "playerid") end)
        assert(not ok, "Load must invalidate handles like Refresh")
        assert(string.find(tostring(err), "stale"), tostring(err))

        local after_load = LE.db:GetTable("players")
        local after_load_r = after_load:GetFirstRecord()
        assert(after_load:GetRecordFieldValue(after_load_r, "playerid") == 20801)
        LE.db:Reset()
        ok, err = pcall(function() return after_load:GetRecordFieldValue(after_load_r, "playerid") end)
        assert(not ok, "Reset must invalidate handles like Refresh")
        assert(string.find(tostring(err), "stale"), tostring(err))
        "#)
    .unwrap();
}

#[test]
fn write_operations_are_explicitly_rejected() {
    let (_ctx, _events) = run(r#"
        local t = LE.db:GetTable("players")
        local r = t:GetFirstRecord()
        local ok, err = pcall(function() t:SetRecordFieldValue(r, "overallrating", 99) end)
        assert(not ok)
        assert(string.find(tostring(err), "read%-only"), tostring(err))
        local f = t:GetField("overallrating")
        ok, err = pcall(function() f:SetValue(r, 99) end)
        assert(not ok)
        assert(string.find(tostring(err), "read%-only"), tostring(err))
        ok, err = pcall(function() SetPlayerSharpness(20801, 99) end)
        assert(not ok)
        assert(string.find(tostring(err), "read%-only"), tostring(err))
        for _, name in ipairs({
            "SetPlayerMorale", "SetPlayerForm", "SetPlayerFitness",
            "PlayerSetValueInDevelopementPlan", "DeletePlayer", "CreatePlayer",
            "TransferPlayer", "cTransferPlayer", "LoanPlayer", "cLoanPlayer",
            "ReleasePlayerFromTeam", "TerminateLoan", "DeletePresignedContract",
            "AddPlayerToTransferList", "AddPlayerToLoanList",
            "RemovePlayerFromTransferList", "RemovePlayerFromLoanList",
            "RemovePlayerFromLists", "InsertDBTableRow", "DeleteDBTableRow",
            "EditDBTableField", "SetTransferBudget",
        }) do
            assert(type(_G[name]) == "function", name .. " must exist as an explicit stub")
            local stub_ok, stub_err = pcall(_G[name])
            assert(not stub_ok, name .. " must raise")
            assert(string.find(tostring(stub_err), "read%-only"), name .. ": " .. tostring(stub_err))
        end
        "#)
    .unwrap();
}

#[test]
fn unsupported_events_and_memory_are_explicit() {
    let (_ctx, _events) = run(
        r#"
        local ok, err = pcall(function() AddEventHandler("post__CareerModeEvent", function() end) end)
        assert(not ok)
        assert(string.find(tostring(err), "event"), tostring(err))
        for _, name in ipairs({
            "AddEventHandler", "GetEventHandlers", "RemoveEventHandler",
            "ClearEventHandlersForEvent",
        }) do
            assert(type(_G[name]) == "function", name .. " must exist as an explicit stub")
            local stub_ok, stub_err = pcall(_G[name], "post__CareerModeEvent")
            assert(not stub_ok, name .. " must raise")
            assert(string.find(tostring(stub_err), "event"), name .. ": " .. tostring(stub_err))
        end
        ok, err = pcall(function() MEMORY.ReadQword(0) end)
        assert(not ok)
        assert(string.find(tostring(err), "raw process memory"), tostring(err))
        ok, err = pcall(function() HTTP.Get("https://example.invalid") end)
        assert(not ok)
        assert(string.find(tostring(err), "HTTP"), tostring(err))
        ok, err = pcall(function() REQUEST.Get("https://example.invalid") end)
        assert(not ok)
        assert(string.find(tostring(err), "REQUEST"), tostring(err))
        ok, err = pcall(function() return IsInCM() end)
        assert(not ok)
        assert(string.find(tostring(err), "career%-state"), tostring(err))
        for _, name in ipairs({
            "GetTeamIdFromPlayerId", "GetTeamName", "GetPlayerName",
            "GetDBTablesNames", "GetDBTableFields", "GetDBTableRows",
            "PlayerExists", "GetSaveUID", "GetCompetitionNameByID",
            "GetCompetitionNameByObjID", "GetPlayersStats", "GetPlayerStats",
        }) do
            assert(type(_G[name]) == "function", name .. " must exist as an explicit stub")
            local stub_ok, stub_err = pcall(_G[name])
            assert(not stub_ok, name .. " must raise")
        end
        "#,
    )
    .unwrap();
}

#[test]
fn logger_print_messagebox_and_script_error() {
    let (_ctx, events) = run(r#"
        LOGGER:LogInfo("info message")
        LOGGER:LogWarning("warning message")
        LOGGER:LogWarn("warn message")
        LOGGER:LogError("error message")
        LOGGER:LogDebug("debug message")
        LOGGER:LogFatal("fatal message")
        print("printed", 42)
        MessageBox("Title", "Body")
        "#)
    .unwrap();
    let logs = logs(&events);
    assert!(
        logs.iter()
            .any(|(level, m)| *level == LogLevel::Info && m.contains("info message"))
    );
    assert!(
        logs.iter()
            .any(|(level, m)| *level == LogLevel::Warn && m.contains("warning message"))
    );
    assert!(
        logs.iter()
            .any(|(level, m)| *level == LogLevel::Warn && m.contains("warn message"))
    );
    assert!(
        logs.iter()
            .any(|(level, m)| *level == LogLevel::Debug && m.contains("debug message"))
    );
    assert!(
        logs.iter()
            .any(|(level, m)| *level == LogLevel::Fatal && m.contains("fatal message"))
    );
    assert!(
        logs.iter()
            .any(|(level, m)| *level == LogLevel::Error && m.contains("error message"))
    );
    assert!(logs.iter().any(|(_, m)| m.contains("[PRINT] printed\t42")));
    assert!(events.iter().any(|event| matches!(
        event,
        LuaEvent::MessageBox { title, text } if title == "Title" && text == "Body"
    )));

    let (tx, _rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let context = build_fixture_context(fixture(), cancel.clone(), tx).unwrap();
    let job = ScriptJob {
        name: "boom".to_string(),
        source: "error('boom')".to_string(),
        script_dirs: Vec::new(),
        timeout: Duration::from_secs(1),
        memory_limit_bytes: 64 * 1024 * 1024,
    };
    let error = run_script(&context.lua, &job, cancel).unwrap_err();
    assert!(error.contains("boom"));
}

#[test]
fn missing_helper_module_fails() {
    let (_ctx, _events) = run(r#"
        local ok, err = pcall(function() require("definitely_missing_helper") end)
        assert(not ok)
        assert(string.find(tostring(err), "definitely_missing_helper"), tostring(err))
        "#)
    .unwrap();
}

#[test]
fn timeout_and_cancellation_abort_long_scripts() {
    let (tx, _rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let context = build_fixture_context(fixture(), cancel.clone(), tx).unwrap();
    let job = ScriptJob {
        name: "timeout".to_string(),
        source: "while true do end".to_string(),
        script_dirs: Vec::new(),
        timeout: Duration::from_millis(100),
        memory_limit_bytes: 64 * 1024 * 1024,
    };
    let error = run_script(&context.lua, &job, cancel).unwrap_err();
    assert!(error.contains("time limit"), "{error}");

    let cancel2 = Arc::new(AtomicBool::new(false));
    let (tx2, _rx2) = mpsc::channel();
    let context2 = build_fixture_context(fixture(), cancel2.clone(), tx2).unwrap();
    let flag = cancel2.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        flag.store(true, Ordering::SeqCst);
    });
    let job2 = ScriptJob {
        name: "cancel".to_string(),
        source: "while true do end".to_string(),
        script_dirs: Vec::new(),
        timeout: Duration::from_secs(5),
        memory_limit_bytes: 64 * 1024 * 1024,
    };
    let error = run_script(&context2.lua, &job2, cancel2).unwrap_err();
    assert!(error.contains("cancelled"), "{error}");
}

#[test]
fn memory_limit_aborts_large_allocation() {
    let (tx, _rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let context = build_fixture_context(fixture(), cancel.clone(), tx).unwrap();
    let job = ScriptJob {
        name: "memory".to_string(),
        source: "local s = string.rep('x', 8 * 1024 * 1024); return #s".to_string(),
        script_dirs: Vec::new(),
        timeout: Duration::from_secs(2),
        memory_limit_bytes: 2 * 1024 * 1024,
    };
    let error = run_script(&context.lua, &job, cancel).unwrap_err();
    assert!(
        error.to_lowercase().contains("memory"),
        "expected a memory-limit error, got: {error}"
    );
}

#[test]
fn lua_basics_work() {
    let (_ctx, _events) = run(r#"
        assert(2 + 3 * 4 == 14)
        local t = { a = 1, 2, 3 }
        assert(t.a == 1 and #t == 2 and t[2] == 3)
        assert(string.upper("fc27le") == "FC27LE")
        assert(table.concat({ "a", "b" }, "-") == "a-b")
        assert(math.floor(2.9) == 2)
        "#)
    .unwrap();
}
