Lode Runner port semantics
=========================

Status
------

The workspace is being built toward a procedural Rust port and same-implementation trace round trip. Emulator selection, instrumentation, and recording/playback as a separate user feature are outside this milestone. Gameplay implementation is not yet complete.

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
