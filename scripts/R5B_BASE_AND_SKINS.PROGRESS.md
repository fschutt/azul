# R5B_BASE_AND_SKINS progress

Branch `wt/r5b-base-and-skins`, base `d240a1b1d`.
Widgets: combobox, date_picker, datetime_local, dialog, divider, drop_down,
file_input, form, frame, label, menubar.

## DONE
- c4fecd00d checkpoint.
- ace6a34ed RED: `a_<widget>_declares_its_structure_once_for_every_theme` in all
  11 widgets. Expected to fail: combobox (option rows: cursor, user-select),
  menubar (flat's sheet vs flora's inline). The rest pass already.
- 759b7f551 checkpoint.
- 1f2fc5e8f GREEN combobox + menubar (bases in the widget files).
- c105c964e GREEN refactor: divider, drop_down, dialog, form, datetime_local
  (one base each; both looks put their skin after it).
- Report `scripts/R5B_BASE_AND_SKINS_2026_09_29.md` (committed with this file).
- Resumed after the power loss: the 7 uncommitted files were complete; they are
  c105c964e.

## Audit (done)
- Already one source, unchanged: date_picker, frame, label, file_input (Button).

## IN PROGRESS
- (none)

## NEXT
- Parent: compile, run the commands in the report.
- Parent: `ComboBox::list_style_on` should use `theme_blocks::stack_parts`
  (pre-existing stacking of caller extras onto the merged panel; see report).

## Open questions
- dialog_skin is shared with modal and popover: only the dialog parts'
  structure (panel/title/close) was touched.
