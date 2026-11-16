use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use fc27_game::anchors::ScanOptions;
use fc27_game::build_identity::sha256_file;
use fc27_game::db_metadata::{self, Metadata};
use fc27_game::db_service::inspect;
use fc27_game::discovery::{self, DiscoveredProcess};
use fc27_game::name_pool::{NameResolutionReport, PlayerNameResolver};
use fc27_game::player_store::{PlayerScan, scan_players};
use fc27_game::profile::identify_build;
use fc27_platform_linux::{LinuxProcess, LinuxProvider};
use fc27_process::modules::find_module;
use fc27_process::{Pid, ProcessHandle, ProcessProvider};

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub pid: Pid,
    pub metadata_path: PathBuf,
    pub max_mib: u64,
}

pub enum LoadEvent {
    Started { pid: Pid, metadata_path: PathBuf },
    Finished(Result<Box<Session>, String>),
}

pub struct SessionLoader {
    requests: Sender<LoadRequest>,
    events: Receiver<LoadEvent>,
    handle: Option<JoinHandle<()>>,
}

impl SessionLoader {
    pub fn spawn() -> Self {
        let (request_tx, request_rx): (Sender<LoadRequest>, Receiver<LoadRequest>) =
            mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("fc27le-session".to_string())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let _ = event_tx.send(LoadEvent::Started {
                        pid: request.pid,
                        metadata_path: request.metadata_path.clone(),
                    });
                    let result = load_session(&request)
                        .map(Box::new)
                        .map_err(|error| error.to_string());
                    let _ = event_tx.send(LoadEvent::Finished(result));
                }
            })
            .expect("spawn session loader");
        Self {
            requests: request_tx,
            events: event_rx,
            handle: Some(handle),
        }
    }

    pub fn load(&self, request: LoadRequest) {
        let _ = self.requests.send(request);
    }

    pub fn try_recv(&self) -> Option<LoadEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for SessionLoader {
    fn drop(&mut self) {
        // Close the request channel first, then join the worker. Joining while
        // the sender is still alive would deadlock the worker in `recv`.
        let (dummy, _) = mpsc::channel();
        let requests = std::mem::replace(&mut self.requests, dummy);
        drop(requests);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub struct Session {
    pub pid: Pid,
    pub module_name: String,
    pub module_base: u64,
    pub module_size: u64,
    pub build_sha256: Option<String>,
    pub build_label: Option<String>,
    pub build_known: bool,
    pub metadata_path: PathBuf,
    pub metadata_sha256: String,
    pub metadata_size: u64,
    pub metadata: Metadata,
    pub db_table_count: usize,
    pub databases: Vec<(String, usize)>,
    pub metadata_table_count: usize,
    pub metadata_matched: usize,
    pub metadata_verified: usize,
    pub focus_verified: bool,
    pub player_scan: PlayerScan,
    pub name_report: NameResolutionReport,
}

fn load_session(request: &LoadRequest) -> Result<Session, String> {
    let process = LinuxProcess::open(request.pid).map_err(|error| error.to_string())?;
    let modules = process.modules().map_err(|error| error.to_string())?;
    let module = find_module(&modules, "FC27.exe")
        .cloned()
        .ok_or_else(|| "FC27.exe module is not loaded in the selected process".to_string())?;

    let options = ScanOptions {
        max_bytes: request.max_mib.saturating_mul(1024 * 1024),
        max_matches: 4,
        executable_sections_only: true,
    };
    let report = inspect(&process, &module, options).map_err(|error| error.to_string())?;
    if !report.all_checks_passed {
        return Err("database service path failed its verification checks".to_string());
    }
    let loaded = db_metadata::load_metadata_file(&request.metadata_path)
        .map_err(|error| error.to_string())?;
    let validation = db_metadata::validate(&loaded.parsed, &report.tables, &process, "players")
        .map_err(|error| error.to_string())?;
    if !validation.focus_verified {
        return Err(
            "the `players` descriptor did not validate against this metadata/build".to_string(),
        );
    }
    let mut scan = scan_players(&process, &loaded.parsed, &report.tables, usize::MAX)
        .map_err(|error| error.to_string())?;
    if !scan.all_checks_passed {
        return Err("player storage checks did not pass".to_string());
    }
    let names = PlayerNameResolver::load(&process, &loaded.parsed, &report.tables)
        .map_err(|error| error.to_string())?;
    for record in &mut scan.records {
        let resolved = names.resolve_player(
            record.player_id,
            record.first_name_id,
            record.last_name_id,
            record.common_name_id,
        );
        record.first_name = resolved.first_name;
        record.last_name = resolved.last_name;
        record.common_name = resolved.common_name;
        record.display_name = resolved.display_name;
        record.name_source = Some(resolved.source.to_string());
        record.name_error = resolved.error;
    }
    let name_report = names.report(&scan.records);
    let build_sha256 = sha256_file(Path::new(&module.path)).ok();
    let build_label = build_sha256
        .as_deref()
        .and_then(identify_build)
        .map(|build| build.label.to_string());
    let build_known = build_label.is_some();
    let databases = report
        .databases
        .iter()
        .map(|database| (database.name.clone(), database.table_count))
        .collect();
    Ok(Session {
        pid: request.pid,
        module_name: module.name,
        module_base: module.base,
        module_size: module.size,
        build_sha256,
        build_label,
        build_known,
        metadata_path: request.metadata_path.clone(),
        metadata_sha256: loaded.sha256,
        metadata_size: loaded.size_bytes,
        metadata: loaded.parsed,
        db_table_count: report.table_count,
        databases,
        metadata_table_count: validation.metadata_table_count,
        metadata_matched: validation.matched_tables,
        metadata_verified: validation.verified_tables,
        focus_verified: validation.focus_verified,
        player_scan: scan,
        name_report,
    })
}

pub fn discover_game() -> Result<Option<DiscoveredProcess>, String> {
    let provider = LinuxProvider::new();
    let processes = provider.list().map_err(|error| error.to_string())?;
    let report = discovery::analyze(&processes);
    Ok(report.game)
}

pub fn default_metadata_path() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("FC27LE_METADATA") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    let candidates = [
        PathBuf::from("fifa_ng_db-meta.xml"),
        PathBuf::from("data/db/fifa_ng_db-meta.xml"),
        PathBuf::from("scripts/fifa_ng_db-meta.xml"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join(".local/share/fc27le/fifa_ng_db-meta.xml");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}
