# S1: scroll and viewport leftovers (2026-09-29)

Branch `wt/s1-viewport-leftovers`, based on `0a326afe5` (the pushed tip of
`fix/input-bugs-2026-09-19`). **Nothing was compiled or run** (house rule). Each
fix has a RED test commit before it, except item 3 (already fixed: a guard) and
the item-8 refactor (behaviour-preserving). The expected REDs below come from
reading the code.

Sources: `VIEWPORT_SCROLLBAR_2026_09_26.md` (gaps + "open design choices"),
`SCROLL_CHAIN_2026_09_28.md` (R5, performance), `SCROLLBAR_PRESENCE_2026_09_28.md`,
`SPATIAL_NAVIGATION_AUDIT_2026_09_28.md` §8.9, `SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md`
§3 D, `TEXT_SCROLL_REVEAL_FIX_2026_09_28.md` §8.

## 1. Audit and outcome

| # | Item | Before (evidence) | Now |
|---|------|-------------------|-----|
| 1 | Scroll boxes lose the CPU blit while the PAGE is scrolled; a page with a fixed element loses its blit | OPEN. `scroll_fast_path_eligible` refused every frame nested in a scrolled one and repainted the whole clip. A SPLIT frame was checked on its first push only, and `overlay_rects_after_frame` counted the whole rest of the page as overlays. | FIXED |
| 2 | VirtualView child-DOM scrollbars hit-tested in child-local coordinates | OPEN. `calculate_scrollbar_states` moved tracks only by frames inside their own DOM. A child's bar was pressed near the window's top-left corner. | FIXED |
| 3 | Thin/overlay arrow-button size: paint vs hit test | DONE by scrollbar-presence (af9d720c7). Paint, the GPU thumb and the hit test all read `ScrollbarPresence::button_size()`. | GUARD added |
| 4 | One-pass thumb lag | OPEN. `paint_scrollbars` sized non-viewport thumbs from the pre-pass snapshot. The GPU updater used an extent without the caret gutter. A full rebuild presented the thumb position computed before registration. Positions during a scroll or drag were already live. | FIXED |
| 5 | Move-blit risk | OPEN. `execute_translate_blit` dragged an ancestor's (or the viewport's) scrollbar with a mover, and no damage repainted an unchanged bar. | FIXED |
| 6 | R5: `find_scroll_parent` / `find_scroll_target` + autoscroll | PARTIAL. M3 fixed only the TEXT drag. `ScrollManager::find_scroll_parent` (a DOM walk) still served `CallbackInfo`, non-text drags and the momentum hand-off. The reveal's `scroll_box_of_layout_node` walked layout parents. | FIXED |
| 7 | Spatial nav: overflowing `overflow: hidden` boxes are containers | DONE already. `spatial_navigation_containers` uses `is_scroll_container()`, which includes `Hidden`. Pinned by `auto_makes_a_container_of_a_scroll_container_only`. The audit's §8.9 note is only that `hidden` is never "manually scrolled" (spec). | no change |
| 8 | D: `TextLayoutRect` / `WindowRect`, `TextTarget` both directions | OPEN | DONE, and it fixed the static selection handles (an open item of the text-scroll-reveal report) |

## 2. What was built

**1. The CPU scroll blit.** All in `layout/src/cpurender/compositor.rs`.
- `ScrollStack` is the ONE walk for "where is this item on screen". It tracks the frames and `PushClip`s open at each item, their summed offsets, and the part of the screen they let show.
- `collect_scroll_shifts` uses it. It also cuts every clip to what its enclosing frames and clips let show, so a memmove never drags another box's pixels.
- `scroll_fast_path_eligible_in(.., scroll_offsets)` and `overlay_rects_after_frame_in(.., scroll_offsets)` read every item on screen:
  - The content of a split frame is every push of it.
  - What a later push of the same frame paints is not an overlay.
  - A clip that does not match the map is refused, as before.
- `execute_scroll_shift` gained `scroll_offsets` as a new last argument. Call sites: dll `headless/mod.rs`, `layout/src/e2e/cpu_backend.rs` and `layout/tests/scroll_shift_ghost.rs`.
- The old `scroll_fast_path_eligible` and `overlay_rects_after_frame` stay as thin wrappers with no offsets known (their unit tests are unchanged).

