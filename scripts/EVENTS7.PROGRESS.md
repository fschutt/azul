# EVENTS7 progress (wave 7, branch wt/events7 from 2e55eef06)

Brief: scripts/waves/wave7/EVENTS7.md. Report at the end: scripts/EVENTS7_2026_10_03.md.

## DONE
- item 1 (VirtualView page events bubble out through the host):
  - e08ae69dc RED core/src/events_test.rs `event_path_across_doms_tests` + stubs (get_event_path,
    propagate_event_along, PathPropagationResult, hover_callbacks_along_path; propagate_event wraps the
    path version)
  - 971c2fa9a GREEN get_event_path crosses hosts
  - 5e56a6547 dll event.rs + layout/src/e2e/runner.rs Hover arms take core's hover_callbacks_along_path
  - (next commit) layout/src/context_menu.rs nearest_context_menu walks core's get_event_path (twin removed)

## IN PROGRESS
- item 2: macOS Edit menu (read dll/src/desktop/shell2/macos menu / edit_command code)

## NEXT
2. macOS Edit menu Undo/Redo/Cut/Copy/Paste/SelectAll through the key default-action path
3. layout/src/e2e/runner.rs: DefaultAction::UndoTextEdit / RedoTextEdit arms (LayoutWindow method shared with dll)
4. headless menus close on outside click / Escape
5. Ctrl+B with no selection reported to the app

## Decisions
- item 1: the plan lives in core (core/src/events.rs, mine), both dispatchers call it with closures over
  layout_results + VirtualViewManager::host_of_nested_dom; no new layout file.
- the one plan plans each callback once (the old twins planned a node with two same-filter callbacks 4x).

## Open questions
- (none yet)
