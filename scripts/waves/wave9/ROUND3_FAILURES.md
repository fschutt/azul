# Round 3: the failing tests after wave 9 + FIX9 + round 2 (suite run on 3db9fce91, 2026-10-05)

Each: find whether the TEST or the CODE is wrong (Chrome is the layout reference; a pin may have moved on purpose -
FIX9-LAYOUT 1.8 sub/super, SYSUI8's wider system-ui text); fix that side; say which. You may NOT run cargo -
read the test, the code, and the message below.

## layout --test all (38)
```
a_border_defaults_to_a_medium_width_in_the_text_colour::a_border_style_alone_draws_a_medium_border :: assertion `left == right` failed: the initial border-top-width is medium (3px)   left: 0  right: 3 
a_face_fade_frame_is_patched_in_place::tweens_that_restyle_share_one_cascade_refresh_per_frame :: assertion `left == right` failed: two tweens that need the cascade in one frame must share ONE refresh of it (each refresh rebuilds the compact cache of the whole DOM), the frame ran 0 epoch bumps   left: 0  right: 1 
a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it::a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it :: harness: twice the content makes a shorter thumb (88 vs 88) 
a_line_height_in_rem_or_viewport_units_is_the_pitch_on_screen::a_line_height_in_vh_follows_a_resized_window :: line-height: 5vh after a resize to 800 x 800: the lines are 30px apart, not 40px: [21.0, 51.0, 81.0, 111.0, 141.0, 171.0, 201.0, 231.0, 261.0, 291.0] 
a_one_box_slide_does_not_re_lay_out_the_page::text_after_a_block_is_carried_over_by_the_next_layout :: assertion `left == right` failed: relayout 1 of an unchanged page found 7 nodes dirty   left: 7  right: 0 
a_one_box_slide_does_not_re_lay_out_the_page::a_knob_frame_costs_the_same_on_a_page_twice_as_long :: harness: each frame ran a layout pass, FrameCost { layout_passes: 0, text_flows: 0, flex_items_laid_out: 0, formatting_contexts: 0 } / FrameCost { layout_passes: 0, text_flows: 0, flex_items_laid_out: 0, formatting_contexts: 0 } 
a_percentage_height_inline_block_in_an_auto_height_body_is_as_tall_as_its_content::a_clipped_preheader_does_not_make_the_paper_taller :: the paper's 100% computes to auto: as tall as its content, 400px (Chrome 400), not the estimate that counted the clipped text: 300x414 @ (0, 14) 
a_percentage_height_inline_block_in_an_auto_height_body_is_as_tall_as_its_content::a_wrapping_table_makes_the_paper_as_tall_as_the_table :: and so is the paper (Chrome 114): p 300x118 @ (0, 0), t 300x114 @ (0, 0) 
a_rich_text_editor_keeps_one_model_and_one_history::the_pages_of_one_editor_share_one_document :: the editor reported a change 
a_rich_text_editor_sets_its_line_height_and_scales_its_indents_with_its_text::a_rich_text_editors_list_indent_scales_with_its_text :: a level-1 item at 14 px is indented 26 px, not 50 
a_short_list_in_a_shell_pane_fills_its_pane_from_the_top::a_short_list_in_a_shell_pane_fills_its_pane_from_the_top :: the row grows to the pane's bottom (720): it ends at 383 
a_text_field_takes_the_font_size_its_app_gives_it::a_text_field_takes_the_font_size_its_app_gives_it :: a field styled font-size: 24px is 22 px tall (an 11 px field is 22 px): its value line kept the widget's 11 px 
an_absolutely_positioned_child_does_not_split_its_parents_line::a_list_items_text_is_painted_once :: assertion `left == right` failed: "Item" and "block" are painted once each (4 + 5 glyphs), not "Item" again on a marker line   left: 11  right: 9 
an_absolutely_positioned_child_does_not_split_its_parents_line::a_list_item_with_list_style_type_none_has_no_marker_box :: assertion `left == right` failed: list-style-type: none leaves the marker without content: no ::marker box (CSS Lists 3 s3.1)   left: 1  right: 0 
an_animation_frame_sends_assistive_technology_only_what_moved::every_published_patch_leaves_the_screen_reader_holding_the_fresh_tree :: five cards added: node #1 is Some(Node { role: Group, actions: [ScrollIntoView], children: [#2, #7, #12, #17, #22, #27, #32, #37, #42], html_tag: "body", bounds: Rect { x0: 0.0, y0: 0.0, x1: 800.0, y1: 600.0 } }) on the screen reader's side, a fresh build makes Node { role: Group, actions: [ScrollDown, ScrollUp, ScrollIntoView, SetScrollOffset], clips_children: true, children: [#2, #7, #12, #17, #
an_hr_is_a_two_pixel_inset_rule_as_wide_as_its_block::an_authors_single_border_keeps_the_rule_one_pixel :: the author's border wins (Chrome 400 x 1): 400x4 @ (0, 8) 
an_inline_block_sits_on_its_last_lines_baseline::an_inline_block_whose_text_is_in_a_block_shares_the_lines_baseline :: the red inline-block is painted 
an_inline_box_paints_its_border_padding_and_margin::a_float_shrinks_to_its_content_with_the_inline_boxes_padding :: the float is 40px wider with the span's 40px padding; 12px -> 12px 
an_inline_date_picker_fits_its_pane::an_inline_date_picker_shrinks_to_a_pane_narrower_than_its_calendar :: the seven columns fit inside the calendar's padding and border: 224 of 200 ([32.0, 32.0, 32.0, 32.0, 32.0, 32.0, 32.0]) 
azul_widgets_demo_follows_the_theme::the_demo_paints_from_the_system_palette_directly :: premise: the titlebar's label 
azul_widgets_demo_follows_the_theme::the_page_and_titlebar_titles_are_legible_in_both_themes :: premise: the titlebar's label 
css_zoom_scales_the_lengths_of_its_subtree::em_lengths_follow_the_zoomed_font_size :: 1em is 20px at zoom 2 (Chrome 140 x 60): 132x52 @ (0, 0) 
struct_sizes::inline_pipeline_struct_sizes_are_pinned :: assertion `left == right` failed: size_of::<StyleProperties>() changed 248 -> 256. Shared behind an Arc, so one per distinct style - NOT per glyph.   left: 256  right: 248 
struct_sizes::layout_tree_node_struct_sizes_are_pinned :: assertion `left == right` failed: size_of::<LayoutNodeCold>() changed 288 -> 296. Per layout node, rarely touched. GREW 280 -> 288 (2026-08-22): NodeDataFingerprint gained `dataset_hash` so a dataset's allocation is no longer a LAYOUT change (the TextArea-over-Slider fix).   left: 296  right: 288 
svg_paint::a_filled_path_paints :: assertion `left == right` failed: and it is painted in the colour `fill` asked for   left: [193, 193, 193, 255]  right: [255, 0, 0, 255] 
svg_paint::fill_and_stroke_are_independent_paints :: the fill must paint 
svg_paint::a_stroked_path_paints_its_outline :: a 16x4 stroke should cover about 64 px, got 16 
svg_paint::stroke_comes_through_css_as_well_as_the_attribute :: a stylesheet stroke must paint, got 16 
svg_paint::fill_none_paints_nothing :: assertion `left == right` failed: fill="none" must not paint   left: 64  right: 0 
text3_baseline_exact::vertical_align_sub_lowers_glyph :: sub lower = line_ascent(16) * 0.3: expected 4.8000px, got 4.2000px 
text3_baseline_exact::vertical_align_super_raises_glyph :: super raise = line_ascent(16) * 0.4: expected 6.4000px, got 6.3333px 
text3_regression_metrics::vertical_align_sub_lowers_cluster :: assert_px failed: expected 4.8000px, got 1.0000px (|delta| 3.8000px > 0.05px) 
text3_regression_metrics::vertical_align_super_raises_cluster :: assert_px failed: expected -6.4000px, got 0.0000px (|delta| 6.4000px > 0.05px) 
the_incremental_raster_paints_a_transformed_box_where_the_compositor_does::a_turned_box_is_repainted_turned :: harness: the compositor paints the box turned 
the_macos_titlebar_lines_up_with_its_traffic_lights::the_demo_title_is_centred_on_the_window :: premise: the titlebar's label 
the_macos_titlebar_lines_up_with_its_traffic_lights::the_demo_title_sits_on_the_traffic_lights_line :: premise: the titlebar's label 
the_macos_titlebar_lines_up_with_its_traffic_lights::the_demo_titlebar_is_28px_tall :: premise: the titlebar's label 
widget_lint_manifest_is_exhaustive::every_widget_module_is_registered_in_the_lint_manifest :: widget module(s) missing from `every_widget_dom()` in layout/src/widgets/mod.rs: ["list_selection", "close_guard", "rich_text"] The label-convention walk and the dom_lint warning check both iterate that manifest, so an unregistered widget is not merely untested — it is INVISIBLE to both, and they stay green while its text nodes go unwatched. Add a `("name", Widget::create(..).dom())` entry, or, if
result: :: 
```

## layout --lib (4)
```
solver3::display_list::svg_mask_memo_tests::a_rebuilt_display_list_reuses_its_unchanged_svg_masks :: thread 'solver3::display_list::svg_mask_memo_tests::a_rebuilt_display_list_reuses_its_unchanged_svg_masks' (1317131) panicked at layout/src/solver3/display_list.rs:16119:9: the filled path paints through its clip mask 
solver3::sizing::autotest_generated::block_intrinsic_sizes_sanitize_nan_on_the_cross_axis :: thread 'solver3::sizing::autotest_generated::block_intrinsic_sizes_sanitize_nan_on_the_cross_axis' (1317873) panicked at layout/src/solver3/sizing.rs:4283:9: assertion failed: r.min_content_height.is_nan() 
window::autotest_generated::text_sized_in_viewport_units_keeps_its_own_font_after_a_resize :: thread 'window::autotest_generated::text_sized_in_viewport_units_keeps_its_own_font_after_a_resize' (1323039) panicked at layout/src/window.rs:28333:9: 5vw of a 400px window is 20px: the resize collects the text's chain at that size (collected: [40]) 
window::window_theme_context::a_changed_inline_flex_box_behind_a_block_sibling_keeps_its_slot_and_widens :: thread 'window::window_theme_context::a_changed_inline_flex_box_behind_a_block_sibling_keeps_its_slot_and_widens' (1323121) panicked at layout/src/window.rs:32422:9: assertion `left == right` failed: one line, as before   left: 76.0 
result: :: 
```

## Packages (one agent each; touch only the files your tests' root causes need, and say in the report which files you
## touched - another package may touch the same engine file; keep edits minimal and local)
- R3-PAINT: svg_paint (5), the_incremental_raster_paints_a_transformed_box_where_the_compositor_does,
  svg_mask_memo_tests (lib), a_border_defaults_to_a_medium_width_in_the_text_colour,
  an_hr_is_a_two_pixel_inset_rule_as_wide_as_its_block. (Suspects: FIX9-PAINT 2.4-2.6 SVG renderer / premultiplied
  label / mask cache, PDF9's SVG renderer changes, PAINT7.)
- R3-TEXT: text3_baseline_exact + text3_regression_metrics sub/super (4 - FIX9-LAYOUT 1.8 says 6.3333 / 4.2 and
  0.0 / 1.0; VERIFY against Chrome's rule before changing a pin - a super raise of 0 looks wrong),
  an_inline_block_sits_on_its_last_lines_baseline, an_inline_box_paints_its_border_padding_and_margin,
  an_absolutely_positioned_child_does_not_split_its_parents_line (2), css_zoom_scales_the_lengths_of_its_subtree,
  a_line_height_in_rem_or_viewport_units_is_the_pitch_on_screen, text_sized_in_viewport_units_keeps_its_own_font_after_
  a_resize (lib), block_intrinsic_sizes_sanitize_nan_on_the_cross_axis (lib),
  a_percentage_height_inline_block_in_an_auto_height_body_is_as_tall_as_its_content (2),
  a_changed_inline_flex_box_behind_a_block_sibling_keeps_its_slot_and_widens (lib; FIX9-PAINT's report has a bisect
  recipe - suspects 39f090082, b8a5c8cb1, cea6b0840).
- R3-WIDGETS: the_macos_titlebar_lines_up_with_its_traffic_lights (3) + azul_widgets_demo_follows_the_theme (2) ("premise:
  the titlebar's label" - the test cannot find the label any more), a_text_field_takes_the_font_size_its_app_gives_it
  (FIX9 4.1's RED did not turn green), an_inline_date_picker_fits_its_pane, a_rich_text_editor_keeps_one_model_and_one_
  history, a_rich_text_editor_sets_its_line_height_and_scales_its_indents_with_its_text,
  a_short_list_in_a_shell_pane_fills_its_pane_from_the_top, a_grown_scroll_box_paints_its_thumb_from_the_layout_that_
  grew_it, widget_lint_manifest_is_exhaustive (register list_selection / close_guard / rich_text or exempt them with
  a reason).
- R3-FRAME: a_one_box_slide_does_not_re_lay_out_the_page (2), a_face_fade_frame_is_patched_in_place,
  an_animation_frame_sends_assistive_technology_only_what_moved, struct_sizes (2: StyleProperties 248 -> 256 is
  FIX9-LAYOUT 1.8's strut_font_size; LayoutNodeCold 288 -> 296: find the field; update the pins with the reason or
  shrink). (Suspects: ANIMFRAME8 / A11YPATCH8 / FIX9-PAINT 2.8 / 2.11 / 2.13 / FIX9-LAYOUT 1.6 / 1.9 and today's
  LayoutCacheMap::mark_dirty change a253ffd49.)

## App lib tests (10 failing, cargo test --lib over the 35 app crates on 6a39b7f1a)
```
[azkeys] sample::tests::the_sample_vault_has_the_plans_items_of_every_kind :: thread 'sample::tests::the_sample_vault_has_the_plans_items_of_every_kind' (1447602) panicked at examples/azul-keys/src/sample.rs:274:9: 55 
[azmusic] playlists::tests::a_playlist_is_one_file_named_by_its_id_and_round_trips :: thread 'playlists::tests::a_playlist_is_one_file_named_by_its_id_and_round_trips' (1448115) panicked at examples/azul-music/src/playlists.rs:94:9: assertion failed: Playlist::from_json("[]").is_err() 
[aznews] feed::tests::an_email_author_is_shown_by_its_name :: thread 'feed::tests::an_email_author_is_shown_by_its_name' (1448148) panicked at examples/azul-news/src/feed.rs:1363:30: index out of bounds: the len is 0 but the index is 0 
[aznews] store::tests::a_broken_file_is_reported_and_the_rest_loads :: thread 'store::tests::a_broken_file_is_reported_and_the_rest_loads' (1448197) panicked at examples/azul-news/src/store.rs:357:9: assertion failed: loaded.library.feeds[0].items.is_empty() 
[aznews] store::tests::what_is_written_loads_back_the_same :: thread 'store::tests::what_is_written_loads_back_the_same' (1448202) panicked at examples/azul-news/src/store.rs:315:13: assertion `left == right` failed   left: Subscription { id: "f2", title: "Bakery", url: "https://f2.example.org/feed", site: "https://f2.example.org/", folder: "" }  right: Subscription { id: "f1", title: "Example Weekly", url: "
[azreader] xmltree::tests::names_and_attributes_match_without_case_and_prefix :: thread 'xmltree::tests::names_and_attributes_match_without_case_and_prefix' (1448875) panicked at examples/azul-reader/src/xmltree.rs:229:9: assertion `left == right` failed   left: None  right: Some("toc") 
[azshow] text::tests::a_text_body_survives_the_trip_through_the_shared_rich_text_model :: thread 'text::tests::a_text_body_survives_the_trip_through_the_shared_rich_text_model' (1449076) panicked at examples/azul-show/src/text.rs:259:9: nothing changed 
[azshow] text::tests::an_edit_from_the_editor_comes_back_into_the_body :: thread 'text::tests::an_edit_from_the_editor_comes_back_into_the_body' (1449078) panicked at examples/azul-show/src/text.rs:278:9: a numbered item is a bullet 
[azwriter] docx::tests::the_docx_wire_subset_becomes_blocks_and_skips_the_unknown :: thread 'docx::tests::the_docx_wire_subset_becomes_blocks_and_skips_the_unknown' (1449237) panicked at examples/azul-writer/src/docx.rs:252:9: assertion `left == right` failed: the item keeps its level   left: Bullet(     0, 
[azwriter] commands::tests::the_answer_of_an_import_read_is_a_document_or_a_sentence :: thread 'commands::tests::the_answer_of_an_import_read_is_a_document_or_a_sentence' (1449235) panicked at examples/azul-writer/src/commands.rs:348:79: not Word: RichTextDoc {     blocks: [         RichBlock { 
```

- R3-APPS: these 10 (AzKeys sample, AzMusic playlists, AzNews feed + store x3, AzReader xmltree, AzShow text x2,
  AzWriter docx + commands; several touch the shared rich-text model in layout/src/widgets/rich_text - numbering /
  list levels - and the XML parser leniency XML8).
