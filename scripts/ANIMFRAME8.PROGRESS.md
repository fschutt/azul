# ANIMFRAME8 progress (wave-8 follow-up, branch wt/animframe8, base 5745afee6)

Task (PLAN.md "Wave-8 follow-ups" / ANIMFRAME8): an animation frame must cost ~1-2 ms. A transform /
opacity animation: no layout, no display-list rebuild (GPU property channel). A paint-only transition:
patch the display list. A layout-property transition: relayout only the dirty subtree. Answer "do we have
duplicated paths?" with a path map. Then, RED test per item: (1) a tick that changes no DOM node does not
reconcile; (2) style-only changes take the DL patch path; (3) css_transition_tick lightweight; (4)
VirtualView callbacks not re-run on a relayout whose host node is unchanged. Never compile; never touch
page_breaks.rs. Not the a11y code (A11YPATCH8).

## STATUS: IN PROGRESS

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

## IN PROGRESS
- item 3 (css_transition_tick lightweight): batch the slow path into ONE compact rebuild per tick; border
  colours patchable (patch_paint_colors gains border sides) so a flat Button face fade is fully patched.

## NEXT
- item 1: override-only latch (`LayoutCache::overrides_only_hint` + stamp) armed by tick_animations when all
  relayout tweens keep the tree shape; consumed in layout_document Step 1 like resize_only.
- item 2: the skipped-reconcile patch arm re-emits css-dirty nodes (+ subtrees) instead of falling back.
- item 4: VirtualView keep-alive on the relayout entry (host VirtualViewNode identity in the manager).
- report sections 3/4, measurement commands.

## DECISIONS
- Knob: transform in BOTH states (translateX(0px) when off) so the reference frame exists from the first
  layout; the first toggle needs no DL rebuild.
- GPU path only for `transform` (not opacity): CSS opacity is baked into PushOpacity (only the ANIMATION
  opacity channel is key-bound); an opacity tween keeps the DL-rebuild path (now without lingering dirt).
- The three tick decisions unified in LayoutWindow::take_animation_frame_work (dll wraps it to raise
  content_repaint_pending for the paint-only frame).

## OPEN QUESTIONS
- none blocking.
