# Review: layout/src/managers/file_drop.rs

## Summary
- Lines: 50
- Public functions: 5 (`new`, `set_hovered_file`, `get_hovered_file`, `get_dropped_file`, `set_dropped_file`)
- Public structs/enums: 1 (`FileDropManager`)
- Findings: 1 high, 0 medium, 0 low

## Findings

### [HIGH] Dead Code — `set_hovered_file` never called
- **Location**: `file_drop.rs:32` (`set_hovered_file`)
- **Details**: `set_hovered_file` has zero call sites outside its definition. The `hovered_file` field is read via `get_hovered_file()` at `event_determination.rs` but never set, meaning file-hover events can never fire.
- **Evidence**: Grep for `set_hovered_file` returns only the definition.
- **Recommendation**: Wire `set_hovered_file` into the platform backends (Windows `WM_DROPFILES` enter/leave, macOS `draggingEntered`/`draggingExited`) so `FileHover` events actually fire.

## System Documentation
- System identified: yes — event/input handling system (file drag-and-drop subsystem)
- Existing doc: none (no `doc/guide/` document for drag-and-drop or input handling)
- Doc needed: A guide covering the input/event handling system including drag-and-drop, file drop, hover management, and how platform events flow into `FileDropManager` and then into `SyntheticEvent` generation via `event_determination.rs`.
