# Architecture

## 1. Goals and non-goals

**Goal:** a small, auditable, Linux-first Rust workspace that can safely inspect
a running FC 27 process, browse its real player database, and execute a
read-only subset of the Live Editor Lua API using the same verified data layer.

**Non-goals (enforced by absence of code):**

* memory writes, value editing, teleporting, stat changes;
* DLL injection, `ptrace`, `process_vm_writev`, `mprotect`, remote threads;
* detours, hooks, pattern patching, game-function calls, anti-cheat or DRM
  changes;
* a true in-game overlay implemented by hooking/capturing the game; the shipped
  UI is an external native window with documented compositor limits;
* save-file modification;
* Windows support claims without a Windows test.

## 2. Workspace layout

```text
crates/
  core/              fc27-core
  process/           fc27-process
  platform-linux/    fc27-platform-linux
  platform-windows/  fc27-platform-windows   (documented stub)
  fc27/              fc27-game (profile, discovery, anchors, build identity,
                     db_service, db_metadata, player_store, name_pool,
                     table_access, capability/evidence gates)
  lua-compat/        fc27-lua-compat (Lua 5.4 API v2 subset, worker, matrix)
  gui/               fc27-gui -> binary `fc27le-gui` (eframe/egui, session loader,
                     socket toggle, optional KWin focus helper)
  cli/               fc27-cli -> binary `fc27le`
```

Dependency direction:

```text
                 +----------------+
                 |   fc27-core    |  errors, hex, pattern, units
                 +--------+-------+
                          ^
                 +--------+-------+
                 |  fc27-process  |  traits + types + PE parser
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
       +-------------+    fc27-game     +------+  (process/core only in practice)
                     | profile, discovery,
                     | anchors, build id, db,
                     | players, names, table_access
                     +----+---------+---+
                          ^         ^
              +-----------+         +-------------+
              |                                   |
     +--------+---------+              +----------+---------+
     |  fc27-lua-compat |              |      fc27-gui      |
     | Lua 5.4 subset   |<-------------| browser + console  |
     +------------------+              +----------+---------+
                                                  |
                                         +--------+--------+
                                         |    fc27-cli     |
                                         +-----------------+
```

`fc27-cli`, `fc27-gui` and `fc27-lua-compat` all use the verified
`fc27-game` storage model. The Lua runtime does not read game memory through a
second implementation, and the GUI does not execute Lua on its render thread.

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
  -> module of interest (FC27.exe)
  -> build identity (size, optional SHA-256, PE metadata)
  -> anchor verification (bounded scan of executable PE sections)
  -> observations with confidence + explicit limitations
```

```text
db(pid)
  -> anchor pattern scan (FC27.exe executable sections, 160 MiB cap)
  -> resolve LEA/MOV RIP targets in the matched game function
  -> read global object -> vtable -> thunk -> service registry
  -> key 0x0AE932D0 -> table-manager interface -> manager root
  -> traverse 2 database nodes and 273 table descriptors
  -> validate every pointer through the production bounded reader
  -> optional --metadata FILE:
       parse database/table/field names + 4CC codes
       validate all table/field mappings against live descriptors
       resolve the focus table (default players = CZUM)
  -> checks + table summary (no game function is called)
```

```text
players(pid, --metadata FILE)
  -> resolve module + verify the DB service path
  -> parse metadata, validate all table/field mappings
  -> resolve players = CZUM (152 field widths, 156-byte rows)
  -> read +0x78 capacity, +0x7C slot count, +0x7E invalid count
  -> read the verified slot range twice; require identical snapshots
  -> skip slots whose last byte has bit 7 set
  -> decode playerid/overallrating/potential/height/weight (LSB-first + bias)
  -> cross-check invalid rows and the playerid-index count
  -> load playernames (Huffman), dcplayernames (IDs 46000+) and
     editedplayernames overrides
  -> resolve each record's firstnameid/lastnameid/commonnameid
  -> checks + numeric records + names; missing names stay null
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
7. **No `unsafe`.** The workspace contains zero `unsafe` blocks. `/proc` is
   accessed through `std::fs` and `FileExt::read_at`.

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

The current registry contains two anchors:

