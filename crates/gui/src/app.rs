use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, ViewportCommand, WindowLevel};
use fc27_game::discovery::DiscoveredProcess;
use fc27_lua_compat::api::{
    DEFAULT_MEMORY_LIMIT_BYTES, DEFAULT_SCRIPT_TIMEOUT, LogLevel, LuaEvent, ScriptJob,
    ScriptSession,
};
use fc27_lua_compat::matrix::{ApiStatus, compatibility_matrix, status_counts};
use fc27_process::Pid;

use crate::session::{
    LoadEvent, LoadRequest, Session, SessionLoader, default_metadata_path, discover_game,
};
use crate::socket::{ToggleCommand, ToggleListener, start_listener};
use crate::sort::{SortKey, filtered_sorted};

pub struct AppOptions {
    pub metadata: Option<PathBuf>,
    pub pid: Option<Pid>,
    pub max_mib: u64,
    pub filter: Option<String>,
    pub script: Option<PathBuf>,
    pub run_script: bool,
    pub no_auto_connect: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LuaTab {
    Console,
    Api,
    Session,
}

struct LogLine {
    level: LogLevel,
    message: String,
}

pub struct App {
    discovery: Option<DiscoveredProcess>,
    discovery_error: Option<String>,
    pid: Option<Pid>,
    metadata_path: String,
    max_mib: u64,
    session: Option<Session>,
    session_loading: bool,
    session_status: Option<String>,
    session_error: Option<String>,
    loader: SessionLoader,
    lua: fc27_lua_compat::worker::LuaWorker,
    lua_ready: bool,
    lua_session_error: Option<String>,
    lua_script: String,
    lua_script_path: String,
    lua_script_dirs: String,
    lua_log: Vec<LogLine>,
    lua_running: bool,
    lua_status: Option<String>,
    lua_message_box: Option<(String, String)>,
    pending_auto_run: bool,
    search: String,
    sort_key: SortKey,
    sort_desc: bool,
    filtered: Vec<usize>,
    selected: Option<usize>,
    always_on_top: bool,
    visible: bool,
    window_level_initialized: bool,
    pending_focus: bool,
    focus_retry_at: Option<Instant>,
    focus_status: Option<String>,
    socket: Option<ToggleListener>,
    socket_error: Option<String>,
    lua_tab: LuaTab,
    show_help: bool,
    status_message: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, options: AppOptions) -> Self {
        let (socket, socket_error) = match start_listener(cc.egui_ctx.clone()) {
            Ok(listener) => (Some(listener), None),
            Err(error) => (None, Some(error)),
        };
        let (discovery, discovery_error) = match discover_game() {
            Ok(game) => (game, None),
            Err(error) => (None, Some(error)),
        };
        let pid = options
            .pid
            .or_else(|| discovery.as_ref().map(|game| game.pid));
        let metadata = options
            .metadata
            .clone()
            .or_else(default_metadata_path)
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let mut lua_script = default_lua_script().to_string();
        let mut lua_script_path = String::new();
        let mut startup_error = None;
        if let Some(path) = &options.script {
            match std::fs::read_to_string(path) {
                Ok(source) => {
                    lua_script = source;
                    lua_script_path = path.display().to_string();
                }
                Err(error) => {
                    startup_error = Some(format!("read script {}: {error}", path.display()));
                }
            }
        }
        let mut app = Self {
            discovery,
            discovery_error,
            pid,
            metadata_path: metadata,
            max_mib: options.max_mib,
            session: None,
            session_loading: false,
            session_status: None,
            session_error: startup_error,
            loader: SessionLoader::spawn(),
            lua: fc27_lua_compat::worker::LuaWorker::spawn(),
            lua_ready: false,
            lua_session_error: None,
            lua_script,
            lua_script_path,
            lua_script_dirs: default_script_dirs(),
            lua_log: Vec::new(),
            lua_running: false,
            lua_status: None,
            lua_message_box: None,
            pending_auto_run: options.run_script,
            search: options.filter.clone().unwrap_or_default(),
            sort_key: SortKey::Overall,
            sort_desc: true,
            filtered: Vec::new(),
            selected: None,
            always_on_top: false,
            visible: true,
            window_level_initialized: false,
            pending_focus: true,
            focus_retry_at: None,
            focus_status: None,
            socket,
            socket_error,
            lua_tab: LuaTab::Console,
            show_help: false,
            status_message: None,
        };
        app.rebuild_filtered();
        if !options.no_auto_connect && app.pid.is_some() && !app.metadata_path.trim().is_empty() {
            app.start_load();
        } else if app.pid.is_none() {
            app.status_message =
                Some("FC 27 was not found. Start the game, then click Discover.".to_string());
        } else if app.metadata_path.trim().is_empty() {
            app.status_message =
                Some("Select a caller-supplied fifa_ng_db-meta.xml and click Load.".to_string());
        }
        app
    }

