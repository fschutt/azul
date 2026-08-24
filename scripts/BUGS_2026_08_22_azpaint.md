# AzPaint on macOS — bug investigation (2026-08-22)

Read-only investigation of the five AzPaint symptoms reported on macOS. No
source was edited and no cargo was run; the metaball finding was additionally
reproduced with a 60-line Python port of the demo's CPU kernel (scratchpad,
not committed). Line numbers are for branch `fix/open-bugs-wave-2026-08-22`
(worktree `debug-slider-scroll-2026-08-22`).

The demo lives in `examples/azul-paint/src/lib.rs` (1124 lines, single file;
`main.rs` only calls `azul_paint::start()`).

One fact frames everything below: **the desktop default render backend is
CPU** (`dll/src/desktop/shell2/common/compositor.rs:166-176`, `AzBackend::resolve`
step 3 returns `Cpu`; GPU needs `AZ_BACKEND=gpu|auto` or `HwAcceleration::Enabled`).
On that path `LayoutWindow::prepare_frame_content` invokes every
`RenderImageCallback` with `OptionGlContextPtr::None`
(`layout/src/window.rs:4589`), so in AzPaint `gl_usable == false`, the GPU
metaball shader is never compiled, and the demo's CPU branches
(`render_metaballs`, the `cpu_image` cache) are what the user is looking at.

## TL;DR

| # | Symptom (verbatim) | Status | Root cause | Effort |
|---|---|---|---|---|
| 1 | "macOS menu doesn't show" | **Confirmed by reading — engine bug** | `run()` installs the launch-time stub menu *after* `MacOSWindow::new` already installed the DOM menu bar; later re-applies are hash-guarded, so the stub wins forever | 0.5 h + real-Mac check |
| 2 | "cmd-click — no right-click menu (macOS)?" | **Confirmed by reading — engine gap** | Context menu is only resolved/presented on `rightMouseUp:`. Ctrl+click arrives as `mouseDown:` with the Control flag and is treated as a plain left click; Cmd+click is not a context-menu gesture on macOS (and not mapped either). No `menuForEvent:` | 1-2 h + real-Mac check |
| 3 | "metaballs: weird edges on metaball merge (math wrong?)" | **Confirmed — demo math bug, reproduced numerically** | Infinite-support kernel `1/(q+0.18)` is hard-truncated at an axis-aligned box where it is still 0.10-0.20 (10-20 % of the iso threshold, up to 2/3 of the AA band) → step discontinuities along every dab's bbox edges → stair-steps / spurs / horizontal tears exactly where blobs merge. Secondary: the 1/d² tail makes dense strokes balloon | 2-4 h |
| 4 | "resizing: canvas doesn't auto-resize — do we have a working 'node was resized' event?" | **Two halves.** Demo: CPU branch never checks the cached image size. Engine: `ComponentEventFilter::NodeResized` exists but is **dead code in production** (reconcile is fed empty layout maps; the resize fast path never reconciles) | demo 0.5 h; engine 4-8 h |
| 5 | "should respond to cmd+O / cmd+S" | **Not implemented in the demo**; engine supports it only via native NSMenu key equivalents (blocked by #1) | demo 1 h after #1; engine-side accelerator dispatch 4-6 h |

---

## 1. "macOS menu doesn't show"

### What the demo does
`layout()` builds a `Menu` (File / Edit / View, each item with a callback) and
attaches it to the root: `Dom::create_body().with_menu_bar(menu)`
(`examples/azul-paint/src/lib.rs:823-843`, `858-864`). The labels are constant,
the callback `RefAny` is the same app-data handle every rebuild, so
`Menu::get_hash()` is stable across rebuilds (`core/src/menu.rs:77-82`;
`RefAny` hashes its `sharing_info` pointer, `core/src/refany.rs:616-620`).

### How macOS installs it
- `MacOSWindow::new` runs the initial layout (`dll/src/desktop/shell2/macos/mod.rs:4604`)
  and then `window.apply_menu_bar_from_dom()` (`:4613`).
- `apply_menu_bar_from_dom` (`:5974-5997`) reads `NodeId::ZERO` of the root
  DOM (`root.get_menu_bar()`) and calls `set_application_menu` (`:5955-5966`),
  which calls `MenuState::update_menubar_if_changed` (`macos/menu.rs:186-199`)
  and, **only if the hash changed**, `NSApp.setMainMenu(...)`.
- `regenerate_layout_inner` re-applies after every DOM regeneration (`:4821`),
  again hash-guarded.

### Root cause (confirmed by reading, ordering bug)
`dll/src/desktop/shell2/run.rs`:

```
602  MacOSWindow::new_with_fc_cache(...)      // -> apply_menu_bar_from_dom -> setMainMenu(File/Edit/View)  ✔
641  app.setActivationPolicy(Regular);
645  crate::desktop::shell2::macos::setup_main_menu(&app, mtm);   // -> setMainMenu(app + Edit stub)  ✘ overwrites
651  app.finishLaunching();
```

`setup_main_menu` (`macos/mod.rs:2493-2504`) is the launch-time stub (app
submenu + MWA-B14 Edit submenu). Its own comment says "Per-window menu bars
built from the DOM's menu_bar replace this via apply_menu_bar_from_dom", but
the call order is inverted: the DOM menu is installed first (inside window
creation) and the stub replaces it. Every later `apply_menu_bar_from_dom` sees
`menu.get_hash() == current_hash` and returns without touching `NSApp.mainMenu`.
Result: the user sees only the generic "AzPaint | Edit" bar — File/View never
appear. (The DOM menu would only come back if the app changed its menu
contents at runtime.)

Git: `5a665b2f5 feat(macos): populate global menu bar from DOM menu_bar` was
"Verified: cargo check" only; `c6f5912bf` added the stub earlier. Never run on
hardware together.

Side effect once fixed: `create_menubar_nsmenu` (`macos/menu.rs:322-342`)
*also* prepends the engine Edit menu, so AzPaint will show
`AzPaint | Edit | File | Edit | View` (two Edit menus). Cosmetic; either merge
user items into the standard Edit submenu when the user menu has a top-level
"Edit", or drop the engine one when the user supplies one.

### Fix
1. In `run.rs`, move `setActivationPolicy` + `setup_main_menu` **above** the
   `MacOSWindow::new_with_fc_cache` call (line 602), so the stub is installed
   first and the window's DOM menu replaces it. (Same for windows created later
   from callbacks — they already run `apply_menu_bar_from_dom` in `new`.)
