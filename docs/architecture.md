# Architecture

## 1. Goals and non-goals

**Goal:** a small, auditable, Linux-first Rust workspace that can safely answer
questions about a running FC 27 process: what it is, which modules it loaded,
where they are mapped, what bytes are at a verified address, and whether a
research anchor still matches this build.

**Non-goals (enforced by absence of code):**

* memory writes, value editing, teleporting, stat changes;
* DLL injection, `ptrace`, `process_vm_writev`, `mprotect`, remote threads;
* detours, hooks, pattern patching, anti-cheat or DRM changes;
* overlay/ImGui UI;
* save-file modification;
* Windows support claims without a Windows test.

## 2. Workspace layout

```text
crates/
  core/              fc27-core
  process/           fc27-process
  platform-linux/    fc27-platform-linux
  platform-windows/  fc27-platform-windows   (documented stub)
  fc27/              fc27-game
  cli/               fc27-cli -> binary `fc27le`
```

Dependency direction:

```text
                 +----------------+
                 |   fc27-core    |  errors, hex, pattern, units
                 +--------+-------+
                          ^
                 +--------+-------+
                 |  fc27-process  |  traits + platform-neutral types + PE parser
                 +---+--------+---+
                     ^        ^
       +-------------+        +----------------+
       |                                       |
+------+-------------+              +----------+-----------+
| fc27-platform-linux|              | fc27-platform-windows|
| /proc backend      |              | stub only            |
+------+-------------+              +----------+-----------+
       ^                                       ^
       |             +------------------+      |
       +-------------+    fc27-game     +------+  (only process/core in practice)
                     | profile, discovery,
                     | anchors, build id,
                     | capability gates
                     +--------+---------+
                              ^
                     +--------+---------+
                     |     fc27-cli     |
                     +------------------+
```

`fc27-cli` selects the Linux backend explicitly. The Windows crate is a
workspace member so its API cannot rot, but it is not linked into the Linux
binary yet. A future `cfg(target_os)` selection can choose the provider without
changing command code.

## 3. Core types and data flow

```text
LinuxProvider::list()
  -> /proc/<pid>/{status,cmdline,exe,cwd,environ}
  -> Vec<ProcessInfo>  (role + optional WineContext)
  -> fc27_game::discovery::analyze()
  -> DiscoveryReport { game, candidates, wine_helpers, notes }
```

```text
LinuxProcess::open(pid)
  -> ProcessInfo
  -> File::open("/proc/<pid>/mem", O_RDONLY)
  -> snapshot of /proc/<pid>/maps (RwLock<Vec<MemoryMapping>>)
  -> read(addr, len)
       -> reject len > 1 MiB
       -> plan segments across contiguous readable mappings
       -> pread loop (FileExt::read_at)
       -> Vec<u8> or a typed error
```

```text
LinuxProcess::modules()
  -> group file-backed mappings by path
  -> read 4 bytes at each candidate base
  -> MZ  -> parse PE header -> PeSummary (machine, image base, SizeOfImage, sections)
  -> ELF -> ModuleInfo { kind: Elf }
  -> other data files are ignored
  -> SizeOfImage absorbs Wine's anonymous section mappings
```

```text
probe(pid)
  -> process metadata + Wine context + policy
  -> modules of interest (FC27.exe, proprietary editor DLL, activation module, ...)
  -> build identity (size, optional SHA-256, PE metadata)
  -> anchor verification (bounded scan of executable PE sections)
  -> observations with confidence + explicit limitations
```

```text
players(pid)
  -> resolve module
  -> fc27_game::players::resolve()
  -> Err(InsufficientEvidence)  // by design until evidence exists
  -> capability report, exit code 3
```

## 4. Linux memory-reader invariants

1. **Read-only.** The only memory handle is a `std::fs::File` opened with
   `File::open` (`O_RDONLY`). No write method exists on `LinuxProcess`.
2. **Bounded.** `MAX_SINGLE_READ = 1 MiB`. Larger requests fail with
   `Error::ReadLimit` before a syscall.
3. **Mapping-validated.** The entire requested range must be covered by a
   contiguous sequence of readable mappings in the snapshot. A hole fails with
   `Error::UnmappedRange`; `/proc/pid/mem` is never allowed to return silent
   zero-fill.
