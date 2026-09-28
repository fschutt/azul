# Layout + animation bugs (PR #476 user test 2026-09-28) - PROGRESS checkpoint

Branch `wt/layout-animation-bugs` (from 5414bfa6b). NO compilation allowed; RED test commit before
every fix; explicit staging; commit this file after every commit. Delete it in the last commit
(the final report `scripts/LAYOUT_ANIMATION_BUGS_FIX_2026_09_28.md` replaces it).

## DONE
- 971bb137e RED bug 1: layout/tests/a_scroll_box_scrolls_to_its_end_padding.rs (flex page + block box,
  gap below last card must be 44 = margin 20 + padding 24; today ~-24 / -4). Registered in all.rs.
- 9006b27e5 FIX bug 1: `LayoutTree::scroll_extent` (layout/src/solver3/layout_tree.rs) = max(content,
  new `in_flow_scrollable_extent`: in-flow box children margin boxes + leading/trailing padding).
  IFC roots and abs/fixed children skipped. Padded scroll boxes (TextArea too) scroll further.

- 8778955df RED bug 2a: core/src/diff_test.rs `a_moved_tab_stop_is_not_a_layout_change` (today
  might_affect_layout == true).

- 444d77e43 FIX bug 2a: core/src/diff.rs attrs_hash = contenteditable + is_anonymous (no tab index).

- ecb899ed5 RED bug 2b: radio_group_geometry.rs narrow-row test (today circle 10x18) + demo-page
  click/relayout guard (RED status unknown).

- 89860e79c FIX bug 2b: radio_group.rs const NO_SHRINK (flex-shrink 0) on circle + both dots.
  Open: check_box / switch / other fixed-size indicators have no flex-shrink 0 either.

- 78b89ad04 RED bug 4: cfg(test) BOX_ANCHOR_CALLS thread-local in scroll_chain.rs box_anchor; a11y.rs
  test `the_a11y_tree_places_its_nodes_with_linear_scroll_chain_work` (60-deep chain, today 1953
  calls, bound 2 * 62).

- 2e19c4950 FIX bug 4 (part): a11y update_tree computes ScrollChains once per dom (None when the dom
  has no scroll id); ancestor_scroll_offset(dom, Option<&ScrollChains>, idx, scroll_manager).

- a81e22b4a chore(probe): spans shell_incremental_relayout, register_scroll_nodes,
  scroll_chains_compute, cpu_hit_tester_rebuild, hit_test_paint_order_sort, a11y_update_tree,
  css_transition_tick (-> app_phase_seconds{phase}).
- Decided NOT to change (measurement plan items instead): DL enter_scroll_chain (ScrollChain::of per
  stacking context / abs child: k*3*depth lookups, a ScrollChains pass costs n - no clear win);
  shell's duplicate register_scroll_nodes (dll common/layout.rs incremental_relayout, ~n lookups,
  no layout-crate RED possible); hit tester compute_node_clips ancestor walk (O(n*depth) overflow
  lookups per rebuild, pre-existing, not a regression).

- cd8a4c8f6 RED bug 3: layout/tests/accordion_animation.rs (engine: height auto->0 tween at 25% must
  be 75px, today 100; widget open/close must pass through in-between heights, today snap).

## IN PROGRESS
- FIX bug 3 engine: window.rs apply_node_css_change - `from` not a length for width/height and `to`
  a px length -> from = node's laid-out content-box (or border-box) size.
