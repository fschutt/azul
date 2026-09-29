# SCR2_SCROLL_LEFTOVERS - progress (branch `wt/scr2-scroll-leftovers`, base d240a1b1d)

Nothing is compiled here (house rule). The parent compiles once.

Restarted after a power loss (2026-09-29): the first run left no commits.

## Audit (read at d240a1b1d)

| # | Walker / item | Today | Plan |
|---|---|---|---|
| 1a | `scroll_into_view::find_scrollable_ancestors` | DOM parents (+ VirtualView hops, host checked on entry) | the target's box chain (`ScrollChain`, AncestorsOnly), innermost first; after a hop the host, then the host's box chain |
| 1b | `default_actions::spatial_navigation_action` | DOM parents, self-inclusive, first CSS scroll container | innermost scroll container of the SelfAndAncestors chain |
| 1c | `positioning::find_nearest_scrollport` + `find_nearest_scroll_offset` (sticky) | two LAYOUT-parent walks with two rules (`scroll/auto` vs any offset entry) | one chain walk returns both; `hidden` counts (a scroll container) |
| 1d | `focus_cursor::spatial_navigation_containers` + `is_visible` | DOM parents | scroll containers from the chain; `contain` stays a DOM grouping |
| 2 | `cursor_rect_viewport_for` | inverse of `current_transform_values` (the VERTICAL SCROLLBAR THUMB map) along layout parents | `headless` chain resolution, forward, self-inclusive |
| 3 | `execute_translate_blit` | repaints only scrollbars painted over a mover | every item painted over a mover without moving with it (focus ring) |
| 4 | `gpu_value_damage` own frame walk | `frames` / `scrolled` beside `ScrollStack` | onto `ScrollStack` |
| 5 | `NestedDomPlacement` host transforms | not carried | only if 1-4 done |

## DONE (commits)

- d1584a0e7 chore: audit checkpoint
- 1a: RED 1421775f7 (`layout/tests/a_reveal_scrolls_only_the_boxes_that_move_its_target.rs`, 3 tests),
  FIX 22e922976 (`find_scrollable_ancestors` on `ScrollChain`; `ScrollChain::of_node`;
  cfg(test) `LayoutTree::mirroring_dom`; scroll_into_view unit fixtures mirror the DOM)

- 1b: RED c72fd920d (`layout/tests/an_arrow_reads_the_action_of_the_scroll_box_it_is_painted_in.rs`),
  FIX 0881fd44e (`spatial_navigation_action` on the chain; `scroll_chain::is_css_scroll_container` +
  `ScrollChain::innermost_scroll_container`; focus_cursor / scroll_registration twins call it)

- 1c: RED 1ca24e1b4 (3 unit tests in `solver3/positioning.rs` `with_ctx`), FIX d37bc38d6
  (`nearest_scrollport` replaces `find_nearest_scrollport` + `find_nearest_scroll_offset`; their unit
  tests adapted)

- 1d: RED 1eee1a203 (`layout/tests/a_spatial_navigation_container_is_a_scroll_box_its_node_is_painted_in.rs`),
  FIX a1a28a9d0 (`spatial_navigation_containers` + `is_visible` on the chain; focus_cursor unit fixture
  mirrors the DOM)

- 2: RED dc8abbfd3 (`layout/tests/the_ime_caret_rect_is_where_the_raster_paints_the_caret.rs`, 2 tests),
  FIX adcd74e7b (`headless::content_rect_to_screen` + private `rect_to_screen`;
  `LayoutWindow::css_transform_of` replaces 3 inline copies)

- 3: RED 9a723541e (`layout/tests/a_layout_blit_repaints_what_is_painted_over_its_mover.rs`),
  FIX b3bce7e64 (`painted_over_mover` in compositor.rs, generalising S1's scrollbar rule)

- 4: REFACTOR f4e9ea3b8 (`gpu_value_damage` reads `ScrollStack`; its `on_screen` twin of `moved_by` gone)
- 5: RED 8b36d5ee9 (`layout/tests/a_scrollbar_in_a_transformed_virtual_view_is_pressed_where_it_is_painted.rs`),
  FIX a21d17411 (`NestedDomPlacement.host_transform` + per-viewport transforms; `ScrollManager::dom_rect_to_window`;
  `ScreenMapAffine::map_rect`; `nested_dom_viewports(.., resolve_transform)`)

## IN PROGRESS

- self-review, report

## NEXT

- report `scripts/SCR2_SCROLL_LEFTOVERS_2026_09_29.md`

## Open questions

- none yet
