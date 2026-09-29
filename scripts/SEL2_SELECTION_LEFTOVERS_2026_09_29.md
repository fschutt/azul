# SEL2: selection / caret leftovers (2026-09-29)

Branch `wt/sel2-selection-leftovers` from `d240a1b1d`. **UNCOMPILED** (house
rule): every RED below is derived from reading the code, not observed. The
source list is scripts/SELECTION_LEFTOVERS_FIX_2026_09_28.md "## Open".

## What was built

| # | bug | RED test | fix |
|---|---|---|---|
| 1 | Ctrl+D searched the word minus its last grapheme | `layout/tests/ctrl_d_searches_for_the_whole_word.rs` | `select_next_occurrence` reads the block content in the carets' numbering, affinity resolved; new `BlockContent::run_range` mints an occurrence like a word range; searches on after the last selection (after the word for a caret), wraps to the first free occurrence |
| 2 | Shift+Up/Down off a paragraph's edge line lost the column | `layout/tests/shift_down_off_a_paragraph_keeps_the_column.rs` | `step_document_focus`: a `VisualLine` step lands in the neighbour's first/last line at the on-screen column (`LayoutWindow::caret_at_column_in`, new `TextTarget::caret_on_edge_line`) |
| 3 | `<br>` missing from the edit model | `layout/tests/text_after_a_line_break_is_edited_at_its_caret.rs` | `get_text_before_textinput` gives `Br` a hard `LineBreak` item; the keystroke seed goes in front of a lone `<br>`; `caret_at_block_edges` treats a block whose one item is a line break as empty; `overlay::dom_text_of` flattens a direct `<br>` to `'\n'` |
| 4 | `white-space: normal` not collapsed in the edit model | `layout/tests/typing_after_collapsed_spaces_lands_at_the_caret.rs` | `solver3::fc::split_text_for_whitespace` split into the new `fc::white_space_runs` (Phase I) + its text-transform; the edit model reads normal/nowrap text through `white_space_runs` (pre modes unchanged) |
| 5 | generation shift in a list item | `layout/tests/a_list_item_caret_moves_with_the_apps_text.rs` | `shift_carets_across_generation` diffs `element_content` (marker prefix in front); the overlay writer snapshots the same numbering |
| 6 | `inspect_delete_changeset` / `inspect_select_all_changeset` indexed the host with session carets | unit tests in `layout/src/callbacks.rs` (`inspect_backspace_in_the_second_paragraph_...`, `..._in_a_list_item_...`, `inspect_select_all_previews_every_paragraph_of_the_host`) | new `LayoutWindow::delete_preview` / `select_all_preview` (same resolution as `delete_selection` / `select_all_text`); the two inspect fns are one call each |
| 7 | E2E `get_selection_state` ignored a document selection | `selection_state_tests::a_document_selection_is_reported_block_by_block` in `layout/src/e2e/full.rs` | handler body moved to a pure `selection_state` (refactor commit), which reports a document selection one entry per block, type `"block"` |
| 8 | ScopeText: inline host read the whole paragraph; nested block read after its outer one | `layout/tests/a_screen_reader_reads_inline_text_where_it_stands.rs` | `ScopeText` entries carry a window of their block's flat text and a `nested` flag; new `BlockContent::flat_window_of`; `accessible_selection` finds the host from the caret's own text node; the layout-ancestor walk is now `text_block::enclosing_block` (also used by `select_all_extent`) |
| 9 | `TextTarget::caret_at_byte` | - | already gone on the base (`focused_rect_for_byte_offset` reads `block_content(..).caret_at`); the left-over `DenseText::byte_offset_to_cursor` had no product caller: deleted with its equivalence check |
| 10 | review §5 #5 / #7 | - | re-verified FIXED on the base, see below |

`get_cursor_state.position` (also on the Open list) was already fixed on the
base by E1 (7a317019f).

## Commits

```
f60822098 chore  progress
3e5d3a81b test   Ctrl+D searches for the whole word (RED)
e01fcf502 fix    Ctrl+D searches for the whole word
6aec11949 test   Shift+Up/Down off a paragraph keeps the column (RED)
8acd8daa1 fix    Shift+Up/Down off a paragraph keeps the column
374757648 test   text after a <br> is edited at its caret (RED)
cb21b9d3f fix    a <br> is a line break in the edit model
de8cb2949 test   typing after collapsed spaces lands at the caret (RED)
3ef1c620d fix    the edit model collapses white-space: normal text
b1e38aef0 test   a list item caret moves with the app's text (RED)
64d90b739 fix    the generation diff reads the carets' numbering
d50ff317a test   delete / select-all previews read the caret's block (RED)
fb68e7516 fix    delete / select-all previews read the caret's block
2ee3c4465 refactor get_selection_state's response is built by selection_state
e7aa670e5 test   get_selection_state reports a document selection (RED)
0f3295a9f fix    get_selection_state reports a document selection
7201024ab test   a screen reader reads inline text where it stands (RED)
d6eabfd21 fix    a screen reader reads inline text where it stands
71bd030cf refactor delete DenseText::byte_offset_to_cursor
```
(`chore(scripts): SEL2 progress checkpoint` commits interleaved, no code.)