    fn ensure_pid(&mut self) -> bool {
        if self.pid.is_some() {
            return true;
        }
        match discover_game() {
            Ok(game) => {
                self.discovery = game;
                self.discovery_error = None;
                if let Some(game) = &self.discovery {
                    self.pid = Some(game.pid);
                    self.status_message = Some(format!("Found FC27.exe at pid {}", game.pid.0));
                    return true;
                }
                self.status_message =
                    Some("FC 27 was not found. Start the game, then click Discover.".to_string());
                false
            }
            Err(error) => {
                self.discovery_error = Some(error.clone());
                self.session_error = Some(format!("discovery failed: {error}"));
                false
            }
        }
    }

    fn start_load(&mut self) {
        if self.session_loading {
            return;
        }
        if !self.ensure_pid() {
            return;
        }
        let Some(pid) = self.pid else {
            return;
        };
        let path = PathBuf::from(self.metadata_path.trim());
        if path.as_os_str().is_empty() {
            self.session_error = Some("metadata path is empty".to_string());
            return;
        }
        if !path.is_file() {
            self.session_error = Some(format!("metadata file not found: {}", path.display()));
            return;
        }
        self.session_loading = true;
        self.session_error = None;
        self.session_status = Some(format!(
            "Loading player database from pid {} with {}",
            pid.0,
            path.display()
        ));
        self.loader.load(LoadRequest {
            pid,
            metadata_path: path,
            max_mib: self.max_mib,
        });
    }

    fn rebuild_filtered(&mut self) {
        if let Some(session) = &self.session {
            self.filtered = filtered_sorted(
                &session.player_scan.records,
                &self.search,
                self.sort_key,
                self.sort_desc,
            );
            if self
                .selected
                .is_some_and(|selected| !self.filtered.contains(&selected))
            {
                self.selected = self.filtered.first().copied();
            }
        } else {
            self.filtered.clear();
            self.selected = None;
        }
    }

    fn poll_loader(&mut self) {
        while let Some(event) = self.loader.try_recv() {
            match event {
                LoadEvent::Started { pid, metadata_path } => {
                    self.session_loading = true;
                    self.session_status = Some(format!(
                        "Loading pid {} with {}",
                        pid.0,
                        metadata_path.display()
                    ));
                }
                LoadEvent::Finished(Ok(session)) => {
                    self.session_loading = false;
                    self.session_status = Some(format!(
                        "Loaded {} players and {} names from pid {}",
                        session.player_scan.valid_slots,
                        session.name_report.emitted_with_names,
                        session.pid.0
                    ));
                    self.session_error = None;
                    let lua_session = ScriptSession {
                        pid: session.pid,
                        module_name: session.module_name.clone(),
                        metadata: session.metadata.clone(),
                        data_path: session
                            .metadata_path
                            .parent()
                            .map(|path| path.display().to_string())
                            .unwrap_or_default(),
                        max_db_mib: self.max_mib,
                    };
                    self.lua_ready = false;
                    self.lua_session_error = None;
                    self.lua.set_session(lua_session);
                    self.pid = Some(session.pid);
                    self.session = Some(*session);
                    self.selected = None;
                    self.rebuild_filtered();
                    self.selected = self.filtered.first().copied();
                }
                LoadEvent::Finished(Err(error)) => {
                    self.session_loading = false;
                    self.session_status = None;
                    self.session_error = Some(error);
                }
            }
        }
    }

    fn poll_lua(&mut self) {
        while let Some(event) = self.lua.try_recv() {
            match event {
                LuaEvent::SessionReady { pid, tables } => {
                    self.lua_ready = true;
                    self.lua_session_error = None;
                    self.lua_status = Some(format!(
                        "Lua session ready: pid {} / {} table(s)",
                        pid.0, tables
                    ));
                    self.lua_log.push(LogLine {
                        level: LogLevel::Info,
                        message: format!("Lua database session ready: {tables} table(s)"),
                    });
                    if self.pending_auto_run {
                        self.pending_auto_run = false;
                        self.run_lua();
                    }
                }
                LuaEvent::SessionError { message } => {
                    self.lua_ready = false;
                    self.lua_session_error = Some(message.clone());
                    self.lua_status = Some(format!("Lua session failed: {message}"));
                    self.lua_log.push(LogLine {
                        level: LogLevel::Error,
                        message: format!("Lua session failed: {message}"),
                    });
                }
                LuaEvent::Started { name } => {
                    self.lua_running = true;
                    self.lua_status = Some(format!("Running {name}..."));
                }
                LuaEvent::Log { level, message } => {
                    self.lua_log.push(LogLine { level, message });
                    if self.lua_log.len() > 4000 {
                        self.lua_log.drain(..self.lua_log.len() - 4000);
                    }
                }
                LuaEvent::MessageBox { title, text } => {
                    self.lua_log.push(LogLine {
                        level: LogLevel::Info,
                        message: format!("MessageBox: {title}: {text}"),
                    });
                    self.lua_message_box = Some((title, text));
                }
                LuaEvent::Finished { success, error } => {
                    self.lua_running = false;
                    if success {
                        self.lua_status = Some("Script finished".to_string());
                    } else {
                        self.lua_status = Some(format!(
                            "Script failed: {}",
                            error.as_deref().unwrap_or("unknown error")
                        ));
                        self.lua_log.push(LogLine {
                            level: LogLevel::Error,
                            message: format!(
                                "Script failed: {}",
                                error.unwrap_or_else(|| "unknown error".to_string())
                            ),
                        });
                    }
                }
            }
        }
    }

