# Session 8G: Unify Text Deletion with Text Replacement

## Core Idea

Backspace = expand cursor to 1-char selection → replace with empty string.
Uses the SAME changeset path as text input (record_text_input + apply_text_changeset).

## Steps

1. Add `expand_cursor_to_delete_range(content, cursor, forward) -> Option<Selection>` in edit.rs
2. Fix `record_text_input` empty-string guard (allow empty when selection exists)
3. Rewrite `delete_selection` to: expand cursor → set selection → record_text_input("") → apply
4. Remove `TextEdit::DeleteBackward/DeleteForward` enum variants
5. Remove `delete_backward`/`delete_forward` functions (dead code)
6. Keep `inspect_delete_backward`/`inspect_delete_forward` for preview callbacks

## Future: Ctrl+Backspace

Add `word_jump: bool` to `DeleteTextSelection` system change.
Use `move_cursor_to_prev_word`/`move_cursor_to_next_word` for wider selection range.

## Files
- `layout/src/text3/edit.rs` — add expand function, remove old variants
- `layout/src/window.rs` — rewrite delete_selection, fix empty-string guard
- `dll/src/desktop/shell2/common/event.rs` — update dispatch if needed
- `core/src/events.rs` — extend DeleteTextSelection for word_jump
