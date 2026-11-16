# WP4 — Verified Player Record Storage and Enumeration

**Date:** 2026-11-16
**Target:** live `FC27.exe` SHA-256 `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`
**Metadata:** caller-supplied FC 27 descriptor, SHA-256 `<metadata hash>` (not committed)
**Mode:** read-only; no writes, hooks, DLL injection, game-function calls or save modifications
**Result:** `fc27le players <pid> --metadata FILE --limit 20 --json` enumerates 21,814 valid player records from the live `CZUM` table, with numeric fields and resolved names.

---

## 1. Result

`players` resolves to the live 4CC **`CZUM`**. The table is a contiguous, fixed-stride
array of 156-byte slots. The descriptor publishes both the slot high-water mark
and the invalid/deleted marker count:

| Property | Verified value (capture in `docs/evidence/public_summary.json`) |
| --- | --- |
| Table | `players` / `CZUM` |
| Descriptor | `0xE91FFFA8` (session-specific) |
| Data pointer | `0xEA3D41A8` (session-specific) |
| Record size | **156 bytes** |
| Field count | **152** |
| Slot count (`+0x7C`, u16) | **22,098** |
| Capacity (`+0x78`, u16) | **27,000** |
| Invalid/deleted slots (`+0x7E`, u16) | **284** |
| Valid slots | **21,814** (`22,098 - 284`) |
| First valid slot | **57** (`playerid 20801`) |
| Last valid slot | **22,079** at capture time |
| First unused all-zero slot | **22,098** |
| First slot of the next allocation | **27,000** |
| Playerid-index count (`QVtz +0x20`) | **21,814** |

The last two rows are independent boundary checks: the used range ends at
`data + 22,098 * 156`, the zero-filled tail ends at `data + 27,000 * 156`, and
the following structure begins exactly at the capacity boundary.

The table is live and dynamic. Counts can change between captures; the reader
therefore requires two identical snapshots before emitting anything.

## 2. Storage model

### 2.1 Descriptor fields

The table descriptor is reached from the verified table manager. The fields used
for enumeration are:

| Offset | Width | Meaning | Verified by |
| --- | --- | --- | --- |
| `+0x00` | 8 | Database node pointer | traversal; points to the `g_db` name node |
| `+0x08` | 8 | Next table descriptor | table-manager linked list |
| `+0x18` | 8 | Primary-index runtime pointer, or 0 | live CFS/index objects |
| `+0x30` | 8 | First record (data pointer) | Live Editor `table.lua`; `internal function` |
| `+0x40` | 4 | Table 4CC | WP3 metadata validation |
| `+0x44` | 4 | Record size in bytes | Live Editor `table.lua`; `internal function` |
| `+0x48` | 4 | `record_size*8 - 1` bit mask | WP1/WP3 (all 273 tables) |
| `+0x4C` | 4 | Flags (6 for `CZUM`) | WP3 |
| `+0x78` | 2 | Capacity (allocated slots) | boundary check; all 273 tables |
| `+0x7A` | 2 | Capacity duplicate (equal for `CZUM`) | observation only |
| `+0x7C` | 2 | Slot count / high-water mark | Live Editor `table.lua`; `internal function` |
| `+0x7E` | 2 | Invalid/deleted marker count | cross-table invariant (below) |
| `+0x82` | 1 | Field count (152) | WP3 |
| `+0x84` | `16 * count` | Field entries: `(bit_offset, type, bit_width, 4CC)` | WP3 |

The record is **not** zero-terminated or length-prefixed. All slots `0..slot_count-1`
have the same 156-byte stride, including invalid slots.

### 2.2 Valid / invalid slots

The final byte of each slot is a marker:

```text
slot = data + index * record_size
valid  iff (slot[record_size - 1] & 0x80) == 0
invalid otherwise
```

The marker sits in the four padding bits after the 1,244 field bits
(1,248 total bits per record), so it does not overlap any metadata field.

This is exactly the predicate in Live Editor's Lua reader and in the static
disassembly:

