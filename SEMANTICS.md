Lode Runner port semantics
=========================

Status and boundaries
---------------------

The same-implementation gameplay trace round-trip milestone is complete. The procedural Rust port executes keyboard gameplay with guards, digging, gold, holes, death/restart, level advancement, pause, sound/speed controls, high-score display and initials entry. SDL displays packed hires memory and plays speaker edges. Optional traces are independently reexecuted headlessly using only recorded observed inputs. This establishes internal replay consistency; original-machine equivalence is not yet validated. A separate MAME proof of concept exercises debugger hooks and original gameplay boundaries. Full emulator trace instrumentation, compact recordings, and subsequent higher-level ports remain outside the completed milestone.

`sources/lode_runner_reveng/` vendors reference revision `113db1c16951717259c9ae5e128958b29de0634d`, including its provenance and CC BY-SA license. Its README identifies the reconstructed cracked Apple II disk image. The downloaded checkout remains untouched outside the root repository. Root Cargo builds `trace/` and `port-original/`. Gameplay files are flat in the port's `src/`, hardware lives in `src/devices/`, and integration tests live in `tests/`. Future ports become sibling packages sharing the trace contract.

Make drives incremental builds. Checks run formatting, Clippy, then tests in order. `mostlyclean` removes incremental intermediates; `clean` removes Cargo build outputs. Neither removes source assets, traces, or high scores.

Execution, storage, and initialization
--------------------------------------

The port translates original subroutines, loops, and conditions into Rust operations on named state. Shared exits and jumps into other routines are duplicated where necessary to keep functions self-contained. It does not execute 6502 instructions. Counters retain byte wrapping where gameplay depends on it; closed domains use enums. Score retains four little-endian packed BCD pairs, propagates decimal carry, and wraps at eight decimal digits. The observer exposes decimal score; the status screen displays the original seven visible digits.

Storage reads original 256-byte sectors from vendored track data for levels 1 through 150. There are no disk delays or controller emulation. Decoding visits rows top to bottom and columns left to right, consuming each low nibble before its high nibble. Tile values 10 through 15 become empty; only the first 224 bytes describe the 28 by 16 playfield.

Initialization visits rows bottom to top and columns right to left. The first encountered player survives; other player markers disappear. Guards receive slots 1 through 5 in the same traversal; extras disappear. At most 45 hidden ladders are retained in allocation order and removed from both logical maps. Accepted actor markers occupy the active map only. Gold remains in both maps until collection. Both hires pages are built through original sprite helpers, followed by status text. No player is an initialization error. Initial rendering costs are outside the initial virtual-clock origin.

Configuration embeds the full sector list, starting index, whether levels wait for a starting key, and initial high scores. The original configuration contains all 150 boards. Reloading after death or completion retains the virtual clock, speaker history, sound preferences, score, lives, guard scheduler phase, resurrection column, and eligibility for high scores. It increments attempt identity and resets exit availability. A new game resets gameplay progress and eligibility to the configured starting board while retaining sound preferences and high scores.

Devices and graphics
--------------------

Hires retains all bytes of both original 8 KiB pages, including display holes. Device buffers and the inverse scanline map are heap allocated so nested capture/replay construction does not exhaust small test-thread stacks. Visible byte changes dirty their whole scanline; display-hole writes do not dirty pixels. Page selection dirties all visible rows. Whole-row decoding covers neighboring-byte artifact dependencies. SDL uploads only dirty rows to a persistent texture and copies that texture when presenting; backbuffer retention is not assumed.

Sprite shifting uses original image shift pages and pattern tables. Tile draws preserve masking and two/three-byte writes. Actor draws accumulate `(foreground XOR background) AND shifted_sprite` before ORing the sprite into foreground. Erasure uses `(foreground AND (shifted_sprite XOR 0x7f)) OR background`. Collision uses packed framebuffer bytes, never decoded RGB. The digital decoder distinguishes adjacent white bits and alternating artifact colors, but does not implement analog NTSC filtering or fully accurate half-dot phase transitions.

Keyboard reads use C000 and acknowledge through C010. Clearing availability retains the last code; delivery replaces the latch, including repeated delivery of the same code. Persistent keyboard movement is intentional: physical release does not stop movement; Space supplies a stop command. A code delivered while falling or digging remains unobserved until the game polls it. Trace inputs are fresh acknowledged codes, not host key events or repeated reads of an unchanged latch. Thus a release record is not invented for hardware that does not expose releases.

Speaker access toggles drive at a virtual cycle. PLAY_NOTE advances countdown runs between low-counter wraps and pitch reloads, retaining each edge without instruction interpretation. Its first pitch countdown uses duration; subsequent ones use pitch. Incoming low counter affects the first duration segment. Muting retains countdown duration but saves the original three-cycle speaker-access difference. Note buffers append one-based and play last-in-first-out. Overflow reports an error rather than corrupting neighboring memory.