**5. The move-blit.** `execute_translate_blit` repaints every scrollbar item (`ScrollBarStyled` or `ScrollBar`) that crosses a blit clip. It repaints the bar where it is and where its pixels were dragged. The translate hint is refused while anything is scrolled, so the list's bounds are the painted ones.

**2. VirtualView bars.**
- New `scroll_state::NestedDomPlacement` holds a nested DOM's `origin` at rest, its `host_frames` (the scroll containers whose live offsets move it) and its `viewports`.
- New `ScrollManager::{set_nested_dom_placements, dom_window_origin}` and a private `dom_shows_at`.
- `register_scroll_nodes` publishes the placements (`publish_nested_dom_placements`) from `headless::nested_dom_viewports`. That uses the same records as `nested_dom_window_origin`.
- The VirtualView's own scroll is kept symbolic: origin = box + materialized origin, with the view among the `host_frames`. A view scroll only patches `content_offset`, without a layout.
- `calculate_scrollbar_states` lifts every track to window space. Both hit tests skip a bar outside its DOM's viewports.
- `remap_node_ids` drops placements that name the remapped DOM.

**4. The thumb.**
- `paint_scrollbars` reads the extent off THIS layout by registration's rule: `scroll_extent` plus `caret_scroll_extent`. Only a VirtualView keeps the callback size from the snapshot.
- New `scroll_registration::caret_scroll_node(&[CursorLocation])` is the one rule for which box is the caret box. Registration, the reshape fast path and paint all use it.
- The GPU updater uses the extent the manager holds for a box registration described. For any other box it uses this layout's extent.
- The layout funnel calls `refresh_scrollbar_transforms()` right after `register_scroll_nodes`.

**6. R5.** The one rule is `LayoutWindow::scroll_box_in_chain`: the innermost link of the node's `ScrollChain` that the ScrollManager scrolls. This follows containing blocks.
- Entry points: `scroll_box_of_layout_node`, which is unchanged for callers, and the new `pub fn scroll_box_of_node(dom, node, Inclusivity) -> Option<NodeId>`.
- `drag_autoscroll_box` (non-text branch) and `CallbackInfo::find_scroll_container` (behind `find_scroll_parent` / `find_scroll_target`, and so the momentum hand-off) use it. The text branch and `find_scrollable_ancestor` get it through `scroll_box_of_layout_node`.
- `ScrollManager::find_scroll_parent` had no caller left. It is deleted, together with its 4 unit tests, which pinned the DOM rule.

**8. D.**
- core `spaces`: `TextLayoutRect` and `WindowRect` (`rect_space!`, `#[repr(transparent)]` over `LogicalRect`).
- `TextTarget::{rect_to_window, caret_rect_on_screen, point_from_window}`.
- `focused_rect_for_byte_offset` / `_range` and `focused_cursor_for_point` go through them. `block_inline_geometry` reuses `content_box_origin_of` (that was a twin).
- `focused_rect_for_cursor` had no caller and is removed.
- Fix: `selection_handle_geometry` is now in window space, so `selection_handle_at` finds the handle where it hangs in a scrolled field. The twin `rect_for_cursor_in` (the same body as `cursor_rect_for`, documented wrongly as window coordinates) is removed.

## 3. Commits (oldest first)

