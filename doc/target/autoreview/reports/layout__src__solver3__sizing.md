# Review: layout/src/solver3/sizing.rs

## Summary
- Lines: ~2050
- Public functions: 5 (`resolve_percentage_with_box_model`, `calculate_intrinsic_sizes`, `collect_inline_content`, `calculate_used_size_for_node`, `extract_text_from_node`)
- Public structs/enums: 0
- Findings: 0 high, 1 medium, 3 low

## Findings

### [MEDIUM] Module-Level Documentation is minimal
- **Location**: `sizing.rs:1-3`
- **Details**: The module doc is just `//! solver3/sizing.rs` and `//! Pass 2: Sizing calculations (intrinsic and used sizes)`. It doesn't explain key types/entry points or how it fits into the layout solver pipeline.
- **Recommendation**: Add brief documentation listing the two main entry points (`calculate_intrinsic_sizes` for Pass 2a and `calculate_used_size_for_node` for per-node sizing), the key helper `collect_inline_content`, and how this module fits between tree construction and positioning.

### [LOW] `calculate_used_size_for_node` is ~580 lines (lines 1402–1980)
- **Location**: `sizing.rs:1402-1980`
- **Details**: This function is very long but is already structured with clear step comments (Steps 1-6). The width and height resolution branches are necessarily parallel. Extracting sub-functions for width/height resolution would be possible but the steps are already well-delineated.
- **Recommendation**: Consider extracting Step 1 (width resolution, ~230 lines) and Step 2 (height resolution, ~70 lines) into helper functions, but this is low priority since the code is well-commented.

### [LOW] `..Default::default()` usage in `UnifiedConstraints` (lines 489, 520, 824, 866)
- **Location**: `sizing.rs:489`, `sizing.rs:520`, `sizing.rs:824`, `sizing.rs:866`
- **Details**: `UnifiedConstraints` is constructed with `..Default::default()`. In the `#[cfg(not(feature = "text_layout"))]` path, `UnifiedConstraints` is a unit struct (line 192 of `font_traits.rs`), so `Default` is trivial. In the `text_layout` path, the struct comes from the text layout crate. Only `available_width` is set; other fields default. This appears intentional for intrinsic sizing where only the width constraint matters.
- **Recommendation**: No action needed — the defaults are appropriate for the intrinsic sizing context.

### [LOW] Unused imports possible from dead code
- **Location**: Various
- **Details**: Removing the dead functions (`calculate_intrinsic_recursive`, `calculate_node_intrinsic_sizes_stub`, `calculate_inline_intrinsic_sizes`, `debug_log`) may leave unused imports. A compiler pass would catch these.
- **Recommendation**: After removing dead code, run `cargo check` to clean up any resulting unused import warnings.

## System Documentation
- System identified: yes — Layout Solver (CSS sizing/intrinsic size calculation)
- Existing doc: none (no dedicated layout solver guide in `doc/guide/`)
- Doc needed: A `doc/guide/layout-solver.md` explaining the multi-pass layout algorithm (tree construction → intrinsic sizing → used size calculation → positioning → display list), how `solver3/` modules relate to each other, and the key data flow through `LayoutTree`.
