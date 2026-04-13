# Review: layout/src/solver3/taffy_bridge.rs

## Summary
- Lines: 2076
- Public functions: 1 (`layout_taffy_subtree`)
- Public structs/enums: 0
- Findings: 1 high, 2 medium, 0 low

## Findings

### [MEDIUM] Refactoring — `translate_style_to_taffy` is ~478 lines long

- **Location**: `taffy_bridge.rs:599-1076`
- **Details**: This function is the core style translation but at ~478 lines it far exceeds the 60-100 LOC guideline. It handles display, position, inset, size, overflow, min/max, margin, padding, border, gap, grid templates, grid placement, flexbox properties, and alignment all in one function.
- **Recommendation**: Extract logical groups into sub-functions: `translate_grid_properties`, `translate_flex_properties`, `translate_box_model`, `translate_size_properties`.

### [MEDIUM] Refactoring — `compute_non_flex_layout` is ~320 lines long

- **Location**: `taffy_bridge.rs:1518-1835`
- **Details**: This function handles the fallback layout for non-flex/grid nodes. The `Ok` branch alone is ~170 lines. The width selection logic (lines 1680-1706) and the height selection logic (lines 1727-1753) are good extraction candidates.
- **Recommendation**: Extract `compute_effective_width` and `compute_final_height` helper functions.

## System Documentation
- System identified: yes — Layout Solver (specifically the Taffy flex/grid bridge)
- Existing doc: `doc/guide/architecture.md` (general), `doc/guide/styling-system.md` (CSS styling) — no dedicated layout solver guide
- Doc needed: A `doc/guide/layout-solver.md` explaining the solver3 architecture, the role of `taffy_bridge.rs` as the flex/grid delegation layer, how `fc.rs` dispatches to it, and how `LayoutTree` flows through the pipeline.
