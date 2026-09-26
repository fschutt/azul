# Why selection breaks once a TextInput is clipped: architecture analysis (2026-09-26)

Read-only analysis. Base: `39a96b9bc`, the tip of `fix/input-bugs-2026-09-19` (PR #476),
after the TextBlock / EditHost / TextTarget refactor was integrated. Nothing was compiled
or run. Every claim below comes from reading the code and is stated so that a test can
prove it wrong.

User report (macOS, AzWidgets): *"if the single-line textinput horizontally overscrolls,
then selection STILL stops working (I can see that the text moves a bit when I try to
select, but there's no actual selection going on)."*

**Short answer.** The text path never runs. As soon as the value `<p>` overflows, the
ScrollManager builds an invisible 16px horizontal scrollbar for it, and that bar covers
the whole 11px text line. Every physical press on the text is hit-tested against the
scrollbars *first* and is taken as a scrollbar press. The drag then moves that
scrollbar's thumb: this is the "text moves a bit". No `TextSelectionClick` or
`TextSelectionDrag` is ever built. The painter knows the bar does not exist
(`scrollbar-width: none`); the ScrollManager does not. Once the text fits again, the bar
disappears (but see M1b). The coordinate conversions that the last three waves fixed sit
downstream of this gate, and the layout test harnesses never go through the gate. That is
why the layout suite is green while the device still fails.

---

## 1. Mechanisms (falsifiable)

### M1 (PRIMARY, the reported symptom): a phantom scrollbar takes the press

The DOM shape. `flat::text_input` (layout/src/widgets/themes/flat.rs:842-905) builds a
host `div` (contenteditable, tab index; on macOS and Linux `overflow-x: hidden`,
text_input.rs:370) that holds a value `<p>`. The `<p>` is the IFC root and has
`overflow-x: auto; overflow-y: hidden; scrollbar-width: none; white-space: pre;
font-size: 11px` (text_input.rs:505-513). So the `<p>` is a single line, about 13px tall.

The chain of claims:

1. Layout marks the `<p>` as needing a horizontal bar whenever the text is wider than
   the `<p>`: `check_scrollbar_necessity` sets `needs_horizontal = content.width >
   container.width + 1` for `auto` (solver3/fc.rs:10255-10259). The flag means "this
   axis overflows in a scroll container". It does not mean "a bar exists".
2. `scrollbar-width: none` sets `visual_width_px = 0` (getters.rs:5982-5998). Because
   `none` also reserves nothing, `reserve_width_px = 0` and the bar thickness in the
   layout is 0. The value lands in `ScrollbarRequirements.visual_width_px`
   (solver3/cache.rs:2444).
3. `register_scroll_nodes` registers the `<p>` because `needs_horizontal` is true
   (managers/scroll_registration.rs:194-196, 258-270). It passes thickness 0 and visual
   width 0. `register_or_update_scroll_node` stores `has_horizontal_scrollbar`, but
   nothing reads that field later (scroll_state.rs:1557-1561).
4. `calculate_scrollbar_states` builds one bar per axis for every registered state whose
   content is larger than its container (scroll_state.rs:1632-1666). It does not look at
   the style, at `has_*_scrollbar`, or at the overflow value of the axis. For this `<p>`
   the thickness falls back to `DEFAULT_SCROLLBAR_WIDTH_PX = 16.0` (scroll_state.rs:1676-1682,
   fc.rs:81), and the bar is `visible: true` unconditionally (scroll_state.rs:1720). The
   only production case that reaches this fallback is `scrollbar-width: none`. An overlay
   bar has `visual_width_px > 0` and never gets here.
   `calculate_scrollbar_states_zero_thickness_falls_back_to_the_default_width`
   (scroll_state.rs:4463) pins this behaviour as correct.
5. The horizontal track is `(inner.x, inner.bottom - 16, inner.width, 16)`
   (solver3/scrollbar.rs:293-305). The `<p>` is about 13px tall, so the track covers the
   whole visible text line, plus about 3px above it.
6. The macOS press runs `perform_scrollbar_hit_test` before anything else
   (dll/src/desktop/shell2/macos/events.rs:181). That calls `hit_test_scrollbars`
   (event.rs:12506-12531, scroll_state.rs:1811-1860), which returns
   `HorizontalThumb` or `HorizontalTrack` of the `<p>`. The handler then returns through
   `handle_scrollbar_press` (event.rs:12595-12608), and `process_window_events` never
   runs for this press:
   - no `MouseDown` event, so no `TextSelectionClick` (core/src/events.rs:5004-5037);
   - no `text_selection_drag_anchor` latch (event.rs:10871-10886).
7. Each mouse move now goes to `handle_scrollbar_drag`, because `scrollbar_drag_state` is
   `Some` (macos/events.rs:358-361). That scrolls the `<p>` by
   `pixel_delta / (track - thumb) * max_scroll` (event.rs:13154-13235). For a 180px field
   with 400px of text this is about 2.2 px of scroll per pixel of pointer travel. This is
   the "text moves a bit". A press on the track outside the thumb instead page-jumps
   (`handle_track_click`, event.rs:12637).
8. The painter skips the bar (`paint_scrollbars`: `scrollbar-width: none` → return,
   display_list.rs:7341-7347). The press is taken by a bar nobody can see.

The same routing applies on Windows (windows/mod.rs:4939), X11 (x11/events.rs:625),
Wayland (wayland/mod.rs:4564) and the dll headless backend (headless/mod.rs:2734, 8805).
A right-click, a double-click (select word) and a plain click (place caret) on an
overflowing field are taken the same way.

**Claim M1 (RED).** Take the real `TextInput` widget in a 200px-wide window with a value
wider than the field. After `register_scroll_nodes`,
`lw.scroll_manager.hit_test_scrollbars(p)` at any point `p` on the text line returns
`Some(ScrollbarHit { orientation: Horizontal, node_id: <the value p>, .. })`. Expected:
`None`. If the value fits, the same call returns `None` today (the size filter at
scroll_state.rs:1643-1657). That filter is the whole "clipped vs not clipped" difference.

**M1b: stale bar after the text fits again.** When a node does not need bars,
`register_scroll_nodes` skips it with `continue` *before* `register_or_update_scroll_node`
(scroll_registration.rs:194-196). No API removes a scroll state. So a `<p>` that stops
overflowing (the window grows back, or the text shrinks outside the reshape fast path)
keeps its last narrow container rect and wide content rect, and with them the phantom bar
and the old offset. This matches the device bug quoted at the top of
`layout/tests/textinput_resize_selection.rs` ("after growing the window back ... a click
shows no caret and a drag paints no selection, while the select-scroll-drag auto-scroll
still fires"). The "auto-scroll" there is the thumb drag.
RED: shrink so the text overflows, then grow back, then call `hit_test_scrollbars(old track
point)`. Today it returns `Some`; expected `None`.

**M1c: possible vertical phantom bar (verify).** The per-axis filter only compares sizes.
If the `<p>`'s content height ever exceeds its padding box by any amount (line-box
rounding, a caret strut), a 16px *vertical* bar appears at its right edge, even though
`overflow-y: hidden`.

### M2: why the green tests never saw M1 (harness parity)

- The layout harness's `click` is documented as "Simulate the shell's press", but it only
  runs the hover hit test and `process_mouse_click_for_selection`
  (textinput_resize_selection.rs:285-289). The shell's first step is skipped. The file's
  own header records "All of this is GREEN, which is itself a finding". The phantom bar
  is present in that harness, because it calls `register_scroll_nodes`, but no step
  consults it.
- The layout e2e runner has no scrollbar-press routing at all (grep `layout/src/e2e/runner.rs`).
- A scripted press does not go through the gate either, even inside the shipped dylib.
  `DebugEvent::MouseDown` (layout/src/e2e/full.rs:13669-13697) only writes `left_down`
  through `modify_window_state`, and the dll's `ModifyWindowState` arm
  (event.rs:5137-5215) goes straight to the event pass without
  `perform_scrollbar_hit_test`. So AZ_E2E JSON on the real app cannot reproduce M1 either.
  Only the physical `handle_mouse_down` has the gate. This is the
  [[harness_cannot_reproduce]] shape again.

### M3: drag autoscroll never scrolls the field (will surface once M1 is fixed)

`auto_scroll_timer_callback` anchors on `get_focused_node()`, which is the **host**
(event.rs:431-435). It then asks `find_scroll_target(host)` =
`ScrollManager::find_scroll_parent(.., SelfAndAncestors)`, which walks the host and its
DOM **ancestors** (event.rs:457-468, callbacks.rs:6280-6303, scroll_state.rs:1141-1150).
The value `<p>` is a *child* of the host, so it is never found. The target is the page's
scroller, or nothing. The comments at event.rs:449-456 and callbacks.rs:6271-6274 assume
the anchor is the `<p>`, and that assumption is false. The caret reveal anchors on
`mc.block.container_dom_node()` (the `<p>`, window.rs:14069-14079), so the reveal and the
autoscroll use two different "which box does a text gesture scroll" rules.
RED: with focus on an overflowing TextInput, `find_scroll_target(dom, host)` returns the
page or `None`; expected the value `<p>`. Behaviour-level RED: a drag held 20px past the
field's right edge for 10 timer ticks leaves the `<p>`'s offset unchanged; expected an
increase of about `0.66 * 900 px/s * tick`.

### M4: "moves a bit" on a plain click near an edge (minor, by design)

`TextSelectionClick` queues `ScrollSelectionIntoView(Cursor)` (core/src/events.rs:5306-5310).
`calculate_instant_scroll_delta` keeps a 5px margin (window.rs:14329). A click within 5px
of the field's left or right edge therefore nudges the text by up to 5px. This does not
destroy the selection. It is listed only so it is not mistaken for M1.

### M5: the untyped window→text conversions that are left (same class, not the reported symptom)

- `focused_cursor_for_point` (window.rs:13136-13144) subtracts only the static
  content-box origin. It ignores both the ancestor scroll and the `<p>`'s own scroll
  (review §5 #15, still open). Callers: macOS `characterIndexForPoint:` via
  `focused_byte_offset_for_point` (macos/mod.rs:3950), iOS `closestPositionToPoint`
  (ios/text_input.rs:733, 765), and the handle drag (window.rs:13359-13390).
  RED: in a TextInput scrolled by `S`, `focused_byte_offset_for_point(window point over
  char k)` returns the byte of the char `S` px further left.
- `focused_rect_for_byte_offset` / `focused_rect_for_byte_range` → `cursor_rect_for`
  (window.rs:13047-13061, 13084) return **static** layout coordinates ("no scroll
  correction"), but they are documented and consumed as "absolute window coordinates" by
  macOS `firstRectForCharacterRange:` (macos/mod.rs:3979). Only the fallback
  `get_focused_cursor_rect_viewport` applies scroll (window.rs:13830, 13852).
  RED: `focused_rect_for_byte_offset(k).origin.x - get_focused_cursor_rect_viewport().origin.x
  == S` with the caret at `k`. Expected 0. (The IME candidate window is placed `S` px to
  the right.)

### What is correct today (checked, not suspected)

This is the path from the press through the drag to the paint, once a press actually
reaches it:

- **Hit test.** `CpuHitTester::hit_test_scrolled` maps the window point through each
  node's ancestor chain, adding the ancestors' scroll (headless.rs:262-274, 448-497,
  1003-1060). `convert_cpu_hit_test_to_full` subtracts the border origin and the content
  inset, which gives `ContentBoxLocal = w + A - P - E`, with no own scroll by design
  (headless.rs:1154-1215).
- **Click.** `process_mouse_click_for_selection` → `ifc_local_point_rebased`
  (window.rs:19837, 19583-19626) → `ifc_local_point_from`, which adds the `<p>`'s own `S`
  (window.rs:19538-19553) → `TextTarget::hittest` (text_block.rs:228) →
  `UnifiedLayout::hittest_point` (text3/cache.rs:5670). The fallback path builds the same
  chain explicitly (window.rs:19940-19946).
- **Drag.** `process_mouse_drag_for_selection` → `window_point_to_ifc_local`
  (window.rs:20151, 19641-19664) computes `WindowPoint → +A → -P → -E → +S`. Containment
  is tested against the on-screen box (window.rs:20169, 19670-19687), then
  `same_caret_position` (window.rs:20201).
- **Paint.** The `<p>` pushes a scroll frame because it is `auto`
  (display_list.rs:5735-5752). `paint_selections` emits static content-box rects inside
  that frame (display_list.rs:4144-4270), and the raster subtracts the frame's offset.

`textinput_resize_selection.rs` (click + drag + `SelectionRect` + pixels on an overflowing
real widget) and `drag_selection_scroll.rs` pin this downstream path, and they are GREEN.

---

## 2. Architecture diagnosis

### 2.1 Point spaces: typed where the text path reads them, untyped where it writes

| # | Space | Typed? | Producers / consumers |
|---|---|---|---|
| 1 | window | `WindowPoint` | platform events; `window_point_to_ifc_local` |
| 2 | static layout (`w + A`) | `StaticLayoutPoint` | CPU hit tester |
| 3 | border-box local | `BorderBoxLocal` | hit convert |
| 4 | content-box local | `ContentBoxLocal` | `HitTestItem::point_relative_to_item` |
| 5 | scrolled content (IFC-local) | `ScrolledContentPoint` | the only input `hittest_point` accepts |
| 6 | IFC layout rects (caret/selection, unscrolled, content-local) | **untyped `LogicalRect`** | `get_cursor_rect`, `get_selection_rects` |
| 7 | "absolute" static rects (6 + static content origin) | **untyped**, and documented wrongly as "window" | `cursor_rect_for`, `rect_for_cursor_in`, reveal, IME range rects |
| 8 | viewport rects (7 − self-inclusive scroll, transforms) | **untyped** | `cursor_rect_viewport_for` |
| 9 | ScrollManager `container_rect` (static padding box) vs scrollbar `track_rect` (window) | **untyped**, two spaces in one manager | registration, `calculate_scrollbar_states` |
| 10 | autoscroll edge box (`container_rect` − DOM-ancestor scroll) | **untyped**, third walker | `auto_scroll_timer_callback` |

For the text path, the pointer→cursor direction has one funnel per input kind (hit item,
raw window point) and both are typed. The cursor→screen direction (rects 6-8) is not
typed, and it has two answers (7 and 8) that differ by exactly `S + A`.
`focused_cursor_for_point` is the one pointer→cursor conversion left that does not use
the funnel.

### 2.2 "Which ancestors scroll": six rules

| Rule | Where | Set |
|---|---|---|
| R1 paint | display_list.rs:5735-5752 | `scroll \| auto` (+ viewport frame). **`hidden` gets a clip only** |
| R2 CPU hit-tester chains | window.rs:12572-12601; headless.rs:480-497 | `scroll_ids` = `hidden \| scroll \| auto` (+ viewport) |
| R3 `LayoutWindow::accumulated_scroll` | window.rs:19062-19076 | layout ancestors with *any* ScrollManager state |
| R4 `ScrollManager::ancestor_scroll_offset` | scroll_registration.rs:272-302 | layout ancestors in `scroll_ids`, registered nodes only |
| R5 `ScrollManager::find_scroll_parent` | scroll_state.rs:1141-1150 | **DOM** ancestors with a state (autoscroll target and edge box) |
| R6 `node_rect_to_screen` | headless.rs:340-352 | layout ancestors in `scroll_ids` |

These rules agree only while two things hold: every node with a non-zero offset is
`scroll|auto`, and the DOM ancestor chain equals the layout ancestor chain. A programmatic
offset on an `overflow: hidden` box breaks the first. `set_scroll_position` creates a state
for any node (scroll_state.rs:1169-1186), and on macOS and Linux the TextInput host is
`hidden`. Once that happens, content is hit-tested scrolled (R2/R3) but painted unscrolled
(R1). M3 is R5 disagreeing with the reveal's own anchor rule.

### 2.3 "Is there a scrollbar here": three answers

- **Paint** (`paint_scrollbars`): reads the style, so `none` means no bar.
- **ScrollManager** (`calculate_scrollbar_states`): ignores the style, uses a 16px
  fallback, never makes a bar invisible (not even a faded-out overlay bar), and never
  drops a stale state.
- **Layout** (`ScrollbarRequirements.needs_*`): "overflows" and "has a bar" are one bool.

The pointer arbitration between scrollbar and content lives in each shell, before the
shared event pass, and not in `LayoutWindow`. So the component with the wrong answer
(the ScrollManager) is also the one no test harness exercises.

### 2.4 Why the refactors did not fix it

The spaces newtypes (2026-08-25), the TextBlock/TextTarget choke point (today) and the
scroll-frame work all hardened the pipeline *downstream* of `hit_test_scrollbars`. The
reported symptom is decided *upstream*, in an untyped, per-shell pre-dispatch step whose
geometry comes from a different producer than the painted bar. No coordinate-space
refactor could have reached it.

---

## 3. Is another architecture change needed?

**Yes, but it is not primarily "more point newtypes".** The user's symptom is a pointer
arbitration bug with two root layers.

**A. One pointer arbiter in `LayoutWindow`.** For example
`LayoutWindow::route_press(WindowPoint, button) -> PressTarget { Scrollbar(ScrollbarHit)
| Content }` (plus `route_move` for an active thumb drag). All five shells, the dll
headless backend, `layout/src/e2e/runner.rs`, the debug-server `MouseDown` and the test
harnesses would call it.
- Replaces: the five per-shell `perform_scrollbar_hit_test` gates, and the "no gate" in
  the e2e/scripted paths.

**B. Scrollbar presence as one derived value.** Split `needs_horizontal/vertical` into
`overflows_*` (scrollable) and `bar_*: Option<BarGeometry>` (style-resolved: none /
overlay (visual width, fades) / classic (thickness, buttons)). Compute it once in layout
and store it in the scroll state. Paint, `calculate_scrollbar_states`, hit testing and
`update_scrollbar_transforms` would all read it. Also:
- delete the 16px fallback;
- make registration update or remove states that stop overflowing;
- decide whether a faded-out overlay bar is hit-testable.
- Replaces: the `visual_width_px`/`scrollbar_thickness` fallback chain
  (scroll_state.rs:1676-1698) and the unread `has_*_scrollbar` fields.

**C. One `ScrollChain` per laid-out node.** A list of scroll containers above the node, in
layout-tree order, each with whether its offset moves painted content. Compute it once per
layout and consume it in R1-R6. Either give `hidden` scroll containers a real scroll
frame (CSS says they are programmatically scrollable) or keep them out of `scroll_ids`.
Pick one.
- Replaces: `accumulated_scroll`, `ancestor_scroll_offset`/`set_scroll_ancestors`, the
  DOM-hierarchy `find_scroll_parent` walk, the autoscroll's own ancestor loop, and
  `node_rect_to_screen`'s chain assembly.

**D. `TextTarget` owns both conversion directions, with typed rects.** Add
`TextTarget::point_from_window(WindowPoint) -> ScrolledContentPoint` (the hit-item variant
stays as is) and `TextTarget::rect_to_window(TextLayoutRect) -> WindowRect`. Add rect
newtypes `TextLayoutRect` (what `get_cursor_rect` / `get_selection_rects` return) and
`WindowRect`, so that a static rect can no longer be handed to an IME as window
coordinates. Also add `TextTarget::scroll_box()`, the IFC root's self-inclusive scroll
container in the layout tree, used by both the reveal and the autoscroll.
- Replaces: `focused_cursor_for_point`, `cursor_rect_for`, `rect_for_cursor_in`,
  `focused_rect_for_byte_offset`/`_range`, `cursor_rect_viewport_for`,
  `window_point_to_ifc_local`, and the autoscroll anchor.

The coordinator suggested per-space point newtypes owned by `TextTarget`. They mostly
already exist for points, and D covers the missing half (rects). A and B are what fix the
user's bug, and C removes the latent `hidden`-scroller divergence.

---

## 4. Prioritized fix plan (RED first, one commit each)

1. **Put the shell's press gate into the layout harness first**, so the device bug
   reproduces in the harness. Change `Harness::click` in `textinput_resize_selection.rs`
   (or a new `a_press_on_an_overflowing_field_selects.rs`) to call
   `lw.scroll_manager.hit_test_scrollbars(p)` first and skip the text click when it hits,
   exactly as `macos/events.rs:181` does.
   RED: real widget, 200px window, long value, press on the text, then drag 40px. The
   session stays `Cursor` at the end of the text and the `<p>` offset changes. Expected:
   `Selection::Range` covering the dragged glyphs, `SelectionRect` count > 0, offset
   unchanged.
2. **Fix M1 (the phantom bar)** as step B, the minimal version: no bar when the resolved
   width mode is `none`. Carry that into `register_or_update_scroll_node` and have
   `calculate_scrollbar_states` honour it and `has_*_scrollbar`. Rewrite the test at
   scroll_state.rs:4463 to pin the new rule.
   RED (unit): register container `(0,0,200,14)`, content `400x14`, thickness 0, visual 0,
   no bar. `hit_test_scrollbars((100,7))` returns `Some(Horizontal)` today; expected
   `None`. Step 1's test goes green.
3. **Fix M1b (stale states).** A node that no longer needs bars updates, or drops, its
   state in `register_scroll_nodes`.
   RED: overflow, then grow back. `hit_test_scrollbars(old track point)` returns `Some`
   today, expected `None`. `get_current_offset(<p>)` keeps its old value today, expected 0.
4. **Move the arbitration into `LayoutWindow` (step A).** Route all shells, the dll
   headless backend, the layout e2e runner and the debug `MouseDown` through it.
   RED (dll `#[cfg(test)]`, headless window): a scripted and a physical press on an
   overflowing field's text reach the same `PressTarget`. Today the scripted press gives
   `Content` and the physical one gives `Scrollbar`.
5. **Fix M3 (autoscroll target)** with `TextTarget::scroll_box()`, used by both the
   reveal and the autoscroll.
   RED: `find_scroll_target(dom, host)` names the page or `None` today; expected the
   value `<p>`. Or, via the timer arithmetic, a drag held past the right edge leaves the
   `<p>` offset unchanged today.
6. **Fix M5.** `focused_cursor_for_point` and the IME rects go through
   `TextTarget::point_from_window` / `rect_to_window`.
   RED: in a TextInput scrolled by `S`, the point→byte conversion is off by one glyph run
   of `S` px, and `focused_rect_for_byte_offset(k).x - get_focused_cursor_rect_viewport().x
   == S`. Expected 0 for both.
7. **Remove the `hidden` scroller paint/hit divergence** (step C).
   RED: an `overflow: hidden` box with a programmatic offset of 50px. The CPU hit test
   resolves its child 50px away from where the raster paints it.
8. **Refactor: one `ScrollChain`** (step C proper), then **typed rects** (step D). Each is
   behaviour-preserving and guarded by steps 1-7.

Tests that exist and will change meaning:
- `calculate_scrollbar_states_zero_thickness_falls_back_to_the_default_width`
  (scroll_state.rs:4463) pins M1.
- The `textinput_resize_selection.rs` header's "all GREEN" conclusion is explained by M2.
