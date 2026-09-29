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

## IN PROGRESS

- 1c RED (sticky)

## NEXT

- 1c (sticky, unit tests in positioning.rs), 1d (containers + is_visible; focus_cursor
  unit fixture -> mirroring_dom), 2, 3, 4, (5), report

## Open questions

- none yet
