use crate::types::ProcessRole;

/// Classify a process from its comm name, loader path and command line.
///
/// The classification is descriptive only. It does not imply that a process is
/// safe to inspect or that it belongs to a particular game installation.
pub fn classify_role(name: &str, exe_path: Option<&str>, cmdline: &[String]) -> ProcessRole {
    let lower_name = name.to_ascii_lowercase();

    if lower_name == "wineserver" || lower_name.starts_with("wineserver") {
        return ProcessRole::WineServer;
    }
    if lower_name == "winedevice.exe" {
        return ProcessRole::WineDevice;
    }
    if lower_name == "steam.exe" || lower_name == "steamwebhelper.exe" {
        return ProcessRole::Steam;
    }
    if lower_name == "kthreadd" || lower_name.starts_with("kworker") {
        return ProcessRole::KernelThread;
    }

    let loader = exe_path.unwrap_or_default().to_ascii_lowercase();
    let wine_loader = loader.contains("/wine/")
        || loader.contains("wine64-preloader")
        || loader.contains("wine-preloader")
        || loader.contains("/proton")
        || cmdline
            .first()
            .is_some_and(|arg| arg.contains("\\windows\\system32\\"));
    if wine_loader {
        return ProcessRole::WineApplication;
    }

    ProcessRole::NativeApplication
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_wine_helpers() {
        assert_eq!(
            classify_role("wineserver", None, &[]),
            ProcessRole::WineServer
        );
        assert_eq!(
            classify_role("winedevice.exe", None, &[]),
            ProcessRole::WineDevice
        );
        assert_eq!(
            classify_role(
                "FC27.exe",
                Some(
                    "/home/user/protons/GE-Proton11-12-x86_64/files/lib/wine/i386-unix/wine64-preloader"
                ),
                &[]
            ),
            ProcessRole::WineApplication
        );
    }

    #[test]
    fn classifies_native() {
        assert_eq!(
            classify_role("bash", Some("/usr/bin/bash"), &[]),
            ProcessRole::NativeApplication
        );
    }
}
