# TEXT7 progress (wave 7, 2026-10-03)

Branch `wt/text7` from `2e55eef06`. Brief: scripts/waves/wave7/TEXT7.md. Never compiled (house rule).

## DONE
- ec1f991f8 progress file
- 6dafb9850 RED item 1: layout/tests/a_text_indent_narrows_the_first_line.rs (pdfocr's 3 + 8 more), all.rs
- b5728f613 GREEN item 1: text3 cache.rs `text_indent_of_line` + `indent_line_box` (greedy + KP + probe),
  position_one_line lost `is_after_forced_break` (tests/text3/mod.rs compat updated)
- f805f5b0a GREEN item 1: measure_intrinsic_widths counts the indent
- 64d6ee9c5 GREEN item 1: getters::resolve_text_indent (one resolver) used by fc.rs + sizing.rs (LAYOUT7 files,
  minimal edits)
- fbc03a5bd RED item 2: cache.rs test module `a_run_shaped_before_its_font_loads`
- ca05d694c GREEN item 2: per-thread deficit counter; the per-item cache skips deficient groups
- d1763bc89 RED item 3: layout/tests/bolder_and_lighter_are_relative_to_the_parent_weight.rs
- 0a0093605 GREEN item 3: StyleFontWeight::computed (css), compact builder + cascade (core), getters::
  get_computed_font_weight (layout)

## IN PROGRESS
- item 4: dll/src/desktop/shell2/common/layout.rs:~1555 `NodeId::from_usize(i)` off-by-one (GeolocationProbe)

## NEXT
- items 5..6 in order (5: line-height 19px -> 19.55; 6: line-height rem/vw/vh)

## Decisions
- text-indent is applied as geometry of the line box (start-side segment narrowed), not a pen shift.
- KP's continuation-fragment case (flow chain + text-wrap: balance) still indents a continuation: not fixed
  (would need a param through kp_layout + its many test call sites); noted for the report.
- KP takes its base direction from the logical items, the greedy path from `direction` (pre-existing; noted).

## Open questions
- (none)
