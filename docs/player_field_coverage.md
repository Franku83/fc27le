# Verified FC 27 player fields

This document records the player fields that FC27LE has independently verified
against the live FC 27 `players` table. The fields are available to
user-supplied Lua scripts through the read-only database API and to the
first-party helpers in `scripts/lua/player_fields.lua`.

FC27LE does **not** bundle a third-party scouting script. The AgentX V5
PreferredFoot script is user-supplied material and is not distributed with this
repository. No scoring, tag or scouting pipeline from that script is included.

## Environment and evidence

* FC 27 `FC27.exe`, SHA-256
  `d81a7a7465718fddcf1eeffde1ee0f952cab69b2c4771ad2aca29c381cd9d5e9`.
* Live `players` table: 4CC `CZUM`, 152 fields, 156-byte records, 22,098 slots.
* A private compatibility run against a live Career Mode save was used to
  validate the field mappings. Neither that run nor any generated CSV is
  published in this repository.
* A locally available FC 26 preset CSV was used only as an independent
  cross-check, not as an embedded schema and not as a source of FC 27 values.

## Verification method

1. Read the live table descriptors and record bytes directly.
2. Correlate a 4CC's values with known players and with the FC 26 column schema
   across players common to both editions. A field was accepted only when the
   correlation was unique and semantically plausible.
3. Check the mapping against live generated players (the summary face-stat
   fields are zero while raw attributes are populated) and against known
   position-dependent behaviour.
4. Re-read the fields through the FC27LE Lua table API and confirm the values
   match the live descriptors.

No `fifa_ng_db-meta.xml`, table dump, player dump or proprietary metadata file
is embedded in the repository.

## Player fields

Widths are bit widths in the live descriptor. `low`/`high` in the embedded
profile are safe decoding bounds, not claims about the game's semantic range.

| Field | Live field | 4CC | Width | Evidence |
| --- | --- | --- | ---: | --- |
| `playerid` | `playerid` | `ykFq` | 19 | Raw value is one greater than the public player id; the profile applies the `-1` bias. |
| `overallrating` | `overallrating` | `UERs` | 7 | Exact known-player values. |
| `potential` | `potential` | `mpuH` | 7 | Exact known-player values. |
| `preferredposition1` | `preferredposition1` | `wZQU` | 5 | Values 0-27 match the position names; 0 is GK. |
| `birthdate` | `birthdate` | `WVIU` | 20 | Gregorian-day values match known birthdays (Messi 147811, Ronaldo 146942, Mbappé 152008). |
| `gender` | `gender` | `EveZ` | 1 | 0 for male players, 1 for female players (Putellas and Graham Hansen are 1). |
| `nationality` | `nationality` | `enmm` | 12 | Live values match known country IDs (England 14, France 18, Argentina 52, Brazil 54, Spain 45). |
| `preferredfoot` | `preferredfoot` | `MDvm` | 1 | **FC 27 uses 0 = Right, 1 = Left.** Messi, Palmer and Putellas are 1; Ronaldo, Mbappé and João Pedro are 0. |
| `contractvaliduntil` | `contractvaliduntil` | `qvmK` | 7 | Two-digit year: 28 -> 2028, 29 -> 2029, 34 -> 2034. |
| `trait1` | `trait1` | `juxV` | 30 | Outfield PlayStyle bitmask; known-player bits match the FC 26 enum. |
| `icontrait1` | `icontrait1` | `OjQY` | 30 | Outfield PlayStyle+ bitmask. |
| `trait2` | `trait2` | `inAr` | 17 | Goalkeeper PlayStyle bitmask; bits 1-32 decode to the six GK PlayStyles. |
| `icontrait2` | `icontrait2` | `PqtG` | 17 | Goalkeeper PlayStyle+ bitmask. |
| `isretiring` | `isretiring` | `kvuF` | 1 | Semantic live check: 279 flags at the end of 2026/27 (264 male after the gender filter), concentrated in players aged 35-44, including Ronaldo, Modrić, Neuer, Suárez, Di María and Marta. |
| `pacdiv` | `pacdiv` | `aapy` | 7 | Outfield PAC; for GKs this field stores DIV. |
| `shohan` | `shohan` | `NTFr` | 7 | Outfield SHO; for GKs this field stores HAN. |
| `paskic` | `paskic` | `ceRf` | 7 | Outfield PAS; for GKs this field stores KIC. |
| `driref` | `driref` | `zNYP` | 7 | Outfield DRI; for GKs this field stores REF. |
| `defspe` | `defspe` | `aEqa` | 7 | Outfield DEF; for GKs this field stores SPD. |
| `phypos` | `phypos` | `hdMV` | 7 | Outfield PHY; for GKs this field stores POS. |

### Face stats for goalkeepers

FC stores the six face stats position-dependently. For a goalkeeper the same
six fields contain **DIV, HAN, KIC, REF, SPD, POS**, not outfield pace,
shooting, passing, dribbling, defending and physical.

