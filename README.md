Lode Runner
===========

    make
    make install
    loderunner

The executable is `target/release/loderunner`. Rust 1.88 or newer, Python 3, SDL2 development libraries, and pkg-config are required. The sibling `../crt` crate supplies the default NTSC/CRT pipeline; keep it checked out beside this project. SQLite is bundled. `make` incrementally builds the binary and a catalog containing all 434 levels from Total Recall's five packs. `make install` copies the binary to `~/.local/bin` and the catalog to `$XDG_DATA_HOME/loderunner/levels.sqlite3` (default `~/.local/share/loderunner/levels.sqlite3`). Override `BINDIR` and `DATADIR`, or use `DESTDIR` for staging. Installation never replaces player data or configuration.

The installed binary runs from any working directory. Player history goes in `$XDG_DATA_HOME/loderunner/player.sqlite3`; preferences go in `$XDG_CONFIG_HOME/loderunner/config.json` (default `~/.config/loderunner/config.json`). System catalogs can also be found through `XDG_DATA_DIRS`. The game remembers renderer, fitted game size, movement style, mute, speed, selected pack and the last level played in each pack. Normal play automatically records attempts, preserving personal best and newest wins and pruning short/old failures. No Total Recall demos are imported; the catalog reserves a place for future supplied solutions.

Faithful emulation covers core level gameplay: from the start of active play through the committed death or level-completion boundary. This includes movement, digging, guards, gold, holes, exits, collisions, in-level scoring, sound, and gameplay timing. Level selection, ready screens, pause, death/completion presentation and bonuses, reloads, and host controls are application behavior outside that fidelity guarantee. High score tracking and initials entry are absent.

Lives are unlimited. USED counts lives used on the current level, starting at one and resetting on level changes. Completion awards an animated 1,500-point bonus with the original tune; death plays the original capture tune. A circular iris closes for half a second and opens for half a second around immediate board preparation. The ready player blinks until a fresh keypress. [Transition timing](sources/analysis/TRANSITIONS.md) comes from one-time original-machine measurements and stays outside gameplay tracing and validation.


Rendering and window
--------------------

The default renderer sends composed scene ink through the `crt` library's Apple II HGR encoder, NTSC decoder, and CRT beam reconstruction. `--renderer simple` selects digital artifact-color output; C switches between the two while playing or replaying. Renderer selection does not change gameplay, input recordings, or model timing.

Native presentation automatically uses the sibling `crt/native` GPU backend when supported, with the regular CPU pipeline as fallback. `CRT_BACKEND=cpu make run` forces CPU rendering; `CRT_BACKEND=gpu make run` requires a hardware GPU. `make smoke` exercises the headed presentation path, resize, mode switches, and GPU/CPU image agreement. Small retained updates can be faster on the CPU.

The initial default window is 4× native size: 1120×768. Ctrl+=/Ctrl++ and Ctrl+- snap up/down to integer multiples of 280×192. Both output modes retain the native aspect ratio and center the picture with black bars inside manually resized windows. The saved size excludes those bars, so reopening fits the game area. CRT reconstruction uses actual drawable pixel dimensions, including high-DPI displays.

A grouped keyboard-command summary prints at startup. Window shortcuts are reserved for presentation and do not enter gameplay recordings.

    make run ARGS='--renderer simple'


Record, replay, and validate
---------------------------

| Command                          | Behavior                                               |
|----------------------------------|--------------------------------------------------------|
| `loderunner`                     | Interactive play with display and sound                 |
| `loderunner --record FILE`       | Record effective gameplay inputs                        |
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

Explicit file recordings are development tools containing initial configuration, effective Apple II commands indexed by interval, and an explicit endpoint. Capture omits sound/speed controls, repeated directions, and ineffective actions; it supplies Space only when stopping a retained direction is necessary. Pause/resume and effective session navigation remain replayable. Diagnostic traces retain every consumed key and the original modeled timing. They contain no expected gameplay state or timing. These workflows bypass player databases and saved preferences. Database recordings additionally retain the reload history needed to replay an individual attempt independently. Trace verification compares independently generated state, events, and timing, and validates the same executed intervals against MAME. Quitting early reports incomplete verification. File recording replaces its destination on exit; completed prefixes are also saved after execution or validation errors.

