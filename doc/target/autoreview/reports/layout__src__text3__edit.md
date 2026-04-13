# Review: layout/src/text3/edit.rs

## Summary
- Lines: 627
- Public functions: 6 (`edit_text`, `delete_range`, `insert_text`, `delete_backward`, `delete_forward`, `edit_text_multi`, `inspect_delete`)
- Public structs/enums: 1 (`TextEdit`)
- Findings: 1 high, 0 medium, 0 low

## Findings

### [HIGH] Stub — multi-run deletion unimplemented
- **Location**: `edit.rs:166`
- **Details**: `delete_range` only handles deletions within a single run. When `start_run_idx != end_run_idx`, it hits `// TODO: Handle multi-run deletion` and silently returns the unmodified content. This means selecting text across styled run boundaries and pressing Delete/Backspace will appear to do nothing.
- **Evidence**: Line 166: `// TODO: Handle multi-run deletion`. The `else` branch at line 165 is empty.
- **Recommendation**: Implement cross-run deletion: truncate the start run at `start_byte`, truncate the end run from `end_byte`, remove all intermediate runs, and optionally merge the start and end runs if they share styles.

## System Documentation
- System identified: yes — Text Editing / Text Input system (part of the text3 module, handles content mutation via selections)
- Existing doc: none (no guide for text editing/input in `doc/guide/`)
- Doc needed: A guide covering the text editing pipeline: how `text3::edit` pure functions relate to `managers::text_edit`, `managers::text_input`, `core::selection`, and the shell-level keyboard/IME event handling in `shell2::common::event`. Should explain the data flow from keypress to content mutation.