Live audio integrates speaker drive over globally indexed sample windows, preserving fractional-cycle boundaries across batches. Pruning retains the last preceding edge so future samples start in the correct state. SDL output applies a DC-removing filter and gain. Audio and dirty graphics are submitted at sound checkpoints and transition-note checkpoints before sleeping. Host rendering, audio conversion, and file parsing do not add emulated cycles.

Player execution
----------------

MOVE_PLAYER resets the gold-pickup collision exception. Continued digging bypasses gravity and keyboard polling. Otherwise it checks ladder and centered-rope support, then active and background support below. Falling erases the previous sprite, selects a facing-specific animation, nudges horizontal adjustment, advances vertical adjustment, and collects centered gold or transfers the active marker into the next row. Landing plays its note before keyboard observation. The note's incoming low counter is row 15 on the bottom support branch and column on other support paths.

A fresh code replaces both persistent command bytes; otherwise they persist. Vertical and dig commands precede horizontal fallback. Subcell adjustments range from 0 through 4, centered at 2. Crossing a boundary transfers the active player marker; the previous cell receives background terrain except background brick becomes active empty. Horizontal movement blocks brick, stone, and trap. Downward movement blocks brick and stone but permits trap. Nudging can collect gold at the centered position.

Collection removes background gold, erases its graphics, adds 250, appends six original notes, and suppresses framebuffer-overlap death for the remainder of that player call. Otherwise sprite overlap may kill the player. The trace attributes a collision partner only when logical contact establishes that partner; anonymous framebuffer overlap is represented by death without guessing a partner.

Digging requires a brick below the adjacent empty cell. Twelve calls update debris and brick graphics; the thirteenth completes the opening. Interruption restores the brick and erases debris. Completion does not move the player despite the original DROP_PLAYER_IN_HOLE label. It removes the active brick, retains background brick, clears both graphics images, and allocates the first free slot among 0 through 29 with timer 180. Per-cell generations identify later openings independently of reused timer slots.

Control handlers include sacrifice, end game, skip level, extra life, mute, speed adjustment, keyboard mode, pause, and high scores. Skip and extra-life commands disable high-score qualification. Joystick selection codes report an explicit unsupported-device error. The SDL mapping exposes the normal keyboard controls; raw capture codes also exercise original control handlers.

Guards, holes, and exits
-----------------------

Guards retain eight-element parallel arrays with one-based active slots. A working record is loaded, modified, and stored for each selected update. The scheduler advances a three-phase pattern selected by guard count plus difficulty, consumes bits least-significant first, and advances the round-robin guard index for each set bit. A dead player aborts further guard movement.

Gravity distinguishes empty/player cells, supporting guard occupants, and background terrain. Negative gold timers indicate carrying; positive timers represent trap waiting, shake offsets, and climbing out. Centered pickup removes background gold and seeds carry time with 0xff minus resurrection column. Dropping advances carry time only at movement boundaries and retries with 0xff if the background is occupied. Entering a hole drops carried gold above if possible, otherwise removes it from the remaining total. Centering inside background brick starts the trap timer and awards 75.

Route selection first handles trapped escape, then a supported route on the player's row. Otherwise it considers down, up, left columns from the farthest limit inward, and right columns from the farthest inward. Only strictly better pseudo-distance changes the choice. Horizontal distance is relative to the guard. Candidate searches preserve the original left/right stale-row-pointer asymmetry. Bottom-row search stops at row 15 instead of reproducing an out-of-map zero-page pointer-table alias read; ordinary boards with a supporting bottom floor stop earlier.

Resurrection timers update before hole timers and guard movement. The resurrection column advances modulo 28 once per ordinary round. Hole timers scan slots 30 down to zero, decrement once, draw refill stages at 20 and 10, and close at zero. Closure kills a player in the hole with an environment-contact occurrence, handles gold loss, restores brick in the active map and both graphics pages, and buries the matching guard. Burial searches empty background starting at row 1 and the current resurrection column, centers the guard, and starts resurrection time 20. Stages occur at 19 and 10; expiration retries each round until active occupancy is empty, then restores the guard and appends four notes. No empty resurrection location is an error instead of an out-of-bounds scan.

Exit revealing visits hidden ladders in reverse allocation order. Empty background becomes ladder while active occupancy remains intact. Revealed entries leave the pending list; blocked background defers an entry. Once all are processed without blockage, gold count becomes 0xff. Exit availability indicates completion eligibility, not necessarily visibility of every ladder. A centered player on row zero completes when gold count is zero or 0xff.

Rounds and session transitions
------------------------------