4. **Short reads are errors.** `pread` returning `0` yields `Error::ShortRead`.
5. **Snapshot semantics.** Mappings are captured at open time. If the process
   unmaps a region afterwards, the `pread` fails with an I/O error rather than
   reading another region. `refresh_mappings()` exists for long-lived handles.
6. **No privilege escalation.** The backend never invokes `ptrace`, never
   changes Yama, and never requests root.

Range validation is implemented with `plan_segments`, which walks the sorted
mapping snapshot from the start address. This also allows a bounded read to cross
adjacent readable mappings (for example a PE header page followed by the first
section page) without weakening the hole check.

## 5. PE parser

`fc27-process::pe` parses only headers and the section table:

* DOS `MZ`, `e_lfanew`, `PE\0\0`, PE32/PE32+ optional header;
* machine, image base, `SizeOfImage`, entry point, timestamp, subsystem;
* section name, RVA, virtual size, raw offset/size, R/W/X characteristics;
* hard limits: 1 MiB of headers, 256 sections, bounds-checked reads.

The parser is independent of the OS and is covered by negative tests for
non-PE and truncated input. It never follows a pointer, never reads section
contents and never modifies state.

## 6. Module grouping and Wine quirks

Wine does not mirror Windows section protections in `/proc/<pid>/maps`. For the
observed `FC27.exe`:

* the module is one `rwxp` virtual range `0x140000000..0x14fa79000`;
* only three sub-ranges are file-backed (header, `.pdata`, `fieldinf`);
* the remainder is anonymous memory.

Therefore `enumerate_modules`:

* considers only file-backed mappings with a regular path;
* classifies by magic (`MZ`, `\x7fELF`) and discards data files (NLS tables,
  fonts, shader caches);
* groups mappings that are within 1 MiB of each other;
* for a PE image, additionally absorbs every mapping inside `SizeOfImage`;
* reports `size = max(SizeOfImage, grouped_end - base)`.

The result is a stable module base and a size that covers all sections, even
though `/proc/<pid>/maps` does not show one clean image mapping. Native ELF
libraries are reported too, because Wine's loader, winserver and graphics stack
are visible in the same process tree.

## 7. Discovery scoring

`fc27_game::discovery` is deliberately transparent. Every candidate carries a
score and a list of reasons.

| Signal | Score |
| --- | ---: |
| `/proc/<pid>/comm` is `FC27.exe` | +100 |
| `/proc/<pid>/exe` resolves to `FC27.exe` | +30 |
| first command-line argument is `FC27.exe` | +20 |
| loader path indicates Wine/Proton | +20 |
| Wine environment is readable | +10 |
| process is the Wine `steam.exe` shim | -150 |
| first argument is `...\windows\system32\steam.exe` | -50 |

A game is reported only at **score >= 100**. Candidates are listed at
**score >= 50**. This keeps Wine services (`services.exe`, `explorer.exe`, ...)
out of the candidate list while still reporting them as helpers where relevant.
Only the *first* command-line argument is used for the filename bonus; otherwise
every wrapper process that mentions `FC27.exe` in its arguments would be
misclassified.

## 8. Evidence anchors

`fc27_game::anchors` is a registry, not a scanner heuristic. An anchor requires:

* a stable id and description;
* a module name;
* a byte pattern with `??` wildcards;
* an expected RVA;
* a written source;
* a confidence label.

Verification scans **only PE sections** (executable sections by default) with a
1 MiB chunk and a configurable total cap. It reports `Verified`, `Mismatch`,
`PatternMissing`, `ModuleMissing`, `ScanTruncated` or `Error`, plus every match
VA/RVA and the number of bytes scanned. A match is only "verified" when it is at
the expected RVA; finding the pattern elsewhere is a mismatch, not a success.

The current registry contains one anchor: the game-event dispatcher from
`reports/press_any_key_analysis.md` §2.4. It was independently re-verified
against the on-disk executable (unique hit at file offset `0x1CD315C`) and
against live memory (VA `0x141CD3B5C`).

## 9. Error model and exit codes

All fallible APIs return `fc27_core::Result<T>` with a typed `Error`. The CLI
never panics for an expected failure.

