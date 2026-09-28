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
- Task 4 (hover click, anonymous roots):
  - 3d2b1e2c4 test appended to `layout/tests/text_beside_a_block_is_selectable.rs`
    (`a_hit_on_text_beside_a_block_places_the_caret_in_it`; premise: the text
    leaf has its own layout box).
  - 66444a72f fix: hover path via `owning_ifc_root`; `ifc_local_point_rebased`
    takes `Option<NodeId>`; unpositioned hit -> `window_point_to_ifc_local`.
- Task 5 (verify review #5, #7): VERIFIED FIXED, no commit. #5: sessions and
  `affected_blocks` keyed on `TextBlock` (text_edit.rs
  `build_primary_text_selections_map`), paint looks the range up by
  `tree.text_block_at` (display_list.rs `paint_selections`), focus path opens
  its session via `open_session(block)` (window.rs
  `finalize_pending_focus_changes`); guard keyboard_selection_is_painted.rs.
  #7: edits keyed by `edit_element(scope, caret block)` = the block's
  element; guard typing_into_a_formatted_paragraph.rs. Seat variant of #7
  (typing keyed to the PRIMARY's block) fixed in ccd2b9e87.
- Task 6 (N5 a11y):
  - 5d7870735 test: `layout/tests/a_screen_reader_reads_a_host_with_paragraphs.rs`.
  - c907240ee fix: `ScopeText`, `AccessibleSelection`, `scope_text`,
    `accessible_selection` (block_content.rs); a11y full + incremental +
    SetTextSelection through them; `a11y_children_of`; contract test reads the
    increment on the host (CONTAINER).
  - N4 (CaretPos) and N8 (one store): NOT done - list as open.
- Task 7a (E2E get_selection_state):
  - 1439f69d0 refactor: `selection_range_info(lw, block, &Selection)` in
    layout/src/e2e/full.rs (params `_lw`, `_block` unused until the fix).
  - b38d2fe34 test: `selection_state_tests` at the end of e2e/full.rs.
  - a17a3b0ea fix: byte offsets via `byte_offset_of_cursor` + `*_affinity`.

- Task 7b (headless step):
  - 7077ead2a test: two tests before `damage_mouse_move_no_change_is_clean`
    in dll/src/desktop/shell2/headless/mod.rs (+ `harness_layout_editable`).

## IN PROGRESS

7b fix: extract `run()`'s TextInput / Scroll arms into HeadlessWindow methods
(refactor commit), then `step()` calls them (fix commit).

## NEXT (in order)

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
