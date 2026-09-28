# Focus across the picker sub-window + arrow keys: the fix wave (2026-09-26 .. 28)

Branch `wt/focus-subwindow-arrows`, based on `39a96b9bc` (the tip of `fix/input-bugs-2026-09-19`).
It implements `scripts/FOCUS_SUBWINDOW_AND_ARROW_KEYS_ANALYSIS_2026_09_26.md` §E.

**Nothing was compiled or run.** There was no cargo, no rustc and no rust-analyzer. Every
"expected RED" below is a prediction from reading the code, and the parent's single compile
confirms or refutes it. Each defect has one test commit with the RED test alone, then one fix
commit.

## User rulings applied

- **Picker large step.** Cmd+arrow on macOS, Ctrl+arrow elsewhere
  (`mac_shortcut_conventions`, so X11-on-Mac follows Linux). Shift+arrow and PageUp/PageDown also
  take the 10% step, and Home/End go to the ends. Alt+arrow and the other command modifier fall
  through. One pure function, `picker_step(keyboard, mac_conventions)`, has a per-convention test.
- **Escape in the picker closes it.** It does not cancel or restore the colour. A typed hex value
  commits on the way out through the blur. Focus and a visible ring go back to the swatch. Tab
  continues from the swatch, and Shift+Tab goes back from it.

## Commits and expected RED values

| Commit | Kind | Test and expected RED value before its fix |
|---|---|---|
| 15d8e07c7 | test (guard) | `an_arrow_in_the_picker_popup_moves_saturation_by_one_percent`: expected **GREEN** today. A key sent to the popup already reaches the plane. |
| 5eb6eb455 / 1a43f7d7f | test / fix | `no_focus_ring_while_the_window_is_inactive`: rings = 1, expected 0. |
| f08bbe92c / c1a3e08e7 | test / fix | `backdrop_matches_when_the_window_is_unfocused`: `matches` = false. `the_context_sees_a_deactivation_reported_through_window_focused`: `ctx.window_focused` = true. |
| efda9ac6f / 298d73ac5 | test / fix | `a_keyboard_opened_picker_leaves_exactly_one_ring`: parent rings = 1, expected 0. |
| ca1dfe06e / 2341ba37c | test / fix | `escape_in_the_parent_closes_the_picker_without_blurring_the_swatch` (P1-6): parent focus = None, expected the swatch. |
| ff0ec779f / 48fe21255 | test / fix | `a_key_the_parent_receives_while_its_picker_is_open_drives_the_picker` (P0-2): saturation delta 0.0, expected 0.01. The parent-ring assert would also fail (1, expected 0). |
| 990d868e2 / b1bccac6b | test / fix | `a_combobox_list_popup_does_not_take_focus` (P0-3): popup focus = Some(first option), expected None. |
| 56f4ec097 / 8aac4da6b | test / fix | `down_delivered_to_an_unfocused_list_popup_focuses_its_first_option` and `down_in_the_combobox_field_moves_into_its_open_list`: popup focus None, expected the first option. |
| bda63fc5d | feat | P0-4: `AZ_FOCUS_TRACE` logs each KeyDown's window, whether it is a popup, the key, the target, the focus and whether it was prevented, plus the default action chosen. Log only, no test. |
| 79bd1331a | refactor | Extracts `picker_step`. No behaviour change. |
| 865e0d760 / dd7f1b8b9 | test / fix | P1-5 plus the ruling. `the_large_step_is_…_or_shift`: Some(0.01) where Some(0.1) is expected. `shift_right…`: s 0.81, expected 0.90. `page_up_and_down…`: v 1.0, expected 0.9. `home_and_end_on_the_plane…`: s 0.8, expected 0. `…hue_and_alpha…`: h ~11, expected 0. `alt_arrow_is_not_consumed…`: s moved to 0.81. `the_hue_bar_wraps_around`: h ~7.4, expected ~335. `right_on_the_plane…` is a guard and passes today. |
| 8f690fa2b / 458ca7503 | test / fix | `a_popup_opened_by_its_attribute_owes_focus_back` (P1-7): `take_pending_focus_restore()` = None, expected Some((swatch, true)). The fix also gives the e2e runner the same focus bookkeeping (C11). |
| d596053e3 | test (guard, after its fix) | `a_callback_closed_popup_hands_focus_and_ring_back_in_the_runner`: this covers the runner half of 458ca7503. Before that commit, focus stays None. |
| 748660a1a / 990a90618 | test / fix | `a_light_dismiss_blurs_the_popups_focused_field_before_it_closes` (P1-8): blur count 0, expected 1. |
| abb465ecd | test (guards) | Escape ruling, two paths: `escape_in_the_picker_closes_it_and_tab_moves_on_from_the_swatch` (Escape sent to the popup) and `escape_in_the_parent_closes_the_picker_and_shift_tab_moves_back_from_the_swatch` (Escape sent to the parent). Both are expected **GREEN** on this branch; see "Open" below. |
| dbd1be12d / c62c491ed | test / fix | Escape ruling in the e2e runner: `escape_closes_a_widget_opened_picker_and_tab_continues_from_its_swatch`. `forced_open_nodes().len()` stays 1, expected 0. Past that assert, focus is None and Tab lands on `.stop-before`. |
| c160d8a73 / 62f552fe1 | test / fix | `ctrl_or_alt_arrow_on_a_button_has_no_default_action` (P2-10): the action is FocusUp or a scroll, expected None. |
| be7082aa2 / 643fa0a08 | test / fix | `a_popup_autofocuses_only_once` (P2-11): popup focus = Some(plane), expected None. |
| 45163d322 / 39ef5e639 | test / fix | P1-9, as pure helpers in `common/transient.rs`. The test commit writes the helpers with today's behaviour. Routing: Parent is returned where Popup is expected. Grab serial: 7, expected 42. |

