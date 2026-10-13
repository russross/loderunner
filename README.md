Lode Runner Rust port
====================

Build and run
-------------

Requires Rust 1.88 or newer and SDL2 development libraries available to the linker.

    make
    target/debug/lode-original play --level 1 --trace play.json
    target/debug/lode-original replay play.json

Replay executes headlessly without sleeps and compares fresh gameplay transitions and relative round timing. The same-implementation trace round-trip milestone is implemented; original-machine equivalence remains unvalidated. See [SEMANTICS.md](SEMANTICS.md) for execution rules and fidelity limits.

Controls
--------

*   Arrows or I/J/K/L: move. Space: stop; releasing a key retains the original movement command.
*   Z/U: dig left. X/O: dig right.
*   Escape: pause/resume. Return: high scores.
*   Ctrl+A: sacrifice. Ctrl+R: end game. Ctrl+S: mute. Ctrl+Left/Right: speed.
*   Ctrl+Q: exit and save the optional trace and changed `highscores.json`.

Trace development
-----------------

    target/debug/lode-original capture run.json --rounds 500 --input 0:CC --input 40:CA --input 80:D5
    target/debug/lode-original replay run.json
    make check

Input rounds are zero-based and codes are original high-bit hexadecimal keyboard codes. Headless capture starts immediately; SDL play waits for an initial key. `--rounds N` can bound SDL play. `preview N` displays a static original board. Capture schedules are development tools, not the future compact recording feature.

Workspace
---------

`sources/` holds reference assets and analysis, `trace/` defines canonical snapshots and trace validation, and `port-original/` contains the procedural port and devices. The 42 behavioral tests include capture/reexecution on all 150 original boards and SDL dummy-device integration. Full emulator trace instrumentation is deferred.

`make mame-poc` runs the optional [MAME debugger proof of concept](sources/mame-poc/README.md), tested with Debian MAME 0.276. It checks repeated raw snapshots, cycle counts, keyboard injection, and original gameplay boundaries using synthetic ROM stand-ins. It does not compare the Rust port against the original machine.

`make mostlyclean` removes incremental build intermediates; `make clean` removes build outputs. Neither removes trace files or high scores. Reference provenance and licensing remain in `sources/lode_runner_reveng/`.
