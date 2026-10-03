# SCR2: scroll / viewport leftovers after S1 (2026-09-29)

Branch `wt/scr2-scroll-leftovers`, based on `d240a1b1d`. **Nothing was compiled
or run** (house rule). Only `rustfmt --check` was used, as a parse check. Every
behaviour change has a RED test commit before its fix. The expected REDs below
come from reading the code. The first run was lost to a power cut before any
commit; this run started again from the base.

Sources: `S1_VIEWPORT_LEFTOVERS_2026_09_29.md` §7, `SCROLL_CHAIN_2026_09_28.md`,
`VIEWPORT_SCROLLBAR_2026_09_26.md`.

## 1. What was built

All five items are done.

**1. The walkers are on the ScrollChain.** Each one now reads the node's
`ScrollChain`, which follows containing blocks. None of them walks DOM or layout
parents for the scroll question any more.

| Walker | Before | Now |
|---|---|---|
| `scroll_into_view::find_scrollable_ancestors` (the reveal) | DOM parents; after a VirtualView hop, the host and then the host's DOM parents | The target's box chain (AncestorsOnly), innermost first. After a hop: the host, then the host's box chain. |
| `default_actions::spatial_navigation_action` | DOM parents, self-inclusive | `ScrollChain::of_node(.., SelfAndAncestors).innermost_scroll_container()` |
| `positioning::find_nearest_scrollport` + `find_nearest_scroll_offset` (sticky) | Two layout-parent walks with two rules: `scroll \| auto` for the scrollport, "any entry" for the offset | One `nearest_scrollport` returns both. `hidden` now counts as a scroll container. A fixed box gets no page offset. |
| `focus_cursor::spatial_navigation_containers` + `is_visible` | DOM parents | Scroll containers come from the chain. `is_visible` clips by the scrollports on the chain. |

- **`contain` still walks the DOM.** `spatial-navigation-contain: contain` is the
  author's grouping, so it stays a DOM-ancestor rule. The DOM walk there only
  puts the two kinds of container in order; every chain link is also a DOM
  ancestor.
- **Sticky runs before scroll ids exist.** At that point of layout nothing has a
  scroll id yet. So `nearest_scrollport` walks the chain with a one-entry map in
  which the root stands for the viewport's frame. That is enough to tell "painted
  in the page" from "fixed". Every scroll container clips, so it is a chain link
  even without an id.
- **New helpers in `solver3/scroll_chain.rs`:**
  - `ScrollChain::of_node`: the chain of a DOM node's first box.
  - `ScrollChain::innermost_scroll_container`.
  - `is_css_scroll_container`: the style test.
- **Twins removed.** `focus_cursor::is_css_scroll_container` and the style half of
  `scroll_registration::is_scroll_container` were copies of that style test. Both
  now call it. `LayoutWindow::enclosing_scroll_id` uses `of_node`.
- **Unit fixtures.** The unit tests in `scroll_into_view.rs` and `focus_cursor.rs`
  used an empty layout tree, which has no chain. They now use the new
  `#[cfg(test)] LayoutTree::mirroring_dom`: one unsized box per DOM node, parented
  like the DOM.

**2. The IME caret rect.** `cursor_rect_viewport_for` now calls
`headless::content_rect_to_screen`, so `TextTarget::rect_to_window` gets the same
answer.
- **The old code used the wrong map.** It applied the inverse of
  `current_transform_values`, one layout ancestor at a time. That map holds the
  **vertical scrollbar thumb** offsets, not CSS transforms.
- **The effects:** a caret under `transform` was reported untransformed, and a
  caret on a scrolled page was moved by the page's thumb offset.
- **The new mapping** is the raster's forward rule, `T(static - scroll)`, over the
  caret's own chain (the block itself included).
