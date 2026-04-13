# Review: layout/src/solver3/fc.rs

## Summary
- Lines: 8374
- Public functions: 14
- Public structs/enums: 10
- Findings: 1 high, 6 medium, 5 low

## Findings

### [HIGH] Dead Code — `layout_initial_letter` unused
- **Location**: `fc.rs:8332`
- **Details**: The initial letter / drop caps function is a stub (uses hardcoded heuristics like `CAP_WIDTH_RATIO: f32 = 0.7` and `LETTER_GAP: f32 = 4.0`) and has zero call sites anywhere in the codebase.
- **Evidence**: `grep -r 'layout_initial_letter' layout/` returns only the definition in fc.rs.
- **Recommendation**: Remove or gate behind a feature flag until drop caps support is implemented.

### [MEDIUM] Stub Code — colspan/rowspan hardcoded to 1
- **Location**: `fc.rs:4803-4804`
- **Details**: `let colspan = 1; // TODO: Get from CSS` and `let rowspan = 1; // TODO: Get from CSS`. Tables with colspan/rowspan attributes will render incorrectly — all cells will be treated as single-span. This is a functional limitation, not just a TODO note.
- **Recommendation**: Implement colspan/rowspan reading from HTML attributes or CSS properties, or document this as a known limitation.

### [MEDIUM] Stub Code — Font metrics hardcoded
- **Location**: `fc.rs:3623-3628`
- **Details**: `strut_ascent: font_size * 0.8`, `strut_descent: font_size * 0.2`, `strut_x_height: font_size * 0.5`, `ch_width: font_size * 0.5` — all hardcoded ratios instead of reading actual font metrics from the OS/2 table. This causes incorrect vertical alignment and ch-unit calculations for non-Latin fonts or fonts with atypical metrics.
- **Recommendation**: Resolve from `ParsedFontTrait` methods as the TODO suggests.

### [MEDIUM] Stub Code — Table baseline propagation not implemented
- **Location**: `fc.rs:4658`
- **Details**: `baseline: None, // TODO: implement proper table baseline propagation`. Tables used inline (e.g., `display: inline-table`) will not align correctly with surrounding text.
- **Recommendation**: Implement per CSS 2.2 §17.5.4.

### [MEDIUM] Bug-Prone — `EdgeSizes::default()` used for float margin
- **Location**: `fc.rs:7318`
- **Details**: `margin: EdgeSizes::default(), // TODO: Pass actual margin if this function is used`. In `position_floated_child`, a `FloatBox` is created with zero margins. If this code path is active, float clearance calculations that depend on the float's margin box will be incorrect.
- **Recommendation**: Pass the actual margin through from the caller.

### [MEDIUM] Refactoring — `layout_bfc` is ~1465 lines
- **Location**: `fc.rs:851-2316`
- **Details**: This function implements a two-pass BFC layout with margin collapsing, float positioning, clearance, and height calculation. While the logic is cohesive, at ~1465 lines it is difficult to navigate. The pass 1 sizing loop (lines 973-1008), pass 2 positioning loop (lines 1062-2046), and post-loop height calculation (lines 2058-2315) could each be extracted.
- **Recommendation**: Extract pass 1 (sizing), pass 2 (positioning), and the post-loop phase into helper functions.

### [MEDIUM] Refactoring — `collect_and_measure_inline_content_impl` is ~847 lines
- **Location**: `fc.rs:6158-7005`
- **Details**: This function collects inline content for IFC layout. It handles text nodes, inline-blocks, images, list markers, and shapes in a single large function.
- **Recommendation**: Extract the handling of each content type (text, inline-block, image, marker) into separate helper functions.

### [LOW] Magic Numbers — initial letter constants
- **Location**: `fc.rs:8351`, `fc.rs:8356`
- **Details**: `CAP_WIDTH_RATIO: f32 = 0.7` and `LETTER_GAP: f32 = 4.0` are local constants in a stub function. Acceptable for now since the entire function is unused stub code.
- **Recommendation**: When implementing drop caps properly, make these configurable or derive from font metrics.

### [LOW] Magic Number — scrollbar epsilon
- **Location**: `fc.rs:7388`
- **Details**: `const EPSILON: f32 = 1.0;` — a 1-pixel epsilon for scrollbar necessity checks. The comment explains the rationale (floating-point rounding), but 1.0px is unusually large for an epsilon.
- **Recommendation**: Consider reducing to 0.5 or documenting why 1.0 is correct.

### [LOW] Unused Import — `total_sibling_margins` tracked but never subtracted
- **Location**: `fc.rs:1027`
- **Details**: `total_sibling_margins` is incremented during layout but only used in a debug message (line 2291). The extensive comment block (lines 2207-2245) explains why it's NOT subtracted from content height. The variable could be removed or gated behind debug builds.
- **Recommendation**: Consider `#[cfg(debug_assertions)]` or removing if debug logging is sufficient.

### [LOW] Code Style — `resolve_explicit_dimension_width` and `_height` are near-duplicates
- **Location**: `fc.rs:637-672` and `fc.rs:676-712`
- **Details**: These two functions are structurally identical, differing only in which CSS property they read (`get_css_width` vs `get_css_height`) and which constraint axis they use. They could be unified with an axis parameter.
- **Recommendation**: Extract a shared helper or use a generic approach with an axis enum.

### [LOW] Verbose Documentation — `layout_bfc` margin collapsing explanation
- **Location**: `fc.rs:806-850`
- **Details**: The doc comment on `layout_bfc` includes a detailed ASCII diagram of the margin collapsing algorithm and extensive inline comments. While useful for understanding a complex algorithm, the function's internal comments (1400+ lines of them) could be condensed. The inline comments in the margin escape section (lines 1374-1441) are especially verbose.
- **Recommendation**: The algorithm correctness is more important than brevity here. No change strictly needed, but consider moving the extended explanation to a separate doc file.

## System Documentation
- System identified: yes — Layout Solver (CSS Visual Formatting Model)
- Existing doc: none (no `doc/guide/layout-solver.md` or similar)
- Doc needed: A guide covering the layout solver system (`solver3/`), including the formatting context dispatch, BFC/IFC/table/flex layout, margin collapsing, float positioning, and the two-pass layout architecture. This file is the core of the layout engine.
