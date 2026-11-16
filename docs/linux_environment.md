# Linux Environment — Phase 0 Observations

**Date:** 2026-11-15/16
**Host:** CachyOS Linux x86_64, kernel `7.3.4-1-cachyos`
**Game state:** EA SPORTS FC 27 already running; no reinstall, reconfiguration or
runtime troubleshooting was performed.
**Access:** read-only `/proc` inspection as the same unprivileged user.

All values below are snapshots. PIDs, mapping counts and RSS change between
reads; the build hash and the executable path are stable.

## 1. Host

| Property | Value |
| --- | --- |
| Distribution | CachyOS (Arch-like, rolling) |
| Kernel | `7.3.4-1-cachyos` |
| Architecture | x86_64 |
| Session | Wayland (`DISPLAY=:0`, `PROTON_ENABLE_WAYLAND=1`) |
| Game installation | local copy; installation path and distribution metadata omitted |
| Privileges | same unprivileged user; no root, no capabilities, no ptrace escalation |

## 2. Process hierarchy

`FC27.exe` ran under a Wine/Proton launcher. The exact launcher, installation
path and distribution are intentionally omitted from this public repository.
The observed tree had the usual shape:

```text
launcher
  └─ pressure-vessel / Proton wrapper
      └─ FC27.exe
          └─ Wine services
```

`fc27le discover` scores `FC27.exe` by process name, command line, Wine loader
path and Wine environment. Wine service processes and the `steam.exe` shim are
classified as helpers, never as the game.

## 3. Game process

| Property | Value |
| --- | --- |
| `/proc/<pid>/comm` | `FC27.exe` |
| `/proc/<pid>/cmdline` | `Z:\path\to\FC27.exe` |
| `/proc/<pid>/cwd` | `<game directory>` |
| `/proc/<pid>/exe` | `<proton>/files/lib/wine/i386-unix/wine64-preloader` |
| Threads | dynamic (snapshot only) |
| UID / GID | same user as FC27LE |
| Effective capabilities | `CapEff = 0` |
| `NoNewPrivs` | 1 |

`/proc/<pid>/exe` resolves to the Wine preloader for every Wine process, so the
PE image path must come from `/proc/<pid>/maps` and the command line, not from
`exe`. This is why discovery and module resolution use both.

## 4. Wine/Proton runtime and prefix

| Property | Value |
| --- | --- |
| Runtime | GE-Proton11-12 |
| Proton root | `<proton root>` |
| Wine prefix | `<wine prefix>` |
| `STEAM_COMPAT_DATA_PATH` | `<compat data path>` |
| Loader | `files/lib/wine/i386-unix/wine64-preloader` |
| Image reservation | `WINEPRELOADRESERVE=140000000-14fa79000` |

Selected environment variables observed in the game process:

| Variable | Value / meaning |
| --- | --- |
| `WINEDEBUG` | `-all` (Wine logging disabled) |
| `WINEESYNC` / `WINEFSYNC` / `WINENTSYNC` | `1` / `1` / `1` |
| `WINEDLLOVERRIDES` | native overrides for graphics and API DLLs |
| `WINEDLLPATH` | GE-Proton `vkd3d` + `wine` libraries |
| `LD_PRELOAD` | pressure-vessel `gameoverlayrenderer.so` |
| `PROTON_ENABLE_WAYLAND` | `1` |
| `WINE_GRAPHICS_DRIVER` | `wayland` |
| `WINE_LARGE_ADDRESS_AWARE` | `1` |
| `DXVK_LOG_LEVEL` / `VKD3D_DEBUG` | `none` |

Distribution-specific files are not described or redistributed. The measured
build hash is listed below; other copies may differ.

## 5. Build identity

| Property | Value |
| --- | --- |
| File | `FC27.exe` |
| Size | 235,283,352 bytes |
| SHA-256 | `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` |
| PE machine | `0x8664` (x86-64) |
| PE timestamp | `1793851090` |
| Preferred image base | `0x140000000` |
| `SizeOfImage` | `0xFA79000` (262,639,616 bytes, 250.5 MiB) |
| Entry point RVA | `0x2BB90D0` |
| Subsystem | 2 (GUI) |
| Sections | 16 |

This hash matches the build recorded in
`prior research notes/runtime_evidence.md`.

## 6. Memory layout

Snapshot at probe time: **2590 mappings**.

| Mapping kind | Count |
| --- | ---: |
| file-backed | 901 |
| anonymous | 772 |
| device (`/dev/dri/renderD128`) | 905 |
| `memfd` (Wine loader) | 6 |
| special (`[stack]`, `[vdso]`, ...) | 6 |

Permission strings in `/proc/<pid>/maps`: `rw-p` 621, `r--p` 624, `rwxp` 9,
`r--s` 11, `rw-s` 914, `r-xp` 197, `--xp` 1, `---p` 213.

