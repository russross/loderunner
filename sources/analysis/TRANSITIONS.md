Application transitions
=======================

Gameplay boundary
-----------------

Gameplay begins when the player can move and ends at the committed Died or LevelCleared snapshot. Startup, completion bonuses, death presentation, board preparation, iris animation, and ready waits do not advance Clock.cycles. Clock.output_cycles adds a separate presentation offset for continuous speaker timestamps and host pacing. Gameplay notes retain their calibrated event timing; application tunes use fixed schedules measured once from the original.

Trace version 4 normalizes ready snapshots to Playing and stores one zero-duration board checkpoint carrying the new board's starting state, score, and attempt counter. It contains no intermediate bonus updates, animation frames, ready inputs, or presentation duration. The first gameplay interval after that checkpoint starts at the same gameplay time. Replay runs presentation normally and dismisses ready immediately. Recording version 2 addresses gameplay intervals only, omitting the board checkpoint as well; playback infers a transition from the terminal gameplay state before consuming its next key. Old trace version 3 and recording version 1 are rejected because their counters and input addressing have different meanings.

MAME observes ordinary gameplay independently, including the final death/completion interval. Its adapter passes through original transitions and supplies an unrecorded ready key, subtracting all intervening cycles. Transition state/events and duration are excluded from ongoing comparison and timing reports. Replenishing original remaining lives happens outside gameplay so validation can continue after any number of captures. The machine observer derives lives used from board changes and attempt identities rather than comparing original remaining lives with the application's new counter.


Completion and death
--------------------

Completion increments the displayed level and resets lives used to one. The completed board remains visible while the score increases by 100 fifteen times. Three original duration-six notes accompany each update, with pitch parameters 240, 224, ..., 16. The sequence lasts 898,922 microseconds. Fixed score and note onset tables are in src/transition.rs; they do not depend on gameplay speed settings. Pending gameplay notes are cleared before the tune.

Death increments lives used while retaining the captured pose and score. The original has no separate death sprite animation: it updates the status, plays its paired-note tune, then loads the board through the iris. The fixed sequence has 24 notes and lasts 440,313 microseconds. Note duration parameters descend in pairs from thirteen to two and pitch parameters from 240 to 64. The final empty sound call's retained falling-pitch increment is preserved when a landing whistle is pending. Pending gameplay notes are cleared before the fixed death sequence.

Lives are unlimited. The u32 lives_used counter starts at one, increments on each death or requested restart, and resets on each level change, including skip and list wrap. The USED status field displays its lower three digits. Ctrl+A and Ctrl+R both restart the current board while retaining score. The original Ctrl+@ extra-life control is absent. Global attempt identity continues to increment for every reload independently of the per-level counter.


Iris, preparation, and ready
---------------------------

Startup opens the initial board over one second. Completion and death first finish their tune, then close the old board over one second. Only after the aperture is fully black does loading replace board-local state and compose the new scene. Loading runs immediately at host speed, adds no simulated delay, and never displays a partially prepared board. The new board opens over one second.

The iris is a circular aperture centered at (140, 88) in the native 280 by 176 playfield. Its radius grows or shrinks linearly between zero and the farthest corner, independent of original machine rendering speed. Sixty evenly spaced presentation frames cover each one-second leg. The status strip stays visible. Both simple and CRT output receive the same masked scene; GPU processing remains in the ordinary retained CRT pipeline. The effect is applied to a scene copy, leaving contact ink and gameplay draw history intact.

After opening, the player is visible for one-third second and hidden for one-third second, repeating while the board and guards remain stationary. Hiding restores background in a presentation copy. A fresh non-repeat gameplay key dismisses ready without becoming a movement command or recorded input. Keys during startup and transition animations are drained; native quit and window-refresh requests survive. Playback dismisses ready automatically. Headless execution and explicit development input schedules also dismiss ready automatically so bounded runs require no host input.

The default window is 1120 by 768 logical window units, four times the native 280 by 192 image. Number keys one through five retain their existing size shortcuts.


One-time reference measurement
------------------------------

transition-measurement.json retains the raw stopped-boundary observations, original image SHA-256, machine identity, nominal 1,023,000 Hz clock, frame period six, and MAME version 0.289. The pinned sources/lode_runner_reveng/main.bin was loaded into Apple IIe enhanced RAM using the same source-image initialization preconditions as sources/mame-poc/probe.lua. Original initialization ran through the first INIT_GAME_DATA.game_loop before entering either terminal routine. No port timing model supplied these measurements.

The completion capture starts at INIT_GAME_DATA.level_cleared, observes ADD_AND_UPDATE_SCORE and PLAY_NOTE entry, and ends at INIT_GAME_DATA.start_game_ before LOAD_LEVEL. It spans 919,597 CPU cycles. The death capture starts at INIT_GAME_DATA.died, observes PLAY_NOTE entry, and ends immediately after the PLAY_SOUND/BCS loop, before the application tests GAME_MODE or loads another board. It spans 450,440 cycles. Both captures have an empty pending-note buffer and supported, non-falling sound state. Breakpoint register snapshots supply original note duration and pitch; times are converted once to microseconds for the application spec. Disk loading, iris work, start-key waits, and attract-mode delays are outside these measurements.

These observations define presentation constants. They are not an ongoing transition-emulation or validation requirement. Native tests cover score progression, pose/board ordering, trace-clock exclusion, ready-key exclusion, iris clipping, and independent gameplay replay.
