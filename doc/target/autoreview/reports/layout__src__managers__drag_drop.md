# Review: layout/src/managers/drag_drop.rs

## Summary
- Lines: 119
- Public functions: 6 (on `DragDropManager`) + 1 (`DragState::from_context`)
- Public structs/enums: 4 (`DragType`, `DragState`, `OptionDragState`, `DragDropManager`)
- Findings: 2 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — `DragType` enum has zero external call sites
- **Location**: `drag_drop.rs:14-21`
- **Details**: The `DragType` enum is defined and used inside `DragState` but is never pattern-matched or referenced outside this file. Grep for `DragType::` across `*.rs` files returns only hits in `drag_drop.rs` itself and `core/src/drag.rs` (which defines its own `ActiveDragType`). The `DragState` struct that contains it is only used via `get_drag_state()` in `callbacks.rs:3220`, but the caller never inspects the `drag_type` field.
- **Evidence**: `Grep pattern="DragType::" glob="*.rs"` — only matches in `drag_drop.rs` and `core/src/drag.rs` (the latter is a different type `ActiveDragType`).
- **Recommendation**: Remove `DragType` if the backwards-compat API is not actually consumed externally, or document what external consumer needs it.

### [HIGH] Dead Code — `OptionDragState` has zero external call sites
- **Location**: `drag_drop.rs:61-66`
- **Details**: `OptionDragState` (generated via `impl_option!`) is never referenced outside this file. Grep for `OptionDragState` across `*.rs` returns only `drag_drop.rs:63` and `api.json` / `examples/c/drag-drop-test.c`.
- **Evidence**: `Grep pattern="OptionDragState" glob="*.rs"` — only `drag_drop.rs:63`.
- **Recommendation**: Remove unless needed for the C API in `api.json`. If kept for FFI, add a comment explaining why.

### [MEDIUM] Duplicated Functionality — `DragDropManager` largely duplicates `GestureAndDragManager`
- **Location**: `drag_drop.rs:72-147` vs `gesture.rs:1109-1260`
- **Details**: Both managers wrap an `Option<DragContext>` and provide nearly identical methods: `get_drag_context`, `end_drag`, `cancel_drag`, `is_dragging`. The callbacks layer (`callbacks.rs:3198-3227`) explicitly checks `gesture_drag_manager` first, then falls back to `drag_drop_manager`. The module doc says "kept for backwards compatibility only" but the duplication creates confusion and potential state drift (two copies of drag context in the same `LayoutWindow`).
- **Recommendation**: Consider merging `DragDropManager` into `GestureAndDragManager` or reducing it to a type alias / thin wrapper. The `event.rs` code already clones drag context between the two managers.

### [LOW] File size — small but justified
- **Location**: entire file (148 lines)
- **Details**: The file is relatively small but self-contained as a backwards-compat shim. At 148 lines it doesn't warrant merging with another module.
- **Recommendation**: No action needed on file size.

## System Documentation
- System identified: yes — drag-and-drop / gesture system
- Existing doc: none (no `doc/guide/drag-drop.md` or `doc/guide/gestures.md`)
- Doc needed: A guide covering the drag-and-drop architecture would be valuable, especially explaining the relationship between `DragDropManager` (legacy), `GestureAndDragManager` (primary), `DragContext` (core type), and `FileDropManager` (OS file hover tracking). The `scripts/DRAG_DROP_REPORT.md` file exists but is a planning document, not user-facing documentation.
