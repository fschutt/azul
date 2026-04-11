# Review: css/src/css.rs

## Summary
- Lines: 2012
- Public functions: 23
- Public structs/enums: 14 (Css, Stylesheet, CssDeclaration, DynamicCssProperty, BoxOrStatic, CssPropertyValue, PrintAsCssValue, CssRuleBlock, NodeTypeTag, NodeTypeTagParseError, NodeTypeTagParseErrorOwned, CssPath, CssPathSelector, CssPathPseudoSelector, CssNthChildSelector, CssNthChildPattern, RuleIterator)
- Public type aliases: 3 (CssContentGroup, BoxOrStaticStyleBoxShadow, BoxOrStaticString)
- Findings: 1 high, 0 medium, 2 low

## Findings

### [HIGH] Dead Code — `RuleIterator` only used in tests
- **Location**: `css.rs:1554-1585`
- **Details**: `Css::rules()` method and `RuleIterator` struct are not used from production code — only from test files (`css/tests/test_at_rules.rs`, `css/tests/test_nesting.rs`, `css/tests/test_parser_robustness.rs`).
- **Evidence**: Grep for `.rules()` returns only test files. Grep for `RuleIterator` returns only `css/src/css.rs`.
- **Recommendation**: Consider marking `#[cfg(test)]` or keeping as-is if it's part of the public API contract. Low risk but worth noting.

### [LOW] `NodeTypeTag::from_str` doesn't implement `std::str::FromStr` trait
- **Location**: `css.rs:915`
- **Details**: The method is named `from_str` and has the right signature shape, but it's an inherent method rather than implementing the standard `FromStr` trait. This prevents use with `.parse()`.
- **Recommendation**: Consider implementing `FromStr` trait if parse ergonomics are desired.

### [LOW] `DynamicCssProperty::is_inheritable` always returns false
- **Location**: `css.rs:504-509`
- **Details**: The comment explains why (could lead to bugs), but this means dynamic CSS properties can never be inherited, which may be an intentional design choice. The comment is accurate and explains the rationale.
- **Recommendation**: No change needed, but worth documenting this limitation in the `DynamicCssProperty` struct doc.

## System Documentation
- System identified: yes — CSS styling system (parsing, representation, specificity)
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`, `doc/guide/styling-system.md`
- Doc needed: n/a (covered by existing guides)
