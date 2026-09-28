# Pointer menus + SplitPane fix - PROGRESS (checkpoint, deleted by the final commit)

Branch `wt/pointer-menus-splitpane` from `5414bfa6b`. Rules: NO cargo/rustc/LSP; RED test commit
before every fix; `git commit -F -`; explicit staging; never touch `layout/src/solver3/page_breaks.rs`
or `layout/tests/a_padded_table_cell_stays_in_its_row.rs`; no api.json edits.

## Findings so far

- Bug 1 (context menu): the macOS right-click / Control-click path is byte-identical to the one
  verified on screen 2026-08-24 except the press router, which returns `Content` for every non-left
  button (neutral). No code regression found in the shells. The user wrote "cmd+click": Command+click
  is NOT the macOS secondary click (Control+click is). Real defects found: each shell picked the
  menu's node its own way (macOS front-most hit + walk; X11/Wayland lowest dom + highest NodeId;
  Windows first node with a menu in NodeId order = OUTERMOST, no walk, and it skipped the MouseUp
  pass when a menu opened); no walk crossed from a VirtualView page into its host.
- Bug 2 (SplitPane) root causes: (a) the demo's `on_resize` returns RefreshDom (bump) and rebuilds
  the pane with a fixed `with_ratio(0.5)` and a FRESH state RefAny -> drag dies after one move and the
  divider snaps back (no dataset merge, unlike Slider's `merge_slider_state`); (b) MouseLeave of ANY
  child (the 6px divider, left on the first pixels of a drag) bubbles to the container and ends the
  drag; (c) no pointer capture; (d) `delta / container` ignores the 6px divider (divider lags the
  cursor ~3% at 200px); NaN cursor poisons the ratio; (e) resize cursor only on the 6px bar though the
  grab zone is +-9px; (f) no keyboard, container (not divider) carries role Grip + tab stop.

## DONE

- `78fd7f148` refactor: `layout/src/context_menu.rs` (`is_secondary_press`, `nearest_context_menu`,
  `context_menu_under_hit`, `LayoutWindow::context_menu_under_pointer/_under_seat`) = the macOS rule
  verbatim; macOS shell uses it (events.rs `resolve_context_menu(position)`, mod.rs ctrl latch).
- `418aa49b3` RED test `layout/tests/a_context_menu_opens_from_a_secondary_press.rs` (in all.rs):
  RED = `a_right_press_on_a_page_that_a_menu_box_hosts_opens_the_boxs_menu`; rest are guards.
- `e1b6e8121` fix: walk crosses VirtualView hosts, doms front-most first; X11/Wayland/Windows use
  `context_menu_under_pointer/_under_seat` (get_first_hovered_node removed, HitTestNode imports
  dropped in x11/events.rs + wayland/mod.rs, FullHitTest import dropped in windows/mod.rs); Windows
  WM_RBUTTONUP always runs the pass (menu is parked via PostMessage).

- `2dcbe6868` S1 RED `a_divider_drag_survives_the_rebuild_its_on_resize_asks_for` (+ guard
  `an_idle_split_pane_takes_the_ratio_the_app_rebuilds_it_with`); test helpers `split_container`,
  `callback_state`, `rebuild`, `app_dom` at the end of split_pane.rs tests.

- `32e554055` S1 FIX: container `.with_dataset(state).with_merge_callback(merge_split_pane_state)`;
  demo `Showcase.split_ratio` (stored in `on_splitpane`, passed to `with_ratio`).

- `2562870f4` S2 RED: `a_drag_that_leaves_the_divider_keeps_resizing_until_the_release`,
  `a_press_on_the_divider_captures_the_pointer_for_the_split_pane` (+ 2 guards); harness
  `drive_in`, `button_held`, `registered`.

## IN PROGRESS

- S2 FIX (capture on press + `on_split_pointer_leave`), see NEXT 2.

## NEXT (in order)

1. SplitPane S1 RED: reconciler test `a_divider_drag_survives_the_rebuild_its_on_resize_asks_for`
   (copy of slider's `a_drag_survives_reconciliation_of_a_parent_rebuild`, find container by class
   `__azul-native-split-pane`). FIX: container `.with_dataset(state)` + `.with_merge_callback(
   DatasetMergeCallback::from_ptr(merge_split_pane_state))` (carry is_dragging/drag_start_px/
   ratio_at_drag_start/ratio mid-drag, same direction only) + demo `Showcase.split_ratio` stored by
   `on_splitpane`, passed to `.with_ratio(..)`.
2. S2 RED: leave mid-drag keeps the drag; press captures the pointer. Needs a `drive_in(window_state,..)`
   test helper (left_down = true). Invoke the registered MouseLeave callback via
   `crate::callbacks::Callback::from_core(core.callback.clone()).invoke(state, info)`. FIX: press
   `info.capture_pointer(info.get_hit_node())`; new `on_split_pointer_leave` ends the drag only when
   `get_current_mouse_state().left_down` is false; update `pointer_down_records_the_anchor...`
   (changes no longer empty) + `dom_registers_every_pointer_event_on_the_container`.
3. S3 RED: divider is the focusable separator; arrows move it. FIX: `on_split_key` on the DIVIDER
   (Focus VirtualKeyDown), TabIndex + role `Grip` (-> accesskit Splitter) + name "Resize panes" +
   value "NN%" moved from container to divider; Left/Up -1%, Right/Down +1%, Ctrl/Cmd 10%, Home/End
   = MIN/MAX; prevent_default; panes via get_previous_sibling/get_next_sibling; fire on_resize.
4. S4 RED: divider stays under the cursor (`delta / (W - 6)`, centre `r*(W-6)+3`) + NaN cursor keeps
   ratio. FIX + update pins (0.75 -> 0.5+50/194 etc., drop the two NaN "poisons" pin tests).
5. S5 RED: grab area wider than the visible line. FIX: divider `position: relative` + absolutely
   positioned transparent sash child (2*GRAB_THRESHOLD wide, resize cursor); update divider-leaf and
   6-property pin tests.
6. Final report `scripts/POINTER_MENUS_SPLITPANE_FIX_2026_09_28.md`, delete this file.

## Open questions

- Whether the device right-click failure is in the hit test (paint-order sort 7d7b25c0e / scroll
  chain): the guard `a_right_press_on_the_menu_box_of_a_scrolled_page_opens_its_menu` answers it on
  the parent's build.
