Event timing model
==================

Native port
-----------

The native `port/` uses `port/src/model.json`, fitted by `fit_port_model.py` from the retained original-machine measurements. `make -C port timing-fit` reproduces it and `port-model-report.json`. Eighteen whole-operation features describe rounds, input, player attempts, selected guards, digs, scores, holes, drawing and guard-search extents. No branch probes, distance-class events or best-route updates remain in native gameplay. Guard routing returns a direction plus search extents without access to a clock. Fixed graphics and note-duration terms remain reference-fidelity inputs; presentation does not measure or advance game time.

Training uses 2,054 intervals, validation selects ridge regularization using 480 intervals, and 360 held-out intervals report the frozen result. Held-out RMSE is 444 cycles, maximum absolute error 1,390 cycles, maximum relative error 2.775%, and total drift -0.423%. The earlier branch-heavy model achieved 342-cycle RMSE; procedural timing achieved 1,699. The native choice removes control-flow constraints at a modest precision cost. Original-machine state and occurrence checks remain exact.

Native intervals always commit through the model. Polling costs 5,000 cycles; the pause correction is derived so Round + Input + Pause equals the independently measured 187 cycles. Run retains only the latest timing record. Sound synthesis preserves original waveform/countdown rules; model corrections adjust inter-note gaps. Replays require the coefficient set used to capture them, so traces captured with earlier coefficients do not necessarily replay under this model.


Baseline calibration experiment
-------------------------------

The remainder describes the retained `port-original/` calibration and reproduction workflow, including its procedural clock and branch-heavy comparison model.

`event-model.json` contains the runtime coefficients. `event-model-report.json` records the measurement digest, selected factors, validation searches, stopping decision, and independent test results. Raw measurements, paired traces, and fresh validation reports are under `artifacts/event-timing-final/`. The local clock, graphics formulas, sound edges, and SDL pacing continue to use the timing described in `TIMING.md`.


Turn measurement and storage
----------------------------

`Run::step_with_checkpoint` clears two timing journals before executing an interval. Gameplay helpers add counts to `Clock.events`; graphics helpers add counts and their existing local cycle formulas to `Hires.events`. Sound notes add their instruction-derived costs once; sound sections add only the remaining section/delay costs. Draining pending graphics cycles into the legacy clock does not discard the separate graphics event journal. After the entire interval, the journals merge into `TimingInterval { phase, end_phase, legacy, events }`.

`Events` is a map from the closed `TimingEvent` enum to unsigned counts. Most entries count occurrences. `graphics_cycles` and `sound_cycles` measure variable work in cycles using the local helper formulas, independent of the total legacy interval budget. They are the two fixed terms, each with coefficient one. No legacy total-cycle term enters the regression. Counts describe executed work, including attempts that fail, rather than net changes in snapshots.

| Work                  | Events and measured work                                                                                                                    |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Interval and controls | Round baseline, observed input, death, completion                                                                                            |
| Graphics and player   | Tile draw, actor draw, sprite erase, player collision draw, animation update, movement attempt, fall, centered-gold check, dig, score update   |
| Sound                 | Sound section, note, variable countdown/speaker/delay cost                                                                                   |
| Guard scheduling/AI   | Selected update, fall, directional attempt, route determination, same-row scan/support probe, horizontal search and limit span                |
| Route candidates      | Candidate column, up/down search, rows examined above/below, left/right probes, nonempty terrain, relation to player row, distance, best route |
| Environment           | Hole scan, active hole timer, resurrection scan, active resurrection timer, exit revealing                                                    |

Counts for `limit_scan` describe the traversable span, excluding unsuccessful boundary probes. `route_candidate` counts every considered column, even when it has no vertical route. `above_scan` and `below_scan` count each iteration, including its final return test. Left/right probes count the checks actually reached; an early return prevents subsequent probes. `above_player_probe` counts examined upward ladder steps on or above the player; `below_player_probe` counts nonempty downward steps on or below the player. Distance events classify returned candidate rows relative to the player. `better_route` counts strict improvements to the selected pseudo-distance. These distinctions preserve the original search asymmetries instead of treating every AI run as equal work.

The estimator computes the sum of coefficient times count, rounds once to the nearest cycle, and clamps negative totals to zero. Coefficients for unselected factors are zero. Coefficients are conditional corrections to the fixed helper costs; negative coefficients are permitted and do not mean an operation takes negative time. Correlated counts do not support interpreting every coefficient as an isolated subroutine cost.

Intervals beginning in Playing and ending in Playing, Died, or LevelCleared use the model. Polling, pause/high-score entry or return, and death/completion reload intervals retain their legacy durations. Disk, iris and presentation work have no native event equivalents. Initial rendering precedes both clocks' trace origin.

Every successful turn exposes `Run::last_timing()` and `Run::last_model_cycles()`, including when trace retention is disabled. Traced runs also retain `timing_intervals()`. `trace_with_model` reuses the observed gameplay and replaces only timing deltas: each round's start delta closes the preceding interval; `final_delta` closes the last. It changes neither the legacy clock nor speaker timestamps. `replay_with_model` independently executes the inputs, checks model start deltas, and checks terminal duration and state. Replay requires the coefficient set used to produce the trace; the CLI uses the embedded calibrated set.


Original-machine experiments
----------------------------

