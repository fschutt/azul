# S2: the focus / popup leftovers of 2026-09-28 (report, 2026-09-29)

Branch `wt/s2-focus-leftovers`, based on `0a326afe5`. **Nothing was compiled
or run** (house rule). Every "RED today" below is a prediction from reading
the code; the parent's compile and suites confirm or refute it.

Sources: `scripts/FOCUS_SUBWINDOW_AND_ARROW_KEYS_FIX_2026_09_28.md` (its
"Still open" and open question 4), `..._ANALYSIS_2026_09_26.md` §C-§E,
`scripts/ROVING_TABINDEX_2026_09_28.md` ("Still open"),
`scripts/POPOVER_DIALOG_REMODEL_2026_09_28.md`, the ledger's focus items.

## 1. Audit (step 1) and outcome

| # | Item | At `0a326afe5` | Now |
|---|---|---|---|
| 1 | ComboBox active-descendant highlight | STILL OPEN: the field knew only Backspace; the first navigation key moved REAL focus into the list's first option (`focus_list_popup_on_navigation`) | DONE |
| 2 | macOS / Win32: the list became the KEY window, typed text lost | STILL OPEN: every popup was an `AzulPopupWindow` (`canBecomeKeyWindow = YES`) / activated by `SW_SHOWNORMAL`; `popups_route_keys_natively` switched the parent's forwarding off for all popups. Wayland had the same bug (`handle_key` routed every key to any `active_popup`) | DONE (macOS, Win32, Wayland; device check needed) |
| 3 | `:backdrop` in stylesheet rules | STILL OPEN, and worse than recorded: the parser rejected the name, the cascade never collected the rules, and the paint-time resolver never asked for the state - so even the titlebar's INLINE `background_inactive` never painted | DONE |
| 4 | Stepper: every step a Tab stop | STILL OPEN | DONE (one spin button) |
| 5a | a11y checked / selected / expanded not live | STILL OPEN (radio: build only; segmented, tabs, list, tree: nothing) | DONE |
| 5b | Date grid arrows do not cross months | STILL OPEN ("consumed and goes nowhere") | DONE |
| 5c | Tree expand needs `on_node_toggle`; `widgets.c` did not set it | STILL OPEN | DONE |
| 6 | Torn-off palette's invoker (open question 4) | STILL OPEN (a torn palette still paid focus back to the swatch on close) | DONE, decided below |
| 7 | Escape -> swatch, Tab -> the stop after it | DONE headless (two guards existed) | + two device-shaped guards |

## 2. What was built

### Item 6: the torn-off palette owes no focus back (decision)
Decided by the focus report's own reasoning: `transient_keyboard_owner`
already treats a torn-off palette as "a window of its own that the user
clicks into", not a popup holding its swatch's keyboard. So closing it leaves
focus where the user last was. One seam: `TransientWindowManager::recreate`
(every change of kind: a drag, `set_transient_window_torn`, the `torn`
attribute) forgets the focus recorded at open when the window becomes a
toplevel; a window that opens already torn forgets what the callback seam
recorded; `remember_focus_for_opened` records nothing for a torn window.
Docked back, it is a popup again and the next reconcile records the parent's
current focus for it.