```text
internal function:
  rdi = [table + 0x30]              ; data
  r14 = (int)[table + 0x44]         ; record size
  ebp = (u16)[table + 0x7C]         ; slot count
  end = data + (count - 1) * record_size
  loop:
    if (record[record_size - 1] & 0x80) == 0: append(record)
    record += record_size
    while record <= end
```

The invalid slots are **not** empty. In `CZUM` they contain complete-looking
player payloads; zero checks, name checks or monotonic-ID checks would silently
accept them. The marker is the only verified discriminator.

### 2.3 Pointer relationships

```text
table-manager root
  -> database node (g_db)
       +0x10 -> first table descriptor
       +0x18 -> next database node
  -> table descriptor CZUM (0xE91FFFA8)
       +0x00 -> g_db node
       +0x08 -> next table descriptor
       +0x18 -> index runtime (0xE8FF2070)
       +0x30 -> record array (0xEA3D41A8)
       +0x78 -> capacity (27,000)
       +0x7C -> slots (22,098)
       +0x7E -> invalid slots (284)
       +0x84 -> 152 field entries

index runtime QVtz (0xE8FF2070)
  +0x00 -> table descriptor (back-pointer)
  +0x1C -> index 4CC "QVtz"
  +0x20 -> index count (21,814) == validity-scan count
  +0x24 -> comparator function (not called)
  +0x30/+0x40/+0x48/+0x50 -> index tree pointers (not dereferenced)
  +0x58/+0x5C/+0x60/+0x64 -> index sizing/state (not used for enumeration)
  +0x68 -> flags 0x101
  +0x70 -> table descriptor (back-pointer)
```

All pointers used by the reader are validated through the production bounded
reader. The index tree pointers and comparator are documented but deliberately
**not** dereferenced or called: they are not needed to enumerate the contiguous
slot array, and following them would add unverified structure.

## 3. Evidence

### 3.1 Documented table/field behavior

The documented Live Editor Lua API behavior is described here without copying
its implementation:

* the first record is at descriptor `+0x30`;
* record size is at `+0x44`;
* the written-record count is a `u16` at `+0x7C`;
* a record is invalid when the high bit of its last byte is set;
* a field is an LSB-first bit slice at the field entry's offset, and the
  metadata low bound is added to the stored value.

FC27LE independently implements these rules in Rust and validates them against
live data; no Lua implementation is vendored.
### 3.2 Independent static analysis

Independent analysis of the documented slot walk and bit-field access confirms
the same iteration bound and invalid-marker predicate. No third-party code or
decompiler pseudocode is copied or distributed. The live cross-checks below are
the authoritative validation for this project.

### 3.3 Cross-table live validation

The reader's model was checked against **every non-empty table** in the live
273-descriptor manager, not only `players`:

| Check | Result |
| --- | --- |
| Non-empty tables scanned | **233** |
| Rows scanned | 370,095 valid + 948 invalid |
| `slot_count <= capacity` | **233 / 233** |
| `invalid_rows == u16 at +0x7E` | **233 / 233** |
| Tables with a live index runtime | **20** |
| `index +0x20 == valid_rows` | **20 / 20** |

For `CZUM` specifically, the same checks yield `22,098 <= 27,000`,
`invalid = 284`, `valid = 21,814`, and index count `21,814`. This makes the
count/validity interpretation independent of any single table or any one
player sample.

### 3.4 Why `+0x50` was not used

WP3 observed `+0x50 = 0` for `CZUM` and explicitly refused to treat it as a
count. That decision was correct: the real slot count is the u16 at `+0x7C`,
and the real capacity is the u16 at `+0x78`. `+0x50` is not used by the reader.

The extra `CZUM`-only pointers at `+0x20`, `+0x28` and `+0x58` (the latter
points at `FC27.exe` RVA `0x5E17060`) were inspected and are not part of the
row store. Disassembly of `0x5E17060` shows a database/field lookup helper, not
a row-count accessor. None of those fields is dereferenced by enumeration.

## 4. Row encoding

Fields are LSB-first bit slices inside a 156-byte record. For a field with live
descriptor bit offset `o` and width `w`:

```text
raw = sum(bit(o+i) << i for i in 0..w-1)
value = raw + metadata.rangelow
```

