Event-timed port
================

Build with `make -C port`; run `target/debug/lode-port play`. Capture with `target/debug/lode-port capture run.json --rounds 120 --input 0:cc`, then reexecute with `target/debug/lode-port replay run.json`.

`Game::play_round` expresses the gameplay order: player, sound, exit, resurrection and holes, sound, guards. Actors use cell/alignment positions, typed commands and explicit guard states. Digging follows one chronological sequence for either side; notes use a stack and scores use decimal arithmetic. Packed rendering and the reference assets remain unchanged. Functions record counts and typed draw/note/section work; the model owns every cost calculation. Each complete interval commits one duration, schedules speaker edges, submits audio/graphics and paces against the shared clock. The [workspace notes](../NOTES.md) specify the execution flow and remaining fidelity limits.

`src/model.json` retains the calibrated gameplay coefficients and adds polling-batch and measured pause-entry terms. Graphics and sound inputs are evaluated centrally in `event_timing.rs`; they are not measured by gameplay or graphics helpers. Non-gameplay phases use this same model. Reloads include the executed new-board rendering, unlike the baseline's omitted initialization costs. Audio corrections adjust gaps while retaining note pitch/countdown behavior.

Run `make -C port check` for portable checks and `make -C mame-validation port-test` for original-machine validation. Differential tests compare gameplay, calibrated costs and every packed hires byte after 90 turns on all 150 boards. Extra scenarios protect both dig sequences, menus, burial/resurrection and retained guard facing across boards with different guard counts. Isolated note waveforms also match the baseline. No production dependency on `port-original/` is required.
