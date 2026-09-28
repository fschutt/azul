# Scrollbar presence: one "is there a bar" per axis (part B, 2026-09-28)

Branch `wt/scrollbar-presence`, based on `a1985a456` (PR #476,
`fix/input-bugs-2026-09-19`). Implements steps 1, 2 and 3 of §4 of
`scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` (M1, M1b, M1c).
**Nothing was compiled, type-checked or rustfmt'd.** The parent compiles once.

## Commits (RED first, then the fix)

| # | Commit | Kind |
|---|---|---|
| 1 | `0e898681f` test: a press on an overflowing TextInput goes through the shells' scrollbar gate | RED |
| 2 | `1d37b3736` test: no scrollbar where the style draws none, and none on a hidden axis | RED |
| 3 | `ee755da5d` fix: one style-aware "is there a bar" per axis - `ScrollbarPresence` | fix for 1+2 |
| 4 | `68e528f41` test: a scroll box that fits again, or stops scrolling, keeps a stale state (M1b) | RED |
| 5 | `5726f64db` fix: registration refreshes a box that stopped overflowing and drops one that stopped scrolling | fix for 4 |

### Expected RED per test commit

**Commit 1** (`layout/tests/a_press_on_an_overflowing_field_selects.rs`, new, registered in `all.rs`):
- `a_press_and_drag_on_an_overflowing_field_selects_the_dragged_text`: real
  `TextInput`, 200x120 window, value ~330px in a ~170px field. The harness
  press goes through `scrollbar_under_press` (= `hit_test_scrollbars`) first,
  as every shell does. Today it panics at `selected_bytes()`:
  *"THE BUG (overflowing): ... selected nothing - the selection is no editing
  session at all. The press went to Some(ScrollbarHit { Horizontal, Thumb,
  node 2 })"*. The ported thumb drag also scrolls the `<p>` (~2x the pointer
  travel). Expected: `Selection::Range` with anchor < focus, at least 3 bytes
  apart, anchor < 10; `SelectionRect` count > 0; the `<p>` offset unchanged.
  GREEN after commit 3.

**Commit 2**:
- `managers::scroll_state::autotest_generated::a_box_whose_style_draws_no_bar_has_no_bar_to_press`.
  Container (0,0,200,14), content 400x14, registered the way `scrollbar-width:
  none` registers today (thickness 0, visual width 0). Today
  `get_scrollbar_state(.., Horizontal)` is `Some`: the 16px fallback bar, with
  track y from -2 to 14, and `hit_test_scrollbars((100,7))` returns
  `Some(Horizontal)`. Expected: `None` for both.
- `...::a_hidden_axis_gets_no_bar_even_when_its_content_overflows` (M1c).
  Container 200x100, content 400x120, horizontal bar asked for, vertical not
  (`overflow-y: hidden`). Today a Vertical state exists (120 > 100) and
  (195,40) hits it. Expected: no Vertical state, no hit, and the Horizontal
  bar still there.
- `scrollbar_presence::a_hidden_axis_that_overflows_has_no_scrollbar` (new
  file, registered). The same case through the production path (layout +
  `register_scroll_nodes`): a 200x40 `overflow-x: auto; overflow-y: hidden`
  box over 400x120 of content. Today it has a Vertical state, and a press at
  its top-right corner hits it. Expected: `None`.
- All three are GREEN after commit 3.

**Commit 4**:
- `scrollbar_presence::a_box_that_fits_again_loses_its_scrollbar_and_its_offset`.
  A 40px strip over 600px of content. At 300px it overflows: the test takes
  the centre of its bar and scrolls it to 100. Resized to 800 (the resize fast
  path). Today `hit_test_scrollbars(old centre)` is still
  `Some(Horizontal ...)`, because the stale state keeps container 300 and
  content 600. Expected: `None`, then offset `Some(0,0)` (today
  `Some(100,0)`).
- `scrollbar_presence::a_box_that_is_no_longer_a_scroll_container_loses_its_scroll_state`.
  An `overflow: auto` box is scrolled to (50,30), then the same tree is laid
  out again with `overflow: visible`. Today `get_current_offset` returns
  `Some((50,30))`. Expected: `None`.
- `a_press_on_an_overflowing_field_selects::a_field_that_fits_again_is_back_at_its_start_and_still_selects`.
  Real `TextInput`: scroll the `<p>` to 40, then grow 200 -> 900. Today the
  offset stays `Some((40,0))`. Expected: `Some((0,0))`, and a press + drag
  still selects.
- All three are GREEN after commit 5.

