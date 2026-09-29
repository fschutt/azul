# S2_FOCUS_LEFTOVERS - progress

Branch `wt/s2-focus-leftovers`, based on `0a326afe5`. Nothing compiled (house rule).

## Step 1: audit (2026-09-29, against 0a326afe5)

| # | Item | Status | Evidence |
|---|---|---|---|
| 1 | ComboBox active-descendant highlight | STILL OPEN | `combobox.rs` `on_combobox_key_down_inner` handles Backspace only. The list has no key handler and no "active option" state. A navigation key reaches the list popup (`forward_keys_to_popup` forwards list-navigation keys) and `focus_list_popup_on_navigation` (dll `common/event.rs`) moves REAL focus into the popup's first option, so focus leaves the field (tests `down_delivered_to_an_unfocused_list_popup_focuses_its_first_option`, `down_in_the_combobox_field_moves_into_its_open_list` pin that model). No option ever carries an `AccessibilityState::Selected`. |
| 2 | macOS / Win32: the list popup becomes the KEY window | STILL OPEN | macOS: every popup child is `AzulPopupWindow` with `canBecomeKeyWindow = YES` (`macos/mod.rs` ~170) and is shown with `makeKeyAndOrderFront`. Win32: `popups_route_keys_natively` doc says `ShowWindow(SW_SHOWNORMAL)` activates the owned popup; no `SW_SHOWNOACTIVATE` / `WS_EX_NOACTIVATE` / `WM_MOUSEACTIVATE` anywhere. Both answer `popups_route_keys_natively() = true` for EVERY popup, so `forward_keys_to_popup` returns early and a typed key goes to the key popup (a `<p>` with no text handler) - lost. Wayland has the same bug: `handle_key` forwards every key to `active_popup`, a list popup included. |
| 3 | `:backdrop` in stylesheet rules | STILL OPEN (and the inline half is weaker than reported) | `core/src/prop_cache.rs` collects stylesheet rules per pseudo-state (`collect_and_assign!`) for Normal/Hover/Active/Focus/SeatFocus/Dragging/DragOver/Placeholder - no Backdrop, so a `.x:backdrop {..}` rule is dropped at cascade time (`rule_ends_with(None)` also rejects it as interactive). `get_property_slow` (the paint-time resolver) has no `:backdrop` tier either: its inline matcher compares `PseudoState(s) == state` and never queries Backdrop, so only `DynamicSelector::matches` (`match_pseudo_state`, fixed in f3b04e0e8) knows `!ctx.window_focused`. |
| 4 | Stepper: every step is its own Tab stop | STILL OPEN | `stepper.rs` `build`: every step cell `.with_tab_index(TabIndex::Auto)` with role `SpinButton`, only a `Hover(Click)` handler, no key handler. (The time picker's per-column up/down arrows are the literal +/- buttons: `time_picker.rs` `build_spinner_skinned`, each arrow `TabIndex::Auto`.) |
| 5a | Roving widgets: a11y checked/selected/expanded not live | STILL OPEN | `set_accessibility_state` exists (`CallbackInfo`, applied in dll `event.rs` and the e2e runner) and is used by check_box/switch/accordion only. radio_group sets `CheckedTrue/False` at BUILD only; segmented, tabs, list_view, tree_view publish no selected/expanded state at all; none of their click/key handlers update a11y. |
| 5b | Date grid arrows do not cross months | STILL OPEN | `date_picker.rs` `on_day_key`: "an arrow past the displayed month is consumed and goes nowhere" (module TODO2: the grid cannot rebuild itself). |
| 5c | Tree expand needs the app's `on_node_toggle`; `examples/c/widgets.c` does not set it | STILL OPEN | `widgets.c` sets only `AzTreeView_setOnNodeClick`; `set_on_node_toggle` is in api.json (60622). |
| 6 | Torn-off palette's invoker (open question 4) | STILL OPEN | `TransientWindowManager::apply_drop` / `reconcile` recreate the window on a tear-off but keep `focus_before_open`; `remember_focus_for_opened` skips a node that already has a record. Closing the torn palette (`dismiss`) therefore still pays focus back to the swatch, wherever the user has since moved it. |
| 7 | Escape from the picker -> swatch, next Tab -> the stop AFTER it | DONE headless (guards exist) | `dll/tests/transient_window_layout.rs`: `escape_in_the_picker_closes_it_and_tab_moves_on_from_the_swatch` (Escape to the popup, then Tab lands on `.stop-after`) and `escape_in_the_parent_closes_the_picker_and_shift_tab_moves_back_from_the_swatch`; both green in the batch-2 suites (dll transient 37). Not covered: the macOS activation round trip (parent `WindowFocusOut` when the popup takes key, `WindowFocusIn` when it closes) - see NEXT. |

## Decisions

- **Item 6 (torn-off palette).** Decided per the focus report's own reasoning:
  `transient_keyboard_owner` already treats a torn-off palette as "a window of
  its own that the user clicks into", not a popup holding the parent's
  keyboard. So a torn palette owes nobody focus: tearing off forgets the
  focus recorded at open, a palette opened already torn records none, and
  closing it leaves focus where the user last was (the parent's own focus is
  untouched; the OS re-activates whatever window it picks).
- **Items 1+2 (combobox).** WAI-ARIA combobox: DOM focus never leaves the
  field. A popup that keeps focus on its invoker (role List / MenuPopup /
  DropList / Outline / Tooltip) is shown WITHOUT activation on every backend,
  so every key reaches the parent. The parent forwards only the list
  navigation keys to the list popup through the mailbox (the one shared rule,
  now also on macOS / Win32 / Wayland). The list's own key handler (in the
  popup window, where its option nodes live) moves an ACTIVE option: visible
  highlight + `AccessibilityState::Selected`; Enter picks it; the popup's
  engine focus stays None. Down / Up on a closed field opens the list.

## DONE

- 1430889fe audit (this file).
- Item 6: a74bcd02e RED (3 focus_return_tests + 1 guard), 370f9c0d8 fix.
- Item 5c: ce6b04250 widgets.c sets `AzTreeView_setOnNodeToggle`.

## IN PROGRESS

- Item 3.

## NEXT

3. Item 3: stylesheet `:backdrop` (prop_cache collection + resolver tier), layout test.
4. Item 4: stepper one tab stop + arrows (roving); decide on the time-picker columns.
5. Item 5a: a11y live states in radio / segmented / tabs / list / tree.
6. Items 1+2: combobox active descendant + non-activating keep-focus popups.
7. Item 5b: date grid across months (needs a post-rebuild focus target).
8. Item 7: device-faithful variant with the parent's activation round trip.

## Open questions

- none yet
