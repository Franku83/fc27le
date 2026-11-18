use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, ViewportCommand, WindowLevel};
use fc27_game::discovery::DiscoveredProcess;
use fc27_lua_compat::api::{
    DEFAULT_MEMORY_LIMIT_BYTES, DEFAULT_SCRIPT_TIMEOUT, LogLevel, LuaEvent, ScriptJob,
    ScriptSession,
};
use fc27_lua_compat::matrix::{ApiStatus, compatibility_matrix, status_counts};
use fc27_process::Pid;

use crate::config::{self, Config};
use crate::diagnostics::{self, DiagnosticsInput};
use crate::session::{LoadEvent, LoadRequest, Session, SessionLoader, discover_game};
use crate::socket::{ToggleCommand, ToggleListener, start_listener};
use crate::sort::{SortKey, filtered_sorted, selection_after_filter};

pub struct AppOptions {
    pub metadata: Option<PathBuf>,
    pub pid: Option<Pid>,
    pub max_mib: u64,
    pub filter: Option<String>,
    pub script: Option<PathBuf>,
    pub run_script: bool,
    pub no_auto_connect: bool,
    pub no_embedded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Players,
    Lua,
    Settings,
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
    use_embedded: bool,
    config: Config,
    config_status: Option<String>,
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
    search_focused: bool,
    sort_key: SortKey,
    sort_desc: bool,
    filtered: Vec<usize>,
    selected: Option<usize>,
    selected_player_id: Option<u32>,
    always_on_top: bool,
    visible: bool,
    window_level_initialized: bool,
    pending_focus: bool,
    focus_retry_at: Option<Instant>,
    focus_status: Option<String>,
    socket: Option<ToggleListener>,
    socket_error: Option<String>,
    main_tab: MainTab,
    lua_tab: LuaTab,
    show_help: bool,
    status_message: Option<String>,
    diagnostics_preview: Option<String>,
    diagnostics_path: String,
    diagnostics_status: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, options: AppOptions) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
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
        let config = Config::load();
        let env_metadata = std::env::var_os("FC27LE_METADATA")
            .map(PathBuf::from)
            .filter(|path| path.is_file());
        let selected_metadata = options
            .metadata
            .clone()
            .or(env_metadata)
            .or_else(|| config.metadata_path.clone());
        let use_embedded =
            selected_metadata.is_none() && !options.no_embedded && config.use_embedded_profile;
        let metadata_path = selected_metadata
            .as_ref()
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
        let lua_script_dirs = config
            .lua_script_dirs
            .clone()
            .unwrap_or_else(config::default_script_dirs);
        let diagnostics_path = config::save_default_diagnostics_path();
        let mut app = Self {
            discovery,
            discovery_error,
            pid,
            metadata_path,
            use_embedded,
            config,
            config_status: None,
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
            lua_script_dirs,
            lua_log: Vec::new(),
            lua_running: false,
            lua_status: None,
            lua_message_box: None,
            pending_auto_run: options.run_script,
            search: options.filter.clone().unwrap_or_default(),
            search_focused: false,
            sort_key: SortKey::Overall,
            sort_desc: true,
            filtered: Vec::new(),
            selected: None,
            selected_player_id: None,
            always_on_top: false,
            visible: true,
            window_level_initialized: false,
            pending_focus: true,
            focus_retry_at: None,
            focus_status: None,
            socket,
            socket_error,
            main_tab: MainTab::Players,
            lua_tab: LuaTab::Console,
            show_help: false,
            status_message: None,
            diagnostics_preview: None,
            diagnostics_path,
            diagnostics_status: None,
        };
        app.rebuild_filtered();
        if !options.no_auto_connect && app.pid.is_some() && app.load_source_ready() {
            app.start_load();
        } else if app.pid.is_none() {
            app.status_message =
                Some("FC 27 was not found. Start the game, then click Refresh.".to_string());
        } else if !app.load_source_ready() {
            app.status_message = Some(
                "Select a matching metadata XML in Settings, or use the embedded profile."
                    .to_string(),
            );
        }
        app
    }

    fn load_source_ready(&self) -> bool {
        self.use_embedded || !self.metadata_path.trim().is_empty()
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
                    Some("FC 27 was not found. Start the game, then click Refresh.".to_string());
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
        let metadata_path = if self.use_embedded {
            None
        } else {
            let path = PathBuf::from(self.metadata_path.trim());
            if path.as_os_str().is_empty() {
                self.session_error = Some("metadata path is empty".to_string());
                return;
            }
            if !path.is_file() {
                self.session_error = Some(format!("metadata file not found: {}", path.display()));
                return;
            }
            Some(path)
        };
        self.session_loading = true;
        self.session_error = None;
        self.session_status = Some(if metadata_path.is_some() {
            format!("Loading pid {} with external metadata", pid.0)
        } else {
            format!("Loading pid {} with embedded profile", pid.0)
        });
        self.loader.load(LoadRequest {
            pid,
            metadata_path,
            max_mib: self.max_mib,
        });
    }

    fn rebuild_filtered(&mut self) {
        if let Some(session) = &self.session {
            self.filtered = filtered_sorted(
                &session.player_scan.records,
                &session.search_index,
                &self.search,
                self.sort_key,
                self.sort_desc,
            );
            let records = &session.player_scan.records;
            self.selected = selection_after_filter(
                records,
                &self.filtered,
                self.selected,
                self.selected_player_id,
            );
            if let Some(index) = self.selected {
                self.selected_player_id = Some(records[index].player_id);
            }
        } else {
            self.filtered.clear();
            self.selected = None;
            self.selected_player_id = None;
        }
    }

    fn select_index(&mut self, index: usize) {
        self.selected = Some(index);
        if let Some(session) = &self.session {
            if let Some(record) = session.player_scan.records.get(index) {
                self.selected_player_id = Some(record.player_id);
            }
        }
    }

    fn poll_loader(&mut self) {
        while let Some(event) = self.loader.try_recv() {
            match event {
                LoadEvent::Started { pid, metadata } => {
                    self.session_loading = true;
                    self.session_status = Some(match metadata {
                        Some(path) => format!("Loading pid {} with {}", pid.0, path.display()),
                        None => format!("Loading pid {} with embedded profile", pid.0),
                    });
                }
                LoadEvent::Finished(Ok(session)) => {
                    self.session_loading = false;
                    let previous_player_id = self.selected_player_id;
                    self.session_status = Some(format!(
                        "Loaded {} players and {} names from pid {} using {}",
                        session.player_scan.valid_slots,
                        session.name_report.emitted_with_names,
                        session.pid.0,
                        session.metadata_source
                    ));
                    self.session_error = None;
                    let data_path = session
                        .metadata_path
                        .as_ref()
                        .and_then(|path| path.parent())
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "embedded".to_string());
                    let id_map_path = std::env::var_os("FC27LE_ID_MAP")
                        .map(PathBuf::from)
                        .or_else(|| {
                            let directory = PathBuf::from(&data_path);
                            if directory.is_dir() {
                                let candidate = directory.join("id_map.json");
                                candidate.is_file().then_some(candidate)
                            } else {
                                None
                            }
                        });
                    let player_records = Arc::new(session.player_scan.records.clone());
                    let lua_session = ScriptSession {
                        pid: session.pid,
                        module_name: session.module_name.clone(),
                        metadata: session.metadata.clone(),
                        data_path,
                        max_db_mib: self.max_mib,
                        player_records,
                        id_map_path,
                    };
                    self.lua_ready = false;
                    self.lua_session_error = None;
                    self.lua.set_session(lua_session);
                    self.pid = Some(session.pid);
                    self.session = Some(*session);
                    self.rebuild_filtered();
                    let records = &self
                        .session
                        .as_ref()
                        .expect("session set")
                        .player_scan
                        .records;
                    self.selected =
                        selection_after_filter(records, &self.filtered, None, previous_player_id);
                    if let Some(index) = self.selected {
                        self.selected_player_id = Some(records[index].player_id);
                    }
                    self.persist_config();
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
                "Lua session is not ready; connect to FC 27 and load a profile first.".to_string(),
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

    fn persist_config(&mut self) {
        self.config.metadata_path = if self.use_embedded || self.metadata_path.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.metadata_path.trim()))
        };
        self.config.use_embedded_profile = self.use_embedded;
        self.config.lua_script_dirs = Some(self.lua_script_dirs.clone());
        self.config.always_on_top = self.always_on_top;
        match self.config.save() {
            Ok(()) => self.config_status = Some("Settings saved".to_string()),
            Err(error) => self.config_status = Some(format!("Save settings: {error}")),
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
        egui::TopBottomPanel::top("top")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.heading("FC27LE");
                    ui.label(
                        RichText::new("ALPHA · READ-ONLY")
                            .small()
                            .color(Color32::from_rgb(220, 180, 90)),
                    );
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
                    if let Some(session) = &self.session {
                        ui.label(
                            RichText::new(
                                session
                                    .build_sha256
                                    .as_deref()
                                    .map(|sha| format!("build {}…", &sha[..12.min(sha.len())]))
                                    .unwrap_or_else(|| "build unavailable".to_string()),
                            )
                            .small()
                            .weak(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Help").clicked() {
                            self.show_help = true;
                        }
                        if ui.button("Hide (F9)").clicked() {
                            self.hide_window(ctx);
                        }
                        if ui.button("Reconnect").clicked() {
                            self.pid = None;
                            self.ensure_pid();
                            self.start_load();
                        }
                        if ui.button("Refresh").clicked() {
                            self.start_load();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.main_tab, MainTab::Players, "Players");
                    ui.selectable_value(&mut self.main_tab, MainTab::Lua, "Lua Scripts");
                    ui.selectable_value(&mut self.main_tab, MainTab::Settings, "Settings");
                    ui.separator();
                    if let Some(session) = &self.session {
                        ui.label(
                            RichText::new(format!(
                                "{} · {}/{} metadata tables · {} valid players",
                                session.metadata_source,
                                session.metadata_matched,
                                session.metadata_table_count,
                                session.player_scan.valid_slots
                            ))
                            .small()
                            .weak(),
                        );
                    }
                });
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
                ui.add_space(2.0);
            });
    }

    fn players_view(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("player_detail")
            .default_width(350.0)
            .min_width(260.0)
            .resizable(true)
            .show(ctx, |ui| self.detail_contents(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                let response = ui.add_sized(
                    [ui.available_width().min(520.0), 24.0],
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("name, partial name, multiple words, or exact player ID"),
                );
                self.search_focused = response.has_focus();
                if response.changed() {
                    self.rebuild_filtered();
                }
                if ui.button("Clear").clicked() {
                    self.search.clear();
                    self.rebuild_filtered();
                }
            });
            if let Some(session) = &self.session {
                ui.label(
                    RichText::new(format!(
                        "{} result(s) of {}",
                        self.filtered.len(),
                        session.player_scan.valid_slots
                    ))
                    .small()
                    .weak(),
                );
            }
            ui.separator();
            if self.session.is_none() {
                ui.add_space(24.0);
                ui.heading("Connect to FC 27");
                ui.label("1. Start FC 27 and wait until the game database is loaded.");
                ui.label("2. Click Refresh in the top bar.");
                ui.label("3. Use the embedded profile or select your own matching metadata XML in Settings.");
                if ui.button("Open Settings").clicked() {
                    self.main_tab = MainTab::Settings;
                }
                return;
            }
            if self.filtered.is_empty() {
                ui.add_space(20.0);
                ui.heading("No players match");
                ui.label("Try a shorter name, remove a word, or clear the search.");
                if ui.button("Clear search").clicked() {
                    self.search.clear();
                    self.rebuild_filtered();
                }
                return;
            }

            let sort_before = (self.sort_key, self.sort_desc);
            let name_width = (ui.available_width() - 360.0).max(140.0);
            ui.horizontal(|ui| {
                sort_header(
                    ui,
                    "Name",
                    name_width,
                    SortKey::Name,
                    &mut self.sort_key,
                    &mut self.sort_desc,
                );
                sort_header(ui, "ID", 86.0, SortKey::PlayerId, &mut self.sort_key, &mut self.sort_desc);
                sort_header(ui, "OVR", 52.0, SortKey::Overall, &mut self.sort_key, &mut self.sort_desc);
                sort_header(ui, "POT", 52.0, SortKey::Potential, &mut self.sort_key, &mut self.sort_desc);
                sort_header(ui, "Height", 70.0, SortKey::Height, &mut self.sort_key, &mut self.sort_desc);
                sort_header(ui, "Weight", 70.0, SortKey::Weight, &mut self.sort_key, &mut self.sort_desc);
            });
            if (self.sort_key, self.sort_desc) != sort_before {
                self.rebuild_filtered();
            }

            let session = self.session.as_ref().expect("session checked above");
            let records = &session.player_scan.records;
            let filtered = std::mem::take(&mut self.filtered);
            let selected = self.selected;
            let mut clicked = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, 22.0, filtered.len(), |ui, range| {
                    for row in range {
                        let index = filtered[row];
                        let record = &records[index];
                        let is_selected = selected == Some(index);
                        let row = ui.horizontal(|ui| {
                            let name = record.display_name.as_deref().unwrap_or("-");
                            if ui
                                .add_sized(
                                    [name_width, 19.0],
                                    egui::Button::new(name)
                                        .selected(is_selected)
                                        .truncate(),
                                )
                                .clicked()
                            {
                                clicked = Some(index);
                            }
                            ui.add_sized([86.0, 19.0], egui::Label::new(record.player_id.to_string()));
                            ui.add_sized([52.0, 19.0], egui::Label::new(record.overall_rating.to_string()));
                            ui.add_sized([52.0, 19.0], egui::Label::new(record.potential.to_string()));
                            ui.add_sized([70.0, 19.0], egui::Label::new(record.height_cm.to_string()));
                            ui.add_sized([70.0, 19.0], egui::Label::new(record.weight_kg.to_string()));
                        });
                        if row.response.interact(egui::Sense::click()).clicked() {
                            clicked = Some(index);
                        }
                    }
                });
            self.filtered = filtered;
            self.selected = selected;
            if let Some(index) = clicked {
                self.select_index(index);
            }

            if !self.search_focused {
                let current_position = self
                    .selected
                    .and_then(|selected| self.filtered.iter().position(|index| *index == selected));
                let page = 20usize;
                let target = if ctx.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                    Some(current_position.map_or(0, |position| position + 1))
                } else if ctx.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                    Some(current_position.map_or(0, |position| position.saturating_sub(1)))
                } else if ctx.input(|input| input.key_pressed(egui::Key::Home)) {
                    Some(0)
                } else if ctx.input(|input| input.key_pressed(egui::Key::End)) {
                    Some(self.filtered.len().saturating_sub(1))
                } else if ctx.input(|input| input.key_pressed(egui::Key::PageDown)) {
                    Some((current_position.unwrap_or(0) + page).min(self.filtered.len().saturating_sub(1)))
                } else if ctx.input(|input| input.key_pressed(egui::Key::PageUp)) {
                    Some(current_position.unwrap_or(0).saturating_sub(page))
                } else {
                    None
                };
                if let Some(target) = target {
                    if let Some(index) = self.filtered.get(target).copied() {
                        self.select_index(index);
                                    }
                }
            }
        });
    }

    fn detail_contents(&mut self, ui: &mut egui::Ui) {
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
    }

    fn lua_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.lua_tab, LuaTab::Console, "Console");
                ui.selectable_value(&mut self.lua_tab, LuaTab::Api, "API matrix");
                ui.selectable_value(&mut self.lua_tab, LuaTab::Session, "Lua session");
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
                LuaTab::Session => self.draw_lua_session(ui),
            }
        });
    }

    fn draw_console(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Script:");
            ui.add_sized(
                [ui.available_width().min(520.0), 20.0],
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
                [ui.available_width().min(520.0), 20.0],
                egui::TextEdit::singleline(&mut self.lua_script_dirs)
                    .hint_text("semicolon-separated directories"),
            );
            if ui
                .add_enabled(
                    self.lua_ready && !self.lua_running,
                    egui::Button::new("Run (F5)"),
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
                            .desired_rows(20),
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

    fn draw_lua_session(&mut self, ui: &mut egui::Ui) {
        let Some(session) = &self.session else {
            ui.label("No session loaded.");
            return;
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Lua database");
                ui.monospace(format!("metadata source: {}", session.metadata_source));
                ui.monospace(format!(
                    "tables exposed to Lua: {}",
                    session.metadata_table_count
                ));
                ui.monospace(format!("database tables: {}", session.db_table_count));
                ui.monospace(format!(
                    "players: {} slots / {} valid",
                    session.player_scan.slots_scanned, session.player_scan.valid_slots
                ));
                ui.monospace(format!(
                    "names: {}/{} resolved, {} errors",
                    session.name_report.emitted_with_names,
                    session.name_report.emitted_with_names
                        + session.name_report.emitted_without_names,
                    session.name_report.name_errors
                ));
                ui.separator();
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
            });
    }

    fn settings_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.heading("Settings");
                    ui.separator();
                    ui.heading("Metadata profile");
                    ui.radio_value(
                        &mut self.use_embedded,
                        true,
                        "Embedded minimal profile (verified build only)",
                    );
                    ui.label(
                        RichText::new(
                            "Four tables and only the fields used by the player/name reader. \
                             Every field is checked against the live descriptor and only the exact tested build is accepted.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.radio_value(&mut self.use_embedded, false, "External metadata XML");
                    ui.add_enabled_ui(!self.use_embedded, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("XML:");
                            ui.add_sized(
                                [ui.available_width().min(560.0), 22.0],
                                egui::TextEdit::singleline(&mut self.metadata_path)
                                    .hint_text("/path/to/fifa_ng_db-meta.xml"),
                            );
                            if ui.button("Load / validate").clicked() {
                                self.start_load();
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Save settings").clicked() {
                            self.persist_config();
                        }
                        if ui.button("Refresh connection").clicked() {
                            self.start_load();
                        }
                    });
                    if let Some(status) = &self.config_status {
                        ui.colored_label(Color32::GRAY, status);
                    }
                    if !self.use_embedded && self.metadata_path.trim().is_empty() {
                        ui.colored_label(
                            Color32::YELLOW,
                            "External XML mode requires a matching metadata file.",
                        );
                    }
                    if let Some(error) = &self.session_error {
                        ui.colored_label(Color32::LIGHT_RED, format!("Validation error: {error}"));
                    }
                    if let Some(session) = &self.session {
                        ui.label(
                            RichText::new(format!(
                                "Active: {} · {}/{} tables matched · focus {} · {} valid players",
                                session.metadata_source,
                                session.metadata_matched,
                                session.metadata_table_count,
                                if session.focus_verified { "verified" } else { "FAILED" },
                                session.player_scan.valid_slots
                            ))
                            .small(),
                        );
                        if let Some(profile) = &session.metadata_profile_id {
                            ui.monospace(format!("profile: {profile}"));
                        }
                        if let Some(sha) = &session.metadata_sha256 {
                            ui.monospace(format!("metadata sha256: {sha}"));
                        }
                    }
                    ui.separator();
                    ui.heading("Connection");
                    if let Some(session) = &self.session {
                        ui.monospace(format!("pid: {}", session.pid.0));
                        ui.monospace(format!("build: {}", session.build_label.as_deref().unwrap_or("UNVERIFIED")));
                        ui.monospace(format!(
                            "module: {} @ {:#x} ({} bytes)",
                            session.module_name, session.module_base, session.module_size
                        ));
                        ui.monospace(format!("database tables: {}", session.db_table_count));
                        for (name, count) in &session.databases {
                            ui.monospace(format!("  {name}: {count}"));
                        }
                    } else {
                        ui.label("Not connected. Start FC 27 and click Refresh.");
                    }
                    if let Some(status) = &self.focus_status {
                        ui.label(RichText::new(format!("Window activation: {status}")).small().weak());
                    }
                    ui.separator();
                    ui.heading("Diagnostics");
                    ui.label(
                        "Preview a sanitized report before saving. It contains no usernames, paths, PIDs, player records, memory or metadata contents.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Preview report").clicked() {
                            match self.diagnostics_json() {
                                Ok(json) => {
                                    self.diagnostics_preview = Some(json);
                                    self.diagnostics_status = Some("Preview generated; nothing was saved.".to_string());
                                }
                                Err(error) => self.diagnostics_status = Some(error),
                            }
                        }
                        if ui.button("Copy report").clicked() {
                            if self.diagnostics_preview.is_none() {
                                if let Ok(json) = self.diagnostics_json() {
                                    self.diagnostics_preview = Some(json);
                                }
                            }
                            if let Some(json) = &self.diagnostics_preview {
                                ui.ctx().copy_text(json.clone());
                                self.diagnostics_status = Some("Report copied to clipboard".to_string());
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Save path:");
                        ui.add_sized(
                            [ui.available_width().min(480.0), 20.0],
                            egui::TextEdit::singleline(&mut self.diagnostics_path),
                        );
                        if ui.button("Save report").clicked() {
                            if self.diagnostics_preview.is_none() {
                                if let Ok(json) = self.diagnostics_json() {
                                    self.diagnostics_preview = Some(json);
                                }
                            }
                            if let Some(json) = &self.diagnostics_preview {
                                let target = std::path::Path::new(self.diagnostics_path.trim());
                                match diagnostics::save_report(target, json) {
                                    Ok(()) => self.diagnostics_status = Some(format!("Saved {}", self.diagnostics_path)),
                                    Err(error) => self.diagnostics_status = Some(format!("Save report: {error}")),
                                }
                            }
                        }
                    });
                    if let Some(status) = &self.diagnostics_status {
                        ui.colored_label(Color32::GRAY, status);
                    }
                    if let Some(preview) = &self.diagnostics_preview {
                        ui.collapsing("Report preview", |ui| {
                            let mut text = preview.clone();
                            egui::ScrollArea::vertical()
                                .max_height(360.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut text)
                                            .code_editor()
                                            .desired_rows(14)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                        });
                    }
                    ui.separator();
                    ui.heading("Hotkeys and window");
                    ui.monospace("F9: hide while focused");
                    ui.monospace("KDE shortcut: fc27le-gui --toggle");
                    ui.monospace("F5: run Lua from the Lua view");
                    ui.label(
                        "The window is an external XWayland/Wayland window. Exclusive-fullscreen games can keep input; use borderless/windowed mode or a second monitor. No injected overlay is claimed.",
                    );
                    ui.separator();
                    ui.heading("About");
                    ui.monospace(format!(
                        "FC27LE {} · commit {}",
                        env!("CARGO_PKG_VERSION"),
                        diagnostics::GIT_COMMIT
                    ));
                    ui.label("Experimental, open source, read-only. MIT License.");
                });
        });
    }

    fn diagnostics_json(&self) -> Result<String, String> {
        let mut input = DiagnosticsInput {
            connection: if self.session.is_some() {
                "connected".to_string()
            } else if self.session_loading {
                "loading".to_string()
            } else {
                "disconnected".to_string()
            },
            lua_ready: self.lua_ready,
            lua_tables: self
                .session
                .as_ref()
                .map(|session| session.metadata_table_count),
            ..DiagnosticsInput::default()
        };
        if let Some(session) = &self.session {
            input.build_sha256 = session.build_sha256.clone();
            input.build_label = session.build_label.clone();
            input.known_build = session.build_known;
            input.metadata_source = session.metadata_source.clone();
            input.metadata_tables = session.metadata_table_count;
            input.matched_tables = session.metadata_matched;
            input.verified_tables = session.metadata_verified;
            input.focus_verified = session.focus_verified;
            input.player_slots = session.player_scan.slots_scanned;
            input.valid_players = session.player_scan.valid_slots;
            input.invalid_players = session.player_scan.invalid_slots;
            input.players_with_names = session.name_report.emitted_with_names;
            input.name_errors = session.name_report.name_errors;
        }
        if let Some(error) = &self.session_error {
            input.errors.push(error.clone());
        }
        if let Some(error) = &self.discovery_error {
            input.errors.push(error.clone());
        }
        if let Some(error) = &self.lua_session_error {
            input.errors.push(error.clone());
        }
        // Lua output can contain player records chosen by the script. Keep the
        // diagnostic report to app-level events only.
        if let Some(status) = &self.session_status {
            input.logs.push(("INFO".to_string(), status.clone()));
        }
        if let Some(status) = &self.config_status {
            input.logs.push(("INFO".to_string(), status.clone()));
        }
        if let Some(status) = &self.focus_status {
            input.logs.push(("INFO".to_string(), status.clone()));
        }
        diagnostics::to_json(input)
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
                    ui.label("FC27LE is a normal external window. F9 hides it when focused. For a global shortcut, bind a KDE custom shortcut to:");
                    ui.monospace("fc27le-gui --toggle");
                    ui.label("In the tested KWin/Wayland + XWayland setup, the X11 always-on-top state made the window click-through: the game kept mouse input. FC27LE defaults to an interactive floating window. Exclusive-fullscreen games may still cover it; borderless/windowed mode or a second monitor is reliable. No injected overlay is claimed.");
                    ui.separator();
                    ui.heading("Lua safety");
                    ui.label("The Lua runtime is read-only. Standard Lua file I/O remains available; third-party scripts are untrusted and Lua 5.4 is not a security sandbox. os.execute/io.popen/package.loadlib are removed, and scripts have time, instruction and memory limits.");
                    ui.separator();
                    ui.heading("Metadata");
                    ui.label("The embedded profile covers only players and names for the exact tested build. External XML is required for other builds and full-table Lua coverage. The XML is never bundled or downloaded.");
                });
            self.show_help = open;
        }
    }

    fn apply_focus(&mut self) {
        match crate::kwin::focus_self(self.always_on_top) {
            Ok(()) => self.focus_status = Some("KWin activation requested".to_string()),
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
        match self.main_tab {
            MainTab::Players => self.players_view(ctx),
            MainTab::Lua => self.lua_view(ctx),
            MainTab::Settings => self.settings_view(ctx),
        }
        self.show_modals(ctx);
        if self.pending_focus {
            self.pending_focus = false;
            self.apply_focus();
            self.focus_retry_at = Some(Instant::now() + Duration::from_millis(800));
        }
        if self
            .focus_retry_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.focus_retry_at = None;
            self.apply_focus();
        }
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
