# R2-WIDGETS progress (wave 9 round 2, branch wt/r2-widgets, base 440991077)

Brief: scripts/waves/wave9/ROUND2.md "PKG R2-WIDGETS" + USER DECISIONS D1, D3.

## DONE
- W1 date picker marks: RED 693dfcf02, GREEN e98c5a2c9 (`marked` moved to date_picker.rs, pub(crate)).
- W2 dom_lint out-of-flow sibling: RED ea1f7bbaf, GREEN 772285909.
- W3 DEDUP decl::classes (gauge class_list, date_range_picker one_class): c5b2e0c50.
- W4 DEDUP us_char / typed_letter: 23c0d27a5.
- W5 DEDUP cell_grid typed -> data_table::insert_at: 771e5aab1.
- W6 IconGrid item DOM id: RED 95cd51753, GREEN 1d13bd7b4.
- D1 button keyboard focus: RED 3f7db13c9, GREEN 8bac59f23.
- D1 dialog buttons (RowAction, reasons, held skin gone): RED d974ce4aa, GREEN 7afac3f1e
  (settings_dialog.rs touched minimally: outside the package's files).

## IN PROGRESS
- D3 IconGrid optional extras: extra text lines + placeholder colour.

## NEXT
- D3: RED pin "an item that sets none renders exactly as today" + RED for the extras, then GREEN in icon_grid.rs
  (new fields LAST in IconGridItem, new fns), theme skins appended at the END of flat.rs / flora.rs if needed.
- report scripts/R2_WIDGETS_2026_10_05.md.

## Open questions
(none)
