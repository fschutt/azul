# Selection newtypes refactor (2026-09-26)

Worktree branch `wt/selection-newtypes` (from `wt/selection-bugs` aa4e43bed,
worktree `.claude/worktrees/agent-ad4a14f3ed7e90040`), 33 commits,
**UNCOMPILED**. Implements scripts/SELECTION_ARCHITECTURE_REVIEW_2026_09_26.md §4.

```
1c2a5890f test  keyboard selection after keyboard focus is painted (#5)
753802103 test  typing after keyboard focus lands in a formatted paragraph (#7)
7bfcc1c2e refactor TextBlock names an IFC, by one rule
6e9970b7b fix   focus opens its session on the caret's text block
72a13a864 refactor sessions/selections keyed on TextBlock
3265a1e07 refactor smart paste lives in LayoutWindow
5f5f0f8a6 refactor EditHost is the host; edits keyed to the block
4fd0033b3 refactor TextTarget is the one resolution
0d05dc4a2 refactor one document-order walk, BlockFilter
154b3932e test  text beside a block (anonymous IFC) (#10)
48a72db4b fix   anonymous blocks selectable
5f34c2df4 test  typing beside a block never rewrites paragraph below
6d5113adb fix   edit in anonymous block refused
1d930dc1d test  selection covers only selectable text (#11)
75a5ddfa2 fix   SELECTABLE filter + selection_extent
e007a124a refactor one byte-offset->caret converter (TextTarget::caret_at_byte)
72216f622 refactor one way to open a session (open_session)
98923404e test  carets set from outside land where they say
478fddacf fix   byte offset 0 is Leading
3a60f8af5 fix   a11y SetTextSelection
7ce1f2d35 fix   app AddCursor/AddSelectionRange
6e79b7c66 test  Ctrl+A selects whole host
42306cc61 fix   Ctrl+A via document walk (select_all_extent)
d17b0d714 refactor app MoveCursor* in LayoutWindow (dll+runner deduped)
cf3d914b3 test  app caret moves = their keys
6450a0499 fix   app moves via apply_selection_op
39d7041cf refactor app SetSelection in LayoutWindow
0220d3667 test  app SetSelection/MoveCursor land in their node
33e2612ec fix   same
ace0e2872 test  inspect_move_cursor_* previews (unit tests in callbacks.rs)
d1c8f1ce8 fix   previews via LayoutWindow::caret_after_step
441c15f24 test  restored caret lands in its text node
6757b1db5 fix   same
```

## Expected REDs (derived)

- #5 1c2a5890f: `selection_rects > 0` fails with 0.
- #7 753802103: text "Hello worldx"/"Hello worl" instead of "Hello world"; debug panic.
- #10 154b3932e: click `.expect` panics on None; drag focus not in the anonymous block; copy Some("one\nsub") vs Some("one\nItem\nsub"); 2 highlight bands vs 3.
- 5f34c2df4: paragraph shows the host's edited text instead of "para".
- #11 1d930dc1d: copy "one\nlabel\ntwo" vs "one\ntwo"; drag focus lands in the label; Ctrl+A copies "label\ntext" vs "text"; a drag from the field makes a cross-block selection.
- 98923404e: byte 0 reads back Some(1) vs Some(0) (rects for bytes 0 and 1 equal); a11y SetTextSelection range (1,1) vs (1,4), wrong block's session, no session opened when none, moves the caret on user-select:none; app AddCursor/AddSelectionRange leave the session in "one"; app-opened session key Some(0) vs host key.
- 6e79b7c66: copy Some("sub") vs Some("Item\nsub"), Some("alph") vs Some("beta").
- cf3d914b3: MoveCursorRight / DocumentEnd / LineEnd+extend / DocumentEnd on the host give Some(0), Some(0), Some((0,0)), Some(0) vs 1, 5, (0,5), 5 (dense text path: the stored sparse layout is an empty placeholder).
- 0220d3667: set_app_selection / handle_cursor_movement leave the block "one"; with no session set_app_selection returns false.
- ace0e2872: inspect right None; Ctrl+End Leading@(0,len) vs Trailing on the last cluster; line end on the host None.
- 441c15f24: restored caret Leading@(0,1) vs Leading@(run of " two", 1).
- Guards (green before and after): an_app_cursor_naming_the_host_joins_its_session, an_apps_move_in_another_paragraph_leaves_the_caret_alone, an_app_selection_naming_the_host_sets_its_session, inspect_naming_another_paragraph_previews_nothing.

## Newtypes

- `TextBlock`: replaces bare NodeIds in sessions, selections, anchor/focus, affected blocks, paint, seats (core/selection.rs 30, text_edit.rs 15, layout_tree.rs 11, text_block.rs 20, window.rs 29 uses).
- `EditHost`: `caret_text_target` deleted.
- `TextTarget`: one resolution for click, drag, keyboard, Ctrl+A, focus seed, IME/a11y byte offsets, app moves and previews.
- `open_session`: one way to open a session.
- Removed duplicates: `select_all_blocks`, `move_cursor_in_node`, static `byte_offset_to_cursor`, `CallbackInfo::get_inline_layout_for_node`, 8 MoveCursor* arms + SetSelection/SetSelectAllRange written twice (dll + runner).

## Not migrated

FFI shapes (`SelectionState`, `DocumentPosition`/spans, `SystemChange` targets);
the seat's `node` beside `block`; `ifc_candidate_children` BFS; the B-space
content model (N3: `resolve_cursor_to_text_byte`, `byte_offset_of_cursor` still
index B-space with A-space cursors - hits `::marker` list items); `CaretPos`
(N4), offset newtypes (N5), single selection store (N8); hover click path for
anonymous membership roots; `collapse_document_selection_for_move` still calls
`initialize_editing` directly; a11y offsets on a multi-block host resolve
against its first block only (needs N5).

## Least sure to compile

text_block.rs: or-pattern in `apply_app_cursor_move`; `nested` closure in
`select_all_extent` (`let (last, _) = *roots.iter().rev().find(...)?`);
`alloc::collections::BTreeSet` import; `impl FnOnce(&mut MultiCursorState)` in
`add_app_selection`; `is_some_and` in a match guard in `caret_at_node_byte`;
manual `Ord` on `TextBlock`; `.map(EditHost::dom_node)`. window.rs:
`pub(crate) contenteditable_session_key` from text_block.rs;
`names_session_block`; two-phase borrows in tests; the seat-undo closure in
dll event.rs. Tests: callbacks.rs helpers; `restored_caret_lands_in_its_text.rs`
(`DocumentChangeset`, `DocOpRemoveChildren`, `U32Vec: From<Vec<u32>>`).

Behaviour changes: `apply_selection_op` Move/Extend returns false when the
resolved block is not the session's (incl. no session); Ctrl+A on a one-block
host with no session now opens one.
