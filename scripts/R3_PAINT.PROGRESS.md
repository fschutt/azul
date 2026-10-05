# R3-PAINT progress (wave 9 round 3, branch wt/r3-paint from 6a39b7f1a)

## DONE
- 48ecbd53e progress (diagnosis)
- 32c579ef1 svg_paint x5: TEST wrong - fixture lays its svg out as a block (strut ruling 2026-10-03)
- 08534b7fa svg_mask_memo_tests: TEST wrong - fixture uses the tree loader (document loader builds no SVG)
- 4fdbd3a40 a_turned_box_is_repainted_turned: TEST wrong - harness renders with the window's live GPU values
- 5cc29c251 a_border_style_alone_draws_a_medium_border: CODE wrong - compact default border width I16_INITIAL
- c9d2bb710 an_authors_single_border_keeps_the_rule_one_pixel: CODE wrong - `none` of a border is typed

## IN PROGRESS
- report scripts/R3_PAINT_2026_10_05.md

## NEXT
- nothing after the report (coordinator compiles and runs the suites)

## Open (reported, not fixed)
- the document loader (parse_xml_to_styled_dom / parse_html_to_styled_dom) builds no SVG geometry
- the display list's baked transform is stale on a first pass (fallback only; live backends are right)
- a CPU preview / tray icon render paints the viewport scrollbar when its content overflows