A gameplay round starts before MOVE_PLAYER. Its ordered operations are player update, sound/delay, exit revealing and completion check, resurrection timers, resurrection-column advancement, hole timers, sound/delay, and scheduled guards. Completion skips the latter operations. Death stops subsequent gameplay. A committed Died or LevelCleared snapshot ends the round before the separate transition interval changes lives or actor identity.

Death loses one life and plays the descending 24-note tune, appended to any remaining notes, followed by an empty sound section. Zero lives produces GameOver or initials entry. Otherwise the current board reloads. Completion increases displayed level and lives, awards fifteen increments of 100 with three notes each, and loads the next board. End of the configured board list wraps to its beginning and increases difficulty. Skip-level control advances board selection through the death transition without an ordinary life loss.

Waiting, Paused, HighScores, Initials, and GameOver are explicit non-gameplay intervals with 5,000-cycle polling budgets. Waiting accepts a key and retains a movement command when appropriate; Escape resumes pause. Any game-over key starts a new game. Return during gameplay shows high scores; a key or five-million-cycle timeout restores background and gameplay. No game movement runs during these intervals.

High scores rank strictly by `(level, score)`, with at most ten entries. A qualifying eligible nonzero score inserts three blank initials. Letters, digits, and period enter up to three characters; Backspace moves the cursor back and Return confirms. SDL loads `highscores.json` when present, otherwise the original disk table, and writes only a changed table on normal exit. Invalid files are errors. Capture and replay use the table embedded in configuration and never depend on current filesystem scores.

Observation and trace replay
----------------------------

The observer consumes committed snapshots and a transient occurrence journal; it does not simulate gameplay rules. Snapshots contain session level/attempt/phase/score/lives/exit availability, actor cell/subcell/facing/motion/action/lifecycle/carried gold, both logical terrain maps, pending hidden ladders, holes with generations, and high scores. Scheduler internals, delay preferences, animation bytes, and countdown values remain execution state; visible consequences are observed. Gold identity is a location or carrier, not an invented permanent identifier.

The sole cycle-based record is interval start as a delta from the previous start. Events inside are untimed. Empty intervals remain for input addressing. A final delta closes the last interval. Net snapshot changes are atomic and canonical by identity. Occurrences are not reapplied as mutations. Episodes preserve order per subject; independent subjects sort canonically. Consecutive contact runs sort and deduplicate, while non-contact occurrences retain their order. Observed inputs retain order and observation ordinal; this port supports at most one keyboard observation, ordinal zero, per interval.

Trace format version 2 carries reference/configuration identity, initial snapshot, rounds, final duration, and final snapshot. Parsing checks domains, identities, lengths, canonical ordering, interval numbers, clock overflow, change preconditions, and the resulting terminal state. Structural validity alone is not execution validation.

Replay constructs a fresh Run from embedded configuration and compares initial identity/state. For each expected interval it injects only its observed key through the keyboard device, executes the appropriate phase, and compares the independently generated round exactly. Expected changes and occurrences never drive execution. It also compares final state and duration, rejecting the first mismatch. Delayed keyboard deliveries can be injected at their observation interval because they had no prior observable effect. Unobserved host events are intentionally absent. Same-implementation comparison has zero tolerance; later emulator cross-validation may introduce interval timing tolerance without accepting gameplay differences.

Trace capture is optional during SDL play. Disabling it before execution avoids retaining round snapshots and journals. Headless capture accepts an explicit input schedule for development; this is not the later compact recording/playback feature. The earlier guard-free probe remains available for focused subset tests; full sessions use Run and include guards and transitions.

Timing and fidelity limits
--------------------------

Virtual cycles are derived from local sound/graphics analysis and representative gameplay block budgets; they do not depend on host execution speed. Hires helpers accumulate costs, drained before sound or interval completion. Exact helper formulas, page crossings, and rounded budgets are documented in `sources/analysis/TIMING.md`. Host sleeps target absolute deadlines at nominal NTSC 1,023,000 Hz, including multiple checkpoints per round. Headless execution does not sleep.

Sound countdown and scanline lookup costs are locally instruction-derived. Graphics loops account for alignment and indexed page crossings, with representative setup costs. Gameplay dispatch, movement, guard route scans, timer scans, and polling use rounded local budgets. They omit some caller/setup and branch-path differences. Original clock stretching, disk delays, loading/iris timing, and complete transition presentation are not modeled. Deterministic round-trip timing proves accounting consistency, not cycle accuracy against an Apple II.

The original attract/demo mode, level editor, joystick devices, iris effect, and spinning game-over presentation are outside this keyboard gameplay milestone. RGB artifacts and host speaker filtering are approximations. Sound-buffer overflow and impossible resurrection boards fail cleanly. These differences remain explicit so later original-machine validation does not mistake internal replay success for original equivalence.

