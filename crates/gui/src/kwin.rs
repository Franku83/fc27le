//! Minimal KDE/KWin window activation helper.
//!
//! XWayland clients cannot reliably raise/focus themselves over a fullscreen
//! Wayland game. KWin scripting is the compositor-supported way to request
//! activation of a specific window. This is window management only; it does not
//! touch the game.
//!
//! Tested KWin 6.8.3 result: the helper can report FC27LE as the active window,
//! but an exclusive-fullscreen game may still keep the input/stacking. There is
//! no supported API here that guarantees a true overlay. Borderless/windowed
//! mode or a second monitor is required for reliable keyboard/mouse use.

use std::io::Write;
use std::process::Command;

pub fn focus_self(keep_above: bool) -> Result<(), String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop.to_ascii_uppercase().contains("KDE") {
        return Err("KWin helper is only used on KDE".to_string());
    }
    let path = std::env::temp_dir().join(format!("fc27le-focus-{}.js", std::process::id()));
    let script = format!(
        r#"const windows = workspace.windowList();
for (const w of windows) {{
    if (w.caption === "FC27LE" && w.resourceClass === "fc27le-gui") {{
        w.keepAbove = {keep_above};
        workspace.activeWindow = w;
        break;
    }}
}}
"#
    );
    let mut file = std::fs::File::create(&path)
        .map_err(|error| format!("create {}: {error}", path.display()))?;
    file.write_all(script.as_bytes())
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    drop(file);

    let unload = Command::new("qdbus6")
        .args([
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.unloadScript",
            "fc27le_focus",
        ])
        .output();
    let load = Command::new("qdbus6")
        .args([
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.loadScript",
            &path.to_string_lossy(),
            "fc27le_focus",
        ])
        .output()
        .map_err(|error| format!("qdbus6 loadScript: {error}"))?;
    let start = Command::new("qdbus6")
        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
        .output()
        .map_err(|error| format!("qdbus6 start: {error}"))?;
    let _ = std::fs::remove_file(&path);
    if !load.status.success() || !start.status.success() {
        let detail = String::from_utf8_lossy(&load.stderr);
        return Err(format!(
            "KWin focus script failed (load: {}, start: {}): {}",
            load.status,
            start.status,
            detail.trim()
        ));
    }
    let _ = unload;
    Ok(())
}