The measurement executable boots the pinned 4am disk on enhanced Apple IIe/W65C02 with the established `fixed-v2` initial state. It changes level sectors and delivers keys through the existing validator; gameplay instructions remain intact. Controlled boards vary sprite alignment, guard counts zero/one/three/five, movement and blocked attempts, sound muting and speed, falling/landing, ladder travel, gold pickup, digging/refill, completion and sacrifice. A distant guard reaches a dug hole shortly before it closes, forcing burial and twenty resurrection updates through ordinary gameplay.

Original boards 1, 3, 15, 25, 75 and 100 train the model alongside the controlled boards. Boards 2, 10, 50 and 150 select factors and regularization. Boards 5, 40 and 125 are final tests. Splits hold out entire levels, rather than adjacent correlated turns. The implementation reports test results but never uses them to choose coefficients.

MAME's ordinary boundary samples supply actual complete-interval CPU cycles and exact state/occurrence validation. Optional source-entry breakpoints also record timestamped tile draws, actor draws, erases, sound sections, notes, player animation/collision draws, guard route determinations, hole scans and exit calls in separate `timing-N.json` files. At every modeled interval the fitting script requires these independent counts to agree with the native counts. Stopped observer work must leave the CPU cycle count unchanged.

`measurements.json` retains every native interval, actual duration, split and original source-entry list. Each scenario has a machine report linking its retained disk, raw samples and input protocol. `estimates.json` compares both estimates with the same independently measured duration, interval by interval. `*-legacy.json` and `*-model.json` preserve both trace timelines. `timing-verify` reexecutes model traces natively and in fresh unprofiled MAME runs for the test levels and controlled digging, falling, gold, burial, completion and sacrifice scenarios. Actual durations must equal the earlier profiled measurements exactly, in addition to passing normal state/event and timing validation.


Fitting and refinement
----------------------

The dependency-free Python fitter targets actual cycles minus the two fixed helper-work terms. It RMS-scales columns and uses ridge-regularized least squares solved by Cholesky decomposition. Ridge candidates are 0.000001, 0.0001, 0.01 and 0.1; selection minimizes validation RMSE. Zero-work columns are excluded. The first candidate contains interval/input/player/guard/dig/score/hole terms. Successive candidates add route-scan, terrain-probe, graphics, movement, animation/collision and sound factor groups.

After group selection, individual factor additions and removals test whether a finer combination improves validation RMSE. Each accepted step must improve it by at least 0.5%; the final report records the best rejected trial. This is a practical plateau for these datasets and this linear family, not a proof of global optimality. Searches and test metrics are reproducible from the retained measurement file.

Initial coarse fitting showed that route iteration counts mattered much more than an undifferentiated guard-update count. Terrain probes and distance/route improvements further reduced error. Whole-turn tests determined the selected combination; factors that did not improve validation were discarded even when their individual cost sounded plausible.

The report records mean absolute error, RMSE, signed bias, maximum cycle/percentage error and aggregate total-cycle drift for each split. Percentage errors on very short sacrifice turns can be large despite small absolute errors. Coverage does not establish accuracy on all 150 original boards, every burial/resurrection configuration, or unsupported joystick/editor/demo paths. The calibrated CPU is W65C02; NMOS machines and clock stretching have not been separately calibrated.

The final MAME 0.276 dataset contains 2,959 intervals across 36 scenarios, of which 2,894 gameplay turns use the fitted model. All shared event-entry counts agree. The other 65 intervals preserve legacy estimates.

| Split             | Turns | Legacy RMSE | Model RMSE | Legacy MAE | Model MAE | Legacy total drift | Model total drift |
| ----------------- | ----- | ----------- | ---------- | ---------- | --------- | ------------------ | ----------------- |
| Training          | 2,054 | 1,408       | 169        | 1,072      | 124       | -2.800%            | -0.0002%          |
| Factor validation | 480   | 1,840       | 251        | 1,707      | 176       | -3.550%            | +0.022%           |
| Final test        | 360   | 1,699       | 342        | 1,517      | 275       | -3.513%            | -0.221%           |

Errors are CPU cycles. The final test's worst absolute error falls from 3,517 to 965 cycles and its worst percentage error from 6.62% to 2.30%. The best final factor trial yields validation RMSE 250.215 rather than 250.838 cycles, a 0.249% improvement, below the stopping threshold. Short sacrifice turns still reach 14.91% relative error, corresponding to 41 cycles on a 275-cycle turn. Original disk/reload intervals retain much larger errors and are excluded from these gameplay metrics.


Commands
--------

Run from the repository root; measurements require the same MAME/ROM installation as external validation.

1. Measure original execution: `make -C mame-validation timing-measure`.
2. Fit coefficients and write paired trace comparisons: `make -C mame-validation timing-fit`.
3. Validate selected model traces in fresh machines: `make -C mame-validation timing-verify`.
4. After changing only native instrumentation, refresh counts with exact legacy replay: `make -C mame-validation timing-refresh`, then fit again.
5. Check Python types: `make -C mame-validation timing-types`.

`TIMING_WORK` overrides the artifact directory. Measurement resumes already completed scenarios; use a new directory after changing scenario definitions or original-machine profiling. Native refresh requires unchanged legacy execution and input schedules, and retains the independently measured machine data.

Produce both timelines for another original-board schedule with `target/debug/lode-original capture legacy.json --rounds 120 --input 0:cc --event-trace model.json`. Validate the native model timeline with `target/debug/lode-original replay model.json --event-model`; external validation accepts either timeline with its ordinary `validate` command.
