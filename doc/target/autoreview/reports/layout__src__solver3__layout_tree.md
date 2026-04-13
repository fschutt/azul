# Review: layout/src/solver3/layout_tree.rs

## Summary
- Lines: 2766 (2556 non-blank)
- Public functions: 37
- Public structs/enums: 15
- Findings: 1 high, 3 medium, 3 low

## Findings

### [HIGH] Dead Code — `ComputedLayoutStyle` struct only used in its own file
- **Location**: `layout_tree.rs:536`
- **Details**: The `ComputedLayoutStyle` struct is defined and used only within `layout_tree.rs`. It is stored as a field on `LayoutNode` and `LayoutNodeWarm`, and populated by `compute_layout_style()`. However, no code outside this file reads any field of `ComputedLayoutStyle`. The fields are used indirectly via the `LayoutNodeWarm.computed_style` field, but `ComputedLayoutStyle` itself is never imported or referenced in any other `.rs` file.
- **Evidence**: `grep -rn 'ComputedLayoutStyle' layout/src/` — only matches in `layout_tree.rs`.
- **Recommendation**: Reduce visibility to `pub(crate)` since external crates don't need it. The struct itself is used (via the warm tier), but its type visibility is overly broad.

### [MEDIUM] Vibe-Coding Hints — Phase 2 deferred work markers throughout
- **Location**: `layout_tree.rs:161,217,277,291,298-299`
- **Details**: Multiple comments reference "Phase 2", "Phase 2a", "Phase 2c", "Phase 2c/2d" as deferred optimization work. These are not `TODO` macros but indicate code that is intentionally incomplete:
  - Line 291: `source_node_id` for non-cluster `ShapedItem` variants always returns `None` ("Phase 2c will refine this")
  - Line 298-299: `can_break` defaults to `true` for all items ("Phase 2c will refine this")
  - Line 277: `extract_item_metrics` is described as enabling "Phase 2c/2d" optimization
- **Evidence**: Related planning document exists: `scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md`
- **Recommendation**: These are deliberate deferments, not stubs. No action needed unless Phase 2 is being implemented, but they should be tracked.

### [MEDIUM] Missing Documentation — Several private helper functions lack doc comments
- **Location**: `layout_tree.rs:1911` (`is_inline_level`), `layout_tree.rs:2098` (`get_element_font_size`), `layout_tree.rs:2111` (`get_parent_font_size`), `layout_tree.rs:2122` (`get_root_font_size`)
- **Details**: These helper functions have no or minimal doc comments. While private, they implement CSS spec logic that benefits from documentation.
- **Recommendation**: Low priority — these are internal helpers with clear names.

### [MEDIUM] Deprecated Variant — `AnonymousBoxType::ListItemMarker`
- **Location**: `layout_tree.rs:592`
- **Details**: The comment says "DEPRECATED: Use PseudoElement::Marker instead" but the variant is still present in the enum. It should be checked whether any code still references it.
- **Evidence**: Line 592: `/// DEPRECATED: Use PseudoElement::Marker instead`
- **Recommendation**: If no code uses `AnonymousBoxType::ListItemMarker`, remove it. If code still references it, complete the migration.

### [LOW] Type Cast — `usize` to `u32` in `extract_item_metrics`
- **Location**: `layout_tree.rs:310`
- **Details**: `positioned_item.line_index as u32` narrows from `usize`. Safe in practice (a document won't have 4B+ lines) but theoretically lossy.
- **Recommendation**: No immediate action needed; document the assumption or use `try_into()` for defense-in-depth.

### [LOW] Type Cast — `arena.len()` and `children.len()` narrowed to `u32`
- **Location**: `layout_tree.rs:1859-1860`
- **Details**: `arena.len() as u32` and `node.children.len() as u32` truncate silently if values exceed `u32::MAX`.
- **Recommendation**: Acceptable for practical DOM sizes. Consider `debug_assert!` to catch unexpected growth.

### [LOW] Run-in Display Fallback — not implemented, falls back to block
- **Location**: `layout_tree.rs:2744-2754`
- **Details**: `LayoutDisplay::RunIn` and `LayoutDisplay::Marker` silently fall back to `FormattingContext::Block`. Five `+spec` comments document this limitation. This matches browser behavior (run-in is rarely used) but is technically incomplete.
- **Recommendation**: No action needed — this is a deliberate and well-documented limitation.

## System Documentation
- System identified: **Layout Solver** (specifically the layout tree construction and anonymous box generation phase of the CSS layout pipeline)
- Existing doc: `doc/guide/lifecycle.md` covers the high-level lifecycle; `doc/guide/css-styling.md` covers CSS property resolution
- Doc needed: A `doc/guide/layout-solver.md` guide explaining the layout pipeline phases (tree construction → sizing → positioning → display list), the SoA optimization, and the IFC/BFC formatting context model would benefit developers working on this code.
