# Career season-statistics export

The supplied FC 26 `export_season_stats.lua` is kept unchanged. FC27LE provides
a Linux-compatible exporter with the same columns and statistics:

```text
scripts/lua/export_season_stats_linux.lua
scripts/lua/csv.lua
```

## Why a separate script

The original script uses `%USERPROFILE%\Desktop` and writes with `io.open(path,
"w+")`, which truncates an existing file. It also calls the proprietary
`imports/other/helpers` module. The Linux version:

- selects `FC27LE_EXPORT_DIR`, `XDG_DESKTOP_DIR`, `~/.config/user-dirs.dirs`,
  `~/Desktop`, or `$HOME` in that order;
- escapes commas, quotes and line breaks in CSV fields;
- preserves UTF-8 player and team names;
- adds a numeric suffix instead of overwriting an existing export;
- writes to a temporary file and renames only after a successful write;
- raises an explicit error if a field or player row is missing;
- reports how many duplicate player/competition rows were replaced (the
  original script also kept the later row).

## Run it

From the repository root:

```bash
FC27LE_EXPORT_DIR="$HOME/Documents/FC27 exports" \
  ./target/release/fc27le-gui \
  --script "$PWD/scripts/lua/export_season_stats_linux.lua" \
  --run-script
```

The output name is `SEASON_STATS_DD_MM_YYYY.csv` using the in-game career date.
Re-running the same day creates `..._2.csv`, `..._3.csv`, and so on.

## Columns

```text
position,playerid,playername,team,competition,appearances,AVG,MOTMs,goals,
assists,yellow_cards,two_yellow,red_cards,saves,goals_conceded,cleansheets
```

`AVG` keeps the original formula: `(avg / appearances) / 10`, or `avg / 10` for
one appearance, formatted with two decimals. The API's `avg` value is the raw
rating sum, not an already-divided rating.

## Data sources

All reads are bounded and read-only. No game function is called.

| Value | Source |
| --- | --- |
| Career active | Career core service (`0x1297F047`) object path: `+0x20 -> +0x10`, `+0x250 == 1`, `+0x248 -> +0x10 == 1`, and a non-null player at `+600`. |
| Date | Manager type 24 (calendar manager), day/month/year at `+0x34/+0x38/+0x3C`. |
| Stats | Manager type 1 interface (`0x0A613B9A`), then `+0x18 -> +0x10 -> +0x08 -> deref +0x00 -> +0x90`; begin/end are a vector of `0x5C` records. |
| Player name | Existing FC27LE `playernames` / `dcplayernames` / `editedplayernames` resolver. |
| Team name | Live `tWYe` table: career team id `mCXg`, display name `vIys`; 703 validated names. |
| Competition | Live `onMQ` league table plus the active-competitions manager (`C<id>` tags); nine domestic leagues resolve. |
| Competition override | Optional `id_map.json` with a `compobjids` object; it takes precedence. |

Statistics record layout (`0x5C` bytes):

```text
+0x00 u32 teamid
+0x04 u32 playerid
+0x08 u16 compobjid
+0x0C u16 rating sum ("avg")
+0x10 u32 goals (bits 0..8), two_yellow (bits 9..15)
+0x12 u8  yellow
+0x13 u8  red
+0x14 u8  appearances
+0x16 u8  assists
+0x17 u8  motm
+0x18 u8  saves
+0x19 u8  goals conceded
+0x1A u8  clean sheets
+0x1B u8  valid marker (non-zero)
```

A record is skipped when `playerid` is zero or `0xFFFFFFFF`, or the valid marker
is zero. The exporter additionally requires `appearances > 0`.

## Competition names and `id_map.json`

Nine domestic leagues resolve directly from the game's own `onMQ` table:

```text
818  England Premier League (1)
1016 Germany 1. Bundesliga (1)
1118 Italy Serie A (1)
1422 Spain Primera División (1)
919  France Ligue 1 (1)
1248 Portugal Primeira Liga (1)
1161 Holland Eredivisie (1)
1602 USA Major League Soccer (1)
1187 Norway Eliteserien (1)
```

Cups and other competitions use the documented `COBJ<id>` fallback. To supply
exact names, create an `id_map.json` and either:

- pass it with `FC27LE_ID_MAP=/path/to/id_map.json`; or
- place it next to the external metadata XML (the Lua session checks
  `<metadata directory>/id_map.json`).

Format:

```json
{
  "compobjids": {
    "511": "FA Cup",
    "627": "EFL Cup",
    "135": "UEFA Champions League"
  }
}
```

The map is loaded read-only. Invalid JSON, a missing `compobjids` object, a
non-numeric key, or an empty name is reported as an explicit
`GetCompetitionNameByObjID` error instead of silently falling back.

## Validation performed

Live test against the running career session:

- 22,939 valid statistics records; 13,653 exported rows after the original
  `appearances > 0` and player-table filters and 40 duplicate
  player/competition replacements.
- In-game date: 13/05/2027.
- 703 team names loaded and validated against Chelsea, Liverpool, Atlético de
  Madrid, Real Madrid and Getafe CF.
- A repeat run produced a byte-identical `_2.csv` and did not touch `_1.csv`.
- Spot checks: Cristian Cásseres Jr (Getafe) 18 appearances, 6.18 AVG, 3
  assists; Jan Oblak 35 appearances, 7.06 AVG, 32 saves, 36 conceded, 7 clean
  sheets; João Pedro (Chelsea) 22 appearances, 8.30 AVG, 33 goals, 18 assists;
  Cole Palmer (Chelsea) 16 appearances, 7.92 AVG, 23 goals, 22 assists.

The generated CSV contains private career data and is not committed or
published.

## Remaining limitations

- Cup competition names and 31 national/youth team IDs still use
  `COBJ<id>` / `TEAM<id>` unless an `id_map.json` is supplied.
- `GetTeamIdFromPlayerId` is derived from the player's season rows, not from
  the proprietary career-player object. A player with no season row returns 0.
- The exporter reflects the statistics vector at the moment the Lua session is
  created. Restart the session after a match if the game has advanced.
- The original script's Windows path and unescaped CSV writer are intentionally
  left untouched for reference.