`--level N` selects a starting board for live play. `--rounds N` bounds execution and is required for headless live play. Interactive live play waits for a starting key. Playback, headless execution, and development input schedules start ready levels automatically; ready waits are never recorded. The key that starts interactive play also acts as its first gameplay command and is recorded if it affects gameplay. `--input ROUND:HEX` schedules an original high-bit keyboard code at a zero-based gameplay interval for development:

    make run ARGS='--headless --rounds 120 --input 0:cc --record run.input'

Playback uses its embedded level sectors. The game neither reads nor writes high score files. Traces use format version 4 and input recordings version 2; older formats are unsupported. Traces retain only instantaneous board checkpoints between gameplay segments. Playback runs presentation sequences but dismisses ready immediately.


MAME options
------------

Validation requires MAME, enhanced Apple IIe ROMs (`apple2ee`), Python 3, and this source checkout. The validator boots a private copy of the supplied 4am disk and injects only observed keys. MAME 0.276 was used for the original validation work; the consolidated application also passes its external checks with MAME 0.289.

`--mame PATH`, `--rompath PATH`, `--work DIR`, and `--timeout-seconds N` apply to every validation mode. The default executable is `mame` on PATH; `--mame` selects another installation. Packaged Lua plugins are discovered beside the MAME installation; `--pluginspath DIR` overrides that location. Work directories must be empty; otherwise artifacts go under `artifacts/mame-validation/`.

Gameplay timing allows 2,000 cycles plus 10% of the port interval. `--tolerance-cycles N` and `--tolerance-percent N` override this allowance. Gameplay state and events compare exactly; startup and transitions are excluded. See [the validation interface](src/mame/README.md) and [NOTES.md](NOTES.md) for initialization and fidelity limits.


Controls
--------

*   Arrows move; Space stops; Z/X dig; P pauses/resumes. Ctrl+K toggles Commodore movement, where releasing arrows stops movement.
*   Ctrl+R sacrifices a life and restarts; Ctrl+S mutes; Ctrl+Left/Right changes speed. During demos, these speed keys adjust a separate saved playback multiplier (default 2×, range 1–16×).
*   PgUp/PgDown select adjacent levels. Ctrl+PgUp/PgDown cycle packs and resume each pack's most recent level. Selections use the iris and reset lives used.
*   C toggles CRT/simple; Ctrl+=/Ctrl+- changes scale; Q quits.
*   D from ready, or thirty seconds idle, starts a demo when a supplied solution or personal best exists. Left/Right cycle saved recordings; Escape/Space returns to ready.

Demo zero is the supplied solution, or the player's personal best when no solution is supplied. Other wins follow newest-first, and failures use negative numbers with -1 newest. Finishing a demo randomly selects another saved recording for the same level and continues. Demo playback never creates player recordings. Wins and useful failures normally remain for thirty days, with a best useful run per session retained for ninety days; personal best and newest win always remain. Failures shorter than one third of the best winning duration are removed (ten seconds before a win exists).


Development
-----------

    make lint
    make test
    make test-presentation
    make test-render-sdl
    make mame-validation ROMPATH="$HOME/mame/roms"

Portable tests cover all 150 boards, gameplay transitions, capture/reexecution, input recordings, and CLI behavior. `make mame-validation` runs the external machine suite; ordinary tests leave it ignored. `make mame-poc` runs the separate debugger capability probes. `make timing-fit TIMING_MEASUREMENTS=FILE` refits the model from retained measurements.

Make drives incremental Cargo builds. `make mostlyclean` preserves the release executable; `make clean` removes build outputs too. Neither removes recordings or reference assets. Provenance and licensing remain in `sources/lode_runner_reveng/`.

Optional host timing capture runs the real paced graphics/audio pipeline:

    make performance TIMINGS=target/gpu-crt.csv
    make performance BACKEND=cpu TIMINGS=target/cpu-crt.csv
    make performance RENDERER=simple TIMINGS=target/gpu-simple.csv

Each run uses the same 240-round movement/dig schedule and prints graphics mean, p99 and maximum separately for gameplay and transitions. The CSV also records audio work, total host work before pacing, output cycles and host-call intervals. `LODERUNNER_FRAME_TIMINGS=PATH` enables capture for ordinary live or recorded displayed runs. Existing output files are refused. Graphics timing includes scene preparation and native presentation, but GPU execution can remain queued; it does not measure physical scanout. The sibling CRT benchmark’s `GAME=hgr-damage` compares complete-row and bounded HGR submission with output equality, while its paced `stress` command measures completed rendering bursts.
