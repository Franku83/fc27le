# FC27LE v0.1.0-alpha.1

**Experimental, read-only Linux alpha.**

FC27LE is an independent FC 27 player browser and Lua 5.4 scripting host for
Linux/Wine/Proton. It is not affiliated with EA or the FC Live Editor project.

## Download

- `fc27le-v0.1.0-alpha.1-linux-x86_64.tar.gz`
- `fc27le-v0.1.0-alpha.1-linux-x86_64.tar.gz.sha256`

Verify before extracting:

```bash
sha256sum -c fc27le-v0.1.0-alpha.1-linux-x86_64.tar.gz.sha256
tar -xzf fc27le-v0.1.0-alpha.1-linux-x86_64.tar.gz
cd fc27le-v0.1.0-alpha.1-linux-x86_64
chmod +x fc27le fc27le-gui
env -u WAYLAND_DISPLAY DISPLAY=:0 ./fc27le-gui
```

The published archive is dynamically linked, built on Ubuntu 24.04, and requires
**glibc 2.39 or newer**. It uses the desktop's Mesa/OpenGL/EGL and Wayland/X11
stack. It is not statically linked.

## Highlights

- Embedded minimal metadata profile for the verified build: no XML needed for
  players and names. External XML remains supported for other builds and
  full-table Lua coverage.
- Accent-insensitive, multi-token name search and exact player-ID search.
- Keyboard navigation and deterministic sorting.
- Players / Lua Scripts / Settings layout with resizable details.
- Remembered metadata path in the per-user config directory.
- Sanitized diagnostics preview/export.
- CI and reproducible release packaging.

## Verified

- `FC27.exe` SHA-256
  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`.
- 22,098 slots / 21,814 valid players / 21,814 resolved names.
- 268/268 external metadata tables matched; 263 fixed-row layouts verified.
- Lua scan over 21,814 players, MessageBox and cancellation in the earlier live
  WP6 session.
- 91 automated tests at release preparation.

## Known limitations

- One tested build and environment: CachyOS, KDE/KWin/Wayland + XWayland,
  GE-Proton11-12.
- The embedded profile exposes 4 Lua tables and only the player/name fields.
  Other tables and builds require user-supplied matching XML.
- The game was not running while the release tarball was packaged; the
  clean-install smoke test covers disconnected startup, CLI help/selftest and
  the metadata-selection UI. Live player loading was verified in the earlier
  session with external XML.
- No memory writes, injection, hooks, game-function calls, anti-cheat/DRM
  interaction, true in-game overlay or verified Windows support.
- No full Live Editor Lua API compatibility.
- Lua is not a security sandbox; third-party scripts are untrusted.
- The GUI is a floating XWayland window. Exclusive-fullscreen games can keep
  input; borderless/windowed mode or a second monitor is reliable.

## Checksums

The release asset is accompanied by a `.sha256` file. Recompute and compare it
after downloading; do not trust a binary from an unverified mirror.
