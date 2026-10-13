Lode Runner port semantics
=========================

Status
------

The workspace is being built toward a procedural Rust port and same-implementation trace round trip. Emulator selection, instrumentation, and recording/playback as a separate user feature are outside this milestone. The trace contract, initial devices, sprite helpers, level loading, and SDL level preview exist; gameplay implementation is not yet complete. Trace serialization/model application tests are not the gameplay round-trip milestone.

Sources and project boundaries
------------------------------

`sources/lode_runner_reveng/` vendors reference revision `113db1c16951717259c9ae5e128958b29de0634d`. Its README identifies the reconstructed cracked Apple II disk image. The original downloaded checkout remains outside the root repository. Source provenance and its CC BY-SA license remain with the vendored files.

The root Cargo workspace contains `trace/` and `port-original/`. Gameplay files remain flat in the port's `src/`; hardware lives in `src/devices/`; tests live in each project's `tests/`. Future ports become sibling packages and share the trace contract. Make drives builds and checks. `mostlyclean` removes incremental intermediates; `clean` removes Cargo build outputs. Neither removes sources, traces, or persistent game data.

Execution and hardware
----------------------

The port translates original subroutines, loops, and conditions into Rust operations on named state. Short shared exits and jumps into other routines are duplicated to keep functions self-contained. It does not execute 6502 instructions. Ordinary counters retain original byte wrapping where behavior depends on it; closed domains use enums.

Hires devices retain all bytes in both original 8 KiB ranges, including display holes. Rendering and framebuffer collision preserve byte operations and order. Display decoding and incremental dirty updates belong to the device. Keyboard input retains the original last-code latch and availability strobe. Physical release does not stop a persistent keyboard movement command. Speaker access toggles its state on the virtual clock. Storage access is replaced without emulated disk delays.

Timing is calculated from local instruction/block analysis and consolidated into routine/path formulas when practical. Host pacing never determines game state or virtual elapsed time. Approximations must be listed with affected paths and their justification.

Rounds and observation
----------------------

A gameplay round begins immediately before MOVE_PLAYER at the main loop and contains its original ordered operations: player movement, sound/delay, exit-ladder and completion checks, timers, sound/delay, and scheduled guards. Death and level completion finish the current round before reset or level loading changes actor identity. Round duration is variable. Waiting and transitions use explicit non-gameplay intervals.

The only cycle-based trace record is the interval start, expressed as cycles since the previous interval start. Every event inside the interval is untimed. A final boundary closes the last interval and gives its elapsed duration. Empty rounds are retained for replay input addressing and timing comparison.

The observer consumes committed snapshots and occurrence observations; it does not simulate game rules. Net changes are computed from successive snapshots and applied atomically. Occurrences describe transient observations and are not applied a second time as state mutations. Inputs retain delivery order and a logical observation ordinal. Other occurrence episodes retain order per subject; independent subjects are sorted canonically. Snapshot objects and net changes are canonicalized by identity.

Player and guard snapshots contain cell and subcell location, facing, motion/action, lifecycle, and gold possession. Terrain includes original tile identity and current traversability representation. Holes include location, lifecycle, and a generation distinguishing later openings at the same cell. Gold is represented by location or carrier rather than an invented permanent object identity. Session state includes level/attempt identity, phase, score, lives, and exit availability. Guard scheduler and timer internals remain port state; their visible consequences enter the observer. Collision partners must be observed or established from evidence, never inferred solely from death.

Trace round trip
----------------

A trace carries initial observer state, source/configuration identity, interval records, and a terminal boundary. Validation checks identities, event canonicalization, state-change preconditions, monotonically numbered intervals, and final state. Replay restarts execution from the same deterministic configuration and injects only recorded inputs through devices at their interval and observation ordinal. Expected gameplay events never drive execution. A fresh independent trace must match the supplied trace exactly, including timing and final observer state. Later cross-validation may tolerate interval timing drift without tolerating gameplay differences.

Input observations record newly delivered keyboard codes even when identical to a previously delivered code. Absence of a fresh key is not a release. State changes alone cannot recover such deliveries, contacts without persistent effects, or transient lifecycle episodes; these require a small occurrence journal.

Implemented device and rendering behavior
----------------------------------------

Hires writes invalidate whole visible scanlines only when stored bytes change. Whole-scanline decoding covers adjacent-byte artifact dependencies. Selecting another page invalidates all of its scanlines. Texture updates are incremental; presentation copies the persistent texture because SDL backbuffer contents are not assumed to persist. The current digital color decoder distinguishes white adjacent bits and alternating colored patterns; it does not yet reproduce analog NTSC filtering or half-dot phase transitions accurately.

Sprite shifting reads the original image's seven shift pages and pattern tables. Tile drawing retains masks and two/three-byte writes. Actor drawing returns `(foreground XOR background) AND shifted sprite` overlap bits accumulated before drawing. Erasure uses `(foreground AND (shifted sprite XOR 0x7f)) OR background`. These operations do not inspect RGB pixels.

The keyboard device supports the original C000/C010 boundary. The speaker stores ordered toggle edges and averages drive over audio samples. Live SDL audio submission is not yet implemented. PLAY_NOTE is consolidated into pitch-countdown runs: its initial countdown uses duration, subsequent countdowns use pitch, and its incoming low counter determines the first duration segment. Muting retains the countdown duration but saves three cycles at each skipped speaker access, as in the original branches.

Sound buffers retain one-based append and last-in-first-out selection. The port rejects overflow cleanly rather than corrupting adjacent memory. This is an intentional difference for out-of-domain sound sequences and is not exercised by normal original gameplay.

Level loading and preview
-------------------------

The storage implementation reads one original 256-byte sector from the vendored track data using level indices 1 through 150. The decoder visits rows top to bottom and columns left to right, taking each low nibble before its high nibble. Values 10 through 15 become empty. Only the first 224 bytes supply tiles.

Gameplay initialization visits rows bottom to top and columns right to left. The first encountered player survives and later player markers disappear. Guards are numbered 1 through 5 in that same traversal; extra guards disappear. Up to 45 hidden ladders are remembered and removed from both logical maps. Gold remains in both logical maps until collected. Accepted actor markers remain only in the active map. Both packed graphics pages are initialized through the original sprite helpers; the iris transition and status display are not yet implemented.

The current player initially faces right; original guard-facing byte zero also means right for state decoding, although guard animation zero initially selects the left-facing sprite. Actor motion classification after movement is not yet implemented. Score currently initializes an observer decimal value to zero; original BCD score mutation has not yet been ported.

`make` builds the preview executable. `cargo run -p lode-original -- preview 1` opens a static level through the SDL device. It does not accept gameplay input. All 150 levels are covered by decoding, initialization, and observer snapshot validation tests. Additional tests cover reverse actor selection, malformed nibbles, absent player, and invalid level numbers.

Local timing formulas
---------------------

All listed callee costs include RTS and exclude the caller's JSR. SOUND_DELAY1 uses `906*n+5`, interpreting a zero byte as 256 iterations. SOUND_DELAY uses `906*2*(setting+1)+11` for settings 0 through 15. ROW_TO_ADDR uses `23+(row>=123)+(row>=187)` for visible scanlines. The virtual clock currently uses a nominal 1,023,000 Hz NTSC conversion; original machine scanline clock stretching is not represented. Host sleeps target an absolute deadline and never modify virtual time.
