# Review: layout/src/solver3/positioning.rs

## Summary
- Lines: 1154
- Public functions: 4 (`get_position_type`, `position_out_of_flow_elements`, `adjust_relative_positions`, `adjust_sticky_positions`)
- Public structs/enums: 0
- Findings: 0 high, 2 medium, 2 low

## Findings

### [MEDIUM] Refactoring — `position_out_of_flow_elements` is ~455 lines
- **Location**: `positioning.rs:143-598`
- **Details**: This single function is approximately 455 lines long. It contains the entire vertical and horizontal constraint solving for absolutely positioned elements. The horizontal constraint block (lines 429-592) alone is ~160 lines and could be extracted into a helper like `resolve_abspos_horizontal_position`.
- **Recommendation**: Extract the vertical constraint solver (lines 342-419) and the horizontal constraint solver (lines 429-592) into separate helper functions.

### [MEDIUM] Missing Module-Level Documentation
- **Location**: `positioning.rs:1-2`
- **Details**: The module doc comment is minimal: `//! solver3/positioning.rs` and `//! Pass 3: Final positioning of layout nodes`. It does not explain the key types/entry points exported, or how this module fits into the layout pipeline (called from `solver3/mod.rs` after sizing).
- **Recommendation**: Expand the `//!` block to mention the three main passes (out-of-flow positioning, relative adjustment, sticky adjustment) and that this is called from `solver3/mod.rs` after the formatting context pass.

### [LOW] Excessive `// +spec` Comment Density
- **Location**: Throughout file, particularly lines 203-218 (16 spec comments for one `if` branch), lines 1069-1104 (35+ spec comments for one function)
- **Details**: While `// +spec` comments are never to be removed, the density around `find_absolute_containing_block_rect` (35+ spec references on the function header alone) and the fixed-positioning containing-block selection (16 spec references for one `let` binding) significantly hurts readability. The same spec point appears duplicated in several places (e.g., `+spec:positioning:067eab` appears twice at lines 209-210, `+spec:containing-block:faa9a3` appears twice at lines 207-208, `+spec:containing-block:7f5090` appears twice at lines 1156-1157).
- **Recommendation**: Deduplicate exact-same spec references that appear consecutively.

### [LOW] Duplicated Containing Block Walk
- **Location**: `positioning.rs:447-461` and `positioning.rs:1113-1149`
- **Details**: The horizontal constraint solver in `position_out_of_flow_elements` walks up to find the containing block's direction (lines 447-461), and `find_absolute_containing_block_rect` does a very similar walk up ancestors to find the nearest positioned ancestor (lines 1113-1149). The first walk could reuse the result from the containing block calculation instead of walking the tree again.
- **Recommendation**: Consider having `find_absolute_containing_block_rect` return the containing block's DOM ID alongside the rect, so the caller doesn't need a second walk for direction lookup.

## System Documentation
- System identified: yes — Layout Solver (CSS positioning pass)
- Existing doc: none (no `doc/guide/layout-solver.md` or similar)
- Doc needed: A guide document covering the layout solver pipeline (`solver3/`) — its passes (formatting context / sizing, relative positioning, sticky positioning, absolute/fixed positioning), key entry points, and how it integrates with the styled DOM and display list generation.
