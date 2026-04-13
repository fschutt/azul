# Review: layout/src/solver3/scrollbar.rs

## Summary
- Lines: 241
- Public functions: 2 (`compute_scrollbar_geometry`, `compute_scrollbar_geometry_with_button_size`)
- Public structs/enums: 2 (`ScrollbarRequirements`, `ScrollbarGeometry`)
- Findings: 0 high, 1 medium, 1 low

## Findings

### [MEDIUM] `ScrollbarGeometry::default()` Has Manual Default Impl
- **Location**: `scrollbar.rs:72-87`
- **Details**: The `Default` impl for `ScrollbarGeometry` manually sets all fields to zero/default values. Since `LogicalRect::zero()` returns all zeros and `ScrollbarOrientation::Vertical` is presumably the first variant, this could likely be derived. However, this is a minor style issue — the manual impl is correct and explicit.
- **Recommendation**: Consider `#[derive(Default)]` if `ScrollbarOrientation` and `LogicalRect` both derive `Default` with the same values. Otherwise, the explicit impl is fine.

### [LOW] Doc Comment References Unverified Function Names
- **Location**: `scrollbar.rs:42-46`
- **Details**: The doc comment on `ScrollbarGeometry` references `paint_scrollbars`, `update_scrollbar_transforms`, `hit_test_component`, and `handle_scrollbar_drag`. All four were verified to exist in the codebase:
  - `paint_scrollbars`: `layout/src/solver3/display_list.rs`, `layout/src/window.rs`
  - `update_scrollbar_transforms`: `layout/src/managers/gpu_state.rs`, `dll/src/desktop/wr_translate2.rs`
  - `hit_test_component`: `layout/src/managers/scroll_state.rs`
  - `handle_scrollbar_drag`: `dll/src/desktop/shell2/common/event.rs` and platform modules
- **Recommendation**: No action needed — references are accurate.

## System Documentation
- System identified: yes — Layout Solver / Scrollbar subsystem
- Existing doc: No dedicated guide exists. Scrollbars are mentioned in `css-properties.md` and `lifecycle.md` but there is no `doc/guide/scrollbar.md` or `doc/guide/layout-solver.md`.
- Doc needed: A `doc/guide/layout-solver.md` covering the layout solver pipeline (solver3), including how scrollbar requirements are determined, how geometry is computed (this file), and how it feeds into display list painting, GPU state, and hit-testing.
