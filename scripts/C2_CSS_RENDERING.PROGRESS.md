# C2_CSS_RENDERING progress

Branch `wt/c2-css-rendering`, base `d9ce25179`. Nothing compiled (house rule).

## DONE
- 957940314 test(display_list): blurred see-through box-shadow paints at its declared alpha (PIN)
- e37bb50a4 docs(themes): the shadow helpers' single slot is a role, not a workaround
- 86353f719 test(css): a box-shadow list keeps every shadow (RED)
- 277adbe8f fix(css): a box-shadow list fills the four shadow slots, the first on top
- 9bc3c0af7 docs(c2): progress
- a66988711 test(css): an env() among a shorthand's components feeds its own sides (RED)
- 9d8112a6d fix(css): an env() among a shorthand's components feeds only its own longhands
- a13cdcfe8 docs(c2): progress
- 714258f65 test(cascade): a node's own :hover block applies only when it is hovered (RED)
- aebb26349 fix(css): a node's own :hover block becomes a :hover condition at the inline parse
- 1e4dea28a perf(css): a shorthand value without env( skips the env component scan
- report: scripts/C2_CSS_RENDERING_2026_09_29.md

## IN PROGRESS
- (none)

## NEXT
- parent: compile + run the suites listed in the report; api.json autofix (parse_inline doc,
  optional Css.parse_scoped)
- follow-up (not this task): `form_controls::graft` lowering, keep-last shadow dedup

## Open questions
- Add `Css.parse_scoped` to api.json for FFI? (recommended, see report)
