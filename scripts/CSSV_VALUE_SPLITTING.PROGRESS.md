# CSSV_VALUE_SPLITTING - progress

Branch `wt/cssv-value-splitting`, base `d240a1b1d`.

## Findings so far

- The box-shadow case from `SYSTEM_COLOURS_EVERYWHERE_2026_09_26.md` (~l.112) is ALREADY fixed
  on the base: 080901e26 moved box-shadow / text-shadow / drop-shadow, border sides,
  border-color and scrollbar-color onto `basic::parse::split_string_respect_whitespace`
  (tests: `css/tests/spaces_inside_parentheses_stay_in_one_value.rs`).
- Still broken on the base:
  - `font-family` splits with `str::split(',')`: `"Foo, Bar", serif` is three families.
  - `animation` / `-azul-animation-in/out` lists split with `str::split(',')`: any
    `cubic-bezier(a, b, c, d)` tears the declaration apart (dropped).
  - The shared splitters are paren-aware but NOT quote-aware: `url("a).png"), red`
    loses its second layer.
  - Twins of the shared splitter: `grid::split_respecting_parens` (spaces only, so tab /
    newline do not separate tracks), the inline token scan in `parse_style_animation`,
    `custom_properties::split_var_arguments` (first top-level comma).

## DONE

- 43aebb545 RED tests: `css/tests/a_list_value_splits_only_at_its_top_level.rs` + 5 unit tests in
  `css/src/props/basic/parse.rs` (`mod tests`).

## IN PROGRESS

- The fix: one scanner in `basic::parse`.

## NEXT

- One scanner in `basic::parse` (paren + quote aware), the two public splitters on it,
  twins removed, font-family / animation lists / font-family printer fixed.
- Report `scripts/CSSV_VALUE_SPLITTING_2026_09_29.md`.

## Open questions

(none)
