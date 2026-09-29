# R5B_BASE_AND_SKINS progress

Branch `wt/r5b-base-and-skins`, base `d240a1b1d`.
Widgets: combobox, date_picker, datetime_local, dialog, divider, drop_down,
file_input, form, frame, label, menubar.

## DONE
- c4fecd00d checkpoint.
- ace6a34ed RED: `a_<widget>_declares_its_structure_once_for_every_theme` in all
  11 widgets. Expected to fail: combobox (option rows: cursor, user-select),
  menubar (flat's sheet vs flora's inline). The rest pass already.

## Audit (done)
- Structure already authored ONCE in the widget file, both looks extend it:
  date_picker (`DatePickerLook::established`), frame (`FRAME_*_STYLE`),
  label (`resolved_label_style`), file_input (it IS a Button:
  `build_button_container_style`). Nothing to move.
- Structure written in BOTH looks (alike, so the merge shares it, but
  twice): divider, drop_down, dialog (panel/title/close), form,
  datetime_local, combobox (field/list/option). Move to a base.
- menubar: flat's structure in its `with_css` sheet, flora's inline. Move to
  an inline base both use.

## IN PROGRESS
- GREEN: combobox, menubar.

## NEXT
- GREEN: divider, drop_down, dialog, form, datetime_local.
- Report `scripts/R5B_BASE_AND_SKINS_2026_09_29.md`.

## Open questions
- dialog_skin is shared with modal and popover (other agents' widgets?):
  touching only the dialog parts' structure (panel/title/close).
