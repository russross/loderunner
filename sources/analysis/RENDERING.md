Sprite rendering and local texture updates
=========================================

The native port defaults to the sibling `../crt` Rust library for Apple II signal encoding, NTSC decoding, and CRT light reconstruction. `--renderer simple` retains the digital artifact decoder. Tab switches between them. Both consume the same composed scene; output selection never changes contact, game state, consumed inputs, trace events or model cycles.


Storage and appearance flow
---------------------------

Game owns `Scene` from `src/collision.rs` and `Display` from `src/render.rs`. Scene contains a 28-by-18 grid of ten-by-eleven local shapes; the last local row covers five visible rows of the 280-by-192 image. Each cell holds background B and contrast D as 110-bit masks. Foreground ink is B XOR D. Those masks answer player contact and supply rendering. There is no separate rendering bit plane or idealized actor redraw.

Gameplay calls paint_tile, overlay_shape and erase_shape, updating Scene through the Boolean identities in COLLISION.md and recording Draw work in the original-machine timing model. Status-border ink, clearing background and selecting high scores use the same scene/layer operations. Cargo decodes sprites zero through 101 from pinned assets at build time. Supported original writes share their set high color-phase bit and translated low-bit shape; non-gameplay image entries 102 and 103 are outside this port.

Overlay adds ink; erasure restores background over its enclosing fourteen- or twenty-one-pixel span aligned to seven-pixel groups. Foreground replacement can cut through nearby actors. Background changes may leave foreground unchanged by changing contrast correspondingly. Status-border ink occupies rows 176–179, status glyphs begin at 181, and visible right-edge spill reaches the left edge 64 rows below. Scene performs those translations before recording damage.

Every appearance mutation compares old/new Foreground and Background ink separately. Dirty flags are per local cell and per Layer. Repeating identical ink causes no damage. Dirty flags accumulate until successful presentation; unchanged inactive-layer damage remains pending.


CRT source and retained output
------------------------------

The SDL host owns Presentation and its streaming texture. In CRT mode it owns CrtDisplay: a `crt::Renderer`, AppleEncoder, reusable SignalLineBuffer, retained RGBA8 bytes and an optional RenderStamp. All construction is confined to displayed sessions, so headless execution does not initialize the signal pipeline. The dependency is a relative Cargo path to `../crt`; its own NOTES.md specifies signal normalization, filtering, optical integration and numerical assumptions.

For each affected scene scan line, the adapter gathers row_bits from the selected Layer and packs forty HGR bytes. Each byte contains seven consecutive ink bits, LSB first, with bit seven set. This retains the original blue/orange HGR phase instead of inferring a voltage signal from already-colored RGB. AppleProfile::IieNtscV1 and Sampling::Eight are selected. The same color-enabled HGR path renders gameplay and the game's sprite-based status/high-score text.

Rows intersecting dirty local cells are encoded completely. Source samples are compared inside the library; only the differing interval plus electrical support is decoded. The library retains linear light, expands optical dependencies across neighboring scan lines and reports physical-output damage rectangles. There are no application-level filter halos, per-tile phase resets, or independently blended CRT tiles.

The first frame, changed Layer, renderer switch and newly loaded Scene require all source rows to be submitted. A reload replaces Game's Scene and Display but retains the host's renderer; Display.full_upload ensures old ink is explicitly cleared from the retained source. After switching modes Presentation creates a new texture and invalidates Display. Source selection and output mode never affect the game's Draw journal.

CrtDisplay owns persistent output bytes; a successful RenderStamp corresponds to those bytes and their SDL texture. `prepare` submits affected rows and renders into this buffer. Presentation uploads each reported rectangle using the complete output-width-times-four pitch, copies the whole texture, then presents. Only after successful upload and copy does it acknowledge scene damage, clear display flags and retain the new stamp. On failure the old stamp and dirty flags remain, so retries regenerate and upload pending changes. Texture dimensions/format are physical drawable size and SDL RGBA32 (byte RGBA).


Simple output
--------------

Display keeps its original RGB24 cache. A set pixel adjacent to another set pixel is white. Isolated ink alternates blue [32,96,255] and orange [255,128,0] by global parity. A one-pixel gap uses its left neighbor's parity. Other pixels are black. This path has one-pixel horizontal color support and no vertical color dependency.

`Display::prepare` groups dirty cell runs into rectangles, adds one horizontal pixel on each side and clips to the image. Equal adjacent runs merge vertically. It recomputes only those RGB pixels. Initial frames, layer changes, reloads and switching back from CRT require the full 280×192 cache because the previous output may have acknowledged scene damage without updating the simple cache.

`Display::paint` requires a 280×192 RGB24 texture and uploads rectangles with the full 840-byte stride. It clears the canvas black, copies the texture into a centered aspect-preserving destination rectangle and presents. An expose requests another copy without rebuilding or uploading the cache. Scene damage clears only after successful upload and copy.


Window and input flow
---------------------

The SDL window starts at 5× native image size, 1400×960 window-coordinate units. It is resizable and high-DPI aware. Unmodified number keys 1–5 snap to multiples of 280×192. Tab switches output modes. Key repeat does not repeatedly apply window commands. Ctrl+Q quits through the normal recording-finalization flow. The grouped startup summary lists movement, digging, pause, scores, game controls, size snapping, renderer switching and quit; --help adds startup options.

Polling queues host window actions separately from gameplay delivery. The next display checkpoint drains them, applies window size or output selection, submits audio and paints the scene. Playback still ignores live gameplay delivery while processing host commands. Number/Tab commands never reach the keyboard latch or recordings; modified keys continue through normal gameplay mapping.

Both modes preserve the existing 280:192 native picture aspect. Simple uses an integer-centered fitting rectangle; CRT ViewConfig.reference_aspect explicitly uses 280/192 and the library integrates fractional letterbox boundaries. Both sides use black bars. Resizing CRT changes the library view and CPU-buffer/texture dimensions, invalidating presentation but retaining decoded light. No source row is re-decoded solely because of a resize. Exposed or resized windows request presentation even without scene changes. Every copy covers the necessary canvas because SDL backbuffer retention is not assumed.


Validation
----------

`make test-presentation` runs the changed input-dispatch, CLI, simple-rendering and CRT integration tests. Retained CRT output is compared byte-for-byte with a newly constructed renderer across local overlays, erasures, adjacent color changes, visible edge spill, background/foreground selection, status glyphs and scene reload. Tests require local decoded/output work for sprite changes, no work for unchanged images, and zero decoded samples after resize. Geometry checks cover all five snaps and wide/tall letterboxing. CLI checks exercise both renderer selections, invalid modes, startup help and replay/recording workflows.

`make test-render-sdl` runs ignored SDL tests under the dummy video driver with a software canvas. The simple test verifies actual strided texture uploads against its cache. The CRT test verifies actual RGBA uploads against independently prepared output, then exercises window resizing, layer selection, source reload and CRT/simple/CRT switching. Release and headed checks use the same executable built by make.

The release executable also passed a headed X11 run with all five size keys, both output modes, wide/tall manual resizing, and clean Ctrl+Q termination. Captured window output was visually inspected. The generated recording contained only the gameplay Right key and excluded all size and renderer commands. This check used temporary recording/score storage and left no test game running.
