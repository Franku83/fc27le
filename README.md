# FC27LE

**An experimental, open-source EA SPORTS FC 27 Live Editor alternative written in Rust, with native Linux and Proton support.**

FC27LE is an independent, read-only player browser and Lua scripting host for
EA SPORTS FC 27 running under Wine/Proton on Linux. It is not affiliated with,
endorsed by, or derived from EA, the FC Live Editor, or any game distribution.

> **Alpha status:** FC27LE is experimental. It is tested against one observed
> game build and one Linux/Proton environment. Do not expect other builds,
> distributions, Proton versions or desktop sessions to work without testing.
> Read-only behaviour is deliberate: FC27LE does not edit game memory, saves,
> files or online state.

## Overview

FC27LE connects to a running `FC27.exe` process using Linux `/proc`, verifies
the database-service path and a caller-supplied metadata descriptor, and exposes
the verified player data through:

- a native Rust GUI (eframe/egui), and
- a Lua 5.4 runtime implementing a tested subset of the FC Live Editor Lua API v2.

The project is Linux-first. A Windows backend exists only as a documented stub
and has not been compiled or tested.

## Features

### Verified functionality

These features were exercised against a live game session:

- Linux `FC27.exe` process discovery under Wine/Proton.
- Read-only process inspection with bounded reads and mapping validation.
- FC 27 database metadata validation against live table descriptors.
- Player enumeration: 22,098 player slots, 284 invalid markers, 21,814 valid
  players.
- Player-name resolution through the compressed name pools: 21,814 / 21,814
  names resolved with zero errors.
- Native GUI with search by name or player ID, OVR/POT/height/weight sorting,
  a player detail panel, refresh/reconnect and visible errors.
- Lua 5.4 runtime with a tested read-only Live Editor Lua API v2 subset:
  `LE`, `LE.db`, table lookup, record iteration, field reads, `LOGGER`,
  `MessageBox` and `print`.
- External floating-window mode with F9 hide/show and a Unix-socket toggle for
  a KDE global shortcut.

### Planned or partial functionality

- Additional FC 27 game builds and distributions (untested).
- Additional Proton/Wine versions and Linux distributions (untested).
- Team, league and competition names (not implemented).
- Career-state checks and career-specific Lua helpers (not implemented).
- Event callbacks and write APIs (explicit compatibility errors today).
- Windows process backend (stub only).

FC27LE does **not** claim full Lua API compatibility, game-memory editing,
injected in-game overlay support, universal FC 27 build compatibility, or
verified Windows support.

## Current limitations

- **Read-only.** There is no memory-write, save-edit, hook, injection or
  game-function-call code. Lua setters and write helpers raise explicit errors.
- **One tested build.** The verified `FC27.exe` SHA-256 is listed below. A hash
  match only identifies a file; it does not guarantee that a structure layout is
  unchanged.
- **User-supplied metadata.** FC27LE requires a matching FC 27 metadata XML.
  The file is proprietary game data and is not distributed with this project.
  Without a matching file, metadata validation and player enumeration fail
  closed.
- **No true overlay.** The GUI is an external floating window. On the tested
  KWin/Wayland session, an always-on-top window became click-through and an
  exclusive-fullscreen game could still keep input. Use borderless/windowed
  mode or a second monitor for reliable interaction.
- **Lua is not a sandbox.** Third-party scripts are untrusted. Standard Lua file
  I/O remains available for compatibility; `os.execute`, `io.popen` and
  `package.loadlib` are removed. Scripts run with wall-clock, instruction and
  memory limits.
- **No online/anti-cheat interaction.** Use offline/single-player only.

## Supported environments and builds

### Tested environment

| Item | Tested value |
| --- | --- |
| Game | EA SPORTS FC 27 |
| Executable | `FC27.exe`, 235,283,352 bytes |
| SHA-256 | `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` |
| Runtime | GE-Proton11-12 |
| Host | CachyOS, KDE Plasma 6.8.3, KWin/Wayland, XWayland for the GUI |
| Rust | stable toolchain; tested with rustc 1.100.0 |
| Metadata | caller-supplied matching FC 27 metadata XML |
| Live counts | 268/268 metadata tables matched, 263 fixed-row layouts verified |
| Players | 22,098 slots / 27,000 capacity / 284 invalid / 21,814 valid |
| Names | 21,814 / 21,814 resolved, 0 errors |
| Lua tables | 268 tables exposed in the tested session |

Other game builds, Proton versions, Linux distributions, GPUs, compositors and
desktop sessions require independent testing. Compatibility reports are welcome;
see `CONTRIBUTING.md`.

## Installation / Building from source

Requirements:

- Linux x86_64
- Rust stable (see `rust-toolchain.toml`)
- A C toolchain and the usual native dependencies for `eframe`/`egui`
- FC 27 installed and running under Wine/Proton, for live use

Build everything:

```bash
cargo build --release --workspace
```

Binaries:

- `target/release/fc27le` — CLI
- `target/release/fc27le-gui` — GUI

Run the verification suite:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
```

The controlled reader self-test does not need the game:

```bash
./target/release/fc27le selftest
```

## Running the GUI

```bash
env -u WAYLAND_DISPLAY DISPLAY=:0 \
  ./target/release/fc27le-gui \
  --metadata /path/to/fifa_ng_db-meta.xml
