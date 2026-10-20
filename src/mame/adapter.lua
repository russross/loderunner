-- MAME executes the machine. This adapter observes source boundaries and exposes
-- only keyboard delivery and stop commands to the external validator.
local json = require("json")
local file = assert(io.open(assert(os.getenv("LODE_VALIDATION_CONFIG")), "r"))
local config = assert(json.decode(file:read("a"))); file:close()
local machine, cpu = manager.machine, manager.machine.devices[":maincpu"]
local space, debugger = cpu.spaces["program"], machine.debugger
local symbols = config.symbols
local taps, breakpoints = {}, {}
local phase, boot, attempt = "Playing", "entry", 0
local index, emitted, finished = 0, false, false
local latch, inputs, signals, log_cursor = 0, {}, {}, #debugger.consolelog
local polling_target = 0
local origin = nil
local excluded_cycles, transition_started = 0, nil
local active = false
local reloading = false
local observed_latch = false
local continue_interval
local polls = {}
local skip_main = false
local timing_events, timing_hooks, timing_installed = {}, {}, {}
if config.profile_timing then
    for name, event in pairs({DRAW_SPRITE="tile_draw", DRAW_SPRITE_AT_PIXEL_COORDS="actor_draw",
        ERASE_SPRITE_AT_PIXEL_COORDS="sprite_erase", PLAY_SOUND="sound_section", PLAY_NOTE="note",
        INC_ANIM_STATE="player_animation", DRAW_PLAYER="player_draw",
        DETERMINE_GUARD_MOVE="guard_route", HANDLE_TIMERS="hole_scan",
        ENABLE_NEXT_LEVEL_LADDERS="exit"}) do
        timing_hooks[assert(symbols[name], "missing timing symbol " .. name)] = event
    end
end