### Item 5c
`examples/c/widgets.c` sets `AzTreeView_setOnNodeToggle` (Right / Left open
and close rows through the demo's expanded bit set).

### Item 3: `:backdrop`
- `css` parser2: `backdrop` is a pseudo-class name.
- `CssPropertyCache::restyle`: stylesheet rules ending in `:backdrop` are
  collected under `PseudoStateType::Backdrop`; the inheritance walk carries a
  backdrop tier only for a DOM that declares one (stylesheet or inline).
- `get_property_slow`: a `:backdrop` tier (inline, stylesheet, inherited)
  below the interaction states.
- `StyledDom::sync_backdrop_state` raises `StyledNodeState::backdrop` while
  the cascade context says the window is inactive, on exactly the nodes that
  declare or inherit a `:backdrop` value (so every other node keeps the
  `is_normal()` compact fast paths). Called when a DOM is created, on every
  `restyle`, and on every new context (`apply_window_activation` offers one on
  each (de)activation and rebuilds the display lists).

### Item 4: the stepper is one spin button
Built on the shared `widgets::roving` helper: the current step is the one Tab
stop; every step carries `on_step_key` (Up / Right forward, Down / Left back,
Home / End, holding at the ends, consumed even at an end; modified keys left
alone). The click body became `go_to_step_cell` (moved as is behind a
two-line `on_step_click`), shared by click and key; it ends by moving the Tab
stop and announcing the new value ("step N of M", `step_value`) on every
step. (Interpretation: the ledger's "+/- buttons" is §D's "stepper buttons":
the Stepper widget, whose every step declares `SpinButton`. The time picker's
per-column arrows are the same class of defect, see §6.)

### Item 5a: live a11y states
One helper, `roving::announce_chosen(info, items, chosen, on, off)`:
- RadioGroup: click and arrow announce CheckedTrue / CheckedFalse.
- Segmented: a segment is a `RadioButton` (it declared the LIST role,
  `PageTabList`, and no state), built with Checked* and announced live.
- TabHeader: header `PageTabList`, tabs `PageTab`, the active one `Selected`
  (build + live on an arrow's activation). They declared nothing.
- ListView: rows `ListItem` (they declared `List`), `selected_row` is
  `Selected`, live on an arrow.
- TreeView: the tree is the `Outline`, rows `OutlineItem` (they declared
  `Outline`) with Expanded / Collapsed (parents) and Selected, from the node
  model (`row_states`) - the tree changes only through the app's rebuild.

### Items 1 + 2: WAI-ARIA combobox, end to end
- A popup that LEAVES focus on its invoker (role List / MenuPopup / DropList
  / Outline / Tooltip; today only the combobox's list) is never the key /
  active window: macOS `AzulListPopupWindow` (`canBecomeKeyWindow = NO`,
  ordered front without key status, tracking area `ActiveInActiveApp` so its
  rows still hover); Win32 `WS_EX_NOACTIVATE` + `SW_SHOWNOACTIVATE`
  (`Win32Window::show_command`, now the one place both show sites ask);
  Wayland `handle_key` routes only a focus-TAKING popup natively.
- One routing rule, `common::transient::parent_key_route`: the parent keeps
  every key and forwards a list popup's navigation keys (Up / Down / Home /
  End / PageUp / PageDown / Enter) on every backend; the native opt-out now
  concerns the focus-taking popup only. `deliver_forwarded_keys` replays at
  once on macOS / Win32 (the registry, as X11 does) and Wayland
  (`active_popup`). `focus_list_popup_on_navigation` is removed.
- `dismiss_list_popups_on_deactivation`: a list popup has no focus loss of its
  own, so it closes with its parent's deactivation; a focus-taking popup is
  left alone (its parent resigns TO it).
- The widget: options are no Tab stops; the list's window key handler
  (`on_combobox_list_key`, acting only in the popup window's copy) moves the
  ACTIVE option - marker class `__azul-native-combobox-option-active`, the
  theme's option-hover fill (`flat::/flora::COMBOBOX_ACTIVE_OPTION`, appended
  to the theme files, picked by the list's theme marker and the window's
  mode), `Selected`; Enter picks it through the click's own body
  (`pick_option`) or just closes the list; the field says Collapsed /
  Expanded (build + live on click, Down / Up, dismissal); Down / Up on a
  closed field opens the list.

### Item 5b: the date grid crosses months
- `on_day_key`: an arrow past the displayed month turns the calendar to the
  day it aimed at (`shifted_date`, across months and years) through
  `move_to_date`, which `month_nav` (‹ / ›, PageUp / PageDown) now ends in
  too. The key is consumed; the calendar stays open.
- The grid: day cells are keyed by their date (a rebuild onto another month
  UNMOUNTS the focused day instead of handing its focus to the cell in its
  slot); the Tab-stop day carries `autofocus` - so the calendar now OPENS on
  the selected day (APG) instead of on its first header button.
- `common/layout.rs`: a popup whose focused node the rebuild unmounted
  focuses its `autofocus` node before the runtime states are applied.

### Item 7
Two more guards: the macOS / Win32 activation round trip (the parent resigns
when the picker takes key, Escape in the picker, WindowFocusIn before the
dismissal is read), and the demo-shaped app that rebuilds on every picked
colour. Both expected GREEN. If the device still restarts at 0: a RED in the
rebuild guard is the bug; if both are green, run the device with
`AZ_FOCUS_TRACE=1` - the backend's own activation path is the suspect.

## 3. Commits

| Commit | Kind | What | Expected RED before its fix |
|---|---|---|---|
| 1430889fe | chore | audit (progress file) | - |
| a74bcd02e | test | torn palette owes no focus back (3 + 1 guard, layout lib `transient::focus_return_tests`) | pending restore Some((swatch, true)), expected None |
| 370f9c0d8 | fix | item 6 | - |
| ce6b04250 | feat | widgets.c `on_node_toggle` | - |
| cc315ff95 | test | `layout/tests/backdrop_follows_window_activation.rs` (4) + css `backdrop_is_a_pseudo_class_the_stylesheet_parser_knows` | GREEN (#00ff00) where GREY (#808080) is expected; parser Err(UnknownSelector) |
| 6e28a514d | fix | item 3 | - |
| 7bf41a0df | test | stepper (8) | 1 handler per step (expected 2); walk [step0, step1] (expected [step1, after]); key tests panic "every step must carry the spin button's key handler" |
| 4ceaf3df0 | fix | item 4 | - |
| c8a6999ec | test | a11y (9 tests over radio / segmented / tabs / list / tree) + `rv::announced_states` / `rv::declared` | announced [], declared (PageTabList/List/Outline, []) or None |
| b203042a0 | fix | item 5a | - |
| 53e3ec605 | test | combobox: `parent_key_route` (written with today's rule) + unit test; 5 dll tests + 2 guards; 5 combobox unit tests | native=true routes Down to Parent (expected ListPopup); popup focus Some(option 0) (expected None); 0 open windows (expected 1); list stays open on deactivation; options are Tab stops; field states [] |
| 03ee7ff0e | fix | items 1 + 2 (+ list-handler unit tests) | - |
| 05b4b0470 | test | date grid: 2 date_picker tests + 1 dll end-to-end | state stays 2024-02-14 / 02-29; no key / autofocus; dll panics "the calendar asks focus for its selected day" |
| a27e2b747 | fix | item 5b | - |
| 715972c92 | test (guard) | Escape through the activation round trip | expected GREEN |
| 8997527ac | test (guard) | Escape after the app rebuilt on the picked colour | expected GREEN |
| 352f748e5 | fix | macOS narrowing: only a list popup skips key status / tracks app-wide | - |
| 102a08be8 | style | win32 doc placement | - |

Plus `chore(s2): progress` checkpoints after each item.

Guards that are GREEN before and after: `a_docked_palette_still_owes_focus_back_to_its_swatch`,
`enter_in_the_field_picks_the_active_option_and_closes_the_list` (green today by another path:
focus in the list + activation), `a_focus_taking_popup_survives_its_parent_resigning_the_keyboard`,
the two item-7 guards; the active-window halves of the `:backdrop` tests.

For the combined RED pass (`git apply -R` per fix): 370f9c0d8, 6e28a514d, 4ceaf3df0, b203042a0,
03ee7ff0e (+ 352f748e5 on top of it), a27e2b747. The combobox unit tests for the list handler live in
03ee7ff0e (they name the new handler), as do the adjusted
`dismissed_with_no_text_keeps_the_placeholder` (now checks text writes only).

## 4. api.json

**No api.json change is required.** No `#[repr(C)]` struct changed layout, no public widget type
gained a field. New public-but-not-in-api.json items:
- `StyledDom::sync_backdrop_state(&mut self)` (azul_core; optional to add, not needed by bindings).
- dll: `common::transient::{ParentKeyRoute, parent_key_route, dismiss_list_popups_on_deactivation}`;
  `macos::ListPopupWindow`; Win32 constants `WS_EX_NOACTIVATE`, `SW_SHOWNOACTIVATE`.
- The C demo calls the EXISTING `AzTreeView_setOnNodeToggle`.

## 5. Least sure to compile

1. `macos/mod.rs`: the new `define_class!` `ListPopupWindow` (a copy of `PopupWindow`); in the two
   views' `updateTrackingAreas`, `self.window().is_some_and(|w| w.isKindOfClass(ListPopupWindow::class()))`
   (objc2-app-kit 0.3.2 `NSView::window` is safe; `isKindOfClass` via `NSObjectProtocol`, `ClassType`
   imported); the first-show `orderFront(None)` outside an `unsafe` block.
2. Win32 (not compiled on this host): `wcreate::create_hwnd` reading `&options.window_state`;
   `show_command` / `deliver_forwarded_keys` calling the private `route_main_window_result`.
3. Wayland (not compiled on this host): `deliver_forwarded_keys` reaching `popup.common` and the
   private `WaylandPopup::apply_event_result` from `impl PlatformWindow for WaylandWindow` (same
   module); `handle_key`'s `popup_holds_keyboard` closure.
4. `common/event.rs` `forward_keys_to_popup`: the `match (route, owner, list)` moving the two
   `Option<RefAny>`s.
5. `common/layout.rs`: the popup re-focus block uses `styled_dom` (the new composed DOM, before
   `apply_runtime_states_before_layout`) and `focus_manager.set_focused_node`.
6. `core/styled_dom.rs` `sync_backdrop_state`: the flags are computed under an immutable borrow and
   written in a second loop through `styled_nodes.as_container_mut().get_mut(..)`.
7. `core/prop_cache.rs`: `let mut any_backdrop` (mutated only inside the non-empty-sheet branch).
8. `combobox.rs`: `data.downcast_mut::<ComboBoxStateWrapper>()?.inner.open = true;` through a
   temporary `RefMut`; the test module imports `WindowEventFilter` explicitly beside `use super::*`.
9. `stepper.rs` tests: `use azul_core::{dom::TabIndex, window::VirtualKeyCode};` in the middle of
   the module (the top-level `TabIndex` import was removed as unused outside tests).
10. `date_picker.rs` `on_day_key`: `let (year, month) = { let mut probe = shared.clone(); let Some(w)
    = probe.downcast_ref::<DatePickerData>() else {..}; (..) };` and `shared` moved in two exclusive
    branches.
11. `layout/tests/backdrop_follows_window_activation.rs`: `getters::get_style_properties(sd, TEXT,
    None, PhysicalSize::new(800.0, 600.0)).color`.

## 6. Test commands for the parent

```
cargo test --release -p azul-css --lib parser2
cargo test --release -p azul-core --lib
cargo test --release -p azul-layout --lib -- transient:: widgets::stepper widgets::radio_group \
  widgets::segmented widgets::tabs widgets::list_view widgets::tree_view widgets::combobox \
  widgets::date_picker widgets::roving
cargo test --release -p azul-layout --test all backdrop_follows_window_activation
cargo test --release -p azul-dll --lib --features build-dll common::transient
cargo test --release -p azul-dll --test transient_window_layout
```
(then the usual full suites: core, css, layout --lib, layout --test all, dll --lib, dll
transient_window_layout.)

## 7. Behaviour changes to know

- `:backdrop` now PAINTS (inline and stylesheet): the titlebar's `background_inactive` shows on
  every inactive window. Headless windows default to active, so goldens do not move.
- Combobox: options are never focused; on macOS / Win32 the list is non-activating; Enter / arrows
  go through the field's window; the list closes when its window is deactivated.
- Date picker: the calendar opens on the selected day; arrows cross months (the date moves with
  them, like ‹ / ›).
- Stepper: one Tab stop; arrows step it (and fire `on_step_change`).
- Accessibility roles: segments RadioButton, tabs PageTab / header PageTabList, list rows
  ListItem, tree Outline / rows OutlineItem.
- A torn-off palette no longer pulls focus back to its swatch when closed.

## 8. What is left

- **Time picker**: its per-column up / down arrows are still four Tab stops. The spinbutton pattern
  says the column value is the one stop (Up / Down, PageUp / PageDown, Home / End); the tests pin the
  arrows as Tab stops today (`a clickable time-picker cell is not keyboard-focusable`,
  `only_retext`), so it is a separate change.
- **Combobox, G2's area**: typing while an option is active keeps the option active (APG clears it),
  and Tab / blur does not close the list - both belong in the field's text / focus handlers G2 edits.
- `aria-activedescendant` cannot cross windows: the field cannot name the option in the popup's
  tree; the active option announces `Selected` in the popup's own tree instead.
- **Device checks** (not verifiable headless): clicks and hover in the non-key list popup (macOS),
  `WS_EX_NOACTIVATE` popups (Win32), the Wayland list route; and item 7 on the device.
- Layout-affecting `:backdrop` properties still wait for the next relayout (paint-only ones follow at
  once).
- The popup re-focus after an unmount does not dispatch `FocusReceived`.
- The e2e runner (`layout/src/e2e/runner.rs`) mirrors none of the transient changes (it never
  reconciled transient windows).
- Twins found, not unified (other agents own the files): `window_is_dark` / `renders_dark` are four
  copies (date_picker, segmented, stepper, pagination); `get_property_slow` repeats its three-tier
  per-pseudo-state block nine times (the new `:backdrop` tier included).
