# SEL2_SELECTION_LEFTOVERS - progress

Branch `wt/sel2-selection-leftovers` from d240a1b1d. UNCOMPILED (house rule).
List: scripts/SELECTION_LEFTOVERS_FIX_2026_09_28.md "## Open".

## DONE

1. Ctrl+D whole word: 3e5d3a81b test (layout/tests/ctrl_d_searches_for_the_whole_word.rs),
   e01fcf502 fix (`select_next_occurrence`, new `BlockContent::run_range`).

2. Shift+Up/Down column: 6aec11949 test (shift_down_off_a_paragraph_keeps_the_column.rs),
   8acd8daa1 fix (`step_document_focus` VisualLine, `caret_at_column_in`,
   `TextTarget::caret_on_edge_line`).

## IN PROGRESS

3. `<br>` in the edit model (run numbers, Enter-split child indices).

## NEXT

4. `white-space: normal` collapse in the edit model.
5. `shift_carets_across_generation` in a list item.
6. `inspect_delete_changeset` / `inspect_select_all_changeset` with session carets.
7. E2E `get_selection_state` reporting a document selection.
8. ScopeText inline host / nested block order.
9. `TextTarget::caret_at_byte` caller switch + dead code.
10. Re-verify review §5 #5 / #7.

## Open questions

(none)
