# Selection leftovers - progress checkpoint (2026-09-28)

Branch `wt/selection-leftovers` (from 5414bfa6b). NO compilation. RED first.
Final report goes to `scripts/SELECTION_LEFTOVERS_FIX_2026_09_28.md`; this
file is deleted in that last commit.

## DONE

- Task 1 (N3), part 1:
  - 27e6193b9 test: `layout/tests/a_caret_counts_bytes_in_its_own_block.rs`
    (registered in all.rs) - list-item carets (app `document_caret`, IME
    `focused_caret_byte_offset`, `set_focused_selection_from_byte_range`,
    `ime_document`, `ime_surrounding_text`, Enter split, preedit splice incl.
    Trailing affinity, Ctrl+D) + seat typing/Backspace in a two-paragraph host.
  - 9b2b88a5a refactor: new `layout/src/block_content.rs` (`FlatByte`,
    `RunByte`, `BlockContent` with `flat_byte_of` / `caret_at` /
    `run_byte_of` / `past_generated` / `selections_past_generated`,
    `flat_len_of`, `LayoutWindow::block_content(TextBlock)`);
    `caret_block_content` -> pub `element_content` returning `BlockContent`.
  - 05327dfc0 test: `a_line_per_caret_pastes_into_a_list_items_text`.

  - ccd2b9e87 fix: every caret-indexed reader reads its block's content
    (resolve_cursor_to_text_byte, byte_offset_of_cursor, ime_text_block +
    ime_document, ime_surrounding_text, set_focused_selection_from_byte_range,
    seat Backspace + seat typing block, Ctrl+D, paste per caret, preedit
    splice with affinity, Wayland surrounding text + cursor/anchor swap).
- Task 2 (Shift+Arrow):
  - 0c088f5b2 test: `layout/tests/shift_arrows_extend_a_document_selection.rs`
    (registered) - 4 REDs + 1 guard.
  - dc1265537 fix: `extend_document_selection` + `step_document_focus` in
    window.rs, hooked in `apply_selection_op_for_seat` (primary, Extend).
- Task 3 (collapse -> open_session):
  - 2e13ff81a test: `layout/tests/an_arrow_collapses_a_document_selection_like_a_click.rs`
  - d84026094 fix.

## IN PROGRESS

(nothing uncommitted)

## NEXT (in order)

4. Task 4: RED (synthetic hover hit on the text leaf "Item" beside a block,
   click `position` far away) + fix: hover path resolves via
   `LayoutTree::owning_ifc_root` + `text_target_at_layout_index`; anonymous
   root -> no own scroll; unpositioned hit -> `window_point_to_ifc_local` of
   `point_in_viewport`.
5. Task 5: re-verify review #5 (keyboard_selection_is_painted.rs, sessions
   keyed on TextBlock, paint looks up by block) and #7
   (typing_into_a_formatted_paragraph.rs, `edit_element`) - both look FIXED;
   the seat variant of #7 is fixed by the N3 commit. Report only.
6. Task 6 (N5): a11y offsets in a host's flat text: new scope text (blocks of
   the host joined by '\n'), `update_a11y_tree` + `update_a11y_tree_incremental`
   publish host value + selection in it; `SetTextSelection` maps char index ->
   FlatByte -> (block, caret) per end, cross-block when the ends differ. RED
   with `div[ce] > [p "one", p "two"]`.
7. Task 7: E2E `get_selection_state` affinity + byte offsets (layout/src/e2e/full.rs);
   dll headless test `step()` handles Scroll/TextInput via helpers shared with
   `run()` (dll/src/desktop/shell2/headless/mod.rs).
8. Report + delete this file.

## Open questions / items to list in the report

- `<br>` is a LineBreak run in the layout but nothing in the edit model
  (`get_text_before_textinput`): same run-number class, not fixed.
- Collapsed whitespace (`white-space: normal`): layout bytes vs raw DOM bytes.
- `shift_carets_across_generation` diffs the text-only edit model (runs off
  by one in a list item) - no RED written.
- callbacks.rs `inspect_delete_changeset` / `inspect_select_all_changeset`
  index the host's content with session carets.
- `TextTarget::caret_at_byte` (cluster walk) is left for the other agent's
  `focused_rect_for_byte_offset`; switch it to `BlockContent::caret_at` at merge.
- Ctrl+D searches the word minus its last grapheme (raw `start_byte_in_run`).
