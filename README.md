# FC27LE

**An open-source EA SPORTS FC 27 Live Editor alternative, built in Rust for Linux and Proton.**

[![CI](https://github.com/Franku83/fc27le/actions/workflows/ci.yml/badge.svg)](https://github.com/Franku83/fc27le/actions/workflows/ci.yml)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Alpha](https://img.shields.io/badge/status-experimental%20alpha-orange.svg)](https://github.com/Franku83/fc27le/releases/tag/v0.1.0-alpha.1)

**[Download the Linux alpha](https://github.com/Franku83/fc27le/releases/tag/v0.1.0-alpha.1)** · [Report a bug](https://github.com/Franku83/fc27le/issues/new/choose) · [Contribute](CONTRIBUTING.md)

FC27LE connects to a running FC 27 process under Wine/Proton and lets you browse real player data through a native Linux GUI. It also runs a **read-only subset of Live Editor Lua API v2** scripts.

> **Experimental and read-only.** This alpha was validated with one FC 27 executable on CachyOS/KDE and GE-Proton11-12. It cannot edit players or saves, and other builds or environments are not yet verified.

## What works

- **Player browser:** names, IDs, overall rating, potential, height and weight.
- **Fast search:** partial names, multiple words, accents and exact player IDs; sortable results and keyboard navigation.
- **Lua console:** table lookup, record iteration, field reads, logging and script cancellation using Lua 5.4.
- **Native Linux integration:** automatic process discovery, refresh/reconnect and a floating GUI window.
- **Embedded metadata:** no XML file needed for the **verified build** when browsing players.
- **Diagnostics:** opt-in, sanitized compatibility reports for troubleshooting.

In the validated session, the reader enumerated **21,814 players** and resolved all their names.

## Download & run

Download the `linux-x86_64.tar.gz` archive and its `.sha256` checksum from **[Releases](https://github.com/Franku83/fc27le/releases/tag/v0.1.0-alpha.1)**.

```bash
sha256sum -c fc27le-v0.1.0-alpha.1-linux-x86_64.tar.gz.sha256
tar -xzf fc27le-v0.1.0-alpha.1-linux-x86_64.tar.gz
cd fc27le-v0.1.0-alpha.1-linux-x86_64
env -u WAYLAND_DISPLAY DISPLAY=:0 ./fc27le-gui
```

Start FC 27 first and enter your Career Mode save so the database is loaded. FC27LE should discover the process and select the embedded metadata profile automatically **if the executable matches the verified build**. Select an external matching metadata XML in Settings for other builds or broader Lua database coverage.

**Linux requirements:** x86_64, **glibc 2.39+**, and working X11/XWayland plus desktop graphics libraries. This is a dynamically linked Ubuntu 24.04 build, not a universal Linux package.

For CLI usage, run `./fc27le --help` or `./fc27le selftest`.

## Lua scripts

The Lua 5.4 runtime supports a **tested read-only subset** of Live Editor API v2:

```lua
local players = LE.db:GetTable("players")
local row = players:GetFirstRecord()
local count = 0
while row > 0 do
    count = count + 1
    row = players:GetNextValidRecord()
end
MessageBox("FC27LE", "Players: " .. count)
```

With the embedded profile, four name/player tables are exposed. An external matching XML enables wider coverage (268 tables in the tested session). **Write methods, game events, MEMORY access and career-specific managers are not supported.** Unsupported calls fail explicitly. Third-party Lua scripts are untrusted and may still access local files.

See the [full Lua compatibility matrix](docs/lua_compatibility.md).

## Compatibility & limitations

| Item | Status |
| --- | --- |
| Tested OS/runtime | CachyOS · KDE/KWin/Wayland · GE-Proton11-12 |
| Verified `FC27.exe` SHA-256 | `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9` |
| Additional FC 27 builds / Linux setups | Untested; reports welcome |
| Windows | Not supported yet (backend stub) |
| Player/save editing | Not implemented; read-only |
| In-game overlay | **No true overlay**; external floating window only |

On KWin/Wayland, exclusive fullscreen can keep focus/input even when the editor is visible. **Borderless/windowed mode or a second monitor** is more reliable. F9 hides the focused editor; for global show/hide, bind a KDE shortcut to `fc27le-gui --toggle`.

The embedded profile was validated against previously observed game layouts and is checked against live descriptors on connection, but **the packaged alpha was not re-tested end-to-end against a running game**. An external XML remains available as a fallback. See [embedded profile details](docs/embedded_profile.md) and [compatibility notes](docs/compatibility.md).

## Build from source

Install a stable Rust toolchain and the native dependencies required by `eframe/egui`, then:

```bash
cargo build --release --workspace
cargo test --workspace
```

Launch `target/release/fc27le-gui` (or use `target/release/fc27le` for the CLI). Additional GUI flags, configuration and development details are documented under [`docs/`](docs/).

## Contributing

**Linux/Proton compatibility reports, bug fixes and pull requests are welcome.** Include your game executable hash, runtime details, reproduction steps and sanitized diagnostics. Do **not** upload game files, saves, metadata XML, memory dumps or other proprietary material.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before sending a PR. For security issues, use GitHub private vulnerability reporting; see [SECURITY.md](SECURITY.md).

## Documentation

- [Architecture and internals](docs/architecture.md)
- [Compatibility and troubleshooting](docs/compatibility.md)
- [Embedded metadata profile](docs/embedded_profile.md)
- [Lua API compatibility matrix](docs/lua_compatibility.md)
- [Research and evidence](docs/research/)
- [Changelog](CHANGELOG.md) · [Alpha release notes](docs/release_notes_v0.1.0-alpha.1.md)

## License & disclaimer

MIT — see [LICENSE](LICENSE) and [third-party notices](THIRD_PARTY_LICENSES.md).

FC27LE is an independent, unofficial community project, **not affiliated with or endorsed by Electronic Arts or FC Live Editor**. It does not ship game files, metadata XML or activation tools, and does not modify memory, saves, protection systems or anti-cheat. Intended for legally obtained game installations and offline/single-player use only. Provided as-is, without warranty.
