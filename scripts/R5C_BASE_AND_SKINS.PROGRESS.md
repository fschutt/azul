# R5C_BASE_AND_SKINS - progress

Branch `wt/r5c-base-and-skins`, base `d240a1b1d`.
Widgets: number_input, pagination, popover, progressbar, quick_access, radio_group, ribbon,
segmented, slider, spinner, split_pane.

## DONE
- 9131a2c67 RED: one `..._declares_its_structure_once_for_every_theme` test per widget (a
  `base_and_skin_tests` module at the end of each file; ribbon's sits in `flora_tests` for the
  fixture). Expected red: pagination, segmented (cursor / user-select cross), split_pane
  (flora-only box-sizing).

- 0d05c508d GREEN pagination (`PAGINATION_BUTTON_BASE`), segmented (`SEGMENT_BASE`), split_pane
  (`divider_base`, `divider_thickness`; flat's divider gains `box-sizing: border-box`).

- (power loss after 0da3e5744; the radio_group edits survived uncommitted, were complete)
- 6cdb62d18 radio_group: `RADIO_GROUP_CIRCLE_BASE`, `RADIO_GROUP_DOT_BASE`; flat statics are skins.

## IN PROGRESS
- Dedup of structure both theme files spell out: popover panel, progressbar container + mount.
- Parent fact: `theme_blocks::stack_parts(base, extra)` exists in the main checkout (NOT in this
  base). Use it wherever parts are newly stacked AFTER a merge; do not define it; leave the
  ribbon / quick_access `merged_style` bodies alone. My bases are prepended inside each look
  before the merge, so no call is needed so far.

## NEXT
- Report `scripts/R5C_BASE_AND_SKINS_2026_09_29.md`.

## Open questions
- (none)
