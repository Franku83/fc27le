//! Controlled self-test.
//!
//! The parent spawns *itself* with the hidden `selftest-child` command. The
//! child allocates a buffer, writes known bytes and reports the address. The
//! parent then exercises the exact production reader against that address.
//!
//! This validates the reader without touching FC 27 and without requiring any
//! privilege: a child process is always inspectable by its parent.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use fc27_core::{Error, Result};
use fc27_platform_linux::{LinuxProcess, LinuxProvider};
use fc27_process::{MAX_SINGLE_READ, ModuleKind, Pid, ProcessHandle, ProcessProvider};
use serde::Serialize;

use crate::output::print_json;

/// Known bytes at the start of the child's buffer.
pub const SELFTEST_MAGIC: &[u8] = b"FC27LE-SELFTEST-MAGIC-0123456789";
/// Known byte pattern at offset 0x100 of the child's buffer.
pub const SELFTEST_PATTERN: [u8; 8] = [0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xf0, 0x0d];
pub const SELFTEST_PATTERN_OFFSET: u64 = 0x100;
pub const SELFTEST_BUFFER_LEN: usize = 64 * 1024;

#[derive(Debug, Serialize)]
struct Check {
    name: &'static str,
    passed: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct SelftestReport {
    passed: bool,
    child_pid: u32,
    target_address: u64,
    target_length: usize,
    checks: Vec<Check>,
    note: &'static str,
}

pub fn run_child() -> Result<u8> {
    let mut buffer = vec![0u8; SELFTEST_BUFFER_LEN];
    buffer[..SELFTEST_MAGIC.len()].copy_from_slice(SELFTEST_MAGIC);
    let pattern_start = SELFTEST_PATTERN_OFFSET as usize;
    buffer[pattern_start..pattern_start + SELFTEST_PATTERN.len()]
        .copy_from_slice(&SELFTEST_PATTERN);

    let address = std::hint::black_box(buffer.as_ptr()) as u64;
    println!(
        "READY {} {} {} {}",
        std::process::id(),
        address,
        buffer.len(),
        SELFTEST_MAGIC.len()
    );
    std::io::stdout().flush().map_err(Error::Io)?;

    // Block until the parent closes stdin or kills this process. The buffer
    // must remain alive for the whole lifetime of the child.
    let mut sink = String::new();
    let _ = std::io::stdin().read_to_string(&mut sink);
    std::hint::black_box(&buffer);
    Ok(0)
}

pub fn run(json: bool) -> Result<u8> {
    let executable = std::env::current_exe()
        .map_err(|error| Error::io_context("resolve current executable for selftest", error))?;

    let mut child = Command::new(&executable)
        .arg("selftest-child")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Error::io_context("spawn selftest child", error))?;

    let child_pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Unsupported("selftest child stdout was not captured".into()))?;

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader.read_line(&mut line).map(|_| line);
        let _ = sender.send(result);
    });

    let ready_line = match receiver.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(line)) => line,
        Ok(Err(error)) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::io_context("read selftest child handshake", error));
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Unsupported(
                "selftest child did not report READY within 10 seconds".to_string(),
            ));
        }
    };

    let parsed = parse_ready(&ready_line);
    let (reported_pid, target_address, target_length) = match parsed {
        Some(values) => values,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::parse(
                "selftest handshake",
                format!("unexpected line {ready_line:?}"),
            ));
        }
    };

    if reported_pid != child_pid {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::parse(
            "selftest handshake",
            format!("child reported pid {reported_pid}, expected {child_pid}"),
        ));
    }

    let checks = run_checks(child_pid, target_address, target_length);

    let _ = child.kill();
    let _ = child.wait();

    let passed = checks.iter().all(|check| check.passed);
    let report = SelftestReport {
        passed,
        child_pid,
        target_address,
        target_length,
        checks,
        note: "The controlled child is owned by this test. No game process was inspected.",
    };
    tracing::info!(child_pid, passed, "selftest complete");

    if json {
        print_json(&report)?;
    } else {
        println!("selftest child: pid {child_pid}");
        println!("target:         {target_address:#x} ({target_length} bytes)");
        println!();
        for check in &report.checks {
            println!(
                "  [{}] {:<28} {}",
                if check.passed { "ok" } else { "FAIL" },
                check.name,
                check.detail
            );
        }
        println!();
        println!(
            "selftest: {}",
            if report.passed { "PASSED" } else { "FAILED" }
        );
        println!("{}", report.note);
    }

    Ok(if passed { 0 } else { 1 })
}

