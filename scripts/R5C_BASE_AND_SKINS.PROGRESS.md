# R5C_BASE_AND_SKINS - progress

Branch `wt/r5c-base-and-skins`, base `d240a1b1d`.
Widgets: number_input, pagination, popover, progressbar, quick_access, radio_group, ribbon,
segmented, slider, spinner, split_pane.

## DONE
- 9131a2c67 RED: one `..._declares_its_structure_once_for_every_theme` test per widget (a
  `base_and_skin_tests` module at the end of each file; ribbon's sits in `flora_tests` for the
  fixture). Expected red: pagination, segmented (cursor / user-select cross), split_pane
  (flora-only box-sizing).

## IN PROGRESS
- GREEN pagination + segmented.

## NEXT
- GREEN per widget: pagination + segmented (cursor / user-select cross), split_pane (flora's
  box-sizing), then the dedup of structure authored in both theme files (radio_group circle/dot,
  popover panel, progressbar container + mount).
- Report `scripts/R5C_BASE_AND_SKINS_2026_09_29.md`.

## Open questions
- (none)