`player_fields.GOALKEEPER_FACE_STAT_LABELS` exposes this mapping. Consumers
should check `preferredposition1 == 0` before treating the six fields as
outfield stats.

## Raw attributes

The following 7-bit raw attributes are verified and exposed. They are useful
for user-supplied scripts; FC27LE does not include a generated-player rating
formula.

| Attribute | 4CC | Attribute | 4CC |
| --- | --- | --- | --- |
| `sprintspeed` | `NrcP` | `acceleration` | `SPge` |
| `finishing` | `xJZL` | `longshots` | `CsBG` |
| `shotpower` | `ohpV` | `positioning` | `XsFD` |
| `penalties` | `AGsE` | `volleys` | `Dydz` |
| `shortpassing` | `vObb` | `vision` | `ZoOK` |
| `crossing` | `wGOH` | `longpassing` | `kerE` |
| `curve` | `YFaA` | `freekickaccuracy` | `VgKc` |
| `dribbling` | `nEbM` | `ballcontrol` | `MgwU` |
| `agility` | `RRQB` | `balance` | `onkY` |
| `defensiveawareness` | `SJKz` | `standingtackle` | `CsyD` |
| `interceptions` | `wWzG` | `headingaccuracy` | `aReg` |
| `slidingtackle` | `PhuM` | `strength` | `nmgT` |
| `stamina` | `XjDq` | `aggression` | `iTce` |
| `jumping` | `URGo` | `gkdiving` | `xrSG` |
| `gkhandling` | `GBGj` | `gkkicking` | `kqda` |
| `gkreflexes` | `eYFI` | `gkpositioning` | `yfhq` |

## PlayStyle bit maps

### Outfield (`trait1`, `icontrait1`)

| Bit | PlayStyle | Bit | PlayStyle |
| ---: | --- | ---: | --- |
| 1 | Finesse Shot | 2 | Chip Shot |
| 4 | Power Shot | 8 | Dead Ball |
| 16 | Precision Header | 32 | Acrobatic |
| 64 | Low Driven Shot | 128 | Gamechanger |
| 256 | Incisive Pass | 512 | Pinged Pass |
| 1024 | Long Ball Pass | 2048 | Tiki Taka |
| 4096 | Whipped Pass | 8192 | Inventive |
| 16384 | Jockey | 32768 | Block |
| 65536 | Intercept | 131072 | Anticipate |
| 262144 | Slide Tackle | 524288 | Aerial Fortress |
| 1048576 | Technical | 2097152 | Rapid |
| 4194304 | First Touch | 8388608 | Trickster |
| 16777216 | Press Proven | 33554432 | Quick Step |
| 67108864 | Relentless | 134217728 | Long Throw |
| 268435456 | Bruiser | 536870912 | Enforcer |

Labels are sorted alphabetically by the helper. Verified examples: Messi
`34605193` decodes to Dead Ball, Finesse Shot, Gamechanger, Quick Step,
Technical and Tiki Taka; Mbappé `6291681` decodes to Acrobatic, Finesse Shot,
First Touch, Gamechanger, Low Driven Shot and Rapid.

### Goalkeeper (`trait2`, `icontrait2`)

| Bit | PlayStyle |
| ---: | --- |
| 1 | Far Throw |
| 2 | Footwork |
| 4 | Cross Claimer |
| 8 | Rush Out |
| 16 | Far Reach |
| 32 | Deflector |

Bits above 32 are career/CPU flags and are not part of the six GK PlayStyles.
Verified examples: Courtois `17` decodes to Far Throw and Far Reach; Alisson
`2081` decodes to Deflector and Far Throw (the 2048 bit is a career flag);
Kobel `4129` decodes to Far Throw and Deflector.

## Age, contract and unknown values

* `birthdate` is a Gregorian day number. `player_fields.compute_age` returns the
  year difference (`current_year - birth_year`), not a birthday-aware age.
  Missing or zero birthdates return `nil`; no age is fabricated.
* Contract years are two-digit values normalized to full years. Zero, missing
  or out-of-range values become `nil`/`unknown`; they are never year 0.
* Unknown fields are returned as `nil` by the Lua helpers and must be rendered
  as `unknown` by the calling script. A present zero remains `0`.
* `OnLoan` is not exposed by FC27LE because the FC 27 build did not provide a
  verified `career_playercontract` table with `playerid` and `contract_status`.
  User scripts must keep it `unknown`, not `false`.
* Team names come from the live career/team directory. Unresolved team ids have
  no name and should be rendered as `unknown` by the calling script.

## Third-party scripts

The AgentX V5 PreferredFoot script is user-supplied third-party material. It is
not included in this repository, and FC27LE does not redistribute it. The
verified fields and first-party decoders above are available if a user chooses
to run a compatible script privately. Any such script remains the user's
responsibility and is subject to its own license.
