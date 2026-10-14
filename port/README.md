Event-timed port
================

Build with `make -C port`; run `target/debug/lode-port play`. Capture with `target/debug/lode-port capture run.json --rounds 120 --input 0:cc`, then reexecute with `target/debug/lode-port replay run.json`.

Gameplay retains the procedural function boundaries and reference assets. Functions record counts and typed draw/note/section work; the model owns every cost calculation. Each complete interval commits one duration, schedules speaker edges, submits audio/graphics and paces against the shared clock. There is no legacy clock, local block budget or phase fallback. The [workspace notes](../NOTES.md) specify the execution flow and remaining fidelity limits.

`src/model.json` retains the calibrated gameplay coefficients and adds polling-batch and measured pause-entry terms. Graphics and sound inputs are evaluated centrally in `event_timing.rs`; they are not measured by gameplay or graphics helpers. Non-gameplay phases use this same model. Reloads include the executed new-board rendering, unlike the baseline's omitted initialization costs. Audio corrections adjust gaps while retaining note pitch/countdown behavior.

Run `make -C port check` for portable checks and `make -C mame-validation port-test` for original-machine validation. Differential tests compare gameplay over 90 turns on all 150 boards, calibrated gameplay costs, packed graphics and note waveforms with the baseline. No production dependency on `port-original/` is required.