## Expected REDs (derived)

- Ctrl+D: `[(0,3),(4,7)]` vs `[(0,3),(8,11)]` ("fo" found "fox"); caret at the word start `[(0,3)]` vs `[(0,3),(8,11)]`; second Ctrl+D `[(0,3),(4,7),(8,11)]` vs `[(0,3),(8,11),(12,15)]`.
- Shift+Down from "abc|def": copy `"def\n"` vs `"def\nabc"`; Shift+Up `"\nabc"` vs `"def\nabc"`.
- `<br>`: typing at "t|wo" panics in the `insert missed every selection` debug_assert vs `"one\ntxwo"`; Enter `in_text_child(1, ..)` vs `(2, 1)`; `ime_surrounding_text` `("onetwo", 6)` vs `("one\ntwo", 5)`. Guards (green both): a lone-`<br>` paragraph takes a keystroke; Delete in it merges the next paragraph.
- collapse: `"a bx c"` vs `"a b cx"`; IME split `("a b", "c")` vs `("a b c", "")`.
- generation: caret stays `(1, 3)` vs `(1, 5)` after the app renders "XXalpha".
- inspect: `Some("n")` vs `Some("w")`; list item `None` vs `Some("l")`; select-all `("onetwo", run0 L@0..L@6)` vs `("one\ntwo", first..last caret)`.
- E2E: one "cursor" entry vs two "block" entries (node 2 bytes 1..3, node 4 bytes 0..2).
- ScopeText: `"Name: Bob"` vs `"Bob"`; accessible selection on the paragraph at 7 vs the span at 1; caret_at(2) in "Name: " vs "Bo|b"; host with an inline-block `"a inner b\ninner"` vs `"a inner b"`, caret "in|ner" at 12 vs 4, caret_at(4) in the paragraph vs the inline-block's block.

Premises that could be wrong for a reason other than the bug (each asserted
in its test with a "premise:" message): the layout numbers
`p > ["one", br, "two"]` as runs 0, 1, 2 (cross_block_selection.rs has an old
note that a LEADING `<br>` did not diverge - measured before fc had its `Br`
arms); `p > br` alone is a text block with a caret; the inline-block is a text
block of its own; a `layout_new_generation` keeps the session.

## Behaviour changes to watch in the battery

- Edit model now includes `<br>` as `'\n'`: `get_text_before_textinput` / the app's `TextChanged` text / `unsynced_text_edits` of a paragraph with a `<br>` carry `'\n'`. Delete before a TRAILING `<br>` (`p > ["one", br]`) deletes the `<br>` first (one more press to merge).
- `white-space: normal` / `nowrap` edits store the collapsed text (runs of spaces as one, a segment break as a space); an empty normal text node is no item (was an empty run). TextInput (pre) / TextArea (pre-wrap) are unaffected.
- Ctrl+D ranges end `Trailing` on the match's last grapheme (were `Trailing` on the byte after it).
- `get_selection_state` JSON: with a document selection, one entry per block, `selection_type: "block"`; `selection_count` counts entries.
- a11y: an inline editing host publishes its own text as its value; a caret in it is published on the span.

## api.json

No signature changes. Doc-comment changes on two exported methods (for the
autofix to pick up if it mirrors docs): `CallbackInfo::inspect_select_all_changeset`,
`CallbackInfo::inspect_delete_changeset`. `DenseText::byte_offset_to_cursor`
(deleted) is not in api.json. New Rust-side pub API (not FFI):
`BlockContent::run_range`, `BlockContent::flat_window_of`,
`TextTarget::caret_on_edge_line`, `LayoutWindow::delete_preview`,
`LayoutWindow::select_all_preview`, `solver3::fc::white_space_runs`.

## Files outside my lane

- `layout/src/callbacks.rs`: only the two inspect fns and new unit tests (not `set_node_inline_style`).
- `layout/src/e2e/full.rs`: the `GetSelectionState` handler, `selection_state`, the response docs and `selection_state_tests` - no builder op.
- `layout/src/solver3/fc.rs`: `split_text_for_whitespace` split in two, body unchanged.
- No widget / theme file touched.

## Least sure to compile

1. `window.rs select_next_occurrence`: `mc` (shared borrow of `text_edit_manager`) held across `self.block_inline_layout` / `self.block_content`; the `occurrences` closure returning `text.match_indices(search_text).map(..)`.
2. `window.rs caret_at_block_edges`: `blank = |items| items.iter().all(empty)` passing the (capture-less, Copy) closure `empty` by value, then `text.iter().filter(|&item| !empty(item))`.
3. `block_content.rs scope_text`: `inside = |of| move |n| self.node_is_self_or_descendant(node.dom, n, of)` (closure returning a closure); `enclosing_block(tree, .., |p| placed.iter().any(..))` then `placed.push`; `content.flat_text().get(lo..hi).unwrap_or_default()` on a temporary.
4. `e2e/full.rs`: `selection_state(callback_info.get_layout_window(), |d, n| build_selector_for_node(callback_info, d, n))` with `callback_info: &mut CallbackInfo` (needs the implicit `&*` reborrow inside an `Fn` closure); struct update `SelectionRangeInfo { .., ..selection_range_info(..) }`.
5. `block_content.rs flat_window_of`: `run.source_node_id.is_some_and(&holds)` with `holds: impl Fn(NodeId) -> bool`.
6. `text_block.rs caret_on_edge_line`: `clusters` closure returning a borrowing `Filter`, called three times.

