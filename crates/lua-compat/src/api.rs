//! Lua 5.4 bindings for the read-only FC Live Editor Lua API v2 subset.
//!
//! Record handles are **numeric tokens**, not process pointers. A token
//! encodes `(generation, table_id, slot+1)`; scripts can compare it with `0`
//! exactly like the Live Editor's numeric record handles.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use fc27_core::{Error, Result};
use fc27_game::anchors::ScanOptions;
use fc27_game::db_metadata::{Metadata, validate};
use fc27_game::db_service::{DbTable, inspect};
use fc27_game::table_access::{FieldInfo, FieldValue, TableSnapshot};
use fc27_platform_linux::LinuxProcess;
use fc27_process::{Pid, ProcessHandle, modules::find_module};
use mlua::{
    HookTriggers, Lua, Table, UserData, UserDataFields, UserDataMethods, Value, Variadic, VmState,
};
use serde::Serialize;

pub const API_VERSION: &str = "FC27LE Lua API v2 compatibility (read-only)";
pub const DEFAULT_MAX_DB_MIB: u64 = 160;
pub const DEFAULT_MEMORY_LIMIT_BYTES: usize = 256 * 1024 * 1024;
pub const DEFAULT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ScriptSession {
    pub pid: Pid,
    pub module_name: String,
    pub metadata: Metadata,
    pub data_path: String,
    pub max_db_mib: u64,
}

#[derive(Debug, Clone)]
pub struct ScriptJob {
    pub name: String,
    pub source: String,
    pub script_dirs: Vec<PathBuf>,
    pub timeout: Duration,
    pub memory_limit_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

#[derive(Debug)]
pub enum LuaEvent {
    SessionReady {
        pid: Pid,
        tables: usize,
    },
    SessionError {
        message: String,
    },
    Started {
        name: String,
    },
    Log {
        level: LogLevel,
        message: String,
    },
    MessageBox {
        title: String,
        text: String,
    },
    Finished {
        success: bool,
        error: Option<String>,
    },
}

pub trait TableProvider {
    fn table_names(&self) -> Vec<String>;
    fn table_count(&self) -> usize;
    fn has_table(&self, name: &str) -> bool;
    fn table_by_shortname(&self, shortname: &str) -> Option<String>;
    fn load_table(&mut self, name: &str) -> Result<Arc<TableSnapshot>>;
    fn generation(&self) -> Arc<AtomicU64>;
    fn refresh(&mut self) -> Result<()>;
}

pub struct LiveTableProvider {
    process: LinuxProcess,
    metadata: Metadata,
    live_tables: Vec<DbTable>,
    name_to_id: BTreeMap<String, u16>,
    short_to_name: BTreeMap<String, String>,
    names: Vec<String>,
    cache: BTreeMap<String, Arc<TableSnapshot>>,
    generation: Arc<AtomicU64>,
}

impl LiveTableProvider {
    pub fn new(
        process: LinuxProcess,
        metadata: Metadata,
        live_tables: Vec<DbTable>,
    ) -> Result<Self> {
        let live_short: BTreeSet<&str> = live_tables
            .iter()
            .map(|table| table.shortname.as_str())
            .collect();
        let mut name_to_id = BTreeMap::new();
        let mut short_to_name = BTreeMap::new();
        let mut names = Vec::new();
        for table in &metadata.tables {
            if !live_short.contains(table.shortname.as_str()) {
                continue;
            }
            let id = (name_to_id.len() + 1) as u16;
            name_to_id.insert(table.name.clone(), id);
            short_to_name.insert(table.shortname.clone(), table.name.clone());
            names.push(table.name.clone());
        }
        if names.is_empty() {
            return Err(Error::InsufficientEvidence(
                "no metadata table matches the live table manager".to_string(),
            ));
        }
        Ok(Self {
            process,
            metadata,
            live_tables,
            name_to_id,
            short_to_name,
            names,
            cache: BTreeMap::new(),
            generation: Arc::new(AtomicU64::new(1)),
        })
    }
}

impl TableProvider for LiveTableProvider {
    fn table_names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn table_count(&self) -> usize {
        self.names.len()
    }