The fix commits also add unit tests that pin the new rules: `scrollbar.rs`
(presence and kind resolution), `scroll_state.rs`
(`calculate_scrollbar_states_builds_the_bar_layout_described`,
`a_bar_layout_asked_for_exists_with_nothing_to_scroll`,
`a_bar_without_a_usable_thickness_is_no_bar`,
`re_registering_content_that_fits_moves_the_view_home_and_says_so`,
`remove_scroll_node_forgets_the_offset_and_the_bars_and_lets_go_of_the_thumb`),
and `gpu_state.rs` (`an_overlay_vertical_bar_runs_its_whole_track_without_buttons`,
`a_style_that_draws_no_bar_gets_no_thumb_to_move`).

## The design

- `needs_horizontal` / `needs_vertical` keep their name and now mean **"this
  axis scrolls"**: `scroll`, or `auto` whose content overflows. They are never
  true on `hidden`, `clip` or `visible`, and this was already true in
  `check_scrollbar_necessity`. Registration, scroll frames and reflow read
  them.
- **"Is a bar drawn there"** is new. Layout resolves the node's
  `ScrollbarKind` once, in `compute_scrollbar_info_core`, from the style the
  painter reads:
  - `None` for `scrollbar-width: none` or a zero or non-finite width;
  - `Overlay` for the viewport's scroller, or for a style that shows no arrow
    buttons;
  - `Classic` otherwise.
- `ScrollbarRequirements::presence(axis)` combines that kind with
  `needs_axis` and `visual_width_px` into `ScrollbarPresence`.
- `register_scroll_nodes` passes both presences into the scroll state.
  `calculate_scrollbar_states` (and through it `hit_test_scrollbars`),
  `paint_scrollbars`, `update_scrollbar_transforms` and
  `synchronize_scrollbar_opacity` all read the presence. This covers the
  thickness, the buttons, and the "other bar present" corner. There is no
  size filter and no 16px fallback any more.
- Classic buttons are **square, of the bar's thickness**. With the default
  UA style this equals `scroll_button_size_px`, so paint does not change.
  See the behaviour changes below.
- **Faded-out overlay bar: still hit-testable.** This keeps today's
  behaviour: `ScrollbarState::visible` stays `true` for every bar that
  exists, and the fade is paint-only. The decision is documented on
  `ScrollbarState::visible` and `hit_test_scrollbars`.
- The kind is 1 byte (`#[repr(u8)]`) and sits in the padding after the two
  bools of the `#[repr(C)]` `ScrollbarRequirements`. The struct stays 16
  bytes, so `LayoutNodeWarm` (pinned at 928 in `struct_sizes.rs`) should not
  grow. **Please verify this pin.**
- M1b: a node that does not need a bar but already has a state is refreshed
  through its principal box (first in `dom_to_layout`). That means new rects,
  presence `None`, and the offset clamped. A node that never scrolled still
  gets no state.
  - After each DOM, states of nodes that are no longer scroll containers are
    removed (`ScrollManager::remove_scroll_node`). "No longer a scroll
    container" means the computed overflow is not hidden, scroll or auto on
    either axis. The viewport root and `VirtualView` hosts are exempt. A node
    the DOM no longer has is removed.
  - When the clamp in `register_or_update_scroll_node` moves the offset, it
    sets `scroll_dirty`. `remove_scroll_node` also sets it when the removed
    offset was non-zero.

## Public type / field changes (for the api.json autofix)

None of these types appear in `api.json` (grepped: no `ScrollbarRequirements`,
`AnimatedScrollState`, `ScrollbarState`, `ScrollbarHit`). So there is
probably nothing to sync, but here is the full list:

- **new** `azul_layout::solver3::scrollbar::ScrollbarPresence` (enum `None | Overlay { thickness: f32 } | Classic { thickness: f32 }`; `is_present(self)`, `thickness(self)`, `button_size(self)`).
- **new** `azul_layout::solver3::scrollbar::ScrollbarKind` (`#[repr(u8)]` enum `None | Overlay | Classic`; `from_style(&ComputedScrollbarStyle, is_viewport: bool)`, `with_thickness(self, f32)`).
- `ScrollbarRequirements`: **new field** `bar_kind: ScrollbarKind`, placed after `needs_vertical`. **New method** `presence(&self, ScrollbarOrientation) -> ScrollbarPresence`.
- `AnimatedScrollState`:
  - **removed** `scrollbar_thickness: f32`, `visual_width_px: f32`, `has_horizontal_scrollbar: bool` and `has_vertical_scrollbar: bool`;
  - **added** `horizontal_bar: ScrollbarPresence` and `vertical_bar: ScrollbarPresence`;
  - **new** `bar(&self, ScrollbarOrientation) -> ScrollbarPresence`.
