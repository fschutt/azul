# One ScrollChain per laid-out box (2026-09-28)

Part C of `scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` (§2.2, §3 C,
§4 steps 7-8), plus the coordinator's addition: the chain follows CONTAINING
BLOCKS, not layout parents (`position: fixed` / `absolute`).

Branch `wt/scroll-chain`, based on `a1985a456` (PR #476,
`fix/input-bugs-2026-09-19`). **Nothing was compiled, type-checked or run**
(no cargo, rustc, rust-analyzer or rustfmt). Every expected RED below is
derived by reading the code.

## Commits (oldest first)

| Commit | Kind | What |
|---|---|---|
| `d9f67539b` | refactor | `paint_in_flow_descendants`' three identical child loops -> `paint_in_flow_child` |
| `e91bf1ecc` | refactor | `solver3/scroll_chain.rs`: `ScrollChain`, `ScrollChains`; hit tester, `node_rect_to_screen`, `set_scroll_ancestors` read it |
| `a800e3f7d` | RED | `overflow: hidden` painted where the pointer finds it (2 tests) |
| `486061f0c` | fix | hidden box that can scroll gets id + frame; `accumulated_scroll` reads the chain |
| `d873242a9` | RED | a translucent block in a scrolled non-context box |
| `a6c266489` | fix | display list opens the frames a stacking context's chain names (`open_clips`, `enter/leave_scroll_chain`); split frames in the CPU compositor and WebRender |
| `ed209721a` | RED | `position: fixed` on a scrolled page; fixed box under a hidden wrapper |
| `af1ca9d4c` | fix | `box_anchor` containing-block rule for `fixed`; hit-test clips follow the chain |
| `40611918d` | RED | `position: absolute` inside a non-positioned scroll box |
| `918111d91` | fix | `box_anchor` rule for `absolute`; in-flow detour in `paint_in_flow_child` |
| `e671d2237` | RED | focus ring and a11y bounds of a fixed box on a scrolled page |
| `c074d2c2b` | fix | `enclosing_scroll_id` and `A11yManager::ancestor_scroll_offset` read the chain |

All new integration tests are in `layout/tests/scroll_chain.rs` (registered in
`layout/tests/all.rs`). There are 2 new unit tests in `layout/src/solver3/scroll_chain.rs`.

### Expected REDs (value today vs expected)

- `an_overflowing_hidden_box_scrolled_by_a_program_is_painted_where_the_pointer_finds_it`:
  box 200x100 `hidden` over red 50 + blue 250, unclamped offset 50. The hit test at
  (100,25) finds blue (passes). `is_blue(pixel(100,25))` fails: it is red (255,0,0).
- `a_hidden_box_whose_content_fits_is_not_moved_by_an_offset`: `node_under((100,75))`
  is `Some(NodeId(1))` (the box), expected `Some(NodeId(2))`. Then
  `get_node_rect_in_viewport(red).origin.y` is -50, expected 0.
- `a_translucent_block_in_a_scrolled_box_is_painted_scrolled_and_clipped_with_it`:
  `is_translucent_red(pixel(100,25))` fails: it is white (255,255,255). Then
  `is_white(pixel(250,75))` fails: it is about (255,128,128).
- `a_fixed_box_stays_where_it_is_when_the_page_scrolls`: `is_green(pixel(50,25))`
  fails: it is blue (0,0,255). Then `node_under` is `Some(NodeId(2))` instead of
  `Some(NodeId(1))`, and the viewport rect y is -150 instead of 0.
- `a_fixed_box_is_not_clipped_by_a_box_that_is_not_its_containing_block`:
  pixel (150,20) is white, expected green. Before `a6c266489` paint left the box
  unclipped but the hit tester clipped it. That commit reopens the wrapper's clip,
  so the RED fails on the pixel. Then `node_under` is `Some(NodeId(0))` instead of
  `Some(NodeId(2))`.
- `an_absolute_box_is_not_moved_or_clipped_by_a_scroll_box_that_is_not_its_containing_block`:
  pixel (75,45) is blue, expected green. Then `node_under((75,45))` is
  `Some(NodeId(2))` instead of `Some(NodeId(3))`. Pixel (275,45) is white, expected
  green. `node_under((275,45))` is `Some(NodeId(0))` instead of `Some(NodeId(4))`.
