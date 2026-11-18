# Compatibility

This document states exactly what has been tested, what is expected to work, and
what is deliberately unsupported. Nothing here is a promise about an untested
build.

## 1. Host compatibility

| Requirement | Status |
| --- | --- |
| Linux with `/proc/<pid>/{maps,mem,status,environ,cmdline}` | required; tested on CachyOS |
| Kernel | tested `7.3.4-1-cachyos`; any reasonably modern Linux should work |
| Architecture | x86_64 tested; the PE parser handles PE32/PE32+ headers, the Linux backend is architecture-neutral |
| Rust | stable toolchain; tested with `rustc 1.100.0` |
| Root | **not required**; no privilege is requested |
| `hidepid` | supported; hidden processes simply do not appear |
| Yama `ptrace_scope` | observed `1`; reads worked because the target was traceable |
| Containers / pressure-vessel | tested inside the Steam Linux Runtime wrapper; no special handling |

## 2. Tested game session

| Component | Tested value |
| --- | --- |
| Game | EA SPORTS FC 27 |
| Executable | `FC27.exe`, 235,283,352 bytes |
| SHA-256 | `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` |
| Runtime | GE-Proton11-12 |
| Loader | `files/lib/wine/i386-unix/wine64-preloader` |
| Image base | `0x140000000` (preferred, not relocated) |
| Image size | `0xFA79000` |

## 3. Game-build compatibility

Builds are identified by SHA-256, never assumed:

| Build hash | Label | Status |
| --- | --- | --- |
| `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` | tested FC 27 build, 2026-11-12 | **tested**: discovery, modules, anchors, DB service path |
| Any other hash | — | unknown: expect anchor mismatch; re-research before use |

The tool reports `known_build: null` for an unknown hash. A hash match only
identifies the file; it does not validate offsets or structures.

### Anchor expectations

| Anchor | Expected RVA | Tested build |
| --- | ---: | --- |
| `fc27.event-dispatcher` | `0x1CD3B5C` | verified |
| `fc27.db-service-global` | `0x7242D5E` | verified live; chain checks pass |

A different build may move the anchor. `probe` reports `mismatch`,
`pattern_missing`, `scan_truncated` or `module_missing`; it never assumes the
expected location is correct. If the anchor fails, treat all dependent
conclusions as invalid for that build.

## 4. Wine/Proton behaviour

Observed and relied upon:

* a Wine process's `/proc/<pid>/exe` is the preloader, not the PE image;
* PE images are mapped writable+executable (`rwxp`) in `/proc/<pid>/maps`;
* only some image sub-ranges are file-backed; the rest is anonymous;
* `SizeOfImage` is required to get a correct module size;
* Wine reserves the preferred image base (`WINEPRELOADRESERVE=140000000-14fa79000`),
  so RVAs and VAs coincide for this build;
* Wine's process tree appears traceable from the same user even under Yama
  scope 1, most plausibly through a `PR_SET_PTRACER` exception (reproduced with a
  controlled helper, not read from kernel internals).

Not guaranteed:

* the traceability exception on other Wine/Proton versions;
* the preferred image base on other builds (the tool always uses the mapped base);
* the exact set of `memfd` and anonymous mappings;
* module enumeration order or count;

If Wine changes any of these, the tool should fail with a clear error or an
anchor mismatch rather than silently misread.

## 5. Permission compatibility

| Situation | Expected result |
| --- | --- |
| Same user, target is a descendant (selftest) | works |
| Same user, target has `PR_SET_PTRACER` exception (observed Wine tree) | works |
| Same user, unrelated native target, Yama scope 1 | `permission_denied` (exit 5) |
| Different user, unprivileged | `permission_denied` |
| `hidepid=2` | process may be invisible; `process_not_found` |
| Yama scope 2 or 3 | `permission_denied` unless the target is traceable |
| Root | technically bypasses DAC/Yama, but **unsupported and unnecessary**; the project never requests it |

The tool does not change `ptrace_scope`, capabilities, groups or sysctls. If the
game is not traceable on a future setup, the correct outcome is a clean
permission error, not escalation.

## 6. Feature compatibility

