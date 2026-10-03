# WRITER6 progress (wave 6, 2026-10-03)

Branch `wt/writer6` from `25d78e309`. Brief: scripts/waves/wave6/WRITER6.md.

## DONE (commits)
- 8ffc4f4fd LOOK findings (AzNotes E2E passes on the prebuilt; screenshots in target/writer6-look/, not committed)
- 69c7c28d9 RED core (undo keys reach callbacks), cabe8e0b2 RED layout (+ DefaultAction stubs),
  2ba518aeb GREEN core, 5eb307591 GREEN default_actions, b059a2a37 GREEN dll arm.
  NOT done (for HEADLESS6): layout/src/e2e/runner.rs has no UndoTextEdit arm (never had one; the body
  lives in dll event.rs `undo_text_edit_on`). macOS Edit-menu `undo:` still applies UndoTextEdit directly.

- 00a3ee873 RED (RTE: engine formats + undo keys, in a_rich_text_editor_keeps_one_model_and_one_history.rs),
  da8e76e3b GREEN undo keys in the editor, 4cb54a4e6 doc.rs apply_reported_formats (+unit test),
  55ce2dc21 GREEN sync_text takes formats from DocumentTextEdit.runs, toggle at caret from get_typing_formats.
  DECISION: RichTypingStyle / state.typing stay (api.json; inline code has no engine format; pressed state).

## IN PROGRESS
- NEXT: the pager entry for AzWriter (RTE 8b): RichTextEditor renders a RANGE of blocks (a page) with
  `<host>-<index>` ids, and block mapping (path_in_host / block_of) falls back to the nearest ancestor's
  `<host>-<index>` id when the blocks are not the host's direct children. Then AzWriter adoption.

## DONE design notes - Undo/Redo ownership. DESIGN (decided): the browser keydown model. core `handle_key_down` stops
  claiming primary+Z / Shift+Z / Y for the PRIMARY seat (no AddAndSkip: the KeyDown passes to callbacks);
  the engine's text undo becomes the key's DEFAULT ACTION (`DefaultAction::UndoTextEdit { target }` /
  `RedoTextEdit { target }`, appended to core's DefaultAction) decided in layout/src/default_actions.rs
  for an editable focus, run by the dll (event.rs DefaultAction match -> apply_system_change(UndoTextEdit))
  and the e2e runner (runner.rs DefaultAction match) after the callbacks, VETOED by prevent_default -
  exactly like Ctrl+B ToggleTextFormat. An app that owns its history (the RichTextEditor) handles Ctrl+Z in
  its VirtualKeyDown and calls prevent_default. Non-primary seats keep SeatShortcut. macOS Edit-menu
  `undo:` path (macos/mod.rs edit_command) still applies UndoTextEdit directly - follow-up in the report.
- NEXT STEP: RED tests: core/src/events_test.rs (near line 3790: primary+Z on an editable focus yields no
  UndoTextEdit system change and the KeyDown reaches the user events) + layout/tests/<new>.rs
  (determine_keyboard_default_action_with_editing for primary+Z / Shift+Z / Y in a contenteditable host
  = UndoTextEdit / RedoTextEdit; prevented -> no action), pattern of
  layout/tests/a_format_toggle_at_a_caret_styles_what_is_typed_next.rs.

## NEXT
1. (done) LOOK.
2. Undo/Redo shortcut ownership (see IN PROGRESS).
3. Engine formats into the editor (DocumentTextEdit.runs, get_typing_formats) - RED in layout, then rich_text_editor.rs.
4. AzWriter on RichTextEditor + RichTextDoc (RTE section 8 a-e).
5. AzNotes polish (close via CloseRequested, azul_pim search/tags, prefixes, appkit).
6. Report.

## Seen broken (LOOK, prebuilt aa59b2d84, screenshots in target/writer6-look/{notes,notes2,writer})
AzNotes (scripts/aznotes_e2e.py PASSES against the prebuilt binary):
- N1 a check item renders TWICE: a phantom line "* * call the bakery" (two list markers) above the real
  check line; the DOM is right (4 blocks, the check `li` = list-style none, position relative, the box an
  abspos island after the runs). Fresh load (restart) shows the same -> layout, not the edit glue.
- N2 the checkbox island's colour `system:accent` resolves to #53590200 (alpha 0) in the HTML dump (the
  icon still draws blue - check which colour the display list takes).
- N3 the settings screen: "Back to notes" floats top-left over an empty strip, a stray grey bar at the top
  (own ShellSettingsLayout use, not appkit's settings page).
- N4 the title / tag fields are tiny bordered inputs (11px) - the note title should read as a title.
AzWriter (no E2E script exists):
- W1 the chrome is a hand-rolled palette (palette.rs): in light mode the title band stays dark (title
  unreadable), flora-light: the font combo text is white on white, the status bar text grey on blue, the
  canvas stays dark; not on DocumentShell / Titlebar (WindowDecorations::None + QuickAccessBar).
- W2 the page text ignores DOC_CSS: its rules select `body`, the content root is a `div` -> default serif
  at the default size; in dark mode the text inherits the window's light text colour on a light-grey sheet
  -> unreadable (flat-dark / flora-dark).
- W3 the zoom slider in flora-dark is a black box (status bar).
- W4 two undo stacks (DEDUP A3.6), B/I/U blind flip + gallery 2/4 both H1 (A3.5), Ctrl+B over a selection
  does nothing (A3.3) - the RTE adoption fixes these.
- W5 no appkit (args/settings/about/shortcuts), saves with std::fs::write from callbacks, no save-changes
  check on close, no `__azwriter_` prefixes, no E2E script.

## Decisions
- (to fill)
