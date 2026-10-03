# OFFICE7 progress (wave 7, 2026-10-03)

Branch `wt/office7` from `2e55eef06` (the wave-6 integration; every app compiles). Brief:
`scripts/waves/wave7/OFFICE7.md`. Nothing compiled (house rules). Parse check: `/tmp/office7_parse.sh <files>`
(rustfmt --emit stdout; recreate it if /tmp was wiped).

## DONE
- 1. ids -> const AzString: 606addf10 (AzWriter), 5ae4b1a87 (AzNotes; editor's HOST_ID alias removed).
- 2. AzReview MouseMove: 1799a0c56 (RED: unit test on `ui::ink_layer` + E2E point count), 6c823ec53 (GREEN).
- 3. AzWriter: 21547860e (`Update.max_self` replaces `commands::merge` - needs api.json `Update.max_self`);
  1e0c1ea97 / 0c263b933 (appkit RED / GREEN: `files::outside_read`, `files::read_outside`,
  `ui::spawn_outside_read`; the file thread's drive is `without_manifest` outside the data tree);
  2c86643c0 / 59bdedafa (AzWriter RED / GREEN: `commands::imported`, `finish_import`, tag IMPORT).

- 4. AzNotes title: ce4e7d98f (RED E2E: title >= 28 px and taller than the tag field), 4ed56d37a (GREEN:
  font-size 22px + bold on the field). Engine REDs for others: 4afd055db (WIDGETS7: the value <p> pins
  11 px, `a_text_field_takes_the_font_size_its_app_gives_it`), f246c017c (LAYOUT7: a stretched flex
  container item loses min-height, `a_stretched_flex_container_keeps_its_min_height`; measured with
  `mount`: 12 px instead of 22).

- 5. AzMail on the Drive: 165b8506b RED (store tests), 1dd27a351 store (`DriveFolder` + `MailStore`),
  db92bd2df account.rs, ab714f5a6 send.rs (+ testutil `TempDir::folder`), d1d40d3fd sync.rs, 0c83c2ccc
  compose.rs, 77e58380d sample.rs, bb4307a44 lib.rs (MailApp.root: DriveFolder, start() places it with
  `DriveFolder::of(root_path, data_root)`), 92631d148 UI + azmail-send + `write_atomic` deleted, fdcc5397a
  attachments via `read_outside`, bec3cdff3 CSS-zoom TODO(LAYOUT7), 7ca925758 move_prefix fix.

## IN PROGRESS
- 6. AzShow:
  - DONE presenter monitor: ea1ab7b60 (RED `PresenterMonitor` resolve/parse), a1a67ea16 (GREEN: SLIDE SHOW >
    Monitors DropDown, settings `presenter_monitor`, `window_state.monitor_id` in start_show).
  - DONE text boxes on the shared RTE: cd3160a3e (text.rs on RichTextDoc, Editor.text, render via RTE
    read-only / editable, views::on_text_change, commands sync/format via RTE, ribbon state, ir.rs deleted).
  - NEXT: rail drop indicator (ThumbnailStrip widget = WIDGETS7's file; minimal edit planned: DragOver marks
    the hovered item's side, DragLeave / Drop clear) ; tables edited in place.

## NEXT
- 7. AzSheets: Replace in the grid's edit, pickers, Format Cells = one undo step.
- 8. LOOK at each app.
- Report `scripts/OFFICE7_2026_10_03.md`.

## Decisions
- WIDGETS7 heads-up (coordinator): MessageList -> SummaryList is applied by the parent AFTER merging this
  branch; keep using the MessageList names here (I do not touch them).
- AzShow text on the shared RTE: RTE got `line_height` + proportional indents (d02ac85f5 RED, fa1601fe3 GREEN;
  minimal edit of WIDGETS7's rich_text_editor.rs, api.json field + 2 methods in the report).
- Item 1 is a refactor (no behaviour change): no RED commit.
- Item 3: the import is read on appkit's file thread (Drive pattern, `without_manifest` at the file's
  folder); the docx / Markdown parse runs on the UI thread when the answer arrives (as an Open does).
- Item 5: AzMail's durable (fsync) flag dropped (LocalDrive writes whole files; a torn index/state reads as
  none and resyncs). `azul_storage::ops::list_all` used; appkit `files::list_all` is its twin (report).
- Item 4: no app workaround for the two engine bugs (no `align-items: center` on the title row).

## LOOK tools (not committed)
- `/tmp/office7_look.py` (four theme/mode shots + layouts), `/tmp/office7_mount.py` (mount HTML cases);
  run through run_capped.sh. AzNotes LOOK shots: /tmp/office7_look/notes/.

## Open questions
- (none)
