# Research Roadmap — From Verified Process Access to Read-Only Player Data

## 1. Where the project stands

| Layer | Status | Evidence |
| --- | --- | --- |
| Process discovery | **done** | `docs/evidence/discover.json` |
| Bounded memory reader | **done** | `docs/evidence/selftest.json` |
| Module/PE/section resolution | **done** | `docs/evidence/probe.json` |
| Build identity | **done** | SHA-256 match in `docs/evidence/probe.json` |
| Verified anchor scan | **done** | event dispatcher at `0x141CD3B5C`; DB service at `0x147242D5E` |
| Database service / table manager | **done (read-only)** | `fc27le db`, `docs/research/db_interface.md` |
| Players table descriptor | **resolved/verified** | `players` = `CZUM`, 152 fields, `db --metadata` |
| Player record layout | **field widths verified** | 152/152 field widths; row count/extent pending |
| Player listing | **gated** | `fc27le players --explain` exits 3 |

### WP1 result (2026-11-16)

WP1 is complete. `fc27le db <pid>` verifies the runtime path from Live Editor's
database pattern to the game table manager and enumerates **2 databases**
(`g_db`, `a_mx`) and **273 table descriptors**. The chain includes the global
object, vtable thunk, service registry key `0x0AE932D0`, manager root and table
headers; all pointers are read through the bounded reader and twelve semantic
checks pass live.

The remaining player-listing dependency was narrowed to the metadata name map.
WP3 has now resolved that map for the tested build using a caller-supplied
descriptor; the game's own XML still needs CFS extraction. No player offsets are
claimed.

### WP3 result (2026-11-16)

A caller-supplied FC 27 metadata descriptor was parsed by the new Rust parser and
validated against the live table manager:

* **268 / 268** metadata tables matched live 4CC descriptors;
* **268 / 268** field-width sets matched;
* **`players` = `CZUM`**: 152 fields, 156-byte records, mask 1247, 1244 field
  bits + 4 padding bits, non-zero live data pointer;
* **`teamplayerlinks` = `RrqT`**: 16 fields, 16-byte records.

The descriptor is resolved. The CLI exposes this as
`fc27le db <pid> --metadata FILE`; the proprietary XML is not bundled.
Enumeration remains gated on the row count/data extent and on authoritative CFS
extraction. See `docs/research/db_metadata.md`.

The remaining work is *game-side structure discovery*, not process plumbing. This
roadmap is ordered by shortest path and by how cheaply a wrong hypothesis can be
rejected.

## 2. Evidence standard

Every step must produce a reproducible artifact before the next step starts.

| Label | Meaning |
| --- | --- |
| **Confirmed** | Observed directly in the game binary and live memory, with a reproducible command |
| **High** | Multiple independent observations agree; no contradicting evidence |
| **Medium** | Consistent but depends on an unobserved fact |
| **Unverified** | Plausible; must not be used for an implementation |

Acceptance rules:

* a pattern is not evidence until it has an **expected RVA** and a source;
* a match is not a structure until it has passed a **semantic cross-check**;
* a pointer is not followed until the destination is validated as mapped and
  readable;
* a field is not trusted until its value range and type are validated against
  independent data;
* anything that fails a check returns `InsufficientEvidence`, not a guess.

## 3. Known leads

These are strings and metadata, not offsets:

| Lead | Location | Meaning |
| --- | --- | --- |
| `SELECT playerid, overallrating FROM players ORDER BY overallrating DESC` | file `0xB20CB70` / VA `0x14B20CB70` | a `players` table exists in a SQL-like layer |
| `SELECT * FROM players;` | file `0xA999608` / VA `0x14A999608` | full-table query exists |
| `players.playerid`, `teamplayerlinks.playerid`, `teamplayerlinks.teamid` | file `0xA999518` region | relational schema names |
| `ContentDB LWSQL`, `queryCrowdsDb LWSQL`, `querySpaDb LWSQL` | first at file `0xA95CBD8`; multiple locations | the game's content DB layer is named `LWSQL` |
| 25 `players.*` field literals (`players.firstnameid`, `players.lastnameid`, `players.overallrating`, `players.potential`, ...) | string block at file `0xAB5AB28` / VA `0x14AB5AB28` | field names used by game code |
| PE sections `typeinfo` and `fieldinf` | section table | section names suggest reflection/RTTI metadata |
| `lua/scripts/list_players.lua` + `lua/DOC.MD` | previous project | API-level player access: `LE.db:GetTable("players")`, `GetRecordFieldValue(record, "playerid")` |
| DB service pattern | file `0x724235E` / RVA `0x7242D5E` / live VA `0x147242D5E` | unique anchor to the game table manager (WP1) |
| Service registry key `0x0AE932D0` | game service object | resolves to a manager with 2 databases and 273 table descriptors |
| Table header invariant | table `+0x44` / `+0x48` | `record_size*8-1` bit mask; confirmed for all 273 tables |
| `data/db/fifa_ng_db-meta.xml` | CFS hash `1160535545681411833` | full-name → 4CC map loaded by Live Editor (missing link) |
| `players` 4CC | metadata + live validation | **`CZUM`**, 152 fields, 156-byte records, mask 1247 |
| `teamplayerlinks` 4CC | metadata + live validation | **`RrqT`**, 16 fields, 16-byte records |
| `GetTeamIdFromPlayerId` / `GetPlayerName` | DLL `0x3DEE00` / `0x3DDC70` | career-object accessors (`club_team_id`), **not** the DB table manager; do not use as table evidence |

