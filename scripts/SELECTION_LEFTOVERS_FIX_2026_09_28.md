# Selection leftovers fix (2026-09-28)

Branch `wt/selection-leftovers` (from 5414bfa6b, PR #476 base), worktree
`.claude/worktrees/agent-a4815d6fb7aaaa4ec`. **UNCOMPILED** (no cargo, per the
rules). Sources: SELECTION_BUGS_FIX "Still open", SELECTION_NEWTYPES "Not
migrated", SELECTION_ARCHITECTURE_REVIEW §4/§5. Progress checkpoints
(`chore(scripts): checkpoint ...`) are interleaved and carry no code.

## Commits and the REDs they expect (derived, not run)

| # | commit | what |
|---|---|---|
| 1 | 27e6193b9 test | `layout/tests/a_caret_counts_bytes_in_its_own_block.rs` (new, registered) |
| 1 | 9b2b88a5a refactor | `layout/src/block_content.rs`: `BlockContent`, `FlatByte`, `RunByte`; `caret_block_content` -> pub `element_content` |
| 1 | 05327dfc0 test | + `a_line_per_caret_pastes_into_a_list_items_text` |
| 1 | ccd2b9e87 fix | every caret-indexed reader reads its block's content |
| 2 | 0c088f5b2 test | `layout/tests/shift_arrows_extend_a_document_selection.rs` (new) |
| 2 | dc1265537 fix | `extend_document_selection` / `step_document_focus` |
| 3 | 2e13ff81a test | `layout/tests/an_arrow_collapses_a_document_selection_like_a_click.rs` (new) |
| 3 | d84026094 fix | `collapse_document_selection_for_move` -> `open_session` |
| 4 | 3d2b1e2c4 test | `text_beside_a_block_is_selectable.rs` + `a_hit_on_text_beside_a_block_places_the_caret_in_it` |
| 4 | 66444a72f fix | hover click path via `LayoutTree::owning_ifc_root` |
| 6 | 5d7870735 test | `layout/tests/a_screen_reader_reads_a_host_with_paragraphs.rs` (new) |
| 6 | c907240ee fix | `ScopeText` / `AccessibleSelection`; a11y full, incremental, SetTextSelection |
| 7 | 1439f69d0 refactor | e2e `selection_range_info` helper |
| 7 | b38d2fe34 test | e2e `selection_state_tests` (in `layout/src/e2e/full.rs`, feature `e2e-server`) |
| 7 | a17a3b0ea fix | get_selection_state: byte offsets + affinities |
| 7 | 7077ead2a test | dll headless `a_stepped_text_input_...`, `a_stepped_wheel_...` |
| 7 | 43878b997 refactor | `apply_text_input_event` / `apply_wheel_scroll_event` out of `run()` |
| 7 | 598d4cf47 fix | `step()` handles TextInput / Scroll |

Expected REDs, today vs want:

- **N3, list item** (`div[ce] > div.li "alpha"`, caret run 1 byte 2 = "al|pha"):
  `document_caret().text_byte` 5 vs 2; `focused_caret_byte_offset()` None vs
  Some(2); `set_focused_selection_from_byte_range(1,3)` resolves inside the
  marker's clusters, copy != "lp"; `ime_document()` ("alphaZ",(5,6)) vs
  ("alZpha",(2,3)); `ime_surrounding_text()` ("alpha",5) vs ("alpha",2);
  Enter splits at `in_text_child(0,5)` vs `(0,2)`; preedit splice "alpha" vs
  "alZpha", and after a Trailing@4 caret "alpha" vs "alphaZ"; Ctrl+D false vs
  true (2 ranges); paste "X\nY" per caret "alpha" vs "aXlpYha".
- **N3, seats** (`div[ce] > [p "one", p "two"]`, seat 7's caret in "two"):
  typing "x" leaves "two" (edit keyed to the host blob) vs "txwo"; Backspace at
  byte 2 leaves "two" vs "to".
- **Shift+arrow**: focus "sec|ond", Shift+Right copy "rst\nsec" vs "rst\nseco";
  focus at "second"'s start, Shift+Left keeps the document selection vs a range
  "rst" in "first"; caret after "first", Shift+Right no document selection vs
  focus in "second"; Ctrl+Shift+End "rst\nsec" vs "rst\nsecond\nthird". Guard
  (green before/after): Shift+Right inside a paragraph grows its own range.
- **collapse -> open_session**: `tween.focus_scope` stays Some(field) vs the
  paragraph's scope (None).
- **hover click**: synthetic hit on the text leaf "Item" (anonymous block),
  click position (790,590) in no box: `process_mouse_click_for_selection` None
  vs Some, session in "Item"'s block. Premise asserted first: the text leaf
  has its own layout box.
- **N5 a11y** (`div[ce] > [p "one", p "two"]`): SetTextSelection(5,6) on the
  host lands in "one" vs "two" with (1,2); SetTextSelection(1,6) is a range in
  "one" vs a document selection copying "ne\ntw"; the published host node has
  no value / no text selection vs value "one\ntwo", both ends at char 5.
- **E2E**: Cmd+A range Leading@0..Trailing@10 on "hello world" reports end 10
  vs 11; a Trailing@10 caret reports 10 vs 11.
- **headless step()**: TextInput leaves "abc" vs 4 bytes with 'x'; Scroll arms
  no momentum timer and never moves the offset vs armed + moved.

## What changed

- **N3**: one content model per block = `BlockContent` (the edit model behind
  the layout's generated prefix, i.e. the carets' numbering; anonymous blocks
  read the layout's own collection). Rewired: `resolve_cursor_to_text_byte`
  (app DocumentPosition/spans, Enter split via `caret_node_position_for_seat`),
  `byte_offset_of_cursor` (also counts `'\n'` of a preserved newline, as the
  flat text does - review §5 #9a), `ime_document` + `ime_surrounding_text`
  (session block's flat text while the focus holds it - `ime_text_block`),
  `set_focused_selection_from_byte_range` (`BlockContent::caret_at`),
  `spliced_text_with_preedits` (caret's run, affinity resolved),
  `select_next_occurrence`, `paste_one_line_per_caret`, seat Backspace
  (`apply_seat_selection_op`: edit element + block content + marker guard),
  seat typing (keyed to the seat's own caret block, not the primary's; a
  caretless seat types into the node's last block), Wayland surrounding text
  (both fns).
- **N5**: `FlatByte` / `RunByte` newtypes; the one converter pair
  `BlockContent::{flat_byte_of, caret_at}`; `ScopeText` for a node over its
  blocks ('\n' between blocks). The a11y tree publishes the session's editing
  HOST's value (its ScopeText) and selection in it; the incremental update
  publishes on the same node with its a11y children kept (`a11y_children_of`);
  SetTextSelection reads CHARACTER indices in the node's ScopeText, per end,
  cross-block when the ends differ.
- **Review §5 #5 / #7 re-verified: FIXED** by the newtypes branch. #5:
  sessions and `affected_blocks` keyed on `TextBlock`
  (`build_primary_text_selections_map`), paint looks up by `text_block_at`
  (`paint_selections`), the focus path opens via `open_session(block)`
  (`finalize_pending_focus_changes`); guard keyboard_selection_is_painted.rs.
  #7: edits keyed by `edit_element(scope, caret block)`; guard
  typing_into_a_formatted_paragraph.rs. Its seat variant (typing keyed to the
  primary's block) was still live - fixed in ccd2b9e87.

## API changes (none in api.json; all Rust-side)

- New module `azul_layout::block_content`: `FlatByte`, `RunByte`,
  `BlockContent`, `flat_len_of`, `ScopeText`, `AccessibleSelection`.
- New `LayoutWindow` methods: `element_content` (was private
  `caret_block_content`, now returns `BlockContent`), `block_content`,
  `scope_text`, `accessible_selection`, `ime_text_block`.
- Semantics: `byte_offset_of_cursor` always `Some` (clamps a stale caret);
  `ime_document` / `ime_surrounding_text` are the session BLOCK's text (one
  paragraph of a multi-paragraph host); `DocumentPosition.text_byte` and
  document spans are right in list items; a11y `SetTextSelection` offsets are
  character indices (were used as bytes).
- Private: `apply_seat_selection_op` lost its `node_id` param;
  `ifc_local_point_rebased` takes `Option<NodeId>` (unit tests updated);
  headless `apply_text_input_event` / `apply_wheel_scroll_event`.
- E2E JSON `SelectionRangeInfo`: `start` / `end` / `cursor_position` are byte
  offsets in the block's flat text, affinity resolved (were the raw
  `start_byte_in_run`); new `cursor_affinity` / `start_affinity` /
  `end_affinity`. No script in tests/e2e or scripts/e2e-web reads them.
- Test changed: a11y_consumer_contract `a_parked_full_tree_absorbs_...` reads
  the increment's value on the host (CONTAINER), where it is published now.

## Least sure to compile

1. block_content.rs: closure patterns `|(b, _, _)|` over `&&(TextBlock,
   FlatByte, BlockContent)` in `ScopeText::flat_byte_of`; `(entry.1).0`;
   `host.map_or_else(.., EditHost::dom_node)` then `host.is_some()` (relies on
   `Option<EditHost>: Copy`).
2. window.rs `extend_document_selection`: `mc` borrowed while calling
   `self.node_is_self_or_descendant`; `self.text_blocks(.., self.selection_extent(..))`.
3. window.rs SetTextSelection arm: closure `selectable` borrowing `self`
   followed by `self.open_session` (NLL must end the borrow after the `if`).
4. window.rs hover path: `rebased.or_else(|| self.window_point_to_ifc_local(..))`
   while `layout_result` / `tree` borrow `self.layout_results`.
5. window.rs `update_a11y_tree`: `accessible` used in an `and_then` with `?`
   and again after; `a11y_children_of` (`child_nodes`, `is_some_and` with a fn
   path).
6. dll headless: the dedented `apply_wheel_scroll_event` body (moved verbatim);
   `apply_user_change` called from an inherent method (trait imported at the
   module top).
7. layout/src/e2e/full.rs test module: `azul_css::parser2`, `rust_fontconfig`
   under `#[cfg(all(test, feature = "std"))]`.
8. Wayland: `offset` closure capturing `lw` inside the `match` on
   `self.common.layout_window.as_ref()`.

RED premises that may be wrong for a reason other than the bug: the hover
test assumes the text leaf "Item" has its own layout box; the stepped-wheel
test's `delta_y: -120.0` follows viewport_scroll_frame.rs's "notch towards the
user" - flip it if the offset stays 0 with the timer armed.

## Behaviour changes to watch in the battery

- Hover click: inline-box and text-leaf hits now resolve their IFC (were
  skipped, falling to the parent paragraph's hit or the geometric fallback).
- a11y: a host now carries a value + selection; its paragraphs keep their
  labels (a screen reader may read the text twice - check on VoiceOver).
- IME on a multi-paragraph host sees one paragraph.
- Wayland: a range goes out as cursor = its focus, anchor = its start (was
  swapped, review §5 #9d) - platform code, no test.

## Open

- N4 `CaretPos` and N8 one selection store: not done.
- `<br>` is a `LineBreak` run in the layout but nothing in the edit model
  (`get_text_before_textinput`): the same run-number class (a paragraph with
  `<br>` numbers its later text one run lower in the edit model; Enter-split
  child indices skip the `<br>` child).
- `white-space: normal` collapses text in the layout, not in the edit model:
  caret bytes after a collapsed run of spaces are off.
- `shift_carets_across_generation` diffs the text-only edit model: in a list
  item the generation shift targets the wrong run (no RED written).
- callbacks.rs `inspect_delete_changeset` / `inspect_select_all_changeset`
  index the host's content with session carets.
- `TextTarget::caret_at_byte` (cluster walk) is now used only by
  `focused_rect_for_byte_offset` (the other agent's): switch that line to
  `self.block_content(target.block).caret_at(FlatByte(offset))` at merge; then
  `caret_at_byte_in_layout` and `DenseText::byte_offset_to_cursor` may be dead.
- Ctrl+D searches the word minus its last grapheme (raw `start_byte_in_run`
  for a Trailing end): "foo" also matches "fox".
- E2E `get_selection_state` reports only the session's ranges, not a document
  selection; `get_cursor_state.position` is still the raw `start_byte_in_run`
  (its affinity is beside it).
- Shift+Up/Down off a paragraph's last/first line lands on the neighbour's
  first/last caret, not at the same x.
- ScopeText: an inline editing host inside a paragraph reads the whole
  paragraph; a block nested in another (inline-block) comes after its outer
  block, not at its position.
