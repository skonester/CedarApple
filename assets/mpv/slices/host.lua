-- CedarApple's Lua Slice adapter. CEDAR_SLICE_CONFIG is supplied by the host.
local utils = require("mp.utils")
local config = utils.parse_json(CEDAR_SLICE_CONFIG)
local handlers = {}
local stopped = false

local function emit(event, data)
    if stopped then return end
    -- Older mpv returns JSON, nil on success. Capture only the JSON so Lua
    -- does not expand the trailing nil into an extra commandv argument.
    local payload = utils.format_json(data or {})
    mp.commandv("script-message", "cedarapple-slice", config.session,
        event, payload)
end

cedarapple_slice = {
    emit = emit,
    on = function(event, callback) handlers[event] = callback end,
    id = config.id,
}

mp.register_script_message("slice-event", function(event, body)
    if stopped then return end
    local callback = handlers[event]
    if callback then
        local ok, err = pcall(callback, utils.parse_json(body or "{}") or {})
        if not ok then emit("error", {message = tostring(err)}) end
    end
    if event == "shutdown" then
        stopped = true
        mp.keep_running = false
    end
end)

local ok, err = pcall(dofile, config.entry)
if ok then
    emit("ready", {id = config.id})
else
    emit("error", {message = tostring(err)})
    mp.keep_running = false
end