    fn run_lua(&mut self) {
        if !self.lua_ready {
            self.lua_status = Some(
                "Lua session is not ready; connect to FC 27 and load metadata first.".to_string(),
            );
            return;
        }
        if self.lua_script.trim().is_empty() {
            self.lua_status = Some("Script is empty".to_string());
            return;
        }
        let mut dirs = Vec::new();
        for part in self.lua_script_dirs.split(';') {
            let part = part.trim();
            if !part.is_empty() {
                let path = PathBuf::from(part);
                if !dirs.contains(&path) {
                    dirs.push(path);
                }
            }
        }
        let script_path = PathBuf::from(&self.lua_script_path);
        if let Some(parent) = script_path.parent() {
            if !parent.as_os_str().is_empty() && !dirs.contains(&parent.to_path_buf()) {
                dirs.push(parent.to_path_buf());
            }
        }
        let name = script_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("lua-console")
            .to_string();
        self.lua_status = Some(format!("Running {name}..."));
        self.lua_log.push(LogLine {
            level: LogLevel::Info,
            message: format!("--- run {name} ---"),
        });
        self.lua.run(ScriptJob {
            name,
            source: self.lua_script.clone(),
            script_dirs: dirs,
            timeout: DEFAULT_SCRIPT_TIMEOUT,
            memory_limit_bytes: DEFAULT_MEMORY_LIMIT_BYTES,
        });
    }

    fn stop_lua(&mut self) {
        if self.lua_running {
            self.lua.cancel();
            self.lua_status = Some("Cancellation requested...".to_string());
        }
    }

