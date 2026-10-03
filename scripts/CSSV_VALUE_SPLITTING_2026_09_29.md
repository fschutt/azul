# CSSV_VALUE_SPLITTING: CSS values split only at their top level

Branch `wt/cssv-value-splitting`, base `d240a1b1d`. Nothing compiled or run (house rule).
Resumed once after a power loss: the 6 uncommitted files were complete and were committed
as they were.

## The reported bug

`box-shadow: 0 1px 2px rgba(16, 24, 40, 0.1)` (SYSTEM_COLOURS_EVERYWHERE, ~line 112) is
**already fixed on the base**. Commit 080901e26 moved these parsers onto the crate's
parenthesis-aware tokenizer:

- box-shadow and its longhands, text-shadow and drop-shadow();
- the border sides and column-rule;
- border-color and scrollbar-color.

`css/tests/spaces_inside_parentheses_stay_in_one_value.rs` pins that fix.

The rest of the audit found these still broken on the base:

| Family | Bug on the base |
|---|---|
| `font-family` | `str::split(',')`: `"Foo, Bar", serif` read as 3 families, also inside a `var()` fallback. The printer wrote `Foo,Bar` unquoted, so it read back as 2 families. |
| `animation`, `-azul-animation-in/out` | `str::split(',')` over the list: `x 1s cubic-bezier(0.4, 0, 0.2, 1)` was torn apart and **the declaration was dropped** (the lib test only called the single-entry parser). |
| the shared splitters | They respected parentheses but not quotes: `url("a).png"), red` lost its second layer. Form feed was not treated as whitespace. |
| grid tracks | The private twin `split_respecting_parens` cut at U+0020 only, so a track list on several lines was rejected. |

## What was built

`css/src/props/basic/parse.rs` now has **one scanner**, `find_top_level(input, is_separator)`.
A byte is at the top level when it is outside every `( ... )` and outside every `"..."` or
`'...'` string (a backslash inside a string escapes the next byte). A `)` with no `(` open ends
the search, so an unbalanced value stays in one piece and fails in its own parser.

Built on the scanner:

- `split_top_level`: behaves like `str::split` at top-level separators.
- `split_string_respect_comma` and `split_string_respect_whitespace` are now thin wrappers. Their
  pinned edge cases are unchanged: a trailing comma adds no empty item, and runs of whitespace
  collapse.
- `is_css_whitespace` covers space, tab, LF, CR and FF.

Fixed:

- `parse_style_font_family` splits with `split_top_level` at commas. It keeps `str::split`
  behaviour, so the empty-family pins still hold.
- `StyleFontFamily::as_string` quotes a name that contains a comma.
- `parse_style_animation_vec` splits with `split_string_respect_comma`.

Twins removed (house rule: no duplication). Each now calls the shared scanner:

- `grid::split_respecting_parens`;
- the inline token scan in `parse_style_animation`;
- the comma scan in `custom_properties::split_var_arguments`.

**Near-twins left alone:** two functions find a *matching* `)` rather than split a value. They
run the same state machine, so the parent may want them merged later:

- `custom_properties::closing_paren` (handles quotes);
- `filter.rs::parse_one_filter_function` (does not handle quotes).

## Commits

- 43aebb545 test(css): a list value splits only at its top level, never inside quotes (RED)
- b3c1d3879 docs(cssv): progress checkpoint after the RED tests
- daaa1df93 fix(css): one top-level scanner splits every value list, outside quotes too
- 66851cfdb docs(cssv): progress checkpoint after the fix
- (this report)

## Pins changed on purpose

| File | Change |
|---|---|
| `parse.rs` | The private-function tests for `skip_next_braces` and `split_string_by_char` (both functions deleted) were ported to `find_top_level` / `split_top_level`. Same cases, plus quotes. Stray-`)` expectations: `")a,b"` → no split (as before). Changed: `"a) (b,c"` is also one piece now; the old scanner started splitting again once a later `(` brought the depth back to 0. |
| `grid.rs` | The `split_respecting_parens` tests became tests of `parse_grid_template`. The BUG pin `does_not_treat_tab_or_newline_as_a_separator` now asserts 2 tracks. |
| `font.rs` | `style_font_family_as_string_does_not_escape_commas` (the LOSSY pin) became `style_font_family_as_string_quotes_a_name_with_a_comma`. |
| `transform.rs` | Comment only: the depth counter no longer goes negative. |

## api.json

**None.** The new public functions (`is_css_whitespace`, `find_top_level`, `split_top_level` in
`azul_css::props::basic::parse`) are Rust-only helpers; a generic closure argument cannot cross
the FFI. The existing `split_string_respect_*` functions are not in api.json either.

## Least sure to compile

1. `parse.rs` `TopLevelPieces::next`: `find_top_level(rest, &self.is_separator)` passes `&F`
   where the parameter is `impl Fn(u8) -> bool`. This relies on the blanket impl of `Fn` for `&F`.
2. `split_string_respect_comma`: `matches!(items.last(), Some(last) if last.is_empty())`, where
   `last` is a `&&str`.
3. `grid.rs`: `for part in &parts { parse_grid_track_or_repeat(part, ..) }` and
   `.map(|p| parse_grid_track_owned(p))` over `Vec<&str>` rely on `&&str` → `&str` deref
   coercion. Also check for an unused-import warning on `String` / `ToString` if nothing else in
   the non-test code uses them.
