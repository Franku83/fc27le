use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use fc27_game::anchors::ScanOptions;
use fc27_game::build_identity::sha256_file;
use fc27_game::db_metadata::{self, Metadata};
use fc27_game::db_service::inspect;
use fc27_game::discovery::{self, DiscoveredProcess};
use fc27_game::embedded_profile::{
    PROFILE_BUILD_SHA256, PROFILE_ID, metadata as embedded_metadata,
};
use fc27_game::name_pool::{NameResolutionReport, PlayerNameResolver};
use fc27_game::player_store::{PlayerScan, scan_players};
use fc27_game::profile::identify_build;
use fc27_platform_linux::{LinuxProcess, LinuxProvider};
use fc27_process::modules::find_module;
use fc27_process::{Pid, ProcessHandle, ProcessProvider};

use crate::sort::SearchIndex;

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub pid: Pid,
    /// `None` selects the compact embedded profile for the verified build.
    pub metadata_path: Option<PathBuf>,
    pub max_mib: u64,
}

pub enum LoadEvent {
    Started { pid: Pid, metadata: Option<PathBuf> },
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
                        metadata: request.metadata_path.clone(),
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
    pub metadata_path: Option<PathBuf>,
    pub metadata_sha256: Option<String>,
    pub metadata_source: String,
    pub metadata_profile_id: Option<String>,
    pub metadata: Metadata,
    pub db_table_count: usize,
    pub databases: Vec<(String, usize)>,
    pub metadata_table_count: usize,
    pub metadata_matched: usize,
    pub metadata_verified: usize,
    pub focus_verified: bool,
    pub player_scan: PlayerScan,
    pub name_report: NameResolutionReport,
    pub search_index: SearchIndex,
}

fn load_session(request: &LoadRequest) -> Result<Session, String> {
    let process = LinuxProcess::open(request.pid).map_err(|error| error.to_string())?;
    let modules = process.modules().map_err(|error| error.to_string())?;
    let module = find_module(&modules, "FC27.exe")
        .cloned()
        .ok_or_else(|| "FC27.exe module is not loaded in the selected process".to_string())?;
    let build_sha256 = sha256_file(Path::new(&module.path)).ok();

    let options = ScanOptions {
        max_bytes: request.max_mib.saturating_mul(1024 * 1024),
        max_matches: 4,
        executable_sections_only: true,
    };
    let report = inspect(&process, &module, options).map_err(|error| error.to_string())?;
    if !report.all_checks_passed {
        return Err("database service path failed its verification checks".to_string());
    }

    let (metadata, metadata_path, metadata_sha256, metadata_source, profile_id) =
        match &request.metadata_path {
            Some(path) => {
                let loaded = db_metadata::load_metadata_file(path)
                    .map_err(|error| format!("metadata: {error}"))?;
                (
                    loaded.parsed,
                    Some(path.clone()),
                    Some(loaded.sha256),
                    "external XML".to_string(),
                    None,
                )
            }
            None => {
                if build_sha256.as_deref() != Some(PROFILE_BUILD_SHA256) {
                    return Err(format!(
                        "no external metadata XML selected and the embedded profile only supports \
                         build {PROFILE_BUILD_SHA256}; select a matching XML in Settings"
                    ));
                }
                (
                    embedded_metadata(),
                    None,
                    None,
                    "embedded minimal profile".to_string(),
                    Some(PROFILE_ID.to_string()),
                )
            }
        };

    let validation = db_metadata::validate(&metadata, &report.tables, &process, "players")
        .map_err(|error| error.to_string())?;
    if !validation.focus_verified {
        return Err(
            "the `players` descriptor did not validate against this metadata/build".to_string(),
        );
    }
    let mut scan = scan_players(&process, &metadata, &report.tables, usize::MAX)
        .map_err(|error| error.to_string())?;
    if !scan.all_checks_passed {
        return Err("player storage checks did not pass".to_string());
    }
    let names = PlayerNameResolver::load(&process, &metadata, &report.tables)
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
    let search_index = SearchIndex::build(&scan.records);
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
        metadata_path,
        metadata_sha256,
        metadata_source,
        metadata_profile_id: profile_id,
        metadata,
        db_table_count: report.table_count,
        databases,
        metadata_table_count: validation.metadata_table_count,
        metadata_matched: validation.matched_tables,
        metadata_verified: validation.verified_tables,
        focus_verified: validation.focus_verified,
        player_scan: scan,
        name_report,
        search_index,
    })
}

pub fn discover_game() -> Result<Option<DiscoveredProcess>, String> {
    let provider = LinuxProvider::new();
    let processes = provider.list().map_err(|error| error.to_string())?;
    let report = discovery::analyze(&processes);
    Ok(report.game)
}
