# WP5 — Player Name Storage and Resolution

**Date:** 2026-11-16
**Target:** live `FC27.exe` SHA-256 `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`
**Metadata:** caller-supplied FC 27 descriptor, SHA-256 `<metadata hash>` (not committed)
**Mode:** read-only; no writes, hooks, DLL injection, game-function calls or save modifications
**Result:** **21,814 / 21,814** live player records resolve to names with **zero** errors; 43,065 `playernames` entries and 2,279 `dcplayernames` entries decode, and 8 `editedplayernames` overrides are applied.

---

## 1. Result

`fc27le players` now returns verified names alongside the numeric fields:

```text
slot   playerid  name                           ovr  pot  height  weight
57        20801  Cristiano Ronaldo               84   94     187      85
7490     158023  Lionel Messi                    88   94     169      67
7768     192985  Kevin De Bruyne                 84   91     181      75
8759     212831  Alisson                         87   90     193      91
9023     215914  N'Golo Kanté                    82   90     168      70
10918    231747  Kylian Mbappé                   91   94     182      81
```

For `20801` the three name fields are distinct and all resolve:

| Field | Name ID | Decoded value |
| --- | ---: | --- |
| `firstnameid` | 6,183 | `C. Ronaldo` |
| `lastnameid` | 42,468 | `dos Santos Aveiro` |
| `commonnameid` | 8,211 | `Cristiano Ronaldo` |

The display name prefers the common name, then `first + last`.

Live coverage from the full pass:

| Source | Resolution |
| --- | --- |
| `playernames` references | 45,306 |
| `dcplayernames` references | 1,857 |
| zero name ID (no name) | 18,255 |
| missing/unresolved | **0** |
| `editedplayernames` overrides | 8 |

All 21,814 valid players got a display name where at least one component exists. No name was fabricated.

## 2. `playernames`

### 2.1 Row layout

The table descriptor is resolved by the metadata name `playernames`. The live row is 12 bytes:

| Offset | Width | Meaning |
| --- | --- | --- |
| `+0x00` | 4 | Byte offset into the string pool |
| `+0x04` | 2 | `nameid` (u16) |
| `+0x06` | 2 | `commentaryid` (u16) |
| `+0x08` | 4 | Zero / unused |

`nameid` is a 16-bit key. `+0x00` is not a pointer: it is a **byte offset** from the pool base. The sentinel `0xFFFFFFFF` appears on the unused row 0.

### 2.2 Pool location

The pool base is the `u64` at **table descriptor `+0x38`**. In the tested session:

| Item | Value |
| --- | --- |
| `playernames` descriptor | `<descriptor>` |
| Row data (`+0x30`) | `0xE9619058` |
| Row size (`+0x44`) | 12 |
| Rows (`+0x7C`) | 43,066 |
| Capacity (`+0x78`) | 43,066 |
| Pool base (`+0x38`) | `0xE9697328` |
| First string offset | `0x228` |
| Last string offset | `0x47BDC` |
| Pool allocation size (header at pool-0x10) | `0x47C01` |

The pool is **shared by all names in the table**, not per-string.

### 2.3 Huffman tree

The first `0x228` bytes (552 bytes) are a breadth-first serialized binary
Huffman tree:

```text
node n: u16 left, u16 right        (4 bytes per node, root = node 0)
internal child: node index, high byte 0
leaf child:     UTF-8 byte << 8    (low byte 0)
```

There are `552 / 4 = 138` nodes. Node 0 is the root; its children are nodes 2
and 1. Leaf children encode ASCII letters, digits, punctuation, space, and the
UTF-8 lead/continuation bytes needed for names such as `Kanté`, `Mbappé`,
`Aćimović`, `Alarcón`, and `Mishchenko`.

Tree validation rules used by the Rust loader:

* tree size = the smallest non-sentinel string offset;
* tree size is at least 4, a multiple of 4, and at most 64 KiB;
* every internal child is in `1..node_count`;
* no cycles or shared subtrees;
* at least one leaf;
* every leaf byte is non-zero;
* every string offset is at least the tree size and inside the pool snapshot.

### 2.4 String entry

Each entry starts at `pool_base + offset`:

```text
u8  output_byte_count
... Huffman bitstream, MSB-first
```

The length is a **UTF-8 byte count**, not a Unicode scalar count. For `Kanté`
the entry length is 6 bytes; for `Aćimović` it is 10 bytes. Decoding stops after
exactly that many bytes. There is no NUL terminator and no padding between
entries.

