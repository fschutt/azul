# SEL2_SELECTION_LEFTOVERS - progress

Branch `wt/sel2-selection-leftovers` from d240a1b1d. UNCOMPILED (house rule).
List: scripts/SELECTION_LEFTOVERS_FIX_2026_09_28.md "## Open".

## DONE

1. Ctrl+D whole word: 3e5d3a81b test (layout/tests/ctrl_d_searches_for_the_whole_word.rs),
   e01fcf502 fix (`select_next_occurrence`, new `BlockContent::run_range`).

2. Shift+Up/Down column: 6aec11949 test (shift_down_off_a_paragraph_keeps_the_column.rs),
   8acd8daa1 fix (`step_document_focus` VisualLine, `caret_at_column_in`,
   `TextTarget::caret_on_edge_line`).

3. `<br>` in the edit model: 374757648 test (text_after_a_line_break_is_edited_at_its_caret.rs),
   cb21b9d3f fix (`get_text_before_textinput` Br arm, seed before a lone br,
   `caret_at_block_edges` lone-br block is empty, `dom_text_of` br = '\n').
   Behaviour change: Delete before a trailing br deletes the br first.

4. `white-space: normal` collapse: de8cb2949 test (typing_after_collapsed_spaces_lands_at_the_caret.rs),
   3ef1c620d fix (`fc::white_space_runs` split out of `split_text_for_whitespace`; the edit
   model reads normal/nowrap text through it). Behaviour change: an edit stores collapsed text.

5. generation shift in a list item: b1e38aef0 test (a_list_item_caret_moves_with_the_apps_text.rs),
   64d90b739 fix (diff + writer snapshot both read `element_content`).

6. inspect previews: d50ff317a test (unit tests in callbacks.rs), fb68e7516 fix
   (new `LayoutWindow::delete_preview` / `select_all_preview`; the two inspect fns call them).

7. E2E document selection: 2ee3c4465 refactor (`selection_state` fn), e7aa670e5 test
   (selection_state_tests in layout/src/e2e/full.rs), 0f3295a9f fix. (`get_cursor_state.position`
   was already fixed on the base by E1, 7a317019f.)

8. ScopeText: 7201024ab test (a_screen_reader_reads_inline_text_where_it_stands.rs), d6eabfd21 fix
   (ScopeEntry window + nested, `BlockContent::flat_window_of`, `text_block::enclosing_block`,
   accessible_selection host from the caret's text node).

## IN PROGRESS

9. `TextTarget::caret_at_byte` caller switch + dead code.

## NEXT

10. Re-verify review §5 #5 / #7.

## Open questions

(none)
