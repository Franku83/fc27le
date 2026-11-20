# Lua player-field helpers

FC27LE exposes 52 independently verified FC 27 player fields through the
read-only Lua database API. The first-party helpers in
`scripts/lua/player_fields.lua` decode the fields that need interpretation:
preferred foot, contract year, birthdate/age and outfield/goalkeeper
PlayStyles.

FC27LE does **not** bundle a scouting script. The AgentX V5 PreferredFoot
script is user-supplied third-party material and is not redistributed. Users may
run their own compatible scripts where the API and field coverage allow it.

## Including the helpers

When a script is launched from the GUI, its directory is on the Lua module path.
The helper can be loaded normally:

```lua
local fields = require("player_fields")
local csv = require("csv")
```

The verified field list is available as `fields.FIELD_NAMES` (52 entries).
Grouped lists are also exposed:

* `fields.IDENTITY_FIELDS`
* `fields.DATE_FIELDS`
* `fields.TRAIT_FIELDS`
* `fields.FACE_STAT_FIELDS`
* `fields.RAW_OUTFIELD_FIELDS`
* `fields.RAW_GOALKEEPER_FIELDS`

## Reading a player

```lua
local players = assert(LE.db:GetTable("players"))
local record = players:GetFirstRecord()
while record > 0 do
    local values, missing = fields.read_player(players, record)
    if #missing > 0 then
        -- The caller decides whether to skip the row or render `unknown`.
        LOGGER:LogWarning("missing fields: " .. table.concat(missing, ","))
    end

    local foot = fields.decode_preferred_foot(values.preferredfoot)
    local contract = fields.normalize_contract_year(values.contractvaliduntil)
    local age = fields.compute_age(values.birthdate, GetCurrentDate().year)

    local kind = values.preferredposition1 == 0 and "goalkeeper" or "outfield"
    local trait = kind == "goalkeeper" and values.trait2 or values.trait1
    local trait_plus = kind == "goalkeeper" and values.icontrait2 or values.icontrait1
    local playstyles = fields.decode_playstyles(trait, kind)
    local playstyles_plus = fields.decode_playstyles(trait_plus, kind)

    LOGGER:LogInfo(string.format(
        "%s foot=%s contract=%s age=%s playstyles=%s",
        tostring(GetPlayerName(values.playerid)),
        tostring(foot),
        contract and tostring(contract) or "unknown",
        age and tostring(age) or "unknown",
        playstyles and playstyles ~= "" and playstyles or "unknown"
    ))

    record = players:GetNextValidRecord()
end
```

`read_player` returns a table keyed by field name and an array of fields that
could not be read. A failed field is `nil`; the helper never invents a value.
`decode_playstyles` returns an empty string for a present zero mask and `nil`
when the mask itself is unavailable.

## Face stats and goalkeepers

`pacdiv`, `shohan`, `paskic`, `driref`, `defspe` and `phypos` are
position-dependent. For an outfielder they are PAC/SHO/PAS/DRI/DEF/PHY. For a
goalkeeper they are DIV/HAN/KIC/REF/SPD/POS. The labels are available in
`fields.GOALKEEPER_FACE_STAT_LABELS`; always check `preferredposition1 == 0`
before interpreting the six values as outfield stats.

## Safe CSV output

`scripts/lua/csv.lua` is a first-party helper for user scripts:

* `csv.escape(value)` escapes commas, quotes and line breaks.
* `csv.row(values)` builds one newline-terminated row.
* `csv.unique_path(path)` appends `_2`, `_3`, ... instead of overwriting.
* `csv.write_atomic(path, lines)` writes a temporary file and renames it only
  after a complete successful write.

`FC27LE_EnsureDirectory(path)` is an FC27LE host extension that creates a
directory and its parents. It does not write game memory or files.

Example:

```lua
local export_dir = os.getenv("FC27LE_EXPORT_DIR")
if not export_dir or export_dir == "" then
    export_dir = (os.getenv("HOME") or ".") .. "/Documents/FC27 exports"
end
assert(FC27LE_EnsureDirectory(export_dir))

local path = csv.unique_path(export_dir .. "/player_fields.csv")
csv.write_atomic(path, {
    csv.row({ "Name", "PreferredFoot", "ContractUntil" }),
    csv.row({ "Example Player", "Right", "2028" }),
})
MessageBox("Export", path)
```

`FC27LE_EXPORT_DIR` is a convention used by the bundled example scripts; it is
not interpreted by the host. The host function only creates the directory that
the script passes to it.

## Unknown data and limits

* Missing birthdate: `compute_age` returns `nil`; do not substitute a fake age.
* Missing/invalid contract: `normalize_contract_year` returns `nil`.
* Unavailable PlayStyle mask: `decode_playstyles` returns `nil`.
* Unavailable field: `read_field` returns `(nil, false)`.
* Unknown team name: `GetTeamName` may return an empty string; render `unknown`.
* `OnLoan`: no verified `career_playercontract`/`contract_status` table was
  found in this FC 27 build. Keep it `unknown`, never `false`.
* `birthdate`-based age is a year difference, not birthday-aware.

## Running a user script

```bash
FC27LE_EXPORT_DIR="$HOME/Documents/FC27 exports" \
  ./target/release/fc27le-gui --script /path/to/your-script.lua --run-script
```

Third-party Lua scripts are untrusted. The runtime is read-only for game data,
but standard Lua file I/O remains available. Review a script before running it.

## Tests

```bash
lua5.4 scripts/lua/tests/player_fields_test.lua
cargo test -p fc27-lua-compat --test player_fields
```

The Rust integration test uses a synthetic `players` table and checks field
reading, decoding, unknown handling, directory creation, CSV escaping and
no-overwrite output. It does not require a third-party script or a live game.