- `ScrollManager`: **new** `remove_scroll_node(&mut self, DomId, NodeId)`.
- **Signature change**, which the parent has to apply to the two router test helpers (`window_with_a_scroll_box` in `layout/src/press_router.rs`, `seed_a_scroll_box_thumb` in the dll `event.rs` tests):

  ```rust
  // OLD
  pub fn register_or_update_scroll_node(&mut self, dom_id: DomId, node_id: NodeId,
      container_rect: LogicalRect, content_size: LogicalSize, now: Instant,
      scrollbar_thickness: f32, visual_width_px: f32,
      has_horizontal_scrollbar: bool, has_vertical_scrollbar: bool)
  // NEW
  pub fn register_or_update_scroll_node(&mut self, dom_id: DomId, node_id: NodeId,
      container_rect: LogicalRect, content_size: LogicalSize, now: Instant,
      horizontal_bar: ScrollbarPresence, vertical_bar: ScrollbarPresence)
  ```

  Mechanical port for an old call `(.., t, v, has_h, has_v)`: pass
  `if has_h { ScrollbarPresence::Classic { thickness: v } } else { ScrollbarPresence::None }`
  and the same for `has_v`. Use `Overlay { thickness }` for a button-less
  bar, and `None` for both when `v == 0.0`. A test that expects a pressable
  bar must now pass a present bar: the manager no longer invents one from
  the content size.
- `calculate_scrollbar_state_from_geometry` (private) now returns `Option<ScrollbarState>`.
- `fc::DEFAULT_SCROLLBAR_WIDTH_PX` has **no users left**. I left it in place
  (a `pub const`, in `fc.rs`, which is shared); it can be deleted.

## Least sure to compile

1. `ScrollbarKind::from_style` is a `const fn` that calls `f32::is_finite`
   (const since 1.83) and compares floats (const since 1.82); the toolchain is
   1.91. The methods take `&ComputedScrollbarStyle`. `ScrollbarPresence`'s
   methods take `self` by value, to avoid `trivially_copy_pass_by_ref` (the
   enum is 8 bytes).
2. `scroll_registration.rs`: the M1b block reads
   `layout_result.layout_tree.dom_to_layout` and
   `layout_window.scroll_manager` inside the `&mut layout_results` loop, and
   the removal pass captures `layout_result.styled_dom` in a `filter` closure
   over `state_keys()`. The field borrows are disjoint, and existing code
   already does the same, but check them.
3. `scroll_state.rs` `register_or_update_scroll_node`: it writes
   `self.scroll_dirty = true` while `existing` (from `self.states.get_mut`)
   is still live. These are disjoint fields.
4. The `a_press_on_an_overflowing_field_selects.rs` harness:
   - `mc.get_primary().map(|c| &c.selection)`, matched with
     `Some(Selection::Range(r))`;
   - `unwrap_or_else(|what| panic!(..))` returning `(u32, u32)`;
   - `self.lw.layout_cache.resize_only_hint`, `LayoutNodeId::index()`, and
     `tree.materialized_inline_layout_for_node(idx).bounds()`.

   These are copied from `textinput_resize_selection.rs`.
5. `window.rs` reshape plan: `solver3::scrollbar::ScrollbarKind::from_style(&style, solver3::scrollbar::is_viewport_scroller(dom_id, host_dom))` inside the `and_then` closure. `dom_id` is captured from the method.
6. The unit call sites were ported by a script: 27 in `scroll_state.rs`, plus
   2 in `gpu_state.rs`, 1 in `scroll_into_view.rs` and 1 in `scroll_timer.rs`
   by hand. `ScrollbarPresence` reaches the `natural_scroll_tests` and
   `autotest_generated` modules through `use super::*`.
7. Formatting was done by hand and is only approximately rustfmt-shaped
   (chain_width 60). Run `cargo fmt`.

## Behaviour changes

1. A `scrollbar-width: none` box has **no scrollbar state at all**. A press
   on it goes to the content, and the GPU updater no longer moves an
   invisible thumb. The debug-server `GetScrollbarInfo` now answers
   `found: false` for such a box (e2e JSON that pinned a bar there would
   change).
2. An axis with `overflow-y: hidden`, `clip` or `visible` never has a bar,
   whatever its content size.
3. An `overflow: scroll` axis with nothing to scroll now has a bar state. Its
   thumb fills the track, it is pressable, and a drag moves nothing. Paint
   always drew it; before, the press went to the content under it.
4. The bar thickness and buttons come from layout's presence everywhere:
   - A classic bar's buttons are square, the bar's own thickness. Before,
     paint used `scroll_button_size_px`, which differs only when author CSS
     overrides the width of a classic UA bar (for example `scrollbar-width:
     thin`: paint drew 12px buttons on an 8px bar while the hit test
     measured 8px buttons).
   - A style that shows no buttons but reserves a gutter is now measured
     without buttons, as it was painted. The hit test used to give it
     buttons.