```

The GUI auto-discovers the running `FC27.exe`. If discovery is ambiguous, pass
`--pid <pid>`. The metadata path can also be supplied with
`FC27LE_METADATA=/path/to/fifa_ng_db-meta.xml`.

Useful options:

```bash
# Start with a player search already applied
./target/release/fc27le-gui --metadata FILE --filter Messi

# Load a Lua script, then run it after the Lua session is ready
./target/release/fc27le-gui --metadata FILE \
  --script /path/to/script.lua --run-script

# Toggle/show/hide an already running instance
./target/release/fc27le-gui --toggle
./target/release/fc27le-gui --show
./target/release/fc27le-gui --hide
```

F9 hides the window while it has keyboard focus. For a global shortcut, bind a
KDE custom shortcut to:

```bash
fc27le-gui --toggle
```

### Metadata requirement

The metadata XML is proprietary game data. FC27LE does not redistribute it and
does not provide acquisition instructions. Supply a file that matches your game
build; validation is performed against the live table manager and the focus
table (`players`). A mismatched or incomplete file is rejected rather than
guessed around.

## Using the player browser

1. Start FC 27 and wait until the game database is loaded.
2. Launch `fc27le-gui` with the metadata path.
3. Confirm the connection panel reports the PID, build hash, metadata validation
   and player counts.
4. Search by player name or player ID.
5. Sort by name, ID, OVR, POT, height or weight.
6. Select a row to inspect the player detail panel.
7. Use Refresh or Reconnect after changing game state.

The GUI and Lua runtime use the same verified `fc27-game` table snapshot layer.
There is no second, divergent database implementation for scripts.

## Lua API compatibility

FC27LE embeds Lua 5.4 through `mlua` (`lua54`, `vendored`). The implemented
subset is read-only and explicitly error-stubbed elsewhere:

- Supported: `LE`, `LE.db`, `GetTable`, `GetTableByShortname`,
  `GetTableNames`, record iteration, field reads, `LOGGER`, `MessageBox`,
  `print`.
- Unsupported with explicit errors: write operations, event registration,
  `MEMORY`, `HTTP`, `REQUEST`, career managers and unverified helper functions.

Record handles are numeric tokens, not process pointers. They become stale
after `LE.db:Refresh`, `Load` or `Reset`; obtain a fresh table handle afterwards.
Scripts run on a background worker with a 30-second wall-clock limit, a 256 MiB
memory limit and cancellation support.

The full per-function matrix, signatures, return types, evidence and limitations
is generated in [`docs/lua_compatibility.md`](docs/lua_compatibility.md).

Minimal read-only example:

```lua
local players = LE.db:GetTable("players")
local record = players:GetFirstRecord()
local count = 0
while record > 0 do
    local playerid = players:GetRecordFieldValue(record, "playerid")
    local overall = players:GetRecordFieldValue(record, "overallrating")
    count = count + 1
    record = players:GetNextValidRecord()
end
LOGGER:LogInfo(string.format("valid players: %d", count))
MessageBox("FC27LE", string.format("Scanned %d players", count))
```

## Development roadmap

- [x] Read-only process discovery and module/section inspection.
- [x] Verified database-service path and table descriptor access.
- [x] Metadata parsing/validation with a caller-supplied XML.
- [x] Player enumeration and name resolution.
- [x] Native GUI player browser.
- [x] Lua 5.4 read-only compatibility subset.
- [ ] Additional game-build and Proton compatibility reports.
- [ ] Team/league/competition names.
- [ ] Career-state checks and career-specific read-only helpers.
- [ ] Windows backend, only after a real Windows implementation and test.
- [ ] Further Lua APIs only when a verified read-only source exists.

No roadmap item implies memory writing, hooks, injection or anti-cheat/DRM
changes.

## Contributing

Small, evidence-backed contributions are welcome. Read
[`CONTRIBUTING.md`](CONTRIBUTING.md) first. In particular:

- Do not upload game executables, archives, metadata XML, saves, memory dumps,
  extracted databases or proprietary assets.
- Do not add write, injection, hooking or anti-cheat/DRM bypass code.
- Include reproduction steps, the exact build hash, and sanitized logs.
- Keep changes consistent with the read-only safety boundary.

## Security

Please do not report security issues in public issues. Use GitHub's private
vulnerability reporting workflow for this repository; see [`SECURITY.md`](SECURITY.md).

## License

MIT. See [`LICENSE`](LICENSE). Third-party dependency licenses are listed in
[`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md). The project does not
redistribute game data or third-party proprietary binaries.

## Disclaimer

FC27LE is an unofficial community project. It is not affiliated with or endorsed
by EA, the FC Live Editor project, or any game distributor. EA SPORTS FC and
related marks are trademarks of their respective owners.

The software is provided as-is, without warranty. Use it only with a legally
obtained copy of the game, offline/single-player, and in accordance with the
game's terms and applicable law. FC27LE does not modify game files, saves,
Protection/activation components, or anti-cheat systems. It does not enable or
encourage online cheating.

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — workspace, data flow, safety invariants
- [`docs/compatibility.md`](docs/compatibility.md) — tested environments, builds and limits
- [`docs/lua_compatibility.md`](docs/lua_compatibility.md) — generated Lua API matrix
- [`docs/research_roadmap.md`](docs/research_roadmap.md) — evidence gates and next research
- [`docs/research/`](docs/research/) — database, metadata, player-storage and name-storage notes
- [`docs/evidence/`](docs/evidence/) — sanitized verification summaries, no raw game data
