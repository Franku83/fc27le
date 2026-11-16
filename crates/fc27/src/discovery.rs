//! Scoring-based FC 27 process discovery.
//!
//! Discovery distinguishes the game process from Wine helpers and from the
//! `steam.exe` wrapper that appears in Wine command lines. A score is only a
//! hint; the CLI always reports the reasons so a human can audit the result.

use fc27_process::{Pid, ProcessInfo, ProcessRole};
use serde::Serialize;

use crate::profile::GAME_EXE;

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredProcess {
    pub pid: Pid,
    pub name: String,
    pub role: ProcessRole,
    pub score: i32,
    pub reasons: Vec<String>,
    pub exe_path: Option<String>,
    pub cwd: Option<String>,
    pub cmdline: Vec<String>,
    pub wine_prefix: Option<String>,
    pub proton_root: Option<String>,
}

impl DiscoveredProcess {
    fn from_info(info: &ProcessInfo, score: i32, reasons: Vec<String>) -> Self {
        Self {
            pid: info.pid,
            name: info.name.clone(),
            role: info.role,
            score,
            reasons,
            exe_path: info.exe_path.clone(),
            cwd: info.cwd.clone(),
            cmdline: info.cmdline.clone(),
            wine_prefix: info.wine.as_ref().and_then(|wine| wine.prefix.clone()),
            proton_root: info.wine.as_ref().and_then(|wine| wine.proton_root.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryReport {
    pub game: Option<DiscoveredProcess>,
    pub candidates: Vec<DiscoveredProcess>,
    pub wine_helpers: Vec<DiscoveredProcess>,
    pub notes: Vec<String>,
}

/// Score every process and return the best game candidate plus related Wine
/// helpers. A game candidate needs a score of at least 100.
pub fn analyze(processes: &[ProcessInfo]) -> DiscoveryReport {
    let mut candidates = Vec::new();

    for process in processes {
        let (score, reasons) = score(process);
        if score >= 50 {
            candidates.push(DiscoveredProcess::from_info(process, score, reasons));
        }
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.pid.cmp(&right.pid))
    });

    let game = candidates
        .iter()
        .find(|candidate| candidate.score >= 100)
        .cloned();

    let wine_helpers = processes
        .iter()
        .filter(|process| {
            matches!(
                process.role,
                ProcessRole::WineServer | ProcessRole::WineDevice
            ) || (process.role == ProcessRole::Steam)
        })
        .map(|process| {
            let (score, reasons) = score(process);
            DiscoveredProcess::from_info(process, score, reasons)
        })
        .collect();

    let mut notes = Vec::new();
    if game.is_none() {
        notes.push(
            "No process scored >= 100 as FC 27. Start the game, or pass --all to inspect every \
             process."
                .to_string(),
        );
    } else if candidates
        .iter()
        .filter(|candidate| candidate.score >= 100)
        .count()
        > 1
    {
        notes.push(
            "More than one process looks like FC 27. The highest score is reported as the game; \
             inspect candidates before reading memory."
                .to_string(),
        );
    }

    DiscoveryReport {
        game,
        candidates,
        wine_helpers,
        notes,
    }
}

fn score(process: &ProcessInfo) -> (i32, Vec<String>) {
    let mut score = 0;
    let mut reasons = Vec::new();

    if process.name.eq_ignore_ascii_case(GAME_EXE) {
        score += 100;
        reasons.push(format!("process name is {GAME_EXE}"));
    }

    if process
        .exe_path
        .as_deref()
        .and_then(|path| path.rsplit('/').next())
        .is_some_and(|name| name.eq_ignore_ascii_case(GAME_EXE))
    {
        score += 30;
        reasons.push("/proc/<pid>/exe resolves to FC27.exe".to_string());
    }

    let cmdline_mentions_game = process.cmdline.first().is_some_and(|arg| {
        arg.rsplit(['\\', '/'])
            .next()
            .is_some_and(|file| file.eq_ignore_ascii_case(GAME_EXE))
    });
    if cmdline_mentions_game {
        score += 20;
        reasons.push("command line references FC27.exe".to_string());
    }

    if process.role == ProcessRole::WineApplication {
        score += 20;
        reasons.push("loader path indicates Wine/Proton".to_string());
    }

    if process.wine.is_some() {
        score += 10;
        reasons.push("Wine environment is visible".to_string());
    }

    // The Wine `steam.exe` shim is launched with FC27.exe as an argument; it is
    // not the game and must not win.
    if process.name.eq_ignore_ascii_case("steam.exe") {
        score -= 150;
        reasons.push("Wine steam.exe shim (not the game)".to_string());
    }
    if let Some(first) = process.cmdline.first() {
        if first
            .to_ascii_lowercase()
            .contains("\\windows\\system32\\steam.exe")
        {
            score -= 50;
            reasons.push("command line starts the Wine steam shim".to_string());
        }
    }

    (score, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc27_process::WineContext;

    fn process(name: &str, cmdline: &[&str]) -> ProcessInfo {
        ProcessInfo {
            pid: Pid(42),
            ppid: None,
            name: name.to_string(),
            exe_path: None,
            cmdline: cmdline.iter().map(|value| value.to_string()).collect(),
            cwd: None,
            uid: Some(1000),
            state: Some("S".to_string()),
            threads: Some(1),
            rss_bytes: None,
            role: ProcessRole::WineApplication,
            wine: Some(WineContext::default()),
        }
    }

    #[test]
    fn game_process_wins_over_steam_shim() {
        let game = process("FC27.exe", &["Z:\\games\\FC27.exe"]);
        let shim = process(
            "steam.exe",
            &["c:\\windows\\system32\\steam.exe", "Z:\\games\\FC27.exe"],
        );
        let report = analyze(&[game, shim]);
        assert_eq!(report.game.expect("game").name, "FC27.exe");
    }

    #[test]
    fn no_game_when_only_shim_exists() {
        let shim = process(
            "steam.exe",
            &["c:\\windows\\system32\\steam.exe", "Z:\\games\\FC27.exe"],
        );
        let report = analyze(&[shim]);
        assert!(report.game.is_none());
    }
}
