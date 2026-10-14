MAME trace validation
=====================

    make -C .. all
    ../target/debug/lode-mame-validation validate ../run.json
    make check

The CLI reads a port-generated version-2 trace, boots the pinned Apple II 4am disk on `apple2ee/w65c02`, delivers only observed keys, and compares independent object observations. It uses the installed MAME Lua/debugger interfaces without a custom build. Debian MAME 0.276 and the user's enhanced IIe ROMs were exercised. The source checkout and Python 3 are required for disk preparation; no managed Python dependencies are needed by the validator.

Configuration and execution
----------------------------

The validator copies level sectors and initial high scores from trace configuration into a private extracted disk. It preserves the disk's program and marker data. Starting level is an initialization parameter. Splash Space and keyboard-mode Ctrl+K establish the starting screen; they are excluded from gameplay inputs. When configuration disables the ready-screen wait, the adapter supplies keyboard-mode Ctrl+K after each load.

The approved `fixed-v1` profile sets player-facing `$05`, guard-facing `$0C91..$0C95`, and guard scheduler phase `$64` to zero once before the first game initialization. It removes loader residue from the comparison. Later loads and new games retain the original flags. `--original-initialization` leaves the 4am loader residue intact and can therefore fail the port's initial-state comparison.

Raw RAM and source-path observations go through `trace::apple2::Apple2Observer`, then through the same canonical round observer as native capture. Expected changes never drive MAME execution. There is no emulator trace-generation command or recording-playback feature.

Timing and artifacts
--------------------

`--tolerance-cycles N` and `--tolerance-percent N` set the gameplay allowance: absolute cycles plus that percentage of the port duration. `--transition-tolerance-cycles N` overrides the separate transition allowance. Defaults are 2,000 cycles plus 10% for Playing-to-Playing intervals and 20,000,000 cycles otherwise. Every report records the selected allowances and each expected/actual duration. The first boundary establishes a fresh cycle origin; initial boot/setup duration is excluded.

`--work DIR` must identify an empty directory. Without it, a unique directory is created under `artifacts/mame-validation/`. `--mame PATH`, `--rompath PATH`, and `--timeout-seconds N` override executable, ROM search path, and host deadline. Expected CLI errors produce a concise message and a failure exit status.

Each run retains the private disk, adapter configuration, original trace, raw samples and control messages, MAME output, and completion metadata. Successful runs add `report.json`. The first gameplay or timing mismatch adds `difference.json`, `failure.sta`, and `checkpoint.json`. The checkpoint contains phase, attempt, PC, CPU cycles, and the emulated keyboard latch, which is external to MAME's save state. The preserved trace can reproduce the input sequence in a fresh run; MAME's debugger `stateload` command can inspect the stopped machine directly. Use the private disk when restoring it.

Coverage and limits
--------------------

`make test` runs the external suite; ordinary workspace tests leave machine tests ignored when MAME/ROMs are unavailable. Fixtures cover seven original boards with guards, digging and refill, delayed input during falling, menus, death/restart, completion, initials, and game-over restart. Negative checks supply structurally valid false state and false timing and require rejection with a machine checkpoint.

Six selected boards pass the default gameplay timing allowance. Level 150 deliberately demonstrates a default-tolerance failure and then checks state/events with an explicit 100% allowance. No state/event tolerance is applied. Guard burial/resurrection and menu inactivity timeouts are not exhaustively cross-validated by this suite. Original attract mode, editor, and joystick operation are outside the native keyboard port and are not accepted as additional supported workflows.

The level-150 spike comes from a falling-sound pitch counter left as loader residue by the current initialization profile. A controlled run normalizing `$A4` to Rust's `$20` passes all 90 intervals at the default allowance. The earlier guard-search explanation was incorrect; see [the timing analysis](../sources/analysis/TIMING.md). Executable normalization and falling-state retention on Rust reloads remain unchanged by that investigation.

The adapter uses the documented [Lua debugger interface](https://docs.mamedev.org/luascript/ref-debugger.html) and [memory taps](https://docs.mamedev.org/luascript/ref-mem.html). The installed release's [headless debugger implementation](https://github.com/mamedev/mame/blob/mame0276/src/osd/modules/debugger/none.cpp) automatically resumes when its callback returns. The adapter keeps that callback at the stopped boundary until an atomic control file arrives, so validator latency never advances emulated time.
