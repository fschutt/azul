# Review: layout/src/solver3/mod.rs

## Summary
- Lines: 874
- Public functions: 3 (`pos_get`, `pos_set`, `pos_contains`) + 1 gated (`layout_document`)
- Public structs/enums: 2 (`LayoutContext`, `LayoutError`)
- Public type aliases: 1 (`PositionVec`)
- Public constants: 0 (`POSITION_UNSET` is now `pub(crate)`)
- Macros: 10 (`debug_info!`, `debug_warning!`, `debug_error!`, `debug_log!`, `debug_box_props!`, `debug_css_getter!`, `debug_bfc_layout!`, `debug_ifc_layout!`, `debug_table_layout!`, `debug_display_type!`)
- Findings: 0 high, 3 medium, 1 low

## Findings

### [MEDIUM] Thread Safety — global `IfcId` counter with `Relaxed` ordering
- **Location**: `mod.rs:431` (calls `IfcId::reset_counter()` defined in `layout_tree.rs`)
- **Details**: `IfcId` uses a global `static AtomicU32` with `Ordering::Relaxed` for both `store` and `fetch_add`. If layout is ever run concurrently (e.g., two windows in parallel), concurrent resets and increments would produce non-unique IFC IDs, leading to stale cache hits or state corruption. The `T: Sync + 'static` bound on `layout_document` suggests concurrent use was considered.
- **Recommendation**: Use a per-layout-pass counter (e.g., a field on `LayoutContext`) instead of a global static to avoid thread-safety issues entirely.

### [MEDIUM] Module-Level Documentation — incomplete
- **Location**: `mod.rs:1-3`
- **Details**: The module doc says only `//! solver3/mod.rs` and `//! Next-generation CSS layout engine with proper formatting context separation`. It does not describe: the key types/entry points it exports (`LayoutContext`, `layout_document`, `LayoutError`), the submodules and their roles, or how this module fits into the larger crate.
- **Recommendation**: Expand module doc to briefly describe the entry point (`layout_document`), the layout pipeline steps (reconciliation, intrinsic sizing, positioning, display list), the `LayoutContext` struct, and submodule responsibilities.

### [MEDIUM] Refactoring — `layout_document` function is ~360 LOC
- **Location**: `mod.rs:409-771`
- **Details**: The `layout_document` function spans about 360 lines. While the steps are sequential and well-commented, the debug logging blocks (lines 586-612, 647-669) alone account for ~50 lines of verbose formatting that could be extracted into helper functions. The function has 17 parameters.
- **Recommendation**: Extract debug logging blocks into helper functions. Consider grouping related parameters into a config struct to reduce the parameter count.

### [LOW] Verbose Debug Logging in `layout_document`
- **Location**: `mod.rs:586-612`, `mod.rs:647-669`
- **Details**: Two large `if let Some(debug_msgs)` blocks construct multi-line format strings for debug logging. These are inline in the main layout pipeline and account for ~50 lines that could be extracted.
- **Recommendation**: Extract into helper methods (e.g., `log_layout_root_info`, `log_root_position_info`).

## System Documentation
- System identified: **Layout Solver** (CSS layout engine — formatting context separation, intrinsic sizing, positioning, display list generation)
- Existing doc: `doc/guide/architecture.md` (mentions layout at a high level), `doc/guide/lifecycle.md` (lifecycle overview)
- Doc needed: A dedicated `doc/guide/layout-solver.md` explaining the layout pipeline (reconciliation, tree generation, intrinsic sizing, formatting contexts, positioning, display list), the `solver3` module structure and submodule roles, and caching/incremental layout strategy. Related planning docs exist in `scripts/INCREMENTAL_LAYOUT_ARCHITECTURE.md` which could serve as a starting point.
