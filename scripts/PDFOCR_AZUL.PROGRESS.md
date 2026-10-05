# PDFOCR-AZUL progress (branch wt/pdfocr-azul, base 4158b7039)

Task: pdfocr engine issues 2 (position: relative on an inline span does not
move its text) and 3 (`hyphens: auto` ignores the `lang` attribute).
Report: /Users/fschutt/Development/pdfocr/results/engine-issues/README.md

## DONE
- (none yet)

## IN PROGRESS
- issue 3: RED tests

## NEXT
- issue 3 RED: core/src/xml_attributes.rs inline test (lang lands as
  AttributeType::Lang) + layout/tests/hyphens_auto_takes_its_language_from_the_lang_attribute.rs
- issue 3 GREEN: xml_attributes `lang` / `xml:lang` entries; solver3/fc.rs
  falls back to the nearest `lang` when hyphens: auto and no azul property
- issue 2 RED: layout/tests/a_relatively_positioned_inline_moves_its_text.rs
- issue 2 GREEN: positioning.rs shared shift helper + display_list.rs shifts
  the runs (and the paged TextLayout payload) of relpos inline boxes

## Findings
- issue 3 root cause has two halves: the XML loader's ONE attribute table
  (core/src/xml_attributes.rs) has no `lang` entry, so `lang="en"` never
  reaches the DOM; and fc.rs reads the hyphenation language only from
  `-azul-hyphenation-language`.
- issue 2: adjust_relative_positions moves layout BOXES; the text of an inline
  box is painted from its block container's line layout (glyph runs + the
  paged TextLayout payload printpdf draws from), which never sees the offset.
