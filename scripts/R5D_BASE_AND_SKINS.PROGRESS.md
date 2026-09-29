# R5D_BASE_AND_SKINS - progress checkpoint

Branch `wt/r5d-base-and-skins` from `d240a1b1d`. Widgets: statusbar, stepper, switch, tabs, text_area,
text_input, time_picker, titlebar, toast, tooltip, tree_view, video.

## DONE

- c19e2ef1d RED: one `structure_tests` test per widget (`a_<widget>_declares_its_structure_once_for_every_theme`).
- cfd4b8836 GREEN stepper: `CIRCLE_BASE` / `CONNECTOR_BASE` / `LABEL_BASE`; circle box-sizing unified to
  border-box; `circle_style_declares_the_same_property_set_for_both_states` 18 -> 19.
- d70ed0c15 GREEN tabs: `HEADER_BASE` / `AFTER_BASE` / `TAB_BASE` / `PANEL_BASE`; allowed header
  align-items and before-tabs flex-grow; 4 flat-const comparisons now compare with the flat look.
- 55902f260 RED text_input: `the_clear_button_shows_with_the_display_a_filled_field_builds_it_with`.
- 59a6d3ada GREEN text_input: `SEARCH_FIELD_BASE`, `search_clear_base`, `SEARCH_CLEAR_SHOWN` (flex, also
  the live show).
- 889b90705 GREEN time_picker: `CONTAINER_BASE` / `CLICKABLE_BASE` / `READOUT_BASE`; `flat::on_base`
  (end of flat.rs) replaces the tabs' `tab_part`.
- 92b516d0b GREEN toast: `TOAST_CARD_BASE` (incl. placement) / `TOAST_CLOSE_BASE`.

## IN PROGRESS

- GREEN time_picker / toast / tooltip / tree_view.

## NEXT

4. GREEN (already shared, flora restates): time_picker, toast, tooltip, tree_view bases.
5. Report `scripts/R5D_BASE_AND_SKINS_2026_09_29.md`.

## Audit (read from the builders)

- Already shared, no code change: statusbar (flora repaints flat geometry via `chrome_part`), switch (one
  look), text_area (both builders start from the widget's resolvers), titlebar (`container_style_painted`
  / `title_style_painted` are the widget's), video (no structure in the poster).

## Open questions

- none yet