### FC 27 image

The module is one virtual `rwxp` range:

```text
0x140000000..0x14FA79000   FC27.exe image (SizeOfImage 0xFA79000)
```

Only three sub-ranges are file-backed:

| Range | File offset | Content |
| --- | ---: | --- |
| `0x140000000..0x140001000` | `0x0` | PE headers |
| `0x14DC9D000..0x14E41D000` | `0xC28A000` | `.pdata` |
| `0x14E96F000..0x14ED68000` | `0xCF5A000` | `fieldinf` |

Everything else in the image is anonymous memory. This is why:

* the module size comes from the PE `SizeOfImage`, not from `/proc/maps`;
* the file offset relationship for this build is `file_offset = VA - 0x140000000`;
* the image is mapped at its preferred base, so `VA = 0x140000000 + RVA`.

Section table (from `fc27le modules <pid> --filter FC27 --sections`):

| # | Name | RVA | Virtual size | R/W/X |
| ---: | --- | ---: | ---: | --- |
| 0 | `.text` | `0x1000` | `0x8BDC600` | r-x |
| 1 | `ctr` | `0x8BDE000` | `0x9FF000` | r-x |
| 2 | `.rdata` | `0x95DD000` | `0x26E0E00` | r-- |
| 3 | `.data` | `0xBCBE000` | `0x1FDEC90` | rw- |
| 4 | `.pdata` | `0xDC9D000` | `0x77F200` | r-- |
| 5 | `argus` | `0xE41D000` | `0x400` | rw- |
| 6 | `.rodata` | `0xE41E000` | `0xE00` | r-- |
| 7 | `typeinfo` | `0xE41F000` | `0x54FC00` | rw- |
| 8 | `fieldinf` | `0xE96F000` | `0x3F8200` | rw- |
| 9 | `.rsrc` | `0xED68000` | `0x3F400` | r-- |
| 10 | `.reloc` | `0xEDA8000` | `0x351600` | r-- |
| 11 | `.idata` | `0xF0FA000` | `0xAA00` | rw- |
| 12 | `.text` | `0xF105000` | `0x931000` | r-x |
| 13 | `.bm` | `0xFA36000` | `0x16E0` | r-- |
| 14 | `.l2` | `0xFA38000` | `0x3F400` | r-- |
| 15 | `.thirdparty` | `0xFA78000` | `0x712` | rw- |

**Important:** Linux `rwxp` is not a Windows protection. Wine's PE loader maps
the image writable+executable. The PE section characteristics (shown above)
remain the authoritative read/write/execute description.

### Modules

`fc27le modules <pid>` reports **200** PE/ELF modules at snapshot time. Notable
entries:

| Base | Module |
| --- | --- |
| `0x140000000` | `FC27.exe` |
| `0x6FFFFFE70000` | `kernel32.dll` (Wine builtin) |
| `0x6FFFFFEF0000` | `ntdll.dll` (Wine builtin) |
| `0x6FFFFF3F0000` | `user32.dll` (Wine builtin) |
| `0x7F8BFD00E000` | `ntdll.so` (native Wine library) |

Only the game image and standard Wine modules are relevant to this project.

### 6.3 Runtime database interface

`fc27le db <pid>` verified the game's database service path read-only:

| Item | Snapshot value |
| --- | --- |
| DB service pattern | RVA `0x7242D5E`, VA `0x147242D5E`, unique |
| Global service object | `0x14BEB6538` (vtable `0x1498A14D0`) |
| Registry key | `0x0AE932D0` -> manager interface `0x30E0B9A8` |
| Table manager root | `0x30E0B9A0` |
| Databases | `g_db` (269 tables), `a_mx` (4 tables) |
| Table descriptors | **273** |

Table descriptors use 4CC codes and a bit-packed layout. `+0x44` is the record
size in bytes; `+0x48 == +0x44*8 - 1` for all 273 tables. Full table/field names
are not in the game process; Live Editor loads them from
`data/db/fifa_ng_db-meta.xml`. With a caller-supplied FC 27 metadata XML,
`fc27le db <pid> --metadata FILE` validated 268/268 tables and resolved
`players` to `CZUM` (152 fields, 156-byte records, 4 padding bits, non-zero data
pointer). WP4 then verified the storage counters: `+0x78` capacity, `+0x7C`
slot count, `+0x7E` invalid markers, and validity by the high bit of the last
record byte. The live capture was 22,098 slots / 27,000 capacity / 284 invalid
/ 21,814 valid. `fc27le players --metadata FILE` emits the verified numeric
fields and player names. WP5 resolved 21,814/21,814 names through the
`playernames` Huffman pool, `dcplayernames` and `editedplayernames` overrides.
See `docs/research/db_metadata.md`, `docs/research/player_storage.md` and
`docs/research/name_storage.md`.