    fn has_table(&self, name: &str) -> bool {
        self.name_to_id.contains_key(name)
    }

    fn table_by_shortname(&self, shortname: &str) -> Option<String> {
        self.short_to_name.get(shortname).cloned()
    }

    fn load_table(&mut self, name: &str) -> Result<Arc<TableSnapshot>> {
        if let Some(snapshot) = self.cache.get(name) {
            return Ok(snapshot.clone());
        }
        let id = *self.name_to_id.get(name).ok_or_else(|| {
            Error::InsufficientEvidence(format!("table `{name}` is not available"))
        })?;
        let snapshot = Arc::new(TableSnapshot::load(
            &self.process,
            &self.metadata,
            &self.live_tables,
            name,
            id,
        )?);
        self.cache.insert(name.to_string(), snapshot.clone());
        Ok(snapshot)
    }

    fn generation(&self) -> Arc<AtomicU64> {
        self.generation.clone()
    }

    fn refresh(&mut self) -> Result<()> {
        self.cache.clear();
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

pub struct FixtureTableProvider {
    by_name: BTreeMap<String, Arc<TableSnapshot>>,
    short_to_name: BTreeMap<String, String>,
    names: Vec<String>,
    generation: Arc<AtomicU64>,
}

impl FixtureTableProvider {
    pub fn new(tables: Vec<Arc<TableSnapshot>>) -> Self {
        let mut by_name = BTreeMap::new();
        let mut short_to_name = BTreeMap::new();
        let mut names = Vec::new();
        for table in tables {
            let name = table.info().table_name.clone();
            short_to_name.insert(table.info().shortname.clone(), name.clone());
            names.push(name.clone());
            by_name.insert(name, table);
        }
        Self {
            by_name,
            short_to_name,
            names,
            generation: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl TableProvider for FixtureTableProvider {
    fn table_names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn table_count(&self) -> usize {
        self.names.len()
    }

    fn has_table(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    fn table_by_shortname(&self, shortname: &str) -> Option<String> {
        self.short_to_name.get(shortname).cloned()
    }

    fn load_table(&mut self, name: &str) -> Result<Arc<TableSnapshot>> {
        self.by_name
            .get(name)
            .cloned()
            .ok_or_else(|| Error::InsufficientEvidence(format!("table `{name}` is not available")))
    }

    fn generation(&self) -> Arc<AtomicU64> {
        self.generation.clone()
    }

    fn refresh(&mut self) -> Result<()> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

pub struct LuaContext {
    pub lua: Lua,
    pub pid: Pid,
    pub tables: usize,
}

/// Build a Lua state with a validated live database session.
pub fn build_live_context(
    session: &ScriptSession,
    cancel: Arc<AtomicBool>,
    events: Sender<LuaEvent>,
) -> Result<LuaContext> {
    let process = LinuxProcess::open(session.pid)?;
    let modules = process.modules()?;
    let module = find_module(&modules, &session.module_name)
        .cloned()
        .ok_or_else(|| {
            Error::Unsupported(format!(
                "module {} is not loaded in pid {}",
                session.module_name, session.pid.0
            ))
        })?;
    let options = ScanOptions {
        max_bytes: session.max_db_mib.saturating_mul(1024 * 1024),
        max_matches: 4,
        executable_sections_only: true,
    };
    let report = inspect(&process, &module, options)?;
    if !report.all_checks_passed {
        return Err(Error::InsufficientEvidence(
            "database service path failed verification; Lua database access is gated".to_string(),
        ));
    }
    let validation = validate(&session.metadata, &report.tables, &process, "players")?;
    if !validation.focus_verified {
        return Err(Error::InsufficientEvidence(
            "players descriptor did not validate against the supplied metadata".to_string(),
        ));
    }
    let live_provider = LiveTableProvider::new(process, session.metadata.clone(), report.tables)?;
    let table_count = live_provider.table_count();
    let provider: Rc<RefCell<Box<dyn TableProvider>>> =
        Rc::new(RefCell::new(Box::new(live_provider)));
    let lua = Lua::new();
    register_api(
        &lua,
        provider,
        cancel,
        events,
        &module,
        &session.data_path,
        table_count,
    )?;
    Ok(LuaContext {
        lua,
        pid: session.pid,
        tables: table_count,
    })
}

/// Build a Lua state over an already-constructed fixture provider (tests).
pub fn build_fixture_context(
    provider: FixtureTableProvider,
    cancel: Arc<AtomicBool>,
    events: Sender<LuaEvent>,
) -> Result<LuaContext> {
    let table_count = provider.table_count();
    let provider: Rc<RefCell<Box<dyn TableProvider>>> = Rc::new(RefCell::new(Box::new(provider)));
    let lua = Lua::new();
    let module = fc27_process::ModuleInfo {
        name: "FC27.exe".to_string(),
        path: String::new(),
        base: 0,
        size: 0,
        kind: fc27_process::ModuleKind::Pe,
        pe: None,
    };
    register_api(&lua, provider, cancel, events, &module, "", table_count)?;
    Ok(LuaContext {
        lua,
        pid: Pid(0),
        tables: table_count,
    })
}

/// Execute one script on an existing Lua state.
pub fn run_script(
    lua: &Lua,
    job: &ScriptJob,
    cancel: Arc<AtomicBool>,
) -> std::result::Result<(), String> {
    cancel.store(false, Ordering::SeqCst);
    install_package_path(lua, &job.script_dirs).map_err(|error| error.to_string())?;
    lua.set_memory_limit(job.memory_limit_bytes)
        .map_err(|error| format!("FC27LE: cannot apply the Lua memory limit: {error}"))?;
    let started = Instant::now();
    let timeout = job.timeout;
    let hook_cancel = cancel.clone();
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(10_000),
        move |_lua, _debug| {
            if hook_cancel.load(Ordering::SeqCst) {
                return Err(mlua::Error::RuntimeError(
                    "FC27LE: script cancelled".to_string(),
                ));
            }
            if started.elapsed() > timeout {
                return Err(mlua::Error::RuntimeError(format!(
                    "FC27LE: script exceeded its {:?} time limit",
                    timeout
                )));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|error| error.to_string())?;
    let result = lua
        .load(&job.source)
        .set_name(&job.name)
        .exec()
        .map_err(|error| error.to_string());
    lua.remove_hook();
    result
}

fn install_package_path(lua: &Lua, dirs: &[PathBuf]) -> mlua::Result<()> {
    if let Ok(package) = lua.globals().get::<Table>("package") {
        let mut path = String::new();
        for dir in dirs {
            if !path.is_empty() {
                path.push(';');
            }
            path.push_str(&dir.to_string_lossy());
            path.push_str("/?.lua;");
            path.push_str(&dir.to_string_lossy());
            path.push_str("/?/init.lua");
        }
        if !path.is_empty() {
            path.push(';');
        }
        path.push_str("./?.lua;./?/init.lua");
        package.set("path", path)?;
        package.set("cpath", "")?;
        let _ = package.set("loadlib", Value::Nil);
    }
    Ok(())
}

fn register_api(
    lua: &Lua,
    provider: Rc<RefCell<Box<dyn TableProvider>>>,
    _cancel: Arc<AtomicBool>,
    events: Sender<LuaEvent>,
    module: &fc27_process::ModuleInfo,
    data_path: &str,
    table_count: usize,
) -> Result<()> {
    lua.globals()
        .set("LE_VERSION", API_VERSION)
        .map_err(mlua_to_core)?;
    lua.globals()
        .set("LE_DATA_PATH", data_path)
        .map_err(mlua_to_core)?;
    lua.globals()
        .set("LE_GAME_MODULE_BASE", module.base as i64)
        .map_err(mlua_to_core)?;
    lua.globals()
        .set("LE_GAME_MODULE_SIZE", module.size as i64)
        .map_err(mlua_to_core)?;
    lua.globals()
        .set("LE_GAME_MODULE_NAME", module.name.clone())
        .map_err(mlua_to_core)?;

    let le = lua.create_table().map_err(mlua_to_core)?;
    le.set("version", API_VERSION).map_err(mlua_to_core)?;
    le.set("data", data_path).map_err(mlua_to_core)?;
    le.set("game_base", module.base as i64)
        .map_err(mlua_to_core)?;
    le.set("game_size", module.size as i64)
        .map_err(mlua_to_core)?;
    le.set("game_name", module.name.clone())
        .map_err(mlua_to_core)?;

    let db = lua.create_table().map_err(mlua_to_core)?;
    db.set("tables_count", table_count as i64)
        .map_err(mlua_to_core)?;
    db.set("GetTable", make_get_table(lua, provider.clone())?)
        .map_err(mlua_to_core)?;
    db.set(
        "GetTableByShortname",
        make_get_table_by_shortname(lua, provider.clone())?,
    )
    .map_err(mlua_to_core)?;
    db.set(
        "GetTableNames",
        make_get_table_names(lua, provider.clone())?,
    )
    .map_err(mlua_to_core)?;
    db.set("Refresh", make_refresh(lua, provider.clone())?)
        .map_err(mlua_to_core)?;
    db.set("Load", make_refresh(lua, provider.clone())?)
        .map_err(mlua_to_core)?;
    db.set("Reset", make_refresh(lua, provider)?)
        .map_err(mlua_to_core)?;
    le.set("db", db).map_err(mlua_to_core)?;
    lua.globals().set("LE", le).map_err(mlua_to_core)?;

    register_logger(lua, events.clone())?;
    register_print(lua, events.clone())?;
    register_message_box(lua, events.clone())?;
    register_unsupported(lua)?;
    register_event_stubs(lua)?;
    register_unsafe_object_stubs(lua)?;

    Ok(())
}

fn make_get_table(
    lua: &Lua,
    provider: Rc<RefCell<Box<dyn TableProvider>>>,
) -> Result<mlua::Function> {
    lua.create_function(move |lua, (_self, name): (Value, String)| {
        let mut provider = provider.borrow_mut();
        if !provider.has_table(&name) {
            return Ok(Value::Nil);
        }
        let generation_cell = provider.generation();
        let snapshot = provider
            .load_table(&name)
            .map_err(|error| compat_error(format!("LE.db:GetTable(\"{name}\"): {error}")))?;
        let generation = generation_cell.load(Ordering::SeqCst);
        let handle = TableHandle::new(snapshot, generation_cell, generation);
        Ok(Value::UserData(lua.create_userdata(handle)?))
    })
    .map_err(mlua_to_core)
}

fn make_get_table_by_shortname(
    lua: &Lua,
    provider: Rc<RefCell<Box<dyn TableProvider>>>,
) -> Result<mlua::Function> {
    lua.create_function(move |lua, (_self, shortname): (Value, String)| {
        let mut provider = provider.borrow_mut();
        let Some(name) = provider.table_by_shortname(&shortname) else {
            return Ok(Value::Nil);
        };
        if !provider.has_table(&name) {
            return Ok(Value::Nil);
        }
        let generation_cell = provider.generation();
        let snapshot = provider.load_table(&name).map_err(|error| {
            compat_error(format!(
                "LE.db:GetTableByShortname(\"{shortname}\"): {error}"
            ))
        })?;
        let generation = generation_cell.load(Ordering::SeqCst);
        Ok(Value::UserData(lua.create_userdata(TableHandle::new(
            snapshot,
            generation_cell,
            generation,
        ))?))
    })
    .map_err(mlua_to_core)
}

fn make_get_table_names(
    lua: &Lua,
    provider: Rc<RefCell<Box<dyn TableProvider>>>,
) -> Result<mlua::Function> {
    lua.create_function(move |lua, _: Value| {
        let names = provider.borrow().table_names();
        let table = lua.create_table()?;
        for (index, name) in names.iter().enumerate() {
            table.set(index + 1, name.clone())?;
        }
        Ok(table)
    })
    .map_err(mlua_to_core)
}

fn make_refresh(
    lua: &Lua,
    provider: Rc<RefCell<Box<dyn TableProvider>>>,
) -> Result<mlua::Function> {
    lua.create_function(move |_, _: Value| {
        provider
            .borrow_mut()
            .refresh()
            .map_err(|error| compat_error(format!("LE.db:Refresh: {error}")))?;
        Ok(())
    })
    .map_err(mlua_to_core)
}

fn register_logger(lua: &Lua, events: Sender<LuaEvent>) -> Result<()> {
    let logger = lua.create_table().map_err(mlua_to_core)?;
    for (name, level) in [
        ("LogDebug", LogLevel::Debug),
        ("LogInfo", LogLevel::Info),
        ("LogWarn", LogLevel::Warn),
        ("LogWarning", LogLevel::Warn),
        ("LogError", LogLevel::Error),
        ("LogFatal", LogLevel::Fatal),
    ] {
        let events = events.clone();
        let function = lua
            .create_function(move |_, (_self, value): (Value, Value)| {
                let message = value.to_string().unwrap_or_default();
                let _ = events.send(LuaEvent::Log {
                    level,
                    message: format!("[{}] {}", level.label(), message),
                });
                Ok(())
            })
            .map_err(mlua_to_core)?;
        logger.set(name, function).map_err(mlua_to_core)?;
    }
    lua.globals().set("LOGGER", logger).map_err(mlua_to_core)?;
    Ok(())
}

fn register_print(lua: &Lua, events: Sender<LuaEvent>) -> Result<()> {
    let function = lua
        .create_function(move |_, values: Variadic<Value>| {
            let message = values
                .iter()
                .map(|value| value.to_string().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\t");
            let _ = events.send(LuaEvent::Log {
                level: LogLevel::Info,
                message: format!("[PRINT] {message}"),
            });
            Ok(())
        })
        .map_err(mlua_to_core)?;
    lua.globals().set("print", function).map_err(mlua_to_core)?;
    Ok(())
}

fn register_message_box(lua: &Lua, events: Sender<LuaEvent>) -> Result<()> {
    let function = lua
        .create_function(move |_, (title, text): (String, String)| {
            let _ = events.send(LuaEvent::MessageBox { title, text });
            Ok(())
        })
        .map_err(mlua_to_core)?;
    lua.globals()
        .set("MessageBox", function)
        .map_err(mlua_to_core)?;
    Ok(())
}

const UNSUPPORTED_READ: &[&str] = &[
    "GetTeamIdFromPlayerId",
    "GetTeamName",
    "GetPlayerName",
    "GetSaveUID",
    "GetCompetitionNameByID",
    "GetCompetitionNameByObjID",
    "GetPlayersStats",
    "GetPlayerStats",
    "GetDBTablesNames",
    "GetDBTableFields",
    "GetDBTableRows",
];

const UNSUPPORTED_WRITE: &[&str] = &[
    "SetPlayerSharpness",
    "SetPlayerMorale",
    "SetPlayerForm",
    "SetPlayerFitness",
    "PlayerSetValueInDevelopementPlan",
    "DeletePlayer",
    "CreatePlayer",
    "TransferPlayer",
    "cTransferPlayer",
    "LoanPlayer",
    "cLoanPlayer",
    "ReleasePlayerFromTeam",
    "TerminateLoan",
    "DeletePresignedContract",
    "AddPlayerToTransferList",
    "AddPlayerToLoanList",
    "RemovePlayerFromTransferList",
    "RemovePlayerFromLoanList",
    "RemovePlayerFromLists",
    "InsertDBTableRow",
    "DeleteDBTableRow",
    "EditDBTableField",
    "PlayerCaptureSetOutputDirectory",
    "PlayerCaptureSetSize",
    "PlayerCaptureSetType",
    "PlayerCaptureSetCamera",
    "PlayerCaptureAddPlayer",
    "PlayerCaptureStart",
    "SetTransferBudget",
];

const UNSUPPORTED_QUERY: &[&str] = &[
    "IsInCM",
    "PlayerHasDevelopementPlan",
    "IsPlayerLoanedOut",
    "IsPlayerPresigned",
    "IsPlayerTransferListed",
    "IsPlayerLoanListed",
    "PlayerExists",
    "GetTransferBudget",
];

fn register_unsupported(lua: &Lua) -> Result<()> {
    for name in UNSUPPORTED_READ {
        let function = lua
            .create_function(move |_, _: Variadic<Value>| {
                Err::<(), _>(compat_error(format!(
                    "{name} is not implemented in this milestone; no data was fabricated"
                )))
            })
            .map_err(mlua_to_core)?;
        lua.globals().set(*name, function).map_err(mlua_to_core)?;
    }
    for name in UNSUPPORTED_WRITE {
        let function = lua
            .create_function(move |_, _: Variadic<Value>| {
                Err::<(), _>(compat_error(format!(
                    "{name} is a write operation; FC27LE is read-only and no memory was modified"
                )))
            })
            .map_err(mlua_to_core)?;
        lua.globals().set(*name, function).map_err(mlua_to_core)?;
    }
    for name in UNSUPPORTED_QUERY {
        let function = lua
            .create_function(move |_, _: Variadic<Value>| {
                Err::<(), _>(compat_error(format!(
                    "{name} requires game-state data (including career-state) that has not been verified; no value was fabricated"
                )))
            })
            .map_err(mlua_to_core)?;
        lua.globals().set(*name, function).map_err(mlua_to_core)?;
    }
    Ok(())
}

fn register_event_stubs(lua: &Lua) -> Result<()> {
    for name in [
        "AddEventHandler",
        "GetEventHandlers",
        "RemoveEventHandler",
        "ClearEventHandlersForEvent",
    ] {
        let function = lua
            .create_function(move |_, _: Variadic<Value>| {
                Err::<(), _>(compat_error(format!(
                    "{name} is not implemented: FC27LE has no verified game-event source, so no handler was registered"
                )))
            })
            .map_err(mlua_to_core)?;
        lua.globals().set(name, function).map_err(mlua_to_core)?;
    }
    Ok(())
}

fn register_unsafe_object_stubs(lua: &Lua) -> Result<()> {
    for object in ["MEMORY", "HTTP", "REQUEST"] {
        let table = lua.create_table().map_err(mlua_to_core)?;
        let metatable = lua.create_table().map_err(mlua_to_core)?;
        let name = object.to_string();
        let index = lua
            .create_function(move |lua, (_table, key): (Table, String)| {
                let owner = name.clone();
                let key = key.clone();
                lua.create_function(move |_, _: Variadic<Value>| {
                    Err::<(), _>(compat_error(format!(
                        "{owner}.{key} is not implemented; raw process memory/HTTP access is outside this read-only milestone"
                    )))
                })
            })
            .map_err(mlua_to_core)?;
        metatable.set("__index", index).map_err(mlua_to_core)?;
        table.set_metatable(Some(metatable)).map_err(mlua_to_core)?;
        lua.globals().set(object, table).map_err(mlua_to_core)?;
    }
    Ok(())
}

struct TableHandle {
    snapshot: Arc<TableSnapshot>,
    generation_cell: Arc<AtomicU64>,
    generation: u64,
    cursor: Cell<u32>,
}

impl TableHandle {
    fn new(snapshot: Arc<TableSnapshot>, generation_cell: Arc<AtomicU64>, generation: u64) -> Self {
        Self {
            snapshot,
            generation_cell,
            generation,
            cursor: Cell::new(0),
        }
    }

    fn ensure_current(&self) -> mlua::Result<()> {
        if self.generation_cell.load(Ordering::SeqCst) != self.generation {
            return Err(compat_error(
                "table handle is stale after LE.db:Refresh; call GetTable again",
            ));
        }
        Ok(())
    }

    fn handle(&self, slot: u32) -> i64 {
        make_record_handle(self.snapshot.info().table_id, self.generation, slot)
    }

    fn first_valid(&self) -> mlua::Result<i64> {
        self.ensure_current()?;
        let count = self.snapshot.info().slot_count;
        for slot in 0..count {
            if self.snapshot.is_valid(slot).map_err(mlua_to_lua)? {
                self.cursor.set(slot);
                return Ok(self.handle(slot));
            }
        }
        self.cursor.set(0);
        Ok(0)
    }

    fn next_valid(&self) -> mlua::Result<i64> {
        self.ensure_current()?;
        let count = self.snapshot.info().slot_count;
        let mut slot = self.cursor.get().saturating_add(1);
        while slot < count {
            if self.snapshot.is_valid(slot).map_err(mlua_to_lua)? {
                self.cursor.set(slot);
                return Ok(self.handle(slot));
            }
            slot += 1;
        }
        self.cursor.set(count.saturating_sub(1));
        Ok(0)
    }

    fn next_any(&self) -> mlua::Result<i64> {
        self.ensure_current()?;
        let count = self.snapshot.info().slot_count;
        let slot = self.cursor.get().saturating_add(1);
        if slot < count {
            self.cursor.set(slot);
            Ok(self.handle(slot))
        } else {
            self.cursor.set(count.saturating_sub(1));
            Ok(0)
        }
    }

    fn read(&self, lua: &Lua, record: i64, field: &str) -> mlua::Result<Value> {
        self.ensure_current()?;
        let (table_id, generation, slot) = parse_record_handle(record).ok_or_else(|| {
            compat_error("record handle must be a positive numeric token from GetFirstRecord/GetNextValidRecord")
        })?;
        if generation != self.generation || table_id != self.snapshot.info().table_id {
            return Err(compat_error(
                "record handle is stale or belongs to another table; call GetTable again",
            ));
        }
        let value = self
            .snapshot
            .read_field(slot, field)
            .map_err(|error| compat_error(format!("GetRecordFieldValue({field}): {error}")))?;
        field_to_lua(lua, value)
    }
}

impl UserData for TableHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| {
            Ok(this.snapshot.info().table_name.clone())
        });
        fields.add_field_method_get("shortname", |_, this| {
            Ok(this.snapshot.info().shortname.clone())
        });
        fields.add_field_method_get("written_records", |_, this| {
            Ok(this.snapshot.info().slot_count as i64)
        });
        fields.add_field_method_get("record_size", |_, this| {
            Ok(this.snapshot.info().record_size as i64)
        });
        fields.add_field_method_get("last_record_idx", |_, this| {
            Ok(this.snapshot.info().slot_count as i64 - 1)
        });
        fields.add_field_method_get("current_record_idx", |_, this| Ok(this.cursor.get() as i64));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetFirstRecord", |_, this, ()| this.first_valid());
        methods.add_method("GetNextValidRecord", |_, this, ()| this.next_valid());
        methods.add_method("GetNextRecord", |_, this, ()| this.next_any());
        methods.add_method(
            "GetRecordFieldValue",
            |lua, this, (record, field): (i64, String)| this.read(lua, record, &field),
        );
        methods.add_method(
            "SetRecordFieldValue",
            |_, _this, (_record, field, _value): (i64, String, Value)| {
                Err::<(), _>(compat_error(format!(
                    "SetRecordFieldValue({field}) is a write operation; FC27LE is read-only and no memory was modified"
                )))
            },
        );
        methods.add_method("GetField", |lua, this, name: String| {
            let Some(field) = this.snapshot.field(&name) else {
                return Ok(Value::Nil);
            };
            this.ensure_current()?;
            let handle = FieldHandle {
                snapshot: this.snapshot.clone(),
                generation_cell: this.generation_cell.clone(),
                generation: this.generation,
                field: field.clone(),
            };
            Ok(Value::UserData(lua.create_userdata(handle)?))
        });
        methods.add_method("GetFieldsCount", |_, this, ()| {
            Ok(this.snapshot.info().field_count as i64)
        });
        methods.add_method("GetName", |_, this, ()| {
            Ok(this.snapshot.info().table_name.clone())
        });
        methods.add_method("GetShortName", |_, this, ()| {
            Ok(this.snapshot.info().shortname.clone())
        });
        methods.add_method("GetRecordSize", |_, this, ()| {
            Ok(this.snapshot.info().record_size as i64)
        });
        methods.add_method("GetWrittenRecords", |_, this, ()| {
            Ok(this.snapshot.info().slot_count as i64)
        });
        methods.add_method("GetLastRecordIdx", |_, this, ()| {
            Ok(this.snapshot.info().slot_count as i64 - 1)
        });
        methods.add_method("GetCurrentRecordIdx", |_, this, ()| {
            Ok(this.cursor.get() as i64)
        });
    }
}

struct FieldHandle {
    snapshot: Arc<TableSnapshot>,
    generation_cell: Arc<AtomicU64>,
    generation: u64,
    field: FieldInfo,
}

impl UserData for FieldHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, this| Ok(this.field.name.clone()));
        fields.add_field_method_get("shortname", |_, this| Ok(this.field.shortname.clone()));
        fields.add_field_method_get("type", |_, this| Ok(this.field.field_type as i64));
        fields.add_field_method_get("offset", |_, this| Ok((this.field.bit_offset / 8) as i64));
        fields.add_field_method_get("startbit", |_, this| Ok((this.field.bit_offset % 8) as i64));
        fields.add_field_method_get("sz", |_, this| Ok(this.field.bit_width as i64));
        fields.add_field_method_get("type_name", |_, this| Ok(this.field.type_name.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetValue", |lua, this, record: i64| {
            if this.generation_cell.load(Ordering::SeqCst) != this.generation {
                return Err(compat_error(
                    "field handle is stale after LE.db:Refresh; call GetTable again",
                ));
            }
            let (table_id, generation, slot) = parse_record_handle(record).ok_or_else(|| {
                compat_error("record handle must be a positive numeric token from GetFirstRecord/GetNextValidRecord")
            })?;
            if generation != this.generation || table_id != this.snapshot.info().table_id {
                return Err(compat_error(
                    "record handle is stale or belongs to another table; call GetTable again",
                ));
            }
            let value = this
                .snapshot
                .read_field(slot, &this.field.name)
                .map_err(|error| compat_error(format!("Field:GetValue: {error}")))?;
            field_to_lua(lua, value)
        });
        methods.add_method("SetValue", |_, this, (_record, _value): (i64, Value)| {
            Err::<(), _>(compat_error(format!(
                "Field:SetValue({}) is a write operation; FC27LE is read-only and no memory was modified",
                this.field.name
            )))
        });
    }
}

fn field_to_lua(lua: &Lua, value: FieldValue) -> mlua::Result<Value> {
    match value {
        FieldValue::Integer(value) => Ok(Value::Integer(value)),
        FieldValue::Float(value) => Ok(Value::Number(value)),
        FieldValue::String(value) => Ok(Value::String(lua.create_string(value.as_bytes())?)),
    }
}

fn make_record_handle(table_id: u16, generation: u64, slot: u32) -> i64 {
    let value = ((generation & 0x7FFF) << 48) | (u64::from(table_id) << 32) | (u64::from(slot) + 1);
    value as i64
}

fn parse_record_handle(handle: i64) -> Option<(u16, u64, u32)> {
    if handle <= 0 {
        return None;
    }
    let value = handle as u64;
    let slot = (value & 0xFFFF_FFFF).checked_sub(1)? as u32;
    let table_id = ((value >> 32) & 0xFFFF) as u16;
    let generation = (value >> 48) & 0x7FFF;
    if table_id == 0 || generation == 0 {
        return None;
    }
    Some((table_id, generation, slot))
}

fn compat_error(message: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(format!("FC27LE compatibility: {}", message.into()))
}

fn mlua_to_lua(error: Error) -> mlua::Error {
    compat_error(error.to_string())
}

fn mlua_to_core(error: mlua::Error) -> Error {
    Error::InvalidInput(format!("Lua API registration: {error}"))
}
