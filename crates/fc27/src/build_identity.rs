//! File-based build identity: size, SHA-256 and PE header metadata.
//!
//! Hashing is opt-in because `FC27.exe` is roughly 235 MiB. The hash identifies
//! the file; it never authorizes a memory layout by itself.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use fc27_core::{Error, Result};
use fc27_process::pe::PeImage;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::profile::{KnownBuild, identify_build};

#[derive(Debug, Clone, Serialize)]
pub struct PeIdentity {
    pub machine: u16,
    pub image_base: u64,
    pub size_of_image: u32,
    pub entry_point_rva: u32,
    pub timestamp: u32,
    pub subsystem: u16,
    pub section_count: usize,
    pub section_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildIdentity {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub known_build: Option<KnownBuild>,
    pub pe: Option<PeIdentity>,
    pub notes: Vec<String>,
}

pub fn identify_file(path: &Path, compute_sha256: bool) -> Result<BuildIdentity> {
    if !path.exists() {
        return Err(Error::InvalidInput(format!(
            "game executable does not exist: {}",
            path.display()
        )));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| Error::io_context(format!("stat {}", path.display()), error))?;
    if !metadata.is_file() {
        return Err(Error::InvalidInput(format!(
            "game executable path is not a regular file: {}",
            path.display()
        )));
    }

    let sha256 = if compute_sha256 {
        Some(sha256_file(path)?)
    } else {
        None
    };

    let pe = match read_pe_identity(path) {
        Ok(pe) => Some(pe),
        Err(error) => {
            return Ok(BuildIdentity {
                path: path.to_string_lossy().into_owned(),
                size_bytes: metadata.len(),
                known_build: sha256.as_deref().and_then(identify_build).cloned(),
                sha256,
                pe: None,
                notes: vec![format!("PE header could not be parsed: {error}")],
            });
        }
    };

    let mut notes = Vec::new();
    if let Some(build) = sha256.as_deref().and_then(identify_build) {
        notes.push(format!(
            "SHA-256 matches known build '{}' ({}).",
            build.label, build.source
        ));
    } else if sha256.is_some() {
        notes.push(
            "SHA-256 does not match any known build. Record it before drawing conclusions."
                .to_string(),
        );
    }
    if let Some(pe) = &pe {
        if pe.image_base != 0x1_4000_0000 {
            notes.push(format!(
                "Preferred image base is {:#x}, not the 0x140000000 observed in the research.",
                pe.image_base
            ));
        }
    }

    let known_build = sha256.as_deref().and_then(identify_build).cloned();

    Ok(BuildIdentity {
        path: path.to_string_lossy().into_owned(),
        size_bytes: metadata.len(),
        sha256,
        known_build,
        pe,
        notes,
    })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .map_err(|error| Error::io_context(format!("open {}", path.display()), error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| Error::io_context(format!("read {}", path.display()), error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn read_pe_identity(path: &Path) -> Result<PeIdentity> {
    let mut file = File::open(path)
        .map_err(|error| Error::io_context(format!("open {}", path.display()), error))?;
    let mut header = vec![0u8; 1 << 20];
    let read = file
        .read(&mut header)
        .map_err(|error| Error::io_context(format!("read {}", path.display()), error))?;
    header.truncate(read);

    let image = PeImage::parse(&header)?;
    let summary = image.summary;
    Ok(PeIdentity {
        machine: summary.machine,
        image_base: summary.image_base,
        size_of_image: summary.size_of_image,
        entry_point_rva: summary.entry_point_rva,
        timestamp: summary.timestamp,
        subsystem: summary.subsystem,
        section_count: summary.sections.len(),
        section_names: summary
            .sections
            .into_iter()
            .map(|section| section.name)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_file() {
        let error = identify_file(Path::new("/definitely/not/here/FC27.exe"), false).unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn identifies_own_test_binary_as_elf_without_pe() {
        let executable = std::env::current_exe().unwrap();
        let identity = identify_file(&executable, false).unwrap();
        assert!(identity.size_bytes > 0);
        assert!(identity.sha256.is_none());
        assert!(identity.pe.is_none());
    }
}
