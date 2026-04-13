# Review: layout/src/text3/knuth_plass.rs

## Summary
- Lines: 630
- Public functions: 1 (`kp_layout`)
- Public structs/enums: 0 (all types are private)
- Findings: 0 high, 2 medium, 1 low

## Findings

### [MEDIUM] Overflow tracking not computed
- **Location**: `knuth_plass.rs:584-587`
- **Details**: `position_lines_from_breaks` returns `OverflowInfo::default()` which has an empty `overflow_items` vec and a zero `unclipped_bounds` rect. The actual content bounds are never accumulated, so callers cannot detect overflow or compute scroll dimensions for KP-laid-out text.
- **Evidence**: `OverflowInfo` struct (cache.rs:4863) has `overflow_items: Vec<ShapedItem>` and `unclipped_bounds: Rect` — both left at defaults.
- **Recommendation**: Accumulate `unclipped_bounds` from positioned items in the layout loop.

### [MEDIUM] TODO comments indicating incomplete features
- **Location**: `knuth_plass.rs:389`, `knuth_plass.rs:534`
- **Details**:
  1. Line 389: `// TODO: Add demerits for consecutive lines with very different ratios (fitness classes).` — This is a standard part of the Knuth-Plass algorithm that improves visual consistency.
  2. Line 534: `// TODO: also detect lines after forced breaks in KP path` — `text-indent` with `each-line` should apply after forced breaks, not just line 0.
- **Recommendation**: Track these as known limitations. The fitness class TODO is more impactful for quality.

### [LOW] Module doc could be more detailed
- **Location**: `knuth_plass.rs:1-2`
- **Details**: The `//!` module doc says "An implementation of the Knuth-Plass line-breaking algorithm for simple rectangular layouts." This is accurate but could mention the entry point (`kp_layout`), that it's activated by `text-wrap: balance`, and that it depends on `text3::cache` for shaped items and constraints.
- **Recommendation**: Expand to 3-4 lines mentioning the entry point and activation condition.

## System Documentation
- System identified: yes — text layout / line-breaking system (`text3` module)
- Existing doc: none (no `doc/guide/text-layout.md` or similar)
- Doc needed: A guide document covering the text layout pipeline: shaping → line-breaking (greedy vs. Knuth-Plass) → positioning. Should explain when each algorithm is used, how `UnifiedConstraints` controls behavior, and how the text layout integrates with the main layout solver via `CachedInlineLayout`.