    fn load_script_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                self.lua_script = source;
                self.lua_script_path = path.display().to_string();
                self.lua_status = Some(format!("Loaded {}", path.display()));
            }
            Err(error) => {
                self.lua_status = Some(format!("Open {}: {error}", path.display()));
            }
        }
    }

    fn save_script_file(&mut self) {
        if self.lua_script_path.trim().is_empty() {
            self.lua_status = Some("Enter a script path before saving".to_string());
            return;
        }
        match std::fs::write(self.lua_script_path.trim(), &self.lua_script) {
            Ok(()) => self.lua_status = Some(format!("Saved {}", self.lua_script_path)),
            Err(error) => self.lua_status = Some(format!("Save failed: {error}")),
        }
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.visible = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        self.status_message = Some(
            "Hidden. Bind `fc27le-gui --toggle` to F9 in KDE System Settings to restore."
                .to_string(),
        );
        self.lua_log.push(LogLine {
            level: LogLevel::Info,
            message: "Window hidden. Use the KDE F9 shortcut (fc27le-gui --toggle) to restore."
                .to_string(),
        });
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.visible = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        }));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        self.pending_focus = true;
        self.status_message = Some("Window shown".to_string());
    }

    fn top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("connection")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.heading("FC27LE");
                    ui.label(RichText::new("read-only FC 27 browser").small().weak());
                    ui.separator();
                    let (label, color) = if let Some(session) = &self.session {
                        (
                            format!("connected: pid {} / {}", session.pid.0, session.module_name),
                            Color32::LIGHT_GREEN,
                        )
                    } else if self.session_loading {
                        ("loading...".to_string(), Color32::YELLOW)
                    } else if self.pid.is_some() {
                        (
                            format!("game candidate: pid {}", self.pid.unwrap().0),
                            Color32::LIGHT_BLUE,
                        )
                    } else {
                        ("FC 27 not found".to_string(), Color32::LIGHT_RED)
                    };
                    ui.colored_label(color, label);
                    if self.session_loading {
                        ui.spinner();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Discover").clicked() {
                        self.ensure_pid();
                    }
                    if ui.button("Connect").clicked() {
                        self.start_load();
                    }
                    if ui.button("Refresh").clicked() {
                        self.start_load();
                    }
                    if ui.button("Reconnect").clicked() {
                        self.pid = None;
                        self.ensure_pid();
                        self.start_load();
                    }
                    let always_response = ui
                        .checkbox(&mut self.always_on_top, "Always on top")
                        .on_hover_text(
                            "KWin/Wayland tested behavior: the X11 always-on-top state made the \
                             overlay click-through. Leave this off for interactive use.",
                        );
                    if always_response.changed() {
                        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(
                            if self.always_on_top {
                                WindowLevel::AlwaysOnTop
                            } else {
                                WindowLevel::Normal
                            },
                        ));
                        self.pending_focus = true;
                    }
                    if ui.button("Hide (F9)").clicked() {
                        self.hide_window(ctx);
                    }
                    if ui.button("Help").clicked() {
                        self.show_help = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Metadata:");
                    let response = ui.add_sized(
                        [620.0, 20.0],
                        egui::TextEdit::singleline(&mut self.metadata_path),
                    );
                    if response.changed() {
                        self.session_error = None;
                    }
                    if ui.button("Load metadata").clicked() {
                        self.start_load();
                    }
                    if let Some(session) = &self.session {
                        ui.label(
                            RichText::new(format!(
                                "sha256 {}…",
                                &session.metadata_sha256[..12.min(session.metadata_sha256.len())]
                            ))
                            .small()
                            .weak(),
                        );
                    }
                });
                if let Some(session) = &self.session {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "pid {}  |  {} @ {:#x}  |  build {}  |  {}",
                            session.pid.0,
                            session.module_name,
                            session.module_base,
                            session
                                .build_sha256
                                .as_deref()
                                .map(|sha| &sha[..12.min(sha.len())])
                                .unwrap_or("unavailable"),
                            session.build_label.as_deref().unwrap_or("UNVERIFIED BUILD")
                        ));
                        if session.build_known {
                            ui.colored_label(Color32::LIGHT_GREEN, "known build");
                        } else {
                            ui.colored_label(Color32::YELLOW, "build not in known list");
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "metadata {}/{} matched, {}/{} fixed-row verified; players focus {}",
                            session.metadata_matched,
                            session.metadata_table_count,
                            session.metadata_verified,
                            session.metadata_table_count,
                            if session.focus_verified {
                                "VERIFIED"
                            } else {
                                "FAILED"
                            }
                        ));
                        ui.separator();
                        ui.label(format!(
                            "DB {} table(s), {} player row(s), {} valid",
                            session.db_table_count,
                            session.player_scan.slots_scanned,
                            session.player_scan.valid_slots
                        ));
                    });
                } else if let Some(game) = &self.discovery {
                    ui.label(format!(
                        "candidate pid {} score {}: {}",
                        game.pid.0,
                        game.score,
                        game.reasons.join("; ")
                    ));
                }
                if let Some(error) = &self.session_error {
                    ui.colored_label(Color32::LIGHT_RED, format!("ERROR: {error}"));
                }
                if let Some(error) = &self.discovery_error {
                    ui.colored_label(Color32::LIGHT_RED, format!("discovery: {error}"));
                }
                if let Some(status) = &self.session_status {
                    ui.colored_label(Color32::LIGHT_BLUE, status);
                }
                if let Some(status) = &self.status_message {
                    ui.colored_label(Color32::GRAY, status);
                }
                if let Some(error) = &self.socket_error {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!("toggle socket unavailable: {error}"),
                    );
                }
                if let Some(status) = &self.focus_status {
                    ui.colored_label(Color32::GRAY, format!("KWin focus: {status}"));
                }
                ui.add_space(4.0);
            });
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                let changed = ui
                    .add_sized(
                        [360.0, 22.0],
                        egui::TextEdit::singleline(&mut self.search).hint_text("name or player ID"),
                    )
                    .changed();
                if changed {
                    self.rebuild_filtered();
                    self.selected = self.filtered.first().copied();
                }
                if ui.button("Clear").clicked() {
                    self.search.clear();
                    self.rebuild_filtered();
                    self.selected = self.filtered.first().copied();
                }
                if let Some(session) = &self.session {
                    ui.label(format!(
                        "{} result(s) of {}",
                        self.filtered.len(),
                        session.player_scan.valid_slots
                    ));
                }
            });
            ui.separator();
            if self.session.is_none() {
                ui.add_space(24.0);
                ui.heading("No player database loaded");
                ui.label("1. Start FC 27 and select its process.");
                ui.label("2. Choose a caller-supplied fifa_ng_db-meta.xml.");
                ui.label("3. Click Load metadata / Connect.");
                ui.label("The Lua runtime uses the same verified backend.");
                return;
            }
            let sort_before = (self.sort_key, self.sort_desc);
            ui.horizontal(|ui| {
                sort_header(
                    ui,
                    "Name",
                    300.0,
                    SortKey::Name,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
                sort_header(
                    ui,
                    "ID",
                    80.0,
                    SortKey::PlayerId,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
                sort_header(
                    ui,
                    "OVR",
                    54.0,
                    SortKey::Overall,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
                sort_header(
                    ui,
                    "POT",
                    54.0,
                    SortKey::Potential,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
                sort_header(
                    ui,
                    "Height",
                    64.0,
                    SortKey::Height,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
                sort_header(
                    ui,
                    "Weight",
                    64.0,
                    SortKey::Weight,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
            });
            if (self.sort_key, self.sort_desc) != sort_before {
                self.rebuild_filtered();
                self.selected = self.filtered.first().copied();
            }
            let session = self.session.as_ref().expect("session checked above");
            let records = &session.player_scan.records;
            let filtered = std::mem::take(&mut self.filtered);
            let mut selected = self.selected;
            if filtered.is_empty() {
                ui.add_space(12.0);
                ui.label("No players match the current filter.");
                self.filtered = filtered;
                self.selected = selected;
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, 20.0, filtered.len(), |ui, range| {
                    for row in range {
                        let index = filtered[row];
                        let record = &records[index];
                        let is_selected = selected == Some(index);
                        ui.horizontal(|ui| {
                            let name = record.display_name.as_deref().unwrap_or("-");
                            if ui
                                .add_sized(
                                    [300.0, 18.0],
                                    egui::Button::new(name).selected(is_selected),
                                )
                                .clicked()
                            {
                                selected = Some(index);
                            }
                            ui.add_sized(
                                [80.0, 18.0],
                                egui::Label::new(record.player_id.to_string()),
                            );
                            ui.add_sized(
                                [54.0, 18.0],
                                egui::Label::new(record.overall_rating.to_string()),
                            );
                            ui.add_sized(
                                [54.0, 18.0],
                                egui::Label::new(record.potential.to_string()),
                            );
                            ui.add_sized(
                                [64.0, 18.0],
                                egui::Label::new(format!("{} cm", record.height_cm)),
                            );
                            ui.add_sized(
                                [64.0, 18.0],
                                egui::Label::new(format!("{} kg", record.weight_kg)),
                            );
                        });
                    }
                });
            self.filtered = filtered;
            self.selected = selected;
            // The header may have changed the sort key; rebuild lazily on the
            // next frame if the search or key differs from the rendered data.
            let expected = filtered_sorted(records, &self.search, self.sort_key, self.sort_desc);
            if expected != self.filtered {
                self.filtered = expected;
            }
        });
    }

    fn detail_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("player_detail")
            .default_width(350.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Player detail");
                let Some(index) = self.selected else {
                    ui.add_space(12.0);
                    ui.label("Select a player from the table.");
                    return;
                };
                let Some(session) = &self.session else {
                    return;
                };
                let Some(record) = session.player_scan.records.get(index) else {
                    return;
                };
                ui.add_space(4.0);
                ui.heading(record.display_name.as_deref().unwrap_or("(unnamed)"));
                ui.label(format!("playerid {}", record.player_id));
                ui.separator();
                egui::Grid::new("detail_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        detail_row(ui, "Overall", record.overall_rating.to_string());
                        detail_row(ui, "Potential", record.potential.to_string());
                        detail_row(ui, "Height", format!("{} cm", record.height_cm));
                        detail_row(ui, "Weight", format!("{} kg", record.weight_kg));
                        detail_row(ui, "Slot", record.record_index.to_string());
                        detail_row(
                            ui,
                            "First name",
                            record.first_name.clone().unwrap_or_else(|| "-".into()),
                        );
                        detail_row(
                            ui,
                            "Last name",
                            record.last_name.clone().unwrap_or_else(|| "-".into()),
                        );
                        detail_row(
                            ui,
                            "Common name",
                            record.common_name.clone().unwrap_or_else(|| "-".into()),
                        );
                        detail_row(ui, "First ID", record.first_name_id.to_string());
                        detail_row(ui, "Last ID", record.last_name_id.to_string());
                        detail_row(ui, "Common ID", record.common_name_id.to_string());
                        detail_row(
                            ui,
                            "Name source",
                            record.name_source.clone().unwrap_or_else(|| "-".into()),
                        );
                    });
                if let Some(error) = &record.name_error {
                    ui.colored_label(Color32::LIGHT_RED, format!("name: {error}"));
                }
                ui.add_space(8.0);
                if ui.button("Copy player ID").clicked() {
                    ui.ctx().copy_text(record.player_id.to_string());
                }
                ui.separator();
                ui.label(
                    RichText::new("Read-only: no game memory is modified by this application.")
                        .small()
                        .weak(),
                );
            });
    }

    fn lua_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("lua_panel")
            .resizable(true)
            .default_height(300.0)
            .min_height(160.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.lua_tab, LuaTab::Console, "Lua Console");
                    ui.selectable_value(&mut self.lua_tab, LuaTab::Api, "Lua API");
                    ui.selectable_value(&mut self.lua_tab, LuaTab::Session, "Session");
                    ui.separator();
                    if self.lua_ready {
                        ui.colored_label(Color32::LIGHT_GREEN, "Lua session ready");
                    } else if let Some(error) = &self.lua_session_error {
                        ui.colored_label(Color32::LIGHT_RED, format!("Lua: {error}"));
                    } else {
                        ui.colored_label(Color32::YELLOW, "Lua session not ready");
                    }
                    if let Some(status) = &self.lua_status {
                        ui.separator();
                        ui.label(RichText::new(status).small());
                    }
                });
                ui.separator();
                match self.lua_tab {
                    LuaTab::Console => self.draw_console(ui),
                    LuaTab::Api => self.draw_api(ui),
                    LuaTab::Session => self.draw_session(ui),
                }
            });
    }

    fn draw_console(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Script:");
            ui.add_sized(
                [520.0, 20.0],
                egui::TextEdit::singleline(&mut self.lua_script_path)
                    .hint_text("/path/to/script.lua"),
            );
            if ui.button("Open").clicked() {
                let path = PathBuf::from(self.lua_script_path.trim());
                if !path.as_os_str().is_empty() {
                    self.load_script_file(path);
                }
            }
            if ui.button("Save").clicked() {
                self.save_script_file();
            }
            if ui.button("Example").clicked() {
                self.lua_script = default_lua_script().to_string();
                self.lua_status = Some("Loaded read-only example".to_string());
            }
            if ui.button("Clear log").clicked() {
                self.lua_log.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Module dirs:");
            ui.add_sized(
                [520.0, 20.0],
                egui::TextEdit::singleline(&mut self.lua_script_dirs)
                    .hint_text("semicolon-separated directories"),
            );
            if ui
                .add_enabled(
                    self.lua_ready && !self.lua_running,
                    egui::Button::new("Run"),
                )
                .clicked()
            {
                self.run_lua();
            }
            if ui
                .add_enabled(self.lua_running, egui::Button::new("Stop"))
                .clicked()
            {
                self.stop_lua();
            }
        });
        ui.separator();
        ui.columns(2, |columns| {
            egui::ScrollArea::vertical()
                .id_salt("lua_editor_scroll")
                .auto_shrink([false, false])
                .show(&mut columns[0], |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.lua_script)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(10),
                    );
                });
            egui::ScrollArea::vertical()
                .id_salt("lua_log_scroll")
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(&mut columns[1], |ui| {
                    if self.lua_log.is_empty() {
                        ui.label(RichText::new("Lua logs and script errors appear here.").weak());
                    }
                    for line in &self.lua_log {
                        let color = match line.level {
                            LogLevel::Error | LogLevel::Fatal => Color32::LIGHT_RED,
                            LogLevel::Warn => Color32::YELLOW,
                            LogLevel::Debug => Color32::GRAY,
                            LogLevel::Info => Color32::LIGHT_GREEN,
                        };
                        ui.colored_label(
                            color,
                            format!("[{}] {}", log_label(line.level), line.message),
                        );
                    }
                });
        });
    }

    fn draw_api(&mut self, ui: &mut egui::Ui) {
        let (supported, partial, unimplemented, requires_re) = status_counts();
        ui.label(format!(
            "{} supported/tested, {} partial, {} unimplemented/not-applicable, {} requires further reverse engineering",
            supported, partial, unimplemented, requires_re
        ));
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("api_matrix")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        ui.label(RichText::new("API").strong());
                        ui.label(RichText::new("Status").strong());
                        ui.label(RichText::new("Signature").strong());
                        ui.label(RichText::new("Limitations").strong());
                        ui.end_row();
                        for entry in compatibility_matrix() {
                            ui.label(RichText::new(entry.name).monospace());
                            let (text, color) = status_style(entry.status);
                            ui.colored_label(color, text);
                            ui.label(RichText::new(entry.signature).small());
                            ui.label(RichText::new(entry.limitations).small());
                            ui.end_row();
                        }
                    });
            });
    }

    fn draw_session(&mut self, ui: &mut egui::Ui) {
        let Some(session) = &self.session else {
            ui.label("No session loaded.");
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Connection");
                ui.monospace(format!("pid: {}", session.pid.0));
                ui.monospace(format!(
                    "module: {} @ {:#x} ({} bytes)",
                    session.module_name, session.module_base, session.module_size
                ));
                ui.monospace(format!(
                    "build: {} ({})",
                    session.build_sha256.as_deref().unwrap_or("unavailable"),
                    session.build_label.as_deref().unwrap_or("UNVERIFIED")
                ));
                ui.monospace(format!(
                    "metadata: {} ({} bytes)",
                    session.metadata_path.display(),
                    session.metadata_size
                ));
                ui.monospace(format!("metadata sha256: {}", session.metadata_sha256));
                ui.monospace(format!(
                    "metadata validation: {}/{} matched, {}/{} fixed-row verified",
                    session.metadata_matched,
                    session.metadata_table_count,
                    session.metadata_verified,
                    session.metadata_table_count
                ));
                ui.monospace(format!("database tables: {}", session.db_table_count));
                for (name, count) in &session.databases {
                    ui.monospace(format!("  {name}: {count} table(s)"));
                }
                ui.heading("Players");
                ui.monospace(format!(
                    "slots: {}  valid: {}  invalid: {}  emitted: {}",
                    session.player_scan.slots_scanned,
                    session.player_scan.valid_slots,
                    session.player_scan.invalid_slots,
                    session.player_scan.records.len()
                ));
                ui.monospace(format!(
                    "names: {}/{} with names, {} errors; source {}",
                    session.name_report.emitted_with_names,
                    session.name_report.emitted_with_names
                        + session.name_report.emitted_without_names,
                    session.name_report.name_errors,
                    session.name_report.status
                ));
                ui.monospace(format!(
                    "playernames: {} rows, {} tree nodes, {}/{} decoded",
                    session.name_report.playernames.table.rows,
                    session.name_report.playernames.tree_nodes,
                    session.name_report.playernames.decoded_entries,
                    session.name_report.playernames.unique_name_ids
                ));
                if let Some(dc) = &session.name_report.dcplayernames {
                    ui.monospace(format!("dcplayernames: {} rows", dc.rows));
                }
                if let Some(ed) = &session.name_report.editedplayernames {
                    ui.monospace(format!("editedplayernames: {} overrides", ed.overrides));
                }
                ui.heading("Checks");
                for check in &session.player_scan.checks {
                    ui.colored_label(
                        if check.passed {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::LIGHT_RED
                        },
                        format!(
                            "[{}] {}: {}",
                            if check.passed { "ok" } else { "FAIL" },
                            check.name,
                            check.detail
                        ),
                    );
                }
                for check in &session.name_report.checks {
                    ui.colored_label(
                        if check.passed {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::LIGHT_RED
                        },
                        format!(
                            "[{}] {}: {}",
                            if check.passed { "ok" } else { "FAIL" },
                            check.name,
                            check.detail
                        ),
                    );
                }
                for note in &session.name_report.notes {
                    ui.colored_label(Color32::YELLOW, format!("note: {note}"));
                }
            });
    }

    fn show_modals(&mut self, ctx: &egui::Context) {
        if let Some((title, text)) = self.lua_message_box.clone() {
            let mut close = false;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(text);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                });
            if close {
                self.lua_message_box = None;
            }
        }
        if self.show_help {
            let mut open = self.show_help;
            egui::Window::new("FC27LE Help")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(700.0)
                .show(ctx, |ui| {
                    ui.heading("Window and overlay");
                    ui.label(
                        "FC27LE is a normal external window. F9 hides it when focused. \
                         To make F9 work while the game has focus, add a KDE custom shortcut \
                         (System Settings -> Shortcuts) bound to:",
                    );
                    ui.monospace("fc27le-gui --toggle");
                    ui.label(
                        "In the tested KWin/Wayland + XWayland setup, the X11 always-on-top state \
                         made the window click-through: the game kept mouse input. FC27LE therefore \
                         defaults to an interactive floating window. KWin is asked to raise/focus \
                         it on show through a small window-management script. Exclusive-fullscreen \
                         games may still cover it; borderless/windowed mode or a second monitor is \
                         the reliable case. Use Always on top only for view-only use.",
                    );
                    ui.separator();
                    ui.heading("Lua safety");
                    ui.label(
                        "The Lua runtime is read-only against game memory. Standard Lua file I/O \
                         remains available for compatibility; third-party scripts are untrusted and \
                         Lua 5.4 is not a security sandbox. os.execute/io.popen/package.loadlib are \
                         removed, and scripts have a time and memory limit.",
                    );
                    ui.separator();
                    ui.heading("Lua API");
                    ui.label(
                        "See the Lua API tab for the complete compatibility matrix. Unsupported \
                         functions raise explicit compatibility errors; writes are never simulated.",
                    );
                });
            self.show_help = open;
        }
    }

    fn apply_focus(&mut self) {
        match crate::kwin::focus_self(self.always_on_top) {
            Ok(()) => {
                self.focus_status = Some("KWin activation requested".to_string());
            }
            Err(error) => self.focus_status = Some(format!("unavailable: {error}")),
        }
    }

    fn poll_socket(&mut self, ctx: &egui::Context) {
        let mut commands = Vec::new();
        if let Some(listener) = &self.socket {
            while let Some(command) = listener.try_recv() {
                commands.push(command);
            }
        }
        for command in commands {
            match command {
                ToggleCommand::Toggle => {
                    if self.visible {
                        self.hide_window(ctx);
                    } else {
                        self.show_window(ctx);
                    }
                }
                ToggleCommand::Show => self.show_window(ctx),
                ToggleCommand::Hide => self.hide_window(ctx),
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if std::env::var_os("FC27LE_DEBUG").is_some() {
            eprintln!("[gui] frame");
        }
        self.poll_loader();
        self.poll_lua();
        self.poll_socket(ctx);
        if !self.window_level_initialized {
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.always_on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            }));
            self.window_level_initialized = true;
        }
        if self.visible && ctx.input(|input| input.key_pressed(egui::Key::F9)) {
            self.hide_window(ctx);
        }
        if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter))
            || ctx.input(|input| input.key_pressed(egui::Key::F5))
        {
            self.run_lua();
        }
        self.top_panel(ctx);
        self.lua_panel(ctx);
        self.detail_panel(ctx);
        self.central_panel(ctx);
        self.show_modals(ctx);
        if self.pending_focus {
            self.pending_focus = false;
            self.apply_focus();
            // KWin may not know the caption during the first frame; retry once
            // after the window has been mapped and registered.
            self.focus_retry_at = Some(Instant::now() + Duration::from_millis(800));
        }
        if self
            .focus_retry_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.focus_retry_at = None;
            self.apply_focus();
        }
        // The socket listener requests a repaint on incoming commands. This
        // timer also keeps the worker/event polling alive while a script runs.
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn sort_header(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    key: SortKey,
    active: &mut SortKey,
    descending: &mut bool,
) {
    let marker = if *active == key {
        if *descending { " v" } else { " ^" }
    } else {
        ""
    };
    if ui
        .add_sized([width, 20.0], egui::Button::new(format!("{label}{marker}")))
        .clicked()
    {
        if *active == key {
            *descending = !*descending;
        } else {
            *active = key;
            *descending = key != SortKey::Name;
        }
    }
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(RichText::new(label).strong());
    ui.label(value);
    ui.end_row();
}

