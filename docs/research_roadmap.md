# Research Roadmap — From Verified Process Access to Read-Only Player Data

## 1. Where the project stands

| Layer | Status | Evidence |
| --- | --- | --- |
| Process discovery | **done** | `docs/evidence/discover.json` |
| Bounded memory reader | **done** | `docs/evidence/selftest.json` |
| Module/PE/section resolution | **done** | `docs/evidence/probe.json` |
| Build identity | **done** | SHA-256 match in `docs/evidence/probe.json` |
| Verified anchor scan | **done** | event dispatcher at `0x141CD3B5C` |
| Structure resolver for players | **not started** | no verified game-side evidence |
| Player listing | **gated** | `fc27le players --explain` exits 3 |

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

A field name in a string block does **not** give a field offset. SQL column order
does not imply memory order. The Live Editor Lua API is an interface on top of the
game, not a description of the game's storage.

## 4. Work packages

### WP1 — Map the `LWSQL` / ContentDB interface statically

**Goal:** identify the functions and globals that resolve a table name to a table
descriptor.

Tasks:

1. Import `FC27.exe` into Ghidra/REA (or use targeted Capstone disassembly for the
   relevant string xrefs).
2. Find cross-references to:
   * the `players` SQL literal at `0x14B20CB70`;
   * `ContentDB LWSQL` and `query*Db LWSQL` strings;
   * the `players.*` field block at `0x14AB5AB28`;
   * `SELECT * FROM players;`.
3. Classify callers: query parser, table lookup, record iteration, field access.
4. Produce a list of candidate functions with RVA, call graph edges and a short
   rationale.

**Deliverable:** `docs/research/db_interface.md` with addresses, xrefs and quoted
pseudocode (no copied proprietary source).

**Stop rule:** if the SQL strings are only used by an offline tool and not by the
runtime DB, record that and pivot to the reflection metadata (`typeinfo` /
`fieldinf`) or the save-file track.

**Cost:** medium; this is static work and does not touch the live game.

### WP2 — Turn one DB entry point into a verified anchor

**Goal:** a byte pattern with an expected RVA that resolves the table registry or
table lookup in live memory.

Tasks:

1. Pick the smallest stable function from WP1 (prefer a leaf over a dispatcher).
2. Build a wildcard pattern from its prologue/body, avoiding relocations and
   build-specific immediates.
3. Add it to `fc27_game::anchors::ANCHORS` with source and expected RVA.
4. Run `fc27le probe <pid>` and require status `verified`.
5. Confirm that the pattern is unique in the scanned range and stable across a
   game restart (same build).

**Acceptance:** anchor verifies live, is unique, and has a written expectation.

**Stop rule:** if no stable anchor exists (e.g. only virtual calls), document the
indirect path and move to WP3 with a different strategy (vtable + string xref).

### WP3 — Locate the players table descriptor

**Goal:** find the table object for `players`, not just the code that can query it.

Tasks:

1. From WP1/WP2, identify the table registry (name → descriptor) or the lookup
   result structure.
2. Read the registry with a bounded, read-only scan; validate every pointer.
3. Identify the descriptor for `players` by name and/or by a stable id.
4. Record: descriptor address, record count, record stride (if any), data
   pointer(s), validity flags.
5. Cross-check the count against an independent expectation (career save player
   count, or a known league/team filter) without modifying the save.

**Acceptance checks:**

* descriptor address is in a mapped, readable region;
* count is plausible for FC 27 (thousands, not millions);
* every record pointer (if pointer-based) is mapped and readable;
* the table can be enumerated twice with the same count in one session;
* no writes occur.

**Stop rule:** if the registry cannot be found from static evidence, stop and
document the missing link. Do not brute-force every heap pointer.

### WP4 — Derive and validate the record layout

**Goal:** map a small, useful field set:

```text
playerid, firstnameid, lastnameid, commonnameid,
overallrating, potential, teamid (via teamplayerlinks),
preferredposition1, birthdate
```

Tasks:

1. Start from the descriptor/stride evidence, not from a guessed struct.
2. Use diffs between known records: two players with different overall/potential
   and known ids. A field is accepted only if it changes consistently.
3. Validate types and ranges:
   * `playerid` unique and positive;
   * `overallrating` and `potential` within the game's rating range and
     `potential >= overall` for most players;
   * `birthdate` decodes to a plausible date;
   * ids resolve through the same table/ID map used by the game.
4. Resolve names only through the verified string model (WP5).

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

Start with **WP1**, limited to the `players` / `LWSQL` string xrefs. It is static,
cheap, reversible and produces the input for every later step. If WP1 cannot
connect the strings to a runtime table path, stop and report that finding rather
than exploring the heap.