- FIX bug 3 widget (accordion.rs): body = display flow-root + overflow clip, closed = height 0 +
  padding-top/bottom 0, open = height auto + padding 12; declared animation (height, padding-top,
  padding-bottom, 220ms ease-in-out) only under PrefersReducedMotion(False). Handler: animated =
  body resolves an `animation` AND system style not reduced; OPEN = full write [height auto, pt 12,
  pb 12] then override-only [height H] (H = body's get_content_size height, measured while closed);
  CLOSE = full write [height 0, pt 0, pb 0]; not animated + host RefreshDom = write `initial` x3
  (no latch); not animated self-contained = targets. Rewrite the display-pinning unit tests.

TRAP: the sandbox refuses `git commit -F - <<EOF` whose body contains `<`, `>` or `!` ("too complex
to verify") - keep commit messages free of those characters; run git add and git commit separately.

## NEXT (in order)
1. FIX 2a: core/src/diff.rs `NodeDataFingerprint::compute` attrs_hash = contenteditable +
   `flags.is_anonymous()` only (NOT the tab-index bits). Keep the pre-cascade STRUCTURE tier
   (`fingerprint_dom`) hashing all of `flags` (the skip path must not drop a tab-index change).
2. RED 2b: layout/tests/radio_group_geometry.rs - (a) circle stays 18x18 in a row too narrow for
   its label (today circle shrinks, flex-shrink 1, min-content 10 px -> pill); (b) demo-shaped
   (nested flex columns, `align-self: start` group) relayout after moving the roving stops in
   place (set_tab_index) - guard, RED status unknown.
3. FIX 2b: radio_group.rs circle + both dot styles get `flex-shrink: 0`
   (`LayoutFlexShrink { inner: FloatValue::const_new(0) }`; import FloatValue + LayoutFlexShrink).
4. RED bug 4: scroll_chain.rs `#[cfg(test)] thread_local BOX_ANCHOR_CALLS` counter in `box_anchor`;
   a11y.rs test: deep 60-level div chain laid out, reset counter, `A11yManager::update_tree` must call
   box_anchor <= 2n (today ~n^2/2 = ~1950 because `ancestor_scroll_offset` runs `ScrollChain::of`
   per node, every layout = every switch-animation frame).
5. FIX bug 4: a11y.rs update_tree computes `ScrollChains::compute` once per dom and reads
   `box_chain(idx)`; `ancestor_scroll_offset` takes `&ScrollChains`.
6. Probe spans (chore): register_scroll_nodes, scroll_chains_compute, cpu_hit_tester_rebuild,
   a11y_update_tree -> `app_phase_seconds{phase}` under AZ_OBSERVE=1.
7. Maybe: drop the shell's duplicate `register_scroll_nodes` in dll common/layout.rs
   `incremental_relayout` (layout_and_generate_display_list already registers) - needs a RED or
   leave as measured suspect.
8. Bug 3 accordion animation (design below), RED + engine + widget.
9. Final report scripts/LAYOUT_ANIMATION_BUGS_FIX_2026_09_28.md (commits+REDs, API changes,
   least-sure-to-compile, measurement plan, open items); delete this file.

## Findings so far
- Bug 4 per-frame path: switch knob tween (margin-left) = relayout every frame
  (`advance_css_animations_now` -> ShouldIncrementalRelayout -> common incremental_relayout ->
  layout_and_generate_display_list [registers scroll nodes + update_a11y_tree] -> register_scroll_nodes
  AGAIN -> rebuild_cpu_hit_tester [ScrollChains::compute + paint-order sort]).
  Prime suspect: f0844a39c made a11y `ancestor_scroll_offset` call `ScrollChain::of` per node =
  O(n*depth*3) CSS lookups per frame. DL `enter_scroll_chain` also calls ScrollChain::of, but only
  for stacking contexts and abs/fixed children. translate_texts_in_dom only on user-DOM styling (not
  per frame). record_frame per frame (cheap).
- Bug 3 design (not started): switch-style imperative height tween. Closed body must stay laid out
  (height 0, padding-top/bottom 0, overflow clip) so the content height is measurable at click time;
  engine needs `from: auto` -> current laid-out px substitution for width/height transitions;
  declare `animation` only under PrefersReducedMotion(False). Existing accordion tests pin
  display:none/block writes and must be rewritten.

## Open questions
- Is the RadioGroup pill caused by the layout-dirty rows (2a) or only by a narrowed row (2b)? Device
  check: `AZ_RECON_DEBUG=1` prints `[recon] intrinsic_dirty += ...` for the rows on a radio click.
