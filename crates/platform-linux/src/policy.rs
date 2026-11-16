//! Read-only inspection-policy discovery.
//!
//! These values explain whether a same-user process can be inspected without
//! root. They are reported, never changed.

use std::fs;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InspectionPolicy {
    /// `/proc/sys/kernel/yama/ptrace_scope` when Yama is enabled.
    pub yama_ptrace_scope: Option<u8>,
    /// `/proc/sys/kernel/perf_event_paranoid`.
    pub perf_event_paranoid: Option<u8>,
    /// True when `/proc` is mounted with `hidepid=1` or stricter.
    pub hidepid: bool,
    /// Running user ID.
    pub uid: u32,
    /// Effective capabilities of the inspecting process.
    pub capabilities_effective: Option<u64>,
    pub notes: Vec<String>,
}

pub fn inspect_policy() -> InspectionPolicy {
    let yama_ptrace_scope = fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
        .ok()
        .and_then(|text| text.trim().parse().ok());
    let perf_event_paranoid = fs::read_to_string("/proc/sys/kernel/perf_event_paranoid")
        .ok()
        .and_then(|text| text.trim().parse().ok());
    let hidepid = fs::read_to_string("/proc/mounts")
        .map(|mounts| mounts.lines().any(|line| line.contains("hidepid=")))
        .unwrap_or(false);

    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let capabilities_effective = status.lines().find_map(|line| {
        line.strip_prefix("CapEff:")
            .and_then(|value| u64::from_str_radix(value.trim(), 16).ok())
    });
    let uid = status
        .lines()
        .find_map(|line| {
            line.strip_prefix("Uid:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0);

    let mut notes = Vec::new();
    match yama_ptrace_scope {
        Some(0) => notes.push(
            "Yama scope 0: any same-user process may be attached (subject to normal DAC checks)."
                .to_string(),
        ),
        Some(1) => notes.push(
            "Yama scope 1: only descendants may be attached, but this observation was still \
             permitted for the current same-user game process. Access depends on the target's \
             dumpability and the actual ptrace check performed by the kernel for /proc/pid/mem."
                .to_string(),
        ),
        Some(2) => notes.push(
            "Yama scope 2: attachment requires CAP_SYS_PTRACE; same-user inspection may fail."
                .to_string(),
        ),
        Some(3) => notes.push(
            "Yama scope 3: no attachment is permitted; same-user inspection will fail.".to_string(),
        ),
        Some(other) => notes.push(format!("Unknown Yama ptrace scope {other}.")),
        None => notes.push(
            "Yama is not enabled or not readable; standard same-user ptrace checks apply."
                .to_string(),
        ),
    }
    if hidepid {
        notes
            .push("/proc is mounted with hidepid; other users' processes may be invisible.".into());
    }
    notes.push(
        "No root, ptrace attach, process modification or memory write is used by this tool."
            .to_string(),
    );

    InspectionPolicy {
        yama_ptrace_scope,
        perf_event_paranoid,
        hidepid,
        uid,
        capabilities_effective,
        notes,
    }
}
