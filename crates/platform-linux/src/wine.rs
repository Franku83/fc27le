//! Wine/Proton context reconstruction from a process environment.

use std::collections::BTreeMap;

use fc27_process::WineContext;

pub fn build_context(
    loader: Option<&str>,
    environment: &BTreeMap<String, String>,
    environment_error: Option<String>,
) -> Option<WineContext> {
    let loader_is_wine = loader.is_some_and(|path| {
        let lower = path.to_ascii_lowercase();
        lower.contains("/wine/")
            || lower.contains("wine64-preloader")
            || lower.contains("wine-preloader")
    });
    let has_wine_environment = environment.contains_key("WINEPREFIX")
        || environment.contains_key("STEAM_COMPAT_DATA_PATH")
        || environment.contains_key("WINEDLLOVERRIDES");

    if !loader_is_wine && !has_wine_environment {
        return None;
    }

    let prefix = environment.get("WINEPREFIX").cloned().or_else(|| {
        environment
            .get("STEAM_COMPAT_DATA_PATH")
            .map(|path| format!("{}/pfx/", path.trim_end_matches('/')))
    });

    Some(WineContext {
        prefix,
        compat_data_path: environment.get("STEAM_COMPAT_DATA_PATH").cloned(),
        proton_root: infer_proton_root(loader, environment),
        wine_loader: loader
            .filter(|path| {
                let lower = path.to_ascii_lowercase();
                lower.contains("wine")
            })
            .map(str::to_string),
        esync: environment.get("WINEESYNC").map(|value| value == "1"),
        fsync: environment.get("WINEFSYNC").map(|value| value == "1"),
        ntsync: environment.get("WINENTSYNC").map(|value| value == "1"),
        windows_dll_overrides: environment.get("WINEDLLOVERRIDES").cloned(),
        environment_readable: environment_error.is_none(),
        environment_error,
    })
}

/// Infer the Proton/GE-Proton root directory from the loader path or
/// `LD_LIBRARY_PATH`. Returns `None` for plain Wine installations.
fn infer_proton_root(
    loader: Option<&str>,
    environment: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(loader) = loader {
        if let Some(index) = loader.find("/files/") {
            return Some(loader[..index].to_string());
        }
    }

    let ld_library_path = environment.get("LD_LIBRARY_PATH")?;
    for entry in ld_library_path.split(':') {
        if let Some(index) = entry.find("/files/lib/") {
            return Some(entry[..index].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_proton_root_and_prefix() {
        let mut environment = BTreeMap::new();
        environment.insert(
            "WINEPREFIX".to_string(),
            "/home/user/.local/share/prefix/pfx/".to_string(),
        );
        environment.insert("WINEESYNC".to_string(), "1".to_string());
        environment.insert("WINEFSYNC".to_string(), "0".to_string());

        let context = build_context(
            Some(
                "/home/user/protons/GE-Proton11-12-x86_64/files/lib/wine/i386-unix/wine64-preloader",
            ),
            &environment,
            None,
        )
        .expect("wine context");

        assert_eq!(
            context.proton_root.as_deref(),
            Some("/home/user/protons/GE-Proton11-12-x86_64")
        );
        assert_eq!(context.esync, Some(true));
        assert_eq!(context.fsync, Some(false));
        assert!(context.environment_readable);
    }

    #[test]
    fn no_context_for_native_process() {
        assert!(build_context(Some("/usr/bin/bash"), &BTreeMap::new(), None).is_none());
    }
}
