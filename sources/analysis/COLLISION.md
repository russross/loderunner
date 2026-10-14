Object-level collision analysis
===============================

The native port has two independent contact mechanisms. Logical occupancy detects a guard entering a player cell and a closing hole containing the player. Player redraw queries a local shape-composition scene, independently of rendering. The unchanged `port-original` supplies the packed-framebuffer reference detector. The native model retains background and contrast shapes per local cell, rather than trying to assign every surviving pixel to an actor. This captures contact-relevant history without scanline addresses, color phases, or a table of animation-pair outcomes.


When contact is evaluated
-------------------------

`Game::play_round` updates the player before resurrection, hole timers, and scheduled guards. Movement, falling, and successful digging erase the old player, update position and animation, and draw the new player. Only `draw_player` consumes the graphics overlap result. A stopped or blocked player does not receive an extra geometric contact test. Collecting gold suppresses overlap death for the remainder of that player update, including draws after perpendicular centering.

Guard drawing discards its overlap result. A guard kills through logical occupancy when its movement crosses into the cell marked Player. This is asymmetric: a guard drawing over the player's pixels does not itself invoke the player's sprite test. The next eligible player redraw tests the foreground remaining after its own erasure. Guard updates stop after player death. Hole closure checks cell occupancy directly, with guard burial taking priority when the active marker is Guard.

Terrain eligibility, gravity, support on guards, digging eligibility, and guard route selection already use logical tiles and subcell alignments. They do not use sprite overlap and should remain separate from the lethal contact predicate. A global collision pass after every round would move contact to a different execution boundary.


What the graphics test means
----------------------------

For each written byte, actor drawing tests `(foreground XOR background) AND incoming_sprite`, then ORs the incoming sprite into foreground. Background is page two, not a freshly rendered logical terrain map. Ordinary terrain present on both pages cannot cause this overlap. Nor can an opponent's pixel at a location where background already has a set pixel: it is indistinguishable from terrain there.

Actor erasure computes `(foreground AND (sprite XOR 0x7f)) OR background`. It restores background through the old shape without preserving ownership of overlapping pixels. Consequently erasing the player, another guard, gold, or debris can also erase part of a guard. Drawing a tile replaces its cell rectangle and can similarly discard actor pixels. Repeated guard draws can restore erased portions. These operations make the foreground depend on drawing history.

Background replacement also matters. Digging changes the brick on foreground first and clears both pages when the hole opens. Refill stages replace background and OR it into foreground through the empty-sprite erase operation. Resurrection stages replace background before adding the stage sprite to foreground; spawning then clears background and replaces foreground with the guard. Gold removal replaces background and erases the gold shape. Exit revealing changes background before adding ladder pixels. These are ordered transformations, not independent collidable entities that can simply be unioned from current logical state.

The raw formula can detect missing background bits as well as extra foreground bits. An object model must either establish that such deficits cannot occur at player draw boundaries, or represent them. The same requirement applies to leftover pixels and partially erased actors. RGB colors are irrelevant: collision reads the packed bits before artifact decoding.

Every written byte of all 104 original sprites at all seven shifts has its high bit set. With the normal initialized pages and sprite operations, the high bit therefore stays equal on both pages and contributes nothing to collision. Arbitrary external writes to Hires are outside that invariant. An object model for gameplay can use the seven actual pixels per byte, without artifact-color or byte-alignment state.


A compact object-pair predicate
--------------------------------

All 18 player and 16 guard animation shapes fit a ten-pixel-wide, eleven-pixel-high local rectangle. Decoding each at all seven original shifts produces exactly the same local low-bit shape after undoing its horizontal shift. Horizontal coordinates are twice `half_x`; vertical coordinates use the existing uneven alignment offsets `[-5, -3, 0, 2, 4]`. Sprite shape depends on actual animation, not just movement direction or facing.

