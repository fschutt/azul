# Selection live-bug fixes (2026-09-26)

Worktree branch `wt/selection-bugs` (worktree
`.claude/worktrees/agent-a0d97ff9a56c803a7`, based on 97f211f96),
**UNCOMPILED**. Bugs numbered as in scripts/SELECTION_ARCHITECTURE_REVIEW_2026_09_26.md.

```
cd33bf3ee test / 21389dd8c fix   #1 stale document selection
697dd3483 test / b7a90fbbc fix   #2 delete across containers
91df0b0e7 test / 4519ee328 fix   #3 single-block copy
610bdb22a test / c1f74a8ca fix   #4 list-item typing/Backspace
0937d646a refactor(clipboard)    one copy path
47cc89e46 refactor(selection)    Ctrl+A moved into LayoutWindow (behaviour unchanged)
e655aab84 test / 86567911d fix   #6 blank lines
73fb46440 test / be0774a90 fix   #8 block-boundary detection
36d09072f test / eeee3b220 fix   #12/#13 zero-width ranges
aa4e43bed refactor(selection)    cleanup pass
```

## Expected RED at each test commit (derived, not observed)

- #1 `stale_document_selection.rs`: after a click in P2, an arrow key, typing, or Tab into another field, `get_cross_block_selection().is_none()` fails (still Some over nodes [2,4,6]); with the caret in a second field, `get_pending_document_edit().is_none()` fails (Backspace records a ReplaceChildren over the first field).
- #2: `delete_cross_block_selection().is_some()` fails (None). Two existing tests updated: `text_children(&blocks[0])` gives `""` where "first paragraph" / "first PASTEDparagraph" is expected.
- #3 `single_block_copy.rs`: list item copies None instead of Some("lph"); collapsed whitespace copies Some("b") instead of Some("c"); backward selection copies None.
- #4 `list_item_editing.rs`: text stays "alpha" (expected "alphax"); `delete_selection(..).expect` panics on None. The marker test already passes (guards the fix).
- #6 `drag_into_an_empty_line.rs`: a drag from a blank line gives None instead of Some([2,3]); `select_all_text(host)` returns false for blank-last and blank-first documents.
- #8 `block_edge_keys.rs` (via `determine_keyboard_default_action_with_editing`): Trailing@0 decides MergeWithPrevious; Delete at paragraph end decides None instead of MergeWithNext; a backward selection to the start decides MergeWithPrevious.
- #12 `zero_width_selection.rs` (real click + drag): the session holds a Range where a caret is expected; Backspace then returns None.

## Fixes (line numbers at the branch HEAD)

- #1: a new caret ends the document selection (text_edit.rs:964), so does `clear_editing`; the selection-handle grab carries it on purpose; a plain arrow collapses it (`collapse_document_selection_for_move`, window.rs:10623); typing replaces it like paste (window.rs:17035); an edit inside a spanned block ends it (window.rs:17803); Delete/typing act on it only if the focused host contains one of its ends (window.rs:10656).
- #2: the delete is ONE ReplaceChildren on the ends' nearest common ancestor (`document_selection_replacement`, window.rs:4149); the selection is consumed only once the edit exists; `live_subtree` (window.rs:4257) keeps un-synced typing in surviving blocks. Also fixed (not in the review): the merged paragraph was the payload's ROOT but `apply_replace` and the overlay preview read only its children -> bare text after an applied delete.
- #3: single-block copy reads the layout's own runs (`push_layout_range`, window.rs:20869).
- #4: `caret_block_content` (window.rs:3615) puts the list marker back in front of the text so edits use the carets' run numbers; only the text goes to overlay + undo; `reshape_text_node` re-adds the marker (window.rs:17986).
- #6: an empty block ends where it starts (window.rs:3933); `block_edge_caret` (window.rs:10837) falls back to the empty-line caret; Ctrl+A's obsolete single-block fallback deleted.
- #8: `caret_at_block_edges` (window.rs:3691) judges by position in its own block; any real selection = not at a boundary.
- #12/#13: `collapsed_range_caret` (text3/edit.rs:383), used via `same_caret_position` (window.rs:3672) by deletes, `inspect_delete`, the drag path, and the handle-drag "dropped on the anchor" rule.

Cleanup: deleted `extract_clipboard_ranges`, the DOM-walk copy fallback,
`seat_selected_text`'s own extraction loop, `take_cross_block_selection`, the
200-line Ctrl+A arm in dll event.rs; merged reshape's IFC search with the edit
paths' lookup; `sibling_index` / `child_nodes` helpers replace four copies
each. Net source +945/-731; tests +1216.

## Compile / behaviour risk

- Closures borrowing `self` right before a `&mut self` call (`block_end`, `kept`).
- Tuple or-pattern over `&TextEdit` in `apply_edit_to_selection`.
- `content.get(generated..).unwrap_or(&[])`; a `use` inside a closure body.
- dll `SelectAllText` arm borrowing `lw` mutably around `log_debug!`.
- Behaviour changes that may break e2e/JSON scenarios: Delete at a paragraph's end now merges; copies read layout runs (a selection over a list marker copies the marker text); seat copy follows the same path.
- `SelectAllText` NOT ported into the e2e runner (`ce_select_all_ctrl_a`'s reference screenshot may assume it does nothing there).

## Still open

Other code indexing the DOM walk with layout cursors
(`resolve_cursor_to_text_byte`, IME offsets, Enter-split positions, seat
edits); Shift+Arrow over a document selection; the review's bugs #5, #7,
#10, #11 (node-identity resolvers) -> the TextBlock / EditHost / TextTarget
newtype refactor.
