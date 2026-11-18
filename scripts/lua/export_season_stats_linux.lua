-- Linux-compatible season-statistics exporter.
--
-- The original FC 26 script is kept unchanged. This version:
--   * uses an XDG/user-selected output directory instead of USERPROFILE;
--   * escapes CSV fields containing commas, quotes or line breaks;
--   * writes UTF-8 names unchanged;
--   * refuses to overwrite an existing export (a numeric suffix is added);
--   * writes through a temporary file and renames only after success;
--   * raises an explicit error instead of silently dropping a stat row.
--
-- Output columns are the same as the original script.

local function load_csv()
    local source = debug and debug.getinfo(1, "S").source or ""
    local directory = source:match("@(.*[/\\])")
    if directory then
        local chunk = loadfile(directory .. "csv.lua")
        if chunk then
            return chunk()
        end
    end
    local ok, module = pcall(require, "csv")
    if ok and module then
        return module
    end
    error("cannot load csv.lua next to export_season_stats_linux.lua")
end

local csv = load_csv()

local function read_xdg_dir(name)
    local value = os.getenv(name)
    if value and value ~= "" then
        return value
    end
    local home = os.getenv("HOME")
    if not home then
        return nil
    end
    local file = io.open(home .. "/.config/user-dirs.dirs", "r")
    if not file then
        return nil
    end
    local result
    for line in file:lines() do
        local candidate = line:match("^%s*" .. name .. '%s*=%s*"(.*)"%s*$')
        if candidate then
            result = candidate:gsub("%$HOME", home)
            break
        end
    end
    file:close()
    return result
end

local function output_directory()
    local explicit = os.getenv("FC27LE_EXPORT_DIR")
    if explicit and explicit ~= "" then
        return explicit
    end
    local desktop = read_xdg_dir("XDG_DESKTOP_DIR")
    if desktop and desktop ~= "" then
        return desktop
    end
    local home = os.getenv("HOME")
    if home then
        local fallback = home .. "/Desktop"
        if csv.exists(fallback) then
            return fallback
        end
        return home
    end
    error("cannot determine an output directory: HOME is not set")
end

local columns = {
    "position",
    "playerid",
    "playername",
    "team",
    "competition",
    "appearances",
    "AVG",
    "MOTMs",
    "goals",
    "assists",
    "yellow_cards",
    "two_yellow",
    "red_cards",
    "saves",
    "goals_conceded",
    "cleansheets"
}

local function required(stat, key, playerid, compobjid)
    local value = stat[key]
    if value == nil then
        error(string.format(
            "missing field `%s` for player %s in competition %s; no row was written",
            key, tostring(playerid), tostring(compobjid)
        ))
    end
    return value
end

local function get_players(needed)
    local result = {}
    local players_table = LE.db:GetTable("players")
    assert(players_table ~= nil, "players table is unavailable")
    local current_record = players_table:GetFirstRecord()
    while current_record > 0 do
        local playerid = players_table:GetRecordFieldValue(current_record, "playerid")
        if playerid and playerid > 0 and needed[playerid] then
            local preferredposition1 = players_table:GetRecordFieldValue(current_record, "preferredposition1")
            result[playerid] = {
                position = GetPlayerPrimaryPositionName(preferredposition1),
                name = GetPlayerName(playerid),
                teamname = GetTeamName(GetTeamIdFromPlayerId(playerid)),
            }
        end
        current_record = players_table:GetNextValidRecord()
    end
    return result
end

local function build_rows(all_stats, players)
    local rows = {}
    local by_key = {}
    local duplicates = 0
    for index = 1, #all_stats do
        local stat = all_stats[index]
        local playerid = required(stat, "playerid", "?", "?")
        local app = required(stat, "app", playerid, stat.compobjid)
        if playerid > 0 and playerid < 4294967295 and app > 0 then
            local player = players[playerid]
            if player == nil then
                error(string.format(
                    "no player row for player %s in competition %s; no row was written",
                    tostring(playerid), tostring(stat.compobjid)
                ))
            end
            local compobjid = required(stat, "compobjid", playerid, "?")
            local key = string.format("%d:%d", playerid, compobjid)
            if by_key[key] then
                duplicates = duplicates + 1
            end
            local avg = required(stat, "avg", playerid, compobjid)
            if app > 1 then
                avg = (avg / app) / 10
            else
                avg = avg / 10
            end
            local row = {
                player.position,
                playerid,
                player.name,
                player.teamname,
                required(stat, "compname", playerid, compobjid),
                app,
                string.format("%0.2f", avg),
                required(stat, "motm", playerid, compobjid),
                required(stat, "goals", playerid, compobjid),
                required(stat, "assists", playerid, compobjid),
                required(stat, "yellow", playerid, compobjid),
                required(stat, "two_yellow", playerid, compobjid),
                required(stat, "red", playerid, compobjid),
                required(stat, "saves", playerid, compobjid),
                required(stat, "goals_conceded", playerid, compobjid),
                required(stat, "clean_sheets", playerid, compobjid),
            }
            row._playerid = playerid
            row._compobjid = compobjid
            -- The original script keyed rows by playerid and compobjid, so a
            -- later duplicate replaced an earlier one. Keep that behaviour and
            -- report the replacement count instead of hiding it.
            by_key[key] = row
        end
    end
    for _, row in pairs(by_key) do
        rows[#rows + 1] = row
    end
    table.sort(rows, function(left, right)
        if left._playerid == right._playerid then
            return left._compobjid < right._compobjid
        end
        return left._playerid < right._playerid
    end)
    return rows, duplicates
end

assert(IsInCM(), "Script must be executed in career mode")

local current_date = GetCurrentDate()
assert(type(current_date) == "table", "GetCurrentDate did not return a table")
assert(type(current_date.day) == "number", "GetCurrentDate is missing day")
assert(type(current_date.month) == "number", "GetCurrentDate is missing month")
assert(type(current_date.year) == "number", "GetCurrentDate is missing year")

local directory = output_directory()
local base_path = string.format(
    "%s/SEASON_STATS_%02d_%02d_%04d.csv",
    directory, current_date.day, current_date.month, current_date.year
)
local path = csv.unique_path(base_path)

local all_stats = GetPlayersStats()
assert(type(all_stats) == "table", "GetPlayersStats did not return an array")

local needed = {}
for index = 1, #all_stats do
    local stat = all_stats[index]
    if stat.playerid and stat.playerid > 0 and stat.playerid < 4294967295 and stat.app and stat.app > 0 then
        needed[stat.playerid] = true
    end
end

local players = get_players(needed)
local rows, duplicates = build_rows(all_stats, players)

local lines = { csv.row(columns) }
for _, row in ipairs(rows) do
    lines[#lines + 1] = csv.row(row)
end
csv.write_atomic(path, lines)

LOGGER:LogInfo(string.format(
    "Wrote %d season-stat rows to %s (%d duplicate player/competition rows replaced)",
    #rows, path, duplicates
))
