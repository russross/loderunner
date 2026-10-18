MAME validation library
=======================

    make
    target/release/loderunner --verify run.trace
    target/release/loderunner --validate --mame /path/to/mame
    make mame-validation ROMPATH="$HOME/mame/roms"

Run these commands from the repository root. The application owns the CLI; this library exports `Options`, `validate(&Trace, &Options)`, and `validate_stream(&Trace, &Options, producer)`. The stream producer returns a committed `(Round, duration, Snapshot)` or end-of-stream. Both paths boot the pinned 4am disk on `apple2ee/w65c02`, deliver only observed keys and compare independently observed objects. No custom MAME build is required. Python 3 prepares a private disk without managed dependencies. The original validation work used Debian MAME 0.276 and enhanced IIe ROMs. The consolidated library and CLI checks also pass with Homebrew MAME 0.289.

Configuration and execution
----------------------------

The validator copies level sectors and initial high scores from trace configuration into a private extracted disk. It preserves the disk's program and marker data. Starting level is an initialization parameter. Splash Space and keyboard-mode Ctrl+K establish the starting screen; they are excluded from gameplay inputs. When configuration disables the ready-screen wait, the adapter supplies keyboard-mode Ctrl+K after each load.

The approved `fixed-v2` profile sets player-facing `$05`, guard-facing `$0C91..$0C95`, and guard scheduler phase `$64` to zero, and falling-pitch `$A4` and landing-sound flag `$9B` to `$20`, once before the first game initialization. A nonzero `$9B` represents no pending landing sound, matching Rust's initial false flag. The profile removes loader residue from the comparison. Later loads and new games retain these values as modified by gameplay; the profile is not reapplied. `--original-initialization` leaves the 4am loader residue intact and can therefore fail the port's initial-state comparison.

Raw RAM and source-path observations go through `trace::apple2::Apple2Observer`, then through the same canonical round observer as native capture. Expected changes never drive MAME execution. Input recording playback belongs to the root game session; the validator receives only generated gameplay intervals.

Timing and artifacts
--------------------

`--tolerance-cycles N` and `--tolerance-percent N` set the gameplay allowance: absolute cycles plus that percentage of the port duration. `--transition-tolerance-cycles N` overrides the separate transition allowance. Defaults are 2,000 cycles plus 10% for Playing-to-Playing intervals and 20,000,000 cycles otherwise. Every report records the selected allowances and each expected/actual duration. The first boundary establishes a fresh cycle origin; initial boot/setup duration is excluded.

`--work DIR` must identify an empty directory. Without it, a unique directory is created under `artifacts/mame-validation/`. `--mame PATH`, `--rompath PATH`, `--pluginspath DIR`, and `--timeout-seconds N` override executable, ROM search path, Lua plugin directory, and host deadline. The executable defaults to `mame` on PATH. Packaged plugins are discovered beside the executable installation; explicit relative executable, ROM and plugin paths resolve from the caller's directory. External Make checks accept `MAME` and `ROMPATH` overrides. Expected CLI errors produce a concise message and a failure exit status.

Each run retains the private disk, adapter configuration, original trace, raw samples and control messages, MAME output, and completion metadata. Successful runs add `report.json`. The first gameplay or timing mismatch adds `difference.json`, `failure.sta`, and `checkpoint.json`. The checkpoint contains phase, attempt, PC, CPU cycles, and the emulated keyboard latch, which is external to MAME's save state. The preserved trace can reproduce the input sequence in a fresh run; MAME's debugger `stateload` command can inspect the stopped machine directly. Use the private disk when restoring it.

Coverage and limits
--------------------

`make mame-validation` runs the external suite from the root; `make test TEST_ARGS="--test mame airborne -- --include-ignored"` selects the falling-restart regression. Ordinary workspace tests leave machine tests ignored when MAME/ROMs are unavailable. Fixtures cover seven original boards with guards, digging and refill, delayed input during falling, menus, death/restart, completion, initials, and game-over restart. Negative checks supply structurally valid false state and false timing and require rejection with a machine checkpoint.

All seven selected boards, including level 150, pass the default gameplay timing allowance. No state/event tolerance is applied. The airborne-restart regression exercises five deaths and a new game, then compares native falling-pitch and landing-sound state directly with every retained raw MAME sample. These execution counters remain outside the gameplay trace model and never drive emulator execution. A custom fixture exercises delayed guard entry into a refilling hole and resurrection; menu inactivity timeouts are not exhaustively cross-validated. Original attract mode, editor, and joystick operation are outside the native keyboard port and are not accepted as additional supported workflows.

The former level-150 spike came from a falling-sound pitch counter left as loader residue by `fixed-v1`. Its correction is initialization agreement and retained state, rather than a larger timing allowance; see [the timing analysis](../../sources/analysis/TIMING.md).

The adapter uses the documented [Lua debugger interface](https://docs.mamedev.org/luascript/ref-debugger.html) and [memory taps](https://docs.mamedev.org/luascript/ref-mem.html). The installed release's [headless debugger implementation](https://github.com/mamedev/mame/blob/mame0276/src/osd/modules/debugger/none.cpp) automatically resumes when its callback returns. The adapter keeps that callback at the stopped boundary until an atomic control file arrives, so validator latency never advances emulated time.


Timing analysis
---------------

`Options::profile_timing` enables original source-entry observations for callers performing timing experiments. The game embeds its calibrated model in `src/model.json`. Root `make timing-fit` refits from retained measurement artifacts; the procedural milestone measurement driver is retired. See [the event timing specification](../../sources/analysis/EVENT_TIMING.md) for historical calibration evidence.
