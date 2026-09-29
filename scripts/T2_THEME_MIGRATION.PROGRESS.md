# T2 - @theme migration, first half of the widgets (progress)

Branch `wt/t2-theme-migration` from `fix/input-bugs-2026-09-19` @ `36ce2f698`. Recipe:
`scripts/T1_APP_THEME_2026_09_29.md` section 4. Nothing compiled (house rule).

## Approach (decided)

`theme: None` = follow the app theme. The widget builds BOTH pinned DOMs (today's
`themes::flat::x` / `themes::flora::x`, untouched), and
`themes::theme_blocks::every_theme_dom(flat, flora)` keeps the tree of the structural theme
(`UiTheme::current()`: nodes, classes, marker, callbacks, a11y) and gives each node the
declarations of both themes: the leading rules both themes share (unconditional, no dark twin of
their property) once, then flat's rest inside `@theme(flat)`, then flora's inside `@theme(flora)`.
A subtree identical in both themes (user content, a theme-independent part) is kept as is.
Pinned (`with_theme`) = today's path, byte for byte.

Why not const statics per widget: both themes' declarations are runtime-built (`decl::`, `Look`
structs, kind-dependent fns); rewriting ~40 generators into `theme_conditions!` statics without a
compiler is the riskiest possible change. The merge is mechanical, exact (under theme T the live
rules ARE pinned T's rules, in order) and one place to later swap for statics.

## DONE
- RED (this commit): `themes/theme_blocks.rs` `checks` (the contract: under app theme T the
  followed widget IS pinned T node by node, same a11y, pairs per block, pinned ignores the app
  theme, both blocks carried); `theme_checks::applies` / `half_pairs` and `theme_probe` read
  `@theme(<name>)` against `current_theme()`; `mod app_theme_tests` in all 18 widgets with a
  second theme.

## IN PROGRESS
- GREEN: `every_theme_css` / `every_theme_dom` + each widget's `None` path

## NEXT
- G1 divider badge label form card avatar
- G2 accordion alert breadcrumb chip frame menubar
- G3 button check_box drop_down datetime_local
- G4 color_input date_picker
- list_view, backstage: no UiTheme / no flora look exists - report

## Open questions
- list_view / backstage have no second theme: nothing to put in a flora block.
