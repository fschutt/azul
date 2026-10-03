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

## IN PROGRESS
- 4. AzNotes title field.

## NEXT
- 5. AzMail mail files on the Drive (one Drive, per-account scope).
- 6. AzShow: rail drop indicator, tables in place, presenter monitor; text boxes on RichTextDoc (delete ir.rs).
- 7. AzSheets: Replace in the grid's edit, pickers, Format Cells = one undo step.
- 8. LOOK at each app.
- Report `scripts/OFFICE7_2026_10_03.md`.

## Decisions
- Item 1 is a refactor (no behaviour change): no RED commit.

## Open questions
- (none)
