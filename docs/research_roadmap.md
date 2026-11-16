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
| Player record layout | **verified** | 152/152 field widths; 22,098 slots; 27,000 capacity; 284 invalid markers |
| Player enumeration | **implemented read-only** | `fc27le players --metadata`; 21,814 valid numeric rows |
| Player/team names | **blocked** | compressed string pool unresolved |

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
WP4 subsequently resolved the record count/data extent and enabled verified
numeric enumeration. Names remain blocked on the compressed string pool. See
`docs/research/player_storage.md`.

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
2. **Player/team names** (the string pool is Huffman-compressed; see WP5).

The `CZUM` row count/data extent was resolved in WP4 (`+0x7C = 22,098`,
`+0x78 = 27,000`, `+0x7E = 284`); `+0x50` is not a count.

**Acceptance achieved:** descriptor resolved by metadata, 4CC validated against
the live manager, record size/field count/data pointer checked, no guessed
names or offsets.

**Stop rule:** do not bundle or redistribute the proprietary XML. Do not guess
names; emit player IDs until the string pool is decoded and validated.

### WP4 — Derive and validate the record layout — **DONE**

**Status (2026-11-16):** the `players` descriptor (`CZUM`) is resolved, the
bit layout is verified (152 fields, 1244 depth bits, 156-byte rows, 4 padding
bits), and the storage model is verified: slot count `u16 +0x7C`, capacity
`u16 +0x78`, invalid-marker count `u16 +0x7E`, and validity by the high bit of
the last record byte. Live capture: **22,098 slots / 27,000 capacity / 284
invalid / 21,814 valid**, with the playerid-index count also **21,814**. The
following five fields are decoded and emitted:
`playerid`, `overallrating`, `potential`, `height`, `weight`.

Tasks (completed):

1. Field entries at `+0x84` give bit offset, type, width and 4CC; the row layout
   is bit-packed (`+0x48 == +0x44*8 - 1` holds for all 273 tables).
2. The Rust reader implements bounds-checked LSB-first bit extraction and adds
   the metadata `rangelow` bias.
3. The slot count, capacity and invalid markers are read from the descriptor;
   the reader does not assume `+0x50` is a count.
4. Multiple decoded records were cross-checked against metadata ranges and
   independent player identities (Ronaldo, Oblak, Van Dijk, Kanté, ...).
5. Invalid slots are skipped by the marker, not by zero/name heuristics.

**Deliverable:** `docs/research/player_storage.md` plus
`crates/fc27/src/player_store.rs`. No field is emitted without a cross-check.

**Stop rule:** if two candidate layouts both fit, do not pick one; design a
discriminating observation (a player edit in-game is **not** permitted, so use
naturally varying records or a save copy).

### WP5 — String model

**Goal:** resolve first/last/common names safely.

**WP4 finding (2026-11-16):** `playernames` (`BGwe`) has 12-byte rows:
`u32 string-pool reference` at `+0x00`, `u16 nameid` at `+0x04`,
`u16 commentaryid` at `+0x06`, `u32 zero` at `+0x08`. The referenced pool is
Huffman-compressed (`HufCompressed`, `DBOFIELDTYPE_STRING_COMPRESSED_*` in the
DLL); no verified decoder exists yet. The player output therefore emits numeric
IDs, never names.

Tasks:

1. Determine whether names are inline, pointer-based, interned or resolved via
   `playernames` / `editedplayernames` tables.
2. For every pointer: validate mapping + readable range + maximum length before
   reading; reject non-UTF-8 or implausible bytes.
3. Cap string reads (for example 128 bytes) and record them as untrusted data.
4. Decode the Huffman pool and validate names against independent roster data
   before emitting them.

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

### WP7 — Read-only CLI player listing — **IMPLEMENTED (numeric subset)**

**Status (2026-11-16):** implemented as:

```text
fc27le players <pid> --metadata FILE [--limit N] [--json]
```

Requirements met:

* requires a caller-supplied metadata XML (the game's own copy still needs CFS
  extraction);
* resolves `players` to `CZUM` and validates 152/152 field widths before output;
* uses the verified slot count/capacity/invalid markers and skips invalid slots;
* reads the verified range twice and requires byte-identical snapshots;
* validates every emitted field against its metadata range;
* never writes, calls a game function or installs a hook;
* fails closed with `InsufficientEvidence` on any structural inconsistency;
* includes provenance in JSON: descriptor, data pointer, counts, field layout,
  metadata digest, validation checks and both snapshot statuses.

Not yet implemented: names/team IDs in the player output, team filters, and the
career-state check. `playerid` is the stable identifier until WP5 lands.

**Definition of done:** on a disposable career save, the listing matches the
in-game squad/player data for a spot-checked sample, and every check above has a
reproducible artifact. The numeric fields are spot-checked; the save/career
comparison remains open.

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

WP4 is complete. The next work item is **WP5: decode the compressed string
pool** so player and team names can be emitted (or explicitly remain IDs).

1. **String pool:** identify the base and Huffman tables for the
   `DBOFIELDTYPE_STRING_COMPRESSED_*` codec used by `playernames` (`BGwe`).
   Validate decoded names against independent roster data before emitting them.
2. **CFS metadata extraction:** recover `data/db/fifa_ng_db-meta.xml` from
   `contentsb.toc` (entry at `0x7E0B8A`, size `0x158CB2`) by identifying its
   codec/key. Do not bundle the XML.
3. **Career-state check (WP6):** locate a reliable career-mode signal and gate
   the player command on it.

`players = CZUM`, the 22,098/27,000/284 storage model and the five numeric fields
are already verified; do not re-guess them. If a dependency cannot be verified,
stop and report it.