4. The new integration test:
   - `assert_eq!(x.as_slice(), &[a, b])` compares `&[T]` with `&[T; 2]`;
   - `let CssProperty::Animation(value) = &parsed else`;
   - the `declarations()` helper asserts **no parser warnings**. If some unrelated warning
     appears, that assertion fails, not the one under test.

## Test commands for the parent

```
cargo test --release -p azul-css --test a_list_value_splits_only_at_its_top_level
cargo test --release -p azul-css --test spaces_inside_parentheses_stay_in_one_value
cargo test --release -p azul-css --test custom_property_resolution
cargo test --release -p azul-css --lib props::basic::parse
cargo test --release -p azul-css --lib props::basic::font
cargo test --release -p azul-css --lib props::basic::animation
cargo test --release -p azul-css --lib props::layout::grid
cargo test --release -p azul-css --lib props::style      # shadow/border/scrollbar/background/transform/filter users
cargo test --release -p azul-css --lib custom_properties
cargo test --release -p azul-css --lib parser2
cargo test --release -p azul-css                          # everything, once
```

**Expected at the RED commit 43aebb545:**

- 5 new `parse.rs` unit tests fail: `a_comma_inside_a_quoted_string_does_not_split_a_list`,
  `a_parenthesis_inside_a_quoted_string_does_not_change_the_nesting`,
  `whitespace_inside_a_quoted_string_does_not_split_a_value`,
  `an_escaped_quote_does_not_end_a_quoted_string`, `a_form_feed_is_css_whitespace`.
- 7 integration tests fail: the 3 font-family tests, the 2 animation tests,
  `a_grid_template_splits_its_tracks_at_tabs_and_newlines` and
  `a_background_layer_list_splits_after_a_quoted_url_that_contains_a_parenthesis`.
- 5 integration tests are regression tests and pass: hsl() border, drop-shadow() filter list,
  rgba() gradient layers, the text-shadow `var()` fallback, and repeat(minmax()).

I checked every expected value in the new and ported scanner tests with a Python port of the
scanner (81 cases, all match).

## Audit: split sites left as they are

**Keyword and length components only.** No supported component here is a function or a quoted
string (`calc()` is only on width/height, and parsed whole):

- padding and margin;
- border-spacing, border-radius, border-style, border-width;
- object-position, overflow-clip-margin, background-position/size, gradient direction, the
  radial shape words;
- text-indent, initial-letter, hanging-punctuation, text-combine-upright, text-box-edge;
- counters;
- the blur, color-matrix, offset and composite arguments of filters;
- the property.rs shorthands: overscroll-behavior, overflow, flex, gap, columns, text-box,
  inset-block/inline;
- shape_parser.rs.

Moving these to the shared splitter would only flip about 6 autotest pins that document U+00A0
splitting.

**Splits inside a function's arguments:** rgb/hsl args, transform args, cubic-bezier args,
`rect()`, `polygon()`, grid `minmax()`. The arguments are numbers or lengths. Grid `minmax()`
stays unaware of parentheses on purpose: that bounds its recursion depth (pinned).

**Other sites:**

- Gradient stops use `split_color_and_offsets`, which scans from the end. A functional colour
  ends in `)`, so it is never mistaken for an offset. This is correct.
- parser2.rs: media `" and "`, nth-child, aspect-ratio. `env()` uses `splitn(2, ',')`, which is
  correct because the name comes first.

## Widgets and themes (not edited)

No widget or theme CSS string works around the splitting bug. These spaced functional colours
were silently dropped before 080901e26 and parse now:

- `layout/src/widgets/color_input.rs`: 91, 886/887, 923, 957 (box-shadow `rgba(0, 0, 0, x)`);
- `color_input.rs:697`: a gradient with `rgba(0, 0, 0, ...)`;
- `layout/src/widgets/themes/flora.rs`: 4989, 4991 (box-shadow `rgba(48, 45, 38, ...)` /
  `rgba(0, 0, 0, ...)`).

**Separate bug, not a splitting bug.** The widgets that build a single-side `StyleBoxShadow` in
Rust (`card.rs:66`, `node_graph.rs:2009ff`, `themes/decl.rs:382` `shadow()`,
`themes/style_kit.rs:290/315/378`) are working around this, as `decl.rs` says: the `box-shadow:`
shorthand expands to 4 side copies, and the painter draws each one as a whole-box shadow. A
translucent CSS shadow therefore paints about 4× too dark. That applies to the
color_input/flora strings above.

**Outside the widgets:** `examples/azul-widgets/src/lib.rs:180` (the dock panel) was changed to
`rgba(0,0,0,0.1)` because of this bug. It can go back to `rgba(16, 24, 40, 0.1)`.

## Left / adjacent findings (not fixed)

- `parser2::check_if_value_is_css_env` cuts at the LAST `)` and has no single-call guard (the
  `var()` check has one). `padding: env(safe-area-inset-top, 4px) 8px` silently drops `8px`.
- box-shadow parses one shadow only: a comma list of shadows is dropped whole (documented).
- `color-mix()`, and the space-separated `rgb(0 0 0 / 50%)` syntax, are not supported by
  `parse_css_color`, so there is no color-mix test.
- The `background:` shorthand takes a single layer value (a colour, image or gradient), not
  `rgba(..) url(..) no-repeat`.
- Behaviour change: an unquoted font name with an apostrophe (`Joe's Font, serif`) now reads as
  an unclosed string that runs to the end of the list. CSS treats `'` as the start of a string
  too.
- `filter.rs::parse_one_filter_function` does `char_indices().skip(open_paren + 1)`, which
  counts chars where it should count bytes. This looks wrong for non-ASCII text before `(`; I
  did not investigate it.
