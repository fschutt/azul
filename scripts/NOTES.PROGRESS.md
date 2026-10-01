# NOTES progress (AzNotes)

Branch `wt/notes` from `39092feee`. Worktree
`/Users/fschutt/Development/azul/.claude/worktrees/agent-a15ab00b973aabdc4`. Report:
`scripts/NOTES_2026_10_01.md`. Nothing is compiled here (house rule); Rust files are
parse-checked with `rustfmt --edition 2021 --check`.

## DONE
- `149a2cf36` RED / `853792345` GREEN E1: `compute_counters` skipped counter-reset in any
  pseudo-state (a hovered `ol` renumbered its items) - unit test in `solver3/cache.rs`.
- `546d645eb` RED / `9d6a3e80d` GREEN E2: `MessageListMark` (Flag / Pin / None) on MessageList.
- `6271b43ca` azul-storage `time::iso8601` (+ test).
- examples/azul-notes/src/{doc,markdown,model}.rs: the flat block document with its edits,
  front matter + Markdown both ways, the library / queries / keys (unit tests inside).

- store.rs: the storage jobs (Load, Rescan, Save with move + version, Delete, PutText,
  History, Version, Import, Images, Seed) over `&dyn Drive`, tests on a LocalDrive in a temp
  folder; Note gained `generation` + `file_modified`.

- Cargo.toml, .cargo/config.toml, main.rs, args.rs (+ tests).

- sample.rs: 8 sample notes (fixed ids `sample_id(n)`), 2 pinned, + empty `Archive`.

- look.rs (colours per mode, text sizes); editor.rs PART 1 (rendering: host_dom, block_dom,
  run_dom, print_dom; mapping: host_node, path_in_host, block_of, caret, selection,
  focus_editor). editor.rs references (not yet written): `on_text_changed`,
  `on_document_edit`, `on_editor_key`, `on_check_click` (part 2) and `crate::AppState`.

- editor.rs PART 2: EditorState/Typing, sync_text, on_text_changed, on_document_edit (split /
  merge / ReplaceChildren incl. multi-block paste via Doc::replace_with), on_editor_key, on_check_click,
  toolbar commands toggle_format / toggle_kind / indent / insert_rule / set_link.
  REQUIRES in lib.rs: `pub struct AppState { pub library: Library, pub open: Option<String>,
  pub editor: editor::EditorState, .. }`, `AppState::open_note_mut(&mut self) -> Option<&mut Note>`,
  `AppState::edited(&mut self)`.

- lib.rs (AppState, Settings, start, data_root, with_state, refresh_if) `065d4440a`;
  jobs.rs (spawn, on_startup, autosave_tick, focus_editor_soon, on_window_focus,
  on_close_requested, save_note/save_all, request_images, open_note, new_note, apply outcomes)
  `252f4c0f0`.

- ui.rs parts a-d committed (`07993c66e`, `1d3326aad`, `5e29de7ca`, `c547695b3`): layout,
  title row, status bar, window keys, dropped files, navigation pane, note list, editor pane,
  exports. STILL MISSING in ui.rs (referenced): `open_link_sheet(&mut AppState)`,
  `show_history(&mut CallbackInfo, &RefAny, &mut AppState)`, `overlay_dom(s, app, look)`,
  `settings_screen(s, app, look)`, `history_screen(s, app, look)`.

- ui.rs parts e-g (`d7d0cb7ff` sheets + palette, `0c705f532` settings, `10d2b565f` View
  interactive + line_diff, `361490786` history). The crate parses from lib.rs (rustfmt).
- `27e956cdd` registration (Cargo.toml member, workspace_test_members, CI step).

- `4913910a7` scripts/aznotes_e2e.py + #open-settings button.

- `9c88a887c` review fixes (match guard, *level).
- E3 `ff496d1da` RED / `de4616f0a` GREEN: an acked split of a list item resumes past the new
  item's marker (restore_caret_from_resume_point -> caret_past_markers). Test
  layout/tests/an_acked_split_of_a_list_item_resumes_past_the_new_items_marker.rs (in all.rs).

- Report scripts/NOTES_2026_10_01.md written (keep its commit list current when adding work).

## IN PROGRESS (precise next steps, in order)
8. (optional, time permitting) more review passes / engine gaps; update the report's commit list.
   A careful compile-in-head review pass over every AzNotes file (types, borrows, imports).
9. Report scripts/NOTES_2026_10_01.md.
   (old plan follows, done up to 6) REQUIRED by lib.rs/jobs.rs: `pub extern "C" fn layout(RefAny,
   LayoutCallbackInfo) -> Dom`, `pub const SETTINGS_SHORTCUTS: usize`, `pub const SETTINGS_ABOUT:
   usize`, `pub fn show_history(&mut CallbackInfo, &RefAny, &mut AppState)`. Pieces: (a) layout +
   title row + status bar + window callbacks (keys Ctrl+N/K/S/,/Escape, focus, close);
   (b) navigation pane + its event; (c) note list (MessageList, Pin mark) + its events;
   (d) editor pane (title TextInput, tag chips + tag field, toolbar Buttons) + callbacks;
   (e) overlays (palette with commands, new notebook, link, confirm delete); (f) settings
   (ShellSettingsLayout: General, Editor, Storage, Keyboard shortcuts, About); (g) history
   screen (versions list, preview, restore); (h) exports (PDF via Pdf::from_dom_in_callback,
   Markdown via FileDialog::save_bytes), empty states.
6. registration (root Cargo.toml member, scripts/workspace_test_members.txt, CI step),
   scripts/aznotes_e2e.py, report scripts/NOTES_2026_10_01.md.

## NEXT
- registration (workspace, test members, CI), scripts/aznotes_e2e.py, the report.

## Decisions (made unattended, for the report)
- D1 The editor is app-local for this pass (`examples/azul-notes/src/editor.rs` +
  `doc.rs`), written so it can be promoted to `azul::widgets::RichTextEditor`. Reason: no
  compiler tonight, and the widget's api surface (blocks, runs, events, the sync glue) needs a
  design pass with the parent. AzWriter's `ir.rs` is the twin (named in the report).
- D2 The editor model is a FLAT block list (paragraph, heading, bullet/numbered/check item with
  an indent level, quote, code block, rule, image), one DOM child of the contenteditable host
  per block; list items are `li` (display: list-item, the engine's `::marker`) directly under
  the host; a numbered item's number is set with `counter-reset: list-item <n-1>` on the item;
  a check item's box is an absolutely positioned `contenteditable=false` island (out of the
  IFC, walled off from the text the engine edits), so the item's runs start at DOM child 1.
- D3 The note list is azul's `MessageList` with a new row MARK (`MessageListMark::Pin`): the
  same widget AzMail uses, so search row, sort header, sections, keyboard and virtualisation
  are not duplicated.
- D4 Typing: TextChanged -> sync the block text into the model (prefix/suffix diff keeps the
  runs), ack the revision, no rebuild. Enter / Backspace merges: DocumentEdit -> apply the
  split / merge to the model, ack, RefreshDom. Markdown shortcuts, toolbar block kinds and
  inline formats: change the model, ack, RefreshDom (the new generation's text wins:
  `gc_app_set_text` / the acked GC). Switching notes / restoring a version / an external
  edit: `reset_editor_content`.
- D5 Dates: `azul_storage::time` (one place for ISO 8601; a formatter is added there) for the
  front matter, chrono `Local` for the list's dates (as AzDrive / AzMail).
- D6 Empty notebooks persist as a marker object `notes/<notebook>/.notebook` (S3 has no
  folders).

## Open questions
- (none yet)