* the game-event dispatcher from `reports/press_any_key_analysis.md` §2.4,
  independently re-verified against the on-disk executable (unique hit at file
  offset `0x1CD315C`) and live memory (VA `0x141CD3B5C`);
* the database service-object initializer, unique at file offset `0x724235E`,
  RVA `0x7242D5E`, verified live at VA `0x147242D5E`.

## 9. Database service path (WP1)

`fc27_game::db_service` verifies the game-side database path without calling a
game function:

```text
pattern (RVA 0x7242D5E)
  -> global object (LEA/MOV targets)
  -> vtable +0x30 thunk
  -> service object -> service vtable +0x40 method
  -> registry key 0x0AE932D0
  -> table-manager interface (value - 8)
  -> 2 database nodes -> 273 table descriptors
```

The traversal is bounded (`MAX_DATABASES`, `MAX_TABLES`) and every pointer read
goes through the production `ProcessHandle::read`, so an unmapped or
read-protected pointer fails closed. The command emits twelve semantic checks,
including the expected RVA, LEA/MOV agreement, module containment of the
vtable/thunk/method, the registry key, the two expected database names and a
non-empty table list.

The table-manager layer is not a player resolver: it exposes 4CC table
shortnames and a record layout header (`+0x44` record size in bytes, `+0x48`
`record_size*8-1` bit mask, `+0x4c` flags, `+0x82` field count, `+0x84`
16-byte field entries). The full-name -> 4CC mapping comes from a caller-supplied metadata XML;
no verified name map is bundled with the game process.

`fc27_game::db_metadata` parses a caller-supplied descriptor and validates it
against the live manager. On the tested build, 268/268 metadata tables matched
and **`players` resolved to `CZUM`** (152 fields, 156-byte records, 1244 field
bits + 4 padding bits, non-zero data pointer).

`fc27_game::player_store` then uses the descriptor's own storage counters:
`+0x78` capacity, `+0x7C` slot count, `+0x7E` invalid-marker count, and the
high bit of each record's last byte as the invalid predicate. On the tested
build the `CZUM` capture was 22,098 slots / 27,000 capacity / 284 invalid /
21,814 valid, with the playerid-index count also 21,814. The reader requires two
byte-identical snapshots and emits the verified numeric fields plus the three
name IDs. The metadata XML is never bundled.

`fc27_game::name_pool` resolves those name IDs from the live database:
`playernames` (`<table-4cc>`) is a 12-byte row with a u32 pool offset; the pool at
descriptor `+0x38` starts with a 138-node breadth-first Huffman tree, followed
by length-prefixed MSB-first bitstreams (UTF-8 output). IDs 46000+ use
`dcplayernames` inline strings, and `editedplayernames` overrides names by
player ID. The full live pass resolved 21,814/21,814 players with zero errors.
Missing names stay null with an explicit error; no name is fabricated.

See `docs/research/player_storage.md` and `docs/research/name_storage.md`.

### 9.1 Generic table snapshots

`fc27_game::table_access::TableSnapshot` generalizes the verified player storage
model to any metadata table that matches a live descriptor. It snapshots
`slot_count * record_size` bytes once, bounds the read at 64 MiB, validates every
field against the record size, maps metadata field names to live 4CC entries,
and decodes integer (with bias/range check), float32, inline string and
compressed-string fields. Compressed strings raise an error rather than guessing;
callers use the name pool where appropriate.

The snapshot is immutable and exposes `is_valid` (high bit of the last record
byte) plus `read_field`. Both the GUI loader and the Lua runtime use this type;
there is no parallel Lua database reader.

### 9.2 GUI session, Lua worker and record tokens

The GUI spawns a session thread that performs discovery, database anchor
verification, metadata validation, player enumeration, name resolution and build
hashing. Only the resulting `Session` is sent back to the render thread, so the
UI never blocks on `/proc` reads. Refresh/Reconnect create a new session; errors
remain visible in the top panel.

Lua runs on its own worker thread. The worker builds a Lua state over a
`TableProvider` backed by `TableSnapshot`, then executes scripts. The GUI polls
`LuaEvent` values (session ready, logs, message boxes, finished/error) and never
calls Lua on the render thread. A debug hook enforces a wall-clock deadline and
a cancellation flag; `mlua::Lua::set_memory_limit` enforces the memory cap.

