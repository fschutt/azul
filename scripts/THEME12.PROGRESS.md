# THEME12 - list widgets follow the app theme (2026-10-08)

The user: "the list view in AzDrive (and in general?) doesn't follow the theme".

## DONE
- RED: `list_view::theme_tests` (ground + ink, every colour from its theme, selection, stripes,
  header band / titles / sort arrow, contrast in flat/flora x light/dark via
  `widgets::theme_contrast::findings_under`).

## IN PROGRESS
- GREEN: `ListViewLook` (base + skins), `flat::list_view_look`, `flora::list_view_look`,
  `ListView::{set_theme, with_theme}` + `theme` field.

## NEXT
- follows / structure / invariants tests for the list.
- tree_view: move the flat skin + its colours into flat.rs (values unchanged).
- audit notes (cell_grid, data_table, icon_grid: no production colour literals).
- AzDrive notes for the report.

## Open questions
- none
