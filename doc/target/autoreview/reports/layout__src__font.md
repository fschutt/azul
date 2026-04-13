# Review: layout/src/font.rs

## Summary
- Lines: 1471
- Public functions: ~20 (across `loading`, `mock`, `parsed` submodules)
- Public structs/enums: 6 (`FontReloadError`, `MockFont`, `ParsedFont`, `PdfFontMetrics`, `OwnedGlyph`, `FontParseWarning`, `FontParseWarningSeverity`)
- Findings: 1 high, 3 medium, 0 low

## Findings

### [HIGH] Dead Code — `GsubCache`, `GposCache` type aliases unused outside module
- **Location**: `font.rs:148-150`
- **Details**: The type aliases `GsubCache` and `GposCache` are `pub` but only used within `font.rs` (as field types in `ParsedFont`). No external code references these aliases.
- **Evidence**: `grep 'GsubCache\|GposCache'` returns only `layout/src/font.rs`.
- **Recommendation**: Remove `pub` or inline the types.

### [MEDIUM] Lossy Type Conversion — `f32` to `i16` casts in outline collection
- **Location**: `font.rs:189-220`
- **Details**: All outline coordinate conversions use `as i16` (e.g. `to.x() as i16`). `Vector2F` components are `f32`. For fonts with large em-squares or coordinate values > 32767, this silently truncates. The `horz_advance as i16` cast at lines 782 and 812 is also lossy since `horz_advance` is `u16` (values > 32767 wrap to negative).
- **Evidence**: Lines 189, 190, 196, 197, 204-207, 215-220, 782, 812.
- **Recommendation**: Use `i16::try_from` or clamp to `i16::MIN..=i16::MAX` with a warning. For `horz_advance as i16`, consider whether `i16` is the correct target type.

### [MEDIUM] Refactoring — `from_bytes` is ~350 lines
- **Location**: `font.rs:569-916`
- **Details**: `ParsedFont::from_bytes` spans roughly 350 lines. It handles font data reading, table extraction, glyph outline parsing (two passes), layout cache building, cmap parsing, hashing, space width calculation, and space glyph insertion. This should be broken into focused helper methods.
- **Recommendation**: Extract at minimum: `parse_glyph_outlines(provider, ...) -> BTreeMap<u16, OwnedGlyph>`, `extract_raw_hinting_data(loca_glyf, map)`, and the space-glyph-insertion closure (lines 888-911) into a named method.

### [MEDIUM] `FontReloadError` barely used
- **Location**: `font.rs:26-47`
- **Details**: `FontReloadError` is only referenced in `font.rs` and `dll/src/desktop/mod.rs`. The `FontLoadingNotActive` variant in particular appears to be for a compile-time check that may no longer be relevant.
- **Evidence**: `grep 'FontReloadError'` returned only `font.rs` and `dll/src/desktop/mod.rs`.
- **Recommendation**: Verify the error type is still needed; if only used in one place, consider simplifying.

## System Documentation
- System identified: yes — **Text shaping / font system**
- Existing doc: none (no `doc/guide/text-shaping.md` or `doc/guide/fonts.md`)
- Doc needed: A guide covering the font pipeline: `font.rs` (parsing/metrics), `text3/` (shaping/layout), `glyph_cache.rs` (caching), `font_traits.rs` (trait abstractions), and how they connect to the layout solver and rendering backends.
