# Review: css/src/props/style/border.rs

## Summary
- Lines: 916
- Public functions: 18 (parse functions) + constructors via macros
- Public structs/enums: ~20 (BorderStyle, StyleBorder*Style/Color, LayoutBorder*Width, StyleBorderSide, parser error types, shorthand result types)
- Findings: 2 high, 1 medium, 2 low

## Findings

### [HIGH] Missing `FormatAsRustCode` impl for `BorderStyle` — called but not defined
- **Location**: `border.rs:165-199`
- **Details**: The `FormatAsRustCode` impls for `StyleBorderTopStyle`, `StyleBorderRightStyle`, `StyleBorderLeftStyle`, and `StyleBorderBottomStyle` all call `self.inner.format_as_rust_code(tabs)` where `self.inner: BorderStyle`. However, `BorderStyle` does not implement `FormatAsRustCode` anywhere in the codebase. These impls are not behind any `#[cfg]` gate.
- **Evidence**: Searched `FormatAsRustCode.*BorderStyle` and `format_as_rust_code.*BorderStyle` across all `.rs` files — zero results. The trait is defined at `format_rust_code.rs:15` with no blanket impl that would cover `BorderStyle`.
- **Recommendation**: Add `impl FormatAsRustCode for BorderStyle` (can delegate to `Display`), or gate the four impls behind a feature flag if they're not needed unconditionally.

### [HIGH] `FormatAsRustCode` only implemented for Style types, not Color or Width
- **Location**: `border.rs:165-199`
- **Details**: `FormatAsRustCode` is implemented for the four `StyleBorder*Style` types but NOT for `StyleBorder*Color` or `LayoutBorder*Width`. The color types are handled elsewhere (`format_rust_code.rs` has macro-generated impls for color/pixel types), but consistency should be verified. The style-side impls are hand-written here rather than using the macro pattern in `format_rust_code.rs`.
- **Evidence**: Searched `FormatAsRustCode for StyleBorderTopColor` — zero results in this file. `format_rust_code.rs` has `impl_pixel_value_fmt!` macro at line 418 but no invocation for border width types was found.
- **Recommendation**: Verify all border property types have `FormatAsRustCode` impls, ideally via the centralized macros in `format_rust_code.rs`.

### [MEDIUM] Shorthand parser structs only exist behind `#[cfg(feature = "parser")]`
- **Location**: `border.rs:622-628` (`StyleBorderColors`), `border.rs:693-699` (`StyleBorderStyles`), `border.rs:760-766` (`StyleBorderWidths`)
- **Details**: These structs are only available when the `parser` feature is enabled, but `display_list.rs` defines its own separate `StyleBorderColors`/`StyleBorderStyles`/`StyleBorderWidths` structs (with different field types: `Option<CssPropertyValue<...>>`). While not true duplicates (different field types), the naming collision is confusing — two unrelated `StyleBorderColors` types in the same project.
- **Recommendation**: Consider renaming the parser-only structs (e.g., `ParsedBorderColors`) or the display-list structs to avoid confusion.

### [LOW] `CssBorderParseError` type alias and `CssBorderParseErrorOwned` newtype for backwards compatibility
- **Location**: `border.rs:442`, `border.rs:448`
- **Details**: `CssBorderParseError` is a type alias for `CssBorderSideParseError`, and `CssBorderParseErrorOwned` is a newtype wrapper around `CssBorderSideParseErrorOwned`. The comment says "for compatibility with old code." If no old code depends on these, they add unnecessary indirection.
- **Evidence**: `CssBorderParseError` is used in `property.rs` (via `parse_style_border`'s return type). `CssBorderParseErrorOwned` is used in `property.rs` as well.
- **Recommendation**: Low priority — these are used, so keep them. But consider eventually consolidating to a single error type name.

### [LOW] Repetitive shorthand parser structure (color/style/width)
- **Location**: `border.rs:636-821`
- **Details**: `parse_style_border_color`, `parse_style_border_style`, and `parse_style_border_width` follow the exact same 1-2-3-4 value expansion pattern (CSS shorthand box-side expansion). This pattern is also used elsewhere in the codebase for margin/padding.
- **Recommendation**: Low priority — a generic `parse_four_sided_shorthand` helper could reduce ~150 lines to ~50, but the current code is clear and correct.

## Notes

- `// +spec` comments found at lines 26 and 157 — left as-is per review rules.
- No `todo!()`, `unimplemented!()`, `placeholder`, `stub`, `FIXME`, `HACK`, or other vibe-coding hints found.
- No `unsafe` code in this file.
- No `..Default::default()` patterns.
- No scripts found related to border feature area.
- Module-level doc comment present at line 1: `//! CSS properties for border style, width, and color.` — adequate.
- File size (916 lines) is reasonable for the scope.

## System Documentation
- System identified: CSS styling system (property definitions and parsing)
- Existing doc: `doc/guide/css-properties.md`, `doc/guide/css-styling.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)
