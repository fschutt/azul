# E2E-C progress (branch wt/e2e-c, base 05ef3a8f4) - 2026-10-06

Apps: AzReader, AzReview, AzSetup, AzSheets, AzShow, AzTasks, AzTerm, AzVideoCut, AzWriter.
Binaries: /Users/fschutt/Development/azul/target/release/<App> (engine 889dccf30+). Logs: /tmp/e2e-c/<app>.log.

## Shared helper (scripts/azlin_e2e.py)
- SCRIPT (commit "a bare stdout line counts as printed"): `printed(key)` never matched a bare line
  (`AZREADER_READY`, `AZWRITER_READY`, `AZTERM_READY` print no value; the regex wanted `KEY <value>`), so every
  "wait for the window" timed out. A bare line now counts as "" when the pattern can match an empty value.

## AzReader - PASS (after the helper fix)
- Seen (not a script failure): the first-page screenshot's right page repeats page 1 (the chapter head and the
  first paragraphs) and is cut mid-line - page 2 of the spread does not show the continuation. Not investigated
  yet.

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

## AzShow / AzTasks / AzTerm / AzVideoCut / AzWriter
- status: not run yet

## NEXT
- run AzShow
