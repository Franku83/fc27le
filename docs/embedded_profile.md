# Embedded Metadata Profile

FC27LE can browse the verified FC 27 build without requiring a user-supplied
metadata XML. The compact profile is implemented in
`crates/fc27/src/embedded_profile.rs`.

## What is embedded

The profile contains four tables and only the fields used by the read-only
player browser, name resolver and first-party player-field helpers:

| Table | Purpose | Fields |
| --- | --- | --- |
| `players` | Player records | Identity and position fields used by the browser, plus the independently verified player fields (birthdate, gender, nationality, preferred foot, contract, PlayStyles, retirement, six face stats and the raw attributes used for generated players). See [verified player fields](player_field_coverage.md) for the complete 4CC table. |
| `playernames` | Compressed name pool | `nameid`, `name` |
| `dcplayernames` | Inline names for IDs 46000+ | `nameid`, `name` |
| `editedplayernames` | Per-player overrides | `playerid`, `firstname`, `surname`, `commonname` |

The profile is accepted only when the executable SHA-256 is exactly:

```text
d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9
```

Every load rechecks the listed fields against the live table descriptors:
4CC presence, bit width, bit offset, field type, record bounds, table
count/capacity/invalid markers, the player-id index count, and the name pools.
If any check fails, the embedded profile is refused and the UI reports the
validation error. A mismatch never falls back to guessed data.

## Provenance and redistribution

This is **not** a converted or redistributed copy of the proprietary metadata
XML. It is a hand-written minimal schema containing only the identifiers and
decoding bounds needed by the independently verified reader. The field set was
derived from the runtime facts needed by WP4/WP5 and is revalidated live on every
connection. The player fields added in WP10 were verified by correlating live
FC 27 values with known players, generated-player behaviour and the independently
supplied FC 26 column schema; the schema itself is not embedded. No XML
document, extracted table dump, player dump or third-party file is included.

The external XML mode remains available for:

* other game builds;
* full 268-table database coverage;
* Lua scripts that need tables outside the four embedded tables.

The XML is never downloaded, searched for or bundled. Users must supply a file
they are entitled to use.

## Lua impact

With the embedded profile, `LE.db.tables_count` is **4**. The `players` table
exposes the identity/position fields used by the read-only scan and the 52
player fields verified in WP10; name lookup works through the three name tables.
With external metadata, the tested session exposes the full 268-table
metadata/live intersection. Scripts that require other tables must use external
XML.

## Validation

Automated checks:

```bash
cargo test -p fc27-game --test embedded_profile
cargo test --workspace
```

The first test builds a synthetic snapshot with the verified record shape and
checks the partial-profile path. The second covers the parser, player reader,
name reader and Lua runtime. Live validation is performed by the normal
metadata/player/name gates when the game is running.
