//! Module enumeration from file-backed memory mappings.
//!
//! Wine maps PE images section by section and may leave anonymous guard pages
//! between them. Grouping therefore has to tolerate small gaps and, for a PE
//! image, absorb every mapping that falls inside `SizeOfImage`.

use std::collections::BTreeMap;

use fc27_core::Result;

use crate::backend::ProcessHandle;
use crate::pe::read_pe_image;
use crate::types::{MappingKind, MemoryMapping, ModuleInfo, ModuleKind};

/// Maximum gap between two file-backed mappings that still belongs to one
/// module. PE images may have guard pages; 1 MiB is deliberately generous.
const MAX_INTERNAL_GAP: u64 = 1 << 20;

pub fn enumerate_modules(handle: &dyn ProcessHandle) -> Result<Vec<ModuleInfo>> {
    let mappings = handle.mappings()?;

    let mut by_path: BTreeMap<&str, Vec<&MemoryMapping>> = BTreeMap::new();
    for mapping in &mappings {
        if !is_module_backing(mapping) {
            continue;
        }
        let Some(path) = mapping.path.as_deref() else {
            continue;
        };
        by_path.entry(path).or_default().push(mapping);
    }

    let mut modules = Vec::new();
    for (path, mut path_mappings) in by_path {
        path_mappings.sort_by_key(|mapping| mapping.start);

        let mut cursor = 0;
        while cursor < path_mappings.len() {
            let base = path_mappings[cursor].start;
            let mut end = path_mappings[cursor].end;
            cursor += 1;

            let magic = handle.read(base, 4).unwrap_or_default();
            let kind = match magic.get(..2) {
                Some(b"MZ") => ModuleKind::Pe,
                Some([0x7f, b'E']) if magic.get(2) == Some(&b'L') => ModuleKind::Elf,
                _ => {
                    // Data files (locale tables, fonts, shader caches) are not modules.
                    continue;
                }
            };

            let pe = if kind == ModuleKind::Pe {
                read_pe_image(handle, base).ok().map(|image| image.summary)
            } else {
                None
            };
            let image_size = pe
                .as_ref()
                .map(|image| u64::from(image.size_of_image))
                .filter(|size| *size > 0);
            let image_end = image_size.map(|size| base.saturating_add(size));

            while cursor < path_mappings.len() {
                let next = path_mappings[cursor];
                let inside_image = image_end.is_some_and(|image_end| next.start < image_end);
                let small_gap = next.start <= end.saturating_add(MAX_INTERNAL_GAP);
                if inside_image || small_gap {
                    end = end.max(next.end);
                    cursor += 1;
                } else {
                    break;
                }
            }

            let size = match image_size {
                Some(size) => size.max(end.saturating_sub(base)),
                None => end.saturating_sub(base),
            };

            modules.push(ModuleInfo {
                name: file_name(path),
                path: path.to_string(),
                base,
                size,
                kind,
                pe,
            });
        }
    }

    modules.sort_by_key(|module| module.base);
    Ok(modules)
}

fn is_module_backing(mapping: &MemoryMapping) -> bool {
    if mapping.kind != MappingKind::File {
        return false;
    }
    let Some(path) = mapping.path.as_deref() else {
        return false;
    };
    path.starts_with('/') && !path.starts_with("/dev/") && !path.starts_with("/memfd:")
}

fn file_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Case-insensitive lookup by exact file name, path suffix, or path substring.
pub fn find_module<'a>(modules: &'a [ModuleInfo], needle: &str) -> Option<&'a ModuleInfo> {
    let needle_lower = needle.to_ascii_lowercase();
    modules
        .iter()
        .find(|module| module.name.eq_ignore_ascii_case(needle))
        .or_else(|| {
            modules
                .iter()
                .find(|module| module.path.to_ascii_lowercase().ends_with(&needle_lower))
        })
        .or_else(|| {
            modules
                .iter()
                .find(|module| module.path.to_ascii_lowercase().contains(&needle_lower))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MemoryPermissions;

    fn mapping(start: u64, end: u64, path: &str) -> MemoryMapping {
        MemoryMapping {
            start,
            end,
            permissions: MemoryPermissions {
                read: true,
                write: false,
                execute: true,
                shared: false,
            },
            offset: 0,
            inode: 1,
            path: Some(path.to_string()),
            kind: MappingKind::File,
        }
    }

    #[test]
    fn data_files_are_not_module_backings() {
        assert!(is_module_backing(&mapping(0, 0x1000, "/tmp/FC27.exe")));
        assert!(!is_module_backing(&mapping(
            0,
            0x1000,
            "/dev/dri/renderD128"
        )));
        assert!(!is_module_backing(&mapping(
            0,
            0x1000,
            "/memfd:wine-mapping"
        )));
    }

    #[test]
    fn finds_modules_by_name_and_path() {
        let modules = vec![
            ModuleInfo {
                name: "kernel32.dll".into(),
                path: "/prefix/drive_c/windows/system32/kernel32.dll".into(),
                base: 0x1000,
                size: 0x1000,
                kind: ModuleKind::Pe,
                pe: None,
            },
            ModuleInfo {
                name: "FC27.exe".into(),
                path: "/games/FC27.exe".into(),
                base: 0x2000,
                size: 0x1000,
                kind: ModuleKind::Pe,
                pe: None,
            },
        ];
        assert_eq!(find_module(&modules, "fc27.exe").unwrap().base, 0x2000);
        assert_eq!(find_module(&modules, "kernel32.dll").unwrap().base, 0x1000);
        assert!(find_module(&modules, "absent.dll").is_none());
    }
}
