# Review: layout/src/solver3/geometry.rs

## Summary
- Lines: 666
- Public functions: 0
- Public structs/enums: 9 (`PositionedRectangle`, `EdgeSizes`, `UnresolvedMargin`, `UnresolvedEdge`, `ResolutionParams`, `UnresolvedBoxProps`, `MarginAuto`, `ResolvedBoxProps`, `PackedBoxProps`, `IntrinsicSizes`, `WritingModeContext`)
- Public type aliases: 1 (`BoxProps`)
- Findings: 1 high, 1 medium, 2 low

## Findings

### [HIGH] Dead Code — `MarginAuto` has zero external callers
- **Location**: `geometry.rs:287-293`
- **Details**: `MarginAuto` is only referenced within `geometry.rs` itself (as a field of `ResolvedBoxProps` and `PackedBoxProps`). No code outside this file reads or constructs a `MarginAuto` directly.
- **Evidence**: `grep 'MarginAuto'` across `*.rs` → only `layout/src/solver3/geometry.rs`.
- **Recommendation**: This struct is currently only used as an internal detail. Consider making it `pub(crate)` or verify that auto-margin handling is actually wired up in the layout solver. If the layout solver never checks `margin_auto`, the entire auto-margin tracking is dead.

### [MEDIUM] Duplicated type — `EdgeSizes` vs `ResolvedOffsets`
- **Location**: `geometry.rs:38-43` vs `core/src/ui_solver.rs:72-77`
- **Details**: `EdgeSizes` (top/right/bottom/left: f32) is structurally identical to `ResolvedOffsets` (top/left/right/bottom: f32). Both represent four edge values. `PositionedRectangle` at line 22 even uses `ResolvedOffsets` for its fields while the rest of the file uses `EdgeSizes`. This is confusing.
- **Recommendation**: Consider consolidating. The writing-mode-aware methods (`main_start`, `cross_start`, etc.) could be added as an extension trait or impl on `ResolvedOffsets`.

### [LOW] TODO comment on `BoxProps` type alias
- **Location**: `geometry.rs:392-393`
- **Details**: `/// TODO: Remove this once all code uses ResolvedBoxProps directly.` — `BoxProps` is still used in 5 files. The TODO is still valid but should track whether migration is happening.
- **Recommendation**: Either migrate callers to `ResolvedBoxProps` or accept the alias and remove the TODO.

### [LOW] `PositionedRectangle` uses `ResolvedOffsets` while rest of file uses `EdgeSizes`
- **Location**: `geometry.rs:22-31`
- **Details**: `PositionedRectangle` uses `ResolvedOffsets` for margin/border/padding, while all other types in this file use `EdgeSizes`. This inconsistency is confusing since both types are structurally identical.
- **Recommendation**: Unify on one type, or at minimum add a comment explaining why `PositionedRectangle` uses `ResolvedOffsets`.

## System Documentation
- System identified: **Layout solver** (specifically the box model / geometry subsystem)
- Existing doc: No dedicated layout solver guide exists. `doc/guide/css-properties.md` and `doc/guide/css-styling.md` touch on styling but not the layout algorithm itself.
- Doc needed: A `doc/guide/layout-solver.md` covering the layout pipeline: CSS property resolution → box model construction → formatting contexts → final positioning. This file is the entry point for box model types.
