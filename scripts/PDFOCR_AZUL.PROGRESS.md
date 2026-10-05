# PDFOCR-AZUL progress (branch wt/pdfocr-azul, base 4158b7039)

Task: pdfocr engine issues 2 (position: relative on an inline span does not
move its text) and 3 (`hyphens: auto` ignores the `lang` attribute).
Report: scripts/PDFOCR_AZUL_2026_10_05.md (pdfocr's issue list:
/Users/fschutt/Development/pdfocr/results/engine-issues/README.md)

## DONE
- 7f90585b5 progress file
- a8e5b0167 issue 3 RED: core/src/xml_attributes.rs inline test +
  layout/tests/hyphens_auto_hyphenates_in_the_language_of_the_lang_attribute.rs (NEW, unregistered)
- 030ce1de7 issue 3 GREEN: xml_attributes `lang` / `xml:lang` entries;
  fc.rs content_language + hyphenation_language_of_tag, read under hyphens: auto only
- 0221a4aca progress
- 92001bbee issue 2 RED: layout/tests/a_relatively_positioned_inline_box_moves_its_text.rs (NEW, unregistered)
- 794987ebb issue 2 GREEN 1/2: positioning.rs relative_shift + inline_relative_offset
- 93706ab82 issue 2 GREEN 2/2: display_list.rs inline_run_shifts, shifted runs + paged payload
- 919a2032d issue 2: a moved run claims no proven uniform background
- report scripts/PDFOCR_AZUL_2026_10_05.md

## IN PROGRESS
- (none)

## NEXT
- coordinator: register the two new test files in layout/tests/all.rs, compile, run
  the commands in the report.

## Open questions / follow-ups
- caret + selection highlight of moved inline text stay at the laid-out place.
- an inline-block inside a relatively positioned span does not move with it.
- DL patching (`try_copy_cached_run`) could keep a stale shift when only a span's
  `top` changes and the IFC does not re-emit.