The bit order is MSB-first: bit 7 of each byte is consumed first. A `0` bit
selects the node's left child, a `1` bit selects the right child. When the child
is a leaf (`child & 0xFF00 != 0`), its high byte is appended and traversal
restarts at the root.

Strict bounds:

* the bitstream must not read past the **next name's offset** (or 512 bytes for
  the last entry);
* the decoded byte count must equal the length prefix;
* the result must be valid UTF-8;
* NUL and control characters are rejected.

### 2.5 Example

The `Cristiano Ronaldo` common-name entry at offset `0xA152`:

```text
length = 0x11 (17 UTF-8 bytes)
bitstream = d8 32 a8 66 0f c1 20 0a d8 32 ...
output = "Cristiano Ronaldo"
```

Adjacent entries decode independently. The next-offset bound prevents a
corrupt length from consuming the following name.

## 3. `dcplayernames`

IDs **46000 and above** are not in `playernames`. They live in
`dcplayernames`, a 40-byte fixed-row table:

| Field | Offset/width | Meaning |
| --- | --- | --- |
| `name` | bit 0, 304 bits (38 bytes) | Inline UTF-8, NUL-terminated |
| `nameid` | bit 304, 13 bits | Stored key; actual ID = stored + 46000 |

Live capture: 2,279 rows, IDs 46000..48278. All 2,279 names are non-empty
UTF-8. There is no string pool for this table.

This table explains the 1,857 player name references that are above the
`playernames` range. Without it, those players would have unresolved surnames.

## 4. `editedplayernames`

`editedplayernames` overrides names by `playerid` in career mode. Each 184-byte
row contains four 45-byte inline strings plus the player key:

| Field | Offset/width |
| --- | --- |
| `firstname` | bit 0, 360 bits |
| `commonname` | bit 360, 360 bits |
| `playerjerseyname` | bit 720, 360 bits |
| `surname` | bit 1080, 360 bits |
| `playerid` | bit 1440, 19 bits, bias `-1` |

The live table has 8 rows for created players `460002..460009`. The tool applies
an override only when the player ID matches. If an edited field is empty, the
base `playernames`/`dcplayernames` value is used for that field. The
`playerjerseyname` is decoded but not used for the display name.

Examples:

| playerid | first | surname | display |
| ---: | --- | --- | --- |
| 460002 | Afonso | Monteiro | Afonso Monteiro |
| 460005 | Tyler | Bates | Tyler Bates |
| 460009 | Mykola | Mishchenko | Mykola Mishchenko |

## 5. Name-ID routing

For each player record, the three IDs are read from the verified player layout
(`firstnameid` at bit 0/16, `lastnameid` at bit 16/16, `commonnameid` at bit
48/16):

```text
id == 0                  -> no name (not an error)
id in 1..=43065          -> playernames (Huffman pool)
id in 46000..=48278      -> dcplayernames (inline)
id in 43066..=45999      -> unresolved (gap; none in the live capture)
other / missing          -> unresolved; null with an error
editedplayernames[id]    -> overrides first/surname/common when present
```

Display name:

```text
common_name, if non-empty
else first_name + " " + last_name
else whichever single component exists
else null
```

Names are associated with the row's `playerid`, not decoded as standalone
strings. A name ID is never substituted across players.

## 6. Evidence and validation

### 6.1 Structural validation

| Check | Result |
| --- | --- |
| `playernames` rows | 43,066 |
| Pool tree | 138 nodes, 552 bytes |
| Tree child references | all valid; no cycles/shared subtrees |
| Non-sentinel entries | 43,065 |
| Entries decoded | **43,065 / 43,065** (Rust `playernames_entries_decoded` check) |
| UTF-8 failures | 0 |
| Length/bounds failures | 0 |
| `dcplayernames` rows | 2,279 |
| `dcplayernames` UTF-8 failures | 0 |
| Player name references missing | **0** |
| Player records with names | **21,814 / 21,814** |

### 6.2 Independent player spot checks

The full Rust CLI pass (`--limit 22098`) produced:

