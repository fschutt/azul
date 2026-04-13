# Review: dll/src/desktop/shell2/common/event.rs

## Summary
- Lines: 4575
- Public functions: 3 (HitTestNode, InvokeSingleCallbackBorrows, CommonWindowState structs; PlatformWindow trait with ~40 methods)
- Public structs/enums: 3 (HitTestNode, InvokeSingleCallbackBorrows, CommonWindowState)
- Findings: 1 high, 3 medium, 2 low

## Findings

### [HIGH] Unsafe Code — Lifetime extension via raw pointer cast
- **Location**: `event.rs:3502`
- **Details**: `let info_ptr = &info as *const InputInterpreterInfo as *const InputInterpreterInfo<'static>;` casts a local reference to a `'static` lifetime pointer. This is unsound — the `InputInterpreterInfo` borrows local data (`synthetic_events`, `hit_test_for_dispatch`, etc.) and the callback could store the pointer beyond the local scope. If the callback stores or leaks this pointer, it becomes a dangling reference.
- **Evidence**: Line 3502 performs `*const InputInterpreterInfo as *const InputInterpreterInfo<'static>`.
- **Recommendation**: Redesign the interpreter callback to accept a proper reference with the correct lifetime, or document the safety invariant that the callback must not store the pointer.

### [MEDIUM] Dead code — `record_accessibility_action` has zero callers
- **Location**: `event.rs:3288-3306`
- **Details**: The `record_accessibility_action` method is defined on `PlatformWindow` but never called anywhere in the codebase outside this file. It is gated behind `#[cfg(feature = "a11y")]`.
- **Evidence**: Grep for `record_accessibility_action` across the entire codebase returns only the definition and comments in this file.
- **Recommendation**: This appears to be a planned but unwired accessibility integration point. Flag for follow-up when a11y actions are connected from platform-specific screen reader code.

### [MEDIUM] Placeholder stub — DefaultAction handlers
- **Location**: `event.rs:3887-3891`
- **Details**: `DefaultAction::SubmitForm`, `DefaultAction::CloseModal`, `DefaultAction::SelectAllText` all have a comment "Placeholder for future implementation" and do nothing.
- **Evidence**: Line 3890: `// Placeholder for future implementation`.
- **Recommendation**: Track these as open items. Low priority if these DefaultAction variants are rarely triggered.

### [MEDIUM] Unsafe raw pointer for hit testing could be avoided
- **Location**: `event.rs:591-616`
- **Details**: `CommonWindowState::perform_hit_test` creates a raw pointer `layout_results_ptr` to work around simultaneous borrows of `self.hit_tester` and `self.layout_window`. The safety comment says "layout_results is not modified by hit testing" but this pattern is fragile.
- **Evidence**: Lines 591-602: `let layout_results_ptr = match self.layout_window.as_ref() { ... }; ... let layout_results = unsafe { &(*layout_results_ptr).layout_results };`
- **Recommendation**: Consider restructuring to pass `layout_results` separately or use a temporary reference split. The raw pointer trick works but is easy to break with future refactoring.

### [LOW] File size — 4575 lines
- **Location**: Entire file
- **Details**: The file is large but cohesive: it contains the `PlatformWindow` trait with its unified cross-platform event processing logic. The `apply_user_change` (~980 lines) and `process_window_events` (~725 lines) methods are large but each arm of the match statements is relatively small.
- **Recommendation**: No split needed. The file is cohesive — all code serves the unified event processing system. Individual match arms could potentially be extracted to helper methods if the file grows further.

### [LOW] Documentation verbosity — Module doc and trait docs
- **Location**: `event.rs:1-136`, `event.rs:735-770`
- **Details**: The module doc (136 lines) and `PlatformWindow` trait doc are extensive with ASCII diagrams and platform-specific integration notes. While useful as a reference, the per-platform notes may drift as platforms evolve.
- **Recommendation**: Acceptable. The module doc is a valuable architectural reference. Keep it maintained as platforms change.

### [LOW] `parse_node_type_from_str` only called from `InsertChildNode`
- **Location**: `event.rs:176-236`
- **Details**: This function is only called once (line 1935) from the `InsertChildNode` debug API handler. It's a private function, so not dead code, but could live closer to its single caller.
- **Recommendation**: Low priority — the function is fine where it is.

## System Documentation
- System identified: yes — Event Processing / Windowing System (cross-platform event loop, callback dispatch, state diffing)
- Existing doc: `doc/guide/lifecycle.md` (partial), `doc/guide/architecture.md` (partial)
- Doc needed: A dedicated `doc/guide/event-processing.md` covering the state-diffing event model, `PlatformWindow` trait contract, `process_window_events()` flow, and the `PreCallbackFilterResult`/`SystemChange` pipeline. The module doc in this file is excellent but not discoverable from the guide.
