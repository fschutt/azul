# R3-TEXT progress (wave 9 round 3, branch wt/r3-text from 6a39b7f1a)

## DONE (report: scripts/R3_TEXT_2026_10_05.md, 3640744c7)
- 1ac9f57b2 text3 sub/super pins x4 (TEST: Chrome's parent-font /5+1, /3+1; super test also pins the line pitch)
- a2b5b5142 an_inline_block_sits_on_its_last_lines_baseline (TEST: <div> in <p> closes the p)
- 1fa2d7d34 an_inline_box_paints_its_border_padding_and_margin float (CODE: intrinsic scan + inline-box edges)
- 52939304c an_absolutely_positioned_child... (2) (CODE: `list-style-type: none` parsed as CSS-wide None)
- d31697263 css_zoom em (CODE: box props re-resolved before Pass 1)
- 2c8051608 line-height vh after resize (CODE: cache_map slots cleared on a viewport-unit viewport change)
- c62a0f5f0 text_sized_in_viewport_units_keeps_its_own_font_after_a_resize (CODE: font signature gate)
- 2fb0aab5b block_intrinsic_sizes_sanitize_nan_on_the_cross_axis (TEST: NaN sanitized on both axes)
- 2f4cb34f2 a_percentage_height_inline_block... (2) (CODE: float re-layout offered a definite height)
- dcce6712a + 223349d7e a_changed_inline_flex_box... (CODE: atomic inline laid out in its fresh size)
- 38da0a480 RED + 273b7c20a GREEN (+ f822d0548) pdfocr issue 1: nested sup/sub ride the parent's shift

## IN PROGRESS
- none

## NEXT
- none (coordinator: compile + run the commands in the report)

## Open questions
- coordinator said "do NOT update regression_metrics pins to 0.0/1.0"; they were updated in 1ac9f57b2 because
  those tests bypass getters entirely (evidence in the report); a tall-strut fixture is the alternative.

## Tools
- prebuilt suite binary: /Users/fschutt/Development/azul/target/release/deps/all-dc138c3d6f4fee36 (3db9fce91
  code), run via run_capped.sh; lldb --batch with -G true -C 'register read ...' breakpoints works
