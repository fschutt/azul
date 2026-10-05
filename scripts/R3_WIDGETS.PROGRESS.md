# R3-WIDGETS progress (wave 9 round 3) - branch wt/r3-widgets, base 6a39b7f1a

## DONE
- d29767be2 the_macos_titlebar_lines_up_with_its_traffic_lights (3) + azul_widgets_demo_follows_the_theme (2):
  TEST wrong (the demo uses the Titlebar widget since e22863a7a); page_frame rebuilt, demo-bar tests macOS-only.
- 2e7f99692 a_text_field_takes_the_font_size_its_app_gives_it: CODE wrong; 11 px default moved to a UA-priority
  component sheet (inline beat the app's Dom::with_css).
- 659cdc482 a_rich_text_editor_keeps_one_model_and_one_history (pages): TEST wrong (never focused a page).
- 48b260343 a_rich_text_editor_sets_its_line_height_and_scales_its_indents_with_its_text: TEST wrong (orphan
  Bullet(1) normalized to Bullet(0)).

## NEXT
- a_short_list_in_a_shell_pane_fills_its_pane_from_the_top
- a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it
- widget_lint_manifest_is_exhaustive
- final report scripts/R3_WIDGETS_2026_10_05.md

## STOPPED (big change, see report)
- an_inline_date_picker_fits_its_pane: CODE wrong in the ENGINE (taffy_bridge compute_non_flex_layout answers a
  flex item's ContentSize min-content query with its own `width` - own_definite_width injection + intrinsic
  min_content_width clamped to `width` in sizing.rs - so a `width: 32px` item's automatic minimum size is 32,
  never shrinks; Chrome: min(32, content min-content) = 15, cells shrink to 26). Widget is right per Chrome.