- `a_fixed_boxs_focus_ring_is_painted_around_it_on_a_scrolled_page`:
  `ring_y` is -152, expected -2.
- `a_fixed_box_is_reported_to_assistive_technology_where_it_is_painted`
  (feature `a11y`): bounds y0 is -150, expected 0.

### Guards for the refactors

- `d9f67539b`: the display-list and pixel suites that paint floats and dragged
  nodes (`static_opacity_paints.rs`, `float_integration.rs`,
  `viewport_scroll_frame.rs`, the dll pixel tests, the `display_list.rs` unit tests).
- `e91bf1ecc`: `viewport_scroll_frame.rs`, `viewport_scrollbar.rs`,
  `flex_intrinsic_text.rs::overflow_hidden_is_a_programmatic_scroll_container_but_not_a_wheel_target`,
  `a11y_consumer_contract.rs`, the `headless.rs` unit tests, and the two new
  `scroll_chain.rs` unit tests (one pins that the bulk table and the per-node walk
  agree for every node).

## The design

`layout/src/solver3/scroll_chain.rs`:

- `ScrollChainLink { layout_index, node, moves_content }` is one box whose clip or
  scroll frame a node is painted in. `moves_content` is set for a box with a scroll id
  that is not a `VirtualView`. That is exactly the set `push_node_clips` opens a
  `PushScrollFrame` for (`opens_scroll_frame`). A clip-only box (`overflow: clip`,
  or a hidden box whose content fits) is a link that moves nothing.
- `ScrollChain::of(tree, styled_dom, scroll_ids, index, Inclusivity)` walks one node,
  costing the depth of the tree. `AncestorsOnly` gives the frames the node's box is
  in; `SelfAndAncestors` also includes the node itself.
- `ScrollChains::compute(..)` builds every node's box chain, linear in the tree, as a
  trie: `box_chain_id`, `chain(id)`, `box_chain(index)`.
- `box_anchor` is the one rule both use. An in-flow box sits in its parent's content.
  An `absolute` box sits in the content of the nearest positioned or transformed
  ancestor, else the root (the initial containing block, which scrolls with the
  page). A `fixed` box sits in the content of the nearest transformed ancestor; with
  none, it leaves every frame, the page's included. The walk stops at the first box
  that `paints_as_a_group` (the root, a stacking context, a clip-path, an SVG clip
  mask). The box leaves that box's own frames and keeps the ones around it, because
  the push/pop display list cannot take a box out of a group painted around it.

Consumers (the six rules, before -> after):

| Rule | Before | After |
|---|---|---|
| R1 paint | `push_node_clips`: frame for `scroll \| auto` | frame for every box with an id (`opens_own_scroll_frame`); `open_clips` + `enter_scroll_chain` / `leave_scroll_chain` close and reopen frames so each child context and each `absolute` / `fixed` child is painted in exactly its chain |
| R2 hit tester | layout ancestors in `scroll_ids` | transforms along layout ancestors + scroll links from `ScrollChains`; `compute_node_clips` skips ancestors not in the chain (`clips_it`) |
| R3 `accumulated_scroll` | layout ancestors with ANY state | `ScrollChain::of(..).scrolling()` |
| R4 `set_scroll_ancestors` | layout ancestors in `scroll_ids` | `ScrollChains::box_chain(..).scrolling()` |
| R5 `find_scroll_parent` | DOM ancestors with a state | **untouched** (handed to the autoscroll/M3 agent, see Open) |
| R6 `node_rect_to_screen` | layout ancestors in `scroll_ids` | `ScrollChain::of(..).scrolling()` + transforms |
| R7 (found) focus ring `enclosing_scroll_id` | DOM ancestors with a state | innermost moving link of the chain |
| R8 (found) `A11yManager::ancestor_scroll_offset` | DOM ancestors with a state | the chain's moving links |

### Paint mechanics (`display_list.rs`)

- `DisplayListGenerator.open_clips: Vec<OpenClip>` holds `Owner(layout index)` for
  every chain link whose clips `push_node_clips` pushed, and `Barrier` for everything
  the walk cannot close around a descendant. That is a stacking context's own push
  (pushed before its reference frame), an in-flow child's reference frame or image
  mask, or a clip-path on a box whose overflow clips nothing.
