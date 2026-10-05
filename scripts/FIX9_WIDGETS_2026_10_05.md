# FIX9-WIDGETS report (wave 9, PKG 4 WIDGETS) - 2026-10-05

Branch `wt/fix9-widgets`, based on b454da215 (the SMALL_FIXES commit). Not compiled (house rule): every touched
file passes `rustfmt --edition 2021 --check` as a parse check. 17 of 18 items done, 4.17 skipped (not small),
the 4 PKG 4 suite failures fixed (3 code, 1 test).

## Commands for the parent

```
cargo test -p azul-layout --features e2e-server --lib widgets::
cargo test -p azul-layout --features e2e-server --lib dom_lint
cargo test -p azul-layout --test all a_text_field_takes_the_font_size_its_app_gives_it
cargo test -p azul-layout --test all a_slide_rails_thumbnails_line_up_with_and_without_a_badge
```
No new layout/tests/*.rs file; all.rs untouched (the two RED files above are registered already).

## api.json (parent, through azul-doc autofix)

- `ReferencePickerEventKind` gains the variant `Clear` (appended LAST, after `Create`; the discriminants of
  Query / Pick / Create do not move). autofix does not refresh an existing entry, so:
  `azul-doc autofix remove ReferencePickerEventKind` then `azul-doc autofix add ReferencePickerEventKind`
  (enum_fields becomes Query, Pick, Create, Clear).
- Nothing else public changed. New helpers are `pub(crate)`: `button::styled_button`,
  `money_input::group_digits`, `data_table::{fit_width, CELL_PADDING_X, DEFAULT_FONT_PX}`,
  `themes::decl::classes`, `text_input::paint_invalid_ring` (was private). New `pub const`
  `reference_picker::REFERENCE_PICKER_CLEAR_CLASS` (a class name, like REFERENCE_PICKER_CLASS; not an api.json item).

## Items

4.1 TextInput takes its app's font size - 4abc009bc (RED existed: 4afd055db).
  Root cause: `font-size: 11px` was pinned on the value `<p>` (TEXT_INPUT_LABEL_PROPS, 3 platform variants), so
  a size on the field never reached the text. Moved onto TEXT_INPUT_CONTAINER_PROPS for Windows and macOS/mobile
  (Linux had it already); the label lines are gone. color_input / flora number_input build on the container
  statics and keep 11 px. Verify: both tests of a_text_field_takes_the_font_size_its_app_gives_it.rs; AzNotes' title.

4.2 Slide-rail thumbnails line up - e71e2c9fb (RED existed: 4809ad3ed).
  Root cause: the column-mode number column was sized by its content (a badge or two digits widened it). In
  column layout it is now a fixed 24 px column (width + min-width). Verify the RED; look at AzShow's rail.

4.3 TokenInput refused token rings the entry - RED ae080e7fb, GREEN 5c990e71f.
  Uses the text field's own ring painter (`text_input::paint_invalid_ring`, now pub(crate)): ringed on Refuse,
  removed (`initial` overrides) on an accepted commit and on any plain edit of the entry. Note: a plain edit
  writes 4 `initial` overrides per keystroke (cheap; the ring must go even after the app's rebuild, because
  overrides migrate across rebuilds while the widget's own state does not). Tests:
  a_refused_token_rings_the_entry_as_invalid, an_accepted_token_takes_a_refusals_ring_away.

4.4 IconGrid type-ahead - RED bc1697854, GREEN 4bb2e7aae.
  A letter / digit (no primary modifier) selects + focuses the next item whose label starts with it (case
  folded, after the focus, around the end, row revealed); no match = not the grid's key. Labels come from the
  data callback item by item (the grid holds no items) - worst case one call per item for a miss.
  Test: typing_a_letter_moves_the_focus_to_the_next_item_named_with_it. Twin note: terminal_view.rs `us_char`
  maps the same VirtualKeyCode ranges (letters 10..=35, digits); a shared `key_char` would serve both
  (round 2: terminal_view.rs is not PKG 4's).

4.5 DEDUP ribbon `styled_button` / toolbar `tool()` - f25d09b43.
  One `pub(crate) fn styled_button(.., theme: OptionUiTheme) -> Button` in button.rs; the ribbon's 3 calls (the
  brief said 4; there are 3) pass `Some(theme)` + `.dom()`, toolbar's tool() uses it and sets its toggle.
  Ribbon doc links to [`Button`] became path links. Existing ribbon / toolbar tests cover it.

4.6 MoneyInput digits right-aligned - RED 9ddfb18ec, GREEN 4ee11ed29.
  `build` lays `text-align: right` over the resolved container style unless the caller's own container style
  declares an alignment. Verify LIVE: the caret and the horizontal scroll of a long amount (value line has
  overflow-x: auto). Test: a_money_input_aligns_its_digits_to_the_right.

4.7 DateRangePicker presets roving + year jump - RED bad3f6692, GREEN 381a9c5fb.
  Presets: one Tab stop (the preset whose span is the range picked, else the first); Up / Down / Home / End move
  it (roving::step_target / move_stop, ends hold). Days: Shift+Page Up / Down turn 12 months (the one chord the
  day keys claim). Tests: the_presets_are_one_tab_stop_and_arrows_walk_them, shift_page_down_turns_a_year.

4.8 ReferencePicker clear button - RED 09ee9a125, GREEN 2d58a6847. API: the `Clear` variant above.
  While `selected` is set, an x (icon "close", PushButton "Clear", not a Tab stop) sits in the combobox field
  between the text and the arrow (the field's first child stays the text, which the combobox handlers rely
  on). Its click stops propagation (the field toggles its list on click), drops a pending debounced query and
  reports Clear (text "", id 0). Verify live: the x's position in flat / flora, light / dark.

4.9 CellGrid Ctrl+C / Ctrl+X - RED 0c8d84eec, GREEN 84f2a1ef3.
  Root cause (engine, by design): core events dispatch Copy / Cut only to a contenteditable focus or a text
  selection. The grid's key handler now claims primary+C / X when not editing (prevent_default + the existing
  copy_selection), as DataTable does. Note: callbacks.rs `set_copy_content`'s doc says the content applies only
  "if preventDefault() was not called" - the shell applies it regardless (event.rs SetCopyContent); the doc is
  stale (round 2, callbacks.rs).

4.10 DEDUP CellGrid edit caret keys -> data_table::line_edit - 1db1f4517. Same behaviour.
  Twin left: cell_grid `typed()` re-implements data_table's private `insert_at` (both in PKG 4 files; small
  follow-up: make insert_at pub(crate) and call it from typed()).

4.11 DataTable double-click on a header edge auto-fits - RED a809309aa, GREEN 0d13d1703.
  `fit_width`: widest text in view (header label with a sorted column's arrow, the column's cells in the
  geometry's rows) x font x cell_grid::SPILL_EM + 2 x CELL_PADDING_X (6, now the constant the cell base uses),
  >= MIN_COLUMN_PX; one `resized` builder for drag_end and the double-click. The test is an inline module in
  data_table.rs (data_table_tests.rs belongs to nobody in this wave; left untouched).
  Verify live: a double-click after the first click grabbed the edge leaves no drag (the event's view has
  none) - check the following mouse-up does not re-apply the drag size.

4.12 DEDUP timeline tick_label -> seek_bar::media_time - b4b7b3350. Same output for finite times.

4.13 DEDUP thousands grouping - RED a6a000f6c (RED by compile: the helper did not exist), GREEN ea917eadd.
  `money_input::group_digits(digits, Option<char>)`; data_table::grouped and chart::group_thousands call it.
  Apps switch in PKG 5 / 6.

4.14 DEDUP classes(&[&str]) - ec5bfb751. One `decl::classes` APPENDED at the end of themes/decl.rs (decl.rs is
  outside PKG 4's Files line; the item names it and no package owns it). chart.rs / timeline.rs import it.
  Third twin: gauge.rs `class_list` (round 2, not PKG 4). dialog_kit::class(name) is the one-class variant.

4.15 DEDUP private hook() builders - dbd708496. 14 setters use `Option<X>OnEvent::Some(<X>OnEvent::create(..))`.

4.16 rich_text/html.rs escape -> core encoder - RED becbfa671, GREEN 82fad2e6b.
  Text calls -> `azul_core::xml::html::encode_text`, attribute calls -> `encode_attribute`. Behaviour notes:
  `"` in text is no longer escaped (plain between tags), `'` in attributes now is, C0 controls are dropped.
  Test: a_control_character_in_a_paragraph_does_not_reach_the_html.

4.17 dialog_kit row_button -> Button::with_disabled - SKIPPED, NOT SMALL (a design decision + another file):
  - Button's disabled model is "has a reason": `with_disabled("")` ENABLES. Two of the three callers
    (standard_dialogs, settings_dialog Apply) have no reason - a reason text must be invented, or Button needs
    a reasonless disabled state (a new repr(C) field = api.json).
  - Button's disabled button KEEPS its Tab stop and answers a click / hover with its reason
    (mark_disabled adds Click / MouseEnter / MouseLeave). row_button's contract is "no click, no Tab stop", and
    wizard_layout.rs's test (`a held Next is inert`: `rv::fire(Click).is_none()`) pins it - not a PKG 4 file.
  - The box's `held` skin (opacity 50%) would stack on the Button's disabled dimming (40%): 20%.
  Decision wanted: one disabled model for dialog buttons (Tab stop yes/no, a reasonless disabled state), then
  row_button drops its fake (manual Unavailable + held skin) and wizard_layout's test changes with it.

4.18 impl_option_inner hand imports dropped - 3460fd689 (doc.rs, rich_text_editor.rs, close_guard.rs;
  page_breaks.rs left to its session).

## Suite failures (PKG 4)

- dom_lint `every_widget_dom_is_warning_free` - CODE wrong (as ruled) - 3e948cc3e. The check item's bare text
  "Send the invite" sat beside the check box div. A check item's plain runs are now each a `span` around the
  text (`with_runs_in(.., wrap_plain: true)`); child indices unchanged, a classless span reads back plain.
  Round 2: dom_lint does not model out-of-flow siblings (an absolutely positioned box does not split a line).
- button `dom_carries_the_container_style_on_the_root_and_the_label_style_on_the_child` - TEST wrong -
  17708ce84. d5cebf5a7 (the face fade, on purpose) appends `decl::state_fade`'s unconditional `animation` to
  every non-link button; the expectation now includes it.
- code_view `the_wheel_scrolls_whole_lines_and_never_past_the_last_line` - CODE wrong - 49fffa034.
  scroll_event started from `geo.top`, the BUILD's geometry (store_view refreshes only the view), so a second
  wheel turn before the app's rebuild started from the old top. It now starts from the view's top line (as the
  column does) and no longer takes the geometry (handler + test calls updated).
- date_range_picker `a_pinned_date_range_picker_keeps_its_theme_invariants` - CODE wrong - a7dc9e502.
  Today's ring (a resting inset shadow in the focus halo's slot) stacked after the face hid the focus halo on
  today's cell (the reported root/1/0/0/2/1/2 = 4 March, today and the range start); the range's wash hid
  flora's hover face the same way (flora was never reached: the loop panicked on flat). `marked(face, mark)`
  re-stacks the face's state declarations after each resting mark.
  Round 2: date_picker.rs `ringed` / `washed` stack the same marks the same way (the DatePicker's today cell
  and lit range lose focus / hover); `marked` belongs there, shared by both pickers.

## Round-2 notes (files outside PKG 4)

1. date_picker.rs: `ringed` / `washed` -> the `marked` rule (state declarations after a resting mark); then
   date_range_picker::day_face uses it (one helper).
2. dom_lint.rs: treat `position: absolute / fixed` siblings as out of flow in the block-sibling check.
3. wizard_layout.rs / standard_dialogs.rs / settings_dialog.rs: 4.17 after the disabled-model decision.
4. gauge.rs `class_list` -> `decl::classes`.
5. terminal_view.rs `us_char` and icon_grid `typed_letter`: one VirtualKeyCode -> char helper.
6. callbacks.rs `set_copy_content` doc ("if preventDefault() was not called") is stale.
7. Apps that set a TextInput container style WITHOUT a font size (and relied on the value's pinned 11 px) now
   get the inherited size - intended (4.1), but worth a look in the E2E screenshots.

## Least sure to compile

- reference_picker.rs `dom()`: `let parts: &mut [Dom] = dom.children.as_mut();` then
  `core::mem::replace(&mut field.children, DomVec::from_const_slice(&[]))` and `dom.fixup_children_estimated()`.
- money_input.rs `build`: `self.text_input.container_style.as_ref().is_some_and(|style| style.as_slice() ..)`.
- date_range_picker.rs `on_range_day_key`: `matches!(ks.current_virtual_keycode.into_option(), Some(K::PageUp |
  K::PageDown))` and the i32 `months` in `turn(..)`.
- button.rs test: `let [fade, _pressed] = decl::state_fade(..);` (array destructuring by value).
- icon_grid.rs `typed_letter`: `key as u32` (as terminal_view does).
- ribbon.rs: the import is now `button::{styled_button, OptionButtonOnClick}` (Button dropped; doc links are
  path links).
