//! Sanitized diagnostics export for external testers.
//!
//! The report intentionally omits usernames, home/game/prefix paths, PIDs, IP
//! addresses, environment secrets, player records and memory contents.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;
pub const GIT_COMMIT: &str = match option_env!("FC27LE_GIT_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsReport {
    pub schema_version: u32,
    pub generated_at_unix: u64,
    pub app: AppInfo,
    pub host: HostInfo,
    pub game: Option<GameInfo>,
    pub validation: ValidationInfo,
    pub errors: Vec<String>,
    pub logs: Vec<LogLine>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub git_commit: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    pub os: String,
    pub kernel: String,
    pub desktop_environment: Option<String>,
    pub session_type: Option<String>,
    pub wayland: bool,
    pub display: bool,
    pub gpus: Vec<GpuInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub vendor_id: Option<String>,
    pub device_id: Option<String>,
    pub driver: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameInfo {
    pub executable: String,
    pub build_sha256: Option<String>,
    pub build_label: Option<String>,
    pub known_build: bool,
    pub connection: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationInfo {
    pub metadata_source: String,
    pub metadata_tables: usize,
    pub matched_tables: usize,
    pub verified_tables: usize,
    pub focus_verified: bool,
    pub player_slots: u32,
    pub valid_players: u32,
    pub invalid_players: u32,
    pub players_with_names: usize,
    pub name_errors: usize,
    pub lua_ready: bool,
    pub lua_tables: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsInput {
    pub connection: String,
    pub build_sha256: Option<String>,
    pub build_label: Option<String>,
    pub known_build: bool,
    pub metadata_source: String,
    pub metadata_tables: usize,
    pub matched_tables: usize,
    pub verified_tables: usize,
    pub focus_verified: bool,
    pub player_slots: u32,
    pub valid_players: u32,
    pub invalid_players: u32,
    pub players_with_names: usize,
    pub name_errors: usize,
    pub lua_ready: bool,
    pub lua_tables: Option<usize>,
    pub errors: Vec<String>,
    pub logs: Vec<(String, String)>,
}

pub fn build(input: DiagnosticsInput) -> DiagnosticsReport {
    DiagnosticsReport {
        schema_version: SCHEMA_VERSION,
        generated_at_unix: now_epoch(),
        app: AppInfo {
            name: "FC27LE".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: GIT_COMMIT.to_string(),
            read_only: true,
        },
        host: HostInfo {
            os: os_release(),
            kernel: read_trimmed("/proc/sys/kernel/osrelease").unwrap_or_else(|| "unknown".into()),
            desktop_environment: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            session_type: std::env::var("XDG_SESSION_TYPE").ok(),
            wayland: std::env::var_os("WAYLAND_DISPLAY").is_some(),
            display: std::env::var_os("DISPLAY").is_some(),
            gpus: gpu_info(),
        },
        game: input.build_sha256.map(|sha| GameInfo {
            executable: "FC27.exe".to_string(),
            build_sha256: Some(sha),
            build_label: input.build_label,
            known_build: input.known_build,
            connection: input.connection.clone(),
        }),
        validation: ValidationInfo {
            metadata_source: input.metadata_source,
            metadata_tables: input.metadata_tables,
            matched_tables: input.matched_tables,
            verified_tables: input.verified_tables,
            focus_verified: input.focus_verified,
            player_slots: input.player_slots,
            valid_players: input.valid_players,
            invalid_players: input.invalid_players,
            players_with_names: input.players_with_names,
            name_errors: input.name_errors,
            lua_ready: input.lua_ready,
            lua_tables: input.lua_tables,
        },
        errors: input
            .errors
            .into_iter()
            .map(|error| sanitize_text(&error))
            .collect(),
        logs: input
            .logs
            .into_iter()
            .map(|(level, message)| LogLine {
                level,
                message: sanitize_text(&message),
            })
            .collect(),
        limitations: vec![
            "Experimental alpha; read-only.".to_string(),
            "Only the verified FC 27 build/environment is supported.".to_string(),
            "No player records, memory dumps or metadata contents are included.".to_string(),
            "Lua 5.4 is not a security sandbox.".to_string(),
        ],
    }
}

pub fn to_json(input: DiagnosticsInput) -> Result<String, String> {
    serde_json::to_string_pretty(&build(input))
        .map_err(|error| format!("serialize diagnostics: {error}"))
}

/// Replace absolute paths and obvious personal path fragments. The report never
/// adds paths in the first place; this is defense in depth for error text.
pub fn sanitize_text(text: &str) -> String {
    let mut output = String::new();
    for token in text.split_whitespace() {
        if !output.is_empty() {
            output.push(' ');
        }
        let looks_like_path = token.starts_with('/')
            || token.starts_with("~/")
            || (token.len() > 2
                && token.as_bytes()[1] == b':'
                && matches!(token.as_bytes()[2], b'/' | b'\\'));
        if looks_like_path {
            output.push_str("<path>");
        } else {
            output.push_str(token);
        }
    }
    output
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn os_release() -> String {
    let Ok(text) = std::fs::read_to_string("/etc/os-release") else {
        return "unknown".to_string();
    };
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return value.trim_matches('"').to_string();
        }
    }
    "unknown".to_string()
}

fn gpu_info() -> Vec<GpuInfo> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut gpus = Vec::new();
    let mut names: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            (name.starts_with("card") && !name.contains('-')).then_some(name)
        })
        .collect();
    names.sort();
    for name in names {
        let base = format!("/sys/class/drm/{name}/device");
        let driver = std::fs::read_link(format!("{base}/driver"))
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|value| value.to_string_lossy().to_string())
            });
        gpus.push(GpuInfo {
            vendor_id: read_trimmed(&format!("{base}/vendor")),
            device_id: read_trimmed(&format!("{base}/device")),
            driver,
        });
    }
    gpus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_stable_and_sanitized() {
        let input = DiagnosticsInput {
            connection: "connected".to_string(),
            build_sha256: Some("abc".to_string()),
            build_label: Some("Tested build".to_string()),
            known_build: true,
            metadata_source: "embedded".to_string(),
            metadata_tables: 4,
            matched_tables: 4,
            verified_tables: 4,
            focus_verified: true,
            player_slots: 22098,
            valid_players: 21814,
            invalid_players: 284,
            players_with_names: 21814,
            name_errors: 0,
            lua_ready: true,
            lua_tables: Some(4),
            errors: vec!["failed to read /home/user/game/FC27.exe".to_string()],
            logs: vec![("INFO".to_string(), "opened /tmp/private.log".to_string())],
        };
        let json = to_json(input).unwrap();
        assert!(!json.contains("/home/"));
        assert!(!json.contains("/tmp/"));
        assert!(json.contains("\"schema_version\": 1"));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["app"]["read_only"], true);
        assert_eq!(parsed["validation"]["valid_players"], 21814);
    }

    #[test]
    fn sanitize_replaces_absolute_tokens() {
        assert_eq!(
            sanitize_text("open /home/user/x and C:\\games\\FC27.exe failed"),
            "open <path> and <path> failed"
        );
        assert_eq!(sanitize_text("no paths here"), "no paths here");
    }
}
