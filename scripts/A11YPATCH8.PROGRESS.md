# A11YPATCH8 progress (branch wt/a11ypatch8, base 5745afee6)

Task: the accessibility tree sends patches (accesskit TreeUpdate with only changed / added / removed nodes) and
NOTHING when a frame changed nothing a11y-visible. Brief: scripts/waves/wave8/PLAN.md "A11YPATCH8".

## DONE
- c73731cb2 progress file; 93928238a measurement
- d2d7e3cb4 RED: layout/tests/an_animation_frame_sends_assistive_technology_only_what_moved.rs (registered in all.rs)
- 75fe8e2fe / 6f22afe35 / b233be0c2 GREEN a11y.rs: retained tree types, A11yTreeMirror::apply_patch_in_place, refresh/publish/fold/take_pending, rebuild_retained + node_signature + build_content + screen_bounds (update_tree = a from-scratch rebuild)
- 954aa5d49 GREEN window.rs: update_a11y_tree -> A11yManager::refresh; incremental path note_published_node / resend_full_tree on refusal

- 0de75c39c tests adapted (a11y_consumer_contract parked-full test, scroll_chain fixed box reads published_node)
- 3937fde8f A11yIdHasher / A11yIdMap (one-multiply hasher for id-keyed per-frame maps); 19c42122d, df0cb32f8, ba15bae74 tidy
- af61005d9 dll feed: CompleteTree map, O(patch) merge_into, A11yTreeFeed::missed + 2 tests
- 01fba56ff adapters call missed() on a busy lock / caught panic; macOS init_accessibility -> resend_full_tree
- 562e3f9e3 unit tests (retained_tree_tests in a11y.rs)

## IN PROGRESS
- final review of the new code for compile errors; then the report

## NEXT
- review a11y.rs new code once more (compile), write scripts/A11YPATCH8_2026_10_03.md

## Decisions
- Bounds of a moving node are sent EVERY frame they change (a 1-node patch); no throttle: accesskit has no lazy bounds, the cost was the 3 ms full rebuild + full-tree consumer diff, not one node.
- Per-node change detection = a SipHash signature of every input the node's content is built from (node_type, attributes, flags, AccessibilityInfo, focusable/activation bools, direct children's text, text override, cursor, screen bounds, scroll info); an unchanged signature reuses the retained node, no build.
- A refused publish or a new adapter -> the retained tree is dropped, the next pass publishes a FULL tree (resync).

## Open questions
- (none yet)

## Measurement (2026-10-03, prebuilt AzWidgets of 0de2a2529, headless 900x1300, /Users/fschutt/Development/azul-work/a11yp8/)
- `a11y_update_tree` = 2880 / 2906 / 2956 / 3041 / 3179 / 3194 us per call (AZ_PROFILE=cpu, 3472-node page, 4 DOMs).
- Unprofiled: a knob tick (incremental_relayout) 19.9 - 21.7 ms, a no-op relayout 11.1 - 11.6 ms -> the a11y
  rebuild is ~15% of a tick and ~27% of a no-op relayout.
- It runs after EVERY layout pass (window.rs layout_and_generate_display_list_impl tail, `update_a11y_tree`); the
  lint printed right after it shows in every tick. Its span is missing from the tick tables only because spans
  closing after a relayout's last per-DOM flush never reach a [CPU] table (`shell_incremental_relayout`,
  `register_scroll_nodes` are missing the same way; a get_profile_report right after a tick drained nothing).
