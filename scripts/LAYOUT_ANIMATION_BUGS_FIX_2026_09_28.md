# Layout + animation bugs from the 2026-09-28 AzWidgets user test - fix report

Branch `wt/layout-animation-bugs`, from 5414bfa6b (PR #476, `fix/input-bugs-2026-09-19`).
**Nothing was compiled or run** (wave rule): every "today" value below is by reading, and the
parent's build + RED pass is the first check of all of it.

| # | Bug | Verdict |
|---|-----|---------|
| 1 | Page bottom margin/padding missing | Root-caused and fixed: the scroll range lacked `padding-top + padding-bottom` (and a flex item's last margin) |
| 2 | RadioGroup indicators turn into tall pills "sometimes" | Two fixes: roving tab-stop moves made the rows layout-dirty (this round's regression trigger), and the ring could shrink at all (flex-shrink 1). The exact device relayout path is unverified |
| 3 | Accordion expansion not animated | Fixed: a height tween like the switch's knob, reduced motion respected |
| 4 | Switch toggle lags badly (regression) | Prime suspect fixed: the a11y tree resolved a `ScrollChain` per node, per frame (quadratic). Probe spans added; measurement plan below |

## Commits (RED first)

| Commit | What | RED today -> expected |
|--------|------|-----------------------|
| 971bb137e | RED `layout/tests/a_scroll_box_scrolls_to_its_end_padding.rs` (in all.rs): 5 cards (200 px, `margin-bottom: 20px`) in a padded (24 px) scroll box, scrolled to its registered max; the gap below the last card must be 44 px | flex page: -24 (card cut off); block box: -4 -> 44 |
| 9006b27e5 | FIX `LayoutTree::scroll_extent` (layout/src/solver3/layout_tree.rs) = per axis max(content size, new `in_flow_scrollable_extent`: the in-flow box children's margin boxes offset by the leading padding, plus the trailing padding). IFC roots and abs/fixed children are left out | both tests green |
| 8778955df | RED core/src/diff_test.rs `a_moved_tab_stop_is_not_a_layout_change` | `might_affect_layout` true -> false |
| 444d77e43 | FIX core/src/diff.rs `NodeDataFingerprint::compute`: `attrs_hash` hashes contenteditable + the anonymous bit, not the tab index. The pre-cascade STRUCTURE tier keeps hashing all of `flags` (the skip path must not drop a tab-index change) | green |
| ecb899ed5 | RED layout/tests/radio_group_geometry.rs: `the_radio_circle_stays_round_in_a_row_too_narrow_for_its_label` (group in a 40 px div) and `the_radio_circles_keep_their_rects_while_clicks_move_the_check_in_the_demo_page` (demo nesting; 5 rebuilds with the check and roving stop on another row + in-place `set_tab_index` + in-place relayout; rects diffed against the first layout) | narrow: circle 10 x 18 -> 18 x 18. Demo-page test: RED status UNKNOWN (a guard for the device path) |
| 89860e79c | FIX layout/src/widgets/radio_group.rs: const `NO_SHRINK` (`flex-shrink: 0`) on the ring and both dot styles (same slot, so the dots still differ in opacity only) | narrow test green |
| 78b89ad04 | RED layout/src/managers/a11y.rs `the_a11y_tree_places_its_nodes_with_linear_scroll_chain_work` + `#[cfg(test)]` thread-local `BOX_ANCHOR_CALLS` in scroll_chain.rs `box_anchor` | 60-deep div chain (62 boxes): 1953 box_anchor calls -> at most 124 |
| 2e19c4950 | FIX a11y `update_tree`: `ScrollChains::compute` once per dom (skipped when the dom has no scroll id), `ancestor_scroll_offset(dom, Option<&ScrollChains>, idx, sm)` reads `box_chain` | green (0 calls without scroll ids) |
| a81e22b4a | chore(probe): spans `shell_incremental_relayout` (dll), `register_scroll_nodes`, `scroll_chains_compute`, `cpu_hit_tester_rebuild`, `hit_test_paint_order_sort`, `a11y_update_tree`, `css_transition_tick`. Instrumentation only, no RED | - |
| cd8a4c8f6 | RED layout/tests/accordion_animation.rs (in all.rs): engine `a_height_tween_from_auto_starts_at_the_boxs_laid_out_height`; widget `opening_..._grows_its_body_over_several_frames`, `closing_..._shrinks_its_body_over_several_frames` (real header click via `invoke_single_callback_at`, writes through `apply_content_change`, switch-test frame loop) | engine: 100 px a quarter in -> 75; widget: frames [104] / [0] (snap) -> heights in between, settling at 104 / 0 |
| 3648c0565 | FIX layout/src/window.rs `apply_node_css_change`: a `width`/`height` tween toward a length whose start value is not a length (`auto`...) starts at the node's laid-out size (content box, border box under `box-sizing: border-box`) - new `size_transition_start` | engine test green |
| d6ec5401c | FIX layout/src/widgets/accordion.rs: body = flow-root + `overflow: clip` + `min-height: 0`; closed = `height: 0`, no vertical padding (laid out, measurable) instead of `display: none`; `animation: height, padding-top, padding-bottom 220ms ease-in-out` declared only under `prefers-reduced-motion: no-preference`. Handler measures the content height + asks whether the body animates, then writes (see bug 3). Unit tests that pinned the display toggle now pin the height; new `a_body_declares_its_tween_only_without_reduced_motion` | widget tests green |

## Bug 1 - the page's bottom padding

`LayoutTree::scroll_extent` (the one extent registration, the painted thumb and the GPU thumb read)
returned the content size, which is a CONTENT-box extent in the BFC and in the taffy bridge (a flex
item's `overflow_content_size` is the pure child extent after `compute_taffy_scrollbar_info`
subtracts the whole inset), while `ScrollManager::max_scroll_offsets` subtracts the PADDING box. The
demo's page column is a flex item of the flex body, so it lost `24 + 24` px, plus the last card's 20
px margin (taffy's content size has no item margins). The fix measures the extent in the padding box
per CSS Overflow 3 §2.2 and never lowers a range.

Behaviour change: every padded scroll box scrolls `padding-top + padding-bottom` further (plus a
flex/grid item's last margin) and gets a shorter thumb - including the TextArea container (4 px
padding). Tests or pixel goldens with a padded scroll box may move.

## Bug 2 - RadioGroup pills

Mechanism by reading: the ring is a flex item with `width: 16px` + 1 px borders but flex-shrink 1;
its automatic minimum is its content (dot 8 + borders = 10). Any pass that gives a row less than
ring + label squeezes it to 10 x 18 - taller than wide, exactly the screenshot. Since this round
(roving tabindex, P2-12) each selection change rewrites the rows' tab indices, and the tab index
lived in `attrs_hash` = CONTENTEDITABLE = layout-dirty: the rows were rebuilt fresh and relaid out
as roots on the rebuild AND on the next in-place relayout (`SetNodeTabIndex` writes the live DOM) -
the "sometimes". Both are fixed; whether the group was measured short on device through exactly that
path is not proven (see measurement plan, `AZ_RECON_DEBUG`).

## Bug 3 - Accordion animation

How it animates: an imperative write of a property the node declares an `animation` for seeds a
`CssTransition` (the switch's mechanism). The handler:

- tween, opening: full write `height: auto` + 12 px padding (the open state), then override-only
  `height: <content height>` - the value the tween walks to (an `auto` target does not
  interpolate). At t = 1 the override is removed and `auto` shows;
- tween, closing: full write `height: 0` + no padding; the engine starts it at the laid-out height
  (3648c0565). A rebuilding host with an EMPTY body gets `initial` for the height (no transition
  would clear the override);
- no tween (reduced motion) and the host rebuilds: `initial` x3 - the old latch-free rule;
- no tween, nobody rebuilds: the new state at once.

A settled transition removes its override, so the demo (on_accordion returns RefreshDom) ends on its
rebuilt DOM's style with no latch. The host-rebuild path relies on the dll's regenerate_layout
migrating overrides and remapping `css_transitions` (not reachable from layout tests; the tests
cover the self-contained mode).

## Bug 4 - Switch lag: per-frame analysis

A switch toggle tweens the knob's `margin-left` (a layout property) for 150 ms: EVERY animation
frame is `advance_css_animations_now` -> `ShouldIncrementalRelayout` -> common
`incremental_relayout` -> `layout_and_generate_display_list` (solver3 relayout, display list,
`update_a11y_tree`, `register_scroll_nodes`) -> `register_scroll_nodes` AGAIN -> CPU backend
`rebuild_cpu_hit_tester`. The demo's `on_switch` also returns RefreshDom, so each toggle starts with
one full regenerate.

| Candidate | Per animation frame? | Status |
|-----------|---------------------|--------|
| a11y `ancestor_scroll_offset` -> `ScrollChain::of` per node (f0844a39c, this round) | YES, every node, 3 cascade lookups per ancestor level: O(n x depth) | FIXED 2e19c4950 - prime suspect |
| `ScrollChains::compute` in `register_scroll_nodes` (this round) | yes, x2 (window.rs:2606 and dll common/layout.rs incremental_relayout) - ~n lookups each | not changed; the shell's second call is redundant (layout_and_generate_display_list registers first) |
| CPU hit tester rebuild: `ScrollChains::compute` + paint-order sort (7d7b25c0e) | yes (CPU backend) - sort is O(n log n), `paint_ranks` O(items + n); cheap | measured by new spans |
| hit tester `compute_node_clips` ancestor walk (overflow lookups) | yes, O(n x depth) - pre-existing, not this round | open item |
| DL `enter_scroll_chain` -> `ScrollChain::of` (192976f2b/3f4056d50, this round) | yes, but only for child stacking contexts and abs/fixed children: k x 3 x depth lookups | not changed (a full `ScrollChains` pass costs n; only wins with many stacking contexts) - measure `dl_regenerate_full` |
| Split page frame painted in place by the CPU compositor (scroll chain round) | the compositor runs per frame; whether damage stays knob-sized is the question | measure `app_paint_damage_pixels` |
| Press router | pointer events only | not per frame |
| Fluent `translate_texts_in_dom` | only in `style_user_dom_for` (layout callback output), gated on a localizer | not per frame (once per toggle's RefreshDom) |
| `record_frame` on macOS (46927f296) | yes; clones two damage values | negligible |
| Video decode thread / pump timer | thread poll while a Thread lives; status only on change (paused = none) | measure `dispatch.threads` rate |

## Measurement plan (for the parent, on the Mac, when the user is not using it)

Build the dll as usual (build-dll ships telemetry + probe). Run AzWidgets:

1. `AZ_OBSERVE=1` (local Grafana/OTLP stack at 127.0.0.1:4318; the bridge arms probe recording).
   Toggle the switch 20 times, one second apart, then idle 10 s. Read `app_phase_seconds{phase}`
   (use `$__rate_interval`, allow ~6 s flush lag):
   - `a11y_update_tree` p50/p95 per relayout. Suspect confirmed if, on a build WITHOUT 2e19c4950
     (revert locally), it is a large share (>= 30 %) of `shell_incremental_relayout`, and with it
     drops to about the cost of one linear pass.
   - `shell_incremental_relayout` count per toggle: expect ~9-10 (150 ms at 60 Hz). Far fewer =
     pacing, far more = extra relayouts.
   - `register_scroll_nodes` count = 2 x relayouts confirms the duplicate; its p50 is the waste.
   - `scroll_chains_compute` count = 3 x relayouts on the CPU backend (2 registrations + 1 hit test).
   - `cpu_hit_tester_rebuild`, `hit_test_paint_order_sort` - the paint-order sort suspect is
     cleared if the sort is a small fraction of the rebuild.
   - `dl_regenerate_full` vs a pre-round build (a1985a456): a clear rise points at the scroll-chain
     paint changes (`enter_scroll_chain`).
   - `css_transition_tick` should be tiny.
   - `app_paint_damage_pixels` per frame during a toggle: a knob-sized value (a few thousand px at
     2x) is right; ~window-sized (millions) means the in-place split page frame disabled partial
     raster; then compare `raster_damage_body` and `present_view_blit`.
   - `app_frame_seconds` p50 during toggles: 16.7 ms = on budget.
2. `AZ_PACE_TRACE=1` (macOS stderr): the display-link / request-redraw / drawRect timeline. During a
   toggle, drawRect-to-drawRect intervals above 16.7 ms mean a frame's work exceeds the budget;
   `request-redraw-deferred` without a following tick means pacing is starving.
3. `AZ_PROFILE=cpu` (agent/local only) for the same spans without Grafana.
4. `AZ_LOG=debug,+window`: `incremental_relayout` log spans with durations (Window category).
5. Bug 2: `AZ_RECON_DEBUG=1`, click and arrow through the RadioGroup: before 444d77e43 the rows
   print `[recon] intrinsic_dirty += layout_idx ... flag Layout`; after, they must not.
   `AZ_FP_DUMP=1` prints the fingerprint diff of the first 10 layout-dirty nodes.
6. Idle waste: `dispatch.threads` / `dispatch.timer` span rates with the video paused - ~60/s means
   the thread-poll timer ticks for a paused decoder.

## API changes

None to the public API and nothing for api.json:
- `LayoutTree::in_flow_scrollable_extent`, `size_transition_start`, accordion
  `body_style`/`body_animation`/`body_content_height`/`body_animates`/`BODY_*` are private;
  `A11yManager::ancestor_scroll_offset` is `pub(crate)` (signature changed);
- the accordion's `ACCORDION_BODY_STYLE_OPEN/CLOSED` statics (private) are gone;
- `NodeDataFingerprint::attrs_hash` changes MEANING (no tab index), not type.

Behaviour changes: padded scroll boxes scroll further; tab-index changes no longer relayout; the
radio indicator never shrinks; size tweens from `auto` start at the laid-out size; the accordion
body is always laid out (collapsed when closed) and tweens.

## Least sure to compile

1. window.rs `apply_node_css_change`: the new `laid_out_size` block borrows `&layout_result.layout_tree`
   and `&layout_result.styled_dom` while `cache`/`node_data`/`states` borrow other fields of the same
   `&mut DomLayoutResult`; the `filter_map` closure captures `laid_out_size` (Copy).
2. window.rs `size_transition_start`: patterns on `CssPropertyValue::Exact(LayoutHeight::Px(_))`,
   `LayoutHeight::px(f32)`.
3. accordion.rs `body_animation`: `CssPropertyWithConditions::with_condition`,
   `DynamicSelector::PrefersReducedMotion(BoolCondition::False)`, `AnimationIterationCount::Count(1)`
   (u16); handler `let [top, bottom] = vertical_padding(..)`, `if let (true, Some(h)) = (..)`; the
   new unit test compares `apply_if.as_ref()` with `&[DynamicSelector::..][..]`.
4. radio_group_geometry.rs `let mut node_data = result.styled_dom.node_data.as_container_mut();`
   then `node_data[*row].set_tab_index(..)`.
5. scroll_chain.rs `thread_local!` with `const { .. }` init; the `#[cfg(test)] { .. }` statement in
   `box_anchor`.
6. a11y.rs test builds a full `LayoutWindow` inside a lib unit test (`FcFontCache::default()`).
7. accordion_animation.rs `invoke_single_callback_at` argument list and the destructuring of
   `CallbackChange::{ChangeNodeCssProperties, OverrideNodeCssProperties}`.
8. radio_group.rs `const NO_SHRINK` (a const of a type with drop glue, used in static slices - the
   same pattern as `system_palette::DARK_ACCENT_BACKGROUND`).

## Open items

- Bug 2 on device: confirm with `AZ_RECON_DEBUG=1`; the demo-page guard test's RED status is unknown.
  Other fixed-size indicators (check_box box, switch track, ...) have no `flex-shrink: 0` either.
- Bug 1: an INLINE formatting context scroll root (a text field's value `<p>`) still excludes its
  end padding (the caret-gutter rule sizes those); horizontal end padding for such roots likewise.
- Bug 3: collapsed sections' content is laid out (cost) and now present in the a11y tree (it was
  already in the Tab order with `display: none` - `collect_tab_order` ignores display). Consider
  `visibility`/aria-hidden for collapsed bodies. A self-contained accordion opened by click keeps an
  inline `height: auto` (good); one still mid-tween when its node is dropped by a rebuild loses the
  transition with the node. The Switch's own `animation` is unconditional - it ignores reduced
  motion; the same `PrefersReducedMotion(False)` condition would fix it.
- Bug 4 remaining waste (measure first): the shell's second `register_scroll_nodes` in dll
  common/layout.rs `incremental_relayout`; `enter_scroll_chain`'s per-context `ScrollChain::of`;
  the hit tester's `compute_node_clips` ancestor walk; a full accesskit tree rebuild after every
  layout even with no assistive technology connected.