This matches the documented field-reader semantics (a 64-bit window shift
plus the metadata minimum). The eight decoded fields are:

| Field | Bit offset | Width | Metadata low | Metadata high | Decoded range in capture |
| --- | ---: | ---: | ---: | ---: | --- |
| `playerid` | 1040 | 19 | -1 | 500,000 | 27..280,791 |
| `firstnameid` | 0 | 16 | 0 | 53,500 | 0..48,278 |
| `lastnameid` | 16 | 16 | 0 | 53,500 | 0..48,278 |
| `commonnameid` | 48 | 16 | 0 | 53,500 | 0..48,278 |
| `overallrating` | 1106 | 7 | 1 | 99 | 47..93 |
| `potential` | 370 | 7 | 1 | 99 | 49..95 |
| `height` | 514 | 7 | 130 | 220 | 151..210 cm |
| `weight` | 655 | 7 | 30 | 150 | 45..105 kg |

`playerid` is biased by `-1`: slot 57 stores `20,802` and decodes to
**`20,801`**. This is not a guess; it is the metadata `rangelow` that Live
Editor adds, and it is what makes the famous-player spot checks line up.
Similarly, height stores `57` and decodes to `57 + 130 = 187` cm; weight stores
`55` and decodes to `55 + 30 = 85` kg.

Example decoded rows from the live capture:

| Slot | playerid | overall | potential | height | weight |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 57 | 20,801 | 84 | 94 | 187 | 85 |
| 159 | 69,957 | 70 | 70 | 176 | 70 |
| 160 | 69,963 | 68 | 73 | 182 | 78 |
| 161 | 69,965 | 65 | 69 | 170 | 69 |
| 162 | 69,967 | 75 | 77 | 184 | 70 |

Rows 0..56 are invalid markers and are skipped; that is why output starts at
slot 57 rather than slot 0.

## 5. Independent semantic validation

The biases are validated against widely known FC player identities and physical
data (public reference values, not game files):

| playerid | Public reference | Decoded height | Decoded weight | Position code |
| ---: | --- | ---: | ---: | ---: |
| 20,801 | Cristiano Ronaldo | 187 cm | 85 kg | 25 |
| 158,023 | Lionel Messi | 169 cm | 67 kg | 18 |
| 192,985 | Kevin De Bruyne | 181 cm | 75 kg | 18 |
| 200,389 | Jan Oblak | 188 cm | 87 kg | 0 (GK) |
| 202,126 | Harry Kane | 188 cm | 86 kg | 25 |
| 203,376 | Virgil van Dijk | 195 cm | 92 kg | 5 (CB) |
| 209,331 | Mohamed Salah | 175 cm | 72 kg | 12 |
| 212,831 | Alisson | 193 cm | 91 kg | 0 (GK) |
| 215,914 | N'Golo Kanté | 168 cm | 70 kg | 10 |
| 231,747 | Kylian Mbappé | 182 cm | 81 kg | 25 |
| 239,085 | Erling Haaland | 195 cm | 94 kg | 25 |

