use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use fc27_core::{Error, Result};
use fc27_process::modules::enumerate_modules;
use fc27_process::{
    MAX_SINGLE_READ, MemoryMapping, ModuleInfo, Pid, ProcessHandle, ProcessInfo, ProcessProvider,
    WineContext, classify::classify_role,
};

use crate::procfs::{parse_cmdline, parse_environ, parse_maps_line, parse_status};
use crate::wine;

/// Linux `/proc` provider.
#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxProvider;

impl LinuxProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ProcessProvider for LinuxProvider {
    fn list(&self) -> Result<Vec<ProcessInfo>> {
        let mut processes = Vec::new();
        let entries =
            fs::read_dir("/proc").map_err(|error| Error::io_context("read /proc", error))?;

        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|text| text.parse::<u32>().ok()) else {
                continue;
            };
            if let Ok(info) = read_process_info(Pid(pid)) {
                processes.push(info);
            }
        }

        processes.sort_by_key(|process| process.pid);
        Ok(processes)
    }

    fn info(&self, pid: Pid) -> Result<ProcessInfo> {
        read_process_info(pid)
    }

    fn open(&self, pid: Pid) -> Result<Box<dyn ProcessHandle>> {
        Ok(Box::new(LinuxProcess::open(pid)?))
    }
}

/// Read-only handle backed by `/proc/<pid>/mem`.
///
/// Mappings are snapshotted when the handle is opened. Reads validate the whole
/// requested range against that snapshot before issuing `pread`, so a read can
/// never silently zero-fill an unmapped hole.
pub struct LinuxProcess {
    info: ProcessInfo,
    mem: File,
    mappings: RwLock<Vec<MemoryMapping>>,
    maps_path: PathBuf,
}

impl LinuxProcess {
    pub fn open(pid: Pid) -> Result<Self> {
        let info = read_process_info(pid)?;
        let mem_path = format!("/proc/{pid}/mem");
        let mem = File::open(&mem_path).map_err(|error| map_proc_error(pid, "open mem", error))?;
        let maps_path = PathBuf::from(format!("/proc/{pid}/maps"));
        let mappings = read_mappings(pid, &maps_path)?;

        Ok(Self {
            info,
            mem,
            mappings: RwLock::new(mappings),
            maps_path,
        })
    }

    pub fn refresh_mappings(&self) -> Result<()> {
        let fresh = read_mappings(self.info.pid, &self.maps_path)?;
        *self
            .mappings
            .write()
            .map_err(|_| Error::InvalidInput("mapping lock poisoned".to_string()))? = fresh;
        Ok(())
    }

    pub fn mapping_containing(&self, address: u64) -> Option<MemoryMapping> {
        self.mappings
            .read()
            .ok()?
            .iter()
            .find(|mapping| mapping.start <= address && address < mapping.end)
            .cloned()
    }

    fn plan_segments(&self, address: u64, length: usize) -> Result<Vec<(u64, usize)>> {
        let mappings = self
            .mappings
            .read()
            .map_err(|_| Error::InvalidInput("mapping lock poisoned".to_string()))?;

        let end = address
            .checked_add(length as u64)
            .ok_or_else(|| Error::InvalidAddress {
                input: format!("{address:#x}+{length}"),
                reason: "address range overflows u64".to_string(),
            })?;

        let mut segments = Vec::new();
        let mut cursor = address;
        while cursor < end {
            let Some(mapping) = mappings
                .iter()
                .find(|mapping| mapping.start <= cursor && cursor < mapping.end)
            else {
                return Err(Error::UnmappedRange {
                    pid: self.info.pid.0,
                    start: address,
                    end,
                    len: length,
                });
            };
            if !mapping.permissions.read {
                return Err(Error::UnmappedRange {
                    pid: self.info.pid.0,
                    start: address,
                    end,
                    len: length,
                });
            }
            let segment_end = mapping.end.min(end);
            let segment_len = usize::try_from(segment_end - cursor)
                .map_err(|_| Error::InvalidInput("segment length overflow".to_string()))?;
            segments.push((cursor, segment_len));
            cursor = segment_end;
        }

        Ok(segments)
    }
}

impl ProcessHandle for LinuxProcess {
    fn pid(&self) -> Pid {
        self.info.pid
    }

    fn info(&self) -> &ProcessInfo {
        &self.info
    }

