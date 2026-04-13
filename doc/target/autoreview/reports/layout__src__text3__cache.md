# Review: layout/src/text3/cache.rs

## Summary
- Lines: 9805
- Public functions: ~90+
- Public structs/enums: ~80+
- Findings: 3 high, 4 medium, 1 low

## Findings

### [HIGH] Placeholder / Stub — Ruby text uses placeholder width calculation
- **Location**: `cache.rs:6472-6488`
- **Details**: Ruby text rendering uses `base_text.chars().count() as f32 * style.font_size_px * 0.6` as a placeholder width instead of actual font shaping. The `0.6` is an undocumented magic number. The ruby text content is completely ignored for sizing — only the base text length matters.
- **Evidence**: Line 6472: `let placeholder_width = base_text.chars().count() as f32 * style.font_size_px * 0.6;`
- **Recommendation**: Shape the ruby text through the same font pipeline as normal text to get accurate metrics.

### [HIGH] Stub Code — TODO comments indicating unfinished implementations
- **Location**: Multiple
- **Details**: Several TODO/stub comments indicate incomplete functionality:
  1. Line 2796: `// TODO: Parse SVG path data into PathSegments` — `CssShape::Path` falls back to rectangle
  2. Line 9232: `ShapeBoundary::Path { .. } => Ok(vec![]), // TODO!` — Path shapes return empty spans
  3. Line 9255: `// TODO: Dummy polygon function to make it compile` — despite the comment, the function is actually implemented
  4. Line 9312: `/// TODO: In a real app, this would be cached.` — hyphenator not cached
  5. Line 6423: `// TODO: use actual font's space_width via ParsedFontTrait::get_space_width()` — tab width uses approximate `font_size * 0.5`
- **Evidence**: Grep for TODO/placeholder/stub patterns.
- **Recommendation**: Items 1, 2, and 5 are functional gaps that affect layout accuracy. Item 3's comment is misleading and should be removed. Item 4 is a performance concern.

### [MEDIUM] File Size — 9805 lines mixing concerns
- **Location**: Entire file
- **Details**: The file contains: (1) ~80+ type definitions (enums, structs), (2) trait implementations (Hash, Eq, Ord) for those types, (3) the full 5-stage layout pipeline, (4) cursor movement logic (~600 lines), (5) line breaking/hyphenation (~1500 lines), (6) positioning/alignment (~800 lines), (7) caching infrastructure, (8) geometry helpers. While individual functions are reasonably sized, these are distinct concerns.
- **Recommendation**: Consider splitting into: `types.rs` (type definitions + impls), `cursor.rs` (cursor movement), `pipeline.rs` (layout stages), keeping `cache.rs` for caching infra. This is a suggestion, not urgent — the file is cohesive within the text layout domain.

### [MEDIUM] Unwrap on Mutex lock — potential panic on poisoned lock
- **Location**: `cache.rs:691`, `698`, `712`, `723`, `735`, `743`, `752`, `762`
- **Details**: All `FontManager` methods that access `parsed_fonts` or `embedded_fonts` call `.lock().unwrap()`. If any thread panics while holding the lock, the Mutex becomes poisoned and all subsequent `.unwrap()` calls will panic. Note that `load_fonts_for_chains` (line 567) correctly uses `if let Ok(mut map) = self.parsed_fonts.lock()`.
- **Evidence**: Lines 691, 698, 712, 723, 735, 743, 752, 762 all use `.lock().unwrap()`.
- **Recommendation**: Either consistently use `if let Ok(...)` pattern or document that Mutex poisoning is considered unrecoverable in this codebase.

### [MEDIUM] `AvailableSpace::Hash` rounds to usize, losing sub-pixel precision
- **Location**: `cache.rs:201-208`
- **Details**: `AvailableSpace::Definite(v)` hashes as `(v.round() as usize)`. This means widths like `300.3px` and `300.7px` produce the same hash, potentially serving a cached layout for the wrong width. For text layout, even 1px differences can change line breaks.
- **Evidence**: Line 205: `(v.round() as usize).hash(state);`
- **Recommendation**: Use `v.to_bits().hash(state)` for exact precision, or at minimum `((v * 10.0).round() as i64).hash(state)` for sub-pixel sensitivity.

### [MEDIUM] Verbose debug logging throughout pipeline
- **Location**: Throughout (e.g., lines 4160-4165, 5477-5489, 6692-6696, 9058-9063)
- **Details**: Extensive `if let Some(msgs) = debug_messages { msgs.push(...) }` blocks add significant visual noise. Many format strings are multi-line. While debug logging is valuable, the pattern is repeated hundreds of times and inflates function length.
- **Recommendation**: Consider a macro like `debug_msg!(msgs, "format", args)` to reduce boilerplate.

### [LOW] `Eq` and `Hash` contract — `InlineImage::baseline_offset` comparison
- **Location**: `cache.rs:5157`
- **Details**: In `inline_content_layout_eq`, `InlineImage` comparison uses direct `==` on `baseline_offset` (an `f32`). Since `InlineImage` has a custom `PartialEq` using `.to_bits()` this is fine, but it's inconsistent with the rounded comparison used elsewhere.
- **Recommendation**: Minor — document the intentional use of bitwise equality for f32 fields.

## System Documentation
- System identified: yes — Text Layout / Inline Formatting Context pipeline
- Existing doc: none (no `text-layout.md` or `inline-layout.md` in `doc/guide/`)
- Doc needed: A guide document covering the 5-stage text layout pipeline (logical analysis, bidi reordering, shaping, text orientation, flow/positioning), the caching architecture (LayoutCache, per-item cache, incremental relayout), font management (FontContext, FontManager, LoadedFonts), and how the text system integrates with the box layout solver (solver3/fc.rs). This file is the core of the text layout system and would benefit greatly from architectural documentation.
