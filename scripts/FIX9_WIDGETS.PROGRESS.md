# FIX9_WIDGETS progress (wave 9, PKG 4 WIDGETS, branch wt/fix9-widgets, base b454da215)

Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 4 WIDGETS" (4.1 - 4.18) + the 4 PKG 4 SUITE FAILURES.
No cargo. Report: scripts/FIX9_WIDGETS_2026_10_05.md.

## DONE
- 4.1 GREEN 4abc009bc (RED existed: 4afd055db, layout/tests/a_text_field_takes_the_font_size_its_app_gives_it.rs)
- 4.2 GREEN e71e2c9fb (RED existed: 4809ad3ed)
- 4.3 RED ae080e7fb, GREEN 5c990e71f
- 4.4 RED bc1697854, GREEN 4bb2e7aae
- 4.5 refactor f25d09b43 (button::styled_button; no RED)
- 4.6 RED 9ddfb18ec, GREEN 4ee11ed29
- 4.7 RED bad3f6692, GREEN 381a9c5fb
- 4.8 RED 09ee9a125, GREEN 2d58a6847 (api: ReferencePickerEventKind::Clear)
- 4.9 RED 0c8d84eec, GREEN 84f2a1ef3
- 4.10 refactor 1db1f4517 (no RED)
- 4.11 RED a809309aa, GREEN 0d13d1703
- 4.12 refactor b4b7b3350 (no RED)
- 4.13 RED a6a000f6c (RED by compile), GREEN ea917eadd
- 4.14 refactor ec5bfb751 (decl.rs appended - outside Files line, item names it)
- 4.15 refactor dbd708496 (no RED)
- 4.16 RED becbfa671, GREEN 82fad2e6b
- 4.17 SKIPPED (NOT SMALL): Button's disabled model needs a reason and keeps the Tab stop + answers a click
  with its reason; 2 of 3 row_button callers pass no reason; wizard_layout.rs's test pins "a held Next has no
  Click handler" (not a PKG 4 file); the box's held opacity would double the Button's. Decision for the report.
- 4.18 chore 3460fd689

## IN PROGRESS
- suite failures: rich_text_editor dom_lint, button autotest, code_view wheel, date_range_picker invariants

## NEXT
- 4.1 .. 4.18 in order, then the 4 suite failures, then the report.

## Open questions
(none yet)