Store each animation shape once as a `u128`, with ten bits per row and 110 meaningful bits. This is per-animation geometry, not a table of outcomes for pairs. For relative displacement `(dx, dy)`, reject `abs(dx) >= 10` or `abs(dy) >= 11`. Otherwise clip the second shape to columns satisfying `0 <= x + dx < 10`, translate it by the signed bit offset `10 * dy + dx`, and intersect it with the first shape. A nonzero intersection means shape contact. Clipping before translation prevents horizontal wrapping into adjacent rows. Positive bit offsets shift left and negative offsets shift right; all surviving shifts are at most 109 bits.

The runtime predicate needs a bounds check, column mask, translation, and one intersection. Nineteen shared column masks suffice for the horizontal offsets. It needs no rendering, scanline addresses, original shift tables, row loop, or animation-pair lookup. Exact geometry decomposed into vertically merged horizontal-run rectangles requires six through twelve rectangles for these sprites; that representation adds multiple rectangle-pair tests without making the predicate clearer. Minimality of a hand-written inequality representation has not been established.

An independent row-intersection calculation agreed with the whole-object predicate for all 114,912 combinations of 18 player animations, 16 guard animations, 19 horizontal displacements, and 21 vertical displacements. This establishes intact shape intersection, not equivalence to history-dependent gameplay collision.


State needed beyond pose
------------------------

A concrete shape-level counterexample uses old player sprite 9, new player sprite 16, and guard sprite 8. Place the old player at displacement `(-2, -10)` pixels from the guard, then move it two pixels right. The new player shape intersects one guard pixel. That pixel is also covered by the old player shape, so erasing the old player first removes it; the existing framebuffer test sees no collision on an empty background. This is an operation-level counterexample. Reachability through supported whole-game updates has not been established, and it must not be presented as a demonstrated gameplay bug.

For an intact opponent against synchronized terrain, the effective shape is its silhouette minus background pixels. With overlapping erasures, it is the surviving silhouette minus background pixels. Current GuardMode alone cannot express that shape: Roaming, Carrying, and Trapped actors can have partially erased pixels. Resurrection is especially unsuitable for automatic actor contact: its stage image is put on both pages, and no ordinary guard update runs until spawning.

Per-object surviving shapes do not provide a simple complete model. A pixel can have contributions from several actors and terrain, erasure deliberately ignores ownership, and background replacement can create contrast without any actor occupying that location. Assigning such differences to a synthetic actor would add bookkeeping without improving the contact decision. The implemented scene instead partitions space into fixed ten-by-eleven local cells and aggregates contact-relevant history there.


Exact local composition
-----------------------

`port/src/collision.rs` defines `Shape(u128)` and `Scene`. Each local cell stores background shape `B` and contrast shape `D = F XOR B`; `F` denotes visible foreground ink only for the derivation. Actual foreground is not a stored third layer. Both extra foreground and missing background are represented by `D`. Sprite geometry is decoded once from the shared assets and cached; gameplay, text, and transitions use sprites zero through 101. Image entries 102 and 103 have wider data and remain supported only by the baseline's packed renderer, as they are not used by gameplay.

The following rules operate on translated whole shapes, not rows. `S` is incoming sprite ink, `R` is the affected rectangular area, and expressions apply inside `R`; everything outside remains unchanged.

| Operation              | New contrast                         | New background |
| ---------------------- | ------------------------------------ | -------------- |
| Test contact           | `D AND S != 0`                       | unchanged      |
| Overlay                | `(D AND NOT S) OR (S AND NOT B)`      | unchanged      |
| Erase and restore      | `D AND NOT (S OR B)`                  | unchanged      |
| Replace foreground     | `S XOR B`                            | unchanged      |
| Replace background     | `D XOR B XOR S`                      | `S`            |

These identities follow directly by substituting `F = D XOR B` into the reference operations. They give an inductive correspondence after arbitrary mixtures of overlays, erasures, and replacements; no assumption that actors remain intact or foreground contains background is required. Clearing background is background replacement with an empty shape. Closing or opening a hole replaces both layers in the original order. Partial actor erasure, overlapping guards, debris, gold, refill, resurrection, and exit revealing need no individual collision exceptions.

