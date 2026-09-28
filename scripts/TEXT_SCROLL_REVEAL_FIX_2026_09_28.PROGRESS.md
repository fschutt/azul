# Text scroll / caret reveal fix — PROGRESS (checkpoint, 2026-09-28)

Branch `wt/text-scroll-reveal` (from 5414bfa6b). NO compilation (rule). RED test
commit first, then the fix commit. Explicit `git add` paths only. Commit this file
after every commit. The final report `scripts/TEXT_SCROLL_REVEAL_FIX_2026_09_28.md`
replaces it; delete this file in the last commit.

Sources: `scripts/TEXT_SCROLL_VS_CARET_REVEAL_ARCHITECTURE_2026_09_26.md` §8 steps 1-5,
`scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` M3, M5, D.

## DONE

- 0a215362a test(layout) RED bug 1: `layout/tests/typing_past_the_right_edge_reveals_the_newest_character.rs`
  (registered in all.rs). Shell order: edit lands, reveal, no relayout.
- 77797725e fix bug 1: `ScrollManager::update_content_size`; `reshape_text_node` publishes
  the extent to the scroll manager when the necessity did NOT flip (skip viewport scroller);
  `scroll_registration::caret_scroll_extent` = the one caret-gutter rule (registration +
  both reshape branches).
- 6fa7b812c test(dll) RED step 1: headless::tests::
  `a_pass_that_lands_no_edit_leaves_the_text_area_where_the_wheel_put_it` (+ helpers
  `lone_text_area_layout`, `caret_scroll_box`, `type_line`).
- acf1b1bd8 fix step 1: `LayoutWindow::apply_pending_text_and_reveal() -> LandedTextEdit`
  (`landed()`, `event_result()`); used by the shell tail, dll `InsertLineBreakAtCursor`,
  dll `CreateTextInput`, runner tail / `CreateTextInput` / `InsertLineBreakAtCursor`;
  headless `Scroll` arm now runs `process_window_events(0)` (parity).
- 2093f000d test(layout) RED step 2: 2 RED + 1 control in `scroll_timer.rs` tests
  (finger retires Keyboard AnimateTo; wheel click during reveal glide bases on view).
- 5472f5e7f fix step 2: `is_engine_seek`, `retire_engine_seek` in scroll_timer.rs; finger/
  momentum arm retires; wheel arm filters engine targets for its base.

- 1c52c2f4e chore: this progress file.
- 1ab63b01d test(layout) RED step 3: `layout/tests/a_selection_reveal_shows_its_focus_end.rs`
  (registered) — range 0..150 reveal must show the focus; control 160..170 shown whole.

- 3a7871e65 fix step 3: `scroll_selection_into_view` reveals the range rect only when it
  fits (`REVEAL_PADDING_PX` both sides), else the focus caret.

- e9d07918d refactor step 4: `RevealRequest` arbiter (scroll_state.rs), `request_session_reveal`,
  `reveal_for_input`, `perform_pending_reveal(keep_for_layout)` (pub), W5 = take+perform,
  latch fields deleted; requests at: apply_text_changeset (primary landed),
  apply_selection_op_for_seat (wrapper over `..._unrevealed`), process_mouse_click_for_selection
  (wrapper over `place_selection_at_click`), finalize_pending_focus_changes, caret restores
  (funnel + virtual-view path), a11y focus; dll W6/W7 + runner W7 via reveal_for_input;
  legacy `ScrollCursorIntoViewAfterTextInput` arm performs pending only.

## IN PROGRESS

- 2d5068ff6 test(layout) RED step 5: `a_drag_selection_owns_the_scroll_of_its_field.rs` (registered).
- f7ecec42a fix step 5: `process_mouse_drag_for_selection` calls `note_user_scroll` (drag + W9 claim the view; W9 already re-resolves after its scroll in the dll MWA-B8b block).

## IN PROGRESS (next)

