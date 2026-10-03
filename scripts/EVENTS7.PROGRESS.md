# EVENTS7 progress (wave 7, branch wt/events7 from 2e55eef06)

Brief: scripts/waves/wave7/EVENTS7.md. Report at the end: scripts/EVENTS7_2026_10_03.md.

## DONE
- item 1 (VirtualView page events bubble out through the host):
  - e08ae69dc RED core/src/events_test.rs `event_path_across_doms_tests` + stubs (get_event_path,
    propagate_event_along, PathPropagationResult, hover_callbacks_along_path; propagate_event wraps the
    path version)
  - 971c2fa9a GREEN get_event_path crosses hosts
  - 5e56a6547 dll event.rs + layout/src/e2e/runner.rs Hover arms take core's hover_callbacks_along_path
  - 95c932691 layout/src/context_menu.rs nearest_context_menu walks core's get_event_path (twin removed)
- item 2 (macOS Edit menu through the key path):
  - c074af165 RED dll/src/desktop/shell2/headless/tests/shortcut_keys.rs (+ `mod shortcut_keys;` in
    headless/mod.rs tests) + stub PlatformWindow::press_shortcut_keys
  - 76bafe411 GREEN press_shortcut_keys (common/event.rs, after consume_keyboard_delta)
  - b47d4f120 macos/mod.rs edit_command -> press_shortcut_keys(EditCommand::keys()); can_undo/can_redo
    true on an editing focus; dead perform_undo/perform_redo removed

- item 3 (runner undo keys):
  - 08260920e RED runner.rs tests (tap_key primary+Z / Shift+Z / Y; veto guard)
  - 4561f90d6 GREEN LayoutWindow::{undo,redo}_text_edit(_for_seat) + pub UndoRestore in
    layout/src/managers/undo_redo.rs; dll arms call them (dll copies deleted); runner DefaultAction +
    SystemChange arms
  - 500c88730 JSON scenario test a_json_scenarios_undo_key_undoes_the_typing

- item 4 (headless menus close on Escape / outside press, as a chain):
  - 515c695e4 RED dll/src/desktop/shell2/headless/tests/e2e_host.rs (3 tests at the end)
  - d3e4ebcfc refactor transient.rs fresh_press / fresh_release / fresh_escape (3 twins -> 1)
  - 4708e715e GREEN common/event.rs (menu self-dismissal without mailbox; owner side
    PlatformWindow::dismiss_menu_windows + CommonWindowState::menu_release_owed swallow); headless
    dismiss_menu_windows + chain rule in pump_children

- item 5 (Ctrl+B with no selection reported to the app):
  - e2559312e RED: EventType::TypingStyleChanged + FocusEventFilter::TypingStyleChanged (appended,
    matcher, ALL_FOCUS, event cycle test, FOCUS test list); runner test + editor_runner_with helper
  - 5ab5bb504 GREEN: dll keyboard default-action pass dispatches it after a successful toggle
    (typing_style_changed_at, next to the synthetic click); runner arm the same
  - 95ffba393 coverage ratchet `cases` gains the new EventType

## IN PROGRESS
- the report scripts/EVENTS7_2026_10_03.md

## NEXT
6. report scripts/EVENTS7_2026_10_03.md, commit it

## Decisions
- item 1: the plan lives in core (core/src/events.rs, mine), both dispatchers call it with closures over
  layout_results + VirtualViewManager::host_of_nested_dom; no new layout file.
- the one plan plans each callback once (the old twins planned a node with two same-filter callbacks 4x).
- item 2: ONE path for menu click and key equivalent: the keystroke is pressed + released through the
  key passes (no NSApp.currentEvent inspection; layout-independent; also releases the letter key, which
  AppKit never sends a keyUp: for while Cmd is held).

- item 4: the click that leaves a headless menu is spent (press discarded, release owed) like a
  native menu / X11 grab; a menu child closing closes every menu child of its owner (one chain).

- item 5: what was left after TEXTENG (runs in DocumentTextEdit) and WRITER6 (RTE adoption) is the toggle
  itself: a dedicated event (EventType / FocusEventFilter::TypingStyleChanged), not a TextChanged with no
  text change (TextChanged means "the text changed": dirty flags, word counts would misfire).

## Open questions
- (none yet)