Script-visible record handles are `i64` tokens encoding
`(generation & 0x7FFF, table_id, slot + 1)`. `0` terminates iteration; stale
tokens are rejected after `LE.db:Refresh/Load/Reset`. Raw addresses are never
exposed. Table and field userdata carry only snapshot metadata, and every setter
returns an explicit compatibility error.

### 9.3 Window and F9 behavior

The GUI is a normal external window (`eframe`/`egui`, X11/Wayland). F9 hides it
when focused and sends `ViewportCommand::Visible(false)`; a Unix-socket command
restores it and asks KWin to activate the window through `qdbus6` scripting.
The socket is per-user under `XDG_RUNTIME_DIR` (mode 0600).

The X11 always-on-top state is available but defaults off. In the tested KWin
6.8.3/Wayland + XWayland session it made the window click-through: the
compositor kept the hint while mouse input stayed with the game. KWin activation
can also report FC27LE as active while an exclusive-fullscreen game still owns
input/stacking, so the helper is best-effort window management, not an overlay
guarantee. This limitation is documented rather than worked around with hooks,
input capture or game changes. Borderless/windowed mode or a second display is
the reliable interactive setup.

## 10. Error model and exit codes

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

## 11. Logging

`tracing` + `tracing-subscriber` provides structured logs on stderr. The CLI
emits one `starting`, one `finished`/`failed` event per command, plus command
specific fields (process count, game pid, anchor count). `--log-json` switches to
JSON lines; `--log-level` accepts standard `EnvFilter` directives.

Logs never include process memory contents. Hexdumps are explicit command output,
not logs.

## 12. Testing

| Layer | Tests |
| --- | --- |
| `fc27-core` | pattern parsing/matching, hexdump, size formatting |
| `fc27-process` | PE negative cases, module grouping/filtering, role classification |
| `fc27-platform-linux` | maps/status/environ parsing, Wine context inference |
| `fc27-game` | discovery scoring, anchor registry sanity, capability gates, metadata parser, table snapshot decoding, player/name storage |
| `fc27-lua-compat` | 11 integration tests over synthetic snapshots: Lua basics, table/field lookup, valid iteration, invalid table/field errors, missing modules, write/event/MEMORY/HTTP/REQUEST rejection, logger/print/MessageBox, timeout, cancellation and memory limits |
| `fc27-gui` | filter/sort unit tests; live session, search, detail, Lua run and F9/toggle checks against FC 27 (manual evidence) |
| `fc27-cli` | address parsing, JSON contract, exit codes |
| controlled selftest | parent spawns `fc27le selftest-child`; reads a known buffer, checks magic/pattern, refuses 1 MiB+1 and address 0, enumerates modules, confirms discovery |

The Lua tests use synthetic `TableSnapshot` fixtures so they do not need the
game. Live read-only verification is separate: the GUI scanned 21,814 players,
logged values through `LOGGER`, displayed a `MessageBox` result, and cancelled
an infinite loop with the Stop button while the window stayed responsive.

Run everything:

```bash
cargo test --workspace
./target/release/fc27le selftest
cargo run -p fc27-lua-compat --example matrix > docs/lua_compatibility.md
```

## 13. Windows backend plan

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

The crate currently carries `#![forbid(unsafe_code)]`. A real Windows backend
will need a small, reviewed `unsafe` boundary around `windows-sys`; that attribute
must be relaxed deliberately (with a documented audit) rather than removed
silently.

## 14. Adding the player resolver (only with evidence)

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

## 15. Trust model

Process memory and module contents are **untrusted data**. Strings, "SQL",
names and symbol-like text read from the game are never interpreted as
instructions, paths to execute, or configuration. The tool does not load code
from the target and does not follow pointers for player data until a validated
layout exists. A malicious or corrupted target can still return arbitrary bytes;
outputs are for human analysis, not for automated trust decisions.

Lua scripts are equally untrusted. `mlua` with vendored Lua 5.4 provides the
interpreter, but Lua is not a security sandbox. Standard file I/O remains
available for script compatibility; `os.execute`, `io.popen` and
`package.loadlib` are removed, scripts run under time/memory limits, and the
database bridge is immutable. The Lua API never exposes raw process memory,
network access, game-function calls or write primitives.
