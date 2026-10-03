# ANIM8 progress (wave 8, branch wt/anim8, base 45c6bf98b)

Task: CSS transitions / animations jump to the end value (user regression 2026-10-03: AzWidgets toggle,
button hover). Then the idle items (FLIP springs never settle; AzReview per-frame image callbacks).

## DONE
- a8e1b4e9a progress file
- b57bb6bb7 RED core/src/diff_test.rs mod a_toggled_widget_keeps_its_identity (2 tests)
- 1a2be0ff6 GREEN NodeData Hash (core/src/dom.rs ~1892): inline props hashed order-free (count + wrapping
  sum of per-property hashes) - finding 2 fixed at its root
- 722726d66 RED e2e/css-hover-transition.json (RED on prebuilt AzCalculator via AZ_E2E: step 7 transitions 0)
- 60f3f4580 GREEN 1/2 LayoutWindow::seed_state_change_transitions + node_states + CssTransition::declared +
  declared_animation_for (window.rs)
- d58dbdf13 GREEN 2/2 wired: dll common/event.rs apply_hover_restyle / apply_active_restyle /
  apply_focus_restyle_in_dom; layout e2e/runner.rs hover + focus restyles
- 063c009d8 a 0 ms state animation = at once (press instant, release fades)
- 93efe579a RED layout/tests/a_button_fades_into_its_hover_face.rs; d5cebf5a7 GREEN decl::state_fade +
  BUTTON_FACE/BUTTON_FADE_MS, flat::button / flora::button push it (not Link, only when btn_owns_style)
- 0d5e3fd53 RED layout/tests/a_rebuild_under_the_pointer_starts_no_transition.rs; d3ed2527f GREEN
  with_interaction_of in begin_reconciliation's CSS diff
- f66509f40 RED / dd4bb436d GREEN css background::interpolate_background_layers (gradient faces tween)
- 00ae0e853 refactor: rebuild-diff + imperative sites use CssTransition::declared / declared_animation_for
- ca8e4f111 scripts/anim8_probe.py (debug-server probe: knob x / pixels / get_animations per frame) and
  scripts/anim8_switch_scenario_gen.py (writes an AZ_E2E scenario: click the switch, tick_animations 1/3/30,
  each checkpoint ends in a failing assert_response that PRINTS the response). Run:
  python3 scripts/anim8_switch_scenario_gen.py /tmp/anim8/switch.json
  run_capped.sh --cap-mb 1500 --seconds 150 --log /tmp/anim8/switch.log -- env AZ_BACKEND=headless \
    AZ_E2E=/tmp/anim8/switch.json /Users/fschutt/Development/azul/target/release/AzWidgets

## FINDINGS SO FAR (prebuilt AzWidgets, wave-7 build)
1. HOVER FADE NEVER EXISTED: no widget/theme declares `animation` for hover colours, and the pseudo-state
   restyle (dll common/event.rs apply_hover_restyle / apply_active_restyle / apply_focus_restyle_in_dom ->
   StyledDom::restyle_on_state_change) seeds NO CssTransition (only LayoutWindow::apply_node_css_change
   ~window.rs:8780 and begin_reconciliation ~window.rs:13594 do). Measured: button bg jumps
   (101,101,101)->(73,80,87) in one frame, transitions=0.
2. SWITCH: scripted clock (AZ_E2E) the knob DOES tween: 60 -> 59.6 (1 tick) -> 50.5 (3) -> 44 (30).
   BUT after the click the TRACK (node 207) gets a bogus FLIP slide translate=(490, 7425): the rebuild's
   reconcile matched new track 207 with the OTHER switch far down the page (old ~1563, a settings switch).
   ROOT CAUSE (being confirmed): core/src/diff.rs reconcile_dom pass A2 (exact SUBTREE hash, gated only
   by the nearest ancestor with a terminal id/key). The click handler's set_css_property UPSERTS the
   inline props (NodeData::upsert_inline_css_property core/src/dom.rs:3538 removes the decl and APPENDS a
   new rule block) and NodeData's Hash (dom.rs:1878) hashes the inline props IN ORDER (discriminants) ->
   the toggled switch's subtree hash != a freshly built switch in the same state, so the fresh new switch
   matches the OTHER identical switch (document order). Effects: bogus FLIP from far away;
   css_transitions / user overrides remapped (remap_node_ids / migrate_user_overrides_from) to the
   wrong node -> the toggled switch can snap while the other switch gets its tween.
3. Real-time cost (headless): click -> RefreshDom regenerate_layout ~600-700 ms (3472 nodes; phases:
   create_from_dom ~240, layout_and_dl ~230-480, state_migrate ~90-320 = the reconcile + CSS diff over
   CssPropertyType::ALL); each transition tick -> incremental_relayout 118-290 ms (others 12 ms). A 150 ms
   glide therefore shows ~1 frame in real time = "immediately transitions". forget_animation_stall
   (window.rs:14418) is called after regenerate_layout (event.rs:4776/4802) - OK.
4. live_tracks = 23..27 at rest in AzWidgets (spinners etc.) - idle item.

## NEXT (exact)
- DONE. Report: scripts/ANIM8_2026_10_03.md (dba531a16, fbb6f1551). Nothing left on this branch; the open
  items (per-frame relayout cost of layout-property tweens = solver3; GPU verification) are in the
  report's "Left" section for the parent.

## Decisions / open questions
- Hover fade is a missing feature, not a regression; decided to implement it in the engine (the user
  expects it).
