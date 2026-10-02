# TEXTENG progress (wave 5, 2026-10-02)

Branch `wt/texteng` from `2e92c759b`. Brief: scratchpad wave5/TEXTENG.md. House rules: wave5_common.md (never compile).

## DONE
- 164eb6bf1 RED an_absolute_line_height_is_the_exact_line_pitch (+ number case)
- 4c4ea42d9 GREEN item 1: strut takes first available font's A/D (FontManager::first_available_font_metrics);
  translate_to_text3_constraints uses the IFC root's StyleProperties line_height (one reader);
  flex_intrinsic_text text-box-edge test moved to Azul Mock Mono.

## IN PROGRESS
- 2. StyleLineHeight enum (Normal/Number/Length/Percentage), em/% inherit as length

## NEXT
- 2 RED tests (css parse: rem/vw accepted; layout: em/% inherit as length, number as factor, rem pitch)
- 2 GREEN css type, core compact/prop_cache, layout readers, codegen goldens
- 3 inline-block inside an inline span sized by its CSS width
- 4 text-edit report carries formats + pending format

## Decisions
- Item 1 root cause: synthetic 0.8/0.2 strut vs real glyph A/D; the union of the two half-leading boxes is
  taller than line-height by |(A-D)/2 - 0.3em| (Times 11pt: 0.55px). x-height/cap-height stay approximations.
- Compact encoding quantizes absolute line-heights to 0.1px (14pt -> 18.7); item 2 rewrites the encoding.
