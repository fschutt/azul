# BUG: AzWidgets — the scrollable area is stale after a window resize (macOS), and VirtualView on resize

Investigated 2026-08-22 by READING ONLY (no build, no run) on branch
`fix/open-bugs-wave-2026-08-22`, worktree `debug-slider-scroll-2026-08-22`.
All line numbers are from that tree.

## Symptom (verbatim)

> AZWIDGETS: scrollable area doesn't get [updated] if the window has resized
> (also check virtual view)?

Reported against the DOWNLOADED AzWidgets demo on macOS (2026-08-21). The demo
runs with the desktop default backend, which is **CPU** rendering
(`dll/src/desktop/shell2/common/compositor.rs:152-175`: `AZ_BACKEND` unset and
`HwAcceleration::DontCare` → `AzBackend::Cpu`), so every finding below is about
the CPU present path unless it says otherwise.

## Status

**Root cause confirmed by reading, not by running.** There is not one bug but a
chain; the one that matches the report best is F1.

* **F1 (macOS + X11, CPU backend — the demo's configuration): after the resize
  fast path the CPU hit-tester is never rebuilt.** Layout, scroll registration,
  `max_scroll`, thumb geometry and the pixels ARE all updated — but pointer
  events (hover, click, and above all the wheel/trackpad target lookup) keep
  resolving against the pre-resize 640×480 geometry until something forces a
  full `regenerate_layout()`. Over the newly exposed part of the window the
  scroll container is simply not found: wheel does nothing, clicks do nothing.
  Clicking any widget inside the OLD area bumps `interactions` →
  `Update::RefreshDom` → full regeneration → hit-tester rebuilt → "it works
  again", which is what makes this read as "the scroll area doesn't update".
* **F2 (all backends): a scroll container that stops overflowing after a grow
  keeps its stale `ScrollManager` entry** (old rects, un-re-clamped offset). Its
  content is still presented scrolled by the stale offset, and it keeps
  answering "scrollable" to the wheel-target selection, eating events a parent
  scroller should get. Not what AzWidgets shows at 640×480 → larger (its column
  always overflows), but the same report for any app whose content fits after
  the grow.
* **F3 (all backends): `paint_scrollbars` bakes the thumb LENGTH from the
  ScrollManager snapshot taken BEFORE the pass**, i.e. last pass's content
  size; only the thumb offset is corrected later through the GPU value. Wrong
  thumb length after a resize that re-wraps content, until the next display
  list rebuild.
* **F4 (macOS, CPU, `AZ_NATIVE_BACKBUFFER` default on): `CPUView::drawRect`
  wipes its framebuffer white on every bounds change while the native
  backbuffer contract says the buffer holds frame N−1.** A grow therefore paints
  only the damage rects into a white buffer. AzWidgets is masked from this
  (its `body` background rect changes size → full-window damage) but a layout
  with fixed content would blank. Flagged; unverified on hardware.
* **VirtualView**: re-invoked on EVERY layout pass (including every resize
  frame) with reason `InitialRender` and the new bounds, so its content is NOT
  stale — but the reason is wrong, edge-scroll callbacks are re-armed from the
  current offset, the child DOM is laid out against the WINDOW viewport rather
  than the host box, a callback that returns `None` ("keep the old DOM") gets its
  content wiped, and under F1 its hit-tester placements/NodeIds are stale.
* **Why no test caught it (F6):** both harnesses are *more correct* than the
  shells. The DLL headless backend's `relayout_only()` and the e2e runner's
  `layout()` rebuild the CPU hit-tester after every relayout, neither ever sets
  `resize_only_hint` (the branch a real resize takes), and the e2e `scroll` op
  finds its target by walking `layout_results` instead of the hit-tester.

## 1. What a pure window resize does on macOS (the path, and what it skips)

1. AppKit → `windowDidResize:` (`dll/src/desktop/shell2/macos/mod.rs:2654-2700`)
   → `MacOSWindow::handle_resize(w, h)` (`macos/events.rs:864-983`).
2. `handle_resize` snapshots the baseline, writes the new size with source `Os`,
   calls `handle_compositor_resize()` (`events.rs:1150-1208`: WR
   `set_document_view` only in GPU mode; CPU mode just `setNeedsDisplay`), then
   decides the policy at `events.rs:963`:
   `self.common.request_regeneration_for_resize(old, new)`
   (`common/event.rs:2219-2250`) → `LayoutWindow::resize_needs_full_regeneration`
   (`layout/src/window.rs:1465-1503`). Full regeneration ONLY if a recorded
   `window_width_*` query flips, a harvested `@media`/inline viewport
   breakpoint is crossed, or the orientation flips. AzWidgets uses no viewport
   conditions (`StyledDom::viewport_breakpoints()` → `Some((vec![], vec![]))`,
   `core/src/styled_dom.rs:2091-2102`; none of the demoed widgets declare
   `@media`), so **every landscape→landscape resize of the demo takes the FAST
   path**: `request_resize_relayout()` latches `regen.resize_relayout`
   (`common/event.rs:2184-2186`). Only a flip to portrait would take the full
   path.
3. `process_window_events(0)` (`events.rs:977`) dispatches `WindowResize`; the
   result is floored to `RequestRedraw`; `windowDidResize` sets
   `surface_needs_update` and calls `request_redraw()` (`macos/mod.rs:7351`).
4. `CPUView::drawRect` (`macos/mod.rs:1541-1583`) resizes + white-fills its
   framebuffer on a bounds change (F4), then calls
   `render_and_present_in_draw_rect()` because `redraw_requested` is set.
5. `build_atomic_txn` (`macos/mod.rs:6547-6573`): `take_resize_relayout() &&
   !regeneration_pending()` → `common::layout::incremental_relayout_for_resize`
   (`common/layout.rs:1409-1419`, sets `layout_cache.resize_only_hint = true`)
   → `incremental_relayout` (`common/layout.rs:1424-1488`):
   `layout_results.remove(ROOT)` → `layout_and_generate_display_list` →
   `layout_window.current_window_state = new` → **`register_scroll_nodes`**
   (`:1481`). Then `request_relayout_only()`.
6. solver3 takes the reconcile-skip branch (`layout/src/solver3/mod.rs:561-580`,
   `reconcile_skipped_resize_only`, `layout_roots = {root}`), re-solves from the
   root at the new viewport, and emits the display list as a PATCH
   (`mod.rs:1322-1325` `structure_ok = last_reconcile_was_skipped || …`). Scroll
   frames and scrollbars are not patchable items
   (`solver3/display_list.rs:512-534`), so any node that emits them re-emits
   fresh — the `PushScrollFrame` clip/content size is current
   (`display_list.rs:4455-4459` reads `get_scroll_content_size(node, warm)`).
7. Back in `build_atomic_txn`: `take_relayout_only()` → **`regenerate_layout()`
   is skipped** (`macos/mod.rs:6575-6593`), `display_list_needs_rebuild = true`,
   CPU branch (`:6779-6900`): `prepare_frame_cpu()` (`layout/src/window.rs:4532`
   → `refresh_scrollbar_gpu_cache_for_cpu_frame` → `update_scrollbar_transforms`
   + `synchronize_scrollbar_opacity`), `cpu_backend.render_frame(...)`, present.

What this path SKIPS compared with a full regeneration through
`regenerate_layout_inner` (`macos/mod.rs:4719-4830`):

* the CPU hit-tester rebuild (`macos/mod.rs:4793-4800`, the ONLY
  `rebuild_from_layout_with_gpu` call in the macOS shell) → **F1**;
* `a11y_dirty` + lifecycle drain (not relevant here);
* `synchronize_scrollbar_opacity` inside the relayout — `incremental_relayout`
  calls bare `register_scroll_nodes`, not `publish_scroll_state`
  (`common/layout.rs:63-81`); harmless on the desktop because every present
  path re-syncs opacity per frame (CPU `window.rs:7331-7350`, GL
  `wr_translate2.rs:2627-2641`), but it is an asymmetry with the warm path.

If the full path IS taken with an identical DOM (breakpoint/orientation), it
lands in the pre-cascade branch of `regenerate_layout`
(`common/layout.rs:405-556`): `window_size_changed_precheck` (`:405-411`) is
true → warm relayout → **`publish_scroll_state`** (`:547`, added by 37b3067f5)
→ and `regenerate_layout_inner`'s tail rebuilds the hit-tester. That path is
fine.

## 2. Findings

### F1 — The CPU hit-tester is never rebuilt on the resize fast path (macOS, X11)

Evidence:

* `common.cpu_hit_tester` is a SNAPSHOT: `layout/src/headless.rs:49-69`
  (`node_rects`, `scroll_containers`, `dom_placements` filled only by
  `rebuild_from_layout_with_gpu`, `:678`).
* Every CPU-mode hit test reads it: `common/event.rs:2507-2531`
  (`cpu_ht.hit_test_scrolled(...)` → `convert_cpu_hit_test_to_full`).
* macOS rebuilds it in exactly one place, `regenerate_layout_inner`
  (`macos/mod.rs:4793-4800`). Neither the resize fast path
  (`macos/mod.rs:6547-6573`) nor `apply_incremental_relayout_result`
  (`:5359-5374`, the `RegenerateLayoutIncremental` arm for restyles/runtime
  edits) nor the `process_close_event` arm (`:5318`) rebuilds it. `grep
  rebuild_from_layout dll/src/desktop/shell2/macos/` → one hit.
* X11 is the same: `linux/x11/mod.rs:4553-4558` (inside
  `regenerate_layout_inner`, `:4474`) and `:4797-4803` (only when
  `vviews_rebuilt`); the fast path `:4640-4702` and both
  `ShouldIncrementalRelayout` arms (`:2961-2989`, `:3961-3976`) do not rebuild.
* Windows (`windows/mod.rs:1776-1784` in `send_frame_after_incremental_relayout`,
  called from the relayout-only arm at `:3786-3796`), Wayland
  (`linux/wayland/mod.rs:5104-5112`, "FULL or RELAYOUT-ONLY PATH: both rebuild
  the CPU hit-tester") and the headless backend (`headless/mod.rs:1661-1669`)
  DO rebuild on the relayout-only path — macOS and X11 are the outliers.

Consequence for AzWidgets: the wheel target is chosen from the LAST hit test
(`scroll_state.rs:608-657` `record_scroll_from_hit_test` →
`hover_manager.get_current()` → `hovered_nodes[..].scroll_hit_test_nodes`,
which `convert_cpu_hit_test_to_full` fills from the snapshot's
`scroll_containers`). After 640×480 → e.g. 1200×900, a pointer anywhere with
`x > 640` or `y > 480` hits nothing → no scroll, no click, no hover; inside the
old rect everything still works (with correct, fresh `max_scroll`). A widget
callback returning `RefreshDom` (every AzWidgets callback does, via `bump`)
triggers the full path and silently repairs it.

Note `b44804467` makes that repair cheaper (the identical rebuild takes the
pre-cascade path) but does not change the fact that it only happens on a
`RefreshDom`.

### F2 — `register_scroll_nodes` never forgets a container that stopped overflowing

`layout/src/managers/scroll_registration.rs:19-124`: for each layout node it
`continue`s at `:91` when `!(needs_vertical || needs_horizontal)` BEFORE
`register_or_update_scroll_node` (`:109`), and nothing removes entries. The
manager entry keeps last pass's `container_rect`/`content_rect` and an offset
that was never re-clamped (`scroll_state.rs:1109-1165` only re-clamps when it
is called, `:1137`).

Consequences (a grow that makes the content fit):

* The `PushScrollFrame` is still emitted for `overflow: auto` regardless of
  overflow (`display_list.rs:4455-4459`; `MultiValue::is_scroll` covers
  `Scroll | Auto`, `solver3/getters.rs:476-481`). The CPU renderer feeds the
  stale offset through `build_scroll_offset_map` (`scroll_state.rs:562-575`,
  called at `headless/mod.rs:482-484`) and WR through `scroll_all_nodes`
  (`wr_translate2.rs:1996-2047`, `set_scroll_offsets` is unclamped) → the
  fitting content is presented scrolled by the old offset: cut off at the top,
  blank at the bottom.
* `is_node_scrollable` (`scroll_state.rs:831-842`) and `can_consume_delta`
  (`:721-760`) still answer from the stale rects → the dead scroller keeps
  winning `select_scroll_target` (`:670-690`) over its parent.
* `calculate_scrollbar_states` (`:1167-1199`) correctly drops it from
  `scrollbar_states` (the `effective > container` filter), so the BAR
  disappears while the content stays scrolled — which looks exactly like "the
  scroll area was not updated".

Not the AzWidgets 640×480→larger case (the column is far taller than any
screen), but the same report for any app that fits after the grow, and for the
shrink→grow→shrink sequence of a live-resize drag.

### F3 — The thumb length is baked from the PRE-pass ScrollManager snapshot

`layout/src/window.rs:3767`: `scroll_offsets =
scroll_manager.get_scroll_states_for_dom(dom_id)` is taken BEFORE
`solver3::layout_document` (`:3833`), i.e. before this pass's
`register_scroll_nodes` (which runs after `layout_and_generate_display_list`
returns, `common/layout.rs:1481` / `:547`). `paint_scrollbars` then reads
(`solver3/display_list.rs:5618-5634`) the thumb's `scroll_offset` AND
`content_size` from that snapshot (`children_rect.size` = last pass's
`content_rect.size`) and bakes `thumb_bounds.size = thumb_length` into the
`ScrollbarDrawInfo` (`:5692-5745`). The GPU updater
(`managers/gpu_state.rs:117-230`) later recomputes only the OFFSET — from the
tree's fresh `used_size`/`get_content_size` — so after a resize that changes
the content height (width change → text re-wraps) the display list carries a
thumb whose length was computed against the old content and whose offset was
computed against the new one, until the next display-list rebuild. A brand-new
scroll container is unaffected (no snapshot entry → falls back to the tree,
`:5634`), which is why 37b3067f5's ribbon case did not show it.

### F4 — macOS `CPUView` breaks the native-backbuffer contract on every bounds change

`macos/mod.rs:1551-1563`: when the backing size changed, `drawRect` does
`fb.resize(w*h*4, 255)` and fills the buffer white (a `Vec::resize` also
re-strides the old rows, so the old pixels are scrambled even without the
fill). `native_target_ptr` (`:2433-2450`) then hands that same buffer to
`render_frame` with the documented promise "A single persistent buffer always
holds frame N−1 … the engine's catch-up contract is met by construction", and
`headless/mod.rs:776-790` treats a GROW as `can_reuse_previous_frame = true`
(`:528`, from `resize_preserved_pixels` — which describes the COMPOSITOR's own
root layer, `:437-456`, not the view buffer) and rasterises only the damage
into `output = ext` (`:846-851`, "the platform buffer replaces the retained
frame wholesale"). Everything outside the damage set is therefore white after a
grow. `AZ_NATIVE_BACKBUFFER` defaults to on (`headless/mod.rs:315-322`).
AzWidgets does not show it because `body`'s background `Rect` changes size →
the item diff damages old ∪ new = the whole window. The
`op-resize-grow-exposed-strip` fixture would show it on a real macOS window.
Unverified on hardware.

### F5 — VirtualView on resize

* Re-invocation: `layout_and_generate_display_list` calls
  `virtual_view_manager.reset_all_invocation_flags()` on every pass
  (`window.rs:1786-1792`); `scan_for_virtual_views` (`:4309`, `:4490-4510`) takes
  the node's fresh `used_size`; `check_reinvoke` returns `InitialRender` whenever
  the flag is clear (`managers/virtual_view.rs:352-357`). So a resize DOES
  re-invoke every VirtualView with the new bounds
  (`VirtualViewCallbackInfo::new(reason, …, HidpiAdjustedBounds{ bounds.size }…)`,
  `window.rs:5311-5335`) and `update_node_bounds` re-clamps the host offset
  against the preserved `virtual_scroll_size` (`window.rs:5263`,
  `scroll_state.rs:950-964`). Materialised rows are NOT stale.
* But: the reason is always `InitialRender`, never `BoundsExpanded`/`DomRecreated`
  (`core/src/callbacks.rs:417-428`), so an app cannot tell a resize from the
  first render; `initial_scroll_offset` is re-captured on every pass
  (`virtual_view.rs:352-356`) so edge-scroll callbacks are suppressed after any
  relayout until the user scrolls away and back; a callback returning
  `OptionDom::None` ("keep the old DOM") on a later pass is treated as the
  first render and gets an empty `div` (`window.rs:5362-5368`) because
  `layout_results.clear()` already destroyed the child — the documented
  optimisation cannot work.
* The child DOM is laid out with `viewport = window_state.size.dimensions`
  (`window.rs:3497-3500`), not the host's box — a child root with `height:
  100%` tracks the WINDOW on resize, not the VirtualView.
* Under F1 the stale `CpuHitTester` still holds the previous child DOM's
  `node_rects`/`dom_placements` (fresh child NodeIds after the pass) → events on
  the wrong node (the case the X11/Windows `vviews_rebuilt` rebuild exists for,
  `x11/mod.rs:4793-4803`; the resize path has no such rebuild). `05ecdd529`
  (content_offset in the click mapping) is correct but reads from that stale
  snapshot after a resize.
* Every live-resize frame re-invokes every VirtualView callback
  (`InitialRender`), which for a map/writer page view is real work per frame.

### F6 — Why the suites are green: the harnesses do more than the shells

* DLL headless `relayout_only()` rebuilds BOTH hit-testers after
  `incremental_relayout` (`headless/mod.rs:1661-1669`); the e2e runner's `layout()`
  calls `rebuild_hit_tester()` after every pass (`layout/src/e2e/runner.rs:254-273`).
  A resize scenario therefore cannot observe F1.
* No harness ever sets `resize_only_hint` (`grep resize_only_hint = true` →
  only `common/layout.rs:1415`); the headless resize service consumes the latch
  and calls plain `incremental_relayout` (`headless/mod.rs:1595-1614`). The
  reconcile-skip + DL-patch branch a real resize takes has unit coverage
  (`layout/tests/resize_relayout_bug.rs:395-475`, `layout/tests/dl_patch_golden.rs`)
  but none with a scroll container + live `ScrollManager` state.
* The e2e `scroll` op resolves its target by walking `layout_results`
  (`layout/src/e2e/full.rs:12447-12500`), not through the hit-tester/hover
  manager the shells use (`record_scroll_from_hit_test`), so even a
  `resize → scroll` scenario passes with a stale tester.
* Existing resize scenarios (`e2e/op-resize-grow-reflow.json`,
  `op-resize-grow-exposed-strip.json`, `op-resize-shrink-stays-full.json`,
  `op-resize-no-dom-rebuild.json`, `op-dpi-changed.json`, `op-move.json`,
  `bug-author-media-unconditional.json`) contain no scroll container and no
  post-resize pointer op. `layout/tests/test_scrollbar_detection.rs:892-1100`
  lays out FRESH windows at two sizes (no retained manager state).

## 3. What 37b3067f5 / 5a58d2e4a guarantee — and their coverage of a window resize

* `37b3067f5` fixed the pre-cascade WARM RELAYOUT branch, which returned before
  `register_scroll_nodes`/`synchronize_scrollbar_opacity`; both are now
  `publish_scroll_state()` (`common/layout.rs:63-81`, call at `:547`), and it
  made the headless frame refresh thumb transforms
  (`LayoutWindow::refresh_scrollbar_transforms`, `window.rs:4556-4580`). It
  guarantees: a layout that ADDS a scroll container publishes it. It covers a
  resize only when the resize goes through `regenerate_layout` (breakpoint or
  orientation crossed). The plain resize fast path already called
  `register_scroll_nodes` (`common/layout.rs:1481`) — and that is what the
  AzWidgets resize takes.
* `5a58d2e4a` quantises the thumb offset so sub-pixel scrolls raise no damage.
  Unrelated to resize; it does touch the same two producers as F3
  (`paint_scrollbars` + `update_scrollbar_transforms`), which "MUST agree" —
  F3 is the remaining place where they read different inputs (snapshot vs
  tree).
* Resize coverage in the DLL headless tests: `ribbon_row_stays_pixel_true_across_incremental_resizes`
  (`headless/mod.rs:3056-3130`), `real_ribbon_resize_sweep_matches_fresh_at_every_step`
  (`:3132`), `azwriter_ribbon_resize_sweep_…` (`:3290`),
  `resize_regenerates_exactly_at_harvested_breakpoints` (`:3585`). All are pixel
  or policy assertions; none asserts `max_scroll`, thumb geometry, a hit test or
  a wheel after the resize. No `"expect": "fail"` scenario exists at all
  (`grep -rl '"expect"' e2e/` → none).

## 4. AzWidgets specifics

`examples/azul-widgets/src/lib.rs:441-455`: the scroll container is the FIRST
CHILD of `body`, `Dom::create_div().with_css("display: flex; flex-direction:
column; overflow-y: auto; height: 100%; padding: 24px;")`, inside
`Dom::create_body().with_css("… background-color: #f2f4f7")`. So the node whose
state goes stale is `(DomId 0, the body's first child — NodeId 1 in a
`create_body()` tree)`; the `ScrollManager` key is `(DomId{0}, NodeId(1))` and
its `scroll_id` is the single entry in `layout_result.scroll_id_to_node_id`.
The window opens at the default 640×480 (`core/src/window.rs:1626`) and the
column (7 sections × ~4 labelled widgets) overflows at every realistic size, so
F2 does not apply to this demo; F1 does on every landscape→landscape resize;
F3 applies after any width change (captions re-wrap). Every callback in the demo
returns `RefreshDom` (`bump`, `:467-475`), which is the "it heals after a click"
behaviour. No VirtualView in this demo.

## 5. Existing known-failing scenarios about resize + scroll

None. The e2e gate supports `expect: "fail"` (`layout/tests/e2e_json.rs:34-35`)
but no scenario under `e2e/` or `layout/tests/e2e_fixtures/` uses it, and none
combines `resize` with a scroll container or a post-resize pointer op.

## 6. Fix plan (concrete)

1. **One shared finalize tail for every relayout (F1).** Add
   `CommonWindowState::finish_relayout(&mut self)` in
   `dll/src/desktop/shell2/common/event.rs` (next to `layout_borrows`) that:
   (a) `publish_scroll_state(layout_window)` (move `publish_scroll_state`/
   `register_scroll_nodes` from `common/layout.rs` to a `pub(crate)` fn), (b)
   `if let Some(cpu_ht) = self.cpu_hit_tester.as_mut() { cpu_ht.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager)) }`,
   (c) `self.a11y_dirty = true`. Make `incremental_relayout` /
   `incremental_relayout_for_resize` take `&mut CommonWindowState` (or return
   and have a single wrapper `CommonWindowState::incremental_relayout(kind)`)
   so the rebuild cannot be forgotten by a call site. Call sites to switch:
   macOS `build_atomic_txn` fast path (`macos/mod.rs:6547-6573`),
   `apply_incremental_relayout_result` (`:5359-5374`), `process_close_event` arm
   (`:5318`); X11 `:2961-2989`, `:3961-3976`, `:4640-4702`; Windows/Wayland/headless
   keep their behaviour (dedupe their local rebuilds through the helper).
   `regenerate_layout_inner` on every backend calls the same helper instead of
   its private copy. Also make `incremental_relayout` call `publish_scroll_state`
   rather than bare `register_scroll_nodes` so the fast path and the warm path
   are the same code.
2. **Prune / re-clamp dead scroll entries (F2).** In
   `scroll_registration::register_scroll_nodes`, collect the `(DomId, NodeId)`
   keys registered this pass and afterwards call a new
   `ScrollManager::retain_registered(&visited, &layout_results)` that, for an
   entry not visited: if its DOM is gone from `layout_results` → remove; if its
   node is still a scroll container (`is_scroll_container`) but no longer
   overflows → keep the entry (programmatic scroll offsets on `overflow: hidden`
   must survive) but call `register_or_update_scroll_node` with the fresh
   `container_rect`/`content_size` so `clamp` zeros the offset — simplest is to
   drop the `continue` at `:91` and register every scroll-container node that
   has `scrollbar_info` (the `calculate_scrollbar_states` filter already hides
   non-overflowing bars). VirtualView hosts stay as they are (registered via
   `update_node_bounds` + `apply_virtual_scroll_necessity`, `:72-88`).
3. **Thumb geometry from the tree, not the snapshot (F3).** In
   `paint_scrollbars` read `content_size` exactly as
   `update_scrollbar_transforms` does (`layout_tree.get_content_size`, with the
   VirtualView `virtual_scroll_size` override from the snapshot) and clamp the
   snapshot offset against the fresh `max_scroll` before computing
   `thumb_initial_transform`. Alternatively split `layout_document` so
   `register_scroll_nodes` runs between solve and emit; the first is a 10-line
   change, the second a refactor.
4. **macOS CPUView framebuffer on resize (F4).** In `CPUView::drawRect`, on a
   bounds change do NOT white-fill; set a `framebuffer_reset: Cell<bool>` that
   `build_atomic_txn`'s CPU branch consumes to skip `native_target_ptr` for that
   frame AND call a new `CpuBackend::invalidate_retained_frame()` (drop
   `last_frame`, `previous_display_list`, `previous_scroll_offsets`) so
   `render_frame` takes its full-repaint arm. Same check wherever the view's
   backing scale changes.
5. **VirtualView (F5), smaller follow-ups.** (a) In
   `reset_all_invocation_flags` keep a `relayout_generation` so
   `check_reinvoke` returns `DomRecreated` (or a new `Resized` when the bounds
   changed) instead of `InitialRender`, and do not overwrite
   `initial_scroll_offset` for those; (b) honour `OptionDom::None` for a
   non-first reason by re-using the previous child `StyledDom` (store it in
   `VirtualViewState` before `layout_results.clear()`), or document that `None`
   is only an optimisation on scroll re-invokes; (c) lay the child DOM out
   against the host bounds (`window.rs:3497-3500`: take the viewport from a
   `child_viewport` parameter threaded from `invoke_virtual_view_callback_impl`);
   (d) covered by item 1 for the hit-tester.
6. **Make the harness run the shell's path (F6).** Headless `service()` should
   call `incremental_relayout_for_resize` when the resize latch was set (so
   `resize_only_hint` is exercised), and the e2e `scroll` op should resolve its
   target through `perform_hit_test` like the shells (or add a
   `wheel_at` op that goes through `record_scroll_from_hit_test`).

## 7. How to verify

Headless test (in `dll/src/desktop/shell2/headless/mod.rs` `mod tests`, next to
`ribbon_row_stays_pixel_true_across_incremental_resizes`):

```text
resize_keeps_the_scroll_area_live():
  layout = body { bg } > div.column { overflow-y:auto; height:100%; padding:24px }
           with 40 × 60px rows                     (AzWidgets shape)
  window = make_window_sized(640, 480); regenerate_layout() ×2
  inject MouseMove(300, 300); Scroll(dy = +120) → assert offset.y == 120 (sanity)
  // the SHELL path, not the harness path:
  let full = common.request_regeneration_for_resize(640x480 → 1200x900); assert !full
  update_window_state(Os, size = 1200x900)
  assert common.take_resize_relayout()
  common::layout::incremental_relayout_for_resize(...)     // what macOS calls
  <finalize tail under test — today: nothing; after fix: finish_relayout()>
  assert layout_cache.last_reconcile_was_skipped                // the real branch
  let info = scroll_manager.get_scroll_node_info(ROOT, column).unwrap()
  assert info.container_rect.size.height == 900            // padding-box, scroll_registration.rs:36-48
  assert info.max_scroll_y == info_at_480.max_scroll_y - 420  // same content, 420px more viewport
  assert scrollbar_states[(ROOT, column, Vertical)].thumb_length > thumb_len_at_480
  // F1 — fails today on the macOS/X11 code path, passes after fix:
  let hit = common.cpu_hit_tester.hit_test_scrolled((1100.0, 800.0), ..)
  assert hit contains column
  inject MouseMove(1100, 800); Scroll(dy = +120) → assert offset.y == 240
  // F2 — content that fits after the grow:
  rows = 5; offset.y = 100 at 640x480 (content 300 > 480-48? use height 200 rows) …
  resize → 640x900 → assert get_current_offset == (0,0) and
  build_scroll_offset_map()[scroll_id] == (0,0) and !is_node_scrollable
  // F3 — width change re-wraps a long caption: compare ScrollBarStyled thumb_bounds
  // height in the patched DL against a FRESH window at 1200x900 (pixel_identity).
```

e2e scenario sketch (after item 6, since today's `scroll` op bypasses the
hit-tester): `e2e/bug-resize-scroll-area-stale.json` — `mount` the column,
`wait_frame`, `resize` 640×480 → 1200×900, `wait_frame`, `mouse_move`
(1100, 800), `scroll` (1100, 800, Δy 120), `assert_scroll #column y 120`,
`get_scrollbar_info` + `assert_response contains "thumb_length"`, and
`assert_damage_sound pixel_identity` against a `snapshot_frame` taken from a
fresh 1200×900 mount for F3. Tag it `"expect": "fail"` until items 1/3 land.

Manual (macOS, the reported setup): run AzWidgets, enlarge the window by
dragging the bottom-right corner, move the pointer into the newly exposed
bottom area, wheel → nothing moves; click a checkbox in the top-left → wheel in
the same bottom area now scrolls. Resize to a portrait aspect instead →
scrolling works immediately (orientation flip takes the full path).

## 8. Effort

* Item 1 (shared finalize tail, macOS + X11 fixed by construction): 0.5 day
  incl. the headless test; touches 4 backends but is mechanical.
* Item 2 (stale manager entries): 0.5 day incl. unit tests in `scroll_state.rs`
  and `scroll_registration`.
* Item 3 (thumb from tree): 0.25 day; 1 day if `layout_document` is split.
* Item 4 (CPUView white-fill): 0.25 day; needs a macOS run to confirm.
* Item 5 (VirtualView semantics): 1–2 days, API-visible (`VirtualViewCallbackReason`).
* Item 6 (harness parity): 0.5 day.

## 9. Overlaps / already fixed

* `37b3067f5` (warm relayout publishes scroll state) — fixes the SAME symptom
  on a different path (full regeneration with an identical DOM). Does not cover
  the fast path the AzWidgets resize takes; nothing to redo, item 1 reuses its
  `publish_scroll_state`.
* `5a58d2e4a` (thumb quantisation) — orthogonal; keep both producers calling
  `quantize_thumb_offset` when item 3 changes `paint_scrollbars`' inputs.
* `b44804467` (pre-cascade dataset merge, slider drag survives `RefreshDom`) —
  not this bug. It makes the demo's `RefreshDom` rebuilds cheaper, and those
  rebuilds are incidentally what repairs F1 today; after item 1 that
  dependency disappears. No conflict with items 1–3.
* `dd90d4938` (patched-build damage log) — not this bug. The resize fast path
  IS a patched build (`last_reconcile_was_skipped`), so its damage already goes
  through the log; item 4 only adds a "previous frame untrusted → full repaint"
  signal for the macOS view-buffer reset.
* `e59d3f351` (smooth scrolling re-triggers the VirtualView it moves) and
  `05ecdd529` (VirtualView click mapping includes `content_offset`) — both
  correct; `05ecdd529` reads the CPU hit-tester snapshot that F1 leaves stale.
* `fda08926f` (scroll physics two-writers) — unrelated.
* Not on this branch and not fixed anywhere: F1 on X11 has the same shape
  (`linux/x11/mod.rs`), so the fix should land in `common/`, not in
  `macos/mod.rs`.

Written to `scripts/BUGS_2026_08_22_scroll_area_resize.md`.
