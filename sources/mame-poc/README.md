MAME debugger proof of concept
===============================

Run the Debian-release capability checks:

    make -C sources/mame-poc check

Requires MAME, an audited enhanced Apple IIe ROM set (`apple2ee`), Python 3.11+, uv, and ty. Tested with Debian MAME 0.276 and its W65C02 device. No MAME rebuild is required. The script has no Python package dependencies. `make mame-poc` from the repository root runs the same checks.

`check` uses the installed ROMs and preserves full MAME logs. It rejects checksum failures and process warnings. To override MAME's configured ROM search path, use:

    make -C sources/mame-poc probe ROMPATH=/path/to/roms

Omit ROMPATH to use MAME's configured search paths. Both commands skip normal ROM/disk boot and load test code at a stopped reset boundary. The synthetic mode describes a short test program, not substitute firmware. No external ROMs are downloaded or committed.

What the checks establish
-------------------------

*   Headless, unthrottled execution using `-debug -debugger none`, with automatic breakpoint continuation and bounded termination.
*   Exact stopped-boundary cycle counts through the debugger's `totalcycles` symbol; snapshot reads leave CPU cycles unchanged.
*   Mapped RAM snapshots, C000 keyboard substitution, C010 read/write acknowledgement, and C030 speaker observations.
*   An eight-iteration synthetic loop with independently calculated 23-cycle intervals, repeated key delivery, and persistent acknowledged codes.
*   Twenty rounds of unchanged original gameplay code at $60E4, before MOVE_PLAYER, including movement, stopping, digging, guard updates, and sound. Each mode runs twice and must reproduce identical snapshots and timing.

Each header records and verifies `apple2ee` and `w65c02`, preventing results from silently mixing machine models. Enhanced IIe uses a CMOS CPU rather than the original Apple II's NMOS 6502. This short gameplay schedule has the same cycle totals as the initial II+ probe; other instruction paths, including decimal arithmetic, can differ by CPU model. Later timing validation must account for that choice.

Original-code setup
-------------------

The probe loads the reference's relocated image ranges, supplies level 1 at $9F00, and enters INIT_GAME_DATA at $6056. GAME_MODE zero selects the existing in-memory level loader; the first main-loop boundary changes it to two for keyboard gameplay. The original decoder, rendering routines, iris, and scheduler setup execute. The input schedule supplies CC at round zero, A0 at ten, and D5 at twelve. This isolates gameplay from disk/boot requirements without changing gameplay instructions.

The successful 0.276 run measured 1,044,485 cycles across twenty gameplay rounds, with intervals ranging from 40,157 to 66,737 cycles and 200 speaker reads. Loading/iris time is excluded. The snapshots include zero page, active/background maps, guard arrays, and hole arrays. They are raw hook-test records, not the shared gameplay trace schema. No Rust-port comparison, transition instrumentation, or timing tolerance is implemented.

Release-specific details
------------------------

*   Call `cpu.debug:bpset(address, "1", "")` with all arguments. Omitting condition/action crashed the installed 0.276 release; passing explicit strings worked.
*   Use mapped reads for Apple II RAM. `read_direct_u8` returned zero because this driver supplies RAM through handlers rather than direct pointers.
*   MAME exposes the stack pointer as a page-inclusive value: $01FF, not $FF.
*   Wait for reset to reach its vector before replacing PC; startup scripting can run while CPU reset is still pending.
*   Keep tap objects alive. Keyboard acknowledgement must observe writes as well as reads: CHECK_FOR_INPUT uses STX $C010.

The periodic callback runs only after an execution breakpoint has stopped the CPU. It does not sample a running PC periodically or use video frames as gameplay rounds. See [SEMANTICS.md](../../SEMANTICS.md) for the complete probe flow and its limits.

Artifacts and cleanup
---------------------

Each launch preserves `config.json`, `boundaries.jsonl`, and `mame.log` in a separate directory under `artifacts/mame-poc/apple2ee/`. Errors name the relevant log, and a 30-second host timeout bounds startup or debugger stalls. The emulator also has a twenty-second virtual-time limit if expected boundaries never arrive. Generated records are excluded from Git.

`mostlyclean` removes Python bytecode; `clean` also removes the uv environment and generated level input. Neither removes installed ROMs, probe records, MAME configuration, or process logs.


Disk source status
------------------

The reference project reconstructs the Apple II Reset Vector crack. Normal boot from a preferred Apple II 4am disk remains pending an appropriate image. The downloaded `sources/Lode Runner v1.0 (4am crack).zip` contains the Macintosh version: its Disk Copy 4.2 image has 409,600 data bytes, 9,600 tag bytes, and a Macintosh File System signature. The accompanying screenshot explicitly credits the Macintosh version. Those archives remain unchanged and are not used for Apple II validation.

API references
--------------

The implementation uses the documented [Lua debugger interface](https://docs.mamedev.org/luascript/ref-debugger.html) and [memory taps](https://docs.mamedev.org/luascript/ref-mem.html). Compatibility findings were checked against the [0.276 Lua bindings](https://github.com/mamedev/mame/blob/mame0276/src/frontend/mame/luaengine_debug.cpp), [6502 register implementation](https://github.com/mamedev/mame/blob/mame0276/src/devices/cpu/m6502/m6502.cpp), [enhanced IIe configuration](https://github.com/mamedev/mame/blob/mame0276/src/mame/apple/apple2e.cpp), and the installed cheat plugin's stopped-breakpoint handling pattern.
