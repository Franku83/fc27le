use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use fc27_core::{Error, Result};

/// Linux process identifier. Serialized as a plain number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pid(pub u32);

impl fmt::Display for Pid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<u32> for Pid {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl FromStr for Pid {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self> {
        input
            .trim()
            .parse::<u32>()
            .map(Self)
            .map_err(|error| Error::InvalidInput(format!("invalid PID {input:?}: {error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    WineServer,
    WineDevice,
    WineApplication,
    Steam,
    NativeApplication,
    KernelThread,
}

impl ProcessRole {
    pub fn is_wine(self) -> bool {
        matches!(
            self,
            Self::WineServer | Self::WineDevice | Self::WineApplication
        )
    }
}

/// Wine/Proton context reconstructed from `/proc/<pid>/environ` and the loader
/// path. Every field is optional because other users' processes may hide their
/// environment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WineContext {
    pub prefix: Option<String>,
    pub compat_data_path: Option<String>,
    pub proton_root: Option<String>,
    pub wine_loader: Option<String>,
    pub esync: Option<bool>,
    pub fsync: Option<bool>,
    pub ntsync: Option<bool>,
    pub windows_dll_overrides: Option<String>,
    pub environment_readable: bool,
    pub environment_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: Pid,
    pub ppid: Option<Pid>,
    pub name: String,
    pub exe_path: Option<String>,
    pub cmdline: Vec<String>,
    pub cwd: Option<String>,
    pub uid: Option<u32>,
    pub state: Option<String>,
    pub threads: Option<u32>,
    pub rss_bytes: Option<u64>,
    pub role: ProcessRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wine: Option<WineContext>,
}

impl ProcessInfo {
    /// Best-effort single-line command line, useful for human output.
    pub fn command_line(&self) -> String {
        if self.cmdline.is_empty() {
            self.name.clone()
        } else {
            self.cmdline.join(" ")
        }
    }

    /// True when this process is the FC 27 executable according to its
    /// comm/cmdline/exe path. This is intentionally conservative; the game
    /// scoring logic in `fc27-game` adds provenance and wine context.
    pub fn looks_like_fc27(&self) -> bool {
        if self.name.eq_ignore_ascii_case("FC27.exe") {
            return true;
        }
        if let Some(exe) = &self.exe_path {
            if exe.rsplit('/').next() == Some("FC27.exe") {
                return true;
            }
        }
        self.cmdline.iter().any(|arg| {
            arg.rsplit(['\\', '/'])
                .next()
                .is_some_and(|file| file.eq_ignore_ascii_case("FC27.exe"))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub shared: bool,
}

impl MemoryPermissions {
    pub fn is_executable(self) -> bool {
        self.execute
    }
}

impl fmt::Display for MemoryPermissions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = String::with_capacity(4);
        text.push(if self.read { 'r' } else { '-' });
        text.push(if self.write { 'w' } else { '-' });
        text.push(if self.execute { 'x' } else { '-' });
        text.push(if self.shared { 's' } else { 'p' });
        formatter.write_str(&text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingKind {
    /// File-backed mapping with a regular path.
    File,
    /// Anonymous memory (`/dev/zero`, empty path).
    Anonymous,
    /// A deleted memfd mapping (common in Wine's PE loader).
    Memfd,
    /// Device mapping (`/dev/dri/...`, `/dev/shm/...`).
    Device,
    /// Special kernel mapping (`[stack]`, `[vdso]`, `[vvar]`, ...).
    Special,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMapping {
    pub start: u64,
    pub end: u64,
    pub permissions: MemoryPermissions,
    pub offset: u64,
    pub inode: u64,
    pub path: Option<String>,
    pub kind: MappingKind,
}

impl MemoryMapping {
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn contains_range(&self, start: u64, length: usize) -> bool {
        let Some(end) = start.checked_add(length as u64) else {
            return false;
        };
        start >= self.start && end <= self.end
    }

    pub fn file_name(&self) -> Option<&str> {
        self.path.as_deref().and_then(|path| {
            path.rsplit(['/', '\\'])
                .next()
                .filter(|name| !name.is_empty())
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    Pe,
    Elf,
    Unknown,
}

impl ModuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pe => "pe",
            Self::Elf => "elf",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeSectionSummary {
    pub name: String,
    pub index: usize,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeSummary {
    pub machine: u16,
    pub image_base: u64,
    pub size_of_image: u32,
    pub entry_point_rva: u32,
    pub timestamp: u32,
    pub subsystem: u16,
    pub sections: Vec<PeSectionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub path: String,
    pub base: u64,
    pub size: u64,
    pub kind: ModuleKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<PeSummary>,
}

impl ModuleInfo {
    pub fn end(&self) -> u64 {
        self.base.saturating_add(self.size)
    }

    pub fn contains(&self, address: u64) -> bool {
        address >= self.base && address < self.end()
    }

    pub fn rva_of(&self, address: u64) -> Option<u64> {
        self.contains(address)
            .then_some(address.saturating_sub(self.base))
    }

    pub fn address_of_rva(&self, rva: u64) -> Option<u64> {
        (rva < self.size).then_some(self.base.saturating_add(rva))
    }

    pub fn pe_sections(&self) -> &[PeSectionSummary] {
        self.pe
            .as_ref()
            .map(|pe| pe.sections.as_slice())
            .unwrap_or(&[])
    }

    pub fn section_containing_rva(&self, rva: u64) -> Option<&PeSectionSummary> {
        self.pe_sections().iter().find(|section| {
            let start = u64::from(section.virtual_address);
            let end = start + u64::from(section.virtual_size.max(1));
            rva >= start && rva < end
        })
    }
}
