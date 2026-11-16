# FC 27 Open Career Editor

Read-only, Linux-first Rust tooling for inspecting **EA SPORTS FC 27** while it runs
under Wine/Proton. The project independently reimplements the safe, useful first
layer of FC Live Editor functionality: process discovery, module/section
enumeration, bounded memory reads, build identification and verified pattern
anchors.

**It does not write memory, inject DLLs, install hooks, patch binaries, touch
anti-cheat/DRM, or modify game files, prefixes or saves.**

## Status

| Layer | State |
| --- | --- |
| Rust workspace (6 crates) | compiles on stable Rust |
| Linux `/proc` process backend | implemented and exercised against a live FC 27 process |
| Controlled memory-reader selftest | passes (spawns its own child process) |
| Module/PE/section enumeration | implemented |
| Build identity (size, SHA-256, PE metadata) | implemented |
| Verified pattern anchors | implemented (event dispatcher verified live) |
| Player listing | **blocked on evidence** (exit code 3, no invented offsets) |
| Windows backend | documented stub, not compiled or tested on Windows |

The player-data target chain is:

```text
Process Discovery -> Memory Reader -> Structure Resolver -> Player Data -> CLI
        done             done          NOT YET VERIFIED        -         gated
```

`fc27le players --explain` prints the exact missing evidence. The project stops
there rather than fabricating offsets or pointer chains.

## Quick start

```bash
# Rust (once): https://rustup.rs
cargo build --release

# Verify the reader against a process owned by the project (no game needed)
./target/release/fc27le selftest

# Find the running game
./target/release/fc27le discover

# Full read-only reconnaissance report for one process
./target/release/fc27le probe <pid>

# Machine-readable output
./target/release/fc27le probe <pid> --json
```

Example from the verified session:

```text
$ fc27le discover
Visible processes: 521
FC 27 game: pid 19349 (FC27.exe, score 150)
  exe: /path/to/protons/GE-Proton11-12-x86_64/files/lib/wine/i386-unix/wine64-preloader
  cwd: /path/to/volume/.../EA SPORTS FC 27
  wine prefix: /path/to/.local/share/third-party launcher/prefixes/FC27/pfx/
  proton root: /path/to/protons/GE-Proton11-12-x86_64
```

## Commands

| Command | Purpose |
| --- | --- |
| `discover [--all] [--json]` | Score visible processes; report the game and Wine helpers |
| `info <pid>` | Process metadata, Wine context, mapping/module counts, policy |
| `maps <pid> [--module NAME] [--perms rwx] [--anonymous] [--limit N]` | List memory mappings |
| `modules <pid> [--filter TEXT] [--sections]` | List PE/ELF modules and PE sections |
| `read <pid> --address ADDR --length N` | Bounded read, hexdump and JSON bytes |
| `read <pid> --module FC27.exe --offset 0x1CD3B5C --length 48` | Module-relative read |
| `scan <pid> --module FC27.exe --pattern "48 89 ?? ..."` | Bounded PE-section pattern scan |
| `probe <pid> [--no-hash] [--skip-anchors]` | Full reconnaissance report + anchor verification |
| `players <pid> --explain` | Evidence-gated player-listing report (exit 3) |
| `capabilities` | Implemented / blocked / out-of-scope matrix |
| `selftest` | Controlled end-to-end memory-reader test |

Global flags: `--json` (stdout), `--log-level LEVEL`, `--log-json` (stderr).

Exit codes: `0` success, `1` operational error, `2` invalid input/limit,
`3` insufficient evidence, `4` unsupported, `5` permission denied,
`6` process not found.

## Safety boundaries

* `/proc/<pid>/mem` is opened **read-only**.
* Every read is capped at **1 MiB** and validated against the mapping snapshot
  before `pread`; unmapped holes are errors, never zero-filled.
* There is no `ptrace`, `process_vm_writev`, `mprotect`, `dlopen`, injection or
  hooking code anywhere in the workspace.
* No root is required by default and no privilege is ever requested.
* The tool never changes gaming/runtime configuration, prefixes, game files or
  saves. The only optional file write is `read --output FILE` to a path the
  caller explicitly names.
* Offline, single-player inspection only. Do not use it to interact with online
  services or anti-cheat.

See [`docs/architecture.md`](docs/architecture.md) for the invariants in code.

## What was verified

On 2026-11-15/16, with FC 27 already running on CachyOS through
GE-Proton11-12:

* process `FC27.exe` (PID 19349) discovered with score 150;
* `FC27.exe` SHA-256
  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`,
  235,283,352 bytes, mapped at its preferred base `0x140000000`;
* the `press_any_key_analysis.md` event-dispatcher pattern found **exactly once**
  at RVA `0x1CD3B5C` / VA `0x141CD3B5C`, matching the static file offset
  `0x1CD315C`;
* `proprietary editor DLL` and `activation module` were **not** loaded (the game was
  started directly, not through the Live Editor launcher);
* `third-party-module.dll` **was** loaded, matching the third-party build build in the
  previous research (not the other distribution build);
* bounded reads of the PE header and anchor bytes succeeded as the same user.

Evidence snapshots live in [`docs/evidence/`](docs/evidence/).

## Documentation

* [`docs/architecture.md`](docs/architecture.md) — workspace, data flow, safety invariants
* [`docs/linux_environment.md`](docs/linux_environment.md) — observed host, Wine/Proton, memory layout, permissions
* [`docs/fc27_reconnaissance.md`](docs/fc27_reconnaissance.md) — FC 27 process-discovery and anchor report
* [`docs/research_roadmap.md`](docs/research_roadmap.md) — evidence gates and next investigation steps
* [`docs/compatibility.md`](docs/compatibility.md) — supported environments, builds and limitations

## Relation to the previous Live Editor research

The `prior research project/` directory contains a separate reverse-engineering
project and is treated as **research reference only**. This workspace does not
copy its source, modify its binaries, or reproduce its patching/injection
behaviour. Findings used here are limited to:

* process/module identification and the `FC27.exe` build hash;
* the event-dispatcher byte pattern and its expected RVA;
* the root-cause conclusion that `activation module` was absent and the Live
  Editor's wait for it was unbounded;
* the Lua/SQL field names used to describe the player-data evidence gap.

## License

MIT. See [`LICENSE`](LICENSE).