- `enter_scroll_chain(node)` makes the owners opened since the last barrier equal to
  the node's chain (minus owners below the barrier), keeping the common prefix. It
  pops the rest with `pop_node_clips`, then reopens with `push_node_clips` what is
  missing. `leave_scroll_chain` undoes this. It is called in `paint_child_context`
  (every child stacking context) and in `paint_in_flow_child` for `absolute` / `fixed`
  children.
- `pop_node_clips` now takes the exact layout index its push was made for. It used to
  find the dom node's first layout node, which differs for split boxes.
- `node_establishes_stacking_context(styled_dom, tree, idx)` is the old method body
  moved into a free function, so the chain rule can read it.

### Split frames (one scroll id pushed more than once)

- **CPU compositor** (`allocate_layers_from_display_list`): a split frame is painted
  in place, not promoted to a layer. If each half were a layer, it would be
  composited over everything its parent layer paints, including the box painted
  between the halves.
- **`collect_scroll_shifts`**: one shift per id. Two shifts would memmove the clip
  twice. `scroll_fast_path_eligible` / `overlay_rects_after_frame` look at the first
  push. Items after it (the escaped box, the second half) are repainted as overlays,
  which is correct but repaints more.
- **WebRender** (`dll/src/desktop/compositor2.rs`): `SpatialTreeItemKey::new(scroll_id,
  n-th push)`. WebRender asserts on duplicate keys (`spatial_tree.rs`
  `add_spatial_node`: `"duplicate key"`), so this change is required. Without it the
  GPU path would panic on the first fixed header on a scrolled page.
  `set_scroll_offsets` visits every node with the external id (`spatial_tree.rs:1167`).

## The `hidden` decision

Chosen: **a hidden box gets a real scroll frame**, but only when it can scroll.
`compute_scroll_ids` gives a box that only a program can scroll (no axis
`allows_user_scrolling`, not a `VirtualView`) an id only if
`content_overflows_scrollport` (content > padding box + 1px). Paint frames every box
with an id, and all consumers read the same chain. So paint and hit testing agree
for every offset, clamped or not:

- An overflowing hidden box scrolls in paint (the CSS Overflow 3 §3.1 programmatic
  scroll container).
- A hidden box whose content fits has a scroll range of zero. It gets no frame, and an
  unclamped offset on it (`scroll_to_unclamped`) moves nothing: not paint, not the hit
  tester, not `accumulated_scroll`.