2. Make `set_application_menu` identity-aware, not only hash-aware: if
   `NSApp.mainMenu()` is not the `MenuState.ns_menu` pointer, call
   `setMainMenu` even when the hash is unchanged. This also fixes multi-window
   (switching key windows must swap the bar; `windowDidBecomeKey:` at
   `macos/mod.rs:2714` should call `apply_menu_bar_from_dom`).
3. Optional: de-duplicate the Edit menu.

### Verify
- Real macOS run required (AppKit). `AZ_LOG=debug` prints
  `[MacOSWindow] Application menu updated` once at creation; after the fix you
  should see File/View in the bar immediately.
- Cheap guard without a Mac: a source-contract test in the existing style of
  `dll/src/desktop/shell2/common/event.rs:9926` (`the_macos_menu_runloop_is_entered_from_exactly_one_place`)
  asserting that in `run.rs` the text `setup_main_menu(` occurs before
  `new_with_fc_cache(`.

### Effort
0.5 h code, plus a real-Mac smoke.

---

## 2. "cmd-click — no right-click menu (macOS)?"

### What the demo does
`canvas.with_context_menu(ctx_menu)` and `body.with_context_menu(ctx_menu)`
(`lib.rs:848-861`): "Metaballs mode" / "Normal paint mode".
`use_native_context_menus` defaults to `true` on macOS
(`core/src/window.rs:1167`), so the native `NSMenu` pop-up path is used.

### Engine ingress on macOS (confirmed by reading)
- The views declare `mouseDown:`/`mouseUp:`/`rightMouseDown:`/`rightMouseUp:`/
  `otherMouseDown:` (`macos/mod.rs:867-903`, same for CPUView at `:1641-1690`).
  Neither view implements `menuForEvent:` and neither sets `.menu`.
- The context menu is resolved **only** in `handle_mouse_up` when
  `button == MouseButton::Right` (`macos/events.rs:287-291` →
  `resolve_context_menu` `:1216-1258` → `queue_native_context_menu_at_position`)
  and presented **only** by `view_handlers::right_mouse_up` (`macos/mod.rs:298-317`,
  `take_pending_context_menu` → `present_pending_context_menu` `:3052-3065`).
