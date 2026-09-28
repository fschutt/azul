# P2-12: WAI-ARIA APG roving tabindex for the composite widgets (2026-09-28)

Branch `wt/roving-tabindex`, based on `7e020dc49` (PR #476, `fix/input-bugs-2026-09-19`).
Source: `scripts/FOCUS_SUBWINDOW_AND_ARROW_KEYS_ANALYSIS_2026_09_26.md` §D and §E P2-12.
**Nothing was compiled or run.** The parent compiles the whole wave once at the end.

## What changed, in one paragraph

RadioGroup, Segmented, TabHeader, ListView, TreeView and the DatePicker day grid are now
**one Tab stop each**. The active item (the checked radio, the selected segment, the
active tab, the selected row, the first visible selected tree row, the selected day)
is built with `TabIndex::Auto`. If no item is active, the first item gets it. Every
other item is `TabIndex::NoKeyboardFocus`: still focusable by click and from code, but
not by Tab. Every item carries a `Focus(VirtualKeyDown)` handler that moves focus
within the group, following the APG pattern of that widget. The handler moves the Tab
stop with the focus through a new engine call, `CallbackInfo::set_tab_index`, and calls
`prevent_default` so spatial navigation does not run as well. When Alt, Ctrl, Cmd or
Shift is held, the key is never consumed. The mechanics are shared in
`layout/src/widgets/roving.rs`.

## Commits (in order)

| # | Commit | Kind | Expected RED (before its fix) |
|---|---|---|---|
| 1 | `5ebb1ffb7` feat(callbacks): a callback can move a node in or out of the Tab order | engine helper | New API; a RED test naming the variant could not compile. Guarded by `e2e::runner::tests::a_tab_index_written_by_a_callback_moves_the_tab_stop` (e2e-server feature) and by every widget RED below. |
| 2 | `87705a712` feat(widgets): a roving-tabindex helper for the composite widgets | shared helper | None of its own (guarded by the widget REDs). Pure unit tests are in `widgets::roving::tests`. |
| 3 | `beb84581c` test(radio_group) | RED | Tab walk from the item before is `[row 0, row 1]`, expected `[row 1 (checked), after]`. Shift+Tab gives `[row 2, row 1]`, expected `[row 1, before]`. With nothing checked: `[row 0, row 1]`, expected `[row 0, after]`. Click gives `[row 0, row 1]`, expected `[row 2, row 2]`. Every arrow test panics with "every radio must carry a key handler for the arrow keys". |
| 4 | `53c4123a5` fix(radio_group) | fix | – |
| 5 | `52c5f5630` test(segmented) | RED | Walk `[seg 0, seg 1]`, expected `[seg 2 (selected), after]`. Out of range: `[seg 0, seg 1]`, expected `[seg 0, after]`. Click: `[seg 0, seg 1]`, expected `[seg 2, seg 2]`. Arrow tests panic ("every segment must carry a key handler…"). |
| 6 | `b1d9adb27` fix(segmented) | fix | – |
| 7 | `77378d584` test(tabs) | RED | Walk `[after, before]` (the tabs are skipped entirely, since no tab had a tab index), expected `[tab 1 (active), after]`. Out of range: `[after, before]`, expected `[tab 0, after]`. Arrow tests panic ("every tab of an interactive tab list must carry a key handler"). The guard `a_tab_list_without_on_click_stays_out_of_the_tab_order` is green before and after. |
| 8 | `199300a92` fix(tabs) | fix | – |
| 9 | `4e693eaf4` test(list_view) | RED | Walk `[row 0, row 1]`, expected `[row 0, after]`. Arrow tests panic ("every list row must carry a key handler…"). |
| 10 | `5a3ced2e3` fix(list_view) | fix | Also adds the `selected_row` field and its two tests. These are not in the RED commit because they need the new field. |
| 11 | `6dde568ee` test(date_picker) | RED | Walk from the "next" button is `[day 1, day 2]`, expected `[day 14 (selected), prev]`. Out of range: `[day 1, day 2]`, expected `[day 1, prev]`. Arrow, Page and Home/End tests panic ("every day cell must carry a key handler…"). |
| 12 | `708e7914d` fix(date_picker) | fix | – |
| 13 | `0cbf8fe20` test(tree_view) | RED | Walk `[root, a]`, expected `[b (selected), after]`. With no visible selection: `[root, a]`, expected `[root, after]`. Arrow tests panic ("every tree row must carry a key handler…"). |
| 14 | `0924a7d28` fix(tree_view) | fix | Also adds the `on_node_toggle` hook and its five tests (expand/collapse). These are not in the RED commit because they need the new hook. |
| 15 | `9ddc60fdd` refactor(widgets): drop the roving test helpers no widget test uses | cleanup | – |
| 16 | this report | docs | – |

Test names are in each commit body. They sit in each widget's own test module
(`radio_group`, `segmented`, `tabs`, `date_picker`, `tree_view`: `autotest_generated`;
`list_view`: the new `roving_tabindex_tests`). No `layout/tests/` file was added, so
`all.rs` is untouched.

The tests drive the **real** Tab order: `managers::focus_cursor::resolve_focus_target`
with `FocusTarget::Next`/`Previous` over the widget's `StyledDom`, placed between a
plain stop "before" and a plain stop "after". They also drive the **real** key handler:
the `Focus(VirtualKeyDown)` callback registered on the focused node, invoked through
`Callback::from_core`, with a `KeyboardState` holding the key and modifiers
(`widgets::roving::test_support::press`). To check that "the stop moved", the
`SetNodeTabIndex` writes are applied to the `StyledDom` exactly as the shell applies
them (`apply_tab_index_writes`), and the walk is re-run.

## Per-widget behaviour (APG)

| Widget | Tab stop | Keys (all others and every modified key: not consumed) |
|---|---|---|
| RadioGroup | the checked row, or row 0 | Down/Right and Up/Left **move and check**, wrapping. `on_change` fires as for a click. A click also moves the stop. |
| Segmented | the selected segment, or segment 0 | Right/Down and Left/Up **move and select**, wrapping. Themed restyle and `on_change` as for a click. A click moves the stop. |
| TabHeader (only with `on_click`) | the active tab, or tab 0 | Left/Right (wrapping), Home/End. **Automatic activation**: `on_click(TabHeaderState{active_tab})`. Up/Down are not consumed. A header without `on_click` stays inert (no stop), as before. The click handler is unchanged (stateless; the app's rebuild moves the stop). |
| ListView | `selected_row` (new), or row 0 | Up/Down (holding at the ends), Home/End. **Selection follows focus**: `on_row_click(target)`. Keys at an end are consumed but do nothing. Works without `on_row_click` (focus only). Left/Right are not consumed. |
| TreeView | first **visible** selected row, or the first row | Up/Down through visible rows (holding at the ends). Right: on an open parent, go to the first child; on a closed parent, `on_node_toggle(i, true)` (new); on a leaf, nothing. Left: on an open parent, `on_node_toggle(i, false)`; otherwise go to the parent row; at the top level, nothing. Home/End. Moving focus selects nothing: Enter/Space (the click) selects. |
| DatePicker grid (in the popup window) | the selected day, or day 1 | Left/Right move by a day, Up/Down by a week, Home/End to the first/last day of the week row. Focus only: the state is untouched and the popup stays open, and Enter/Space (the click) picks the day. PageUp/PageDown run `month_nav` (same as ‹ / ›). An arrow past the displayed month is consumed and goes nowhere. |

## Public API changes (api.json needs the autofix; I did not touch it)

- `CallbackInfo::set_tab_index(&mut self, node_id: DomNodeId, tab_index: TabIndex)`.
  It is recorded as the new (non-api.json) `CallbackChange::SetNodeTabIndex { dom_id, node_id, tab_index }`.
- `ListView::selected_row: OptionUsize` (new `#[repr(C)]` field, last), plus
  `ListView::with_selected_row(OptionUsize)` / `set_selected_row(&mut, OptionUsize)` (const fns).
- `TreeView::on_node_toggle: OptionTreeViewOnNodeToggle` (new `#[repr(C)]` field, last), plus
  `TreeView::set_on_node_toggle(data, cb)` / `with_on_node_toggle(data, cb)`.
- New callback kind from `impl_widget_callback!` + `impl_managed_callback!`:
  `TreeViewOnNodeToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, usize /*node_index*/, bool /*expand*/) -> Update`,
  `TreeViewOnNodeToggle`, `OptionTreeViewOnNodeToggle`, `TreeViewOnNodeToggleCallback`,
  `AzTreeViewOnNodeToggleCallbackInvoker`, `AzApp_setTreeViewOnNodeToggleCallbackInvoker`,
  `AzTreeViewOnNodeToggleCallback_createFromHostHandle`.
  - Optional: add `"TreeViewOnNodeToggleCallback"` to `HOST_INVOKER_KINDS` in
    `doc/src/codegen/v2/managed_host_invoker.rs` so libffi hosts use the per-kind invoker.
    Most widget kinds are not listed there either, and the generic invoker covers them.
- DOM-level (not API): tree rows carry the new class `__azul-native-tree-view-row`.
  Composite items now carry a second callback, `Focus(VirtualKeyDown)`, after the click
  callback.

## Engine change

`dll/src/desktop/shell2/common/event.rs` (next to `SetNodeIdsAndClasses`) and
`layout/src/e2e/runner.rs` (its port) apply `SetNodeTabIndex`: they write
`node_data[n].set_tab_index(t)` and return `ProcessEventResult::DoNothing`. The Tab order
(`collect_tab_order`) is read from node data on every Tab press, so no relayout is needed.
An out-of-range node is ignored. `CallbackInfo::set_tab_index` ignores a `DomNodeId`
whose node is `None`.

## Least sure to compile

1. `roving::test_support::press`: `get_callbacks().as_ref().iter().find(|cb| cb.event == key_down)`,
   `cb.callback.clone()`, and `Callback::from_core(core_cb).invoke(data, info)`.
2. `tabs.rs` `on_tab_key` ends in a **tail-position** `match dataset.on_click.as_mut() {..}`
   while the `RefMut` `dataset` is a local. The temporary (`Option<&mut _>`) has no drop
   glue, so this should be fine. The older handlers use the same shape inside a
   `let result = {..}` block instead.
3. `if data.downcast_ref::<T>().is_none() { return .. }` in the radio, segmented and tabs
   key handlers. A temporary `Ref` in an `if` condition drops before the block. `data`
   is `mut` because `downcast_ref` takes `&mut self`.
4. `tree_view.rs`: the module-level `const TREE_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TREE_ROW_CLASS_NAME))]`.
   The `impl_managed_callback!` has a `bool` extra arg (`expand: bool`), the first bool
   extra among the widget kinds, which the invoker sees as `*const bool`.
   There is also `first_visible_selected(&root, &mut 0)` and the `#[cfg(test)] render_node`
   shim over `render_rows(&RowContext)`.
5. `list_view.rs` tests: `let is_click = |cb: &CoreCallbackData| ..; ..filter(|&cb| is_click(cb))`,
   and `Some(2_usize).into()` / `None.into()` for `OptionUsize`.
6. The `tree_view` tests' `row_labelled`: `let NodeType::Text(s) = nd.get_node_type() else { continue }`
   with `s.as_ref().as_str()` on `&BoxOrStatic<AzString>`, and
   `styled.node_hierarchy.as_ref()[i].parent_id()`.
7. The `date_picker` tests: `if let Some(day) = text_of(cell)` (an owned `String`)
   compared with `day == "10"`. There is also `panel_of(&dom).clone()` as a popup-window
   root DOM.
8. The runner test uses the private `Runner::new` / `layout` / `apply_user_change` from
   `mod tests`, and `azul_layout::managers::focus_cursor::{resolve_focus_target, FocusResolution}`.
9. Top-level `TabIndex` imports were removed where only tests still use them:
   `radio_group`, `segmented`, `list_view` and `tree_view`. Each test module now imports
   `TabIndex` itself (`date_picker` and `tabs` keep or never had it). An unused-import
   warning here would be the most likely slip.

## Behaviour changes to know about

- Tab no longer visits every radio, segment, list row, tree row or day. Arrow keys on
  these items are now **consumed** (`prevent_default`), so spatial navigation never
  starts from them. In a listbox, tree or date grid, an arrow at the edge is consumed
  and does nothing, so the arrow keys cannot leave the widget. Radio, segmented and tabs
  wrap.
- A modified arrow (Alt, Ctrl, Cmd or Shift) is never consumed by these widgets. This
  fits P2-10 (spatial navigation only on unmodified arrows): such chords now do nothing
  on these widgets.
- An interactive `TabHeader` (one with `on_click`) becomes keyboard-reachable. Before,
  no tab had a tab index at all.
- Every `ListView` row and `TreeView` row carries a key handler, even without the
  app's click hook. That is the only reason their callback counts changed in the
  existing tests.
- A radio or segment click also rewrites the group's tab indices, which means 3–N extra
  `CallbackChange`s. The segmented test helper `restyle_writes` now skips them.

## Still open

- **Accessibility states** do not follow keyboard (or click) changes live: radio
  `CheckedTrue/False`, and tab and listbox selection. There is no `aria-selected`,
  `aria-expanded` or `aria-level` on tree rows. Roles: tree rows are `Outline`, list rows
  are `List`, segments are `PageTabList`. Only the roving part (focusability) is done.
- **DatePicker:** arrows do not cross into the neighbouring month. The grid cannot
  rebuild itself (module TODO2), so the edge arrow is swallowed. PageUp/PageDown change
  the month through the host's rebuild, but do not land focus on "the same day" of the
  new grid. Where focus goes after a rebuild is up to the reconciler's node remap.
- **TreeView:** expand and collapse only work if the app installs `on_node_toggle`.
  `examples/c/widgets.c` does not (its click toggles and selects). After the app
  rebuilds, the stop returns to the selected row.
- **ListView:** for the stop to survive a rebuild, the app must store the row reported
  by `on_row_click` into `selected_row`. ListView still paints no selection highlight.
  There is no PageUp/PageDown and no typeahead (APG optional). The same goes for tree
  typeahead.
- **Tabs:** a click does not move the stop by itself (the click handler is stateless by
  design and pushes no changes).
- There is no headless end-to-end test through the real DLL key dispatch
  (`process_window_events` → focus callback → `SetFocusTarget` + `SetNodeTabIndex`
  applied). The widget tests call the registered handler directly. The only engine-side
  check is the runner test (e2e-server feature).
- Pagination and stepper buttons (named in §D) were out of scope and are still one stop
  per button.