| Commit | Kind | What / expected RED |
|---|---|---|
| ec2be0715 | chore | audit checkpoint |
| bda38913d | RED | `a_scroll_box_keeps_its_blit_on_a_scrolled_page.rs`. Pixels equal to a full repaint (green) AND repainted area ≤ half the clip. Nested box: 12000 of 12000 px repainted today. Fixed header: ~40000 of 40000. |
| 162bfd260 | fix | item 1 |
| 62667f7ec | RED | `a_layout_blit_repaints_the_scrollbar_it_dragged.rs`. First pixel difference ~(180,41): the dragged thumb. Shares the item-1 pixel helpers (`pub(crate)`, `Raster::repaint`). |
| 7a7a7a78a | fix | item 5 |
| 75220dde1 | RED | `a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted.rs`. `hit_test_scrollbars(painted centre)` is `None`, expected `(child, box)`. |
| f5ee65096 | fix | item 2 |
| c670d805f | RED | `a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it.rs`. The painted thumb is the 400px content's (~88px), the measured one the 800px content's (~44px). The box height was corrected in 23bd8fc14: at 100px both thumbs hit the 24px minimum. |
| e2e586796 | fix | item 4 |
| 74018917b | RED | `a_drag_autoscrolls_the_box_its_containing_block_scrolls_in.rs`. Absolute box: `Some(scroller)`, expected `None`. Fixed box: `Some(page)`, expected `None`. Both for `drag_autoscroll_box` and `find_scrollable_ancestor`. |
| df204d5cc | fix | item 6 (also adds the inclusivity asserts to the test) |
| 1f7917a63 | guard | `a_thin_scrollbar_is_pressed_where_it_is_painted.rs` (expected GREEN) |
| 6b0984e4b | RED | `ime_geometry_follows_the_fields_scroll.rs::the_selection_handles_hang_under_the_carets_where_they_are_painted`. The handle is at the static x, 40px right of the caret on screen. |
| 86d0c4116 | refactor | D |
| 6186093cc | fix | handles in window space |
| 9cfaeed2a | refactor | explicit `collect()` types (compile safety) |
| 23bd8fc14 | test | the item-4 test's box is 200px tall, so its thumbs clear the minimum length (self-review) |
| 8fba1bdbc 91470a9f8 7a9734e91 7dc09c318 234253c6a 457755e0b | chore | progress checkpoints |

All new test files are appended to `layout/tests/all.rs`. `page_breaks.rs` and
`a_padded_table_cell_stays_in_its_row.rs` were not touched.

## 4. api.json

**No api.json change.** None of the touched types or functions are in api.json.
The new public items are Rust-only, like their neighbours (`WindowPoint`,
`ScrollManager` and `TextTarget` are not in api.json either):
- `azul_core::spaces::{TextLayoutRect, WindowRect}`
- `azul_layout::managers::scroll_state::NestedDomPlacement`
- `ScrollManager::{set_nested_dom_placements, dom_window_origin}`
- `scroll_registration::caret_scroll_node`
- `headless::nested_dom_viewports`
- `LayoutWindow::scroll_box_of_node`
- `TextTarget::{rect_to_window, caret_rect_on_screen, point_from_window}`
- `cpurender::{scroll_fast_path_eligible_in, overlay_rects_after_frame_in}`

**Changed:**
- `cpurender::execute_scroll_shift` takes `&ScrollOffsetMap` as a new last argument.
- `LayoutWindow::content_box_origin_of` and `window_point_to_ifc_local` are now `pub(crate)`.

**Removed:**
- `ScrollManager::find_scroll_parent` (public, Rust-only).
- The private `LayoutWindow::{rect_for_cursor_in, focused_rect_for_cursor}`.

If the autofix scans `core/src/spaces.rs`, it may propose the two rect types. It
does not list the point types there today, so it should not.

## 5. Least sure to compile

1. `compositor.rs` `overlay_rects_after_frame_in`: `let Some(&(_, end)) = frame_pushes(..).first() else { .. };`. This borrows a temporary `Vec` in a let-else and copies a `usize` out. It should be fine.
2. `compositor.rs` `frame_pushes`: `matches!(it, PushScrollFrame { scroll_id: sid, .. } if *sid == scroll_id)` on a `&&DisplayListItem`, which relies on match ergonomics through two references.
3. `const fn moved_by` (f32 arithmetic in a const fn, stable since 1.82, as `spaces.rs` already relies on).
4. `window.rs` funnel: `#[cfg(feature = "std")] if !self.skip_gpu_sync { .. }`, an attribute on an `if` statement. The same funnel already does this with `#[cfg(feature = "a11y")] if ..`.
5. `window.rs` `selection_handle_geometry`: a closure using `?` over `self.text_target(block)?`.
6. `scroll_state.rs`: `Option::is_none_or` (1.82+, already used in `headless.rs`). The `retain` closure chains `host_frames.iter()` with `viewports.iter().flat_map(|(_, frames)| frames.iter())`.
7. `scroll_registration.rs` `publish_nested_dom_placements`: the `filter_map` closure borrows `layout_window.virtual_view_manager` (a disjoint capture through `&mut LayoutWindow`) before the `&mut` call to `set_nested_dom_placements`.
8. Tests:
   - `a_layout_blit_...` builds a `ScrollbarDrawInfo` literal (every field listed; `ComputedTransform3D::IDENTITY`).
   - `a_thin_scrollbar_...` partially moves `Option<WindowLogicalRect>` fields out of an owned `ScrollbarDrawInfo`.
   - The item-5 test imports the item-1 test's helpers through `super::a_scroll_box_keeps_its_blit_on_a_scrolled_page::{..}`, since both are sibling modules of `all.rs`.
