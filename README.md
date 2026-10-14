Lode Runner Rust port
====================

Build and run
-------------

Requires Rust 1.88 or newer and SDL2 development libraries available to the linker.

    make
    target/debug/lode-port play --level 1 --trace play.json
    target/debug/lode-original replay play.json

Replay executes headlessly without sleeps and compares fresh gameplay transitions and relative round timing. MAME independently validates port-generated traces against the original Apple II code. See [NOTES.md](NOTES.md) for execution rules and fidelity limits.

Controls
--------

*   Arrows or I/J/K/L: move. Space: stop; releasing a key retains the original movement command.
*   Z/U: dig left. X/O: dig right.
*   Escape: pause/resume. Return: high scores.
*   Ctrl+A: sacrifice. Ctrl+R: end game. Ctrl+S: mute. Ctrl+Left/Right: speed.
*   Ctrl+Q: exit and save the optional trace and changed `highscores.json`.

Trace development
-----------------

    target/debug/lode-port capture run.json --rounds 500 --input 0:CC --input 40:CA --input 80:D5
    target/debug/lode-port replay run.json
    make check

Input rounds are zero-based and codes are original high-bit hexadecimal keyboard codes. Headless capture starts immediately; SDL play waits for an initial key. `--rounds N` can bound SDL play. `preview N` displays a static original board. Capture schedules are development tools, not the future compact recording feature.

Workspace
---------

`sources/` holds reference assets and analysis, `trace/` defines canonical snapshots and machine observation, `port-original/` retains the procedural baseline, `port/` uses event timing exclusively, and `mame-validation/` drives external validation. Portable tests include capture/reexecution on all 150 original boards and SDL dummy-device integration. `make -C port check` checks the event-timed port; `make -C mame-validation port-test` runs its external machine suite.

`make mame-poc` runs the optional [MAME debugger proof of concept](sources/mame-poc/README.md), tested with Debian MAME 0.276 and installed enhanced Apple IIe ROMs (`apple2ee`). It checks repeated raw snapshots, cycle counts, keyboard injection, and original gameplay boundaries, including normal boot from the supplied Apple II 4am disk. It does not compare the Rust port against the original machine.

`make mostlyclean` removes incremental build intermediates; `make clean` removes build outputs. Neither removes trace files or high scores. Reference provenance and licensing remain in `sources/lode_runner_reveng/`.


Validate against MAME
---------------------

Requires MAME and installed enhanced Apple IIe ROMs (`apple2ee`); tested with Debian MAME 0.276. The supplied Apple II 4am archive remains under `sources/`. Validation boots a private disk copy headlessly and supplies recorded observed inputs.

    make
    target/debug/lode-mame-validation validate run.json
    make mame-validation

`--mame PATH` and `--rompath PATH` override installation defaults. `--work DIR` chooses an empty artifact directory. Reports and failure checkpoints otherwise go under `artifacts/mame-validation/`.

Gameplay timing defaults to 2,000 cycles plus 10% of the port interval; `--tolerance-cycles N` and `--tolerance-percent N` change it. Transition timing has a separate 20-million-cycle allowance for original disk/loading/presentation delays. State and events compare exactly. All seven selected original boards, including level 150, pass the default gameplay allowance with the `fixed-v2` initialization profile. Rust retains falling-sound state across reloads and new games. See [the timing diagnosis](sources/analysis/TIMING.md) and [the validation interface](mame-validation/README.md).


Compare timing estimates
------------------------

`lode-original capture legacy.json --rounds 120 --input 0:cc --event-trace model.json` writes both baseline timelines; `lode-original replay model.json --event-model` reexecutes its event-based estimate. `lode-port` uses model timing for capture, replay and live play, with one commit per complete interval. [The event model](sources/analysis/EVENT_TIMING.md) documents factors, experiments, results, and the `make -C mame-validation timing-measure`, `timing-fit`, and `timing-verify` targets.
