# L1: small widget leftovers (report, 2026-09-29)

Branch `wt/l1-widget-leftovers`, based on `d9ce25179`. **Nothing was compiled
or run** (house rule). Every "RED today" below is a prediction from reading
the code; the parent's compile and suites confirm or refute it.

Source: `scripts/S2_FOCUS_LEFTOVERS_2026_09_29.md` section 8 and the ledger.

## 1. Outcome

| # | Item | Outcome |
|---|---|---|
| 1 | Time picker: the per-column arrows are four Tab stops | DONE: each column is ONE spin button (APG) |
| 2a | Combobox: typing keeps the active option active | DONE (decision); the popup's stale highlight stays until its next key (see 5) |
| 2b | Combobox: Tab / blur does not close the list | DONE |
| 3 | `window_is_dark` / `renders_dark` twins (4 copies) | ALREADY GONE in the base - nothing to do |
| 4 | `get_property_slow` repeats its three-tier block 9 times | DONE: one loop, no behaviour change |

## 2. What was built

### 1. Time picker: a column is one spin button
- The spinner COLUMN (`__azul-native-time-picker-spinner`) is the Tab stop:
  `TabIndex::Auto`, role `SpinButton`, named for its unit ("Hour" /
  "Minute"), its value the readout's text ("9", "05"), and a
  `Focus(VirtualKeyDown)` handler (`on_hour_key` / `on_minute_key` ->
  `spin_on_key`).