Why not "every hidden box": the CPU compositor promotes every non-empty
`PushScrollFrame` (except the page's) to its own layer with a pixbuf
(`compositor.rs` `allocate_layers_from_display_list`). `overflow: hidden` is on
dozens of widget boxes (avatar, ribbon, titlebar, list view, TextInput host ...), and
each would become a layer on every full and screenshot render. The frame cannot carry
a "programmatic-only, paint in place" flag: `DisplayListItem::PushScrollFrame`'s shape
is frozen, because printpdf matches variants exhaustively (`display_list.rs:400`). A
new `DisplayList` field would touch about 40 exhaustive struct literals.

Effect on the **TextInput host** (macOS/Linux `hidden/hidden`): it gets a frame only if
its content overflows it. Normally the value `<p>` scrolls instead, so nothing changes.
The `<p>` (`auto/hidden`) is unchanged.

Effect on the **blit fast path**: an overflowing hidden box is a scroll layer like an
`auto` one. It only moves on a programmatic scroll.

## Public type / field changes (for the api.json autofix)

None of the touched types are in `api.json` (checked: `DisplayListItem`,
`DomLayoutResult`, `ScrollManager`, `CpuHitTester`, `LocalScrollId`). New public
items in `azul_layout` (Rust-only):

- `solver3::scroll_chain::{ScrollChain, ScrollChainLink, ScrollChains, chain_link,
  opens_scroll_frame, content_overflows_scrollport}`
- `pub(crate)`: `solver3::scroll_chain::box_anchor`,
  `solver3::display_list::node_establishes_stacking_context`

Changed private or `pub(crate)` signatures: `DisplayListGenerator::pop_node_clips`
(+ `node_index`), `headless::compute_node_chains` (+ `&ScrollChains`),
`headless::compute_node_clips` (+ `clips_it`),
`A11yManager::ancestor_scroll_offset(dom, layout_result, layout_idx, scroll_manager)`,
`LayoutWindow::enclosing_scroll_id(layout_result, node)`.

## Least sure to compile

1. `ScrollChains::compute`: `*interned.entry(..).or_insert_with(|| { entries.push(..); id })`
   inside a `match` assigned to `content_of[a]`.
2. `enter_scroll_chain`: the capture-less `owner` closure passed to `filter_map` twice
   (relies on it being `Copy`); `.take_while(|(a, b)| a == b)` over `zip` of `&usize`.
3. `paint_in_flow_child`: `matches!(..).then(|| self.enter_scroll_chain(builder,
   child_index))` while `child_node` (borrowed from the tree, lifetime `'a`) is live.
4. `headless.rs`: `&|anc| scroll_chain.contains(LayoutNodeId::new(anc))` as
   `&dyn Fn(usize) -> bool`; `chains.get(t as usize).cloned()` on `&mut Vec<Vec<_>>`.
5. `compositor.rs` `collect_scroll_shifts`: `out.iter().any(|(id, ..)| id ==
   scroll_id)` (`&u64 == &u64`) and `.filter(|_| !already_shifted)` on
   `Option<(f32, f32)>`.
6. `scroll_chain.rs` `paints_as_a_group`: `.and_then(|nd| nd.get_svg_data())` then
   `matches!` on `&SvgNodeData` with an or-pattern.
7. `layout/tests/scroll_chain.rs`: `accesskit::NodeId(FIXED_BOX.index() as u64 + 1)`
   (a11y-gated) and `DisplayListItem::Border { bounds, .. }` with `bounds.0`.
8. The window.rs unit fixture `window_with_two_scrolled_boxes`: `Dom::with_css` inside
   the `autotest_generated` module (relies on `use super::*` bringing `Dom` in, as
   `fixture_dom` does).

## Behaviour changes (beyond the REDs) and performance

- Absolutely positioned and fixed boxes are no longer clipped or scrolled by the
  non-positioned boxes between them and their containing block (CSS 2.2 §11.1.1). The
  widgets that position popups absolutely (combobox, tooltip, popover, list view,
  ribbon, statusbar, node graph, map) anchor them in `position: relative` wrappers, so
  they are unaffected.
- Stacking contexts (opacity, transform, z-indexed positioned boxes) inside a
  non-context `overflow: auto/hidden` box are now scrolled and clipped by it in paint.
  Before, they were painted in their parent context's frames.
- A stray scroll offset on a box that opens no frame no longer moves clicks,
  `get_node_rect_in_viewport`, a11y bounds or the focus ring.
- **Performance**: a page with a fixed box, or with an absolute box that escapes a
  scroll box, splits that frame. The CPU compositor then paints it in place (no layer,
  no layer blit) and the flat fast path repaints the second half as overlays. An
  overflowing `overflow: hidden` box becomes a scroll layer. `enter_scroll_chain`
  costs O(depth) per child context and per absolute/fixed in-flow child. The hit
  tester and scroll registration compute `ScrollChains` once per DOM (O(n)); the a11y
  build calls `ScrollChain::of` per node (O(n·depth), the same as the DOM walk it
  replaces).
- dll pixel tests: every page with a fixed element or with an overflowing hidden clip
  now has more `PushScrollFrame`s in its display list. Run the dll headless and e2e
  suites.

## Merge notes (the scrollbar-presence (B) and press-router (A) branches)

Functions I changed, per file:

- `layout/src/managers/scroll_registration.rs`: **only `register_scroll_nodes`**.
  - Per DOM, before the node loop: `let scroll_chains = ScrollChains::compute(&tree,
    &styled_dom, &scroll_ids);`
  - At the `set_scroll_ancestors` call: `ancestors =
    scroll_chains.box_chain(LayoutNodeId::new(node_idx)).scrolling().map(|l| l.node).collect()`,
    replacing the hand-written walk over `scroll_ids`.
  - Keep both lines wherever B's restructured loop calls `set_scroll_ancestors`.
- `layout/src/managers/scroll_state.rs`: **untouched**. `calculate_scrollbar_states` /
  `ancestor_scroll_offset` / `set_scroll_ancestors` still work as before; the chain
  only changes what registration stores.
- `layout/src/managers/gpu_state.rs`: **untouched**.
- `layout/src/solver3/scrollbar.rs`: **untouched**.
- `B`'s `remove_scroll_node`: yes, it should also do
  `self.scroll_ancestors.remove(&(dom_id, node_id))`. Nothing reads a removed node's
  entry (`calculate_scrollbar_states` iterates `states`), but it leaks and would be
  wrong if the node id is reused by a later DOM.
- A's press router: I touched none of the shells, `event.rs`, `press_router.rs` or the
  e2e runners. `hit_test_scrollbars` tracks are shifted by `set_scroll_ancestors`,
  which now follows the chain: a bar inside a fixed box is found without the page's
  offset.
- Outside the briefed files: `dll/src/desktop/compositor2.rs` (the `PushScrollFrame`
  arm, spatial key per push) and `layout/src/cpurender/compositor.rs`
  (`allocate_layers_from_display_list`, `collect_scroll_shifts`).
- `layout/src/window.rs`: `compute_scroll_ids` (hidden rule + doc),
  `accumulated_scroll`, `enclosing_scroll_id` and its caller in `apply_text_tweens`,
  the unit fixture `window_with_two_scrolled_boxes`.
- `layout/src/headless.rs`: `node_rect_to_screen`, `compute_node_chains`,
  `rebuild_from_layout_with_gpu`, `compute_node_clips`, the `compute_node_clip` test shim.
- `layout/src/managers/a11y.rs`: `ancestor_scroll_offset` and its caller in
  `update_tree`; removed the now-unused `NodeHierarchyItem` import.

## VirtualView child DOMs

A chain lives inside one DOM's layout tree. Across a `VirtualView` boundary, the host
side comes from the host's display list: `resolve_virtual_view_placements` tracks
`PushScrollFrame` / `PopScrollFrame` nesting, so it follows paint, split frames
included. The child side comes from the child's own chain. Behaviour is unchanged:

- A child DOM's root never gets a viewport frame (`is_viewport_scroller` is `ROOT_ID`
  only).
