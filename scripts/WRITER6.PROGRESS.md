# WRITER6 progress (wave 6, 2026-10-03)

Branch `wt/writer6` from `25d78e309`. Brief: scripts/waves/wave6/WRITER6.md.

## DONE (commits)
- (none yet)

## IN PROGRESS
- LOOK: run AzWriter / AzNotes headless (prebuilt aa59b2d84), screenshots under target/writer6-look/ (not committed).

## NEXT
1. LOOK AzWriter + AzNotes, write the broken list below.
2. Undo/Redo shortcut ownership (core/src/events.rs) - RED tests core + layout, then the design.
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