## What the fixes do

- **Ring and activation.** The focus ring is painted only while its window is active
  (`FullWindowState::is_window_active` = `window_focused && flags.has_focus`, the one reading of
  both flags). It is also suppressed while a focus-taking popup holds the window's keyboard
  (`LayoutWindow::transient_keyboard_owner`). The ring is rebuilt on WindowFocusIn/Out
  (`apply_window_activation`, which also re-offers the cascade context for `:backdrop`), in the
  transient reconcile, after a popup posts its dismissal, and when focus is owed back.
- **Key routing (P0-2).** `forward_keys_to_popup` is shared, runs at depth 0 after dismissal, and
  forwards through the mailbox (`ForwardedKey`). The popup replays the forwarded keys. X11
  delivers them at once through the registry. macOS, Win32 and Wayland opt out through
  `popups_route_keys_natively`.
- **Focus model (P0-3).** It is derived from the role of the popup's content root in
  `transient_takes_focus`. `List`, `MenuPopup`, `DropList`, `Outline` and `Tooltip` keep focus on
  the invoker. The ComboBox list now has the `List` role. For such a list popup, only navigation
  keys are forwarded, and the first navigation key moves focus into the list.
- **Closing a popup.** It blurs its focused control first (P1-8). Escape in the parent is consumed
  (P1-6). Focus owed back is recorded for popups opened through their attribute (P1-7).
- **Wayland (P1-9).** An enter or leave on the popup's own surface no longer toggles the parent.
  A leave on it becomes the popup's own focus loss. `xdg_popup.grab` uses `last_input_serial`.

## Public type/field changes (for the api.json autofix)

- `FullWindowState` is in api.json and gains a new method, `is_window_active()`. No fields changed.
- None of the following are in api.json:
  - `LayoutWindow` gains four pub methods: `apply_window_activation`, `transient_keyboard_owner`,
    `transient_list_popup` and `refresh_focus_ring`.
  - `azul_layout::transient` gains `transient_takes_focus` and
    `TransientWindowManager::remember_focus_for_opened`.
  - dll `TransientWindowData` gains the fields `forwarded_keys`, `takes_focus` and `autofocused`.
  - dll adds new pub items: `ForwardedKey`, `KeyboardFocusSurface`, `keyboard_focus_surface`,
    `popup_grab_serial`, `popup_takes_focus`, `popup_autofocused`, `mark_popup_autofocused`,
    `keyboard_owner_mailbox`, `list_popup_mailbox`, `is_list_navigation_key`, `forward_key`,
    `take_forwarded_keys` and `has_forwarded_keys`.
  - `PlatformWindow` gains six trait methods, all with defaults: `consume_keyboard_delta`,
    `popups_route_keys_natively`, `deliver_forwarded_keys`, `forward_keys_to_popup`,
    `focus_list_popup_on_navigation` and `blur_focus_before_close`.