| playerid | Public reference | display_name | first_name | last_name | common_name |
| ---: | --- | --- | --- | --- | --- |
| 20,801 | Cristiano Ronaldo | Cristiano Ronaldo | C. Ronaldo | dos Santos Aveiro | Cristiano Ronaldo |
| 158,023 | Lionel Messi | Lionel Messi | Lionel | Messi | - |
| 192,985 | Kevin De Bruyne | Kevin De Bruyne | Kevin | De Bruyne | - |
| 200,389 | Jan Oblak | Jan Oblak | Jan | Oblak | - |
| 202,126 | Harry Kane | Harry Kane | Harry | Kane | - |
| 203,376 | Virgil van Dijk | Virgil van Dijk | Virgil | van Dijk | - |
| 212,831 | Alisson | Alisson | Alisson | Ramses Becker | Alisson |
| 215,914 | N'Golo Kanté | N'Golo Kanté | N'Golo | Kanté | - |
| 231,747 | Kylian Mbappé | Kylian Mbappé | Kylian | Mbappé | - |
| 239,085 | Erling Haaland | Erling Haaland | Erling | Haaland | - |

These cover distinct first names, last names and common names, plus UTF-8
(`Kanté`, `Mbappé`, `Aćimović`, `Alarcón`). The same pass resolved all 8 edited
overrides with `name_source = editedplayernames`.

### 6.3 Static analysis

The DLL path was identified statically:

* `internal function` is the field getter. For live type `0xD` (13) it reads the
  32-bit reference from the record and calls `internal function`.
* `internal function` reads the string-pool handle's `+0x28` field as the pool
  base, reads the length byte at `base+offset`, and passes the remaining bytes
  to `internal function`. The same base is stored at the live table descriptor's
  `+0x38`.
* `internal function` performs the tree/map traversal and string construction.
* Live `playernames` confirms the `+0x38` pool pointer, the `+0x7C` row count,
  the length-prefixed entries, the 138-node tree and MSB-first decoding.

The format was **not** assumed from the `HufCompressed` symbol. The symbol
reference is in the row allocator (`internal function`); the actual decoding was
confirmed from the data, the field getter call path, and full-coverage
validation.

## 7. Rust implementation

`crates/fc27/src/name_pool.rs`:

| Item | Role |
| --- | --- |
| `PlayerNameResolver::load` | Loads and validates the three name tables |
| `HuffmanNameTable::load` | Reads `playernames`, the pool, the tree and offsets; validates/decode-checks every entry |
| `decode_huffman_string` | Bounds-checked MSB-first bitstream decode |
| `validate_tree` | Rejects cycles, invalid nodes, NUL leaves and empty trees |
| `DcNameTable::load_optional` | Loads 46000+ inline names |
| `EditedNameTable::load_optional` | Loads per-player overrides |
| `resolve_player` | Routes each player's three IDs and applies overrides |

The CLI fills `first_name`, `last_name`, `common_name`, `display_name`,
`name_source` and `name_error` on each emitted record. `--no-names` disables
resolution and leaves the fields null.

Failure behavior:

* `playernames` missing or structurally invalid -> command exits `3`
  (`insufficient_evidence`);
* a specific name ID is missing or its bitstream is corrupt -> that record's
  name is null with `name_error`, and the command still emits the verified
  numeric fields;
* no name is ever guessed, transliterated, or borrowed from another player.

## 8. Remaining blockers

1. **Team/league/competition names.** They use different tables and string
   fields; player names are the verified scope of this work package.
2. **Career-state check (WP6).** The tool still does not verify that a
   career/squad context is active.
3. **Authoritative metadata extraction.** The game's own XML is still inside
   CFS; the CLI requires a caller-supplied descriptor.
4. **Build gating.** The pool/tree/row layout was validated on one FC 27 build;
   other builds must be revalidated or gated on the known SHA-256.

## 9. Reproduction and evidence

```bash
cargo test --workspace
cargo build --release

./target/release/fc27le players <pid> \
  --metadata /path/to/fifa_ng_db-meta.xml \
  --limit 20 --json
```

| Evidence | File |
| --- | --- |
| Sanitized name coverage and validation summary | `docs/evidence/public_summary.json` |
| Name-table coverage and spot checks | `docs/evidence/public_summary.json` |
| Player storage/counts | `docs/evidence/public_summary.json` |
| Player storage research | `docs/research/player_storage.md` |
| Metadata research | `docs/research/db_metadata.md` |

The full name pool is not committed. The evidence files contain only counts,
tree metadata and a small public spot-check set.

## 10. Conclusion

Player names are resolved from the live database, not from an external roster.
The tool reads `playernames` via the verified 12-byte row layout, decodes the
shared 138-node Huffman tree with strict bounds and UTF-8 checks, resolves IDs
46000+ through `dcplayernames`, applies `editedplayernames` overrides, and ties
every name to the row's `playerid`. A full live pass resolved **21,814 / 21,814**
players with **zero** errors. Missing names remain null and are reported; no
name is fabricated.
