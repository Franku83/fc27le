# Changelog

## v0.1.0-alpha.1 — 2026-11-16

First public Linux alpha.

### Added

- Embedded minimal metadata profile for the verified FC 27 build (players plus
  the three player-name tables); external XML remains supported.
- Accent-insensitive, multi-token, partial-name and exact-player-ID search with
  a precomputed search index.
- Keyboard navigation and deterministic sorting for name, ID, OVR, POT, height
  and weight.
- Players / Lua Scripts / Settings navigation, resizable player detail panel,
  compact status bar and improved empty/error states.
- Per-user configuration with atomic writes and a remembered external metadata
  path.
- Sanitized diagnostics preview and export.
- GitHub Actions CI.
- Reproducible Linux x86_64 release packaging with SHA-256 checksum.
- Portable installation and release documentation.

### Verified

- 22,098 player slots, 21,814 valid players and 21,814 resolved names.
- 268/268 external metadata tables matched and 263 fixed-row layouts verified.
- Read-only Lua scan over 21,814 players, MessageBox and Stop/cancellation.
- 91 automated tests at release preparation.

### Known limitations

- One tested FC 27 build and one CachyOS/KDE/Wayland/GE-Proton environment.
- The embedded profile covers only players and names for that build and exposes
  4 Lua tables; other builds and full-table Lua coverage require external XML.
- No memory writes, injection, hooks, anti-cheat interaction, true in-game
  overlay or verified Windows support.
- No full Live Editor Lua API compatibility.
- Lua is not a security sandbox; third-party scripts are untrusted.
