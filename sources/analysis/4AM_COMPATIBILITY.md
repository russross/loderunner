Apple II 4am disk compatibility
==============================

Baseline and provenance
-----------------------

The preferred external boot baseline is the playable disk in the user-supplied `sources/Lode Runner (4am crack).zip`. Its included write-up identifies Apple ][+ or later, 1983, Doug Smith, and Brøderbund Software, with a crack dated 2018-10-20 and a write-up update dated 2023-08-01. The archive also contains a diagnostic work disk, the cracking account, and Peter Ferrie's BSD-licensed 0boot source. The probe selects the playable disk by its exact archive member name; the work disk is not mounted.

| Asset   | SHA-256                                                          |
| ------- | ---------------------------------------------------------------- |
| Archive | b21d2a6d99c6fb42ec01e17cf8f9bd165fad631f08a58b94269d0369131996ad |
| Disk    | 71412e9b4619291a38abc3ad60e032fcc8fba12775798f8d84aeacd108aacdf6 |

The disk is 143,360 bytes in DOS sector order. The vendored reverse-engineering project remains unchanged at revision `113db1c16951717259c9ae5e128958b29de0634d`; it reconstructs the Reset Vector crack. The Rust port continues using its assets. Selecting the 4am disk for external validation does not replace the port's source image or data.

The extras archive contains original capture media, a WOZ image, screenshots, and recordings. It is not needed for this disk-boot proof. The separate Championship Lode Runner archive is not part of this baseline.

Static disk comparison
----------------------

For original level number `n` in 1 through 150, select track `3 + (n-1)/16` and sector `(n-1)%16`. All 150 complete 256-byte sectors match the corresponding vendored track assembly byte-for-byte, including their unused tails. Thus all 224 playfield bytes per level agree.

Track $0C has differences outside these 150 sectors. In particular, its high-score sector $0F differs in 69 bytes from the vendored reference. The future validator must use matching initial high scores when validating high-score behavior. Ordinary gameplay tests that do not inspect or change the table do not establish equivalence of high-score workflows.

Disk boot and loaded-image comparison
-------------------------------------

Debian MAME 0.276 boots the playable disk using `apple2ee`, installed audited firmware, the W65C02 CPU, and its Disk II controller. The probe initially sets only a conditional execution breakpoint at $6000. ROM reset, disk-controller firmware, 0boot, loading, and the game's own startup execute normally. The condition checks the MOVE_PLAYER call bytes at $60E4-$60E6 so another program temporarily occupying $6000 is not mistaken for the game.

At the $6000 stop, mapped RAM $0000-$BFFF is saved to `loaded.bin` before the application initialization runs. Comparing that image against `main.bin` yields:

| Address range | Result                                                                    |
| ------------- | ------------------------------------------------------------------------- |
| $1A85-$1DB1   | Identical scanline and coordinate lookup data                              |
| $6000-$8FFF   | Identical except $6031/$6032: $84/$1A in reference, $28/$60 in 4am           |
| $9000-$9EFF   | 1,200 differing bytes; includes the boot/startup material used by this disk |
| $9F00-$B6FF   | Identical embedded level, pattern, shift, and sprite data                   |
| $B700-$BFFF   | 519 differing bytes; includes RWTS/DOS code and live disk-control state     |

The two differences inside $6000-$8FFF are the initial address operand of RESET_GAME's self-modified splash-data read. RESET_GAME stores $0EFF into that operand before incrementing and reading through it. These initial placeholder values therefore do not select different splash data. The compatibility check allows exactly those two known values, rejecting other changes inside this range.

Differences outside the checked gameplay/graphics ranges are not blanket-approved as harmless. The disk's startup and RWTS paths have distinct implementations and state; the cracking account explains replacing protected loading with 0boot and adjusting disk access. The probe exercises these paths through real machine emulation rather than substituting their effects.

Keyboard startup and gameplay boundaries
----------------------------------------

After the entry snapshot, the adapter installs keyboard and speaker taps. At the first $61A9 splash input poll it supplies Space. At $8700, the level-ready cursor routine, it supplies $8B for keyboard mode. These are device inputs: the disk probe never writes gameplay RAM, replaces PC/registers, or patches instructions. Each one-time breakpoint is cleared before continuing.

The first $60E4 stop starts the gameplay interval sequence. Startup key observations and speaker counts are discarded at this boundary. Subsequent inputs are right ($CC) at round zero, stop ($A0) at ten, and dig left ($D5) at twelve. Twenty gameplay rounds finish at the twenty-first boundary. All round costs come from MAME's `totalcycles`, with observer reads checked not to advance CPU cycles.

Two fresh disk boots reproduce identical complete raw snapshots and absolute boundary cycle counts. The first gameplay boundary occurred at cycle 12,279,437 in this configuration; this includes boot and menus and is not a gameplay interval duration. The twenty gameplay intervals total 1,044,485 cycles, range from 40,157 to 66,737 cycles, and contain 200 speaker reads.

The direct-reference probe and disk-boot probe also agree at every boundary on interval duration, player coordinates/subcell adjustments/animation, and both logical playfield maps. Each independently confirms three guards, six gold tiles, persistent motion, stopping, eight dig updates, and skipped keyboard polling during continued digging.

Limits of the comparison
------------------------

Raw startup RAM differs between direct loading and disk boot. For example, the player's facing byte $05 initially contains $00 in the direct probe and $A9 after disk boot, although animation byte $04 selects the same initial sprite. Some guard-facing bytes also differ before their first directional update. The scripted first move makes the player's facing bytes agree. These are observed initialization differences, not evidence that initial facing is irrelevant to every path: falling before a movement command could use it. The Rust port is not changed by this proof.

Unused hole coordinates and inactive guard slots contain different loader residue. Matching active/background maps is stronger evidence for this tested schedule than raw equality across initialization methods, but does not replace a game-level observer specification. Full trace mapping must distinguish observable state, retained state that can affect later behavior, and unused storage. It must not discard differences merely because this short schedule does not expose them.

No Rust-port gameplay trace is validated by this proof. Death, restart, level advancement, high scores, timing drift tolerances, and transient event attribution remain outside its scope. Enhanced IIe's CMOS CPU is the selected platform; its timing can differ from the NMOS formulas used in the initial port. Full cross-validation must preserve that platform distinction.