| Feature | Compatibility |
| --- | --- |
| Process discovery | works with Wine/Proton; heuristic reasons are reported |
| Process metadata | Linux `/proc` fields only |
| Mappings | exact `/proc/<pid>/maps` snapshot |
| Modules | PE and ELF; data files excluded |
| PE headers/sections | PE32/PE32+; headers limited to 1 MiB |
| Bounded reads | 1 MiB per read; holes rejected |
| Pattern scans | PE sections only; per-anchor caps (64 MiB event, 160 MiB DB); configurable |
| Database service path | verified read-only (`fc27le db`): 2 databases, 273 table descriptors |
| Database metadata parser | external XML: 268/268 tables and field widths validated live; embedded profile validates its 4 tables and listed fields live |
| Players descriptor | verified as `CZUM` (152 fields, 156-byte records); live count 22,098 / capacity 27,000 |
| Build SHA-256 | opt-out with `--no-hash` |
| `players` table | **supported read-only** on the verified build with the embedded profile or a matching external XML; emits numeric fields and names (`--no-names` for numeric only) |
| Player names | **supported read-only**: `playernames` Huffman pool, `dcplayernames` IDs 46000+, `editedplayernames` overrides |
| Team names | **partial read-only**: live `tWYe` table, 703 validated club names; missing/national IDs use `TEAM<id>` |
| Competition names | **partial read-only**: nine domestic leagues from `onMQ` + active-competition tags; cups/unknown IDs use `COBJ<id>` or an optional `id_map.json` |
| Career season statistics | **supported read-only** on the tested build: live `0x5C` vector, 22,939 valid records; date from manager type 24 |
| Linux season exporter | **provided** in `scripts/lua/export_season_stats_linux.lua`; UTF-8 CSV, atomic writes, no silent overwrite |
| Memory writes, hooks, injection | **not implemented by design** |
| GUI player browser | **implemented**: `fc27le-gui`, eframe/egui 0.32, verified live against PID 5714 |
| GUI player search | name and numeric player ID; verified |
| GUI sorting | name, ID, OVR, POT, height, weight; verified |
| GUI detail panel | name, playerid, OVR/POT, height/weight, slot, name IDs/source; verified |
| GUI reconnect/refresh | re-runs discovery and the full verified loader; visible errors |
| Lua 5.4 runtime | **implemented read-only subset** via `mlua 0.11` (`lua54`, `vendored`) |
| Lua `LE.db` tables | 4 with the embedded profile; 268 with external XML in the tested metadata/live intersection; `players` verified live |
| Lua record iteration | verified over 21,814 live players; `0` terminator |
| Lua writes/events/managers | **explicit compatibility errors**, never simulated |
| In-game presentation | external floating window; F9 hides while focused; socket toggle restores |
| True always-on-top overlay | **not claimed**; X11 always-on-top made the window click-through under tested KWin/Wayland |
| Windows backend | **stub**, not compiled/tested on Windows |

### 6.1 Application and window behavior

The GUI is a normal native window (eframe/egui). It auto-discovers `FC27.exe`,
validates the embedded profile or a caller-supplied metadata XML against the live table manager, loads
players and names on a background thread, and keeps the render thread free of
blocking database or Lua work.

Tested on KWin 6.8.3 / Wayland with the application running through XWayland:

* normal floating window: keyboard and real mouse input work while FC27LE is the
  active, raised window;
* F9 while the GUI has focus: window hides (`WM_STATE` removed / unmapped);
* `fc27le-gui --toggle` over the Unix socket: window is mapped again and KWin
  reports it as active;
* exclusive-fullscreen FC 27 can still keep input and stacking even when KWin
  reports FC27LE active. The KWin helper is best-effort window activation, not an
  input-capture bypass;
* X11 `_NET_WM_STATE_ABOVE` / `STAYS_ON_TOP`: **not reliable**. In the tested
  session the compositor kept the state but mouse input stayed with the game
  (click-through). The default is therefore `always_on_top = false`.
* A true overlay above exclusive-fullscreen FC 27 is **not** provided. Use
  borderless/windowed game mode or a second monitor for reliable interaction.
  The optional KWin helper only manages the FC27LE window; it does not hook,
  capture or modify the game.

### 6.2 Lua compatibility

The runtime is a read-only subset of the documented Live Editor Lua API v2.
`LE`, `LE.db`, `GetTable`, `GetTableByShortname`, `GetTableNames`, record
iteration, field reads, `LOGGER`, `MessageBox` and `print` are implemented and
tested. Record handles are numeric `(generation, table, slot)` tokens, not
process pointers; `LE.db:Refresh` invalidates them.

