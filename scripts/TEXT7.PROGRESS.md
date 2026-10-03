# TEXT7 progress (wave 7, 2026-10-03)

Branch `wt/text7` from `2e55eef06`. Brief: scripts/waves/wave7/TEXT7.md. Never compiled (house rule).

## DONE
- ec1f991f8 progress file
- 6dafb9850 RED item 1: layout/tests/a_text_indent_narrows_the_first_line.rs (pdfocr's 3 + 8 more), all.rs
- b5728f613 GREEN item 1: text3 cache.rs `text_indent_of_line` + `indent_line_box` (greedy + KP + probe),
  position_one_line lost `is_after_forced_break` (tests/text3/mod.rs compat updated)
- f805f5b0a GREEN item 1: measure_intrinsic_widths counts the indent

## IN PROGRESS
- item 1, last piece: the intrinsic-size caller (solver3/sizing.rs ~:959, `UnifiedConstraints::default()`
  + white-space only) must pass text_indent / each_line / hanging (percentage = 0). Plan: ONE resolver
  `getters::resolve_text_indent(..)` (getters.rs, next to get_text_indent_value) used by fc.rs
  translate_to_text3_constraints (replace its inline block ~:5625-5665) and sizing.rs. These are LAYOUT7's
  files: minimal edits, say so in the report.

## NEXT
- items 2..6 in order (2: a run shaped before its font loads; 3: bolder/lighter relative; 4: dll layout.rs:1555
  off-by-one; 5: line-height 19px -> 19.55; 6: line-height rem/vw/vh)

## Decisions
- text-indent is applied as geometry of the line box (start-side segment narrowed), not a pen shift.
- KP's continuation-fragment case (flow chain + text-wrap: balance) still indents a continuation: not fixed
  (would need a param through kp_layout + its many test call sites); noted for the report.
- KP takes its base direction from the logical items, the greedy path from `direction` (pre-existing; noted).

## Open questions
- (none)
