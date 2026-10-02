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

## IN PROGRESS
- review pass of item 2 for compile errors

## NEXT
- 3 inline-block inside an inline span sized by its CSS width (fc.rs collect_inline_span_recursive)
- 4 text-edit report carries formats + pending format (DocumentTextEdit, core/src/selection.rs)
- report scripts/TEXTENG_2026_10_02.md

## Decisions
- Item 1 root cause: synthetic 0.8/0.2 strut vs real glyph A/D; the union of the two half-leading boxes is
  taller than line-height by |(A-D)/2 - 0.3em| (Times 11pt: 0.55px). x-height/cap-height stay approximations.
- Item 2: Number(FloatValue) for <number>; Default = Normal (CSS initial). Compact: px x100 (0.01px), factor
  x1000, I16_AUTO = read cascade (viewport units / out of range). Goldens left for AZ_BLESS (35 languages).
