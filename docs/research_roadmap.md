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
| Structure resolver for players | **not started** | no verified player-table name map or record layout |
| Player listing | **gated** | `fc27le players --explain` exits 3 |

### WP1 result (2026-11-16)

WP1 is complete. `fc27le db <pid>` verifies the runtime path from Live Editor's
database pattern to the game table manager and enumerates **2 databases**
(`g_db`, `a_mx`) and **273 table descriptors**. The chain includes the global
object, vtable thunk, service registry key `0x0AE932D0`, manager root and table
headers; all pointers are read through the bounded reader and twelve semantic
checks pass live.

The remaining player-listing blocker is now precise: the full table/field names
live in `data/db/fifa_ng_db-meta.xml` (CFS hash `1160535545681411833`), which
Live Editor loads and parses into its own name→4CC maps. The game process keeps
the 4CC table manager but not that external name mapping. No player offsets are
claimed.

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

### WP3 — Recover the name→4CC map and locate the players descriptor

**Goal:** find the table object for `players`, not just the code that can query it.

The DB interface is already found. The missing key is the full-name → 4CC map
that Live Editor loads from `data/db/fifa_ng_db-meta.xml` (CFS hash
`1160535545681411833`). The game process stores 4CC table/field codes, not the
full-name map.

Tasks:

1. Implement or reuse a read-only CFS reader to extract
   `data/db/fifa_ng_db-meta.xml` (hash above) from the game archives. Never
   write to the archives.
2. Parse the XML schema already visible in `internal function`:
   `<database><table name="..." shortname="..." savegroups="...">`, a `<fields>`
   block, and `<field .../>` entries. Map `players` to its `shortname` 4CC and
   the player field names to their 4CCs.
3. Use `fc27le db` to locate the table descriptor whose `+0x40` matches that
   4CC. Record `+0x30` (data pointer), `+0x44` (record size bytes), `+0x48`
   (`record_size*8-1` bit mask), `+0x4c` (flags), `+0x82` (field count) and the
   16-byte field entries at `+0x84` (bit offset/type, size/4CC).
4. Cross-check the record count against an independent expectation (career save
   player count, or a known league/team filter) without modifying the save.
5. Candidate observation for later validation: 14 tables share an identical
   233-field, 236-byte, flag-2 schema (`luAI`, `eqqo`, `uxUO`, ...). This is a
   **candidate family, not confirmed players**; only the XML mapping can settle
   it.

**Acceptance checks:**

* the XML is obtained read-only and its digest/record is documented;
* `players` maps to exactly one 4CC (or a documented shard set);
* descriptor address is in a mapped, readable region;
* record size/bit mask and field count are internally consistent
  (`+0x48 == +0x44*8 - 1` holds for all 273 tables today);
* the table can be enumerated twice with the same result in one session;
* no writes occur.

**Stop rule:** if the XML or a stable name mapping cannot be recovered, stop and
document the missing link. Do not brute-force every heap pointer or guess a 4CC
from a hash.

### WP4 — Derive and validate the record layout

**Goal:** map a small, useful field set:

```text
playerid, firstnameid, lastnameid, commonnameid,
overallrating, potential, teamid (via teamplayerlinks),
preferredposition1, birthdate
```

Tasks:

1. Start from the descriptor evidence: field entries at `+0x84` give a bit
   offset and type in the first qword, and a bit width plus 4CC in the second
   qword. The row layout is bit-packed (`+0x48 == +0x44*8 - 1` is confirmed for
   all 273 tables).
2. Implement the bit-reader semantics observed in `internal function` (types 0, 3,
   4 and `0xD`), with explicit bounds checks for every field.
3. Use diffs between known records: two players with different overall/potential
   and known ids. A field is accepted only if it changes consistently.
4. Validate types and ranges:
   * `playerid` unique and positive;
   * `overallrating` and `potential` within the game's rating range and
     `potential >= overall` for most players;
   * `birthdate` decodes to a plausible date;
   * ids resolve through the same table/ID map used by the game.
5. Resolve names only through the verified string model (WP5).

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

* iterate at most a configured cap (for example 100,000 records);
* validate every pointer and field before output;
* never write; never call a game function; never install a hook;
* on any structural inconsistency, fail closed with `InsufficientEvidence`;
* include provenance in JSON: build hash, anchor ids, descriptor address, table
  count, validation summary.

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

Proceed with **WP3**, step 1: implement or reuse a read-only CFS extractor and
recover `data/db/fifa_ng_db-meta.xml` (hash `1160535545681411833`). It is static,
cheap, reversible and produces the name→4CC map needed for every later step.
Parse it, map `players` to its 4CC, then resolve the descriptor with `fc27le db`.
If the XML cannot be recovered or parsed, stop and report that finding rather
than guessing a 4CC or exploring the heap.