Writes, event registration, `MEMORY`, `HTTP`, `REQUEST`, career managers and
unverified helper functions exist only as explicit error stubs. The full
per-entry matrix, signatures, return types, evidence and limitations are in
[`lua_compatibility.md`](lua_compatibility.md). Automated tests use synthetic
fixtures; live read-only tests were run against the build below. The live GUI
Stop button cancelled an infinite loop (`while true do end`) with
`FC27LE: script cancelled` while the window stayed responsive.

## 7. Failure modes

| Symptom | Likely cause | Response |
| --- | --- | --- |
| `process_not_found` (6) | game exited / wrong PID / hidepid | run `discover` again |
| `permission_denied` (5) | target not traceable under current policy | use same user; use selftest; do not escalate |
| `unmapped_range` | address not mapped or read-protected | re-check `maps`; use module+offset |
| `read_limit` (2) | request > 1 MiB | split the read |
| `module ... is not loaded` (4) | wrong PID or module name | run `modules` |
| `pattern missing` | wrong build or wrong pattern | re-verify against the file; update the anchor |
| `mismatch` | pattern exists at another RVA | treat the build as unsupported until re-researched |
| `scan truncated` | scan cap reached before expected RVA | raise `--max-mib` deliberately |
| `probe` pattern missing | wrong build or wrong pattern | do not use the DB path; re-research |
| `db` registry key missing | build does not match the verified service registry | treat as unsupported build |
| `db --metadata` focus fails | metadata/build mismatch or incomplete field set | do not enumerate; re-check the descriptor |
| `players` returns `insufficient_evidence` | no metadata, wrong build, count/capacity mismatch, unstable snapshots, or invalid-marker/index disagreement | read the JSON `checks`; do not bypass the gate |
| `players` emits no records | zero slots or all slots invalid | verify the game has loaded a squad/career database |
| GUI shows `FC 27 not found` | game exited or not discoverable | start FC 27, click Discover, or pass `--pid` |
| GUI toggle socket unavailable | another instance owns the socket or the runtime directory is inaccessible | read the visible error; only one instance listens per user |
| Lua `no active Lua database session` | metadata/player session not loaded yet | connect to FC 27 and load metadata first |
| Lua `FC27LE compatibility:` error | unsupported write/event/memory/helper API | expected; the call is listed in `docs/lua_compatibility.md` |
| Lua `stale` handle | `LE.db:Refresh`, `Load` or `Reset` was called | call `LE.db:GetTable` again and rebuild handles |
| `players` names are null | a specific name ID is missing or its pool entry is corrupt | inspect `name_error` and `name_resolution`; do not substitute another player's name |
| `players` exits 3 on the name pool | `playernames` is absent or its tree/offsets are invalid | use `--no-names` for numeric-only output or revalidate the build |

## 8. Anti-cheat / DRM / online boundary

* The project does not bypass, patch, emulate or interact with EA Anti-Cheat,
  activation/licensing components or any protection mechanism.
* It does not inject code, install hooks, or change game memory.
* Use offline and single-player only. Do not run it against online modes or
  services.
* Integration with, or patching of, third-party editor binaries is out of
  scope and is not reproduced here.

## 9. Versioning and support policy

A build is "supported" for read-only metadata when:

1. `discover` identifies the game with explicit reasons;
2. SHA-256 is recorded (known or unknown);
3. `probe` resolves the PE image and sections;
4. every anchor used by a feature verifies at its expected RVA.

Player enumeration additionally requires:

1. a metadata profile (embedded for the tested build, or external XML) that validates against the live manager;
2. `players` resolving to the verified descriptor with all 152 field widths;
3. `slot_count <= capacity`, `invalid_rows == +0x7E`, and a matching playerid-index
   count;
4. two byte-identical snapshots of the verified record range;
5. a valid `playernames` Huffman tree/pool and readable `dcplayernames` /
   `editedplayernames` tables when names are requested.

If a structural condition fails, the reader exits `insufficient_evidence`
instead of emitting partial or guessed data. A single missing name ID is
reported per record as `null` with a `name_error`; no name is substituted.
