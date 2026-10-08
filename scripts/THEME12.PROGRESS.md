# THEME12 - list widgets follow the app theme (2026-10-08)

The user: "the list view in AzDrive (and in general?) doesn't follow the theme".

## DONE
- RED 0c48b74d1: `list_view::theme_tests` (ground + ink, every colour from its theme, selection,
  stripes, header band / titles / sort arrow, contrast in flat/flora x light/dark via
  `widgets::theme_contrast::findings_under`).
- GREEN (list): `ListViewLook` (base + skins), `flat::list_view_look` (replaces the 60 LIST_*
  state consts), `flora::list_view_look`, `ListView::{set_theme, with_theme}` + `theme` field;
  follows / pinned / structure / invariants tests; KNOWN_HALF_PAIRS list_view entry gone.
  api.json: ListView field `theme: OptionUiTheme`, fns `set_theme`, `with_theme`.

## NEXT
- tree_view: move the flat skin + its colours into flat.rs (values unchanged).
- audit notes (cell_grid, data_table, icon_grid: no production colour literals).
- AzDrive notes for the report.

## Open questions
- none
