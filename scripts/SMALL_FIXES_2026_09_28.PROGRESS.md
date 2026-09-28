# Small fixes 2026-09-28: progress checkpoint

Branch `wt/small-fixes` (from 5414bfa6b). NO cargo / rustc / LSP. RED commit
first, then the fix. Stage explicit paths. Final report goes to
`scripts/SMALL_FIXES_2026_09_28.md`; delete this file in that last commit.

## DONE

1. CSS paren-aware value split
   - 498370420 RED: `css/tests/spaces_inside_parentheses_stay_in_one_value.rs`
     (box-shadow, stylesheet box-shadow, text-shadow, drop-shadow, border,
     border-color, scrollbar-color).
   - dfac82e4a fix: box_shadow.rs / border.rs (parse_border_side,
     parse_style_border_color) / scrollbar.rs use
     `basic::parse::split_string_respect_whitespace`. Pins flipped:
     scrollbar `..._accepts_functional_colors_containing_spaces`,
     box-shadow `10px\u{a0}5px` now Err (NBSP is not CSS whitespace).
2. Bare `auto`
   - c0e2cd23e RED: `css/tests/a_bare_auto_is_only_valid_where_the_grammar_has_it.rs`.
   - 0d73a38fc fix: `grammar_accepts_bare_auto(key)` allowlist (46 longhands)
     in property.rs; spatial_nav.rs pin (446f70725) now asserts `is_err()`.
     Shorthands (parse_combined_css_property) NOT changed -> open item.
3. AppConfig padding
   - b674fcede RED: `core/src/resources_test.rs`
     `app_config_has_no_padding_between_its_fields`.
   - 209759f00 fix: fields reordered (8-aligned, 4-byte enums,
     remote_control, 4 bools). api.json needs autofix resync (NOT edited).

4. Windows accent
   - 752a87e1b RED: `azul_css::system::windows_accent` (css/src/system.rs,
     after `windows_fonts`) with TODAY's logic moved there + 6 tests.
   - 8fffb732f fix: parse by value name (AccentColorMenu / AccentColor,
     else AccentPalette[3]), palette parser, `apply_accent` writes only
     `colors.accent`; system_style.rs drops the Dwmapi loader and queries
     Explorer\Accent then DWM AccentColor. Palette parsed, not stored
     (no SystemColors slot) -> open item.

## IN PROGRESS

5. AzWidgets demo a11y
   - 877c4906b RED: date_picker.rs `every_day_cell_is_a_button_named_by_its_full_date`,
     `the_month_navigation_buttons_are_named`; chip.rs
     `the_remove_button_is_named_after_the_chip_it_removes`.
   - a2da2c573 fix: day cells PushButton + "Tuesday, 23 June 2026" names
     (named in build_grid; build_day_cell signature kept), nav "Previous
     month"/"Next month", chip × "Remove <label>".
   - NEXT STEP: demo call sites in examples/azul-widgets/src/*.rs:
     section titles / Card body / other `create_div_with_text` -> heading /
     p / span (keep the look: set margin-top: 0 on headings);
     `.with_accessibility_name` on Slider/Switch/CheckBox/RadioGroup builders;
     ProgressBar name; image a11y; the files drop zone (labelled() gives it
     a11y with role Unknown while it has callbacks) needs a role; icon-only
     node 318 (an Explore survey was running to identify it).

## NEXT (in order)

5b. (rest of 5, see IN PROGRESS) (`examples/azul-widgets/src/*.rs`,
   `layout/src/widgets/date_picker.rs`): div-as-text headings, icon-only
   control (node 318), Slider/Switch/CheckBox/RadioGroup names, date-picker
   day names ("Tuesday, 23 June 2026"), Progress name, image a11y, role
   Unknown interactive node. RED where testable (layout/tests, register in
   layout/tests/all.rs).
6. Report `scripts/SMALL_FIXES_2026_09_28.md` + delete this file.

## Open questions / notes

- Shorthand `auto` (margin/flex/padding...) still intercepted generically in
  parse_combined_css_property; `flex: auto` / `flex: none` semantics look
  wrong there (grow/shrink set to generic keywords) - list as open.
- CSS whitespace: the shared tokenizer splits on space/tab/LF/CR only (no
  form feed).
