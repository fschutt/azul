# Review: layout/src/managers/undo_redo.rs

## Summary
- Lines: 303
- Public functions: 11
- Public structs/enums: 4 (`NodeStateSnapshot`, `UndoableOperation`, `NodeUndoRedoStack`, `UndoRedoManager`) + 1 option type + 2 constants
- Findings: 1 high, 2 medium, 1 low

## Findings

### [HIGH] Dead Code — `OptionUndoableOperation` unused in Rust code
- **Location**: `undo_redo.rs:82–87`
- **Details**: `OptionUndoableOperation` is generated via `impl_option!` macro but has no Rust call sites. It only appears in `api.json` (FFI type registry) and the definition itself.
- **Evidence**: Grep for `OptionUndoableOperation` outside api.json and undo_redo.rs — 0 matches.
- **Recommendation**: Keep if needed for FFI/C API generation. Otherwise remove.

### [MEDIUM] Panic Risk — `expect()` on `into_crate_internal()` in three locations
- **Location**: `undo_redo.rs:210`, `undo_redo.rs:282`, `undo_redo.rs:296`
- **Details**: `record_operation`, `push_redo`, and `push_undo` all call `.into_crate_internal().expect(...)` on the changeset target node. If the target node is somehow `None` (e.g. corrupted changeset), this panics. The callers in event.rs already guard against this with `match target.node.into_crate_internal() { Some(id) => ..., None => return }`, so the panic is unlikely but the defense-in-depth is inconsistent.
- **Recommendation**: Return `Result` or `Option` instead of panicking, or at minimum document the precondition.

### [MEDIUM] Missing Documentation — `NodeUndoRedoStack` methods
- **Location**: `undo_redo.rs:95–157`
- **Details**: While the struct itself and most methods have doc comments, the `new` method (line 96) lacks documentation. More importantly, the `#[repr(C)]` on `NodeStateSnapshot` and `UndoableOperation` is not explained — these are presumably for FFI but that's not documented.
- **Recommendation**: Add brief doc noting that `#[repr(C)]` is for FFI compatibility.

### [LOW] `get_stack_mut` is private but only called from `pop_undo`/`pop_redo`
- **Location**: `undo_redo.rs:191–193`
- **Details**: This is fine as-is. `get_stack_mut` is used by `pop_undo` and `pop_redo`.
- **Recommendation**: No action needed.

## System Documentation
- System identified: yes — **Text Editing / Input Management** system (undo/redo is a sub-component of text input handling, alongside `changeset.rs`, `text_input.rs`, `selection.rs`, `clipboard.rs`)
- Existing doc: none — no `doc/guide/` document covers text editing or input management
- Doc needed: A `doc/guide/text-editing.md` covering the text input pipeline: text input manager, changesets, undo/redo, selection, clipboard, and how they integrate with the event loop and callback system.