fn parse_ready(line: &str) -> Option<(u32, u64, usize)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 || fields[0] != "READY" {
        return None;
    }
    Some((
        fields[1].parse().ok()?,
        fields[2].parse().ok()?,
        fields[3].parse().ok()?,
    ))
}

fn run_checks(child_pid: u32, target: u64, length: usize) -> Vec<Check> {
    let pid = Pid(child_pid);
    let provider = LinuxProvider::new();
    let mut checks = Vec::new();

    let process: LinuxProcess = match LinuxProcess::open(pid) {
        Ok(process) => process,
        Err(error) => {
            checks.push(Check {
                name: "open_child_process",
                passed: false,
                detail: error.to_string(),
            });
            return checks;
        }
    };

    checks.push(Check {
        name: "open_child_process",
        passed: true,
        detail: format!("opened /proc/{child_pid}/mem read-only"),
    });

    checks.push(Check {
        name: "process_visible",
        passed: process.info().name.contains("fc27le") || !process.info().name.is_empty(),
        detail: format!("comm = {}", process.info().name),
    });

    let mapping = process.mapping_containing(target);
    let mapping_ok = mapping
        .as_ref()
        .is_some_and(|mapping| mapping.permissions.read);
    checks.push(Check {
        name: "target_is_mapped",
        passed: mapping_ok,
        detail: mapping
            .as_ref()
            .map(|mapping| {
                format!(
                    "{:#x}-{:#x} {}",
                    mapping.start, mapping.end, mapping.permissions
                )
            })
            .unwrap_or_else(|| "no mapping contains the target".to_string()),
    });

    let magic = process.read(target, SELFTEST_MAGIC.len());
    checks.push(Check {
        name: "magic_bytes_match",
        passed: magic.as_deref().is_ok_and(|bytes| bytes == SELFTEST_MAGIC),
        detail: match &magic {
            Ok(bytes) => format!(
                "read {} bytes: {}",
                bytes.len(),
                fc27_core::hex::to_hex(bytes)
            ),
            Err(error) => error.to_string(),
        },
    });

    let pattern = process.read(target + SELFTEST_PATTERN_OFFSET, SELFTEST_PATTERN.len());
    checks.push(Check {
        name: "pattern_bytes_match",
        passed: pattern
            .as_deref()
            .is_ok_and(|bytes| bytes == SELFTEST_PATTERN.as_slice()),
        detail: match &pattern {
            Ok(bytes) => format!(
                "read {} bytes: {}",
                bytes.len(),
                fc27_core::hex::to_hex(bytes)
            ),
            Err(error) => error.to_string(),
        },
    });

    let too_large = process.read(target, MAX_SINGLE_READ + 1);
    checks.push(Check {
        name: "oversized_read_refused",
        passed: matches!(&too_large, Err(Error::ReadLimit { .. })),
        detail: match &too_large {
            Err(error) => error.to_string(),
            Ok(bytes) => format!("unexpectedly read {} bytes", bytes.len()),
        },
    });

    let unmapped = process.read(0, 16);
    checks.push(Check {
        name: "unmapped_read_refused",
        passed: matches!(&unmapped, Err(Error::UnmappedRange { .. })),
        detail: match &unmapped {
            Err(error) => error.to_string(),
            Ok(bytes) => format!("unexpectedly read {} bytes at address 0", bytes.len()),
        },
    });

    match process.modules() {
        Ok(modules) => {
            let elf_count = modules
                .iter()
                .filter(|module| module.kind == ModuleKind::Elf)
                .count();
            checks.push(Check {
                name: "module_enumeration",
                passed: elf_count > 0,
                detail: format!(
                    "{} module(s), {elf_count} ELF (runtime test binary)",
                    modules.len()
                ),
            });
        }
        Err(error) => checks.push(Check {
            name: "module_enumeration",
            passed: false,
            detail: error.to_string(),
        }),
    }

    match provider.list() {
        Ok(processes) => checks.push(Check {
            name: "process_discovery",
            passed: processes.iter().any(|info| info.pid == pid),
            detail: format!("{} visible process(es)", processes.len()),
        }),
        Err(error) => checks.push(Check {
            name: "process_discovery",
            passed: false,
            detail: error.to_string(),
        }),
    }

    let length_ok = length >= SELFTEST_BUFFER_LEN;
    checks.push(Check {
        name: "reported_buffer_length",
        passed: length_ok,
        detail: format!("child reported {length} bytes"),
    });

    checks
}