- `view_handlers::mouse_down` / `mouse_up` (`:173-240`) never look at
  `event.modifierFlags()`; the left handlers never take a pending menu.

### Root cause
- **Ctrl+click** (the macOS secondary-click convention): AppKit delivers it to
  a view that has no `menuForEvent:` menu as `mouseDown:` with
  `NSEventModifierFlags::Control` set — NOT as `rightMouseDown:`. azul treats
  it as a left click: on the canvas it starts a stroke, no menu.
- **Cmd+click**: not a context-menu gesture anywhere on macOS (it is
  open-in-background / multi-select). azul does not map it on any backend
  either (no `Command` check in any mouse handler). If the user literally
  tried Cmd+click, "no menu" is expected behaviour; the thing that *should*
  work and does not is Ctrl+click.
- A genuine secondary click (right button, two-finger click, corner click)
  goes through `rightMouseDown:`/`rightMouseUp:` and should present the menu.
  **UNVERIFIED on hardware in this wave** — if that also fails on the user's
  machine, the next suspect is `get_first_hovered_node()` returning the body
  rather than the `<img>` (both carry the same menu here, so it would still
  show something).
- Convention nit: macOS shows context menus on mouse **down**; azul shows them
  on right mouse **up** (Windows convention). Not a bug, but worth aligning
  when touching this code.

### Demo nit (overlap with the stuck-input report)
The canvas registers `HoverEventFilter::MouseDown` / `MouseUp`
(`lib.rs:812-814`), which match **any** button (`core/src/events.rs:1266,1274`).
A right click therefore begins and ends a one-dab stroke before the menu
opens. Use `LeftMouseDown` / `LeftMouseUp`.

### Fix
Primary (small): in `view_handlers::mouse_down` (`macos/mod.rs:173`), if
`event.modifierFlags()` contains `Control`, route to the `Right` handlers
(`handle_mouse_down(event, MouseButton::Right)`) and latch a
`ctrl_click_as_right: bool` on `MacOSWindow`; in `mouse_down`'s matching
`mouse_up`, consume the latch and run the `right_mouse_up` body (incl.
`take_pending_context_menu` + `present_pending_context_menu`). The latch is
needed because the Control key may be released before the button. This is what
SDL's Cocoa backend does.

More native (larger): implement `menuForEvent:` on both views — hit-test at
the event location, build the `NSMenu` via `recursive_build_nsmenu`, return it
— and let AppKit pop it up for right-click *and* ctrl-click on mouse-down.
Must respect the park-then-present invariant tested in
`common/event.rs:9924-9950` (the pop-up call site count is asserted to be 1).

### Verify
Real Mac only (AppKit event synthesis): `cliclick kd:ctrl c:400,400 ku:ctrl`
over the canvas; and a plain two-finger click. Headless cannot express
modifier-on-mouse. A headless test can still cover the shared half:
`HeadlessEvent::MouseDown{Right}` + `MouseUp{Right}` on a node with
`with_context_menu` must produce the window-based menu (`show_window_based_context_menu`).

### Effort
1-2 h for the latch approach; 3-4 h for `menuForEvent:`.

---

## 3. "metaballs: weird edges on metaball merge (math wrong?)"

### Screenshot (read with the Read tool)
Dark blobs on a light background; isolated dabs are clean anti-aliased discs
(~17 logical px ≈ 35 px on the 2× screenshot); the merged clusters (top-left,
bottom) have horizontal tear lines, flat cuts, stair-stepped outlines and thin
horizontal spurs that end abruptly. Consecutive dabs along a stroke are ~30
logical px apart (see "Side findings: sparse dabs").

### The implementation
`render_metaballs` (`lib.rs:383-448`) — the CPU path, which is the one that
runs by default (see framing note):

```
393   let r = (BASE_RADIUS * (0.6 + p.pressure * 2.0)).max(2.0);   // 9.6 px at pressure 0.5
401   let reach = ax.max(ay) * 2.2;
402-405  bbox = [p.x - reach, p.x + reach] × [p.y - reach, p.y + reach]   (axis-aligned, clamped)
406-419  for every pixel IN THE BBOX:
412       q = (lx/ax)² + (ly/ay)²
413       c = 1.0 / (q + 0.18)          // kernel, infinite support
415       field[idx] += c                // pixels OUTSIDE the bbox get +0
429   a = smoothstep(0.85, 1.15, field)   // iso-surface at 1.0, AA band width 0.30
```

