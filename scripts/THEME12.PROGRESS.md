# THEME12 - list widgets follow the app theme (2026-10-08)

The user: "the list view in AzDrive (and in general?) doesn't follow the theme".

## DONE
- RED 0c48b74d1: `list_view::theme_tests` (ground + ink, every colour from its theme, selection,
  stripes, header band / titles / sort arrow, contrast in flat/flora x light/dark via
  `widgets::theme_contrast::findings_under`).
- GREEN 7c2501245: `ListViewLook` (base + skins), `flat::list_view_look` (replaces the 60 LIST_*
  state consts), `flora::list_view_look`, `ListView::{set_theme, with_theme}` + `theme` field;
  follows / pinned / structure / invariants tests; KNOWN_HALF_PAIRS list_view entry gone.
- RED 978a67313: `tree_view::theme_tests::every_colour_the_flat_tree_paints_is_one_of_flats`;
  shared `theme_checks::{colours_of, theme_colours, foreign_colours}` (the list test reads it).
- GREEN 4d2363729: flat's tree skins (`TREE_..._STYLE` statics) + colours moved into flat.rs on
  the flat palette's tokens (field #FCFCFC -> PG, chevrons -> ICON, counts -> ACC / SOFT, night
  ink -> DARK_INK); tree_view.rs keeps only its bases.
- perf 4f1302a62: the list's four row styles stacked once per build.
- Audit: data_table / icon_grid / cell_grid / summary_list / token_input carry no production colour
  literals (cell_grid's are the app's cell styles + `auto_ink`; icon_grid's are test fixtures).
  Left for their owners: combobox.rs (WHITE / #ACACAC / #333 flat skin in the widget file),
  thumbnail_strip.rs:922 (drop line in the system accent, #0078D7 fallback).

## api.json (autofix)
- ListView: field `theme: OptionUiTheme` (last); fns `set_theme(&mut self, theme: UiTheme)`,
  `with_theme(self, theme: UiTheme) -> ListView` (as TreeView's).

## NEXT
- report to the lead (AzDrive notes: look.rs hand-copies, no list widget used).

## Open questions
- none