fn status_style(status: ApiStatus) -> (&'static str, Color32) {
    match status {
        ApiStatus::SupportedTested => ("supported/tested", Color32::LIGHT_GREEN),
        ApiStatus::PartiallySupported => ("partial", Color32::YELLOW),
        ApiStatus::Unimplemented => ("unimplemented", Color32::GRAY),
        ApiStatus::RequiresRe => ("requires RE", Color32::LIGHT_RED),
        ApiStatus::NotApplicable => ("not applicable", Color32::GRAY),
    }
}

fn log_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
        LogLevel::Fatal => "FATAL",
    }
}

fn default_script_dirs() -> String {
    let mut dirs = vec!["scripts".to_string()];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(
            home.join(".local/share/fc27le/scripts")
                .display()
                .to_string(),
        );
        dirs.push(home.join(".config/fc27le/scripts").display().to_string());
    }
    dirs.join(";")
}

pub fn default_lua_script() -> &'static str {
    r#"-- FC27LE read-only Lua API v2 compatibility test.
-- Uses the same verified database layer as the Rust player browser.
local players = LE.db:GetTable("players")
if players == nil then
    LOGGER:LogError("players table not found")
    return
end

local record = players:GetFirstRecord()
local count = 0
while record > 0 do
    local playerid = players:GetRecordFieldValue(record, "playerid")
    local overall = players:GetRecordFieldValue(record, "overallrating")
    count = count + 1
    if count <= 10 then
        LOGGER:LogInfo(string.format("playerid %d  OVR %d", playerid, overall))
    end
    record = players:GetNextValidRecord()
end
LOGGER:LogInfo(string.format("read-only scan complete: %d valid players", count))
MessageBox("FC27LE", string.format("Read-only scan complete: %d players", count))
"#
}
