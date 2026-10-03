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
- e2c27bddf GREEN tooltip: `TIP_BASE` (placement, nowrap, opacity 0); 7 tests read the base / flat tip.
- daa25f33a GREEN tree_view: `TREE_CONTAINER_BASE` / `ROW_BASE` / `CHILDREN_BASE` / `ICON_BASE` /
  `LABEL_BASE`; `style_is` checks read base + static.

- ad84d2163 report `scripts/R5D_BASE_AND_SKINS_2026_09_29.md`.

## IN PROGRESS

- nothing - R5-D is done.

## NEXT

- The parent compiles and runs the commands in the report.

## Coordinator facts (after the power cut)

- The cascade ranks a live `@theme` block above unthemed declarations, whatever the order: a later
  shared declaration loses to an earlier themed one when parts are STACKED after the merge. The parent
  added `theme_blocks::stack_parts` in the main checkout (not in this base). R5-D stacks no merged parts:
  every base + skin composition happens inside one theme's builder, before `follow_props` /
  `follow_dom`, on plain declarations. statusbar's `merged_style` is left as it is.

## Audit (read from the builders)

- Already shared, no code change: statusbar (flora repaints flat geometry via `chrome_part`), switch (one
  look), text_area (both builders start from the widget's resolvers), titlebar (`container_style_painted`
  / `title_style_painted` are the widget's), video (no structure in the poster).

## Open questions

- none yet
