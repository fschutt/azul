# R2-WIDGETS progress (wave 9 round 2, branch wt/r2-widgets, base 440991077)

Brief: scripts/waves/wave9/ROUND2.md "PKG R2-WIDGETS" + USER DECISIONS D1, D3.

## DONE
- W1 date picker marks: RED 693dfcf02, GREEN e98c5a2c9 (`marked` moved to date_picker.rs, pub(crate)).
- W2 dom_lint out-of-flow sibling: RED ea1f7bbaf, GREEN 772285909.
- W3 DEDUP decl::classes (gauge class_list, date_range_picker one_class): c5b2e0c50.
- W4 DEDUP us_char / typed_letter: 23c0d27a5.
- W5 DEDUP cell_grid typed -> data_table::insert_at: 771e5aab1.
- W6 IconGrid item DOM id: RED 95cd51753, GREEN 1d13bd7b4.

## IN PROGRESS
- D1 dialog buttons: always a reason, Button's own disabled state, reason on hover + focus.

## NEXT
- D1: read button.rs disabled model, dialog_kit row_button, standard_dialogs, settings Apply, wizard Next.
- D3 IconGrid extras (extra text lines + placeholder colour), report scripts/R2_WIDGETS_2026_10_05.md.

## Open questions
(none)
