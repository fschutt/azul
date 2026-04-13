# Review: layout/src/managers/hover.rs

## Summary
- Lines: 258
- Public functions: 13 (new, push_hit_test, remove_input_point, get_current, get_current_mouse, get_frame, get_history, get_active_input_points, frame_count, clear, clear_input_point, has_sufficient_history_for_gestures, any_has_sufficient_history_for_gestures, current_hover_node, previous_hover_node, remap_node_ids)
- Public structs/enums: 2 (HoverManager, InputPointId)
- Findings: 1 high, 0 medium, 0 low

## Findings

### [HIGH] Dead Code — `get_active_input_points` only used in tests
- **Location**: `hover.rs:102`
- **Details**: `get_active_input_points` is only called in `layout/tests/hover_manager.rs:40`. No production call sites exist.
- **Evidence**: Grepped `\.get_active_input_points\(` — only match outside hover.rs is in `layout/tests/hover_manager.rs`.
- **Recommendation**: Downgrade to `pub(crate)` or remove if not needed for public API. Note: cannot use `pub(crate)` because the call site is in an integration test (`layout/tests/`), which is outside the crate.

## System Documentation
- System identified: yes — Event / Input handling system (hover state, gesture detection, hit testing)
- Existing doc: `doc/guide/lifecycle.md` covers the event loop at a high level; no dedicated event/input system guide exists
- Doc needed: An "Event & Input Handling" guide covering the hit-test → hover-manager → event-determination → callback pipeline, including gesture detection and multi-touch support
