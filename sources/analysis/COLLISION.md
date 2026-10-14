Object-level collision analysis
===============================

The native port currently has two independent contact mechanisms. Logical occupancy detects a guard entering a player cell and a closing hole containing the player. Player redraw detects foreground differences intersecting the new player sprite. Replacing the latter with ordinary actor bounding boxes changes behavior. A compact sprite-shape predicate is possible, but using only current actor poses does not describe every state accepted by the existing graphics operations.


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

The recommended concrete types are `CollisionShape(u128)`, `CollisionPose { shape, x, y }`, and, only if reachable cases require it, per-object surviving-shape state. A Game-owned contact component would receive actor draw/erase and terrain-transition operations explicitly, independently of Hires. It would query nearby effective objects only at the current player-draw boundary. Player suppression, logical guard entry, and hole closure remain gameplay decisions. No trait or interchangeable collision backend is needed.

Per-object surviving shapes alone are not yet a proven complete model. Multiple overlapping objects, changes beneath them, and foreground deficits need an explicit ownership/composition rule. Simply clearing erased bits in all nearby objects may explain extra pixels, but does not by itself explain every foreground/background difference. Introducing an object mask scene without resolving that rule would conceal the same assumption behind higher-level types.


Replacement and validation boundary
-----------------------------------

First run the compact predicate alongside the existing detector at every actual player draw. Compare three candidates separately: intact actor shapes, shapes masked by current background, and surviving shapes updated through relevant operations. Classify mismatches by actor overlap, player self-erasure, gold, digging/debris, refill, resurrection, and exit revealing. Use forced original-machine fixtures as well as native differential traces to establish which operation-level cases are reachable. Keep the old detector authoritative during this investigation.

The reference comparison must occur after player erasure and terrain/gold updates but before the new player pixels are merged. Tests must include adjacent cells with subcell offsets, both facings and every animation, multiple nearby guards, guard support/falling, trap shaking/escape, foreground debris, refill stage transitions, resurrection stages, and gold-pickup suppression. Full packed-page equality remains a rendering check; collision equality additionally checks the exact draw boundary and resulting death/contact trace. Synthetic shape tests cannot substitute for these round-level checks.

After mismatch analysis, retain only state proven necessary and switch the player contact decision to the object query. Rendering can continue unchanged initially. Preserve the existing `PlayerDraw` and graphics Draw timing events, including sprite and alignment inputs: those describe work performed by the original machine, regardless of how cheaply Rust now decides contact. Object-pair tests must not become extra modeled original work. Existing cycle estimates and speaker timing should remain identical for identical gameplay events.

No gameplay detector has been replaced by this analysis. The compact geometric predicate is established; fidelity of a pose-only or surviving-shape gameplay model remains to be measured.