A field name in a string block does **not** give a field offset. SQL column order
does not imply memory order. The Live Editor Lua API is an interface on top of the
game, not a description of the game's storage.

## 4. Work packages

### WP1 — Map the `LWSQL` / ContentDB interface statically — **COMPLETE**

**Result:** the DB interface is a game-side table manager reached through a
service registry. Live Editor's `internal function` scans for a unique pattern, then
`internal function` calls the service method with key `0x0AE932D0`. The manager
contains two databases (`g_db`, `a_mx`) and 273 table descriptors. Live Editor's
`internal function` loads `data/db/fifa_ng_db-meta.xml` through CFS and parses it
with `internal function`; `internal function` applies that metadata to the game tables.

Evidence and the full function map are in
[`docs/research/db_interface.md`](research/db_interface.md). The implementation
is `fc27le db` (`crates/fc27/src/db_service.rs`).

**What remains from WP1:** nothing for the interface itself. The name→4CC
mapping is external XML metadata, which is WP3's first task.

**Stop rule (retained):** if the XML is not recoverable from CFS, stop before
guessing 4CCs or field offsets.

### WP2 — Turn one DB entry point into a verified anchor — **IMPLEMENTED**

**Result:** the `fc27.db-service-global` anchor is registered at expected RVA
`0x7242D5E` (unique in the tested file and live). `fc27le db <pid>` performs the
semantic verification: LEA/MOV agreement, global/vtable/thunk/method
containment, registry key lookup, manager root, expected database names and a
non-empty table list. All twelve checks pass on the tested build.

The anchor is read-only: no game function is called, and every pointer read is
validated by the bounded reader.

**Next acceptance step:** re-run `fc27le db` on a fresh game launch and on any
other distribution; a failure must be treated as an unsupported build.

### WP3 — Recover the name→4CC map and locate the players descriptor — **DESCRIPTOR RESOLVED**

**Result:** with a caller-supplied FC 27 metadata XML, `players` resolves to
**`CZUM`**. The live descriptor has 152 fields, 156-byte records, mask 1247,
1244 field bits plus 4 padding bits and a non-zero data pointer. All 152 field
widths match the metadata. `teamplayerlinks` resolves to `RrqT` (16 fields).
Across the full descriptor, 268/268 metadata tables matched live descriptors and
268/268 field-width sets matched.

`fc27le db <pid> --metadata FILE` implements the parser and validation. The
metadata XML is not bundled.

**Still open:**

1. **Authoritative CFS extraction** of `data/db/fifa_ng_db-meta.xml` (the
   `contentsb.toc` entry is not plaintext or standard zlib/gzip/LZ4/ZSTD; codec
   and key are unresolved).
2. **Players row count/data extent** (`+0x50` is zero; enumeration bounds are
   not verified).

**Acceptance achieved:** descriptor resolved by metadata, 4CC validated against
the live manager, record size/field count/data pointer checked, no guessed
names or offsets.

**Stop rule:** do not enumerate records until the row count/data extent is
verified. Do not bundle or redistribute the proprietary XML.

### WP4 — Derive and validate the record layout

**Status:** the `players` descriptor (`CZUM`) is resolved and the bit layout is
verified: 152 fields, 1244 depth bits, 156-byte rows, 4 padding bits, with every
metadata field width matching its live field entry. The remaining WP4 task is
the **record count and data extent**, then field-value validation on sampled
rows.

Tasks:

1. Start from the descriptor evidence: field entries at `+0x84` give a bit
   offset and type in the first qword, and a bit width plus 4CC in the second
   qword. The row layout is bit-packed (`+0x48 == +0x44*8 - 1` is confirmed for
   all 273 tables).
2. Implement the bit-reader semantics observed in `internal function` (types 0, 3,
   4 and `0xD`), with explicit bounds checks for every field.
3. Establish the `CZUM` row count/data extent, then iterate only within that
   verified bound.
4. Use diffs between known records: two players with different overall/potential
   and known ids. A field is accepted only if it changes consistently.
5. Validate types and ranges:
   * `playerid` unique and positive;
   * `overallrating` and `potential` within the game's rating range and
     `potential >= overall` for most players;
   * `birthdate` decodes to a plausible date;
   * ids resolve through the same table/ID map used by the game.
