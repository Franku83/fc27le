# FC27LE v0.1.0-alpha.2

**Experimental, read-only Linux alpha. Prerelease.**

FC27LE is an independent FC 27 player browser and Lua 5.4 scripting host for
Linux/Wine/Proton. It is not affiliated with EA or the FC Live Editor project.
It does not edit players, transfers, saves or game memory.

## Download

- `fc27le-v0.1.0-alpha.2-linux-x86_64.tar.gz`
- `fc27le-v0.1.0-alpha.2-linux-x86_64.tar.gz.sha256`

Verify before extracting:

```bash
sha256sum -c fc27le-v0.1.0-alpha.2-linux-x86_64.tar.gz.sha256
tar -xzf fc27le-v0.1.0-alpha.2-linux-x86_64.tar.gz
cd fc27le-v0.1.0-alpha.2-linux-x86_64
chmod +x fc27le fc27le-gui
env -u WAYLAND_DISPLAY DISPLAY=:0 ./fc27le-gui
```

The published archive is dynamically linked, built on Ubuntu 24.04, and
requires **glibc 2.39 or newer**. It uses the desktop's Mesa/OpenGL/EGL and
Wayland/X11 stack. It is not statically linked.

## Highlights

- **Career Mode season statistics:** `GetPlayersStats`, `GetPlayerStats`,
  `IsInCM`, `GetCurrentDate`, `GetPlayerName`, `GetTeamIdFromPlayerId`,
  `GetTeamName`, `GetPlayerPrimaryPositionName` and partial
  `GetCompetitionNameByObjID`.
- **Linux season-statistics exporter:** `scripts/lua/export_season_stats_linux.lua`
  with UTF-8 CSV escaping, XDG/user-selected output, atomic writes and no silent
  overwrite. The original FC 26 script is kept unchanged.
- **Verified live data:** 22,939 valid season-stat records, 13,653 exported rows,
  703 live team names and nine domestic league names.
- **Diagnostics fixes:** saving a report creates a missing parent directory, and
  diagnostic logs redact numeric PID values.
- **Screenshots and documentation:** real Career Mode, player-browser and Lua
  screenshots; a compact user-facing README; a detailed season-export guide.
- **Compatibility matrix:** updated honestly for supported, partial and
  unsupported APIs.

## Verified

- `FC27.exe` SHA-256
  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`.
- 22,098 slots / 21,814 valid players / 21,814 resolved names.
- 22,939 valid season-stat records in the live career session.
- 13,653 exported rows after the original script's filters; 40 duplicate
  player/competition rows replaced, matching the original behavior.
- In-game date 13/05/2027 in the tested session.
- The untouched original `export_season_stats.lua` completed through the
  compatibility layer and logged `Done` (its Windows path is not Linux-safe).
- `cargo fmt --all --check`, Clippy with `-D warnings`, workspace tests and the
  release build passed before packaging.
- The packaged archive was extracted, checksum-verified and smoke-tested.

## Known limitations

- One tested build and environment: CachyOS, KDE/KWin/Wayland + XWayland,
  GE-Proton11-12.
- Cup and national-team display names are partial. Unresolved competition IDs
  use the documented `COBJ<id>` fallback; unresolved team IDs use `TEAM<id>`.
  An optional `id_map.json` can supply exact competition names.
- `GetTeamIdFromPlayerId` is derived from season rows, not the proprietary
  career-player object.
- Season statistics are a snapshot taken when the Lua session is created.
- No memory writes, injection, hooks, game-function calls, anti-cheat/DRM
  interaction, true in-game overlay or verified Windows support.
- No full Live Editor Lua API compatibility.
- Lua is not a security sandbox; third-party scripts are untrusted.
- The GUI is a floating XWayland window. Exclusive-fullscreen games can keep
  input; borderless/windowed mode or a second monitor is reliable.
- The screenshots are composed from real captured states for presentation; no
  interface state or statistic was fabricated.

## Checksums

The release asset is accompanied by a `.sha256` file. Recompute and compare it
after downloading; do not trust a binary from an unverified mirror.

`v0.1.0-alpha.1` remains unchanged and available.
