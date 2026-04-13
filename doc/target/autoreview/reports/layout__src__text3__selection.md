# Review: layout/src/text3/selection.rs

## Summary
- Lines: 204
- Public functions: 2 (`select_word_at_cursor`, `select_paragraph_at_cursor`)
- Public structs/enums: 0
- Findings: 0 high, 1 medium, 0 low

## Findings

### [MEDIUM] Potential Duplication — Two independent word-boundary algorithms
- **Location**: `selection.rs:137` vs `cache.rs:4677,4761`
- **Details**: `find_word_boundaries` in selection.rs uses byte-offset scanning over concatenated text, while `UnifiedLayout::move_cursor_to_prev_word` and `move_cursor_to_next_word` in `cache.rs` use cluster-level iteration (whitespace vs non-whitespace). These are two different word-boundary implementations with different semantics (one is Unicode-alphanumeric-based, the other is whitespace-based). They could diverge in behavior for punctuation like commas and periods.
- **Evidence**: `cache.rs:4709-4714` skips whitespace clusters; `selection.rs:201-202` uses `is_alphanumeric() || '_'`.
- **Recommendation**: Unify the word-boundary definition so double-click selection and Ctrl+Left/Right use consistent boundaries. Consider extracting a shared `is_word_boundary` predicate.


## System Documentation
- System identified: yes — Text Selection (part of the text shaping / editing subsystem)
- Existing doc: none (no `text-selection.md` or `text-editing.md` in `doc/guide/`)
- Doc needed: A guide covering the text selection and editing system — how cursor positioning, word/paragraph selection, and text editing interact with the shaping pipeline (`text3/`). This would cover `selection.rs`, `edit.rs`, and the cursor movement methods in `cache.rs`.
