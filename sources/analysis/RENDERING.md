Sprite rendering and local texture updates
=========================================

The native port renders composed sprite shapes directly into a retained RGB24 SDL texture. It has no HIRES device, packed display pages, graphics addresses, soft switches, inverse scanline map, or display-hole storage. The unchanged baseline in `port-original` supplies the reference output in differential tests. The native compositor preserves the visible effects of the original ordered appearance operations; it does not rebuild an idealized scene from current actor poses.


Storage and appearance flow
---------------------------

Game owns one `Scene` from `port/src/collision.rs` and one `Display` from `port/src/render.rs`. Scene holds the fixed 28-by-18 grid of local ten-by-eleven shapes. The final local row covers five visible rows; the image is 280 by 192 pixels. Each cell retains background `B` and contrast `D`, both 110-bit masks. Visible foreground ink is `B XOR D`. The same masks answer player contact and supply rendering. No separate rendering bit plane mirrors collision state.

The closed `Layer` enum selects Foreground or Background. Gameplay calls `paint_tile(layer, sprite, col, row)`, `overlay_shape(sprite, half_x, y)`, and `erase_shape(sprite, half_x, y)` on Game. These operations update Scene through the identities in `COLLISION.md`, then record the same typed Draw work for the original-machine timing model. Changing the status border, clearing background, and selecting the high-score display use explicit scene/presentation operations. Presentation never changes the collision masks or adds timing work.

Cargo converts sprites zero through 101 from the reference pattern encoding into native ten-bit rows at build time. Scene packs those rows once into local shapes. Native rendering does not generate shifted byte blocks. Every supported original shift has the same translated low-bit shape, and all written asset bytes have the same high color-phase bit. These properties are checked against the baseline for all 102 supported sprites and all seven shifts. Image entries 102 and 103 are wider non-gameplay data; only the reference renderer supports those entries.

Initialization applies terrain and actor tiles in the same reverse allocation order as gameplay initialization, overlays the player without lethal contact evaluation, and draws status. A reload creates a new Scene and Display; the existing SDL texture receives a complete upload at the new Display's first presentation. High scores replace Background and select it. Returning rebuilds Background from terrain and selects Foreground, retaining all foreground damage and actor ink. No semantic actor redraw is inserted during these transitions.


Composition details required for fidelity
----------------------------------------

Ordinary sprite overlay only adds its ink. Erasure removes its silhouette and restores background across the enclosing fourteen- or twenty-one-pixel span aligned to seven-pixel groups. The wider restoration can affect ink outside the silhouette. Foreground tile replacement can cut through nearby actors; changing Background can expose old foreground differences or produce missing-background contrast. Keeping `B` and `D` captures these effects without pixel ownership or a per-object exception list.

Status-border ink occupies rows 176 through 179. Glyphs in logical row 16 begin at row 181, crossing two local scene rows. Right-edge actor spill can place ink at the left edge 64 rows below; Scene performs that coordinate translation before marking damage. Spill reaching unused reference storage is discarded because it cannot influence visible output or legal player contact. Validated actor placements use half_x below 140 and y no greater than 181; wrapped placements remain rejected. Tile placements require columns zero through 27 and rows zero through 16.

The reference decoder's color depends only on a pixel, its immediate horizontal neighbors, and global horizontal parity. Adjacent set pixels are white. Isolated set pixels alternate blue `[32, 96, 255]` and orange `[255, 128, 0]`. A one-pixel gap between set pixels uses the left pixel's parity; other unset pixels are black. There is no vertical color dependency. The native decoder uses those same rules, with the invariant high phase bit folded into the fixed palette. This preserves the existing digital artifact decoder, not an analog NTSC simulation.


Damage and presentation transaction
-----------------------------------

Each Scene mutation compares old and new visible ink. Foreground damage is recorded only if `old_B XOR old_D` differs from `new_B XOR new_D`; Background damage is recorded only if `old_B` differs from `new_B`. Dirty flags are per local cell and per layer. Consequently a background replacement can change contrast without dirtying the displayed Foreground. Repeating an already-present overlay causes no damage. Multiple operations accumulate damage until presentation; flags conservatively retain a cell changed and subsequently restored during the same interval.

Display owns one retained RGB buffer, the selected Layer, a full-upload flag and an expose/present flag. Clock owns the Draw timing journal. `prepare(&Scene)` finds contiguous dirty-cell runs in each local row, expands them by one pixel horizontally, and clips them to the image. The expansion covers every neighboring color that changed ink can affect. Consecutive rectangles with matching horizontal bounds merge vertically. It decodes only those rectangles into the RGB buffer and returns their upload rectangles. A normal ten-pixel sprite aligned across two cells produces a 22-by-11 update, 242 pixels rather than all 53,760 pixels.

The first presentation and a changed Layer require one full-image decode/upload because the SDL texture holds the previously selected image. Selecting the same Layer does not invalidate it. Window expose/size events request a canvas presentation using retained texture content; they do not force an RGB rebuild or texture upload. An unchanged ordinary frame performs neither upload nor present.

`paint(&mut Scene, &mut Canvas, &mut Texture)` requires a 280-by-192 RGB24 texture. It prepares changed rectangles and uploads each from the RGB buffer using its full 840-byte row pitch, even when the rectangle is narrow. SDL retains texture pixels outside those rectangles. After uploading, it copies that texture to the window canvas and presents. Canvas copy covers the window when presenting because SDL does not promise that backbuffer contents survive a present; CPU decoding and texture transfers remain local.

Only after successful texture uploads and canvas copy does Display acknowledge the selected layer's damage and clear its full-upload/present flags. An error leaves the pending damage intact for retry. Inactive-layer dirty flags remain pending. The native SDL loop presents at the same model-clock checkpoint as audio submission; capture and headless replay keep appearance state and timing work without submitting texture updates.


Checks and evidence
-------------------

`make -C port check` runs formatting, Clippy and portable tests. The differential suites cover the existing 90-interval schedule and an extended 180-interval schedule on all 150 original boards, 40,500 intervals in total. They compare both complete visible ink planes and verify the reference's phase-bit invariant, which together determine every RGB pixel under the shared decoder. State, occurrence traces and calibrated gameplay costs also match. Packed display holes no longer belong to the native contract.

Lifecycle tests prepare and acknowledge the incremental Display after each interval and compare every RGB pixel against the reference's selected image. They cover digging on both sides, refill, burial/resurrection, menu selection, death/reload and changing guard rosters. Dedicated rendering cases cover idempotent overlays, hidden background updates, erasure, neighboring colors across cell boundaries, status placement and right-edge spill. Collision probes and mixed-operation tests continue to use the unchanged packed baseline as their oracle.

`make -C port test-render-sdl` uses SDL's dummy video driver and software canvas to exercise actual streaming-texture updates. It reads back the canvas after explicitly copying the texture, avoiding any assumption about backbuffer retention. Readback exactly matches the RGB cache after initial upload, local overlays/erasures, artifact-color changes, visible edge spill, layer switches and expose presentation. CLI tests also run the native SDL application and replay its resulting trace without SDL. All nine `make -C mame-validation port-test` tests pass; these validate gameplay, timing and validation-stream behavior, not analog display color.
