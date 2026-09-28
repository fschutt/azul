# Text scroll vs. caret reveal: fix report (2026-09-28/29)

Branch `wt/text-scroll-reveal`, based on `5414bfa6b` (PR #476,
`fix/input-bugs-2026-09-19`). **Nothing was compiled or run** (the rule for
this wave). Every fix has a RED test commit before it, except the step-4
refactor, where a RED is impossible (see below). The parent compiles once.

Sources:
- `scripts/TEXT_SCROLL_VS_CARET_REVEAL_ARCHITECTURE_2026_09_26.md` §8, steps 1-5
- `scripts/SELECTION_WHEN_CLIPPED_ARCHITECTURE_2026_09_26.md` M3 and M5 (D was not done)

## 1. The bugs, and what was wrong

**Bug 1: the TextInput caret reveal is ONE KEYSTROKE LATE.** The shells
reveal the caret right after the edit lands and before any relayout: the
tail's `ApplyTextChangeset` → `ScrollCursorIntoViewAfterTextInput`, and the
`CreateTextInput` arm on the macOS IME path. The reveal is clamped against
`ScrollManager::content_rect`. `reshape_text_node` wrote the edit's new
extent into the layout tree, but it refreshed the scroll manager only when
the box's scrollbar necessity FLIPPED. So a field that already overflowed
kept the previous keystroke's extent. `max_scroll` stopped one keystroke
short, and the new glyph was clipped. This is candidate (a) of the brief.
The caret gutter was applied correctly, but only by the registration pass.

**Bug 2: TextArea wheel jitter (steps 1-5).** The mechanism is as the
architecture report describes it. The shell tail revealed on every pass that
was not prevented. Every reveal re-armed the one-bit arbiter. A finger left
the reveal glide dormant, not dead. A wider-than-port range revealed its
start. And the drag had several writers.

**M3.** The drag-autoscroll anchored on the focused HOST and walked its DOM
ancestors. The TextInput's value `<p>` is the host's child, so the drag never
scrolled the field. **M5.** The IME rects were static layout rects, and the
point → byte conversion ignored the field's own scroll.

## 2. Commits (in order) and the RED each one pins

