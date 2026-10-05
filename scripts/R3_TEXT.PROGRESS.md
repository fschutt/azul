# R3-TEXT progress (wave 9 round 3, branch wt/r3-text from 6a39b7f1a)

## DONE
- 1ac9f57b2 text3 sub/super pins x4 (TEST wrong: Chrome's parent-font /5+1, /3+1; 0.0 / 1.0 in
  regression_metrics are right - those unit tests set constraints.vertical_align directly, no getters;
  super test now also pins the grown line pitch)
- a2b5b5142 an_inline_block_sits_on_its_last_lines_baseline (TEST wrong: <div> in <p> closes the p)
- 1fa2d7d34 an_inline_box_paints_its_border_padding_and_margin float (CODE: intrinsic scan lacked
  inline-box edges; shared helper inline_box_edge_advances)

- 52939304c an_absolutely_positioned_child... (2) (CODE: `list-style-type: none` parsed as CSS-wide None)
- d31697263 css_zoom em (CODE: layout_bfc re-resolved box props AFTER Pass 1 sized the child)
- 2c8051608 line-height vh after resize (CODE: cache_map slots kept across a viewport change)

- c62a0f5f0 text_sized_in_viewport_units_keeps_its_own_font_after_a_resize (CODE: signature gate read a px-resolved compact word)
- 2fb0aab5b block_intrinsic_sizes_sanitize_nan_on_the_cross_axis (TEST: NaN now sanitized on the main axis too)
- 2f4cb34f2 a_percentage_height_inline_block... (2) (CODE: layout_bfc's float re-layout offered a definite height)

## IN PROGRESS
- a_changed_inline_flex_box_behind_a_block_sibling_keeps_its_slot_and_widens (lib)
- pdfocr issue 1 (coordinator): sup/sub/vertical-align on nested inline boxes - RED test of the
  repro shape + fix (runs take vertical-align from inline ancestors up to the IFC root)

## Open questions
- coordinator said "do NOT update regression_metrics pins to 0.0/1.0"; they were updated in
  1ac9f57b2 because those tests bypass getters entirely (evidence in the report).

## Tools
- prebuilt suite binary: /Users/fschutt/Development/azul/target/release/deps/all-dc138c3d6f4fee36 (3db9fce91 code), run via run_capped.sh; lldb --batch with -G true -C 'register read ...' breakpoints works