- e38569fe1 refactor M3: `LayoutWindow::drag_autoscroll_box(anchor)` (old rule), dll timer calls it.
- 0b4d797d2 test RED M3: `a_selection_drag_autoscrolls_the_box_its_text_scrolls_in.rs` (registered).
- c47c53a92 fix M3: `TextTarget::scroll_box`, `LayoutWindow::scroll_box_of_layout_node` (find_scrollable_ancestor delegates), drag_autoscroll_box prefers the session box during a text drag, reveal uses it, timer edge box = `ancestor_scroll_offset`.
- M5 RED next: `focused_byte_offset_for_point` / `focused_rect_for_byte_offset` ignore the field scroll.

## NEXT (in order)

2. (DONE, kept for reference) Step 4 (structural, RED impossible; contract tests in the same commit): replace
   `ScrollManager::last_view_action`/`ViewAction`/`note_reveal_intent` with
   `pending_reveal: Option<RevealRequest>` (`RevealRequest::{Caret, Selection}`),
   `request_reveal`, `pending_reveal()`, `take_pending_reveal()`; `note_user_scroll()` drops it;
   keep `reveal_may_move_view()` = `pending_reveal.is_some()`; rewrite `mod last_action_wins`
   tests. `scroll_selection_into_view` no longer touches the arbiter (pure compute+apply).
   Issue requests at input sites: `apply_one_text_changeset` (primary seat, landed),
   `apply_selection_op_for_seat` (primary, true), `process_mouse_click_for_selection` (Some),
   `finalize_pending_focus_changes` (seeded caret), `restore_caret_from_resume_point`,
   dll W6 `ScrollSelectionIntoView` arm + W7 `ScrollActiveCursorIntoView` (runner W7 too).
   Consumers: W5 `scroll_focused_cursor_into_view` = take + perform (delete
   `last_revealed_caret_rect/_key` fields + their destructure sites window.rs ~966/~21644 +
   test caret_follows_typing.rs `a_moved_caret_rect...` line mutating the field);
   `apply_pending_text_and_reveal` = perform pending now, keep it only if `needs_relayout`;
   W6/W7 = take + perform. `scroll_node_into_view` (W8, window.rs ~8942) stops calling
   note_reveal_intent. Behaviour change: app caret moves no longer reveal by themselves.
3. Step 5: RED (layout) — press (click request) then `process_mouse_drag_for_selection`
   near the right edge of a scrolled field, relayout: offset must not move / no pending
   reveal. Fix: a drag extension drops the pending reveal (the drag / W9 owns the view).
4. M3: refactor commit `LayoutWindow::drag_autoscroll_box(anchor)` (behaviour-preserving,
   dll `auto_scroll_timer_callback` calls it via `callback_info.get_layout_window()`);
   RED layout test (host focused, `text_selection_drag_anchor = Some`, expect value `<p>`);
   fix: `TextTarget::scroll_box(&LayoutWindow)` (self-inclusive walk from the IFC layout
   index; shared with `scroll_selection_into_view`), autoscroll uses it during a text
   drag; replace the timer's ancestor loop by `scroll_manager.ancestor_scroll_offset`
   (ScrollChain-published).
5. M5: RED layout test (TextInput scrolled by S: `focused_byte_offset_for_point` off by S;
   `focused_rect_for_byte_offset(k).x - get_focused_cursor_rect_viewport().x == S`); fix:
   `focused_cursor_for_point` via `window_point_to_ifc_local` (minus
   `window_space_offset_of_dom`) + `TextTarget::hittest`; `focused_rect_for_byte_offset`
   through `cursor_rect_viewport_for`.
6. D typed rects: skip unless trivial (open item).
7. Report `scripts/TEXT_SCROLL_REVEAL_FIX_2026_09_28.md`, delete this file.

## Open questions

- Why the device's post-layout reveal (W5 after the incremental relayout) did not already
  correct bug 1 is not proven by reading; the fix removes the stale clamp at its source.
- Headless `Scroll` arm now runs a pass: may change dll e2e/golden tests that scroll.
