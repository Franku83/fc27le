-- Read-only Career Mode season-statistics summary for FC27LE.
-- This uses the compatibility APIs and never calls a game function or writes memory.

local playerid = 252042 -- João Pedro
local rows = GetPlayerStats(playerid)

LOGGER:LogInfo(string.format(
    "Career statistics rows for player %d: %d",
    playerid, #rows
))

table.sort(rows, function(left, right)
    return left.compobjid < right.compobjid
end)

for index = 1, #rows do
    local stat = rows[index]
    local average = 0
    if stat.app > 0 then
        average = (stat.avg / stat.app) / 10
    end
    LOGGER:LogInfo(string.format(
        "%s | %d apps | %.2f AVG | %d goals | %d assists | %d MOTM",
        stat.compname, stat.app, average, stat.goals, stat.assists, stat.motm
    ))
end

LOGGER:LogInfo(string.format("Total season-stat records: %d", #GetPlayersStats()))
