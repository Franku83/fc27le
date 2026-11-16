use fc27_core::Result;

use crate::types::{MemoryMapping, ModuleInfo, Pid, ProcessInfo};

/// Read-only handle to one process.
///
/// Implementations must never expose writes. `read` must return an error rather
/// than zero-filling when the requested range is not mapped or cannot be read.
pub trait ProcessHandle {
    fn pid(&self) -> Pid;

    fn info(&self) -> &ProcessInfo;

    fn mappings(&self) -> Result<Vec<MemoryMapping>>;

    fn modules(&self) -> Result<Vec<ModuleInfo>>;

    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>>;

    fn read_u8(&self, address: u64) -> Result<u8> {
        Ok(self.read(address, 1)?[0])
    }

    fn read_u16(&self, address: u64) -> Result<u16> {
        let bytes = self.read(address, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&self, address: u64) -> Result<u32> {
        let bytes = self.read(address, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&self, address: u64) -> Result<u64> {
        let bytes = self.read(address, 8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_cstring(&self, address: u64, max_length: usize) -> Result<String> {
        let bytes = self.read(address, max_length)?;
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
}

/// Discover and open processes. Implemented by each platform backend.
pub trait ProcessProvider {
    fn list(&self) -> Result<Vec<ProcessInfo>>;

    fn info(&self, pid: Pid) -> Result<ProcessInfo>;

    fn open(&self, pid: Pid) -> Result<Box<dyn ProcessHandle>>;
}
