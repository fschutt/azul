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

- none yet

## IN PROGRESS

- 1a RED

## NEXT

- 1a fix, 1b, 1c, 1d, 2, 3, 4, (5), report

## Open questions

- none yet
