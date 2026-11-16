# WP3 — Database Metadata and 4CC Resolution

> The shipped compact profile is documented in
> [`docs/embedded_profile.md`](../embedded_profile.md); this research note covers
> the external descriptor and live validation.

**Date:** 2026-11-16
**Targets:** live `FC27.exe` (SHA-256 `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`), metadata descriptor `data/db/fifa_ng_db-meta.xml`
**Mode:** read-only; no game, prefix, save or Live Editor binary changes
**Result:** the `players` table descriptor is **resolved and verified** as `CZUM` using metadata plus live descriptor checks.

---

## 1. Result

The runtime `players` table is:

| Property | Verified value |
| --- | --- |
| Metadata name | `players` |
| 4CC shortname | **`CZUM`** |
| Database | `g_db` |
| Live table address (tested session) | `0xE91FFFA8` |
| Field count | **152** |
| Record size | **156 bytes** (`+0x44`) |
| Bit mask | **1247** (`+0x48 = 156*8 - 1`) |
| Sum of metadata field depths | **1244 bits** |
| Row padding | **4 bits** |
| Data pointer | non-zero (`0xEA3D41A8` in the tested session) |
| Field-width validation | **152 / 152 match** |
| Status | **verified** |

The related `teamplayerlinks` table resolves to a 16-field, 16-byte table;
`teams` and `leagues` also validate against the metadata.

The metadata file is **not bundled** in this repository. The CLI accepts a
caller-supplied descriptor:

```bash
fc27le db <pid> --metadata /path/to/fifa_ng_db-meta.xml
```

The game's own copy is still inside encrypted/compressed CFS. That extraction
dependency is documented in §7; it does not block descriptor resolution when a
matching descriptor is supplied.

## 2. Metadata discovery

### 2.1 Declared source

`internal function` loads `data/db/fifa_ng_db-meta.xml` through Live Editor's CFS
manager and passes it to `internal function` (`ParseDBMetaXML`). The hash list maps
the path to:

```text
1160535545681411833;data/db/fifa_ng_db-meta.xml
```

### 2.2 Live CFS index

The game's in-memory CFS index contains the metadata entry. In the tested
session (PID 5714) it was at `0x312AF2B0`:

| Field | Observed |
| --- | --- |
| Hash | `0x101B0CEE158DFEF9` |
| Start | `0x004CA48F` |
| End | `0x004E7DEA` |
| Offset | `0x007E0B8A` |
| Size | `0x00158CB2` (1,412,274 bytes) |
| Chunk index | `1` |

The entry matches the structure decoded from Live Editor's
`internal function` (`LoadFileByHash`): 8-byte hash, five 32-bit fields and a
32-bit chunk index, stride `0x1C`.

The only `.toc` file large enough for the offset is
`Data/Win32/contentsb.toc` (23,025,479 bytes). Its bytes at offset `0x7E0B8A`
are **not** plaintext XML and do not start with zlib/gzip/LZ4/ZSTD markers. The
first bytes are:

```text
00 01 15 60 25 55 00 00 85 7A 00 00 A3 A0 0D E7 ...
```

A 2 GiB scan of readable process memory found **no raw `<database` or
`<table name=` document**. The game retains parsed XML fragments in the
`0xE90B...` heap region, but they are allocator-fragmented and not a
reconstructible document. Therefore the authoritative copy is the CFS entry,
and its codec/key is still unresolved.

### 2.3 Reference descriptor used for validation

A caller-supplied FC 27 descriptor was used outside the repository to validate
the mapping against the live process:

| Property | Value |
| --- | --- |
| Reference path | caller-supplied; not in the repository |
| Size and SHA-256 | not published |
| Validation | 268/268 tables, field counts and widths checked against live descriptors |

The file is **not committed or redistributed**. The validation is reproducible
with any matching descriptor. Do not obtain or share the file in violation of
the game's terms or applicable law.

## 3. Metadata format

The descriptor is XML:

```xml
<database name="<name>" shortname="<4cc>" version="<n>">
  <table name="<name>" shortname="<4cc>" savegroups="<groups>">
    <fields>
      <field name="<name>" shortname="<4cc>"
             type="DBOFIELDTYPE_INTEGER" depth="<bits>"
             rangehigh="<n>" rangelow="<n>" null="False" key="True"/>
      ...
    </fields>
  </table>
  ...
</database>
```

