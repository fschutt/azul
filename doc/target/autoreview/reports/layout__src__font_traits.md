# Review: layout/src/font_traits.rs

## Summary
- Lines: 209
- Public functions: 0 (only trait methods)
- Public structs/enums: ~27 (including stub types)
- Public traits: 3 (ShallowClone, ParsedFontTrait, FontLoaderTrait)
- Findings: 1 high, 0 medium, 1 low

## Findings

### [HIGH] Missing stub — `AvailableSpace` not provided in stub module

- **Location**: `font_traits.rs:114-240` (stub module)
- **Details**: When features `text_layout` + `font_loading` are enabled, `AvailableSpace` is re-exported from `text3::cache` (line 13). But the stub module does not define a stub `AvailableSpace`. Code that references `AvailableSpace` (used in 10+ files across solver3, tests, etc.) would fail to compile without the features. If the stub module's purpose is to allow compilation without these features, this is a gap.
- **Evidence**: `AvailableSpace` is used in `solver3/taffy_bridge.rs`, `solver3/fc.rs`, `solver3/sizing.rs`, `solver3/layout_tree.rs`, `solver3/cache.rs`, `window.rs`, `text3/knuth_plass.rs`, and tests — but none of these have `#[cfg]` guards either, so in practice the features are likely always enabled and the stubs may be vestigial. Still worth noting for correctness.
- **Recommendation**: Either add a stub `AvailableSpace` enum or document that the stub module is not intended to be a complete compilation target.

### [LOW] `ShallowClone` trait has no implementations in this file

- **Location**: `font_traits.rs:29-32`
- **Details**: `ShallowClone` is defined here and implemented elsewhere (text3/cache.rs, font.rs, test files). This is expected for a trait-definition file — not a bug, but noting for completeness. The trait is well-used (6+ impls).
- **Recommendation**: None needed.

## System Documentation
- System identified: yes — **Text Shaping & Font System** (font loading, text layout, glyph shaping)
- Existing doc: none (no `doc/guide/` file for fonts or text shaping)
- Doc needed: A `doc/guide/text-shaping.md` or `doc/guide/fonts.md` covering:
  - The `ParsedFontTrait` / `FontLoaderTrait` trait hierarchy
  - How `text3/cache.rs` implements font caching and layout
  - The feature-gating system (`text_layout` + `font_loading`) and stub fallbacks
  - How `rust_fontconfig` integrates for font discovery
  - The relationship between `FontManager`, `LayoutCache`, and the solver
