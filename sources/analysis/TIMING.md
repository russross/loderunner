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

Coverage limits
---------------

These helpers have local timing accounting. Sprite rendering and level initialization currently have no charged timing. Their timing analysis must be completed before declaring gameplay round-trip timing established. RGB decoding, host texture updates, and real storage parsing are host work, not original instruction costs.