## Test commands for the parent

```
cargo test -p azul-layout --test all -- ctrl_d_searches_for_the_whole_word \
  shift_down_off_a_paragraph_keeps_the_column text_after_a_line_break_is_edited_at_its_caret \
  typing_after_collapsed_spaces_lands_at_the_caret a_list_item_caret_moves_with_the_apps_text \
  a_screen_reader_reads_inline_text_where_it_stands
cargo test -p azul-layout --lib callbacks::tests::inspect_
cargo test -p azul-layout --features e2e-server --lib selection_state_tests
# regressions most likely to move:
cargo test -p azul-layout --test all -- a_caret_counts_bytes_in_its_own_block block_edge_keys \
  shift_arrows_extend_a_document_selection a_screen_reader_reads_a_host_with_paragraphs \
  a11y_consumer_contract app_set_text_beats_typing single_block_copy list_item_editing \
  text3_dense_equivalence cross_block_selection typing_into_a_formatted_paragraph
cargo test -p azul-layout --lib window::tests
```

## Review §5 #5 / #7 re-verified: FIXED on the base

- #5 (keyboard selection unpainted): `finalize_pending_focus_changes` opens the session on the seed's `TextBlock` via `open_session`; `build_primary_text_selections_map` keys `affected_blocks` by `session.block`; `paint_selections` looks up `affected_blocks.get(&block)` with `block = tree.text_block_at(..)` - one key on both sides. Guard: `keyboard_selection_is_painted.rs`.
- #7 (edits keyed to an inline element): `apply_one_text_changeset` and `delete_selection` key edits by `edit_element(scope, caret block)` = the block's IFC-root element; the seat variant keys by the seat's own caret block. Guard: `typing_into_a_formatted_paragraph.rs`. The new `delete_preview` uses the same rule.

## Not done - plans

**N4 `CaretPos`** (no bug here needed it). Plan: `pub struct CaretPos { run: u32, byte: RunByte }` minted only by `BlockContent::caret_pos(&TextCursor)` (affinity resolved = today's `run_byte_of`, with Trailing-at-run-end normalised to Leading on the next text run unless a break follows, i.e. the rule `BlockContent::caret_at` already uses). Replace structural `==` at: `same_caret_position` (becomes `caret_pos(a) == caret_pos(b)`), the drag's `anchor == focus`, the handle-drag "dropped on the anchor", `step_document_focus`'s `at_edge`/`moved`, `extend_document_selection`'s collapse check, `collapsed_range_caret`. ~10 sites, all already funnelled through `same_caret_position` or `collapsed_range_caret`; do it as one refactor commit with a guard test that Trailing@k-1 and Leading@k compare equal in each path.

**N8 one selection store**. Plan: replace `multi_cursor` + `cross_block` with `DocumentSelection { anchor: (TextBlock, TextCursor), focus: (TextBlock, TextCursor), extra: Vec<(TextBlock, SelectionRange, SelectionOwner)> }` in `TextEditManager`; per-block ranges derived on demand by one fn (`ranges_in(block)`, today's `set_cross_block_selection` body minus the store) used by paint (`build_primary_text_selections_map`), copy, delete (`delete_cross_block_selection`), E2E (`selection_state`) and a11y (`accessible_selection`). Order: (1) introduce the type beside the two stores, written by `open_session` / `set_cross_block_selection`; (2) switch readers one at a time (each a guarded commit); (3) delete `cross_block` and the `clear_cross_block_selection` calls (the stale-selection class then cannot exist). Largest change; its own task.

## Left open / noticed

- Plain (non-Shift) Up/Down never leave the session's block: Down on a paragraph's last line does nothing in a multi-paragraph host (only Shift+Down crosses). Same column logic (`caret_at_column_in`) would serve it.
- `text-transform` + pre modes: the pre edit model still stores transformed text (`split_text_for_whitespace`); normal/nowrap now store untransformed. A case mapping that changes byte length (ß -> SS) still desynchronises caret bytes in both.
- `dom_text_of` reads DIRECT children only: an edited paragraph whose text sits under `<b>`/`<span>` never converges by text equality (pre-existing).
- Inline-blocks in the edit model: the outer block's edit model holds the inline-block's text as runs where the layout has one `Shape`, so run numbers after an inline-block with several runs diverge (N3 remainder).
- An empty inline host reads as an empty window at the block's start (its position in the paragraph is not known without a text run).
