//! Minimal, bounds-checked PE32/PE32+ header parser.
//!
//! Only headers and the section table are parsed. Nothing is executed, written
//! or relocated. The parser is used to turn a memory range into a module
//! description and to bound pattern scans to real PE sections.

use fc27_core::{Error, Result};

use crate::backend::ProcessHandle;
use crate::types::{PeSectionSummary, PeSummary};

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

const MAX_HEADER_BYTES: usize = 1 << 20;
const MAX_SECTIONS: usize = 256;

#[derive(Debug, Clone)]
pub struct PeImage {
    pub summary: PeSummary,
    pub optional_magic: u16,
}

impl PeImage {
    pub fn is_64_bit(&self) -> bool {
        self.optional_magic == 0x20b
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let reader = Reader::new(bytes);

        if reader.slice(0, 2)? != b"MZ" {
            return Err(Error::parse("PE header", "missing DOS MZ signature"));
        }

        let e_lfanew = usize::try_from(reader.u32(0x3c)?)
            .map_err(|_| Error::parse("PE header", "e_lfanew does not fit in usize"))?;
        if !(0x40..=MAX_HEADER_BYTES).contains(&e_lfanew) {
            return Err(Error::parse(
                "PE header",
                format!("e_lfanew {e_lfanew:#x} is outside the accepted range"),
            ));
        }

        if reader.slice(e_lfanew, 4)? != b"PE\0\0" {
            return Err(Error::parse("PE header", "missing PE signature"));
        }

        let coff = e_lfanew + 4;
        let machine = reader.u16(coff)?;
        let number_of_sections = usize::from(reader.u16(coff + 2)?);
        let timestamp = reader.u32(coff + 4)?;
        let size_of_optional_header = usize::from(reader.u16(coff + 16)?);
        if number_of_sections > MAX_SECTIONS {
            return Err(Error::parse(
                "PE header",
                format!("implausible section count {number_of_sections}"),
            ));
        }

        let optional = coff + 20;
        let optional_magic = reader.u16(optional)?;
        if optional_magic != 0x20b && optional_magic != 0x10b {
            return Err(Error::parse(
                "PE optional header",
                format!("unsupported magic {optional_magic:#x}"),
            ));
        }

        let entry_point_rva = reader.u32(optional + 16)?;
        let image_base = if optional_magic == 0x20b {
            reader.u64(optional + 24)?
        } else {
            u64::from(reader.u32(optional + 28)?)
        };
        let size_of_image = reader.u32(optional + 56)?;
        let subsystem = reader.u16(optional + 68)?;

        let section_table = optional
            .checked_add(size_of_optional_header)
            .ok_or_else(|| Error::parse("PE header", "optional header size overflow"))?;

        let mut sections = Vec::with_capacity(number_of_sections);
        for index in 0..number_of_sections {
            let offset = section_table + index * 40;
            let raw_name = reader.slice(offset, 8)?;
            let name_end = raw_name.iter().position(|byte| *byte == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&raw_name[..name_end]).into_owned();
            let virtual_size = reader.u32(offset + 8)?;
            let virtual_address = reader.u32(offset + 12)?;
            let raw_size = reader.u32(offset + 16)?;
            let raw_offset = reader.u32(offset + 20)?;
            let characteristics = reader.u32(offset + 36)?;

            sections.push(PeSectionSummary {
                name,
                index,
                virtual_address,
                virtual_size,
                raw_offset,
                raw_size,
                readable: characteristics & IMAGE_SCN_MEM_READ != 0,
                writable: characteristics & IMAGE_SCN_MEM_WRITE != 0,
                executable: characteristics & IMAGE_SCN_MEM_EXECUTE != 0,
            });
        }

        Ok(Self {
            summary: PeSummary {
                machine,
                image_base,
                size_of_image,
                entry_point_rva,
                timestamp,
                subsystem,
                sections,
            },
            optional_magic,
        })
    }
}

/// Read and parse the PE headers of the module mapped at `base`.
///
/// The read is bounded to at most [`MAX_HEADER_BYTES`].
pub fn read_pe_image(handle: &dyn ProcessHandle, base: u64) -> Result<PeImage> {
    let initial = handle.read(base, 0x1000)?;
    if initial.len() < 0x40 || &initial[..2] != b"MZ" {
        return Err(Error::Unsupported(format!(
            "memory at {base:#x} does not start with a DOS MZ header"
        )));
    }

    let e_lfanew =
        u32::from_le_bytes([initial[0x3c], initial[0x3d], initial[0x3e], initial[0x3f]]) as usize;
    let coff = e_lfanew
        .checked_add(4)
        .ok_or_else(|| Error::parse("PE header", "e_lfanew overflow"))?;
    let needed = if coff + 20 <= initial.len() {
        let optional_size = u16::from_le_bytes([initial[coff + 16], initial[coff + 17]]) as usize;
        let section_count = u16::from_le_bytes([initial[coff + 2], initial[coff + 3]]) as usize;
        coff + 20 + optional_size + section_count.saturating_mul(40)
    } else {
        MAX_HEADER_BYTES
    };

    let header_len = needed.clamp(0x1000, MAX_HEADER_BYTES);
    let bytes = if header_len > initial.len() {
        handle.read(base, header_len)?
    } else {
        initial
    };

    PeImage::parse(&bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn slice(&self, offset: usize, length: usize) -> Result<&'a [u8]> {
        let end = offset
            .checked_add(length)
            .ok_or_else(|| Error::parse("PE header", "offset overflow"))?;
        self.bytes.get(offset..end).ok_or_else(|| {
            Error::parse(
                "PE header",
                format!(
                    "structure at {offset:#x} needs {length} bytes but the buffer holds {}",
                    self.bytes.len()
                ),
            )
        })
    }

    fn u16(&self, offset: usize) -> Result<u16> {
        let bytes = self.slice(offset, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&self, offset: usize) -> Result<u32> {
        let bytes = self.slice(offset, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&self, offset: usize) -> Result<u64> {
        let bytes = self.slice(offset, 8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pe() {
        let error = PeImage::parse(b"not a PE file at all, but long enough to read").unwrap_err();
        assert!(error.to_string().contains("MZ"));
    }

    #[test]
    fn rejects_truncated_pe() {
        let mut bytes = vec![0u8; 0x80];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
        let error = PeImage::parse(&bytes).unwrap_err();
        assert!(error.to_string().contains("magic"));
    }
}
