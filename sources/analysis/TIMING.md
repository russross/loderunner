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

Exact same-implementation trace equality establishes deterministic timing accounting. MAME validation now compares individual interval durations with explicit tolerances. Six selected original boards pass the 2,000-cycle plus 10% gameplay allowance over a 90-interval schedule. The level-150 fixture fails at interval 43: 67,995 port cycles versus 126,735 MAME cycles. Its object state and events pass with an explicit 100% allowance. Long guard route searches remain a demonstrated gap in the rounded budgets; the validator reports this gap without changing expected gameplay or silently enlarging its default tolerance. Transition timing is checked separately because the port omits original disk, iris, and presentation delays.
