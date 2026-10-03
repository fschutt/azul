# A11YPATCH8 progress (branch wt/a11ypatch8, base 5745afee6)

Task: the accessibility tree sends patches (accesskit TreeUpdate with only changed / added / removed nodes) and
NOTHING when a frame changed nothing a11y-visible. Brief: scripts/waves/wave8/PLAN.md "A11YPATCH8".

## DONE
- c73731cb2 progress file; 93928238a measurement
- d2d7e3cb4 RED: layout/tests/an_animation_frame_sends_assistive_technology_only_what_moved.rs (registered in all.rs)

## IN PROGRESS
- GREEN in layout/src/managers/a11y.rs: retained tree (per-node input signature + built node), diff -> patch, publish validates a patch in O(patch) (no full mirror clone), fold prunes removed nodes

## NEXT
- a11y.rs GREEN (A11yRetainedTree + A11yManager::refresh), then window.rs update_a11y_tree -> refresh, incremental path keeps retained in sync / resyncs on refusal
- fix tests whose premise was 'every pass parks a full tree': a11y_consumer_contract a_parked_full_tree_absorbs..., scroll_chain a_fixed_box_is_reported...
- dll feed (common/accessibility.rs): HashMap tree, O(patch) merge, missed delivery -> full resync; macOS init_accessibility asks for a full tree

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
