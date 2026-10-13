-- The adapter stops at instruction boundaries and keeps snapshots below I/O.
-- All machine execution, bus accesses, and cycle accounting remain in MAME.
local json = require("json")
local config_file = assert(io.open(assert(os.getenv("LODE_POC_CONFIG")), "r"))
local config = assert(json.decode(config_file:read("a")))
config_file:close()
local output = assert(io.open(config.output, "w"))
local machine = manager.machine
local cpu = assert(machine.devices[":maincpu"])
local space = assert(cpu.spaces["program"])
local debugger = assert(machine.debugger)
local taps = {}
local started, finished = false, false
local index, previous = 0, nil
local latch, acknowledgements, speaker_edges = 0, 0, 0
local observed = {}
local boundary = config.mode == "synthetic" and 0x1000 or 0x60e4
local boot_phase = "entry"
local boot_breakpoint = nil

local function emit(record)
    output:write(json.encode(record), "\n")
    output:flush()
end

-- The debugger symbol is exact at a stopped instruction boundary. The older
-- Lua API does not expose the CPU execution interface's total_cycles directly.
local function cycles()
    debugger:command('printf "LODE_POC_CYCLES %d",totalcycles')
    local line = debugger.consolelog[#debugger.consolelog]
    return assert(tonumber(line:match("LODE_POC_CYCLES (%d+)")), line)
end

local function hex_memory(first, last)
    local bytes = {}
    for address = first, last do
        bytes[#bytes + 1] = string.format("%02x", space:read_u8(address))
    end
    return table.concat(bytes)
end

local function load_bytes(first, bytes)
    for offset = 1, #bytes do
        space:write_u8(first + offset - 1, bytes:byte(offset))
    end
end

local function save_loaded_image()
    local bytes = {}
    for address = 0, 0xbfff do bytes[#bytes + 1] = string.char(space:read_u8(address)) end
    local file = assert(io.open(config.loaded_image, "wb"))
    file:write(table.concat(bytes))
    file:close()
end

-- Loading follows the reference's relocated image ranges. Its in-memory level
-- source supplies sector zero without DOS, before switching to keyboard play.
local function initialize_game()
    local file = assert(io.open(config.image, "rb"))
    local image = file:read("a")
    file:close()
    assert(#image == 0xc000, "unexpected reference image size")
    load_bytes(0, image:sub(1, 0x2000))
    load_bytes(0x5f00, image:sub(0x5f01, 0xc000))
    file = assert(io.open(config.level, "rb"))
    local level = file:read("a")
    file:close()
    assert(#level == 256, "unexpected level sector size")
    load_bytes(0x9f00, level)
    assert(space:read_u8(boundary) == 0x20 and space:read_u8(boundary + 1) == 0xbd
        and space:read_u8(boundary + 2) == 0x64, "round boundary image mismatch")

    -- These are entry preconditions normally established by the splash/loader.
    -- Initialization, rendering, the iris, and guard setup then execute unchanged.
    space:write_u8(0xa7, 0)
    space:write_u8(0xa6, 1)
    space:write_u8(0x95, 0xcb)
    space:write_u8(0x8c, 6)
    space:write_u8(0x99, 1)
    space:write_u8(0x1f, 0x20)
    space:write_u8(0x87, 0x20)
    space:write_u8(0x05, 0)
    cpu.state["SP"].value = 0x1ff
    cpu.state["P"].value = 0x24
    cpu.state["PC"].value = 0x6056
end

local function initialize_synthetic()
    -- INC zp, LDA absolute, STA zp, BIT absolute, LDA absolute, JMP absolute:
    -- 5 + 4 + 3 + 4 + 4 + 3 = 23 cycles between consecutive loop entries.
    load_bytes(boundary, string.char(0xe6, 0x20, 0xad, 0x00, 0xc0, 0x85, 0x21,
        0x2c, 0x10, 0xc0, 0xad, 0x30, 0xc0, 0x4c, 0x00, 0x10))
    space:write_u8(0x20, 0)
    space:write_u8(0x21, 0)
    cpu.state["PC"].value = boundary
end

local function install_input()
    taps[#taps + 1] = space:install_read_tap(0xc000, 0xc000, "poc_keyboard",
        function()
            if latch & 0x80 ~= 0 then observed[#observed + 1] = latch end
            return latch
        end)
    local function acknowledge()
        acknowledgements = acknowledgements + 1
        latch = latch & 0x7f
    end
    taps[#taps + 1] = space:install_read_tap(0xc010, 0xc010, "poc_ack_read", acknowledge)
    taps[#taps + 1] = space:install_write_tap(0xc010, 0xc010, "poc_ack_write", acknowledge)
    taps[#taps + 1] = space:install_read_tap(0xc030, 0xc030, "poc_speaker",
        function() speaker_edges = speaker_edges + 1 end)
end

local function snapshot()
    local current = cycles()
    emit({kind = "boundary", index = index, cycles = current,
        delta = previous and current - previous or 0, pc = cpu.state["PC"].value,
        ram = hex_memory(0, 0xff), active = hex_memory(0x0800, 0x09bf),
        background = hex_memory(0x0a00, 0x0bbf), guards = hex_memory(0x0c60, 0x0c9f),
        holes = hex_memory(0x0ca0, 0x0cff), observed = observed,
        acknowledgements = acknowledgements, speaker_edges = speaker_edges})
    assert(cycles() == current, "snapshot changed CPU cycle count")
    previous = current
    observed, acknowledgements, speaker_edges = {}, 0, 0
end

-- The initial breakpoint waits for reset to finish before changing CPU state.
-- Supplying all bpset arguments avoids a null-string crash in MAME 0.276.
local reset_vector = space:read_u16(0xfffc)
local reset_breakpoint = nil
if config.mode == "disk" then
    -- Observe the loaded application's entry without changing PC, registers,
    -- code, disk reads, or the game's own initialization workflow.
    boot_breakpoint = cpu.debug:bpset(0x6000,
        "pb@60e4 == 20 && pb@60e5 == bd && pb@60e6 == 64", "")
else
    reset_breakpoint = cpu.debug:bpset(reset_vector, "1", "")
end
emit({kind = "header", mode = config.mode, version = emu.app_version(),
    machine = emu.romname(), cpu = cpu.shortname,
    boundary = boundary, reset_vector = reset_vector, disk_sha256 = config.disk_sha256})

-- Starting at the splash and ready cursor uses only keyboard-device delivery.
-- Setup observations are discarded when the first gameplay boundary is reached.
local function boot_stopped()
    local pc = cpu.state["PC"].value
    if boot_phase == "entry" then
        assert(pc == 0x6000, "unexpected disk entry stop")
        save_loaded_image()
        install_input()
        cpu.debug:bpclear(boot_breakpoint)
        boot_breakpoint = cpu.debug:bpset(0x61a9, "1", "")
        boot_phase = "start"
    elseif boot_phase == "start" then
        assert(pc == 0x61a9, "unexpected splash input stop")
        latch = 0xa0
        cpu.debug:bpclear(boot_breakpoint)
        boot_breakpoint = cpu.debug:bpset(0x8700, "1", "")
        boot_phase = "ready"
    elseif boot_phase == "ready" then
        assert(pc == 0x8700, "unexpected ready cursor stop")
        latch = 0x8b
        cpu.debug:bpclear(boot_breakpoint)
        cpu.debug:bpset(boundary, "1", "")
        boot_phase = "game"
    else
        assert(pc == boundary, "unexpected first gameplay stop")
        observed, acknowledgements, speaker_edges = {}, 0, 0
        started = true
        return
    end
    cpu.debug:go()
end

local function stopped()
    if finished or debugger.execution_state ~= "stop" then return end
    if config.mode == "disk" and not started then
        boot_stopped()
        if not started then return end
    end
    if not started then
        assert(cpu.state["PC"].value == reset_vector, "unexpected startup stop")
        cpu.debug:bpclear(reset_breakpoint)
        if config.mode == "synthetic" then initialize_synthetic() else initialize_game() end
        install_input()
        cpu.debug:bpset(boundary, "1", "")
        started = true
        if config.mode == "game" then cpu.debug:go(); return end
    end

    assert(cpu.state["PC"].value == boundary, "unexpected instruction stop")
    if config.mode == "game" and index == 0 then space:write_u8(0xa7, 2) end
    snapshot()
    if index == config.rounds then
        finished = true
        emit({kind = "complete", rounds = index, tap_count = #taps})
        output:close()
        machine:exit()
        return
    end
    for _, delivery in ipairs(config.inputs) do
        if delivery.round == index then latch = delivery.key end
    end
    index = index + 1
    cpu.debug:go()
end

emu.register_periodic(function()
    local ok, message = pcall(stopped)
    if not ok and not finished then
        finished = true
        emit({kind = "error", message = tostring(message)})
        output:close()
        machine:exit()
    end
end)
cpu.debug:go()
