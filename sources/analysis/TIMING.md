Local timing derivations
========================

The formulas here use NMOS 6502 timings and the vendored assembled image. They include callee RTS and exclude caller JSR. They are independent of host execution speed. The initial port does not interpret these instructions at runtime.

SOUND_DELAY1 at $86B5
---------------------

Each outer iteration loads Y with 180 for two cycles. The inner DEY/BNE loop costs `180*2 + 179*3 + 2 = 899` cycles. DEX costs two; the outer BNE costs three except for the final iteration, which costs two. Consequently n outer iterations plus RTS cost `n*(2+899+2+3)-1+6 = 906*n+5`. Incoming X zero means 256 iterations, not zero delay.

SOUND_DELAY at $86B1
--------------------

LDA absolute-X and TAX add six cycles. The indexed load cannot cross a page for settings 0 through 15: the table starts at $86BE. The table supplies `2*(setting+1)`, giving `906*2*(setting+1)+11` including SOUND_DELAY1.

ROW_TO_ADDR at $7A31
--------------------

The routine costs `4+3+4+3+3+6 = 23` cycles before page penalties. The first indexed table starts at $1A85, adding a cycle for Y at least 123. The second starts at $1B45, adding a cycle for Y at least 187. For visible rows the cost is `23+(row>=123)+(row>=187)`.

PLAY_NOTE at $87BA
------------------

Saving pitch and the high duration counter costs six cycles. Each speaker iteration costs nine cycles before the low countdown when enabled: LDA zero-page, untaken BEQ, LDA absolute. Muted iterations cost six: LDA zero-page and taken BEQ. Speaker access occurs at the end of that prefix.

A normal low/pitch countdown step costs ten cycles: DEY, taken BNE, DEX, taken BNE. A nonterminal low-counter wrap adds six cycles: untaken BNE, DEC zero-page, untaken BEQ replace the taken BNE. A pitch reload adds five cycles: untaken BNE, LDX zero-page, JMP replace the taken BNE. Terminal low wrap returns before DEX and costs eighteen cycles including RTS.

The first pitch countdown starts with the duration argument, not the saved pitch; subsequent countdowns start with pitch. The total number of low decrements is `(high_count-1)*256+low_count`, interpreting a zero byte count as 256. The Rust helper advances whole runs between low-counter wraps and pitch reloads. It retains each speaker edge without simulating individual original instructions.

PLAY_SOUND at $8811
-------------------

A buffered note reaches PLAY_NOTE after 19 cycles including the calling JSR. After the note, LDY/DEC/LDA/SEC/SBC take 17 cycles. Equal duration adds a taken BEQ, SEC, RTS: 28 total post-note cycles. Longer duration adds untaken BEQ and taken BCC: 30 total. Shorter duration adds two untaken branches, TAX, JSR, SEC, RTS: 37 plus SOUND_DELAY1 cost. These indexed buffers cannot cross pages for supported note indices 1 through 127.

With no buffered note and falling disabled, the routine consumes 29 cycles plus SOUND_DELAY. Falling enabled consumes 34 cycles before PLAY_NOTE and eight after it. The initial LDY NOTE_INDEX leaves its low counter at zero on this path. All PLAY_SOUND formulas exclude the caller's JSR.

Sprite shifting and graphics loops
---------------------------------

The shift helper's eleven row iterations read 22 sprite planes, each separated by 104 bytes. Its base accounting is 1,824 cycles, plus one for each indirect indexed plane read whose low address plus sprite index crosses a page: `sum(((plane*104)%256)+sprite >= 256)` for planes 0 through 21. Pattern lookup pages and seven shift alignments remain the original data; no runtime instruction interpreter is needed.

Tile drawing charges `120 + shifted_cost + sum(ROW_TO_ADDR(y+line)) + 11*loop_cost + 5`. The two-byte loop costs 69 cycles per row; the three-byte loop costs 81. The setup budget 120 is representative rather than a path-complete instruction total. The final five adjusts final-loop termination/return accounting.

Actor drawing charges `64 + (half_x>=102) + shifted_cost + 2*sum(row_page_penalties) + 11*loop_cost + 5`. Two-byte and three-byte row loops cost 133 and 170 respectively, including nominal lookup helper costs for both foreground and background. Erasure uses setup 61 and row loops 107 or 131. The horizontal lookup starts at $1C9A, crossing for half-X at least 102. Each actual row supplies both indexed scanline lookup penalties. Sprite width depends on shift: shifts 5 and 6 touch three bytes, other shifts touch two.

