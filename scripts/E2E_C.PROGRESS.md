# E2E-C progress (branch wt/e2e-c, base 05ef3a8f4) - 2026-10-06

Apps: AzReader, AzReview, AzSetup, AzSheets, AzShow, AzTasks, AzTerm, AzVideoCut, AzWriter.
Binaries: /Users/fschutt/Development/azul/target/release/<App> (engine 889dccf30+). Logs: /tmp/e2e-c/<app>.log.

## Shared helper (scripts/azlin_e2e.py)
- SCRIPT (commit "a bare stdout line counts as printed"): `printed(key)` never matched a bare line
  (`AZREADER_READY`, `AZWRITER_READY`, `AZTERM_READY` print no value; the regex wanted `KEY <value>`), so every
  "wait for the window" timed out. A bare line now counts as "" when the pattern can match an empty value.

## AzReader - PASS (after the helper fix)
- Seen (not a script failure): the right page of the spread repeated page 1's start, cut to page 2's height.
  ENGINE: a relatively positioned box moved without its content (adjust_relative_positions shifted only the box,
  its subtree only for table rows); get_node_layout: shift box y -265.8, the column in it 236.2. RED da8623f2e,
  GREEN 5db6092da (layout/src/solver3/positioning.rs). Probe: no drift of the shift across theme / mode /
  resize / hover relayouts on today's build.

## AzReview - ENGINE (headless), fixed unverified
- Fails at "Escape closes the About box first: expected 'closed', last 'open'".
- Root cause: every transient popup is a WindowType::Menu window; the headless owner's `dismiss_menu_windows`
  closed it as a window-based menu on the Escape (no `Dismissed`, key spent), so the Modal's on_close never ran.
  And headless never replayed a key forwarded to a popup (`deliver_forwarded_keys` was the default no-op).
- RED b764fe592 (dll/src/desktop/shell2/headless/tests/e2e_host.rs, two tests), GREEN 238e8e612
  (dll/src/desktop/shell2/headless/mod.rs: `is_menu_window` excludes mailbox windows; `deliver_forwarded_keys`).
- Probe on the current binary: closing the box by its close button prints AZREVIEW_ABOUT closed and the second
  Escape closes the settings page - so the rest of the script should pass once the fix is built.

## AzSetup - ENGINE (the AzReview fix), unverified
- Fails at "Escape closes the question" (the exit question is a MessageBox in a Modal): the same headless
  menu-dismissal of a modal. With the two modal closes done by the modal's close button instead
  (/tmp/e2e-c/setup_skipbox.py), the whole rest of the script passes on today's binary.
- Seen: Settings > General, the "Requires restart" badge's second line ("restart") hangs out of its pill.

## AzSheets - PASS (after two script fixes)
- SCRIPT: its `Sheets.settle(before, what)` shadowed the helper's `settle(limit)` that every click runs since
  05ef3a8f4 (TypeError) - renamed `await_reply`.
- SCRIPT: the close question is a Modal (window "azul-transient"); "Cancel" was clicked in the owner window.
  The helper's `click` now takes `window=`.
- Seen (not a script failure): after File > New > Budget sample the grid shows stale cell borders of the
  previous layout all over the empty cells (headless CPU repaint / damage?), screenshot sample-flat-light.png.

## AzShow - PASS (after a script fix)
- SCRIPT: the close question's Cancel is in the Modal's own window ("azul-transient").
- NOTE the script already tolerates: the slide sorter's drag and drop delivers no drop headlessly (Mod+Down
  reorders instead). Not investigated.

## AzTasks - SCRIPT fixed + ENGINE fix, unverified
- SCRIPT: the quick-add click landed 14 px below the field: dismissing the reminder banner slides the list
  up (~170 layout animations) and the raw click op targets what is painted. Clicks now go through the
  helper's `click` (settles first).
- SCRIPT: the planned-month drag took the day's FIRST planned task - the sample's own task on tomorrow - not
  the ferns; `planned_box` finds the ferns by title in the day.
- ENGINE: no drop ever arrived: a scripted pointer (ModifyWindowState / QueueWindowStateSequence) never fed
  the gesture manager, so no DragStart (the log showed only TextSelectionDrag). RED 22d564f60 + 32952e3ba
  (e2e_host.rs: a scripted drag drops; two scripted press/release cycles are one double click), GREEN
  9695862dc (dll event.rs `record_scripted_pointer_sample` in both arms; full.rs double_click op stops
  injecting a native DoubleClick, which would now be a second one).
- With the drag steps skipped (/tmp/e2e-c/tasks_skipdnd.py) everything else PASSES on today's binary.

## AzTerm - PASS (after the helper's bare-line fix)

## AzVideoCut - PASS (after script fixes, c6beb5121)
- SCRIPT: the export Dialog is modal (window "azul-transient"): its controls were clicked in the owner.
- SCRIPT: AZVIDEOCUT_EXPORTED's key has spaces ("Sample cut.mp4"); the \S+ pattern never matched.
- SCRIPT: key_up carried the chord's modifiers (held for the next click); click now settles (shared settle).

## AzWriter - SCRIPT fixed + ENGINE fix, unverified
- SCRIPT: `get_node_layout text="Undo"` answers the text node (rect null) -> helper `text_rect`.
- SCRIPT: the close question's Save was clicked in the owner -> clicked in "azul-transient".
- ENGINE: Save saved, then the write-back's close_window() closed the QUESTION's window (the thread was
  started by the question's button, so it replies in the popup); the document window stayed. RED fcfce42a2,
  GREEN 199df4129 (mailbox `close_owner`; CloseWindow in a popup closes the owner through its close protocol).
- With the guard step replaced by Cancel + Ctrl+S (/tmp/e2e-c/writer_skipguard.py) the rest PASSES, the
  restart included.

## NEXT
- AzSheets stale grid borders (optional), then the final report scripts/E2E_C_2026_10_06.md
