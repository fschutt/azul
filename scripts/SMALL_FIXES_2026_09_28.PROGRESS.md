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

## IN PROGRESS

4. Windows accent
   - 752a87e1b RED: `azul_css::system::windows_accent` (css/src/system.rs,
     after `windows_fonts`) with TODAY's logic moved there as text parsers
     (`accent_from_reg_query`, `accent_palette_from_reg_query` -> None,
     `apply_accent` copies into selection) + 6 tests.
   - NEXT STEP: implement the three fns properly (AccentColorMenu /
     AccentColor DWORD 0xAABBGGRR, else palette entry 3; palette = 8 x RGBA
     from REG_BINARY, entries 0..6; apply_accent must NOT touch
     selection_background), then rewire
     `dll/src/desktop/shell2/windows/system_style.rs`: stop using
     DwmGetColorizationColor as the accent (it is the frame colour), query
     `reg` for Explorer\Accent then DWM AccentColor via the new fns, and stop
     copying into selection. Keep `adopt_theme_palette` accent survival.

## NEXT (in order)

5. AzWidgets demo a11y (`examples/azul-widgets/src/*.rs`,
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