- **Refactor in `headless.rs`:** `node_rect_to_screen` (the node's box) and
  `content_rect_to_screen` (the node's content) are two thin entries over one
  private `rect_to_screen(.., Inclusivity, ..)`.
- **One transform lookup:** `LayoutWindow::css_transform_of` (`pub(crate)`)
  replaces three inline copies in window.rs.

**3. Items painted over a mover.** This is the layout blit
(`execute_translate_blit`), not the scroll blit. The scroll blit already repaints
every overlay after the frame through `overlay_rects_after_frame_in`, the focus
ring included.
- **The fix generalises S1's scrollbar rule.** The new `painted_over_mover`
  returns every scrollbar, plus every painting item after the mover's first item
  that lies inside no mover's destination. Examples: an ancestor's focus ring,
  or an ancestor's inline content.
- **Each item is repainted twice:** where it is, and where the blit dragged its
  pixels.

**4. `gpu_value_damage` reads `ScrollStack`.** Its own `frames` / `scrolled` walk
and its `on_screen` copy of `moved_by` are gone. This is a behaviour-preserving
refactor.

**5. Host transforms for nested-DOM bars.**
- **Placement:** `NestedDomPlacement` gained `host_transform`, and each viewport
  now carries its own transform (`headless::NestedDomViewport`).
- **Resolution:** the transforms are resolved when the placement is published, by
  `nested_dom_viewports(.., resolve_transform)`. The frames stay symbolic, so live
  offsets still apply.
- **One lift:** `ScrollManager::dom_rect_to_window` is used by the tracks,
  `dom_window_origin` and `dom_shows_at`. It places the rect, subtracts the live
  offsets, then maps it through `ScreenMapAffine::map_rect` (the AABB of the four
  corners).

## 2. Commits (oldest first)

| Commit | Kind | What / expected RED |
|---|---|---|
| d1584a0e7 | chore | audit checkpoint |
| 1421775f7 | RED | `a_reveal_scrolls_only_the_boxes_that_move_its_target.rs`:<br>• fixed toolbar: the page is scrolled 500 -> 0;<br>• absolute box: the escaped scroller gets +70;<br>• fixed VirtualView: the page gets about -480. |
| 22e922976 | fix | 1a |
| c72fd920d | RED | `an_arrow_reads_the_action_of_the_scroll_box_it_is_painted_in.rs`: `DefaultAction::None`, expected `ScrollFocusedContainer { Down, Line }` |
| 0881fd44e | fix | 1b |
| 1ca24e1b4 | RED | 3 unit tests in `solver3/positioning.rs` (`with_ctx`, next to the other sticky tests):<br>• escaped scroller: y 200, expected 150;<br>• hidden box: y 100, expected 130;<br>• fixed box: y 80, expected 10. |
| d37bc38d6 | fix | 1c. The unit tests of the two old helpers now call the halves of the new one (see §6). |
| 1eee1a203 | RED | `a_spatial_navigation_container_is_a_scroll_box_its_node_is_painted_in.rs`:<br>• container `Some(1)`, expected `Some(0)`;<br>• visible areas `[2]`, expected `[2, 4]`. |
| a1a28a9d0 | fix | 1d |
| dc8abbfd3 | RED | `the_ime_caret_rect_is_where_the_raster_paints_the_caret.rs`:<br>• translateX(60): reported x = layout x, expected +60;<br>• scrolled page: y off by the thumb offset (about 15px). |
| adcd74e7b | fix | 2 |
| 9a723541e | RED | `a_layout_blit_repaints_what_is_painted_over_its_mover.rs`: first pixel difference at about (40,140), the dragged ring |
| b3bce7e64 | fix | 3 |
| f4e9ea3b8 | refactor | 4 |
| 8b36d5ee9 | RED | `a_scrollbar_in_a_transformed_virtual_view_is_pressed_where_it_is_painted.rs`: `hit_test_scrollbars` returns `None`, expected `(child, box)` |
| a21d17411 | fix | 5 |
| 5ad06f646 b3795d67a 3ab9d2a48 3c4df6f87 a54c49adb b518d6a74 6afbcaab2 | chore | progress checkpoints |

The 6 new integration test files are appended to `layout/tests/all.rs`. I did not
touch:
- `page_breaks.rs` or `a_padded_table_cell_stays_in_its_row.rs`;
- `layout/src/widgets/**`;
- S1's two tests. The item-3 test only imports helpers from
  `a_scroll_box_keeps_its_blit_on_a_scrolled_page`.

## 3. api.json

**No api.json change.** None of the touched names are in api.json (I checked
with grep). The Rust-only changes:

- **New:**
  - `ScrollChain::{of_node, innermost_scroll_container}`
  - `solver3::scroll_chain::is_css_scroll_container`
  - `headless::{content_rect_to_screen, NestedDomViewport}`
  - `ScreenMapAffine::map_rect`
  - `ScreenMapAffine` now derives `PartialEq`
  - the field `NestedDomPlacement::host_transform`
- **Changed:**
  - `headless::nested_dom_viewports` takes a `resolve_transform` and returns `NestedDomViewport`s.
  - `NestedDomPlacement::viewports` is now `Vec<NestedDomViewport>`.
  - `LayoutWindow::css_transform_of` is new and `pub(crate)`.
- **Removed:** the private `positioning::{find_nearest_scrollport, find_nearest_scroll_offset}`, replaced by the private `nearest_scrollport`.

## 4. Least sure to compile

1. `scroll_registration::publish_nested_dom_placements` passes the closure
   `&|dom, node| layout_window.css_transform_of(dom, node)`, where `layout_window`
   is `&mut LayoutWindow`. It is an immutable capture, used next to
   `&layout_window.layout_results`.
2. `headless::nested_dom_viewports`:
   - the `transform_of` closure calls `resolve_chain(chain, &no_scroll, resolve_transform)`;
   - `resolved.has_transform.then_some(resolved.forward)`.
3. `ScreenMapAffine::map_rect`: `[LogicalPosition; 4].map(..)` and `fold(f32::INFINITY, f32::min)`.
4. `positioning::nearest_scrollport`:
   - `core::iter::once((root, 0)).collect()` into a `std::collections::HashMap<LayoutNodeId, u64>`;
   - `map_or_else(LogicalPosition::zero, ..)`.
5. `scroll_into_view::find_scrollable_ancestors`:
   - `host.into_iter().chain(chain.links.iter().rev().map(|link| link.node))`;
   - `ScrollChain::of_node(..).unwrap_or_default()`, which needs `ScrollChain: Default` (it derives it).
6. `LayoutTree::mirroring_dom` (cfg(test)): `.and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id).map(|p| p.index())`.
7. `compositor.rs` `painted_over_mover`: `first.is_some_and(|f| i > f)` on `Option<usize>`.
8. Tests:
   - `escaping_sticky_fixture` uses `["root", ..].map(|c| node_by_class(&sd, c))`;
   - the item-3 test builds a `DisplayListItem::Border` literal with the `azul_css::props::style::*` side types, spelled as in `LayoutWindow::apply_text_tweens`;
   - the IME test uses `gpu_state_manager.get_cache(..)` and the public field `ComputedTransform3D::m`.

## 5. Commands for the parent

```
cargo test -p azul-layout --test all -- a_reveal_scrolls_only_the_boxes_that_move_its_target \
  an_arrow_reads_the_action_of_the_scroll_box_it_is_painted_in \
  a_spatial_navigation_container_is_a_scroll_box_its_node_is_painted_in \
  the_ime_caret_rect_is_where_the_raster_paints_the_caret \
  a_layout_blit_repaints_what_is_painted_over_its_mover \
  a_scrollbar_in_a_transformed_virtual_view_is_pressed_where_it_is_painted \
  a_layout_blit_repaints_the_scrollbar_it_dragged a_scrollbar_in_a_virtual_view_is_pressed_where_it_is_painted \
  a_drag_autoscrolls_the_box_its_containing_block_scrolls_in spatial_navigation scroll_chain \
  ime_geometry_follows_the_fields_scroll a_caret_counts_bytes_in_its_own_block seat_text_session \
  viewport_scroll_frame viewport_scrollbar scroll_shift_ghost vview_contenteditable_e2e \
  caret_reveal_and_session_identity a_selection_reveal_shows_its_focus_end
cargo test -p azul-layout --lib -- solver3::positioning solver3::scroll_chain managers::scroll_into_view \
  managers::focus_cursor managers::scroll_state default_actions cpurender::compositor headless
cargo test -p azul-layout --test managers_scroll_into_view
cargo test -p azul-dll --lib -- headless
```

Then run the full layout `all` and `lib` suites, the dll suite and the e2e corpus.

**RED pass.** Revert each fix alone and run its test:

| Revert | Test that must fail |
|---|---|
| 22e922976 | `a_reveal_...` |
| 0881fd44e | `an_arrow_reads_...` |
| d37bc38d6 | The 3 `sticky_*` unit tests. Also put the old unit-test calls back; the RED commit 1ca24e1b4 has them. |
| a1a28a9d0 | `a_spatial_navigation_container_...` |
| adcd74e7b | `the_ime_caret_rect_...` |
| b3bce7e64 | `a_layout_blit_repaints_what_is_painted_over_its_mover` |
| a21d17411 | `a_scrollbar_in_a_transformed_virtual_view_...` |

## 6. Behaviour changes beyond the REDs, and what is left

**Behaviour changes to check in the suites:**
- **Sticky boxes stick to `overflow: hidden` boxes** (CSS Position 3 says "nearest
  scroll container"). A stray offset on a plain div no longer moves a sticky box.
  Two unit pins changed with this:
  - `..._ignores_non_scrolling_overflow` lost its `hidden` case, which moved to the
    new `nearest_scrollport_of_a_hidden_box_is_its_content_box`;
  - `nearest_scroll_offset_picks_the_nearest_ancestor` now makes `.mid` a scroll
    container.
- **Boxes without a layout box.** A reveal target or focus with no box
  (`display: none` / `contents`) now finds no scroll ancestors in its own dom. It
  still crosses a VirtualView hop.
- **The IME caret rect** now includes the block's own transform, and scale or
  rotation make it the AABB of the transformed caret. Before, only the origin
  moved.
- **The focus ring under a layout blit.** A focused ancestor's ring is repainted
  at its bounds inside the blit clip. A border's bounds cover its inside too, so a
  focused container around a moving block loses most of that block's blit.
  Painting only the four sides would fix that, but the corner radii would need
  resolving.

**Left:**
- **Nested-DOM thumbs under a scaled host.** The track is placed exactly, but the
  thumb offset along it stays in the dom's own units. A host transform that
  animates between two layouts is picked up at the next registration. A bar in a
  transformed box of its own dom is still placed as if untransformed.
- **`ScrollChain` needs a box.** Nodes with `display: contents` fall out of every
  chain-based question.
- **`is_within` is still a DOM test.** It decides which candidates `candidates_in`
  takes, so an escaping absolute box is still a candidate "inside" the scroller it
  escapes. css-nav-1's focusable areas are DOM descendants, so I left it.
- **`is_visible` clips by scroll containers only.** An `overflow: clip` box does
  not clip (unchanged).
- **`TextTarget::point_from_window` is not transform-aware.** It is the inverse
  of `rect_to_window` and goes through `window_point_to_ifc_local`, which does not
  apply transforms. It should use the same chain inverted.
- **Found, not fixed: a node's own transform.** `node_rect_to_screen` and the hit
  tester's `compute_node_chains` do not apply a transformed node's own transform
  to its own box, but the raster paints the box inside the node's own reference
  frame. `content_rect_to_screen` does include it.
- **Found, not fixed: the animation transform channel.** Every transform resolver
  reads only `css_current_transform_values`. The raster also paints
  `anim_current_transform_values`.
- **Twin lookup left:** `e2e/runner.rs` and `dll/.../event.rs` still inline the CSS
  transform lookup closure, like `css_transform_of`.
