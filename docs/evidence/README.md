# Evidence

This directory contains **sanitized summaries** of the verified live session. It
does not contain raw process memory, player dumps, metadata XML, extracted game
databases, screenshots, PIDs, heap addresses or local filesystem paths.

| File | Content |
| --- | --- |
| `public_summary.json` | Build identity, metadata validation counts, player/name counts, Lua/GUI verification and automated test totals |

The original local captures included richer command output and screenshots. They
are intentionally excluded from the public repository because they contained
machine-specific paths, session-specific addresses, a distribution label or
game data beyond the small public summary above.

## Reproducing the checks

The controlled tests and build checks need no game:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
./target/release/fc27le selftest
```

A live check requires a running copy of FC 27 and a matching, user-supplied
metadata XML:

```bash
./target/release/fc27le discover --json
./target/release/fc27le db <pid> --metadata /path/to/fifa_ng_db-meta.xml --json
./target/release/fc27le players <pid> --metadata /path/to/fifa_ng_db-meta.xml --limit 20 --json
```

Do not publish the metadata XML, game executables, saves, memory dumps or raw
command output that contains local paths or session addresses.

## Stable facts for the tested build

- `FC27.exe` SHA-256:
  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`
- image base `0x140000000`, `SizeOfImage` `0xFA79000`
- metadata: 268/268 tables matched, 263 fixed-row layouts verified
- players: 22,098 slots / 27,000 capacity / 284 invalid / 21,814 valid
- names: 21,814 / 21,814 resolved, 0 errors
- Lua: 268 tables exposed; read-only scan visited 21,814 valid players
- automated tests: 78 passed, 0 failed

PIDs, addresses, mapping counts and table data are session-specific. A hash
match identifies a file, not the correctness of a structure layout.