5. An `auto` VirtualView whose bar is amended after layout keeps its classic
   buttons in the GPU thumb and in the hit test, as paint always drew them.
   Before, the GPU path and the hit test treated it as a button-less overlay
   (the `gpu_state` test expectation went from 34.0 to 18.0).
6. A state not created by registration has no bars until registration
   describes the node. This covers `set_scroll_position`, `update_node_bounds`
   and `update_virtual_scroll_bounds` on an unregistered node. Before, they
   got a 16px classic bar as soon as their content exceeded their container.
7. M1b:
   - A box that stops overflowing is refreshed: its bars go, its offset is
     clamped (usually to 0), and `scroll_dirty` is set when that moves it.
   - A box with a state whose computed overflow becomes visible or clip
     loses its state. That includes a programmatic offset on a `visible` or
     `clip` node, which CSS says cannot scroll.
   - An `overflow: hidden` box with a programmatic offset is now refreshed
     with its laid-out rects, and its offset is clamped to its real range.
     Before, registration never touched it. This matters for the TextInput
     host, which is `hidden` on macOS and Linux, if anything scrolls it.
   - Fewer scroll states means e2e manager fingerprints (`fp_scroll`, key
     counts) may shrink.
8. `synchronize_scrollbar_opacity` only keeps opacity keys and fades for
   bars that are painted. Before, it also did this for any axis that
   scrolled, including `scrollbar-width: none`.

## Touch points with the other two agents

- **Press router** (already merged on the PR branch):
  - Two test helpers call `register_or_update_scroll_node`; the signature
    above applies to them.
  - My harness gate is the single fn `scrollbar_under_press` in
    `a_press_on_an_overflowing_field_selects.rs`. Its thumb-drag port is
    `Harness::drag`. Both are meant to become `lw.route_press(p,
    MouseButton::Left, now).is_scrollbar()`, plus
    `route_move`/`route_release` while a thumb is held.
  - `scrollbar.rs` doc comments now name `LayoutWindow::route_move` instead
    of `handle_scrollbar_drag`.
  - I added no callers of the removed dll functions.
- **Scroll chain**:
  - I did not touch `ancestor_scroll_offset`, `set_scroll_ancestors`, the
    ancestor walk in `register_scroll_nodes`, or the one
    `ancestor_scroll_offset` line in `calculate_scrollbar_states`.
  - `remove_scroll_node` does **not** clean `scroll_ancestors`. That map is
    theirs, and a stale entry for a removed node is harmless, because only
    registered nodes' bars read it.
  - The M1b refresh now also runs the ancestor publication for boxes that
    stopped overflowing.
  - `scroll_registration.rs` imports gained `ScrollbarOrientation`, and a
    free fn `is_scroll_container` was added at the bottom of the file.
    Expect trivial merge conflicts there.
- **Outside my listed ownership, as minimal call-site changes**:
  - `window.rs`: `synchronize_scrollbar_opacity` reads presence, and the
    TextInput reshape plan sets `bar_kind` and uses the new registration
    signature.
  - `scroll_into_view.rs` and `scroll_timer.rs` test helpers: signature port.
  - `getters.rs`, `cache.rs` and `fc.rs`: test and constructor literals gain
    `bar_kind`.

## Open

- **Paint's scroll-frame decision** (display_list.rs:5735, the scroll-chain
  agent's) is unchanged. A `hidden` box still gets only a clip.
- **`is_node_scrollable` / `can_consume_delta`** still decide wheel
  scrollability from the content and container sizes alone, not from each
  axis's overflow. I did not verify end to end whether the wheel then moves
  an `overflow-y: hidden` axis. The presence split makes this easy to tighten
  (for example, only axes whose `needs_*` is true), but that is outside this
  part.
- **Faded overlay hit-testability** is kept as today. If the platforms should
  pass a press on a hidden overlay bar to the content, `ScrollbarState::visible`
  is the switch. `synchronize_scrollbar_opacity` knows the opacity, and the
  scroll manager does not.
- **`scroll_button_size_px` in `ComputedScrollbarStyle`** is now read only by
  its own getters. Buttons are square in the bar's thickness everywhere. The
  getter quirk remains: `-azul-scrollbar-visibility: always` over an overlay
  UA leaves `show_scroll_buttons == false`, so such a bar is `Overlay`, a
  gutter bar without buttons.
- The removal pass trusts `layout_results` to hold the complete DOM for each
  laid-out `DomId`. A caller that registers with a partial set would prune
  nothing for the DOMs that are missing, which is safe, but a DOM that is
  present must be complete.