6. Resolve names only through the verified string model (WP5).

**Deliverable:** a field table with offset, width, signedness, confidence and
validation evidence. No field is added without a cross-check.

**Stop rule:** if two candidate layouts both fit the observed values, do not pick
one; design a discriminating observation (a player edit in-game is **not**
permitted, so use naturally varying records or a save copy).

### WP5 — String model

**Goal:** resolve first/last/common names safely.

Tasks:

1. Determine whether names are inline, pointer-based, interned or resolved via
   `playernames` / `editedplayernames` tables.
2. For every pointer: validate mapping + readable range + maximum length before
   reading; reject non-UTF-8 or implausible bytes.
3. Cap string reads (for example 128 bytes) and record them as untrusted data.

**Stop rule:** if the model cannot be distinguished with static evidence, return
IDs only and document that names are unresolved.

### WP6 — Career-mode state check

**Goal:** avoid reporting stale/menu-mode data as a career save.

Tasks:

1. Locate the game-state object and the career-mode flag using the same anchor
   discipline as the event dispatcher.
2. Verify the flag changes when the user loads/exits a career (observation only;
   the user controls the game).
3. Gate the player command on this check; if unavailable, report the limitation.

**Stop rule:** if no reliable state signal exists, require an explicit
`--assume-career-loaded` flag with a prominent warning, or keep the command
disabled.

### WP7 — Read-only CLI player listing

**Goal:** the Phase 4 deliverable.

```text
fc27le players <pid> [--limit N] [--team ID] [--json]
```

Requirements:

* require a caller-supplied metadata XML (or an authoritative CFS extraction);
* resolve `players` to `CZUM` and validate 152/152 field widths before output;
* establish and use a verified `CZUM` row count/data extent;
* iterate at most a configured cap (for example 100,000 records);
* validate every field before output;
* never write; never call a game function; never install a hook;
* on any structural inconsistency, fail closed with `InsufficientEvidence`;
* include provenance in JSON: build hash, anchor ids, descriptor address, table
  count, metadata digest, validation summary.

**Definition of done:** on a disposable career save, the listing matches the
in-game squad/player data for a spot-checked sample, and every check above has a
reproducible artifact.

## 5. Parallel track — save-file research

If the in-memory DB proves too indirect, a separate, read-only track can study a
**copy** of a career save:

* container format, compression, encryption/checksums;
* table equivalents and field layout;
* cross-validation against the in-memory findings.

Rules: never modify the original save; work on a copy; document every transform.
This track is independent and must not be used to justify memory offsets.

## 6. Test strategy

| Test | Kind |
| --- | --- |
| Synthetic table buffer with known records | unit |
| Bad pointer / short mapping / non-UTF-8 name | negative unit |
| Descriptor count vs enumerated count | live read-only |
| Two reads of the same table in one session | live repeatability |
| Field diff between two known players | live semantic |
| Career-mode off/on (user-driven) | live state |
| Restart with same build, re-verify anchors | reproducibility |
| Different build/distribution | compatibility; expect re-research |

No fuzzing against the live process. No writes, no function calls, no hooks.

## 7. Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Game title update changes code/data | anchors break | fail closed; re-verify anchors; update build table |
| Distribution differs (other distribution vs third-party build) | layout may differ | record hashes; never generalize without verification |
| ASLR / relocation | VA math breaks | always use mapped base, never assume preferred base |
| Target not traceable | reads denied | report permission error; no privilege escalation |
| Indirect/virtual DB access | static anchor hard | use string xrefs + vtable analysis; do not guess |
| False-positive pattern | wrong structure | require semantic cross-checks, not only a match |
| Anti-cheat / online | account risk | offline only; never interact with online services |
| Untrusted memory strings | misleading output | cap, validate, treat as data, never execute |

## 8. Safety policy for all future work

* Read-only. No `WriteProcessMemory`, `process_vm_writev`, `mprotect`, `ptrace`
  write, injection, detour or patch.
* No calling game functions to "see what happens".
* No scanning unrestricted anonymous memory. Scans must be bounded to a module's
  PE sections or to a small, justified structure.
* No changes to saves, prefixes, Proton settings or game files.
* No online play with the tool attached.
* Every new capability needs a written evidence record and a stop rule.

## 9. Recommended next action

Proceed with the two remaining WP3/WP4 dependencies:

1. **CFS extraction:** recover `data/db/fifa_ng_db-meta.xml` from
   `contentsb.toc` (entry at `0x7E0B8A`, size `0x158CB2`) by identifying its
   codec/key. Do not bundle the XML.
2. **Row extent:** locate the `CZUM` row count/data extent, validate record
   boundaries, and only then enable bounded enumeration.

`players = CZUM` is already resolved and validated; do not re-guess 4CCs or
field widths. If either dependency cannot be verified, stop and report it.