Validation coverage
-------------------

`make check` passes formatting, warning-free Clippy, and 42 behavioral tests. Tests include all 150 original boards through loading and guarded execution, serialized full-session capture/reexecution on each board, repeated keyboard delivery, persistent motion and gravity polling, gold pickup, thirteen-call digging, hole refill, guard scheduling/carrying/resurrection, death/restart/game over, level advancement, pause/speed/mute, high-score entry, canonical contacts, and negative state/timing/configuration traces. CLI tests capture and replay real files, reject altered timing, and replay a trace produced by SDL with dummy audio/video drivers. No display was available for a headed visual check.


MAME interface proof of concept
-------------------------------

`sources/mame-poc/` exercises the installed Debian MAME 0.276 executable without changing MAME or the Rust port. `make mame-poc` runs Python type checks and two repeated executions of each probe. It is optional and separate from Rust checks. Raw snapshots and full process logs remain under `artifacts/mame-poc/apple2ee/`. These records are debugger capability evidence, not gameplay traces in the shared format and not port-equivalence validation.

The launcher creates isolated MAME configuration directories and starts enhanced Apple IIe (`apple2ee`) with no video/audio presentation, no throttle, and the noninteractive debugger. It uses the installed real ROMs, whose MAME audit passes; checksum failures and process warnings fail the probe. An explicit ROM search path can override MAME configuration. Headers record and verify the machine and W65C02 CPU identity. Enhanced IIe is the selected validator platform; its CMOS CPU can differ from the original NMOS timing analysis, so later interval tolerances must distinguish CPU-model effects. Ordinary ROM/disk boot is not exercised by either current mode.

Lua waits at the reset-vector execution breakpoint before loading RAM or setting PC. Breakpoint creation supplies explicit condition and action strings: the 0.276 binding crashes when these are omitted. RAM inspection uses mapped reads below $C000; `read_direct_u8` cannot obtain pointers for this driver's handler-backed RAM and returns zero. The CPU's debugger SP representation includes the stack page, so a stack reset uses $01FF. A stopped CPU's `totalcycles` debugger symbol is read through a tagged printf line. Repeated cycle reads around each snapshot assert that observer work advances no CPU cycles.

Each breakpoint stops execution before the boundary instruction. A periodic Lua callback samples the stopped state, optionally delivers the next key, and resumes with `cpu.debug:go()`. Periodic scheduling affects host overhead, not boundary choice or virtual time. Persistent read/write tap objects override C000 values, clear the synthetic availability latch on either read or write to C010, and count C030 reads without replacing their hardware behavior. Fresh high-bit keyboard observations are captured; unchanged low-bit latch reads are omitted.

The synthetic loop at $1000 executes INC zero-page, LDA absolute, STA zero-page, BIT absolute, LDA absolute, and JMP absolute. Eight intervals each cost 23 cycles, increment a RAM counter, observe repeated delivery of the same key as separate inputs, retain low-bit codes after acknowledgement, and access the speaker once. Both launches produce identical parsed snapshots and timing.

The original-code probe loads the reference image at $0000-$1FFF and $5F00-$BFFF, supplies original level 1 at $9F00, establishes loader preconditions, and enters INIT_GAME_DATA at $6056. GAME_MODE zero selects the original in-memory level-copy path, avoiding disk access. Original board decoding, graphics helpers, iris, actor initialization, and scheduler setup execute. At the first $60E4 main-loop breakpoint, GAME_MODE changes to two for ordinary keyboard play. Initial loading/iris cycles are excluded from interval deltas. Gameplay instructions are not patched or translated by this probe.

The twenty-round schedule delivers right at round zero, stop at ten, and dig left at twelve. Player state starts at column 14, row 14, adjustments 2/2, moves to column 16, and stays there through stopping and eight dig updates. Three guards move, six gold tiles initialize, and continuing digging skips keyboard polling. Both enhanced-IIe original-code launches produce identical snapshots with 1,044,485 gameplay cycles in total, individual intervals from 40,157 to 66,737 cycles, and 200 speaker reads. This short schedule also matches the prior II+ cycle totals; that does not establish identical CPU timing for all paths. Transitions, full trace mapping, timing tolerances, mismatch diagnosis, and real disk boot remain unimplemented in the proof of concept.

The current reference reconstructs the Apple II Reset Vector crack. A preferred Apple II 4am crack must be compared against its code and level data before adopting it as the boot baseline. The user-supplied `Lode Runner v1.0 (4am crack)` archives are Macintosh media, not Apple II: the Disk Copy image has 409,600 data bytes, 9,600 tag bytes, and a Macintosh File System signature, and the accompanying screenshot names the Macintosh version. Those archives are left unchanged and excluded from the current proof. Disk-boot work is pending an Apple II image.