| Element / attribute | Meaning |
| --- | --- |
| `database@name`, `@shortname` | database name and 4CC |
| `table@name`, `@shortname` | human table name and runtime 4CC |
| `table@savegroups` | comma-separated save groups (informational) |
| `field@name`, `@shortname` | human field name and runtime 4CC |
| `field@type` | `DBOFIELDTYPE_INTEGER`, `DATE`, `STRING`, ... |
| `field@depth` | bit width for integer/date fields; string capacity for string fields |
| `field@rangehigh`, `@rangelow` | declared value range |
| `field@null`, `@key` | nullability and key flag |
| `field@fkparenttable` | parent table for foreign keys |

The descriptor contains **268 table elements**. Every 4CC is the exact
four-character code stored at runtime: table 4CC at `table+0x40`, field 4CC in
the low 32 bits of the second qword of the 16-byte field entry.

## 4. Rust parser

`crates/fc27/src/db_metadata.rs` implements:

```rust
parse_metadata_xml(xml: &str) -> Result<Metadata>
Metadata::table_by_name / table_by_shortname
validate(metadata, live_tables, process_handle, focus_table)
compare_table(metadata_table, live_table, live_fields) // pure, tested
```

Design rules:

* required attributes are validated; a shortname must be exactly four ASCII
  graphics;
* numeric depth/range fields are parsed and bounds-checked;
* unknown XML elements are ignored;
* a focus table is only `verified` when field count, every field width, the
  row fit, and a non-zero data pointer all agree;
* the parser never executes or writes anything.

The metadata file is caller-supplied. The CLI reads at most 16 MiB, computes its
SHA-256, parses it, and validates it against the live manager. It never stores
the XML.

Tests use a **synthetic** descriptor (`test_db`, `PLAY`, invented fields), not
EA metadata.

## 5. Live validation

Command:

```bash
./target/release/fc27le db 5714 \
  --metadata /path/to/fifa_ng_db-meta.xml \
  --focus-table players
```

| Check | Result |
| --- | --- |
| Metadata tables | 268 |
| Live table descriptors | 273 |
| Metadata tables matched by 4CC | **268 / 268** |
| Field-count matches | **268 / 268** |
| Field-width sets matched by 4CC | **268 / 268** |
| Tables satisfying fixed-row fit | 263 / 268 |
| `players` field widths | **152 / 152** |
| `players` descriptor | **CZUM, verified** |

The five non-fixed-row tables are variable-string/commentary tables:
`commentarynames`, `trainingteamplayernames`, `ssfstadiummusic`, `audionation` and
`playernames`. Their
XML string `depth` is a string capacity, not a fixed row width, so the
fixed-row sum check is expected to exceed the row size. Their field counts and
field 4CC sets still match.

### 5.1 Players layout

| Check | Value |
| --- | --- |
| Metadata fields | 152 |
| Live field entries | 152 |
| Width matches | 152 |
| Sum of depths | 1244 bits |
| Record size | 156 bytes (1248 bits) |
| Padding | 4 bits |
| Mask invariant | `1247 == 156*8 - 1` |
| Data pointer | non-zero |

The live field entries are not stored in XML order; they are matched by 4CC.
A bounded LSB-first bit reader over the first records produced plausible player
IDs (monotonically increasing in the sampled rows), overall/potential values in
the expected 0..99 range, and plausible height/weight values. WP4 subsequently
resolved the storage model and implemented verified numeric enumeration; see
`docs/research/player_storage.md`. No XML content is committed.

### 5.2 `players` ↔ `teamplayerlinks`

The metadata defines:

* `players` = `CZUM`, 152 fields, key `playerid`;
* `teamplayerlinks`, fields including `teamid` and `playerid`;
* `teams`, key `teamid`;
* `playernames`, key `nameid`, string field `name`.

The SQL/string relationship (`players.playerid = teamplayerlinks.playerid`,
`teamplayerlinks.teamid = teams.teamid`) is therefore backed by the metadata
and by the live descriptors for all four tables. It is no longer only a static
SQL-string inference.

## 6. CLI behavior

`fc27le db` without metadata still verifies the DB service path and lists the
273 descriptors.

With `--metadata FILE`:

* parses the XML;
* validates all metadata tables against live 4CC/field layouts;
* resolves the focus table (default `players`);
* reports descriptor address, field counts, record size, bit depth, padding and
  data pointer;
