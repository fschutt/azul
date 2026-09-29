# C2_CSS_RENDERING progress

Branch `wt/c2-css-rendering`, base `d9ce25179`. Nothing compiled (house rule).

## DONE
- 957940314 test(display_list): blurred see-through box-shadow paints at its declared alpha (PIN)
- e37bb50a4 docs(themes): the shadow helpers' single slot is a role, not a workaround
- 86353f719 test(css): a box-shadow list keeps every shadow (RED)
- 277adbe8f fix(css): a box-shadow list fills the four shadow slots, the first on top

## IN PROGRESS
- 3. `env()` among other shorthand components

## NEXT
- 3. RED (css/tests) then fix in parser2 (`check_if_value_is_css_env` single-call
  guard + per-component env expansion for shorthands)
- 4. V2 P1: RED (layout/tests) then fix: `Css::parse_inline` lowers node-targeting
  pseudo-states into conditions; `Dom::set_css` keeps the selector form
- report

## Findings so far
- Item 1 is ALREADY FIXED on the base: 3ffdafbb8 (RED) + 2b3e82c81 (fix,
  `getters::get_box_shadows` paints each distinct slot shadow once). The new
  test is a pin, not RED. The Rust helpers cannot become the shorthand: the
  single slot is how style_kit's roles stack (see e37bb50a4). node_graph.rs is
  codegen output (the shorthand's four slots) - left alone.
- Item 4 is a real bug, worse than V2 guessed: `color: blue; :hover { color: red; }`
  never turns red (both rules have empty conditions, the base rule comes last);
  `:hover { color: red; }` alone is red at rest.

## Open questions
- (none)
