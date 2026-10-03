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
- 562e3f9e3 unit tests (retained_tree_tests in a11y.rs); 1eb875879 doc links
- 6d7e41bba RED + 937db639f GREEN: telemetry FramePump no longer discards the AZ_PROFILE=cpu spans (drain_probe_events_for)

## IN PROGRESS
- appending test `an_animation_frame_builds_only_the_nodes_it_changed` (asserts A11yManager::last_pass: a no-op
  relayout built/sent 0, a knob frame built 1 sent 1) to the END of
  layout/tests/an_animation_frame_sends_assistive_technology_only_what_moved.rs

## NEXT (exact)
1. Commit that test (check the last 3 commits for "test(a11y): an animation frame builds only ...").
2. Write the report scripts/A11YPATCH8_2026_10_03.md (what was built, measurement, commits, api.json: none -
   A11yManager is Rust-only, least-sure-to-compile spots, test commands, left / follow-ups) and commit it.
   Least-sure spots: a11y.rs `retain_or_build` match guard on `get_mut`; `published.is_some_and(..)` reading `r`
   in the third pass of `rebuild_retained`; window.rs disjoint borrows (`inputs` borrows self fields while
   `manager = &mut self.a11y_manager`); dll feed `tree.as_ref()` through the MutexGuard deref; the unit-test module
   `retained_tree_tests` uses `super::autotest_generated::layout_result_of` (made pub(super)).
3. Follow-ups for the report: dll `refill_a11y_tree_after_regeneration` (event.rs, ANIMFRAME8's file) is now a
   redundant second pass per regenerate (finds nothing, ~1 ms of hashing); the mobile A11ySnapshot is still rebuilt
   whole (could skip when `last_pass.published` is false); the remaining per-frame cost (~1 ms est.) is SipHash
   signatures of ~3500 nodes + the structure pass - a per-node content epoch set by every NodeData mutation would
   remove the hashing.

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
  lint printed right after it shows in every tick. Its span is missing from the tick tables because the telemetry
  FramePump around every present / regenerate_layout drained and DISCARDED the probe buffer (telemetry off) - every
  span after a relayout's last per-DOM flush vanished (`shell_incremental_relayout`, `register_scroll_nodes`,
  `cpu_hit_tester_rebuild` too). Fixed in 937db639f. The 4 visible a11y spans were the dll's refill after
  regenerate_layout (a second full rebuild per regenerate: ~6 ms per DOM rebuild, ~3 ms per tick).
