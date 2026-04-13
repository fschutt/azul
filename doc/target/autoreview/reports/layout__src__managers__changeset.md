# Review: layout/src/managers/changeset.rs

## Summary
- Lines: 527
- Public functions: 4 free functions, 6 methods on `TextChangeset`
- Public structs/enums: 12 structs, 2 enums, 1 type alias, 1 re-export
- Findings: 4 high, 1 medium, 1 low

## Findings

### [HIGH] Dead Code — Four public free functions have zero call sites

- **Location**: `changeset.rs:287` (`create_copy_changeset`), `changeset.rs:311` (`create_cut_changeset`), `changeset.rs:331` (`create_select_all_changeset`), `changeset.rs:395` (`create_delete_selection_changeset`)
- **Details**: None of these four public functions are called from any other file in the codebase. (`create_paste_changeset` was removed as a non-functional stub.)
- **Evidence**: `grep -r 'create_copy_changeset\|create_cut_changeset\|create_select_all_changeset\|create_delete_selection_changeset'` excluding `changeset.rs` returns zero matches.
- **Recommendation**: Either wire these into the event/window code paths, or remove them if the changeset approach is not yet in use.

### [HIGH] Dead Code — All `TextChangeset` methods have zero external call sites

- **Location**: `changeset.rs:220` (`mutates_text`), `changeset.rs:232` (`changes_selection`), `changeset.rs:244` (`uses_clipboard`), `changeset.rs:252` (`resulting_cursor_position`), `changeset.rs:265` (`resulting_selection_range`)
- **Details**: These five public methods are never called from outside this file. The types `TextChangeset` and `TextOperation` are imported in `window.rs`, `undo_redo.rs`, and `event.rs`, but only the struct fields / enum variants are used — none of these methods.
- **Evidence**: `grep -r '\.mutates_text\|\.changes_selection\|\.uses_clipboard\|\.resulting_cursor_position\|\.resulting_selection_range'` returns zero matches outside `changeset.rs`.
- **Recommendation**: Verify these are needed by the undo/redo or event systems; remove if unused.

### [HIGH] Stub Code — Delete changeset never extracts selected text

- **Location**: `changeset.rs:457-458`
- **Details**: When deleting a selection range, the deleted text is hardcoded to an empty string: `let deleted = String::new(); // Placeholder`. The TODO comment at line 457 confirms this: `// TODO: Actually extract text between range.start and range.end`. This means undo/redo for selection deletion would have no text to restore.
- **Recommendation**: Implement actual text extraction from the selection range.

### [HIGH] Bug — `CursorPosition::Uninitialized` used as post-operation cursor

- **Location**: `changeset.rs:336`, `changeset.rs:516`
- **Details**: Both `create_cut_changeset` and `create_delete_selection_changeset` set the resulting cursor position to `CursorPosition::Uninitialized`. Comments say "SelectionManager will map this to physical coordinates" but this is not a sentinel that downstream code is guaranteed to handle — other usages of `Uninitialized` in the codebase (e.g., `wr_translate2.rs:614`, `windows/mod.rs:2379`) map it to `(0.0, 0.0)` or skip rendering. If these changesets were applied, the cursor would jump to an undefined position.
- **Recommendation**: Compute the actual cursor position (start of deleted range) instead of using `Uninitialized`.

### [MEDIUM] Bug-Prone — Single-byte character assumption in delete logic

- **Location**: `changeset.rs:474`, `changeset.rs:494`
- **Details**: `byte_pos + 1` and `byte_pos.saturating_sub(1)` assume single-byte characters when computing deletion ranges. The code works with `GraphemeClusterId` and byte offsets into UTF-8 text — deleting one byte forward/backward on a multi-byte character (e.g., emoji, CJK) will produce an invalid UTF-8 range and potentially panic on `text[byte_pos..end_pos]`.
- **Recommendation**: Use proper grapheme cluster boundary detection (the `unicode-segmentation` crate or the project's existing text shaping infrastructure) instead of `+1`/`-1` byte arithmetic.

### [LOW] Hardcoded `source_run: 0` in select-all

- **Location**: `changeset.rs:393`, `changeset.rs:401`
- **Details**: `create_select_all_changeset` hardcodes `source_run: 0` for both start and end cursors. If the text node contains multiple inline runs (e.g., mixed styled text), the end cursor should reference the last run, not run 0.
- **Recommendation**: Determine the correct run index from the inline content structure.

## System Documentation
- System identified: yes — text editing / changeset system (part of the broader text input, selection, and undo/redo pipeline)
- Existing doc: none (no guide for the text editing / undo-redo / changeset system in `doc/guide/`)
- Doc needed: A guide explaining the text editing pipeline: how `text_input`, `changeset`, `undo_redo`, `selection`, and `focus_cursor` managers interact, and how they integrate with `window.rs` and the event system.