- A `fixed` box in a child DOM leaves the child's frames but stays inside the
  `VirtualView` placement.
- Scroll bars inside child DOMs are still hit-tested in child-local coordinates (open
  since the viewport work).

## Open / not done

1. **R5 is untouched on purpose.** The coordinator handed it to the autoscroll (M3)
   agent. That covers `ScrollManager::find_scroll_parent` (DOM, any state),
   `CallbackInfo::find_scroll_parent` / `find_scroll_target`, the autoscroll loop
   (`event.rs:431-505`) and the momentum hand-off (`scroll_timer.rs:1061`). They
   disagree with the chain for fixed or absolute boxes and for stray states.
   Suggestion: the target is `ScrollChain::of(.., SelfAndAncestors).scrolling().last()`
   of the text block's IFC root; the edge box is the target's box chain.
2. Other walkers not switched: `LayoutWindow::find_scrollable_ancestor` (caret reveal
   target), `scroll_into_view::find_scrollable_ancestors` (reveal chain, DOM +
   VirtualView hops), `default_actions::spatial_navigation_action`, and
   `positioning::find_nearest_scrollport` / `find_nearest_scroll_offset` (sticky).
3. Push/pop limits, where CSS and the chain differ (paint and hit test agree):
   - An out-of-flow box cannot escape past a stacking context, clip-path or image
     mask between it and its containing block.
   - A drag or animation reference frame on a non-context ancestor is a barrier in
     paint but unknown to the chain, so the two can disagree during a drag.
4. Layout still positions `fixed` against the viewport inside a transformed ancestor
   (`positioning.rs`). The chain and the reference frame treat that ancestor as the
   containing block.
5. Absolute `z-index: auto` boxes still paint in tree order among in-flow siblings,
   not in CSS step 8.
6. The focus ring goes at the end of the first push of a split frame. Content in the
   reopened half can paint over it.
7. The hit tester still clips at the border box, while paint clips at the padding box
   minus the scrollbar gutter (pre-existing).
8. A hidden box whose content grows without a relayout (text reshape fast path) keeps
   its frame decision until `compute_scroll_ids` runs again.
9. Clamped `CallbackInfo::scroll_to` on a pure `overflow: hidden` box still clamps
   to 0, because such boxes are not registered (B's registration area).
