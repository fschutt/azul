# ANIMFRAME8 progress (wave-8 follow-up, branch wt/animframe8, base 5745afee6)

Task (PLAN.md "Wave-8 follow-ups" / ANIMFRAME8): an animation frame must cost ~1-2 ms. A transform /
opacity animation: no layout, no display-list rebuild (GPU property channel). A paint-only transition:
patch the display list. A layout-property transition: relayout only the dirty subtree. Answer "do we have
duplicated paths?" with a path map. Then, RED test per item: (1) a tick that changes no DOM node does not
reconcile; (2) style-only changes take the DL patch path; (3) css_transition_tick lightweight; (4)
VirtualView callbacks not re-run on a relayout whose host node is unchanged. Never compile; never touch
page_breaks.rs. Not the a11y code (A11YPATCH8).

## STATUS: ALL ITEMS DONE, REPORT WRITTEN (scripts/ANIMFRAME8_2026_10_03.md). In the final self-review pass.

## DONE (oldest first)
- 047a83241 progress file
- 914cfc932 report skeleton: measured knob tick (19-22 ms unprofiled; no-op relayout 10 ms) + path map
  (P1..P7) + 8 duplications/divergences
- 29c762631 RED layout/tests/a_transform_tween_moves_no_box_and_rebuilds_no_list.rs (registered in all.rs)
- 97025b4e2 core gpu.rs `GpuValueCache::refresh_transform_value_of`
- bde873961 window.rs tick_animations GPU property path (transform tween -> matrix only) +
  `LayoutWindow::take_animation_frame_work` (the one tick decision)
- bd7b4f74a window.rs `sync_css_gpu_values` shared by the layout pass and `regenerate_display_list_for_dom`;
  the DL-only build consumes paint-scope css dirt for its DOM
- e58aded70 switch.rs knob slides by `transform: translateX` (consts KNOB_OFF/ON_TRANSFORM, click handler,
  unit tests read the transform)
- dbfb358c5 dll advance_css_animations_now + TickAnimations arm + e2e runner use take_animation_frame_work;
  dll raises content_repaint_pending for a repaint frame (headless paints instead of relayout_only);
  dll driver tests write the knob transform
- c9d59e666 switch_animation.rs pins the margin-left tween with an explicit margin knob style
- d332a93fb RED layout/tests/a_face_fade_frame_is_patched_in_place.rs (item 3)
- 5c5668705 GREEN item 3: batched restyle (one recompute per frame), PaintColorSlot border patch +
  patch_compact_border_color, text-colour fade refreshes inheritance on its last frame
- a3772c8ba RED layout/tests/a_layout_tween_frame_reuses_the_tree_and_patches_the_list.rs (items 1+2)
- f581d023a GREEN items 1+2: LayoutCache::overrides_only_hint + OverridesOnlyStamp, armed in tick_animations
  (tween_keeps_layout_tree_shape allowlist), consumed in layout_document Step 1; css_dirty_reemit_set in the
  patch arm; voided by apply_content_change and new generations
- 1823dcb68 RED layout/tests/a_relayout_keeps_the_virtual_views_of_an_unchanged_host.rs (item 4)
- 64feec075 GREEN item 4: VirtualViewManager record_invoked_node / invoked_node / carry_over_views /
  take_carried / take_all_carried; funnel keeps unchanged views (unchanged_virtual_views); invoke re-renders a
  carried view whose box changed size; unreached kept views drop their child
- 16c1989d6 scripts/animframe8_tick_probe_gen.py (frame-report probe); before: 6 ticks = 6 layout passes
- 7e79718e6 + 5a851d5d6 report sections 3-8 (what changed, commits, expected after, api.json, least sure,
  test commands, left/risks)

## IN PROGRESS
- Self-review of the diff (`git diff 5745afee6`) for compile / behaviour slips. Reviewed so far:
  solver3/mod.rs (ok). Next to re-read: dll headless/mod.rs:7012 reads `last_reconcile_was_skipped` (now
  also true for an overrides-only pass - check that test's meaning), then window.rs tick block once more.

## NEXT (if resumed)
1. dll/src/desktop/shell2/headless/mod.rs ~7012: the resize test asserts the resize took the fast path via
   `last_reconcile_was_skipped`; an overrides-only pass sets it too. Only a problem if that test arms a tween
   (it does not, as far as read) - confirm and note in the report.
2. Re-read window.rs `tick_animations` css block (search "THE GPU PROPERTY PATH") end to end.
3. Nothing else owed: report + progress committed. The parent builds and runs section 7 of the report.

## DECISIONS
- Knob: transform in BOTH states (translateX(0px) when off) so the reference frame exists from the first
  layout; the first toggle needs no DL rebuild.
- GPU path only for `transform` (not opacity): CSS opacity is baked into PushOpacity (only the ANIMATION
  opacity channel is key-bound); an opacity tween keeps the DL-rebuild path (now without lingering dirt).
- The three tick decisions unified in LayoutWindow::take_animation_frame_work (dll wraps it to raise
  content_repaint_pending for the paint-only frame).
- Overrides-only latch armed only for shape-keeping layout tweens (allowlist), never with paint restyles in
  the same frame or other pending dirt; voided by content changes / new generations; stamp = node count +
  cascade epoch + states hash.
- VirtualView identity = the VirtualViewNode (callback + RefAny instance) the view was last invoked for;
  only on the relayout entry; a carried view re-renders when its box size changed.
- Not done (documented in the report "Left"): per-node compact patch, CSS opacity on the GPU channel, the
  DL Arc copy of a colour patch, the no-op relayout's reconcile, lints per relayout.

## OPEN QUESTIONS
- none blocking.
