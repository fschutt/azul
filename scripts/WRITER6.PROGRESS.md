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

- 8d1e858e3 RED paged editor tests, ff83a66ff GREEN RichTextEditor::page_doms(page_starts: U32Vec,
  first_page, page_count) -> DomVec (one host per page `<host>-page-<first>`, ONE shared EditorData),
  mapping via host_of/model_path (walks up to the nearest host of this editor).

- 880dc5984 AzNotes close via prevent_window_close (B29/A16; INFRA6 says all backends honour it now)
- 064112be6 RED + b5b620b5b GREEN AzNotes search/tags on azul_pim (B16/B27/E5)

- AzWriter rewrite: 1c8ae86c9 model+ids, 7155c5172 docx, f05dd0197 storage, 89b3db84e paginate,
  20e89e3a3 (RTE page host no padding), 08fb5fe1e app, 56e02440d ribbon, 780f4029d commands,
  f6090e8bb pages, e3ddbd2ac backstage, fc31910fa THE SWITCH (lib.rs, Cargo.toml, old modules deleted).
- SMALL6 engine bug (copy/cut/paste/select-all/ctrl+d swallowed on a non-editable focus): 2a56d8c49 RED,
  61583aeac GREEN (handle_key_down gets has_selection; claims only on editable focus or a selection),
  f203c4cbe RED + 2d76958f5 GREEN for Ctrl+D.

- 08f6e5586 AzWriter Cow fix, 9e5086d05 scripts/azwriter_e2e.py, 78bd32e87 AzNotes `__aznotes_` ids.

- ef9a62f7a AzNotes on azul-appkit (args/data root/settings page/About/shortcuts; E2E --data-dir).

## IN PROGRESS
- NEXT: the report scripts/WRITER6_2026_10_03.md (api.json list, least-sure spots, test commands, left).
- Scratch helpers (api.json lookup script) live in the session scratchpad; it was wiped by the restart.
- USER asked "don't we already have pagination?": yes - paginate.rs does NOT re-implement it. It calls the
  engine's Pdf::compute_pagination (PaginationSnapshot::break_path) and only maps each break path to the
  page's first block, on a Thread; it replaces the old document.rs glue (split_content_at / DomSplit /
  memo) that did the same with more code. query_pagination gives Y positions only (no paths), so
  compute_pagination is the one that fits page_doms.

## Undo/Redo ownership - DESIGN (done): the browser keydown model. core `handle_key_down` stops
  claiming primary+Z / Shift+Z / Y for the PRIMARY seat (no AddAndSkip: the KeyDown passes to callbacks);
  the engine's text undo becomes the key's DEFAULT ACTION (`DefaultAction::UndoTextEdit { target }` /
  `RedoTextEdit { target }`, appended to core's DefaultAction) decided in layout/src/default_actions.rs
  for an editable focus, run by the dll (event.rs DefaultAction match -> apply_system_change(UndoTextEdit))
  after the callbacks, VETOED by prevent_default - like Ctrl+B ToggleTextFormat. The RichTextEditor
  handles Ctrl+Z in its VirtualKeyDown and calls prevent_default. Non-primary seats keep SeatShortcut.
  macOS Edit-menu `undo:` path (macos/mod.rs edit_command) still applies UndoTextEdit directly (report).

## Seen broken (LOOK, prebuilt aa59b2d84, screenshots in target/writer6-look/{notes,notes2,writer})
AzNotes (scripts/aznotes_e2e.py PASSES against the prebuilt binary):
- N1 a check item renders TWICE: a phantom line "* * call the bakery" (two list markers) above the real
  check line; the DOM is right (4 blocks, the check `li` = list-style none, position relative, the box an
  abspos island after the runs). Fresh load (restart) shows the same -> layout, not the edit glue.
  ROOT (layout tree dump, prebuilt): the check `li` (dom 268) is a Block FC whose children are its ::marker
  (dom 268 again, Inline - generated although list-style-type is none), an ANONYMOUS InlineWrapper with the
  text, and the abspos island (dom 270) as an in-flow "Inline" child. The out-of-flow island made the tree
  builder wrap the inline content in an anonymous block (CSS 2.2 9.2.1.1: only IN-FLOW block children do),
  and the marker / text paint on a line of their own above the item. Owner: MAILENG6 (solver3 layout_tree);
  RED suggestion in the report.
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
- Undo ownership = browser keydown model (see DONE design notes). macOS Edit-menu path left (report).
- RichTypingStyle stays (api.json field; inline code; pressed state); B/I/U/S of new text from the engine.
- Prefix names: the generated Rust API (link-dynamic, target/codegen/dll_api_external.rs) has NO const
  `AzString::from_const_str`, so app names are `pub const X: &str = "__azapp_..."` in an `ids` module
  (one definition each; `.with_id(ids::X)` via From<&str>). Report: codegen should emit the const fn
  (doc/src/codegen/v2/lang_rust.rs AzString impl block), then the consts become AzString.
- AzWriter rewrite (examples/azul-writer), files:
  model.rs (DocumentModel: id, RichTextEditorState, dirty, saved text; Markdown via RichTextDoc;
  title = first heading / first line), docx.rs (wire JSON -> RichTextDoc; was ir.rs), paginate.rs
  (read-only RTE content_dom at A4 content width -> Pdf::compute_pagination -> page_starts = first path
  component of each break; memo per generation; Thread worker), storage.rs (keys `writer/<uuid>.md`,
  exports `writer/exports/<name>.pdf`, appkit FileJob on a Thread), ribbon.rs (Command enum +
  CommandRef/on_command; HOME/INSERT/VIEW only, fake tabs dropped; gallery = Normal/H1/H2/H3/Quote/Code
  each unique; B/I/U/S pressed from is_current_format; align from the caret block), pages.rs (canvas
  VirtualView: page_doms per visible page in a sheet div; status bar), backstage.rs (Info/New/Open/
  Save/Export/Close), lib.rs (AppState, run(), layout on DocumentShell+OfficeShell+Titlebar+
  ShellThemeScope, CloseGuard, appkit kit/settings/about/shortcuts). Deleted: ir.rs, palette.rs,
  fonts.rs, args.rs (appkit), document.rs, editor_ui.rs, ribbon_ui.rs, backstage_ui.rs.
  Paper follows the mode (sheet = system window background, ink = system text) like the RTE's
  content colours; the PDF export is the document on white.
  Dropped dev flags: --paginate-twice, --dump-xml; --frame-log -> env AZWRITER_FRAME_LOG.
  Paragraph split across pages: a page holds whole blocks (break path's first component); a block taller
  than a page overflows its sheet (report).
