# TEXTENG progress (wave 5, 2026-10-02)

Branch `wt/texteng` from `2e92c759b`. Brief: scratchpad wave5/TEXTENG.md. House rules: wave5_common.md (never compile).

## DONE
- 164eb6bf1 RED an_absolute_line_height_is_the_exact_line_pitch (+ number case)
- 4c4ea42d9 GREEN item 1: strut takes first available font's A/D (FontManager::first_available_font_metrics);
  translate_to_text3_constraints uses the IFC root's StyleProperties line_height (one reader);
  flex_intrinsic_text text-box-edge test moved to Azul Mock Mono.
- 89bc3aab3 RED item 2 (css parse round trip; layout em/%/number inheritance, rem, viewport units, 14pt exact)
- 5a680e173 GREEN item 2: StyleLineHeight enum, compact encoding x100 + CompactLineHeight, cascade computes
  em/% (compute_font_relative_line_height + inherits_its_computed_length), getters::get_used_line_height
  = one reader; empty_editable_caret_rect takes px.

- c71d2be9c RED item 3 (an_inline_block_inside_a_span_is_sized_by_its_own_css.rs)
- 029462d39 GREEN item 3: fc.rs measure_atomic_inline = ONE helper for the anonymous-wrapper, IFC-root and
  span branches (twins found: anonymous copy used constraints.containing_block_size); span collector takes
  text_cache + child_map
- e9aa6acc7 GREEN item 3b: nested boxes looked up among the span's own layout children (tree builder puts
  them under the span's node; the old lookup in the root's children never found them)

- 0594c94b6 RED item 4 (a_text_edit_reports_the_formats_of_its_text.rs, compile-RED)
- 6c892ca71 GREEN item 4: DocumentTextEdit.runs, TextFormatSet/TextFormatSpan, typing_formats
- report scripts/TEXTENG_2026_10_02.md

## IN PROGRESS
- final review pass

## NEXT
- done unless the review finds something

## Decisions
- Item 1 root cause: synthetic 0.8/0.2 strut vs real glyph A/D; the union of the two half-leading boxes is
  taller than line-height by |(A-D)/2 - 0.3em| (Times 11pt: 0.55px). x-height/cap-height stay approximations.
- Item 2: Number(FloatValue) for <number>; Default = Normal (CSS initial). Compact: px x100 (0.01px), factor
  x1000, I16_AUTO = read cascade (viewport units / out of range). Goldens left for AZ_BLESS (35 languages).
