-- CSV and safe-output helpers for the Linux season-statistics exporter.
-- Independent implementation; no proprietary Live Editor code is copied.

local M = {}

function M.exists(path)
    local file = io.open(path, "r")
    if file then
        file:close()
        return true
    end
    return false
end

function M.escape(value)
    if value == nil then
        value = ""
    end
    local text = tostring(value)
    if text:find('[,"\r\n]') then
        text = '"' .. text:gsub('"', '""') .. '"'
    end
    return text
end

function M.row(values)
    local cells = {}
    for index = 1, #values do
        cells[index] = M.escape(values[index])
    end
    return table.concat(cells, ",") .. "\n"
end

-- Pick a new path instead of silently overwriting an existing export.
function M.unique_path(path)
    if not M.exists(path) then
        return path
    end
    local base, extension = path:match("^(.*)(%.[^./\\]+)$")
    if not base then
        base, extension = path, ""
    end
    for index = 2, 9999 do
        local candidate = string.format("%s_%d%s", base, index, extension)
        if not M.exists(candidate) then
            return candidate
        end
    end
    error("too many existing exports for " .. path)
end

-- Write to a temporary file first and rename only after a complete, successful
-- write. A failure leaves no partial final file and removes the temporary file.
function M.write_atomic(path, lines)
    local temporary = path .. ".tmp"
    local file, open_error = io.open(temporary, "w")
    if not file then
        error("cannot create " .. temporary .. ": " .. tostring(open_error))
    end
    local ok, write_error = pcall(function()
        for _, line in ipairs(lines) do
            local wrote, error_message = file:write(line)
            if not wrote then
                error("write failed: " .. tostring(error_message))
            end
        end
    end)
    local close_ok, close_error = file:close()
    if not ok then
        os.remove(temporary)
        error(write_error)
    end
    if not close_ok then
        os.remove(temporary)
        error("close failed: " .. tostring(close_error))
    end
    local renamed, rename_error = os.rename(temporary, path)
    if not renamed then
        os.remove(temporary)
        error("rename failed: " .. tostring(rename_error))
    end
end

return M