9. `display_list.rs` `paint_scrollbars`: `crate::managers::scroll_registration::caret_scroll_node(&self.ctx.cursor_locations)` (`&Vec` to `&[_]` coercion).

## 6. Commands for the parent

```
cargo test -p azul-layout --test all -- a_scroll_box_keeps_its_blit_on_a_scrolled_page \
  a_layout_blit_repaints_the_scrollbar_it_dragged a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted \
  a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it \
  a_drag_autoscrolls_the_box_its_containing_block_scrolls_in a_thin_scrollbar_is_pressed_where_it_is_painted \
  ime_geometry_follows_the_fields_scroll scroll_shift_ghost viewport_scroll_frame viewport_scrollbar \
  scroll_chain scrollbar_presence a_selection_drag_autoscrolls_the_box_its_text_scrolls_in \
  a_press_on_an_overflowing_field_selects click_into_a_virtual_view_page vview_contenteditable_e2e \
  caret_follows_typing caret_reveal_and_session_identity
cargo test -p azul-layout --lib -- cpurender::compositor managers::scroll_state managers::gpu_state press_router scroll_timer timer
cargo test -p azul-core --lib -- spaces
cargo test -p azul-dll --lib -- headless        # damage_scroll_takes_the_memmove_fast_path et al.
```

Then run the full layout `all` and `lib` suites, the dll suite and the e2e corpus:
- More frames now keep the scroll blit.
- Scroll-shift clips are cut to their enclosing clips.
- Thumbs are sized from this layout on a box's first pass (`scroll_extent` includes the end padding).

Goldens with a padded scroll box's thumb may move by a pixel or two.

**RED pass.** Revert each fix alone and run its test:

| Revert | Test that must fail |
|---|---|
| 162bfd260 | Item 1. Also put the old 7-argument `execute_scroll_shift` calls back in the test. The RED commit bda38913d already has them. |
| 7a7a7a78a | item 5 |
| f5ee65096 | item 2 |
| e2e586796 | Item 4. Keep the test at 23bd8fc14 (the 200px box). |
| df204d5cc | Item 6. Drop the `scroll_box_of_node` asserts, which that commit added. |
| 6186093cc | Item 8 handles. The test is 6b0984e4b. |

## 7. What is left

**Walkers not on the ScrollChain yet.** The scroll-chain report's list minus R5:
- `scroll_into_view::find_scrollable_ancestors` (the reveal chain across VirtualView hops);
- `default_actions::spatial_navigation_action`;
- `positioning::find_nearest_scrollport` / `find_nearest_scroll_offset` (sticky);
- spatial navigation's container chain, which walks DOM parents.

**Host transforms.**
- `NestedDomPlacement` does not carry host transforms. A child DOM's bar inside a transformed host is hit-tested untransformed. The same is true for a bar inside a transformed box of its own DOM, which `calculate_scrollbar_states` never handled.
- `cursor_rect_viewport_for` applies the INVERSE of ancestor GPU transforms (and walks layout parents). That looks wrong: the raster applies the forward transform. `TextTarget::rect_to_window` inherits this. Fixing it should switch to `headless::node_rect_to_screen`'s chain resolution. I did not fix it, because it has no RED yet.

**Nested VirtualViews.** Two levels deep: the outer view's own scroll is baked into the inner DOM's placement at registration (only the innermost view's scroll is symbolic).

**Painted handles.** The DL paints the End handle at the end of the last selection rect. `selection_handle_geometry` uses the end caret. These agree up to the caret width, which is pre-existing.

**Items painted over a mover.** Item 5 covers scrollbars only. Other items an ancestor paints over a mover are still not repainted by the blit. An example is an ancestor's outline or focus ring (step 10).

**Twin walks.** `gpu_value_damage` keeps its own frame-offset walk (`frames` / `scrolled`) beside `ScrollStack`. It could move onto `ScrollStack` in a follow-up.

**`e2e/cpu_backend.rs`.** Outside the briefed files, I changed only the one `execute_scroll_shift` call. The runner (E1's `runner.rs`) is untouched.
