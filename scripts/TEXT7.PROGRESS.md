# TEXT7 progress (wave 7, 2026-10-03)

Branch `wt/text7` from `2e55eef06`. Brief: scripts/waves/wave7/TEXT7.md. Never compiled (house rule).

## DONE
- (none yet)

## IN PROGRESS
- item 1: text-indent narrows the first line (greedy path in layout/src/text3/cache.rs)

## NEXT
- item 1 RED: copy pdfocr's a_text_indent_narrows_the_first_line.rs into layout/tests, register in all.rs
- item 1 GREEN: break_one_line / get_line_constraints subtract the indent on the paragraph's first line;
  justify spreads over width - indent
- then items 2..6 in order

## Decisions
- (none yet)

## Open questions
- (none yet)
