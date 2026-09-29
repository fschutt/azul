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

- GREEN (this commit): `every_theme_css` / `every_theme_dom` (+ unit tests) and the `None`
  path of all 18: accordion alert avatar badge breadcrumb button card check_box chip
  color_input date_picker datetime_local divider drop_down form frame label menubar.
  DropDown's default theme is now `None` (was a pinned Flat; `None` built an empty div).
  card / form build their looks around a placeholder and put the caller's content in once;
  datetime_local builds its parts once, unpinned; color_input passes its pin to the
  picker's Label / TextInput / NumberInput. Older tests read nodes through the live view.

## IN PROGRESS
- review pass, report

## NEXT
- list_view, backstage: no UiTheme / no flora look exists - report

## Open questions
- list_view / backstage have no second theme: nothing to put in a flora block.
- A StyledDom with NO context (`StyledDom::create`, `create_from_dom`) evaluates every
  non-pseudo condition as false, so a followed widget there shows only its shared prefix.
  Engine fix (core, not mine): with no context, `Theme(Custom(n))` holds iff
  `n == DEFAULT_APP_THEME` (prop_cache.rs `matches_pseudo_state`, compact.rs inline scan).
- datetime_local's follow test needs T3's TimePicker migration (the time part follows).