## 7. Linux process-inspection permissions

### Policy snapshot

| Control | Value |
| --- | --- |
| User | UID 1000 (`franku`) |
| Effective capabilities | `0x0` |
| `/proc/sys/kernel/yama/ptrace_scope` | `1` |
| `/proc/sys/kernel/perf_event_paranoid` | `2` |
| `/proc` `hidepid` | not set |
| Root | not used |

### Empirical access

At `ptrace_scope=1`, Yama normally restricts attachment to descendants. The game
is not a descendant of the inspection process, yet these reads succeeded:

* `/proc/<pid>/maps`, `/proc/<pid>/status`, `/proc/<pid>/environ` — readable;
* `/proc/<pid>/mem` opened `O_RDONLY`; the PE header (`MZ`) and anchor bytes were
  read successfully.

Control observations on the same host and user:

* an unrelated native process (`java`, PID 15253, same UID) denied
  `/proc/15253/mem` with `EACCES`;
* `systemd --user` (PID 1039, same UID) denied even `/proc/1039/maps`;
* the Wine process tree (all `wine64-preloader` children and `wineserver`) was
  readable.

### Controlled PR_SET_PTRACER experiment

To identify the likely mechanism without touching the game, two helper
processes owned by this project were started as siblings (neither is an ancestor
of the reader):

| Helper | Setup | Read result |
| --- | --- | --- |
| PID 38017 | `prctl(PR_SET_PTRACER, PR_SET_PTRACER_ANY)` | **allowed** |
| PID 38018 | no `prctl` (control) | **denied** (`EACCES`) |

This reproduces the observed pattern: a `PR_SET_PTRACER_ANY` exception makes a
same-user, non-descendant process readable under Yama scope 1.

**Inference (not read from kernel internals):** Wine/Proton's process tree
appears to install such a traceability exception, likely for its own debugging
support. That is why the game is readable here without root. It is an
environment property, not a guarantee of this tool.

### Consequences

* The tool works as an unprivileged same-user process **when the target is
  traceable** (descendant or `PR_SET_PTRACER` exception).
* If a future Wine build does not install that exception, reads fail with
  `permission_denied` (exit 5); the tool does **not** change Yama, use `sudo`, or
  attach with `ptrace`.
* The controlled `selftest` always works because the target is the tool's own
  child; it validates the reader independently of Wine's behaviour.
* No root requirement exists by default. Raising `ptrace_scope` or granting
  capabilities is a system-administration decision outside this project.

## 8. Reproduction commands

```bash
cd fc27le
cargo build --release

./target/release/fc27le selftest
./target/release/fc27le discover
./target/release/fc27le info <pid>
./target/release/fc27le modules <pid> --filter FC27 --sections
./target/release/fc27le maps <pid> --module FC27.exe
./target/release/fc27le probe <pid>
```

`probe` hashes the 235 MiB executable by default; use `--no-hash` to skip that
cost and `--skip-anchors` to skip the bounded 64 MiB pattern scan.

## 9. Desktop session and window behavior

Observed host session during WP6:

| Item | Value |
| --- | --- |
| Desktop | KDE Plasma 6.8.3 |
| Compositor | `kwin_wayland` 6.8.3 |
| Session | Wayland (`WAYLAND_DISPLAY=wayland-1`) |
| App transport | XWayland (`DISPLAY=:0`, `WAYLAND_DISPLAY` unset) |
| Display | 1920x1080 primary (DP-2); X screen reports 3000x1920 across outputs |

`fc27le-gui` is a normal eframe/egui window. F9 hides it while it has keyboard
focus; a KDE global shortcut bound to `fc27le-gui --toggle` restores it through
a per-user Unix socket. The optional KWin scripting helper requests activation
of the FC27LE window by caption and resource class.

Window findings:

* normal floating window, active and raised: keyboard and real mouse input work;
* X11 `_NET_WM_STATE_ABOVE` / `STAYS_ON_TOP`: compositor keeps the state but the
  game kept mouse input in testing (click-through), so it is **off by default**;
* exclusive-fullscreen FC 27 can keep input and stacking even when KWin reports
  FC27LE active; the helper is best-effort window management, not an overlay or
  input-capture bypass;
* borderless/windowed game mode or a second monitor is the reliable interactive
  setup.

No game window is hooked, captured, resized or moved by FC27LE.

## 10. What was not changed

No game file, third-party file, anti-cheat file, Proton prefix, registry key, graphics
setting, career save or system sysctl was modified. The only writes performed by
this project are build artifacts under `target/`, evidence files under
`docs/evidence/`, and files the user explicitly asks the GUI to save (Lua
scripts). Standard Lua file I/O remains available to scripts; that is the script
author's action, not an automatic game/save write. The `read --output` option is
never used automatically.