These formulas retain the important data-dependent alignment and page-crossing costs. Setup constants and some branch/caller costs remain approximations. Hires accumulates the charges and gameplay drains them before sound or interval completion, preserving the temporal relationship between drawing and speaker edges.

Gameplay and polling budgets
----------------------------

The procedural translation uses the following rounded local block budgets in addition to graphics and sound charges. These are estimates from the local instruction sequences, not complete enumeration of every execution path.

| Path                              | Cycles                         |
| --------------------------------- | ------------------------------ |
| MOVE_PLAYER dispatch              | 200                            |
| Player left/right/down attempt    | 180                            |
| Player up attempt                 | 200                            |
| Digging update                    | 200                            |
| Score update                      | 350                            |
| Selected guard update             | 200                            |
| Guard directional movement        | 180                            |
| Horizontal guard route-limit step | 75 per scanned cell            |
| Guard route candidate search      | 100 per candidate/row examined |
| Hole scan baseline                | 690                            |
| Active hole processing            | 50 per active timer            |
| Waiting/menu/pause polling batch  | 5,000                          |

The main loop retains two PLAY_SOUND boundaries when ordinary gameplay reaches both. Completion and death can bypass later work. Transition tunes retain individual note checkpoints. Host audio and graphics are submitted before pacing each checkpoint; replay skips host pacing altogether.

Coverage limits
---------------

The nominal clock is 1,023,000 Hz. Apple II scanline clock stretching is not represented. Caller JSR and miscellaneous gameplay branch/setup costs are not universally charged. Initial level construction is outside the clock origin; reload/iris and disk timing are not reproduced. RGB conversion, SDL uploads, sample conversion, and real file parsing are host work rather than original instruction costs.

Exact same-implementation trace equality establishes deterministic timing accounting. MAME validation now compares individual interval durations with explicit tolerances. Six selected original boards pass the 2,000-cycle plus 10% gameplay allowance over a 90-interval schedule. The level-150 fixture fails at interval 43: 67,995 port cycles versus 126,735 MAME cycles. Its object state and events pass with an explicit 100% allowance. Profiling identifies differing falling-sound initialization as the cause of this spike; the earlier attribution to guard-route cost was incorrect. Transition timing is checked separately because the port omits original disk, iris, and presentation delays.


Level 150 falling-sound initialization
--------------------------------------

Level 150 starts the player at column 14, row zero, without immediate support. MOVE_PLAYER takes the gravity path for many consecutive rounds before reaching its supported-input path. That supported path normally assigns `$20` to `$A4`; ordinary supported starting positions therefore hide its loader residue. The tested 4am boot leaves `$A4 = $AC`, whereas Rust initializes `falling_pitch = $20`. The current fixed profile normalizes facing and guard phase but does not normalize this sound counter.

When no buffered notes remain and falling is active, PLAY_SOUND computes pitch as `$A4 >> 1`, increments the byte with wrapping, and plays a note with duration FRAME_PERIOD + 2. The counter advances twice per ordinary falling round. In MAME it reaches `$FE` before round 41, `$00` before round 42, and `$02` before round 43. Round 43 plays two notes at pitch one. Their countdown loop reloads and toggles the speaker much more frequently, adding substantial CPU time. Rust's counter is `$76` before round 43 and produces pitch `$3B` instead.

Source-boundary profiling of the failing interval places its two PLAY_SOUND entries at cycles 15,249,974 and 15,299,942, with guard 2's route determination starting at 15,349,339. Thus approximately 99,000 cycles precede guard route selection in these two sound/timer sections. The guards perform short route scans; their search is not the source of the 58,740-cycle discrepancy.

A controlled experiment changes only first-game `$A4` initialization to `$20` in a temporary adapter, leaving the executable validator and Rust unchanged. It supplies the same observed inputs over all 90 intervals. Round 43 becomes 70,595 MAME cycles versus 67,995 Rust cycles, a 3.82% difference. All 90 intervals then pass the default 2,000-cycle plus 10% allowance; the largest relative difference is 5.29%. Raw profiling and controlled-run artifacts remain under `artifacts/analysis/level150-profile-1791938354386517049/` and `artifacts/analysis/level150-normalized-sound-1791938438475889131/`.

The appropriate correction is to include falling-sound state in the shared initialization profile, rather than increasing guard-route budgets or the gameplay tolerance to mask this mismatch. Rust's initial `$20` value already matches the supported-input reset. Reload persistence also needs correction: original LOAD_LEVEL and INIT_GAME_DATA do not reset `$A4` or `$9B`, while Rust currently constructs fresh falling-pitch and falling-sound fields during reload. Retaining these fields is necessary for starts/restarts that fall before the supported-input reset. This analysis does not change executable initialization or reload behavior.
