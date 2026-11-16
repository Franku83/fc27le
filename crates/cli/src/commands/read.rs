use std::fs;

use fc27_core::{Error, Result, hex};
use fc27_process::{MAX_SINGLE_READ, MemoryMapping, Pid, ProcessHandle};
use serde::Serialize;

use crate::cli::ReadArgs;
use crate::commands::{open_process, require_module};
use crate::output::{parse_u64, print_json};

#[derive(Debug, Serialize)]
struct MappingRef {
    start: u64,
    end: u64,
    permissions: String,
    path: Option<String>,
}

impl From<&MemoryMapping> for MappingRef {
    fn from(mapping: &MemoryMapping) -> Self {
        Self {
            start: mapping.start,
            end: mapping.end,
            permissions: mapping.permissions.to_string(),
            path: mapping.path.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ReadOutput {
    pid: Pid,
    address: u64,
    requested_length: usize,
    returned_length: usize,
    mapping: Option<MappingRef>,
    bytes_hex: String,
    ascii: String,
    read_only: bool,
    output_file: Option<String>,
}

pub fn run(args: &ReadArgs, json: bool) -> Result<u8> {
    if args.length > MAX_SINGLE_READ {
        return Err(Error::ReadLimit {
            address: 0,
            len: args.length,
            limit: MAX_SINGLE_READ,
        });
    }

    let process = open_process(args.pid)?;

    let address = match (&args.address, &args.module, &args.offset) {
        (Some(address), None, None) => parse_u64(address)?,
        (None, Some(module), Some(offset)) => {
            let modules = process.modules()?;
            let module = require_module(&modules, module)?;
            let offset = parse_u64(offset)?;
            if offset >= module.size {
                return Err(Error::InvalidInput(format!(
                    "offset {offset:#x} is outside module {} (size {:#x})",
                    module.name, module.size
                )));
            }
            module.base + offset
        }
        _ => {
            return Err(Error::InvalidInput(
                "specify either --address ADDRESS or --module NAME --offset OFFSET".to_string(),
            ));
        }
    };

    let mapping = process.mapping_containing(address);
    let bytes = process.read(address, args.length)?;

    if let Some(path) = &args.output {
        fs::write(path, &bytes)
            .map_err(|error| Error::io_context(format!("write dump to {path}"), error))?;
    }

    let output = ReadOutput {
        pid: args.pid,
        address,
        requested_length: args.length,
        returned_length: bytes.len(),
        mapping: mapping.as_ref().map(MappingRef::from),
        bytes_hex: hex::to_hex(&bytes),
        ascii: hex::ascii_preview(&bytes),
        read_only: true,
        output_file: args.output.clone(),
    };

    if json {
        print_json(&output)?;
        return Ok(0);
    }

    println!("pid:        {}", output.pid);
    println!("address:    {:#x}", output.address);
    println!(
        "length:     {} requested, {} returned",
        output.requested_length, output.returned_length
    );
    if let Some(mapping) = &output.mapping {
        println!(
            "mapping:    {:#x}-{:#x} {} {}",
            mapping.start,
            mapping.end,
            mapping.permissions,
            mapping.path.as_deref().unwrap_or("[anonymous]")
        );
    }
    println!("mode:       read-only");
    if let Some(path) = &output.output_file {
        println!("raw bytes:  written to {path}");
    }
    println!();
    println!("{}", hex::hexdump(address, &bytes));

    Ok(0)
}