- Keys: Up / Down one step, PageUp / PageDown the large step (2 hours, 15
  minutes - react-aria's time field steps), Home / End the ends of the band
  (0 / 23, or 1 / 12 in 12-hour mode; 0 / 59). All go through
  `adjust_spinner_at`, the body an arrow click and the wheel already take
  (clamp, retext, `on_change` - which also fires for a step clamped away, as a
  click does). A handled key is `prevent_default`ed, also at an end; modified
  keys (Alt / Ctrl / Cmd / Shift) and other keys are left alone - the
  stepper's rule, reused (`roving::plain_key`), not copied. Focus stays on the
  column.
- The arrows keep their click handler, role `PushButton` and name
  ("Increase hour") but lose their tab index: a click on one focuses its
  column (the engine focuses the nearest focusable ancestor,
  `managers::hover::focusable_under_pointer`), so the keys work right after a
  click.
- `adjust_spinner_at` announces the column's new value live
  (`set_accessibility_value`) on every change - key, click, wheel.
- Themes: the focus ring moved from the arrows to the column (flat
  `FIELD_RING` / `DARK_ACC`, flora `LIGHT_ACC` / `DARK_GLOW`; inset shadow
  ring, radius 3). **Shared theme files touched in place** (minimal): inside
  the existing `// ==== time_picker ====` sections of `themes/flat.rs` and
  `themes/flora.rs` - the spinner part gained its ring, the arrow part lost
  its; nothing was reordered or appended elsewhere.
- The AM/PM toggle stays a button of its own (a Tab stop).

### 2. Combobox (APG)
- **Typing clears the active option.** Typing and Backspace set the new
  `ComboBoxStateWrapper::active_option_cleared`. The active option lives in
  the POPUP window's dom (a marker class, S2); the field lives in the PARENT
  window and cannot address the popup's nodes - the shared state is the one
  channel. The list's key handler reads the flag and acts as with no option
  active: Enter keeps the typed text and just closes the list, Down / Up
  start at the first / last option. It clears the flag when it makes an
  option active again.
- **Losing focus closes the list.** The field registers `FocusLost`
  (`on_combobox_blur`): an open list closes (`open` false, popup closed,
  field says Collapsed); the text stays. Tab also closes it in the field's
  key handler (the field may be the window's only stop, and then no blur
  follows), without consuming the key. A blur cannot swallow a pick: options
  are never focused and the list popup is never the key window (S2). The
  module doc's old "no blur dismissal (it races the option click)" is
  rewritten accordingly.
- One `show_list(info, field, popup, open)` for every field-side open /
  close (click, Down / Up, Tab, blur) - the toggle and the keyboard-open
  path used to repeat its two lines.

### 3. Twins `window_is_dark` / `renders_dark`
Already removed in the base by `57cdc850e` (pagination), `ccb4c8530`
(segmented), `3b62aeff7` (stepper) and `11dd02237` (date_picker, "the last
twin"): those widgets now restyle with their built styles (dark twins that
the cascade picks), and the one mode decision left is
`CallbackInfo::get_resolved_mode` (combobox, text_input). `grep` finds no
`window_is_dark` / `renders_dark` anywhere. No change made.

### 4. `get_property_slow`
The nine tier blocks (`::placeholder`, `:focus`, `:seat-focus`, `:active`,
`:dragging`, `:drag-over`, `:hover`, `:backdrop`, Normal) are one ordered
table `(raised flag, state)` and one loop: PRIORITY 1 inline, 2 stylesheet,
(2b global `*`, Normal only), 3 cascaded. Order and every lookup unchanged.
`:backdrop` keeps its own inline lookup (see open question 1). The comments
of the old blocks (placeholder pseudo-element, seat focus, backdrop window
state, global tier) moved into the one header comment.

Guards (existing tests that cover it): core `prop_cache_test`
`get_property_finds_an_inline_normal_property`,
`get_property_ignores_pseudo_state_props_unless_the_state_is_active`,
`get_property_user_override_beats_inline_and_stylesheet`,
`get_property_falls_back_through_stylesheet_global_cascaded_then_ua`,
`get_property_on_an_out_of_range_node_id_falls_through_to_ua_css`,
`get_property_with_context_matches_pseudo_state_conditions`; core
`custom_properties::a_hover_definition_recolours_a_resting_consumer_while_hovered`;
layout `backdrop_follows_window_activation` (4),
`a_theme_chain_ranks_its_blocks`, `inline_media_follows_source_order`,
`a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules`, the
placeholder paints (`textinput_first_draw_and_focus`,
`textinput_seed_style`, display_list's placeholder resolve), and every
widget `:hover` / `:focus` / `:active` theme test (`theme_checks::resolve`
goes through the cascade).

## 3. Commits

| Commit | Kind | What | Expected RED before its fix |
|---|---|---|---|
| 8577516c1 | test | time picker spin button (9 new, 5 restated) | columns have no key handler ("every column must carry the spin button's key handler"), no role (None), no tab index; walk hits the arrows (2, 6, 11, 15) |
| e1756d917 | fix | item 1 | - |
| 82f670d90 | chore | progress | - |
| 90d420671 | refactor | item 4 | none (no behaviour change) |
| ee08e38e7 | chore | progress | - |
| 83b01fc61 | test | combobox (6 new, 1 restated) | Enter picks the stale option (text "b", selected 1); Down after typing lands on option 2 (expected 0); no FocusLost handler (panic); Tab leaves `open` true; field wires 3 handlers (expected 4) |
| bc5cc8f9b | fix | items 2a + 2b | - |

Plus this report's commit.

### Tests that pinned the old behaviour, restated (all in 8577516c1 / 83b01fc61)
- `time_picker::autotest_generated::dom_registers_every_handler_on_mouse_up_and_makes_the_cell_focusable`
  -> `dom_registers_clicks_on_the_arrows_and_the_toggle_and_keys_on_the_columns`
  (its assert "a clickable time-picker cell is not keyboard-focusable": now only
  the two columns and AM/PM are Tab stops; the columns also take the keys).
- `time_picker::autotest_generated::only_retext` (helper): counts retexts and
  lets the column's announced value through (it asserted "no change beyond the
  retext"); same for the `changes.len() == 1` in
  `the_change_callback_sees_the_state_after_the_edit_and_its_verdict_is_forwarded`.
  Green before and after.
- `dom_leaves_the_displays_and_the_separator_inert`: a column carries the wheel
  AND the key handler (was: exactly the wheel).
- `dom_shares_one_state_refany_across_every_handler`: 9 payloads (was 7).
- `build_spinner_makes_both_arrows_focusable_click_targets`
  -> `build_spinner_makes_both_arrows_click_targets_but_no_tab_stops`.
- `theme_tests::every_arrow_and_the_toggle_show_a_focus_ring_in_every_theme_and_mode`
  -> `every_column_and_the_toggle_show_a_focus_ring_in_every_theme_and_mode`
  (3 stops; the column's ring colours in flat and flora).
- In the fix commit: `dom_wires_each_of_the_five_handlers_exactly_once`
  -> `dom_wires_each_of_the_seven_handlers_exactly_once` (+ the two key
  handlers); `build_spinner` takes the key handler (six call sites).
- `combobox::autotest_generated::dom_structure_classes_and_callbacks`: the field
  wires four handlers, the fourth `FocusLost` (`on_combobox_blur`).

### New tests
Time picker (`time_picker::autotest_generated`):
`tab_visits_the_hour_the_minute_and_am_pm_but_never_an_arrow`,
`an_arrow_stays_clickable_and_its_click_focuses_its_column`,
`a_column_declares_a_spin_button_named_for_its_unit_with_its_value`,
`up_and_down_move_the_focused_column_by_one`,
`page_up_and_page_down_take_the_large_step_and_clamp`,
`home_and_end_jump_to_the_ends_of_the_columns_band`,
`a_key_on_a_column_notifies_the_host_like_an_arrow_click`,
`every_change_of_a_column_announces_its_new_value`,
`a_modified_or_unused_key_on_a_column_is_not_consumed`.

Combobox (`combobox::autotest_generated`):
`typing_while_an_option_is_active_clears_it_so_enter_keeps_the_typed_text`,
`after_typing_the_next_arrow_starts_over_from_no_active_option`,
`backspace_while_an_option_is_active_clears_it_too`,
`the_list_closes_when_the_field_loses_focus`,
`losing_focus_with_the_list_closed_changes_nothing`,
`tab_in_the_field_closes_its_list_and_still_moves_focus`.

Test helpers: `roving::test_support::announced_values` (the stepper's local
copy now reads through it); combobox `mark_active` (the two S2 tests' inline
marker setup, now one helper); the combobox test literals of
`ComboBoxStateWrapper` take the remaining fields from `Default`.

For the combined RED pass (`git apply -R` per fix): e1756d917, bc5cc8f9b.

## 4. api.json

One change:
- `ComboBoxStateWrapper`: new field `active_option_cleared` of type `bool`,
  APPENDED after `on_text_input` (last field). Doc: "The user typed into the
  field (or deleted) since the list last made an option ACTIVE: that option
  no longer counts (WAI-ARIA combobox - typing clears the active option). The
  list's next arrow starts over from no active option, and Enter keeps the
  typed text instead of picking it. The list clears the flag when it makes an
  option active." `Default` sets it `false`. (Tail padding only; no interior
  padding.)

Nothing else: the time picker's new items (`spin_on_key`, `on_hour_key`,
`on_minute_key`, the page-step constants) are private; `TimePickerSkin` is
`pub(crate)`; `get_property_slow` is `pub(crate)`.

## 5. Least sure to compile

1. `time_picker.rs` `build_spinner_skinned`: `column_name.get_mut(..1)` then
   `first.make_ascii_uppercase()` (`str::get_mut` through `String`'s
   `DerefMut`); the `#[allow(clippy::too_many_arguments)]` on an 8-argument fn.
2. `time_picker.rs` `spin_on_key`: `if data.downcast_ref::<..>().is_none()`
   with `mut data`, then `data` moved into `adjust_spinner_at`; the match
   arms mixing the literal `1` and `page: i64`.
3. `time_picker.rs` tests: `matches!(idx, N_HOUR_SPINNER | N_MINUTE_SPINNER | N_AMPM)`
   (consts as patterns); `stop.then_some(TabIndex::Auto)` compared with
   `nd.flags.get_tab_index()`; `styled.node_hierarchy.as_ref()[arrow].parent_id()`;
   integer-literal tuples inferred as `(u32, u32)` in the key tables; the
   theme test's `let column = |dom: &Dom| -> Dom { tc::find(..).expect(..).clone() }`.
4. `prop_cache.rs` fold: `.filter(|&(prop, conds)| ..)` over
   `iter_inline_properties()` then `.last().map(|(prop, _)| prop)`, and the
   `if` whose two arms are that and `winning_inline_in(..)` (both
   `Option<&'a CssProperty>`); `for (raised, state) in tiers` over an array.
5. `combobox.rs` `on_combobox_list_key`: `data` became `mut data` for
   `downcast_ref(..).is_some_and(|combo| combo.active_option_cleared)` and the
   later `downcast_mut`; `data` is still moved into `pick_option` on Enter.
6. `combobox.rs` `on_combobox_key_down_inner`: `key?` in `if key? != VirtualKeyCode::Back`;
   `close_list(..).then_some(Update::DoNothing)`.
7. `combobox.rs` tests: `blur_field` moves the cloned `CoreCallback` into the
   `run` closure (`Callback::from_core(handler).invoke(r, ci)`);
   `..ComboBoxStateWrapper::default()` in four literals.

## 6. Test commands for the parent

```
cargo test --release -p azul-core --lib prop_cache
cargo test --release -p azul-core --test custom_properties
cargo test --release -p azul-layout --lib -- widgets::time_picker widgets::combobox \
  widgets::stepper widgets::roving widgets::themes
cargo test --release -p azul-layout --test all -- backdrop_follows_window_activation \
  a_theme_chain_ranks_its_blocks inline_media_follows_source_order \
  a_node_restyled_by_a_callback_resolves_its_hover_and_dark_rules widgets_follow_the_app_theme \
  textinput_first_draw_and_focus textinput_seed_style
cargo test --release -p azul-dll --test transient_window_layout combobox
```
Then the usual full suites (core, css, layout --lib, layout --test all, dll
--lib, dll transient_window_layout). After the api.json autofix for
`ComboBoxStateWrapper.active_option_cleared`, the bindings / size checks.

## 7. Behaviour changes to know

- Time picker: Tab visits hour, minute, AM/PM (was: the four arrows, then
  AM/PM). Up / Down / PageUp / PageDown / Home / End on a focused column
  change it; a click on an arrow focuses the column; the focus ring is on the
  column. Screen readers hear "Hour, spin button, 9".
- Combobox: typing drops the active option (Enter then keeps the text); Tab
  or a blur closes the list.

## 8. What is left / open questions

1. **`:backdrop` inline tier** reads the DECLARED static inline view with no
   theme rank (`iter_inline_properties`, last match), while every other state
   reads the resolved (`var()`) and theme-ranked view (`inline_properties` +
   `winning_inline_in`). It looks like a stale copy (its comment says "last
   match wins as above"). Kept as is in the fold (no behaviour change);
   unifying it is a one-line change in the loop, but a `:backdrop` inline
   `var()` / `@theme` declaration would then start to resolve. Decide.
2. **Combobox stale highlight**: after typing, the popup still PAINTS (and
   announces `Selected` on) the old active option until its next key or its
   close - the field cannot restyle the popup window's nodes. A real fix
   needs the shell: forward "typing happened" to the list popup (e.g. a
   `parent_key_route` that also forwards edit keys to a list popup as a
   notification), or re-push the popup content. Not attempted (dll).
3. **PageUp / PageDown steps** on the time picker: 2 hours / 15 minutes
   chosen (react-aria's). Say if other steps are wanted.
4. The AM/PM toggle is still a push button; APG would make it a third spin
   button (Up / Down toggle). Not asked for.
5. No dll end-to-end test for the typing / blur behaviour: the headless
   harness in `dll/tests/transient_window_layout.rs` has no text-injection
   helper; the unit tests drive the real handlers against the parent and the
   extracted popup doms, as S2's did.
6. Stale doc, not touched: `ComboBoxSkin::option` says "a Tab stop too, so
   it owes the focus ring" (options are no Tab stops since S2).