- No `#[repr(C)]` struct changed layout.

## Least sure to compile

1. `dll/.../linux/wayland/events.rs` and `wayland/mod.rs` are not compiled on this macOS host.
   Risky spots: the private `WaylandPopup.surface` read from `wayland::events`, and
   `popup.keyboard_left()` / `apply_event_result`.
2. `dll/.../windows/mod.rs`: `popups_route_keys_natively` in `impl PlatformWindow for Win32Window`
   is not compiled here.
3. `x11/mod.rs` `deliver_forwarded_keys`: `if let LinuxWindow::X11(popup)` is irrefutable on
   macOS, which gives a warning only. It also calls the private `apply_event_result` from the
   trait impl.
4. `event.rs` `forward_keys_to_popup`:
   - `self.get_layout_window_mut().filter(|_| …).and_then(…)`;
   - the `retain_mut` closure that captures `typed`;
   - `ForwardedKey { keyboard, previous_key, text }` built after `keyboard.current_virtual_keycode`
     is read.
5. `event.rs` popup replay loop: the partial move `key.keyboard`, then `key.text.as_deref()`.
6. `event.rs` `blur_focus_before_close`: `SyntheticEvent::new(.., azul_core::task::Instant::now()
   ..)` and the `Update` path.
7. `color_input.rs` tests: `with_info_keys` copies `with_info_cursor`, including
   `system::SystemStyle`. `picker_control` indexes `styled.node_data.as_ref()`.
8. `runner.rs`: the `matches!(…, Some(NodeType::TransientWindow(cfg)) if cfg.dismiss != …)` guard
   in `dismiss_popups_on_escape`.
9. dll test helpers: `focus_rings` pattern-matches `DisplayListItem::Border { colors, .. }` with
   `CssPropertyValue::Exact(c)` and `c.inner`.

## Verified vs assumed, per platform

- **Engine and headless:** covered by the tests above. None of them has been run.
- **X11:** keys are forwarded and delivered through the registry, Escape is consumed by the parent,
  and the parent's ring is hidden while the popup is open. This follows from reading the code and
  is not checked on a device.
- **macOS / Win32:** the popup is assumed to be the key/active window, so native routing applies.
  The P0-1 ring, `:backdrop`, blur and autofocus-once changes are shared code.
  - A combobox list popup still becomes the key window, so typing into it is still lost.
- **Wayland:** the routing and serial changes are uncompiled here. Only the helper unit tests run
  on this host.

## Still open

- **The device symptom "Tab restarts at the first stop after Escape"** was not reproduced by
  reading. The only engine-side focus loss I found on that path was the parent's ClearFocus, fixed
  in 2341ba37c. Both Escape paths are guarded in abb465ecd. The e2e runner did reproduce it (it had
  no Escape dismissal) and is fixed in c62c491ed.
  - If the guards pass and the device still fails, run with `AZ_FOCUS_TRACE=1`. The log shows the
    window each key reached, the focus, and the default action.
- **P2-12, WAI-ARIA roving tabindex** (RadioGroup, Segmented, Tabs, ListView, TreeView, DatePicker
  grid) was not started, as agreed.
- **Combobox active-descendant highlighting.** The list takes real focus on the first navigation
  key. It does not highlight an active descendant while focus stays on the field.
  - On macOS and Win32 the list popup still becomes the key window, so text typed while it is open
    is lost. Fixing that needs `orderFront` / `SW_SHOWNOACTIVATE` for keep-focus popups plus native
    delivery of the navigation keys.
- **`:backdrop` in stylesheet rules** (as opposed to inline conditional declarations) still reads
  the per-node flag, which nothing sets.
- **Layout-affecting `:backdrop` properties** wait for the next relayout.
- **The e2e runner fires no transient lifecycle events,** so a widget's `Dismissed` handler never
  runs there.
- **Tear-off (C9).** A new torn window autofocuses once again (fresh mailbox) and re-reads the
  parent's current modality.
- **Open question 4** (the invoker of a torn-off palette) is unchanged.