| Variant | Meaning | Exit |
| --- | --- | ---: |
| `ProcessNotFound` | `/proc/<pid>` disappeared | 6 |
| `PermissionDenied` | DAC/Yama denied inspection | 5 |
| `ProcessUnavailable` | process exists but metadata is unusable | 1 |
| `UnmappedRange` | range not fully mapped/readable | 1 |
| `ReadLimit` | request exceeds 1 MiB | 2 |
| `ShortRead` | kernel returned fewer bytes | 1 |
| `InvalidAddress` / `InvalidPattern` / `InvalidInput` | usage error | 2 |
| `Unsupported` | operation not available | 4 |
| `InsufficientEvidence` | capability gated by missing research | 3 |
| `IoContext` / `Io` | OS failure with context | 1 |

`--json` keeps stdout machine-readable: JSON payloads go to stdout, logs and
errors go to stderr. Error objects include a stable `kind`.

## 10. Logging

`tracing` + `tracing-subscriber` provides structured logs on stderr. The CLI
emits one `starting`, one `finished`/`failed` event per command, plus command
specific fields (process count, game pid, anchor count). `--log-json` switches to
JSON lines; `--log-level` accepts standard `EnvFilter` directives.

Logs never include process memory contents. Hexdumps are explicit command output,
not logs.

## 11. Testing

| Layer | Tests |
| --- | --- |
| `fc27-core` | pattern parsing/matching, hexdump, size formatting |
| `fc27-process` | PE negative cases, module grouping/filtering, role classification |
| `fc27-platform-linux` | maps/status/environ parsing, Wine context inference |
| `fc27-game` | discovery scoring, anchor registry sanity, capability gates |
| `fc27-cli` | address parsing, JSON contract, exit codes |
| controlled selftest | parent spawns `fc27le selftest-child`; reads a known buffer, checks magic/pattern, refuses 1 MiB+1 and address 0, enumerates modules, confirms discovery |

The selftest is the important one: it exercises the **production** reader against
a real `/proc/<pid>/mem` of a process owned by the test. It requires no game and
no privilege, so it is reproducible in CI (Linux only).

Run everything:

```bash
cargo test --workspace
./target/release/fc27le selftest
```

## 12. Windows backend plan

The trait boundary is already platform-neutral:

```rust
pub trait ProcessProvider {
    fn list(&self) -> Result<Vec<ProcessInfo>>;
    fn info(&self, pid: Pid) -> Result<ProcessInfo>;
    fn open(&self, pid: Pid) -> Result<Box<dyn ProcessHandle>>;
}

pub trait ProcessHandle {
    fn pid(&self) -> Pid;
    fn info(&self) -> &ProcessInfo;
    fn mappings(&self) -> Result<Vec<MemoryMapping>>;
    fn modules(&self) -> Result<Vec<ModuleInfo>>;
    fn read(&self, address: u64, length: usize) -> Result<Vec<u8>>;
}
```

A Windows implementation should use:

* `CreateToolhelp32Snapshot` or `EnumProcesses` for discovery,
* `OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION)`,
* `QueryFullProcessImageNameW`, `GetProcessTimes`, `GetProcessMemoryInfo`,
* `VirtualQueryEx` for mappings,
* `EnumProcessModulesEx` + `GetModuleInformation` for modules,
* `ReadProcessMemory` with the same 1 MiB cap and full-range validation.

It must preserve the same invariants: no writes, no `WriteProcessMemory`, no
`VirtualProtectEx`, no remote threads, no injection. Until that code is compiled
and tested on Windows, `fc27-platform-windows` returns `Error::Unsupported` and
the project does not claim Windows support.

## 13. Adding the player resolver (only with evidence)

A future resolver must be a separate module with an explicit evidence record for
each step. Minimum acceptance criteria:

1. a game-side DB manager locator with a reproducible anchor and an expected
   RVA that verifies live;
2. a table descriptor with count, stride, validity rules and a bounded iterator;
3. a record layout with field offsets, widths and signedness;
4. a string model where every pointer is validated as mapped/readable before
   dereference;
5. a career-mode/state check so menu-mode or stale data is not reported;
6. cross-checks: record count plausibility, id monotonicity, overall/potential
   ranges, team-id referential integrity;
7. a CLI path that degrades to exit code 3 when any check fails.

If any of these is missing, the resolver must return `InsufficientEvidence`
instead of guessing. That is the current state.

## 14. Trust model

Process memory and module contents are **untrusted data**. Strings, "SQL",
names and symbol-like text read from the game are never interpreted as
instructions, paths to execute, or configuration. The tool does not load code
from the target and does not follow pointers for player data until a validated
layout exists. A malicious or corrupted target can still return arbitrary bytes;
outputs are for human analysis, not for automated trust decisions.
