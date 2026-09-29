# S1_VIEWPORT_LEFTOVERS - progress (branch `wt/s1-viewport-leftovers`, base 0a326afe5)

Nothing is compiled here (house rule). The parent compiles once.

## Step 1: audit (2026-09-29, read at 0a326afe5)

| # | Item | Verdict | Evidence |
|---|------|---------|----------|
| 1 | Scroll boxes lose the CPU blit while the PAGE is scrolled; pages with a fixed element lose the page-frame blit | STILL OPEN | `cpurender::scroll_fast_path_eligible` (compositor.rs ~1854) returns `false` for a nested frame whose on-screen clip differs from its display-list clip, i.e. whenever any enclosing frame (the page) is scrolled: the whole clip is repainted. A split frame (`pushes_per_id > 1`, a fixed/escaping box between the halves) is checked on its FIRST half only, and `overlay_rects_after_frame` counts every item after the first pop - the whole second half of the page - as an overlay, so a page with a fixed header repaints ~the whole window per scroll step. |
| 2 | VirtualView child-DOM scrollbars hit-tested in child-local coordinates | STILL OPEN | `ScrollManager::calculate_scrollbar_states` moves a track only by `ancestor_scroll_offset` (frames inside its own dom). A child dom is laid out 0-relative and composited at `headless::nested_dom_window_origin`; nothing adds that. So a child bar is found near the window's top-left (stealing presses there) and not where it is painted. |
| 3 | Thin / overlay arrow-button size: paint vs hit test | DONE (by scrollbar-presence B, af9d720c7) | Paint (`display_list.rs paint_scrollbars` -> `v_bar.button_size()`), the hit test (`calculate_scrollbar_state_from_geometry` -> `bar.button_size()`) and the GPU thumb (`gpu_state.rs update_scrollbar_transforms` -> `v_bar.button_size()`) all read `ScrollbarPresence::button_size()` = the bar's own thickness (Classic) or 0 (Overlay/None). No test pins a THIN classic bar yet - add a guard. |
| 4 | One-pass thumb lag | STILL OPEN (size); position OK | `paint_scrollbars` sizes a non-viewport thumb from `scroll_offsets` = the ScrollManager snapshot taken BEFORE this pass (window.rs ~6488); `register_scroll_nodes` publishes this pass's extent only after the display list is built (window.rs ~2801). So after a relayout that grows/shrinks a scroll box's content the thumb LENGTH is the previous layout's until the next pass. Position: `refresh_scrollbar_transforms` runs before each CPU frame from the live offset, but from `LayoutTree::scroll_extent` WITHOUT the caret gutter while the hit test uses the published size (with it) - a third number for the same thumb. |
| 5 | Move-blit risk | STILL OPEN | `execute_translate_blit` memmoves old∪new of every mover rect. `compute_patch_move_summary` excludes a mover's ANCESTORS from the exceptions ("ancestors paint below") - but an ancestor's scrollbar (and the viewport's bar) is painted ABOVE the mover. An unchanged bar is in no damage set (`changed_scrollbar_damage` only damages bars whose drawing changed), so its pixels are dragged by the mover's delta and stay. |
| 6 | R5: `find_scroll_parent` / `find_scroll_target` + autoscroll | PARTIAL | M3 (c47c53a92) made a TEXT drag autoscroll `TextTarget::scroll_box`. Still on the old DOM-ancestors-with-a-state walk (`ScrollManager::find_scroll_parent`): `CallbackInfo::find_scroll_parent` / `find_scroll_target`, the NON-text branch of `LayoutWindow::drag_autoscroll_box`, the momentum hand-off (`scroll_timer.rs` ~1074). And `scroll_box_of_layout_node` (reveal + text autoscroll) walks LAYOUT parents, not containing blocks. A fixed box answers the page; an absolute box inside a non-positioned scroll box answers that box. |
| 7 | Spatial nav: overflowing `overflow: hidden` boxes are containers | DONE (already) | `spatial_navigation_containers` (focus_cursor.rs ~2560) uses `is_scroll_container()`, which includes `Hidden` (css overflow.rs:80, getters.rs:499). Pinned by `auto_makes_a_container_of_a_scroll_container_only` (Auto, Scroll, Hidden). The audit's open item 9 is only that `hidden` can never be "manually scrolled" (spec-correct). |
| 8 | D: typed rects + TextTarget both directions | STILL OPEN | No `TextLayoutRect` / `WindowRect` in core/src/spaces.rs; `TextTarget` has `hittest` + `scroll_box` only. Twins found: `LayoutWindow::rect_for_cursor_in` == `cursor_rect_for` (same body; the first documented as "window coordinates", both are static layout). |

## DONE (commits)

- item 1: RED bda38913d (`layout/tests/a_scroll_box_keeps_its_blit_on_a_scrolled_page.rs`, 2 tests:
  pixel-equal to a full repaint + repainted area <= half the clip), FIX 162bfd260 (`ScrollStack` in
  compositor.rs; `scroll_fast_path_eligible_in`, `overlay_rects_after_frame_in`, `execute_scroll_shift`
  gained `scroll_offsets`; dll headless + e2e cpu_backend + scroll_shift_ghost.rs call sites).

- item 5: RED 62667f7ec (`layout/tests/a_layout_blit_repaints_the_scrollbar_it_dragged.rs`, shares the
  pixel helpers of the item-1 file), FIX 7a7a7a78a (`execute_translate_blit` damages every scrollbar
  crossing a blit clip + its dragged copy).

- item 2: RED 75220dde1 (`layout/tests/a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted.rs`),
  FIX f5ee65096 (`NestedDomPlacement` + `ScrollManager::{set_nested_dom_placements, dom_window_origin}`,
  `headless::nested_dom_viewports`, `scroll_registration::publish_nested_dom_placements`).

- item 4: RED c670d805f (`layout/tests/a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it.rs`),
  FIX e2e586796 (paint_scrollbars reads this layout's extent + caret gutter; `caret_scroll_node`; GPU
  updater reads the published extent; funnel refreshes thumb transforms after registration).

## IN PROGRESS

- item 6 (R5 scroll-parent search via ScrollChain)

## NEXT

6, 3 (guard), 8 (implement small or plan), report.

## Open questions

- none yet
