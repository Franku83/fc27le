-- Unit checks for the first-party player-field helpers.
--
-- Run directly with Lua 5.4:
--   lua5.4 scripts/lua/tests/player_fields_test.lua

local root = FC27LE_LUA_ROOT
if not root then
    assert(debug, "set FC27LE_LUA_ROOT or run with the standard Lua interpreter")
    local source = debug.getinfo(1, "S").source
    local directory = source:match("@(.*[/\\])")
    assert(directory, "cannot resolve the test directory")
    root = directory .. "../"
end
root = root:gsub("[/\\]$", "") .. "/"

local fields = assert(dofile(root .. "player_fields.lua"))

local function fail(message)
    error(message, 2)
end

local function eq(actual, expected, label)
    if actual ~= expected then
        fail(string.format("%s: expected %s, got %s", label,
            tostring(expected), tostring(actual)))
    end
end

local function truthy(value, label)
    if not value then
        fail(label .. ": expected a truthy value")
    end
end

local function falsy(value, label)
    if value then
        fail(label .. ": expected a falsy value")
    end
end

eq(#fields.FIELD_NAMES, 52, "verified field count")
eq(#fields.IDENTITY_FIELDS, 4, "identity fields")
eq(#fields.DATE_FIELDS, 5, "date fields")
eq(#fields.TRAIT_FIELDS, 5, "trait fields")
eq(#fields.FACE_STAT_FIELDS, 6, "face-stat fields")
eq(#fields.RAW_OUTFIELD_FIELDS, 27, "raw outfield fields")
eq(#fields.RAW_GOALKEEPER_FIELDS, 5, "raw goalkeeper fields")

local seen = {}
for _, name in ipairs(fields.FIELD_NAMES) do
    falsy(seen[name], "duplicate field " .. name)
    seen[name] = true
end
truthy(seen.playerid, "playerid is present")
truthy(seen.preferredfoot, "preferredfoot is present")
truthy(seen.trait2, "trait2 is present")
truthy(seen.isretiring, "isretiring is present")
truthy(seen.gkkicking, "gkkicking is present")

eq(fields.decode_preferred_foot(0), "Right", "right foot")
eq(fields.decode_preferred_foot(1), "Left", "left foot")
eq(fields.decode_preferred_foot(2), "Unknown", "unknown foot")
eq(fields.decode_preferred_foot(nil), "Unknown", "nil foot")

eq(fields.normalize_contract_year(28), 2028, "two-digit contract year")
eq(fields.normalize_contract_year(2028), 2028, "full contract year")
eq(fields.normalize_contract_year(0), nil, "zero contract year")
eq(fields.normalize_contract_year(nil), nil, "nil contract year")
eq(fields.normalize_contract_year(150), nil, "invalid contract year")

local epoch = fields.birthdate_to_date(141429)
eq(epoch.year, 1970, "epoch year")
eq(epoch.month, 1, "epoch month")
eq(epoch.day, 1, "epoch day")
eq(fields.compute_age(141429, 2027), 57, "year-difference age")
eq(fields.birthdate_to_date(0), nil, "zero birthdate")
eq(fields.compute_age(nil, 2027), nil, "nil birthdate")

eq(fields.decode_playstyles(1 | 8 | 2048, "outfield"),
    "Dead Ball;Finesse Shot;Tiki Taka", "outfield playstyles")
eq(fields.decode_playstyles(1 | 16, "goalkeeper"),
    "Far Reach;Far Throw", "goalkeeper playstyles")
eq(fields.decode_playstyles(0, "outfield"), "", "empty playstyles")
eq(fields.decode_playstyles(nil, "outfield"), nil, "unknown playstyles")
eq(fields.decode_playstyles(1), "Finesse Shot", "default outfield kind")

eq(fields.GOALKEEPER_FACE_STAT_LABELS.pacdiv, "DIV", "GK DIV label")
eq(fields.GOALKEEPER_FACE_STAT_LABELS.phypos, "POS", "GK POS label")

truthy(fields.is_generated(460000, 460000), "inclusive generated threshold")
falsy(fields.is_generated(459999, 460000), "below generated threshold")
falsy(fields.is_generated(nil, 460000), "nil generated playerid")

local mock = {
    GetRecordFieldValue = function(_, record, name)
        if record ~= 9 then error("unexpected record") end
        if name == "playerid" then return 1001 end
        if name == "broken" then error("read failed") end
        return 7
    end,
}
local value, ok = fields.read_field(mock, 9, "playerid")
eq(value, 1001, "read present field")
truthy(ok, "present field flag")
value, ok = fields.read_field(mock, 9, "broken")
eq(value, nil, "read failing field")
falsy(ok, "failing field flag")

local values, missing = fields.read_player(mock, 9)
eq(values.playerid, 1001, "read_player playerid")
eq(values.preferredfoot, 7, "read_player field value")
eq(#missing, 0, "read_player missing count")
eq(values.broken, nil, "read_player unknown field")

local partial = {
    GetRecordFieldValue = function(_, _, name)
        if name == "gkkicking" then error("unavailable") end
        return 3
    end,
}
local partial_values, partial_missing = fields.read_player(partial, 1)
eq(partial_values.playerid, 3, "partial playerid")
eq(#partial_missing, 1, "partial missing count")
eq(partial_missing[1], "gkkicking", "partial missing field")

print("player field unit tests: ok")
