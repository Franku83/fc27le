//! Parsers for the textual files under `/proc/<pid>`.

use std::collections::BTreeMap;

use fc27_core::{Error, Result};
use fc27_process::{MappingKind, MemoryMapping, MemoryPermissions};

#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    pub name: String,
    pub state: Option<String>,
    pub ppid: Option<u32>,
    pub uid: Option<u32>,
    pub threads: Option<u32>,
    pub rss_bytes: Option<u64>,
    pub capabilities_effective: Option<u64>,
    pub dumpable: Option<String>,
}

pub fn parse_status(text: &str) -> StatusInfo {
    let mut info = StatusInfo::default();

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "Name" => info.name = value.to_string(),
            "State" => {
                info.state = Some(value.split_whitespace().next().unwrap_or(value).to_string())
            }
            "PPid" => info.ppid = value.parse().ok(),
            "Uid" => info.uid = value.split_whitespace().next().and_then(|v| v.parse().ok()),
            "Threads" => info.threads = value.parse().ok(),
            "VmRSS" => {
                info.rss_bytes = value
                    .split_whitespace()
                    .next()
                    .and_then(|kib| kib.parse::<u64>().ok())
                    .map(|kib| kib.saturating_mul(1024))
            }
            "CapEff" => {
                info.capabilities_effective =
                    u64::from_str_radix(value.trim_start_matches("0x"), 16).ok()
            }
            "Dumpable" => info.dumpable = Some(value.to_string()),
            _ => {}
        }
    }

    info
}

/// Parse one line of `/proc/<pid>/maps`.
///
/// `start-end perms offset dev inode pathname`
///
/// The pathname is the remainder of the line and may contain spaces.
pub fn parse_maps_line(line: &str) -> Result<MemoryMapping> {
    let mut cursor = 0;
    let mut fields = [""; 5];
    for field in &mut fields {
        *field = next_field(line, &mut cursor).ok_or_else(|| {
            Error::parse(
                "/proc/<pid>/maps line",
                format!("not enough fields: {line:?}"),
            )
        })?;
    }
    let path = line[cursor..].trim();

    let (start_text, end_text) = fields[0].split_once('-').ok_or_else(|| {
        Error::parse(
            "/proc/<pid>/maps address range",
            format!("missing '-': {:?}", fields[0]),
        )
    })?;
    let start = u64::from_str_radix(start_text, 16)
        .map_err(|error| Error::parse("mapping start", error.to_string()))?;
    let end = u64::from_str_radix(end_text, 16)
        .map_err(|error| Error::parse("mapping end", error.to_string()))?;
    if end < start {
        return Err(Error::parse(
            "mapping range",
            format!("end {end:#x} precedes start {start:#x}"),
        ));
    }

    let permissions = parse_permissions(fields[1])?;
    let offset = u64::from_str_radix(fields[2], 16)
        .map_err(|error| Error::parse("mapping offset", error.to_string()))?;
    let inode = fields[4]
        .parse::<u64>()
        .map_err(|error| Error::parse("mapping inode", error.to_string()))?;

    let path_option = if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    };
    let kind = classify_mapping(path_option.as_deref());

    Ok(MemoryMapping {
        start,
        end,
        permissions,
        offset,
        inode,
        path: path_option,
        kind,
    })
}

fn next_field<'a>(line: &'a str, cursor: &mut usize) -> Option<&'a str> {
    let bytes = line.as_bytes();
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
    if *cursor >= bytes.len() {
        return None;
    }
    let start = *cursor;
    while *cursor < bytes.len() && !bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
    Some(&line[start..*cursor])
}

fn parse_permissions(text: &str) -> Result<MemoryPermissions> {
    let bytes = text.as_bytes();
    if bytes.len() != 4 {
        return Err(Error::parse(
            "mapping permissions",
            format!("expected four characters, got {text:?}"),
        ));
    }
    Ok(MemoryPermissions {
        read: bytes[0] == b'r',
        write: bytes[1] == b'w',
        execute: bytes[2] == b'x',
        shared: bytes[3] == b's',
    })
}

