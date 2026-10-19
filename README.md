Lode Runner
===========

    make
    make run

The executable is `target/release/loderunner`. Rust 1.88 or newer, SDL2 development libraries, and pkg-config are required. The sibling `../crt` crate supplies the default NTSC/CRT pipeline; keep it checked out beside this project. The game in `src/` uses native game state, composed sprites, and event timing. `src/trace/` defines gameplay observations; `src/mame/` validates them against the original Apple II program. Reference assets and analysis remain in `sources/`.


Rendering and window
--------------------

The default renderer sends composed scene ink through the `crt` library's Apple II HGR encoder, NTSC decoder, and CRT beam reconstruction. `--renderer simple` selects the existing digital artifact-color output; Tab switches between the two while playing or replaying. Renderer selection does not change gameplay, input recordings, or model timing.

The window starts at 5× native size: 1400×960. Keys 1 through 5 snap to 280×192 through 1400×960. Both output modes retain the native 280:192 aspect ratio and center the picture with black bars inside manually resized windows. CRT reconstruction uses the actual drawable pixel dimensions, including high-DPI displays.

A grouped keyboard-command summary prints at startup. Window shortcuts are reserved for presentation and do not enter gameplay recordings.

    make run ARGS='--renderer simple'


Record, replay, and validate
---------------------------

| Command                          | Behavior                                               |
|----------------------------------|--------------------------------------------------------|
| `loderunner`                     | Interactive play with display and sound                 |
| `loderunner --record FILE`       | Record consumed gameplay inputs                        |
| `loderunner --replay FILE`       | Play an input recording with display and sound          |
| `loderunner --trace FILE`        | Save a trace and validate live against MAME              |
| `loderunner --verify FILE`       | Verify a trace against the game and MAME, headlessly     |
| `loderunner --validate`          | Validate live play without a trace destination           |

Add `--headless` for silent, full speed execution or `--display` for display, sound, and normal pacing. Both work with input replay and trace verification. `--unpaced` removes display pacing. `--record` can accompany tracing, validation, or playback with a separate output path.

    make run ARGS='--record play.input'
    make run ARGS='--replay play.input --headless'
    make run ARGS='--trace play.trace'
    make validate TRACE=play.trace
    make run ARGS='--verify play.trace --display'

Input recordings contain initial configuration, consumed keys indexed by interval, and an explicit endpoint. They contain no expected gameplay state or timing. Playback runs only the game unless validation is requested. Trace verification compares independently generated state, events, and timing, and validates the same executed intervals against MAME. Quitting early reports incomplete verification. Recording replaces its destination on exit; completed prefixes are also saved after execution or validation errors.

`--level N` selects a starting board for live play. `--rounds N` bounds execution and is required for headless live play. All live modes wait for a starting key. `--input ROUND:HEX` schedules an original high-bit keyboard code at a zero-based interval for development:

    make run ARGS='--headless --rounds 120 --input 0:cc --record run.input'

Playback uses its embedded level sectors and initial high scores. Live play loads `highscores.json` when present and saves changed scores after successful completion. Playback never reads or writes that file.


MAME options
------------

Validation requires MAME, enhanced Apple IIe ROMs (`apple2ee`), Python 3, and this source checkout. The validator boots a private copy of the supplied 4am disk and injects only observed keys. MAME 0.276 was used for the original validation work; the consolidated application also passes its external checks with MAME 0.289.

`--mame PATH`, `--rompath PATH`, `--work DIR`, and `--timeout-seconds N` apply to every validation mode. The default executable is `mame` on PATH; `--mame` selects another installation. Packaged Lua plugins are discovered beside the MAME installation; `--pluginspath DIR` overrides that location. Work directories must be empty; otherwise artifacts go under `artifacts/mame-validation/`.

Gameplay timing allows 2,000 cycles plus 10% of the port interval. `--tolerance-cycles N`, `--tolerance-percent N`, and `--transition-tolerance-cycles N` override timing allowances. State and events compare exactly. See [the validation interface](src/mame/README.md) and [NOTES.md](NOTES.md) for initialization and fidelity limits.


Controls
--------

*   Arrows or I/J/K/L move. Space stops; releasing a key retains the movement command.
*   Z/U digs left; X/O digs right.
*   Escape pauses/resumes; Return shows high scores.
*   Ctrl+A sacrifices; Ctrl+R ends the game; Ctrl+S mutes; Ctrl+Left/Right changes speed.
*   Ctrl+Q quits and finishes recording.


Development
-----------

    make lint
    make test
    make test-presentation
    make test-render-sdl
    make mame-validation ROMPATH="$HOME/mame/roms"

Portable tests cover all 150 boards, gameplay transitions, capture/reexecution, input recordings, and CLI behavior. `make mame-validation` runs the external machine suite; ordinary tests leave it ignored. `make mame-poc` runs the separate debugger capability probes. `make timing-fit TIMING_MEASUREMENTS=FILE` refits the model from retained measurements.

Make drives incremental Cargo builds. `make mostlyclean` preserves the release executable; `make clean` removes build outputs too. Neither removes recordings, high scores, or reference assets. Provenance and licensing remain in `sources/lode_runner_reveng/`.