* exits `1` if the focus table fails validation, `0` on a verified focus;
* the separate `players` command is implemented for numeric enumeration (see
  `docs/research/player_storage.md`) and exits `3` only when its evidence gates
  fail.

Example output:

```text
metadata:        /path/to/fifa_ng_db-meta.xml
  size/sha256:   <metadata size> / <metadata hash>
  coverage:      268 table(s); 268/268 matched, 263/268 verified
  focus players:     CZUM at 0xe91fffa8
    fields:      152 metadata / 152 live; widths 152/152; row_size 156 bytes; depth 1244 bits; padding 4 bits
    data:        <data pointer>; status: VERIFIED
```

## 7. The remaining dependency: CFS metadata extraction

The game's own descriptor is still inside CFS. The exact unresolved dependency
is:

1. **CFS codec/key for `contentsb.toc`.** The entry at offset `0x7E0B8A`
   (`size 0x158CB2`) is not plaintext and is not zlib/gzip/LZ4/ZSTD by magic.
   The CFS entry size is only 55 bytes larger than the reference XML, which
   suggests a small container/header around an encrypted or custom-compressed
   payload. The codec/key has not been verified.
2. **Players row count/data extent — resolved in WP4.** The count is the u16 at
   `+0x7C` (22,098), capacity is the u16 at `+0x78` (27,000), and invalid slots
   are counted by the u16 at `+0x7E` (284). `+0x50` is not a count. See
   `docs/research/player_storage.md`.
3. **Authoritative metadata provisioning.** Until (1) is solved, the user must
   supply a metadata XML matching their build. The project does not bundle or
   redistribute it.

No 4CC was guessed: `players = CZUM` is validated by 268/268 table matches,
152/152 player field-width matches, and the live descriptor invariants.

## 8. Reproduction

```bash
cargo test -p fc27-game
cargo build --release

./target/release/fc27le db <pid> \
  --metadata /path/to/fifa_ng_db-meta.xml \
  --focus-table players --json
```

Expected: `metadata.focus.verified == true`, `shortname == "CZUM"`,
`metadata_field_count == 152`, `row_size == 156`, `padding_bits == 4`.

Evidence snapshot: `docs/evidence/public_summary.json` (summary only; no XML
content, no player records).

## 9. Limitations

* The reference XML used for validation was a user-supplied FC 27 descriptor, not the
  locally extracted game file. The mapping is validated against the live build,
  but authoritative local extraction is still pending.
* The 55-byte difference between the CFS entry and the reference XML is not yet
  explained; do not treat it as a verified container layout.
* Row count/enumeration bounds were resolved in WP4; player names were resolved in WP5.
* String fields in the five variable-string tables use capacity semantics, not
  fixed row widths. The `playernames` pool and name resolution are covered in
  `docs/research/name_storage.md`.
* The tested build is the tested FC 27 build; other distributions
  must be re-validated.
* All live addresses in this document are session-specific except static RVAs.

## 10. Evidence index

| Item | Evidence |
| --- | --- |
| CFS metadata entry | live index at `0x312AF2B0`, hash `0x101B0CEE158DFEF9`, offset `0x7E0B8A`, size `0x158CB2` |
| `ParseDBMetaXML` | `internal function`, local evidence `local evidence...` |
| Metadata loader | `internal function`, `local evidence...` |
| `LoadFileByHash` | `internal function`, `local evidence...` |
| Metadata parser tests | `crates/fc27/src/db_metadata.rs` |
| Live validation | `docs/evidence/public_summary.json` |
| DB interface | `docs/architecture.md` and `crates/fc27/src/db_service.rs` |

## 11. Conclusion

The runtime `players` descriptor is resolved by verified metadata: **`CZUM`**,
152 fields, 156-byte bit-packed records, 1247-bit mask, 4 bits of padding, and a
non-zero live data pointer. The field mapping is validated 152/152 against the
live descriptor, and the metadata-to-live mapping matches 268/268 tables.

The remaining blocker for a fully self-contained tool is **CFS extraction of
`data/db/fifa_ng_db-meta.xml`**; player names are resolved from the live name
pools (WP5), and team/league names remain separate work. The CLI supports a
caller-supplied descriptor and validates it against the live process, so the
verified mapping is usable today without distributing proprietary metadata.
