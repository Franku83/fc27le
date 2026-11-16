# Contributing to FC27LE

FC27LE is a small experimental, read-only project. Contributions that improve
verified functionality, documentation, tests or compatibility reports are
welcome.

## Before you contribute

- FC27LE is **read-only** by design. Do not add memory writes, save edits, DLL
  injection, hooks, game-function calls, anti-cheat/DRM changes or online
  interaction.
- Do not upload or commit game executables, archives, metadata XML, extracted
  databases, saves, Proton prefixes, memory dumps, full name pools or copied
  proprietary implementations.
- Do not include local filesystem paths, PIDs, secrets, tokens or personal data
  in issues, pull requests or fixtures.
- If a compatibility concern cannot be resolved safely, open an issue rather
  than guessing.

## Development checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
```

The controlled self-test does not need the game:

```bash
./target/release/fc27le selftest
```

Live testing requires a running FC 27 process and a matching, user-supplied
metadata XML. Do not publish that XML or raw command output containing local
paths or session addresses.

## Pull requests

- Keep changes focused and explain the verified behaviour or evidence.
- Add or update tests for code changes.
- Update the compatibility matrix when adding a Lua API.
- State the tested environment and `FC27.exe` SHA-256 for live claims.
- Preserve the read-only safety boundary.

## Compatibility and bug reports

Please use the GitHub issue forms and include:

- Linux distribution and version
- desktop environment and compositor
- GPU and driver
- Proton/Wine version
- `FC27.exe` SHA-256
- FC27LE version or commit
- reproduction steps and sanitized logs

Never attach game files, proprietary resources, saved games, memory dumps or
personal files.

## Security

Do not report security issues in public issues. See [`SECURITY.md`](SECURITY.md).

## License

By contributing, you agree that your original contribution is licensed under the
project's MIT License. Do not contribute third-party or proprietary material
unless you have the right to distribute it under compatible terms.