| Commit | Kind | What | RED today → expected |
|---|---|---|---|
| `0a215362a` | test | `layout/tests/typing_past_the_right_edge_reveals_the_newest_character.rs`: real TextInput, value scrolled to its end, "WWWW" typed + revealed with no layout in between (the shell's order) | caret right edge ≈ 4 advances − 6px (~30px) PAST the field's right edge → ≤ field right. Control (typing 20px inside leaves the offset alone) green both ways |
| `77797725e` | fix | `ScrollManager::update_content_size`; `reshape_text_node` publishes the extent to the manager when the necessity did not flip (not for the viewport scroller) + refreshes the bar states/thumbs; `scroll_registration::caret_scroll_extent` = one gutter rule (registration + both reshape branches) | — |
| `6fa7b812c` | test (dll) | `headless::tests::a_pass_that_lands_no_edit_leaves_the_text_area_where_the_wheel_put_it`: 15 lines typed into a TextArea, then the wheel (`record_scroll_from_hit_test(0,-30)` + physics commit), the macOS wheel pass, and a 1px MouseMove | offset snaps back to `y_typed` → stays `y_typed-30`; `reveal_may_move_view()` true → false; no `AnimateTo` queued. Control: the next keystroke reveals again |
| `acf1b1bd8` | fix | `LayoutWindow::apply_pending_text_and_reveal() -> LandedTextEdit` (`landed()`, `event_result()`): ONE tail for the shell's post-callback tail, dll `InsertLineBreakAtCursor`, dll `CreateTextInput`, and the runner's three ports. The headless `Scroll` arm now runs `process_window_events(0)` (parity with macOS) | — |
| `2093f000d` | test | `scroll_timer` tests: `a_finger_retires_the_caret_reveal_glide_it_interrupts`, `a_wheel_click_during_a_caret_reveal_glide_moves_from_the_view_not_the_reveal_target` (+ control `a_wheel_glide_is_the_users_own...`) | Keyboard `AnimateTo` target still armed after a finger → removed; a wheel target of y=370 (400−30) → 70 (100−30) |
| `5472f5e7f` | fix | `is_engine_seek` / `retire_engine_seek`: the finger/momentum arm retires engine seeks; the wheel arm bases its target on its own glide only | — |
| `1ab63b01d` | test | `layout/tests/a_selection_reveal_shows_its_focus_end.rs`: range 0..150 of 200 chars, focus at 150 | offset → 0, focus off-screen → focus on screen. Control: range 160..170 shown whole |
| `3a7871e65` | fix | `scroll_selection_into_view`: the bounding rect only when it fits the port (`REVEAL_PADDING_PX` on both sides), else the focus caret | — |
| `e9d07918d` | refactor! | the one-shot `RevealRequest` arbiter (§3), with contract tests in `scroll_state::last_action_wins` | no RED possible (new API); negative control in §6 |
| `2d5068ff6` | test | `layout/tests/a_drag_selection_owns_the_scroll_of_its_field.rs`: press 1px inside the right edge of a field scrolled to x=40, 3 drag steps, a layout after each | `pending_reveal() == Some(Caret)` after the drag, and ~5px text shift per layout → `None`, offset unchanged |
| `f7ecec42a` | fix | `process_mouse_drag_for_selection` claims the view (`note_user_scroll`). This covers the pointer drag, the handle drag and the W9 autoscroll frames (which re-resolve the end AFTER their scroll in the dll's MWA-B8b block) | — |
| `e38569fe1` | refactor | `LayoutWindow::drag_autoscroll_box(anchor)` = the old timer rule. The timer calls it via `callback_info.get_layout_window()` | guarded by existing tests |
| `0b4d797d2` | test | `layout/tests/a_selection_drag_autoscrolls_the_box_its_text_scrolls_in.rs` | `drag_autoscroll_box(host)` = page/None → the value `<p>`. Control: TextArea → its container, both ways |
| `c47c53a92` | fix | `TextTarget::scroll_box(&LayoutWindow)` + `LayoutWindow::scroll_box_of_layout_node` (`find_scrollable_ancestor` delegates). `drag_autoscroll_box` uses the session box during a text drag, and the caret reveal uses the same box. The timer's edge box subtracts `ScrollManager::ancestor_scroll_offset` (the ScrollChain frames) instead of its own DOM walk | — |
| `284b3b7c4` | test | `layout/tests/ime_geometry_follows_the_fields_scroll.rs` (value scrolled by 40px, caret on byte 22 as the oracle) | `focused_rect_for_byte_offset(22).x` is 40px right of the painted caret → equal; the point 1px right of the caret resolves ~7 bytes left → byte 22 (±1) |
| `e82f6af75` | fix | `focused_rect_for_byte_offset` (and `_range`) via `cursor_rect_viewport_for`; `focused_cursor_for_point` via the dom-host offset + `window_point_to_ifc_local` + `TextTarget::hittest` | — |

The `chore(scripts): progress checkpoint` commits in between only update
`scripts/TEXT_SCROLL_REVEAL_FIX_2026_09_28.PROGRESS.md`. The last commit
deletes that file.

All new `layout/tests/*.rs` files are registered in `layout/tests/all.rs`.
`page_breaks.rs` and `a_padded_table_cell_stays_in_its_row.rs` were not
touched.

## 3. The arbiter (step 4) in one paragraph

`ScrollManager::pending_reveal: Option<RevealRequest>` (`Caret | Selection`)
replaces `last_view_action`/`ViewAction`/`note_reveal_intent` and
`LayoutWindow::last_revealed_caret_rect`/`_key`.

- **Who issues a request.** Only input sites issue one, through
  `request_session_reveal`: a landed primary-seat edit (inside
  `apply_text_changeset`), a primary caret or selection op (the
  `apply_selection_op_for_seat` wrapper), a click (the
  `process_mouse_click_for_selection` wrapper), a focus that seeds a caret, and
  a restored structural-edit caret (in the funnel and on the virtual-view path).
  The shells' `ScrollSelectionIntoView` and every `ScrollActiveCursorIntoView`
  (app, a11y focus, runner) go through `reveal_for_input`.
- **What drops it.** `note_user_scroll` drops it. That covers the wheel, the
  trackpad, a thumb drag, and a selection drag.
- **Who consumes it, once.** The input's own pass consumes it:
  `perform_pending_reveal(keep_for_layout)`, which keeps it only when a
  relayout follows. Otherwise the post-layout `scroll_focused_cursor_into_view`
  consumes it, after registration.
- **The reveal itself.** `scroll_selection_into_view` is now pure compute and
  apply.

## 4. Public API changes (Rust; nothing in `api.json`, which was not edited)

None of the touched symbols are in `api.json` (checked: 0 hits). So there is
no autofix work, and the C ABI is unchanged.

- **New in `azul_layout::managers::scroll_state`:**
  - `RevealRequest`;
  - `ScrollManager::{request_reveal, pending_reveal, take_pending_reveal, update_content_size}`.
- **Removed:**
  - `ScrollManager::note_reveal_intent`;
  - `ViewAction`;
  - `LayoutWindow::{last_revealed_caret_rect, last_revealed_caret_key}` (these were pub fields).
- **Changed meaning:** `ScrollManager::reveal_may_move_view()` now means "a
  request is pending". It is false on a fresh window.
- **New on `LayoutWindow`:**
  - `apply_pending_text_and_reveal`, `LandedTextEdit`;
  - `perform_pending_reveal(bool)`, `reveal_for_input(RevealRequest)`;
  - `drag_autoscroll_box`, `scroll_box_of_layout_node`;
  - `request_session_reveal` (pub(crate)).
- **New:** `TextTarget::scroll_box`;
  `scroll_registration::caret_scroll_extent`.
- **Now private:** `process_mouse_click_for_selection` and
  `apply_selection_op_for_seat` are thin wrappers over the new private
  `place_selection_at_click` and `apply_selection_op_for_seat_unrevealed`.

## 5. Least sure to compile

1. `scroll_timer.rs`: the closure patterns on double references,
   `.get(&key).filter(|(_, device)| !is_engine_seek(*device))` (on
   `Option<&(LogicalPosition, ScrollInputDevice)>`), and the `is_some_and`
   in `retire_engine_seek`.
2. `scroll_state.rs`: the `const fn`s that assign through `&mut self`
   (`note_user_scroll`, `request_reveal`). This relies on const `&mut`, stable
   since 1.83; the old `note_user_scroll` already did it.
3. `window.rs` `scroll_selection_into_view`: the `fits` closure plus the
   guarded tuple match `(range_rect, focus_rect)`, and
   `.as_ref().and_then(|_| self.session_text_target())` inside a `&mut self`
   method.
4. `dll/src/desktop/shell2/headless/mod.rs` test: the wheel block's split
   borrow (`lw.scroll_manager.record_scroll_from_hit_test(.., &lw.hover_manager, ..)`,
   the same shape as the Scroll arm), and `window.service_frame(r)` from the
   tests module.
5. The dll timer: `let _ = node_id;` (kept for the early return) and
   `callback_info.get_layout_window().scroll_manager.ancestor_scroll_offset(..)`.
6. `window.rs`: the renamed bodies `place_selection_at_click` and
   `apply_selection_op_for_seat_unrevealed` keep their original attributes.
   Check for a doc or `#[allow]` mismatch.

## 6. Checks the parent must run (they could not be run here)

- **Every RED above goes GREEN.** Run layout `--test all` for the new modules,
  layout `--lib` for `scroll_timer` and `scroll_state`, and dll `--lib` for the
  headless test.
- **Negative control for step 4.** After `e9d07918d` no landed-edit gate is
  left to revert: `apply_pending_text_and_reveal` just performs whatever is
  pending, and the shell tail runs it on every non-prevented pass. So the
  step-1 test staying green IS the negative control. The arbiter decides,
  because the wheel dropped the only request. To see it fail on purpose, make
  `note_user_scroll` a no-op: the test must go RED.
- **Existing suites that touch the reveal:**
  - `caret_follows_typing` (one test edited: it nudged the deleted latch);
  - `caret_reveal_and_session_identity`, `caret_scroll_glide`;
  - `textinput_resize_selection`, `textarea_enter_repaint`, `viewport_scroll_frame`;
  - `a_press_on_an_overflowing_field_selects`;
  - `vview_contenteditable_e2e`, `click_into_a_virtual_view_page`;
  - the dll headless suite;
  - the e2e JSON corpus (`e2e/*.json`, especially the `op-scroll-*`,
    `noninterference-*` and `bug-caret-off-after-focus` scenarios).

## 7. Behaviour changes

- **Typing.** A pass that lands no edit no longer reveals the caret (wheel,
  momentum, mouse move, `ScrollEnd`).
- **Reveal timing.** The reveal after a keystroke is correct in the same pass.
  When the text's extent changed, it runs again after the relayout.
- **Programmatic caret moves.** An app that moves the caret itself (without
  `ScrollActiveCursorIntoView`) no longer scrolls it into view. A peer's or
  another seat's edit that shifts the local caret no longer reveals it.
- **Headless.** The dll headless `Scroll` event now runs an event pass, so the
  `Scroll` callback fires and `pending_wheel_event` is consumed there. This may
  shift e2e and golden expectations.
- **The e2e runner.** The runner performs a click's or caret op's reveal at its
  pass tail (the shells' `ScrollSelectionIntoView` timing), not after the next
  layout.
- **User gestures and engine glides.** A finger, or a wheel click, on a node
  retires an engine glide on that node. That includes an app's
  `scroll_to_animated` (`Programmatic` provenance) and `TestDriver` seeks.
- **Selection reveals.** A selection wider or taller than its scrollport
  reveals its focus end (it used to reveal its start).
- **Drag selection.**
  - A drag drops any pending reveal.
  - A text-selection drag autoscrolls the field (the TextInput's value `<p>`)
    instead of the page.
  - The autoscroll's edge box uses the ScrollChain offsets.
- **IME and handles.** The IME candidate window and `characterIndexForPoint:`
  now follow the field's scroll (macOS and iOS), and so does the
  selection-handle drag's focus resolution.
- **Custom post-filters.** `SystemChange::ScrollCursorIntoViewAfterTextInput`
  (from a custom post-filter) now performs only a pending request.

## 8. Open items

- **Bug 1 on the device.** Reading the code, the post-layout reveal after the
  incremental relayout should already have corrected the stale clamp. The
  keystroke does trigger an incremental relayout, because the IFC `<p>`'s box
  width never equals its text width. Why it did not correct it on the device
  is not proven. The fix removes the stale clamp at its source. Confirm on the
  device. If the lag persists, log `ScrollManager::content_rect` and the offset
  around `apply_pending_text_and_reveal` and `scroll_focused_cursor_into_view`.
- **D (typed rects: `TextLayoutRect`, `WindowRect`,
  `TextTarget::rect_to_window`/`point_from_window`) was NOT done.** It did not
  fall out naturally.
- **Selection handles are still static.** `selection_handle_geometry` /
  `selection_handle_at` compare a window point with STATIC handle rects (the
  same class as M5), so they are off by the field's scroll.
- **The runner has no `ScrollSelectionIntoView`.** Undo, redo, cut, paste and
  select-all in the runner issue no request unless they go through
  `apply_text_changeset`. The dll covers them.
- **Focus-seed request timing.** A focus seed's request waits for the next
  layout (as W5 did). If no layout follows, it stays pending until the next
  layout or the next user scroll.
- **M4 (by design).** A click within 5px of an edge still nudges the text (the
  click's own reveal).
- **Cross-check against the SELECTION_BUGS ledger items.** Not done: N3 index
  spaces, `byte_offset_of_cursor`.
