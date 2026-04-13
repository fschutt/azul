# Review: layout/src/solver3/cache.rs

## Summary
- Lines: 2453 (233 blank)
- Public functions: 14
- Public structs/enums: 7 (ComputeMode, AvailableWidthType, SizingCacheEntry, LayoutCacheEntry, NodeCache, LayoutCacheMap, LayoutCache, ReconciliationResult)
- Findings: 0 high, 2 medium, 3 low

## Findings

### [MEDIUM] `reconcile_recursive` is 320 lines — consider splitting
- **Location**: `cache.rs:793-1113`
- **Details**: This function handles fingerprinting, node creation/cloning, marker pseudo-element creation, table whitespace filtering, anonymous IFC wrapper creation for mixed block/inline content, subtree hashing, and dirty classification. Several of these are independent phases.
- **Recommendation**: Extract the anonymous-box-wrapping logic (lines 938-1089) into a helper function like `reconcile_mixed_content_children`.

### [MEDIUM] `calculate_layout_for_subtree` is 340 lines
- **Location**: `cache.rs:1680-2019`
- **Details**: This function covers cache lookup (both modes), cache miss path (prepare context, layout, content-height, scrollbar handling, position updates, child processing, out-of-flow processing, cache storage). Already partially factored into helpers, but the cache-hit path alone is ~120 lines.
- **Recommendation**: Extract the cache-hit logic into `apply_cached_layout_result` and `apply_cached_sizing_result` helpers.

### [MEDIUM] Missing docs on `LayoutCache` fields
- **Location**: `cache.rs:341-369`
- **Details**: All fields have doc comments. However, the struct-level doc is minimal ("The persistent cache that holds the layout state between frames."). No mention of the lifecycle (when it's created, when it's cleared, who owns it).
- **Recommendation**: Add a brief lifecycle note to the struct doc.

### [LOW] Hardcoded `LayoutWritingMode::HorizontalTb` in cache hit path
- **Location**: `cache.rs:1774, 2071, 2117`
- **Details**: The cache-hit path in `calculate_layout_for_subtree` and `position_flex_child_descendants` uses `LayoutWritingMode::HorizontalTb` hardcoded when computing inner size, rather than reading the node's actual writing mode. This could produce incorrect results for vertical writing modes.
- **Recommendation**: Read the actual writing mode from the node's computed style instead of hardcoding horizontal-tb.

### [LOW] Indentation inconsistency in anonymous-box creation
- **Location**: `cache.rs:970-1008`
- **Details**: The `else` block starting at line 969 has inconsistent indentation — the `let anon_idx` at line 972 is indented 20 spaces while the enclosing `else` brace is at 24 spaces. The closing `}` at line 1008 is also misaligned.
- **Recommendation**: Fix indentation to be consistent.

### [LOW] `AvailableWidthType` not used outside cache.rs
- **Location**: `cache.rs:99`
- **Details**: `AvailableWidthType` is `pub` but only referenced within `cache.rs` (in `slot_index`). The 9-slot scheme currently only uses slot 0 (both dimensions known) — slots 1-8 are never populated.
- **Evidence**: `Grep("AvailableWidthType") -> 2 files: layout/src/solver3/cache.rs, layout/tests/cache_and_dirty_propagation.rs`. Only used in the test file for unit testing.
- **Recommendation**: Keep for now since it's tested, but note that the multi-slot scheme is only partially implemented (only slot 0 is used at lines 1713 and 2009).

## System Documentation
- System identified: yes — Layout Solver / Incremental Layout Cache
- Existing doc: none (no `doc/guide/layout*.md` or `doc/guide/solver*.md` exists)
- Doc needed: A `doc/guide/layout-solver.md` covering the layout pipeline (reconciliation, dirty propagation, two-pass BFC layout, Taffy-inspired caching, formatting contexts, and how `cache.rs`, `fc.rs`, `mod.rs`, and `sizing.rs` fit together)