The GPU twin (`METABALL_FS_BODY`, `lib.rs:466-497`) evaluates the same kernel
for **all** balls at every fragment (no bbox), capped at 128 most-recent balls
(`lib.rs:454, 572-578`). It is never used on the default CPU backend.

### Root cause (confirmed numerically)
The kernel `1/(q+0.18)` decays like 1/d² and never reaches zero, but it is only
accumulated inside a square of half-width `2.2·r`. At that cutoff it still
contributes

- `1/(2.2² + 0.18) = 0.199` at the middle of a box edge,
- `1/(2·2.2² + 0.18) = 0.101` at a box corner,

i.e. 10-20 % of the iso threshold (1.0) and up to 2/3 of the anti-aliasing band
(0.30). Every dab therefore injects a **step discontinuity** into the summed
field along the four axis-aligned lines `x = p.x ± 21.1`, `y = p.y ± 21.1`.
An isolated dab is unaffected (its own iso-radius is `0.905·r ≈ 8.7 px`, far
inside its box, and 0.2 < 0.85 never crosses the band on its own). But wherever
a neighbour's partial sum is already near the band — i.e. exactly where blobs
merge — the step pushes the contour across the threshold along a pixel row or
column: stair-steps, flat cuts through a neighbour's rim, spurs that end at a
box edge, and with several dabs a lattice of horizontal/vertical terraces.
With dabs ~30 px apart (the screenshot's spacing), dab B's box edge at
`B.x − 21` cuts through dab A's rim at `A.x + 9`, which is precisely the
"flat cut + notch" look on the screenshot.

Python port of the loop (two rows of dabs 30 px apart), shipped kernel with the
bbox truncation vs. a compact-support kernel:

```
shipped (1/(q+0.18), bbox 2.2r)              Wyvill (1-q/R²)³, R=2r, T=0.5
            .##############+++..........+++##     .##############.              .##############.
            .################++........++###     .+##############+.            .+##############+.
           .+#################+++....+++####     .################.            .################.
           .###################++++++++#####     +################+            +################+
           +####################++++++######     +################+            +################+
          .+######################++########     +################+.          .+################+.
           +############################### ...  .#################.          .##################.
            .############################++...    +###############++.      .++##################++.
            .############################+++....  .+################++....++######################+
             +############################+++...    .+####################################################+++..
             .+###########################++++++...   ..+++##########################################++++++++
             ..+############################...       ..++######################++....++######################++
              ..+++#########################++..
```

The shipped output shows the terraced right edge (`+++...`, `++++++...`) and
notches at box columns; the compact kernel gives discs with small organic
bridges. (Script: scratchpad `metaball_sim.py` / `metaball_sim2.py`, pure
Python, reproducible in a minute.)

Secondary defects in the same function:
- **Saturation / ballooning**: because of the 1/d² tail, a *line* of dabs sums
  to a field that decays only like 1/d, so a slowly drawn stroke (dabs every
  1-2 px) is above threshold out to ~5 radii — strokes get much fatter the
  slower you draw, and three dabs two radii apart already fuse into one fat
  blob (first sim). The bbox truncation partly hides this, which is why the
  look "almost works" for isolated dots only.
- **CPU/GPU mismatch**: with `AZ_BACKEND=gpu` the shader has no truncation, so
  the picture changes (fatter, no tears) and older dabs vanish past 128 balls.
- The iso-radius (`0.905·r`) is only ~17 px in diameter at mouse pressure 0.5,
  fine, but note `r` here uses `0.6 + 2·pressure` while the SVG export uses
  `0.4 + 0.6·pressure` (`lib.rs:304, 338`) — the export does not match the
  raster.

### Fix (demo, `render_metaballs` + `METABALL_FS_BODY`)
1. Use a **compact-support** kernel whose value is exactly 0 at the box
   radius, so the bbox clip is exact and the field is continuous. Classic
   choice (Wyvill / soft objects): `c = (1 − q/R²)³ for q < R², else 0`, with
   `R = 2·r` in the ellipse-normalised frame (`reach = R·max(ax,ay)`), iso
   threshold `T = 0.5` and AA band `[0.45, 0.55]`. With these constants an
   isolated dab keeps a visible radius of `≈0.9·r` (`r_vis = R·sqrt(1 − T^(1/3))`),
   so the brush size does not change. Cheaper C0 alternative if the look of
   `1/(q+ε)` must stay: `c = max(0, 1/(q+0.18) − 1/(2.2²+0.18))`.
2. Apply the **same** kernel/constants in the GLSL body (one `const` block
   shared by both strings) so `AZ_BACKEND=gpu` renders the same image.
3. To stop the width from depending on dab density, either weight each dab by
   `min(1, spacing/r)` or (better) evaluate the distance to the *segment*
   between consecutive points (capsule metaballs) instead of per-point balls.
   The SVG export already approximates dabs as ellipses; keep them in sync.
4. Drop the `a <= 0.0 || f <= 1e-4` double guard to a single `a <= 0.0`.

### Verify
- Headless/unit (no Mac needed): add a `#[test]` in `examples/azul-paint`
  (the crate already has unit tests for `strokes_to_svg`) that calls
  `render_metaballs` with two dabs 30 px apart on a 100×60 canvas and asserts,
  for every row through the blobs, that the alpha profile has exactly one
  rising and one falling edge per blob (no notch), and that the iso-radius of a
  dab is the same with and without a neighbour 30 px away. A pixel golden of the
  three-dab cluster is a good second guard.
- Real Mac: draw slowly with the mouse, merge two strokes; edges must be
  smooth. Also run once with `AZ_BACKEND=gpu` to confirm parity.

### Effort
2-4 h for the kernel + shared constants + test; +4-6 h optional for the
incremental rasterisation in "Side findings" (which also fixes the sparse dabs).

---

## 4. "resizing: canvas doesn't auto-resize — do we have a working 'node was resized' event?"

### What the canvas is
A single `Dom::create_image(ImageRef::callback(render_canvas, cache))` with
`flex-grow: 1; position: relative; overflow: hidden` (`lib.rs:775, 805-811`).
The box is CSS-determined (a callback image has intrinsic size 0×0,
`core/src/resources.rs:1414`; layout deliberately ignores produced frames for
sizing, `layout/src/overlay.rs:523-537`).

### How the engine drives it (confirmed by reading)
- Every rendered frame, on both backends, `invoke_image_callbacks_into_overlay`
  (`layout/src/window.rs:4954-5027`) walks the layout tree and invokes each
  callback with `HidpiAdjustedBounds { logical_size: node.used_size, .. }` —
  the **current laid-out size**. CPU: `prepare_frame_cpu` from
  `macos/mod.rs:6795`; GL: `process_image_callback_updates` → `prepare_frame_gl`
  (`wr_translate2.rs:2911`).
- A macOS resize: `windowDidResize:` (`macos/mod.rs:2654-2700`) →
  `handle_resize` → coalesced resize fast path in `build_atomic_txn`
  (`:6543-6570`) → `incremental_relayout_for_resize`
  (`common/layout.rs:1409-1422`) → display list rebuild → frame → callback
  invoked with the new `used_size`. So the engine half works: the callback IS
  told the new size on the next frame.
- The produced frame goes through `apply_image_change`
  (`layout/src/window.rs:4875-4896`): same `ImageRef` id → `Unchanged`;
  different id → `Paint` tier, display list patched in place.
- The CPU rasteriser scales whatever image it gets to the node rect with
  nearest-neighbour (`layout/src/cpurender/raster.rs:3806-3807, 3829-3831`).

### Root cause A — demo (the visible symptom on the default CPU backend)
`render_canvas_inner` (`lib.rs:634-731`):

- GPU branch: `need_alloc = texture.size != (w, h)` → re-allocates and forces
  `rendered_rev = 0` (`lib.rs:680-688`). Correct.
- CPU branch: `if cache.rendered_rev != rev || cache.cpu_image.is_none() || export_path.is_some()`
  (`lib.rs:715`) — **no size comparison**. After a resize the callback returns
  the cached `RawImage` at the *old* dimensions; the chokepoint sees the same
  `ImageRef` (unchanged), and the rasteriser stretches the old bitmap into the
  new box. Strokes look stretched/blurry and the click→pixel mapping is off
  until the next `rev` bump (next stroke / undo), when the whole canvas snaps
  to the new size. That is "canvas doesn't auto-resize".

Fix: mirror the GPU branch — also re-rasterise when
`cache.cpu_image.as_ref().map(|i| i.get_size()) != Some(LogicalSize::new(w, h))`
(one condition added to line 715). 0.5 h.

### Root cause B — engine: `NodeResized` is dead code
There *is* a subscribable event: `EventFilter::Component(ComponentEventFilter::NodeResized)`
(`core/src/events.rs:2329`, public in `api.json`), mapped to `EventType::Resize`
with `LifecycleEventData { previous_bounds, current_bounds }`
(`core/src/events.rs:1239, 2659`). The video widget subscribes to it
(`layout/src/widgets/video.rs:167, 306`) to resize its decoder target. But:

1. The only production emitter is `reconcile_dom` (`core/src/diff.rs:653-672`),
   which compares `old_layout[node].size` with `new_layout[node].size`. Both
   production callers pass **empty** maps:
   - `dll/src/desktop/shell2/common/layout.rs:751-752`
     (`// Build layout maps for reconciliation (empty for now - we just need node moves)`),
   - `layout/src/window.rs:7404-7411` (`begin_reconciliation`, `&empty_layout` twice).
   With empty maps both rects are `LogicalRect::zero()`, sizes compare equal,
   nothing fires. The unit test `core/src/diff.rs:3236-3250` even pins this
   ("zero-vs-zero bounds must not be treated as a resize") — the production
   situation is the tested no-op case. The sibling helpers that *would* work
   (`detect_lifecycle_events`, `create_resize_event`, `core/src/events.rs:1448, 1590-1615`)
   have no non-test callers.
2. Even with real maps it could not fire at reconcile time: the new tree is not
   solved yet when `reconcile_dom` runs (the FLIP code at
   `common/layout.rs:801-824` says so explicitly and captures "First" rects to
   pair them after the solve).
3. A **window resize** never reconciles at all: the fast path
   (`incremental_relayout_for_resize`) re-solves the existing `StyledDom`, so
   no `DiffResult.events` exist, and `dispatch_pending_lifecycle_events`
   (`common/event.rs:6766`) only runs inside the `regenerate_layout` loop
   (`common/event.rs:2889-2935`), not after the fast path.

So the answer to "do we have a working node-was-resized event" is: it exists in
the API and in unit tests, it has never fired in a running app.

Fix (engine):
- Emit `Resize` **after the solve**. The natural hook is section 5b in
  `regenerate_layout` (`common/layout.rs:1141-1190`), which already has, per
  `NodeMove`, the old rect (`anim_first_rects`) and the new rect
  (`get_node_bounds`). For every move whose new node has a `NodeResized`
  callback and whose size changed (`size_changed`, `core/src/events.rs`), push a
  `create_lifecycle_event(EventType::Resize, …)` into
  `layout_window.pending_lifecycle_events`. Delete/ignore the rect comparison
  inside `reconcile_dom` or keep it for the unit tests only.
- Resize fast path: in `incremental_relayout` (`common/layout.rs:1424`),
  snapshot `used_size` for nodes carrying a `NodeResized` callback before
  `layout_and_generate_display_list`, compare after, push the same events, and
  call `dispatch_pending_lifecycle_events` from the fast-path callers (macOS
  `build_atomic_txn`, and the X11/Wayland/Win32/headless equivalents) — with
  the same bounded "regenerate if a callback asked for it" contract.
- Do the same for `LayoutWindow::begin_reconciliation` (`layout/src/window.rs:7380`)
  so the headless/E2E runner matches.
4-8 h including a headless test; the video widget gets its resize for free.

### Verify
- Headless (no Mac): `dll/tests/headless_lifecycle.rs` style —
  register `NodeResized` on a `flex-grow: 1` child, `HeadlessEvent::Resize{..}`,
  assert the counter increments and `current_bounds` matches the new layout.
  This test **fails today** and is the proof of B.
- Headless: a `RenderImageCallback` that records `info.get_bounds().get_logical_size()`;
  after `Resize`, the last recorded size must be the new one (passes today —
  confirms the engine half of A) and, with the demo fix, the produced
  `ImageRef::get_size()` must follow it.
- Real Mac: resize the AzPaint window; strokes must keep their shape and
  position, and a click after the resize must land under the cursor.

---

## 5. "should respond to cmd+O / cmd+S"

### Status: not implemented in the demo; engine support is partial
- The demo binds no keyboard events at all (`lib.rs:805-817` only mouse/touch;
  `start()` `lib.rs:1062-1068`) and sets no `StringMenuItem.accelerator`
  (`lib.rs:826-843`). Nothing can react to Cmd+O/Cmd+S.
- macOS engine ingress is fine: Cmd is tracked as `LWin`/`RWin` via
  `flagsChanged:` (`macos/events.rs:820-860`), and a Cmd+letter `keyDown:`
  reaches the view (`macos/mod.rs:1009-1093` → `handle_key_down`
  `macos/events.rs:639-724`) as long as no NSMenu key equivalent consumed it
  first (AppKit tries `performKeyEquivalent:` on the main menu before
  `keyDown:`). `KeyboardState::super_down()` / `primary_down()` exist
  (`core/src/window.rs:364-377`) but are **not exported** in `api.json`
  (`KeyboardState` has fields only); bindings must scan
  `pressed_virtual_keycodes` for `LWin`/`RWin` themselves.
- Native accelerators: `macos/menu.rs:391-394` → `set_menu_item_accelerator`
  (`:415-506`) maps `VirtualKeyCode::LWin/RWin → Command`, letters → key
  equivalent, so `accelerator = [LWin, O]` produces a real Cmd+O menu item that
  fires the item's callback through `menuItemAction:` → `handle_menu_action`
  (`macos/mod.rs:5713`). This requires the menu bar to actually be installed —
  **blocked by #1**.
- No engine-side accelerator dispatch exists anywhere: `matches_accelerator`
  (`core/src/window.rs:388`) has only unit tests; `AcceleratorKey`
  (`core/src/window.rs:1674-1694`) has `Ctrl/Alt/Shift/Key` but no
  `Super`/`Primary`; the Windows backend (`windows/menu.rs`) ignores
  `accelerator` (no ACCEL table), and the Linux software menubar
  (`layout/src/widgets/menubar.rs`) only stores it. So `accelerator` is
  display-only everywhere except native macOS.
- `StringMenuItem` exposes the `accelerator` field but no `with_accelerator`
  builder in `api.json` (`StringMenuItem.functions`: `with_child`,
  `with_children`, `with_callback`…).

### Fix
Demo (after #1): set `accelerator = Some(VirtualKeyCodeCombo { keys: [LWin, O] })`
on "Import image…" and `[LWin, S]` on "Export PNG…" (`[LWin, LShift, S]` for
SVG); optionally a `WindowEventFilter::VirtualKeyDown` callback that checks
`pressed_virtual_keycodes` for `LWin|RWin` + `O|S` as a menu-independent path.
Note `on_import`/`on_export` open a **blocking** `FileDialog` from inside the
callback — that is the "file-picker async" item of the open-bugs wave; a Cmd+O
that opens a modal from `keyDown:` has the same re-entrancy exposure as from a
menu action.

Engine: (a) add `AcceleratorKey::Super` and `AcceleratorKey::Primary`;
(b) add `StringMenuItem::with_accelerator` to `api.json` via the autofix
workflow; (c) shared accelerator dispatch in the common event pass: on
`VirtualKeyDown`, walk the root's `menu_bar` (and the software menubar on
Linux), fire the first leaf whose chord matches, `prevent_default`-style skip
when a native menu already consumed the key (macOS/Windows native bars).

### Verify
- Headless: `HeadlessEvent::KeyDown{LWin}` then `KeyDown{O}`; a
  `VirtualKeyDown` callback must observe `super_down()`. With engine dispatch,
  the menu callback counter must increment.
- Real Mac: Cmd+O must open the import dialog; with the menu installed the
  File menu must display "⌘O".

### Effort
Demo 1 h (after #1); engine 1 h for `AcceleratorKey`, 4-6 h for shared dispatch.

---

## Side findings (not in the report, but visible in the screenshot or on the path)

- **Sparse dabs ("string of pearls")**: the screenshot's dabs are ~30 logical px
  apart although `on_pointer_move` records every `MouseOver`. AppKit coalesces
  `mouseDragged:` by default when the app falls behind, and each move returns
  `Update::RefreshDom` → full DOM regeneration + the CPU metaball pass
  re-rasterises the **whole** canvas from scratch (`field` + `acc` = 16 B/px
  allocated and filled per frame, `lib.rs:386-387`), then `render_image` copies
  the bitmap again (`raster.rs:3723 bytes.to_vec()`) and blits it per pixel at
  2× on retina. Tens of ms per frame → coalescing → dabs too far apart to
  merge at all. Fix: keep `field`/`acc` in `CanvasCache` and add only the new
  dab(s) (the kernel is additive), full re-raster only on undo/clear/mode
  change/resize. 4-6 h; also makes the merged look dramatically better since
  dense dabs then actually merge.
- **Right-click starts a stroke** (see #2): `MouseDown`/`MouseUp` match any
  button.
- **GPU path (only with `AZ_BACKEND=gpu`)**, untested here: `compile_metaball_gpu`
  (`lib.rs:517-546`) never checks `GL_COMPILE_STATUS`/`GL_LINK_STATUS` — a
  failed shader yields `Some(MetaballGpu)` with a dead program and a blank
  canvas instead of the CPU fallback; `draw_arrays` (`lib.rs:598`) runs without
  binding a VAO on a 3.2 Core context (`macos/mod.rs:2936-2940`), which is
  `GL_INVALID_OPERATION` unless WebRender happened to leave one bound (the
  engine's own `Texture::paint_stroke`, `core/src/gl.rs:3204-3292`, has the
  same dependency); `ImageRef::gl_texture(t.clone())` (`lib.rs:711`) mints a new
  `ImageRef` id every frame, so the chokepoint reports a changed image and
  repaints every frame even when idle.
- **Export/raster mismatch**: metaball radius in the SVG export
  (`0.4 + 0.6·p`, `lib.rs:338`) differs from the raster (`0.6 + 2·p`, `lib.rs:393`).
- **HiDPI**: the canvas is deliberately rasterised at logical resolution
  (`lib.rs:606-626`), so on a 2× display every edge is upscaled — this makes
  the stair-steps in #3 twice as visible; not a bug by itself (documented
  trade-off in the file).
- **Duplicate Edit menu** once #1 is fixed (see #1).
- **Overlap with the stuck-input / "clicking sometimes selects text" report**:
  the header is a plain `<p>` in a flex row (`lib.rs:788-792`) and the canvas
  callbacks fire for every button; nothing else in this demo touches text
  selection. Nothing here changes that agent's conclusions.

## Verification matrix

| Item | Headless / unit (Linux CI) | Needs a real macOS run |
|---|---|---|
| 1 menu bar | source-contract test on `run.rs` ordering | yes — AppKit menu |
| 2 ctrl-click | right-button context menu via `HeadlessEvent::MouseDown{Right}` | yes — modifier-on-mouse needs AppKit; `cliclick kd:ctrl c:x,y ku:ctrl` |
| 3 metaballs | `render_metaballs` profile/golden test in `examples/azul-paint` (pure CPU) | optional visual + `AZ_BACKEND=gpu` parity |
| 4a canvas resize | callback-bounds + produced-size test with `HeadlessEvent::Resize` | optional |
| 4b NodeResized | lifecycle counter test (fails today) | no |
| 5 shortcuts | `KeyDown{LWin}`+`KeyDown{O}` keyboard-state test; menu callback counter with engine dispatch | yes — native key equivalents |

## Effort summary

| Item | Effort |
|---|---|
| 1 menu ordering + identity check (+ Edit de-dup) | 0.5 h (+0.5 h) |
| 2 ctrl-click latch (or `menuForEvent:`) | 1-2 h (3-4 h) |
| 3 metaball kernel + shared GPU constants + test | 2-4 h |
| 3' incremental canvas rasterisation (sparse dabs) | 4-6 h |
| 4a demo CPU size check | 0.5 h |
| 4b engine `NodeResized` (both paths + dispatch + test) | 4-8 h |
| 5 demo accelerators / key handler | 1 h |
| 5' engine `AcceleratorKey::Super/Primary` + shared accelerator dispatch + `with_accelerator` API | 5-7 h |
