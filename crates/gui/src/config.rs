//! Per-user GUI configuration.
//!
//! Only non-sensitive preferences are stored. No player data, memory contents,
//! game paths or Wine prefix paths are persisted.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_DIR: &str = "fc27le";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// External metadata XML selected by the user. `None` means the embedded
    /// profile is preferred when the verified build is detected.
    #[serde(default)]
    pub metadata_path: Option<PathBuf>,
    /// Prefer the compact embedded profile when no external XML is configured.
    #[serde(default = "default_true")]
    pub use_embedded_profile: bool,
    /// Optional semicolon-separated Lua helper directory list.
    #[serde(default)]
    pub lua_script_dirs: Option<String>,
    /// Last UI preference; no game data is stored.
    #[serde(default)]
    pub always_on_top: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            metadata_path: None,
            use_embedded_profile: true,
            lua_script_dirs: None,
            always_on_top: false,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        let mut config: Self = serde_json::from_str(&text).unwrap_or_default();
        if let Some(metadata) = &config.metadata_path {
            if !metadata.is_file() {
                config.metadata_path = None;
            }
        }
        config
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = config_path() else {
            return Err("no per-user configuration directory is available".to_string());
        };
        let Some(parent) = path.parent() else {
            return Err(format!("invalid configuration path {}", path.display()));
        };
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
        let temporary = parent.join(format!(".config-{}.tmp", std::process::id()));
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| format!("serialize configuration: {error}"))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("chmod {}: {error}", temporary.display()))?;
        fs::rename(&temporary, &path).map_err(|error| {
            format!(
                "rename {} -> {}: {error}",
                temporary.display(),
                path.display()
            )
        })?;
        Ok(())
    }
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|directory| directory.join("config.json"))
}

pub fn config_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path.join(APP_DIR));
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join(APP_DIR))
}

pub fn data_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path.join(APP_DIR));
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share").join(APP_DIR))
}

pub fn default_script_dirs() -> String {
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

pub fn save_default_diagnostics_path() -> String {
    data_dir()
        .map(|directory| directory.join("diagnostics.json").display().to_string())
        .unwrap_or_else(|| "diagnostics.json".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_embedded_and_has_no_paths() {
        let config = Config::default();
        assert!(config.use_embedded_profile);
        assert!(config.metadata_path.is_none());
    }

    #[test]
    fn config_round_trips() {
        let config = Config {
            metadata_path: Some(PathBuf::from("/path/to/fifa_ng_db-meta.xml")),
            use_embedded_profile: false,
            lua_script_dirs: Some("scripts;helpers".to_string()),
            always_on_top: true,
        };
        let text = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&text).unwrap();
        assert!(!parsed.use_embedded_profile);
        assert_eq!(parsed.lua_script_dirs.as_deref(), Some("scripts;helpers"));
    }

    #[test]
    fn config_path_uses_xdg_or_home() {
        // This only checks the shape of the computed path; it does not write.
        if let Some(path) = config_path() {
            assert!(path.ends_with("config.json"));
            assert!(path.to_string_lossy().contains("fc27le"));
        }
    }
}
