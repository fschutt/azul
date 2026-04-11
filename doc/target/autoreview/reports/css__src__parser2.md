# Review: css/src/parser2.rs

## Summary
- Lines: 1860
- Public functions: 4 (`new_from_str`, `parse_css_path`, `pseudo_selector_from_str`, `parse_css_declaration`)
- Public structs/enums: 20 (many borrowed/owned pairs for FFI safety)
- Findings: 0 high, 3 medium, 2 low

## Findings

### [MEDIUM] Dead Code — multiple public types with no external callers
- **Location**: Various
- **Details**: Several public types are only referenced within `parser2.rs` itself:
  - `CssPathParseError` / `CssPathParseErrorOwned` (lines 596, 624)
  - `UnknownSelectorError` (line 299)
  - `VarOnShorthandPropertyError` (line 159)
  - `UnknownPropertyKeyError` (line 151)
  - `CssSyntaxErrorPos` (line 11) / `CssSyntaxInvalidAdvance` (line 25)
  - `UnparsedCssRuleBlock` / `UnparsedCssRuleBlockOwned` (lines 763, 774)
  - `CssPseudoSelectorParseErrorOwned` (line 306)
  - `DynamicCssParseErrorOwned` (line 376)
- **Evidence**: Grep for each type across `*.rs` returns only `css/src/parser2.rs`.
- **Recommendation**: These may be needed for FFI via `api.json`. If so, they should remain `pub` but be documented as FFI types. Otherwise reduce visibility to `pub(crate)`.

### [MEDIUM] TODO comments — unfinished nth-child "+" handling
- **Location**: `parser2.rs:484`
- **Details**: `// TODO: Test for "+"` in `parse_nth_child_pattern`. The "+" sign in patterns like `+3n` is not explicitly tested or handled, though `parse_nth_child_pattern` does split on "+" which partially covers it.
- **Recommendation**: Add test cases for nth-child patterns with leading "+".

### [MEDIUM] Refactoring — `new_from_str_inner` is ~340 LOC
- **Location**: `parser2.rs:1344-1683`
- **Details**: The main parsing loop is a single function spanning ~340 lines with a large `match` over token types and complex nesting state management. While cohesive, extracting the `BlockStart` handler (lines 1491-1555) and `BlockEnd` handler (lines 1564-1606) into separate functions would improve readability.
- **Recommendation**: Extract `handle_block_start` and `handle_block_end` helpers.

### [LOW] `pseudo_selector_from_str` is public but only called internally
- **Location**: `parser2.rs:409`
- **Details**: Called only at lines 740 and 1640 within `parser2.rs`.
- **Evidence**: `grep "pseudo_selector_from_str"` returns only `css/src/parser2.rs` and `tests/src/css.rs`.
- **Recommendation**: Consider `pub(crate)`.

### [LOW] Boilerplate — ~400 lines of `to_contained`/`to_shared` conversions
- **Location**: Throughout file (lines 90-405, 633-670, 780-805, 822-932)
- **Details**: Nearly a quarter of the file is mechanical borrowed-to-owned and owned-to-borrowed conversion code for error types. This is necessary for FFI but makes the file harder to maintain.
- **Recommendation**: Consider a derive macro to auto-generate these conversions long-term.

## System Documentation
- System identified: yes — CSS parsing system
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/styling-system.md`, `doc/guide/css-properties.md`
- Doc needed: n/a (existing guides cover the CSS system)