fn classify_mapping(path: Option<&str>) -> MappingKind {
    match path {
        None => MappingKind::Anonymous,
        Some(path) if path.starts_with("[") => MappingKind::Special,
        Some(path) if path.starts_with("/memfd:") => MappingKind::Memfd,
        Some(path) if path.starts_with("/dev/") => MappingKind::Device,
        Some(_) => MappingKind::File,
    }
}

/// Parse `KEY=VALUE` entries from `/proc/<pid>/environ`. Entries without `=`
/// are ignored; duplicate keys keep the last value.
pub fn parse_environ(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for entry in bytes.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(entry);
        if let Some((key, value)) = text.split_once('=') {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

/// Parse `/proc/<pid>/cmdline`.
pub fn parse_cmdline(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| String::from_utf8_lossy(entry).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_fields() {
        let status = "\
Name:\tFC27.exe
State:\tS (sleeping)
PPid:\t19188
Uid:\t1000\t1000\t1000\t1000
Threads:\t89
VmRSS:\t 2444856 kB
CapEff:\t0000000000000000
";
        let parsed = parse_status(status);
        assert_eq!(parsed.name, "FC27.exe");
        assert_eq!(parsed.state.as_deref(), Some("S"));
        assert_eq!(parsed.ppid, Some(19188));
        assert_eq!(parsed.uid, Some(1000));
        assert_eq!(parsed.threads, Some(89));
        assert_eq!(parsed.rss_bytes, Some(2_444_856 * 1024));
        assert_eq!(parsed.capabilities_effective, Some(0));
    }

    #[test]
    fn parses_maps_line_with_spaces_in_path() {
        let mapping = parse_maps_line(
            "140000000-140001000 rwxp 00000000 00:38 1293799                    \
             /mnt/New Volume/FC27.exe",
        )
        .unwrap();
        assert_eq!(mapping.start, 0x140000000);
        assert_eq!(mapping.end, 0x140001000);
        assert!(mapping.permissions.read);
        assert!(mapping.permissions.write);
        assert!(mapping.permissions.execute);
        assert_eq!(mapping.path.as_deref(), Some("/mnt/New Volume/FC27.exe"));
        assert_eq!(mapping.kind, MappingKind::File);
    }

    #[test]
    fn parses_anonymous_and_memfd_mappings() {
        let anonymous = parse_maps_line("00010000-00012000 ---p 00000000 00:00 0 ").unwrap();
        assert_eq!(anonymous.kind, MappingKind::Anonymous);
        assert!(anonymous.path.is_none());

        let memfd = parse_maps_line(
            "00810000-00954000 r--p 00000000 00:01 25981                      \
             /memfd:wine-mapping (deleted)",
        )
        .unwrap();
        assert_eq!(memfd.kind, MappingKind::Memfd);
    }

    #[test]
    fn rejects_malformed_maps_line() {
        assert!(parse_maps_line("garbage").is_err());
        assert!(parse_maps_line("1000-2000 rw 0 0 0").is_err());
        assert!(parse_maps_line("zzzz-2000 rwxp 0 0 0").is_err());
    }

    #[test]
    fn parses_environ_and_cmdline() {
        let env = parse_environ(b"WINEPREFIX=/tmp/pfx\0WINEESYNC=1\0EMPTY\0");
        assert_eq!(env.get("WINEPREFIX").map(String::as_str), Some("/tmp/pfx"));
        assert_eq!(env.get("WINEESYNC").map(String::as_str), Some("1"));

        let args = parse_cmdline(b"Z:\\games\\FC27.exe\0-arg\0");
        assert_eq!(args, vec!["Z:\\games\\FC27.exe", "-arg"]);
    }
}
