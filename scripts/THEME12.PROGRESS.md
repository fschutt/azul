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

- GREEN 7c2501245 (list) as above.
- RED 978a67313: `tree_view::theme_tests::every_colour_the_flat_tree_paints_is_one_of_flats`;
  shared `theme_checks::{colours_of, theme_colours, foreign_colours}` (the list test reads it).
- GREEN (tree): flat's tree skins (`TREE_*_STYLE` statics) + colours moved into flat.rs, on
  the flat palette's tokens (field #FCFCFC -> PG, chevrons -> ICON, counts -> ACC / SOFT,
  night ink -> DARK_INK); tree_view.rs keeps only its bases.

## NEXT
- audit notes (cell_grid, data_table, icon_grid: no production colour literals).
- AzDrive notes for the report.

## Open questions
- none