Goalkeepers decode with position code `0`; center backs with `5`; strikers with
`25`; Kanté with `10`. The physical values are close to or exactly the public
values (FC's own database sometimes differs by a centimetre or a kilogram).
This validates the field identities, the biases, the bit order and the record
boundaries together. WP5 subsequently resolves the three name IDs through the
verified name tables; see [`name_storage.md`](name_storage.md). The reference
labels in this table are used only for validation.

Aggregate checks on 21,814 valid rows: `playerid` is unique and positive;
overall/potential are inside `1..=99`; height is inside `130..=220` cm; weight is
inside `30..=150` kg. Invalid slots are excluded before any of these checks.

## 6. Rust implementation

`crates/fc27/src/player_store.rs`:

| Function | Role |
| --- | --- |
| `resolve_layout` | Resolves `players` by metadata 4CC, validates field count, every field width, row fit, padding, count/capacity, data pointer and both ends of the range |
| `read_range` | Chunked read using `MAX_SINGLE_READ` (1 MiB) |
| `scan_players` | Reads the verified range twice, requires byte-identical snapshots, then scans |
| `scan_snapshot` | Pure scanner: skips invalid markers, checks counts, decodes selected rows |
| `decode_record` | Decodes and range-checks the eight fields (three name IDs + five numeric fields) |
| `read_bits` | Bounds-checked LSB-first bit extraction |

Gates (fail closed with `InsufficientEvidence`):

* metadata has no `players` table, or the live manager has no matching 4CC;
* field count or any field width differs from metadata;
* a field crosses the record boundary, or padding is `>= 8` bits;
* `slot_count > capacity`, `capacity == 0`, or scan size exceeds 64 MiB;
* the data pointer is null or either end of the range is unreadable;
* two snapshots differ;
* `invalid_rows != +0x7E`;
* the playerid-index count (when present) differs from the validity scan;
* any emitted field is outside its metadata range.

Tests are synthetic: a 12-byte test record encodes all eight fields with the
playerid/height/weight biases, a validity marker, an invalid-marker skip, a
limit truncation, a marker-count mismatch, an out-of-range height and a short
snapshot. No EA metadata is embedded in tests.

## 7. CLI

```bash
fc27le players <pid> --metadata /path/to/fifa_ng_db-meta.xml --limit 20 --json
```

The command:

1. verifies the database-service path (same gates as `fc27le db`);
2. parses the caller-supplied metadata and validates all 268 mappings against
   the live manager;
3. resolves `players = CZUM` and validates the 152 field widths;
4. scans all 22,098 slots, skips 284 invalid markers, and emits up to `--limit`
   valid records;
5. returns exit `0` only when every check passes, exit `3` when evidence is
   insufficient, and exit `2`/`4` for input/unsupported-build errors.

The JSON output contains the descriptor, data pointer, count/capacity/invalid
counts, field layout (offset, width, low, high), both snapshot status, every
check, the emitted records, and a name-resolution report. Names are decoded
from the verified name tables; no metadata XML content is included.

Without `--metadata`, `fc27le players <pid> --explain` prints the remaining
evidence gates and exits `3`.

## 8. Remaining blockers

1. **Authoritative metadata extraction.** The caller must still supply a
   matching FC 27 descriptor. The game's own `data/db/fifa_ng_db-meta.xml` is
   inside encrypted/compressed CFS (`contentsb.toc`, offset `0x7E0B8A`, size
   `0x158CB2` in the tested build). The CFS codec/key is unresolved.
2. **Team/league/competition names.** Player names are resolved (WP5), but the
   other name tables and string fields are not yet exposed.
3. **Career-state check.** The tool does not yet verify that a career/squad
   context is active; a menu-mode or partially loaded database could expose
   stale rows.
4. **Build gating.** The storage model was validated on one FC 27 build. Other
   builds must be revalidated or gated on the known `FC27.exe` SHA-256.
5. **Static-analysis import limit.** Importing the proprietary editor binary exceeded its
   startup deadline; direct Capstone analysis and Live Editor's Lua source were
   used instead. This is a tooling limitation, not an evidence gap for the
   enumerated model.

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
| Sanitized player/name counts and validation summary | `docs/evidence/public_summary.json` |
| Cross-table invariants, boundaries, spot checks | `docs/evidence/public_summary.json` |
| Metadata parser/validation | `docs/evidence/public_summary.json` |
| DB service path | `docs/evidence/public_summary.json` |
| WP3 metadata research | `docs/research/db_metadata.md` |

All live addresses are session-specific. The stable facts are the descriptor
offsets, the 4CC `CZUM`, the 156-byte record size, the 152-field layout, and the
count/validity semantics. The data pointer must be re-read on every run.

## 10. Conclusion

The `players` table is now safely enumerable read-only. The reader does not
guess a count or scan the heap: it resolves the descriptor through the verified
manager, uses the game's own u16 slot/capacity/invalid counters, skips rows by
the exact marker used by Live Editor and the game, requires two identical
snapshots, and cross-checks the validity count against both `+0x7E` and the
playerid-index count. The five numeric fields and three name IDs are validated
against metadata ranges and independent player identities. WP5 adds verified name resolution
through `playernames`, `dcplayernames` and `editedplayernames`; see
[`name_storage.md`](name_storage.md).