For overlay and contact, the region is the incoming ten-by-eleven bounding rectangle. A legal player query touches at most four local cells and performs one translated-shape intersection per cell, stopping at the first contact. Tile replacement affects its ten-by-eleven rectangle. For erasure, restoration extends over the enclosing fourteen or twenty-one pixels aligned to seven-pixel groups. The eraser silhouette is still ten pixels wide; the wider region restores missing background even where that silhouette has no ink. This detail is necessary for fidelity and remains one explicit geometric rule.

The scene covers all 192 visible rows using 28 columns and 18 local cell rows; the last row is partially used. The status border occupies rows 176 through 179, while text in logical row 16 starts at pixel row 181. Actor ink spilling beyond the right edge can reach the first pixels 64 rows below; a short coordinate translation preserves that visible effect. Spill originating at row 128 or lower reaches unused storage, which cannot affect a legal player query. The native port does not retain that unused storage; the baseline still does. Actor placements require half_x below 140 and y no greater than 181; wrapped coordinates remain rejected rather than being interpreted as unrelated scene locations.

Game owns the scene and an incremental RGB Display. `paint_tile`, `overlay_shape`, and `erase_shape` update Scene in order and record original-machine Draw work for timing. Display reads the same masks, including their damaged overlaps, and never feeds changes back into contact state. Status-border appearance and background clearing also update Scene explicitly. Player contact is queried after old-player erasure and collection/terrain updates, before the new sprite is overlaid. Gold suppression and logical guard/hole contact stay in their original gameplay paths. Initialization overlays the player without evaluating lethal contact. `RENDERING.md` specifies local RGB updates and texture presentation.

The native renderer no longer computes overlap. The reference detector lives in the unchanged baseline and is used by tests. Timing journals preserve the same `PlayerDraw` and Draw records, including sprite, position and shift inputs. Those records describe original-machine work; shape-composition operations add no timing-model factors. No production dependency on the baseline or extra collision interface is introduced.


Experiments and validation
-------------------------

Run `make -C port test-collision` to reproduce the persistent study. It includes all 114,912 intact animation-pair intersections, explicit erasure/background/status/right-edge cases, 30,000 deterministic mixed operations with reference contact probes, and an extended differential execution on all 150 boards. The mixed-operation test deliberately replaces either layer independently, clears background, overlays arbitrary supported shapes, and erases arbitrary shapes, so it exercises deficits and damaged overlaps beyond ordinary terrain/actor unions.

The extended execution runs 180 intervals per board, cycling seven commands every fifteen intervals, for 27,000 actual intervals. Native state, logical trace rounds, both visible pixel planes and calibrated gameplay costs match the baseline. Before each interval, it also copies the independently executed baseline display, erases the old player in both the scene and reference, and probes the retained animation at three nearby horizontal poses. These are 81,000 forced probes of reached states; the proposed poses are not claimed to be naturally executed movements or deaths.

| Candidate                              | Disagreements with reference |
| -------------------------------------- | ---------------------------- |
| Intact guard poses                     | 857                          |
| Intact guard poses masked by background | 795                          |
| Background/contrast scene              | 0                            |

The background-masked candidate produced 560 false contacts and 235 missed contacts. Thus masking intact actors by terrain does not resolve the problem in either direction. The scene exactly matched the reference in all mixed-operation probes as well. These results favor retaining two local masks over adding pose-specific exceptions; no minimal exception list has been established or needed.

The existing 90-interval schedule on all 150 boards also passes, giving another 13,500 intervals with exact state, calibrated gameplay costs and all visible pixels. Lifecycle coverage includes both dig sequences, refill, guard burial/resurrection, menus, reloads and retained guard state, with full RGB comparison through the incremental Display. All seven MAME validation groups pass with the new detector and sprite presentation active, including selected original boards, airborne deaths/restarts, digging/refill, burial/resurrection, controls and session transitions. This does not establish exhaustive original-machine equivalence on every supported pose or board. Host collision speed has not been benchmarked; the established change is a bounded whole-shape query with no framebuffer reads or row loop.
