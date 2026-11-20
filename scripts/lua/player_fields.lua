-- First-party FC 27 player-field helpers for user-supplied Lua scripts.
--
-- This module is an independent implementation. It contains no third-party
-- scouting script, no scoring/tag pipeline and no copied helper source.
--
-- The field names and 4CC mappings are verified in
-- docs/player_field_coverage.md. Face stats are position-dependent: the same
-- six fields hold PAC/SHO/PAS/DRI/DEF/PHY for outfielders and DIV/HAN/KIC/REF/
-- SPD/POS for goalkeepers.

local M = {}

M.IDENTITY_FIELDS = {
    "playerid", "overallrating", "potential", "preferredposition1",
}
M.DATE_FIELDS = {
    "birthdate", "nationality", "gender", "preferredfoot", "contractvaliduntil",
}
M.TRAIT_FIELDS = {
    "trait1", "icontrait1", "trait2", "icontrait2", "isretiring",
}
M.FACE_STAT_FIELDS = {
    "pacdiv", "shohan", "paskic", "driref", "defspe", "phypos",
}
M.RAW_OUTFIELD_FIELDS = {
    "curve", "agility", "positioning", "standingtackle", "longpassing",
    "penalties", "longshots", "interceptions", "crossing", "finishing",
    "dribbling", "slidingtackle", "sprintspeed", "strength", "ballcontrol",
    "shotpower", "balance", "vision", "shortpassing", "freekickaccuracy",
    "aggression", "acceleration", "headingaccuracy", "jumping", "stamina",
    "defensiveawareness", "volleys",
}
M.RAW_GOALKEEPER_FIELDS = {
    "gkdiving", "gkhandling", "gkkicking", "gkreflexes", "gkpositioning",
}

M.FIELD_NAMES = {}
for _, group in ipairs({
    M.IDENTITY_FIELDS,
    M.DATE_FIELDS,
    M.TRAIT_FIELDS,
    M.FACE_STAT_FIELDS,
    M.RAW_OUTFIELD_FIELDS,
    M.RAW_GOALKEEPER_FIELDS,
}) do
    for _, name in ipairs(group) do
        M.FIELD_NAMES[#M.FIELD_NAMES + 1] = name
    end
end

M.OUTFIELD_PLAYSTYLES = {
    { bit = 1, label = "Finesse Shot" },
    { bit = 2, label = "Chip Shot" },
    { bit = 4, label = "Power Shot" },
    { bit = 8, label = "Dead Ball" },
    { bit = 16, label = "Precision Header" },
    { bit = 32, label = "Acrobatic" },
    { bit = 64, label = "Low Driven Shot" },
    { bit = 128, label = "Gamechanger" },
    { bit = 256, label = "Incisive Pass" },
    { bit = 512, label = "Pinged Pass" },
    { bit = 1024, label = "Long Ball Pass" },
    { bit = 2048, label = "Tiki Taka" },
    { bit = 4096, label = "Whipped Pass" },
    { bit = 8192, label = "Inventive" },
    { bit = 16384, label = "Jockey" },
    { bit = 32768, label = "Block" },
    { bit = 65536, label = "Intercept" },
    { bit = 131072, label = "Anticipate" },
    { bit = 262144, label = "Slide Tackle" },
    { bit = 524288, label = "Aerial Fortress" },
    { bit = 1048576, label = "Technical" },
    { bit = 2097152, label = "Rapid" },
    { bit = 4194304, label = "First Touch" },
    { bit = 8388608, label = "Trickster" },
    { bit = 16777216, label = "Press Proven" },
    { bit = 33554432, label = "Quick Step" },
    { bit = 67108864, label = "Relentless" },
    { bit = 134217728, label = "Long Throw" },
    { bit = 268435456, label = "Bruiser" },
    { bit = 536870912, label = "Enforcer" },
}

M.GOALKEEPER_PLAYSTYLES = {
    { bit = 1, label = "Far Throw" },
    { bit = 2, label = "Footwork" },
    { bit = 4, label = "Cross Claimer" },
    { bit = 8, label = "Rush Out" },
    { bit = 16, label = "Far Reach" },
    { bit = 32, label = "Deflector" },
}

M.GOALKEEPER_FACE_STAT_LABELS = {
    pacdiv = "DIV",
    shohan = "HAN",
    paskic = "KIC",
    driref = "REF",
    defspe = "SPD",
    phypos = "POS",
}

-- FC 27 stores 0 for a right foot and 1 for a left foot.
function M.decode_preferred_foot(value)
    if value == 0 then
        return "Right"
    elseif value == 1 then
        return "Left"
    end
    return "Unknown"
end

-- FC 27 stores the last two digits of the contract year. Keep a full year
-- unchanged, and return nil for missing or invalid values.
function M.normalize_contract_year(value)
    if value == nil or value <= 0 then
        return nil
    end
    if value >= 1900 then
        return value
    end
    if value < 100 then
        return value + 2000
    end
    return nil
end

-- Convert the player's Gregorian-day birthdate to a civil date. The
-- conversion is a public-domain civil-calendar algorithm.
function M.birthdate_to_date(days)
    if not days or days <= 0 then
        return nil
    end
    local a = days + 2331205
    local b = math.floor((4 * a + 3) / 146097)
    local c = math.floor((-b * 146097 / 4) + a)
    local d = math.floor((4 * c + 3) / 1461)
    local e = math.floor(-1461 * d / 4 + c)
    local m = math.floor((5 * e + 2) / 153)
    return {
        day = math.ceil(-(153 * m + 2) / 5) + e + 1,
        month = math.ceil(-m / 10) * 12 + m + 3,
        year = b * 100 + d - 4800 + math.floor(m / 10),
    }
end

-- The year difference used by the FC database. This is not birthday-aware;
-- callers that need exact age should compare month and day themselves.
function M.compute_age(birthdate, current_year)
    local date = M.birthdate_to_date(birthdate)
    if not date or not current_year then
        return nil
    end
    return current_year - date.year
end

function M.is_generated(playerid, threshold)
    return playerid ~= nil and threshold ~= nil and playerid >= threshold
end

-- Decode an outfield or goalkeeper PlayStyle bitmask. `kind` is
-- "outfield" (default) or "goalkeeper". A present zero returns an empty
-- string; a missing bitmask returns nil.
function M.decode_playstyles(bitmask, kind)
    if bitmask == nil then
        return nil
    end
    if bitmask == 0 then
        return ""
    end
    local mapping = M.OUTFIELD_PLAYSTYLES
    if kind == "goalkeeper" then
        mapping = M.GOALKEEPER_PLAYSTYLES
    end
    local labels = {}
    for _, entry in ipairs(mapping) do
        if (bitmask & entry.bit) ~= 0 then
            labels[#labels + 1] = entry.label
        end
    end
    table.sort(labels)
    return table.concat(labels, ";")
end

-- Read one field through the FC27LE table API, converting an unsupported or
-- unreadable field into (nil, false) instead of inventing a value.
function M.read_field(tbl, record, field)
    local ok, value = pcall(function()
        return tbl:GetRecordFieldValue(record, field)
    end)
    if ok and value ~= nil then
        return value, true
    end
    return nil, false
end

-- Read the complete verified field set for one record. Returns a table keyed
-- by field name and an array of field names that were unavailable.
function M.read_player(tbl, record)
    local values = {}
    local missing = {}
    for _, name in ipairs(M.FIELD_NAMES) do
        local value, ok = M.read_field(tbl, record, name)
        if ok then
            values[name] = value
        else
            missing[#missing + 1] = name
        end
    end
    return values, missing
end

return M