    fn mappings(&self) -> Result<Vec<MemoryMapping>> {
        self.mappings
            .read()
            .map(|mappings| mappings.clone())
            .map_err(|_| Error::InvalidInput("mapping lock poisoned".to_string()))
    }

    fn modules(&self) -> Result<Vec<ModuleInfo>> {
        enumerate_modules(self)
    }

    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
        if length == 0 {
            return Ok(Vec::new());
        }
        if length > MAX_SINGLE_READ {
            return Err(Error::ReadLimit {
                address,
                len: length,
                limit: MAX_SINGLE_READ,
            });
        }

        let segments = self.plan_segments(address, length)?;
        let mut output = Vec::with_capacity(length);

        for (segment_address, segment_length) in segments {
            let mut buffer = vec![0u8; segment_length];
            let mut filled = 0;
            while filled < segment_length {
                let offset = segment_address.checked_add(filled as u64).ok_or_else(|| {
                    Error::InvalidAddress {
                        input: format!("{segment_address:#x}+{filled}"),
                        reason: "address overflow".to_string(),
                    }
                })?;
                match self.mem.read_at(&mut buffer[filled..], offset) {
                    Ok(0) => {
                        return Err(Error::ShortRead {
                            address: offset,
                            requested: segment_length - filled,
                            received: filled,
                        });
                    }
                    Ok(read) => filled += read,
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(error) => {
                        return Err(map_proc_error(self.info.pid, "read mem", error));
                    }
                }
            }
            output.extend_from_slice(&buffer);
        }

        Ok(output)
    }
}

fn read_mappings(pid: Pid, maps_path: &Path) -> Result<Vec<MemoryMapping>> {
    let text = fs::read_to_string(maps_path)
        .map_err(|error| map_proc_error(pid, &format!("read {}", maps_path.display()), error))?;
    let mut mappings = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        mappings.push(parse_maps_line(line)?);
    }
    mappings.sort_by_key(|mapping| mapping.start);
    Ok(mappings)
}

fn read_process_info(pid: Pid) -> Result<ProcessInfo> {
    let base = format!("/proc/{}", pid.0);
    let status_text = fs::read_to_string(format!("{base}/status"))
        .map_err(|error| map_proc_error(pid, "read status", error))?;
    let status = parse_status(&status_text);
    if status.name.is_empty() {
        return Err(Error::ProcessUnavailable {
            pid: pid.0,
            reason: "process disappeared while reading /proc/<pid>/status".to_string(),
        });
    }

    let cmdline = fs::read(format!("{base}/cmdline"))
        .map(|bytes| parse_cmdline(&bytes))
        .unwrap_or_default();
    let exe_path = fs::read_link(format!("{base}/exe"))
        .ok()
        .map(|path| path.to_string_lossy().into_owned());
    let cwd = fs::read_link(format!("{base}/cwd"))
        .ok()
        .map(|path| path.to_string_lossy().into_owned());

    let (environment, environment_error): (BTreeMap<String, String>, Option<String>) =
        match fs::read(format!("{base}/environ")) {
            Ok(bytes) => (parse_environ(&bytes), None),
            Err(error) => (
                BTreeMap::new(),
                Some(format!("{}: {}", error.kind(), error)),
            ),
        };

    let role = classify_role(&status.name, exe_path.as_deref(), &cmdline);
    let wine: Option<WineContext> =
        wine::build_context(exe_path.as_deref(), &environment, environment_error);

    Ok(ProcessInfo {
        pid,
        ppid: status.ppid.map(Pid),
        name: status.name,
        exe_path,
        cmdline,
        cwd,
        uid: status.uid,
        state: status.state,
        threads: status.threads,
        rss_bytes: status.rss_bytes,
        role,
        wine,
    })
}

fn map_proc_error(pid: Pid, context: &str, error: std::io::Error) -> Error {
    match error.kind() {
        ErrorKind::NotFound => Error::ProcessNotFound { pid: pid.0 },
        ErrorKind::PermissionDenied => Error::PermissionDenied {
            pid: pid.0,
            context: context.to_string(),
            source: error,
        },
        _ => Error::IoContext {
            context: format!("{context} for process {pid}"),
            source: error,
        },
    }
}

/// `/proc/<pid>/environ` is a NUL-separated block; expose the raw bytes for
/// callers that want to parse it themselves.
pub fn read_environ_bytes(pid: Pid) -> Result<Vec<u8>> {
    fs::read(format!("/proc/{}/environ", pid.0))
        .map_err(|error| map_proc_error(pid, "read environ", error))
}