local function cycles()
    debugger:command('printf "VLD_CYCLES %d",totalcycles')
    return assert(tonumber(debugger.consolelog[#debugger.consolelog]:match("VLD_CYCLES (%d+)")))
end
local function memory()
    local bytes = {}
    for address = 0, 0x1fff do bytes[#bytes + 1] = string.format("%02x", space:read_u8(address)) end
    return table.concat(bytes)
end
local function save_json(path, value)
    local f = assert(io.open(path .. ".tmp", "w"))
    f:write(json.encode(value)); f:close(); assert(os.rename(path .. ".tmp", path))
end
local function actor(slot) return slot == 0 and "Player" or {Guard = slot} end
local function motion(slot, value) return {kind = "motion", actor = actor(slot), motion = value} end
local function words(line)
    local result = {}; for value in line:gmatch("%S+") do result[#result + 1] = value end; return result
end

-- Breakpoint actions run and continue synchronously. They record source-path
-- observations without pausing at every helper or interpreting instructions.
local function drain_signals()
    local last = #debugger.consolelog
    for i = log_cursor + 1, last do
        local line = debugger.consolelog[i]
        if line and line:sub(1, 4) == "TME " then
            local w = words(line)
            timing_events[#timing_events + 1] = {event=w[2], cycle=assert(tonumber(w[3]))}
        elseif line and line:sub(1, 4) == "VLD " then
            local w = words(line)
            local kind, slot = w[2], tonumber(w[3])
            if kind == "motion" then signals[#signals + 1] = motion(slot, w[4])
            elseif kind == "select" then signals[#signals + 1] = {kind = "select_guard", slot = slot}
            elseif kind == "collect" then signals[#signals + 1] = {kind = "collected", actor = actor(slot)}
            elseif kind == "drop" then signals[#signals + 1] = {kind = "dropped", slot = slot, cell = {col = tonumber(w[4]), row = tonumber(w[5])}}
            elseif kind == "contact_guard" then signals[#signals + 1] = {kind = "contact", actor = "Player", other = {Actor = {Guard = slot}}}
            elseif kind == "contact_cell" then signals[#signals + 1] = {kind = "contact", actor = actor(slot), other = {Cell = {col = tonumber(w[4]), row = tonumber(w[5])}}}
            elseif kind == "trapped" then signals[#signals + 1] = {kind = "trapped", slot = slot}
            elseif kind == "landed" then signals[#signals + 1] = {kind = "landed"}
            elseif kind == "exit" then signals[#signals + 1] = {kind = "exit"}
            else error("unknown source observation " .. line) end
        end
    end
    log_cursor = last
end
local function hook(address, message, args, condition)
    local action = 'printf "VLD ' .. message .. '"' .. (args and ',' .. args or '')
    if timing_hooks[address] then
        action = action .. ';printf "TME ' .. timing_hooks[address] .. ' %d",totalcycles'
        timing_installed[address] = true
    end
    cpu.debug:bpset(address, condition or "1", action .. ';g')
end
local function at(name) return assert(symbols[name], "missing symbol " .. name) end
local function stop(name, condition)
    local address = at(name)
    breakpoints[address] = name
    cpu.debug:bpset(address, condition or "1", "")
end
local function poll(name, condition)
    if polls[name] then cpu.debug:bpclear(polls[name]) end
    breakpoints[at(name)] = name
    polls[name] = cpu.debug:bpset(at(name), condition or "1", "")
end

local function install_hooks()
    hook(at("MOVE_PLAYER.check_for_keyboard_input"), "motion 0 Standing")
    hook(config.anchors.player_landed, "landed", nil, "b@9b == 0")
    hook(at("MOVE_PLAYER.make_player_fall"), "motion 0 Falling")
    for direction, name in pairs({Left="TRY_MOVING_LEFT.move_player_left", Right="TRY_MOVING_RIGHT.move_player_right",
        Up="TRY_MOVING_UP.move_player_up", Down="TRY_MOVING_DOWN.move_player_down"}) do
        hook(at(name), "motion 0 " .. direction)
    end
    hook(at("MOVE_GUARD.guard_num_incremented"), "select %d", "b@19")
    hook(at("MOVE_GUARD.blank_or_player"), "motion %d Falling", "b@19")
    for direction, name in pairs({Left="TRY_GUARD_MOVE_LEFT.move_left", Right="TRY_GUARD_MOVE_RIGHT.move_right",
        Up="TRY_GUARD_MOVE_UP.move_up", Down="TRY_GUARD_MOVE_DOWN.move_down"}) do
        hook(at(name), "motion %d " .. direction, "b@19")
    end
    hook(config.anchors.player_collect, "collect 0")
    hook(config.anchors.guard_collect, "collect %d", "b@19")
    hook(config.anchors.guard_drop, "drop %d %d %d", "b@19,b@85,b@86")
    hook(at("MOVE_GUARD.drop_gold"), "drop %d %d %d", "b@19,b@85,b@86")
    hook(at("MOVE_GUARD.reset_gold_timer"), "trapped %d", "b@19")
    for _, address in ipairs(config.anchors.guard_contacts) do hook(address, "contact_guard %d", "b@19") end
    hook(config.anchors.player_hole_contact, "contact_cell 0 %d %d", "b@85,b@86")
    hook(at("HANDLE_TIMERS.reset_guard_gold_timer"), "contact_cell %d %d %d", "x,b@85,b@86")
    hook(at("ENABLE_NEXT_LEVEL_LADDERS"), "exit")
    for address, event in pairs(timing_hooks) do
        if not timing_installed[address] then
            cpu.debug:bpset(address, "1", 'printf "TME ' .. event .. ' %d",totalcycles;g')
        end
    end
end

local function install_devices()
    taps[#taps + 1] = space:install_read_tap(0xc000, 0xc000, "validation_keyboard", function()
        if active and latch & 0x80 ~= 0 and not observed_latch then
            inputs[#inputs + 1] = {observation=#inputs, key=latch}; observed_latch = true
        end
        return latch
    end)
    local function acknowledge() latch = latch & 0x7f end
    taps[#taps + 1] = space:install_read_tap(0xc010, 0xc010, "validation_ack_read", acknowledge)
    taps[#taps + 1] = space:install_write_tap(0xc010, 0xc010, "validation_ack_write", acknowledge)
end

local function sample()
    drain_signals()
    local current = cycles()
    if not origin then origin = current end
    save_json(config.work .. "/sample-" .. index .. ".json", {cycle=current-origin-excluded_cycles, phase=phase,
        attempt=attempt, memory=memory(), inputs=inputs, signals=signals})
    if config.profile_timing then
        save_json(config.work .. "/timing-" .. index .. ".json", timing_events)
    end
    timing_events = {}
    assert(cycles() == current, "sampling advanced machine time")
    inputs, signals, log_cursor = {}, {}, #debugger.consolelog
    emitted = true
    -- The "none" debugger automatically resumes after this callback returns.
    -- Keep its stopped instruction here until the validator supplies input.
    local deadline = os.time() + 120
    while emitted and not finished do
        local f = io.open(config.work .. "/command-" .. index .. ".json", "r")
        if f then
            local command = assert(json.decode(f:read("a"))); f:close()
            continue_interval(command)
        else assert(os.time() < deadline, "validator command timeout") end
    end
end
local function stop_session(save)
    if save then
        save_json(config.work .. "/checkpoint.json", {pc=cpu.state.PC.value, cycle=cycles(),
            phase=phase, attempt=attempt, latch=latch, observed_latch=observed_latch})
        debugger:command('statesave "' .. config.work .. '/failure.sta"')
    end
    finished = true
    save_json(config.work .. "/done.json", {machine=emu.romname(), cpu=cpu.shortname,
        version=emu.app_version(), saved=save})
    machine:exit()
end

-- Waiting polls are batched by cycle threshold. Keyboard delivery ends the
-- interval at the next game boundary, before another gameplay instruction runs.
continue_interval = function(command)
    if command.stop then stop_session(command.save); return end
    if phase == "Died" or phase == "LevelCleared" then
        transition_started = cycles()
        -- Remaining lives belong to the original application shell. Replenish
        -- them outside gameplay so repeated captures never enter game over.
        space:write_u8(0x98, 5)
    end
    if type(command.key) == "number" then latch = command.key; observed_latch = false end
    index = index + 1; emitted = false
    polling_target = cycles() + 5000
    local condition = "totalcycles >= " .. string.format("%x", polling_target)
    for _, name in ipairs({"WAIT_FOR_KEY_WITH_CURSOR_PAGE_1.loop2", "WAIT_FOR_KEY_WITH_CURSOR_PAGE_1.loop3",
        "WAIT_KEY_QUEUED", "WAIT_FOR_KEY.loop2", "WAIT_FOR_KEY.loop3",
        "SHOW_ANIM_LINE.check_for_keypress", "CHECK_FOR_BUTTON_DOWN.poll_keyboard"}) do poll(name, condition) end
    cpu.debug:go()
end

local function stopped()
    if finished then return end
    if emitted then
        local f = io.open(config.work .. "/command-" .. index .. ".json", "r")
        if f then local command = assert(json.decode(f:read("a"))); f:close(); continue_interval(command) end
        return
    end
    if debugger.execution_state ~= "stop" then return end
    local pc = cpu.state["PC"].value
    if boot ~= "done" then
        if boot == "entry" then
            assert(pc == 0x6000, "unexpected boot stop")
            cpu.debug:bpclear()
            install_devices()
            stop("CHECK_FOR_BUTTON_DOWN.poll_keyboard")
            boot = "splash"
        elseif boot == "splash" then
            latch = 0xa0; cpu.debug:bpclear()
            stop("INIT_GAME_DATA"); boot = "initialize"
        elseif boot == "initialize" then
            space:write_u8(0xa6, config.start + 1); space:write_u8(0x96, config.start)
            if config.fixed_initialization then
                space:write_u8(5, 0); space:write_u8(0x64, 0)
                for slot=1,5 do space:write_u8(0xc90+slot, 0) end
                -- A supported input dispatch establishes these sound values.
                -- Airborne starts need the same initial state before that path.
                space:write_u8(0xa4, 0x20); space:write_u8(0x9b, 0x20)
            end
            cpu.debug:bpclear(); stop("WAIT_FOR_KEY_WITH_CURSOR_PAGE_1"); boot = "ready"
        else
            cpu.debug:bpclear(); install_hooks()
            stop("INIT_GAME_DATA.game_loop"); stop("INIT_GAME_DATA.died"); stop("INIT_GAME_DATA.level_cleared")
            stop("WAIT_FOR_KEY_WITH_CURSOR_PAGE_1"); stop("LOAD_LEVEL")
            stop("ESC_HANDLER"); stop("RETURN_HANDLER"); stop("SPINNING_GAME_OVER")
            stop("INIT_GAME_DATA"); stop("INIT_GAME_DATA.play_game")
            breakpoints[0x6a7d] = "pause_resume"; cpu.debug:bpset(0x6a7d, "1", "")
            stop("RECORD_HI_SCORE_DATA_TO_DISK")
            boot="done"; phase="Playing"
            active=true
            latch=0x8b; cpu.debug:go(); return
        end
        cpu.debug:go(); return
    end

    local name = breakpoints[pc]
    if name == "INIT_GAME_DATA.game_loop" then
        if skip_main then skip_main=false; cpu.debug:go(); return end
        if not origin then inputs, signals = {}, {}; drain_signals(); signals = {} end
        if transition_started then
            excluded_cycles = excluded_cycles + cycles() - transition_started
            transition_started = nil
            inputs, signals = {}, {}
            drain_signals(); signals = {}
        end
        phase="Playing"; reloading=false; sample()
    elseif name == "INIT_GAME_DATA.died" then phase="Died"; sample()
    elseif name == "INIT_GAME_DATA.level_cleared" then phase="LevelCleared"; sample()
    elseif name == "LOAD_LEVEL" then
        if not reloading then attempt=attempt+1 end
        reloading=true; cpu.debug:go()
    elseif name == "WAIT_FOR_KEY_WITH_CURSOR_PAGE_1" then
        reloading=false
        latch=0x8b; observed_latch=true; phase="Playing"; cpu.debug:go()
    elseif name == "INIT_GAME_DATA.play_game" then
        if phase == "Waiting" then phase="Playing"; skip_main=true; sample() else cpu.debug:go() end
    elseif name == "ESC_HANDLER" then
        if phase ~= "Paused" or #inputs > 0 then phase="Paused"; sample() else cpu.debug:go() end
    elseif name == "pause_resume" then
        phase="Playing"; sample()
    elseif name == "RETURN_HANDLER" or name == "RECORD_HI_SCORE_DATA_TO_DISK" then
        -- Score menus lie outside level gameplay. Return through an existing RTS
        -- without executing either menu or changing gameplay storage.
        cpu.state.PC.value = at("RECORD_HI_SCORE_DATA_TO_DISK.end")
        cpu.debug:go()
    elseif name == "SPINNING_GAME_OVER" then phase="GameOver"; sample()
    elseif name == "INIT_GAME_DATA" then
        space:write_u8(0xa6, config.start+1); space:write_u8(0x96, config.start)
        signals[#signals+1] = {kind="new_game"}
        attempt=0; reloading=true; cpu.debug:go()
    elseif name == "WAIT_FOR_KEY_WITH_CURSOR_PAGE_1.loop2" or name == "WAIT_FOR_KEY_WITH_CURSOR_PAGE_1.loop3"
        or name == "WAIT_KEY_QUEUED" or name == "WAIT_FOR_KEY.loop2" or name == "WAIT_FOR_KEY.loop3"
        or name == "SHOW_ANIM_LINE.check_for_keypress" or name == "CHECK_FOR_BUTTON_DOWN.poll_keyboard" then
        if phase ~= "Playing" and cycles() >= polling_target and not (latch & 0x80 ~= 0 and not observed_latch) then
            sample()
        else cpu.debug:go() end
    else
        error("unsupported validation boundary " .. tostring(name))
    end
end

assert(emu.romname()=="apple2ee" and cpu.shortname=="w65c02", "validation platform mismatch")
cpu.debug:bpset(0x6000, "pb@60e4 == 20 && pb@60e5 == bd && pb@60e6 == 64", "")
emu.register_periodic(function()
    local ok, message = pcall(stopped)
    if not ok and not finished then
        save_json(config.work .. "/error.json", {message=tostring(message), pc=cpu.state["PC"].value})
        stop_session(true)
    end
end)
cpu.debug:go()
