# W1 - every HTML `<input>` type as an azul widget (2026-09-29)

Branch `wt/w1-input-types`, cut from `fix/input-bugs-2026-09-19` at `9c6065b05` (before W2 landed
on it). NOTHING was compiled or run: every test below is written to the surrounding harnesses
and every API call was checked by reading its definition. See "Least sure to compile" first.

## What was built

| HTML | Widget / constructor | How |
|---|---|---|
| `password` | `TextInput::create_password()` | a MODE of TextInput (`TextInputKind::Password`). The engine's buffer holds one U+2022 per grapheme; `TextInputState.text` keeps the real value. An insertion is spliced into the real value at the masked caret/selection (mapped onto grapheme boundaries), the hook sees the real text, `max_len` and a vetoing hook still veto, and the pending changeset is rewritten (`set_text_changeset`) to the bullets the new value needs. A post-edit notification (Backspace/Delete/cut) removes the same graphemes (`masked_deletion`). Copy and Cut are vetoed (`FocusEventFilter::Copy/Cut` -> `default_on_clipboard_veto`). a11y: `AccessibilityState::Protected`, no accessibility value; the a11y manager maps a contenteditable host with `Protected` or `type=password` to accesskit `Role::PasswordInput` (and search/email/tel/url/text to their input roles). |
| `search` | `TextInput::create_search()` | the field + a clear button (x) in one row (the button is a SIBLING of the editable host, not a Tab stop, a11y push button "Clear search"). Hidden while empty; the handlers flip `display` only on the empty<->non-empty transition. Click or Escape (non-empty only; Escape's focus-dropping default is then vetoed) clears: hook first (may veto), then mirror, engine line, looks. |
| `email`, `tel`, `url` | `TextInput::create_email()` / `create_tel()` / `create_url()` | `type=` attribute on the host (the soft-keyboard hint `crate::form::input_purpose` already reads; also the a11y role). email/url syntax checks (`crate::form::is_valid_email` = HTML's own "valid e-mail address" rule; `is_valid_absolute_url`). |
| `pattern` attr | `.with_pattern(p)` on any TextInput | `crate::form::pattern_matches` (regex-lite, whole value, empty exempt, uncompilable ignored). |
| validity | `TextInputState.validity: ValidityState` | computed at every build and after every edit; readable in every hook. New `ValidityReason::TypeMismatch = 6` (appended). The look follows CSS `:user-invalid`: an edit that leaves the value invalid overrides the 4 border colours with the THEME's invalid ring (flat danger red, flora brick red, each with a lifted dark variant chosen when written); an edit that makes it valid removes the override (`CssProperty::initial`), so hover/focus ring and dark twins come back. A failed form submit paints it too (`mark_user_invalid`). An app-supplied invalid value is invalid in the state/FormData at once but not painted until the user edits it (that is why browsers added `:user-invalid`; also the only design without the override latch). |
| `month` | `DatePicker::create_month(year, month)` | `DatePickerMode::Month`: field `YYYY-MM`; popup = `‹ YYYY ›` over a 4x3 month grid in the day cells' face (one Tab stop, arrow keys, PageUp/PageDown = year). The year arrows re-title the header live (the month grid is the same every year). |
| `week` | `DatePicker::create_week(iso_year, week)` | `DatePickerMode::Week`: ISO 8601 (`iso_week_of`, `iso_week_monday`, `iso_weeks_in_year`, `DatePickerState::iso_week`); the state holds the week's Monday; field `YYYY-Www`; the same day grid built Monday-first with the whole selected row lit; a day click picks its week. |
| `datetime-local` | `DateTimeLocalPicker::create(y, mo, d, h, mi)` | new `datetime_local.rs`: a DatePicker and a TimePicker composed in one themed row; each part reports into ONE combined `DateTimeLocalPickerState`; value `YYYY-MM-DDTHH:MM` (24 h). |
| `submit` | `Button::create_submit(label)` | `ButtonFormAction::Submit`, `type=submit`, click submits the enclosing `Form`. |
| `reset` | `Button::create_reset(label)` | `type=reset` (the engine's Enter-on-reset-control default finds it too), click resets the enclosing `Form`. |
| `image` | `Button::create_image(image, alt)` | a submit button showing only the image; `Button.alt` is its accessible name and `alt=` attribute; `type=image`. |
| `hidden` | `HiddenInput::create(name, value)` | `display:none` div (no tab stop, `hidden`) with `name=`, `value=`, `type=hidden`; submitted, never reset. |
| `<optgroup>` | `DropDown::with_optgroup(label, options)` | `DropDown.groups`; options go to `choices` (a heading is never a choice, so every reported index still counts options only); the menu shows each heading as a `MenuItemState::Disabled` item WITHOUT a callback before its (indented) options - nothing can pick it and native menu keyboard navigation passes over it. |
| `<form>` | `Form::create(children)` + `with_on_submit` / `with_on_reset` | see below. |

## Form design (the minimal one)

azul already had the ENGINE half: `NodeType::Form`, `Submit`/`Reset`/`Invalid` events, the Enter-to-
submit and reset-button default actions, and attribute-based constraint validation
(`layout/src/form.rs`). It lacked the WIDGET half - the values live in widget states and nothing
collected them. `layout/src/widgets/form.rs` is that half:

- A control participates by carrying `name` on the node holding its state as dataset
  (`TextInput/DatePicker/DateTimeLocalPicker::with_name`, `HiddenInput`); if the named node has no
  known state, its FIRST CHILD's is used (a search row, or any root W2 grafted `name` onto); any
  other node with `name` + `value` attributes contributes its value. Unnamed controls are left out.
- `Form::dom()` renders a `NodeType::Form` node (so the engine's defaults find it) whose dataset is a
  `FormStateWrapper`; at build it records each named control's value as the INITIAL `FormData`
  (HTML's default value = the value the page was built with; widgets are built from app state). It
  listens to the engine's `Submit`/`Reset` events on the form node.
- `submit_form` (submit/image button click, Enter in a TextInput = HTML implicit submission, engine
  Submit event): the CURRENT values in document order (`FormData`: multimap `get`/`get_all`/`has`,
  plus `invalid` names and `is_valid()`), `:user-invalid` painted on the invalid fields, then the
  app's `on_submit(FormData)`. HTML would refuse to submit an invalid form; azul has no browser
  bubble to show instead, so the app decides.
- `reset_form` (reset button, engine Reset event): each TextInput back to its initial value
  (mirror, line, looks), then `on_reset(initial FormData)` so the app restores the rest (pickers are
  rebuilt from app state). No forced RefreshDom (it would rebuild from the app's CURRENT values).

## Commits (in order)

- `4a8713eb7` progress file
- `29a66b586` RED password / `ad87f6c10` password
- `db3a54852` RED search / `e7ca659e1` search
- `0f9eed9b4` RED email/tel/url/pattern / `885972248` validity + :user-invalid look
- `ebbdb7370` RED month/week/datetime-local / `50342904c` pickers
- `901832389` RED form/submit/reset/image / `32504b223` form
- `e689fa68f` RED hidden / `4b7496f5a` hidden
- `9f9ac0d1f` RED optgroup / `17b706795` optgroup
- `00dec6398` RED name on a wrapper root / `293037f2c` fix
- `8cb23d046` refactor: one RefAny borrow per statement (compile-by-reading pass)
- `610e9a46a` RED lint manifest covers the new modes (+ wheel pin names `datetime_local`)
- `08bff3ce0` RED lint the flora search field / `8bd615659` flora badge dark fix
- progress checkpoints `919c61e2a 8e49b0d46 d78f93555 b96ea2381 0535b63af c91cbe228 9feba626d`, and this report.

Tests: unit tests in `text_input.rs` (`autotest_generated::{password, search, validation}`),
`date_picker.rs` (`autotest_generated::month_and_week`), `datetime_local.rs`, `widgets/form.rs`
(incl. `hidden`, `wrapper_roots`), `drop_down.rs` (`optgroups`), `managers/a11y.rs` (typed roles),
`layout/src/form.rs` (email/url/typeMismatch), `core/src/form.rs` (TypeMismatch discriminant),
and the widget lint manifest in `widgets/mod.rs` (10 new entries). No `layout/tests/*.rs` file was
needed.

## Public API for api.json (autofix)

All `#[repr(C)]`, constructors `create*`, callbacks via `impl_widget_callback!` +
`impl_managed_callback!`, docs ASCII.

core `form`:
- `ValidityReason::TypeMismatch` (= 6, appended).

`widgets::text_input`:
- enum `TextInputKind { Text, Password, Search, Email, Tel, Url }` (+ Rust-only `html_type() -> &'static str`).
- `TextInputState` new fields, APPENDED after `cursor_pos`: `pattern: OptionString`, `validity: ValidityState`, `kind: TextInputKind`.
- `TextInput.name: OptionString` (between `accessibility_name` and `theme`).
- `TextInput::create_with_kind(kind: TextInputKind) -> TextInput`, `create_password()`, `create_search()`, `create_email()`, `create_tel()`, `create_url()`, `set_kind(&mut self, kind: TextInputKind)`, `with_kind(self, kind) -> TextInput`, `set_pattern(&mut self, pattern: String)`, `with_pattern(self, pattern: String) -> TextInput`, `set_name(&mut self, name: String)`, `with_name(self, name: String) -> TextInput`.
- `TextInputState::compute_validity(&self) -> ValidityState`, `TextInputState::is_constrained(&self) -> bool`.
- `extern "C"` handlers (Rust-internal like the other `default_on_*`): `default_on_clipboard_veto`, `default_on_search_clear_click`.
- Rust-only: `display_text(&TextInputState) -> String`, `mark_user_invalid(&mut CallbackInfo, DomNodeId, &TextInputState)`, consts `PASSWORD_MASK_CHAR`, `THEME_FLAT_CLASS`, `THEME_FLORA_CLASS`, `SEARCH_FIELD_CLASS`, `SEARCH_CLEAR_CLASS`.

`widgets::date_picker`:
- enum `DatePickerMode { Date, Month, Week }` (+ Rust-only `html_type`).
- `DatePicker` new fields APPENDED: `name: OptionString`, `mode: DatePickerMode`.
- `DatePicker::create_month(year: u32, month: u32)`, `DatePicker::create_week(year: u32, week: u32)`, `set_mode/with_mode(DatePickerMode)`, `set_name/with_name(String)`.
- Rust-only: `DatePickerState::iso_week(&self) -> (u32, u32)` (a tuple; for FFI add two getters if wanted).

`widgets::datetime_local` (new):
- `DateTimeLocalPickerState { date: DatePickerState, time: TimePickerState }` (+ Rust-only `to_html_value() -> String`; return `String` if exported).
- `DateTimeLocalPickerStateWrapper { inner: DateTimeLocalPickerState, on_change: OptionDateTimeLocalPickerOnChange }`.
- `DateTimeLocalPicker { state, container_style: OptionCssPropertyWithConditionsVec, accessibility_name: OptionString, name: OptionString, theme: OptionUiTheme }`: `create(year, month, day, hour, minute: u32)`, `with_24h(bool)`, `set_on_change/with_on_change`, `with_accessibility_name`, `set_name/with_name(String)`, `set_theme/with_theme`, `swap_with_default`, `dom`.
- callback family `DateTimeLocalPickerOnChangeCallbackType = extern "C" fn(RefAny, CallbackInfo, DateTimeLocalPickerState) -> Update`, `DateTimeLocalPickerOnChange`, `OptionDateTimeLocalPickerOnChange`, `DateTimeLocalPickerOnChangeCallback`; managed-FFI symbols `AzApp_setDateTimeLocalPickerOnChangeCallbackInvoker`, `AzDateTimeLocalPickerOnChangeCallback_createFromHostHandle(Byref)`.
- const `DATETIME_LOCAL_CLASS`.

`widgets::form` (new):
- `FormEntry { name: String, value: String }`, `FormEntryVec` (+ `FormEntryVecDestructor`, `FormEntryVecDestructorType`, `FormEntryVecSlice`), `OptionFormEntry`.
- `FormData { entries: FormEntryVec, invalid: StringVec }`: `get(&self, name: String) -> OptionString`, `get_all(&self, name: String) -> StringVec`, `has(&self, name: String) -> bool`, `is_valid(&self) -> bool`.
- `FormStateWrapper { initial: FormData, on_submit: OptionFormOnSubmit, on_reset: OptionFormOnReset }`.
- `Form { children: DomVec, state: FormStateWrapper, container_style: OptionCssPropertyWithConditionsVec, accessibility_name: OptionString, theme: OptionUiTheme }`: `create(children: DomVec)`, `set_children/with_children(DomVec)`, `with_child(Dom)`, `set_on_submit/with_on_submit`, `set_on_reset/with_on_reset`, `set_container_style/with_container_style`, `with_accessibility_name`, `set_theme/with_theme`, `swap_with_default`, `dom`.
- callback families `FormOnSubmitCallbackType = extern "C" fn(RefAny, CallbackInfo, FormData) -> Update` (`FormOnSubmit`, `OptionFormOnSubmit`, `FormOnSubmitCallback`) and the same for `FormOnReset*`; managed symbols `AzApp_setFormOnSubmitCallbackInvoker`, `AzFormOnSubmitCallback_createFromHostHandle(Byref)`, `AzApp_setFormOnResetCallbackInvoker`, `AzFormOnResetCallback_createFromHostHandle(Byref)`.
- `HiddenInput { name: String, value: String, theme: OptionUiTheme }`: `create(name, value)`, `set_value/with_value`, `set_theme/with_theme`, `dom`.
- Rust-only: `submit_form(&mut CallbackInfo, DomNodeId) -> Update`, `reset_form(...)` (for FFI they would become `CallbackInfo` methods), `extern "C"` handlers `default_on_form_submit_event`, `default_on_form_reset_event`, `default_on_form_button_click`, const `FORM_CLASS`.

`widgets::button`:
- enum `ButtonFormAction { None, Submit, Reset }`; `Button.alt: String` (after `trailing_icon`), `Button.form_action: ButtonFormAction` (last).
- `Button::create_submit(label: String)`, `create_reset(label: String)`, `create_image(image: ImageRef, alt: String)`, `set_form_action/with_form_action(ButtonFormAction)`.

`widgets::drop_down`:
- `DropDownOptGroup { label: String, first_choice: usize, len: usize }`, `DropDownOptGroupVec` (+ Destructor, DestructorType, Slice), `OptionDropDownOptGroup`; `DropDown.groups: DropDownOptGroupVec` (after `choices`).
- `DropDown::add_optgroup(&mut self, label: String, options: StringVec)`, `with_optgroup(self, label, options) -> DropDown`.

Rust-only elsewhere: `azul_layout::form::{is_valid_email, is_valid_absolute_url, value_matches_type}`,
`managers::a11y::typed_text_input_role`, themes `flat/flora::{search_clear_button, search_field,
text_input_invalid_ring, INVALID_RING, DARK_INVALID_RING, datetime_local, form}`.

## Type -> constructor mapping (for the W2 glue in `form_controls.rs`)

| `type` / element | build | attributes to feed |
|---|---|---|
| `password` | `TextInput::create_password()` | value, placeholder, maxlength, pattern -> `.with_pattern` |
| `search` | `TextInput::create_search()` | same; NOTE the root is a ROW: the field (dataset, contenteditable) is `root.children[0]`, the clear button `root.children[1]`. Grafting `name` onto the row is fine (the form reads the first child). |
| `email` / `tel` / `url` | `TextInput::create_email()` / `create_tel()` / `create_url()` | + `.with_pattern` |
| any text-like with `name` | `.with_name(name)` (or keep grafting `name=` - both work) | |
| `month` | `DatePicker::create_month(y, m)` from `YYYY-MM` | `.with_name` |
| `week` | `DatePicker::create_week(y, w)` from `YYYY-Www` (ISO-exact now; drop the Jan 1 + 7(w-1) approximation) | `.with_name` |
| `datetime-local` | `DateTimeLocalPicker::create(y, mo, d, h, mi)` from `YYYY-MM-DDTHH:MM` | `.with_name`; recorder needs a `DateTimeLocalPickerOnChangeCallbackType` fn |
| `submit` | `Button::create_submit(value or "Submit")` | |
| `reset` | `Button::create_reset(value or "Reset")` | |
| `image` | `Button::create_image(image_from_src, alt)`; without a loaded image `Button::create_submit(alt)` | |
| `hidden` | `HiddenInput::create(name, value)` (or keep W2's div: it already carries name+value, which the form reads) | |
| `<optgroup label>` in `<select>` | `DropDown::with_optgroup(label, options)` instead of flattening | |
| `<form>` | `Form::create(children)` - suggested new W2 row, so raw XML forms get FormData, implicit submission and reset | |

## Least sure to compile (check these first)

1. `text_input.rs` `default_on_virtual_key_down_inner`: `drop(text_input)` (the `RefMut` guard) inside a nested `if` before `crate::widgets::form::submit_enclosing_form(&mut info, container)` - a conditional move; nothing uses the guard afterwards.
2. `text_input.rs`: `masked_notification(&mut text_input, ..)`, `masked_insertion(&mut text_input, ..)`, `clear_field(&mut text_input, ..)` rely on deref coercion `&mut RefMut<'_, TextInputStateWrapper>` -> `&mut TextInputStateWrapper`; `form.rs` reset does the same with `&mut w`.
3. `text_input.rs` `paint_invalid_ring`: `crate::widgets::date_picker::window_is_dark(info)` with `info: &mut CallbackInfo` (made `pub(crate)` in date_picker.rs), `CssProperty::initial(CssPropertyType::BorderTopColor ..)` variant names, `info.override_node_css_properties(dom, node_id, Vec<CssProperty>.into())`.
4. `form.rs`: `impl_managed_callback!` with `extra_args: [ form_data: FormData ]` (named `form_data` because the macro already uses `data` for the RefAny), the `impl_vec!` family for `FormEntry` (explicit macro imports incl. `impl_option_inner`), `Form` deriving `PartialEq` over `DomVec`.
5. `date_picker.rs`: `WEEKDAY_NAMES.iter().cycle().skip(offset).take(7)` and `MONTH_ABBREVIATIONS.chunks(3)` over const-array temporaries; `build_month_cell(.., abbreviation: &str, ..)` fed `&&str`; `close_calendar_showing(&mut info, ..)` taking `&mut CallbackInfo` (was inline in `on_day_click`).
6. `drop_down.rs` `build_menu_items`: the non-capturing `heading` closure passed by value to two `map`s (Copy); `azul_css::impl_vec_eq!`/`impl_vec_mut!` paths.
7. flat/flora: `CssProperty::ColumnGap(LayoutColumnGapValue::Exact(LayoutColumnGap { inner: PixelValue::const_px(8) }))` and the `RowGap` twin; `LayoutAlignSelf::Start` via `CssProperty::align_self`.
8. `layout/src/form.rs`: `str::find(['/', '?', '#', '\\'])` / `trim_start_matches(['/', '\\'])` (char-array patterns).
9. Test helpers: every `downcast_ref` guard was moved out of block tail expressions (edition 2021 temporary lifetimes) - check any I missed if E0597 shows up.
10. `core::iter::repeat_n` (Rust >= 1.82; already used in the crate's tests).

## What is left / known gaps

- ENGINE GAP (report to the ledger): replacing a contenteditable field's value from a callback. `ChangeNodeText` writes the DOM but the content overlay (uncommitted typing) outranks it until the DOM converges; widgets can only EMPTY the live buffer (select-all + `delete_backward`; inserting would echo an `Input` the handlers mirror twice). So a search clear always works, but a form reset of a field whose typed text the app never adopted shows the typed text until the app rebuilds with it. Fix candidates: `ChangeNodeText` (or a new `SetNodeValue` change) supersedes the overlay entry of the node's IFC root and clamps carets; or an `EventSource::Programmatic` flag the widget can skip.
- W2 interplay: W2's `FormControlMemory` restores the user's value on the next rebuild, which would undo `reset_form` for replaced controls - the integration should `forget` the form's controls in the memory on reset (the Form cannot reach `LayoutWindow.form_control_memory` from a callback cleanly). Raw XML `<form>` has no `FormStateWrapper`, so implicit submission / FormData need W2 to map `<form>` to `Form`.
- FormData covers TextInput (all kinds, NumberInput via its TextInput root), DatePicker (all modes), DateTimeLocalPicker, HiddenInput and any named node with a `value` attribute. CheckBox/RadioGroup/Slider/ColorInput/FileInput/DropDown keep no dataset on their root, so they need one (or a `value` attribute) to be collected.
- Password: undo/redo re-inserting bullets cannot be mirrored (the mirror keeps its value; GTK/Qt disable undo in password fields - azul's undo shortcut is swallowed before callbacks, so the widget cannot veto it). A combining mark fused into its base grapheme is handled; a ZWJ merging two graphemes may leave one bullet too many until the next rebuild.
- `:user-invalid` ring colour is picked light/dark when written; a theme switch while a field is invalid keeps the old variant until the next edit.
- `image` buttons do not submit click coordinates (`name.x` / `name.y`).
- Week picker: a week spanning two months shows only its days in the displayed month (the grid cannot rebuild itself - the date picker's existing TODO2).
- `required`, `min`/`max`/`step` on the date types, `<select multiple>`, disabled `<option>`s: not in scope.
- The AzWidgets demo entries (a later step) and api.json (autofix, list above).
