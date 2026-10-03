# MAILWIDGETS - the widgets of the Outlook-2010-style AzMail rewrite (2026-09-30 / 10-01)

Branch `wt/mail-widgets` from `wt/fb2-azdrive-explorer` (`82f1d74c9`). Nothing was compiled here
(house rule); every Rust file passed `rustfmt --edition 2021 --check` as a parse check. No app binary
was run. The session was cut twice by the weekly API limit and resumed from
`scripts/MAILWIDGETS.PROGRESS.md` (nothing was lost: every batch was committed before the cut).

No sibling worktree had `layout/src/widgets/shells/` or a `NavigationPane`, so the ModuleSwitcher
was built standalone (section 3); the SHELLS agent's pane can host it as its foot.

## 1. Engine gaps found by reading, fixed RED-first (`028879ee7` RED, `c06c8bdca` GREEN)

- **DatePicker had no inline mode and no "today".** The calendar only lived in a popup under a
  field, so a to-do bar's mini calendar could not be built from it. Now `DatePicker::inline`
  (`with_inline`) makes the calendar the widget's root: a `Grouping` named by the accessibility
  name, the shared state as its dataset and the mode's HTML `type` like the field would, no field,
  no popup; a pick skips the close-and-fill step (`DatePickerData::inline`) - the old path would
  have called `set_transient_window_open` on the app's container and rewritten its first text.
  `today` (`with_today(y, m, d)`, `OptionDatePickerState`) rings today's cell when its month is
  shown (`DatePickerLook::day_today`, `CellFaces::today`, the `__azul-native-date-picker-today`
  class so a repaint - `restyle_grid` - keeps the ring) and names it "..., today". Flat rings in
  the field ring / night accent, flora in the accent / glow; the established look in the accent.
  Tests `date_picker::inline_and_today_tests`.
- **ListView's lazy-load hook never fired.** `on_lazy_load_scroll` sat on the struct, but `dom()`
  never wired it, so no app could be told to load more rows. The row box now registers
  `scroll_settled_hook` - a `ScrollEnd` HOVER hook: the settled gesture, never the wheel, which the
  wheel-ownership lint keeps the page's - and hands the app its `ListViewState` with the live scroll
  offset and box size (`scroll_window_of`, zero where the engine has no scroll state yet, so the
  app still hears the gesture). `set_on_lazy_load_scroll` / `with_on_lazy_load_scroll`. Both
  helpers are shared with every virtualised list (the MessageList uses them). Tests
  `list_view::lazy_load_tests`.
- **StatusBar had no sync indicator** (Outlook's "Verbunden" / "Uebermittlungsfehler"): section 3.

## 2. One engine rule to know (not a bug): no `Scroll` hooks on widgets

`widgets::wheel_ownership` forbids a `Scroll` handler on any manifest widget (a closed control must
leave the wheel to the page). A virtualised list therefore reports its window on `ScrollEnd` -
once per settled gesture, which is also the right cadence for a rebuild - and the keyboard
(Home / End / PageUp / PageDown past the window) asks the app through `Select` events instead of
scrolling itself.

## 3. Widgets (`layout/src/widgets/`), flat + flora, light + dark, keyboard + a11y, unit tests

Every new part is its BASE (structure, the same in every theme, `*_BASE` statics in the widget's
file) then the theme's skin; looks appended at the END of `themes/flat.rs` / `themes/flora.rs`
under `// ==== <widget> ====` banners (plus a `// ==== mail widgets: shared strokes ====` block
per theme: the strip under / over a hairline and the sheet / leaf, so the five looks do not repeat
them); an unpinned widget follows the app theme (`theme_blocks::follow_app_theme`); the toolkit's
own widgets are reused for every control (TextInput, Segmented, Button, Chip, Avatar, CheckBox,
DatePicker, Stepper, InfoBar) and pinned to the host's theme. All joined the lint manifest
(`widgets/mod.rs`: label convention, dark pairs, light / dark contrast groups, wheel ownership,
chrome text). `decl::border_top` (the twin of `border_bottom`) was added for the footers and
button rows.

- **info_bar** (new, `028879ee7` / `c06c8bdca`): the notice strip - a glyph, one line of text and
  an action link (a link `Button`; `on_action` is a `ButtonOnClick`, so an app hands over the
  handler it already has); the alert palette's `AlertKind` for the colour (`AlertKind::colors` /
  `dark_colors` made `pub(crate)` for it - no second palette); an `Alert` named by its text, the
  action the only keyboard stop. `OptionInfoBar` for the reading pane.
- **statusbar** (`028879ee7` / `c06c8bdca`): `StatusBarSync { label, on_click, kind }` with
  `StatusBarSyncKind::{Connected, Syncing, Error, Offline}` (one glyph each, `kind.icon()`),
  rendered as a segment Button between the filler and the view switcher, named by its label; the
  error glyph in `StatusBarTheme::sync_error` (`StatusBarStyle::sync_icon_error_style`,
  `resolved_sync_icon_error_style`; flora: the clay stone / its glow, 4 lines added inside
  `flora::statusbar_style`). `StatusBar::sync` / `with_sync`. Tests `statusbar::sync_tests`.
- **message_list** (new, `4f643014e` / `d8525a81b`): the search row (search `TextInput` +
  scope `Segmented`), the sort header ("Arrange by:" caption, the field as a link, the direction
  toggle with an arrow), the rows box (`List`, multiselectable) with the WINDOW of rows between two
  spacers: `MessageRow` (id, from, subject, preview, date, icon, kind Message / Group, unread,
  flagged, has_attachment, selected) rendered as glyph | sender (bold when unread) / subject /
  preview | date / clip / flag `Button` ("Flag" / "Unflag"); group headers are `Grouping`s named by
  their title, not focusable. Virtualised: `first_row`, `total_rows`, `row_height`; `on_scroll`
  reports `Scroll { index, end }` from `ListView::visible_row_range` over the settled scroll box.
  Keyboard (APG listbox): one Tab stop (the selected message, else the first), Up / Down over
  messages only, Home / End / PageUp / PageDown (beyond the window: a `Select` at the absolute row
  for the app to render), plain move selects, Shift extends, Ctrl moves alone; Enter `Open`,
  Delete / Backspace `Delete`; click `Select` with `shift` / `ctrl`, double-click `Open`, the flag
  `Flag` (stops at the flag). ONE callback type `MessageListOnEventCallbackType(RefAny,
  CallbackInfo, MessageListEvent)` behind eight hooks (`on_select`, `on_open`, `on_flag`,
  `on_delete`, `on_sort`, `on_search`, `on_scope`, `on_scroll`). `MessageListSelection { rows:
  U32Vec, anchor }` + `apply(index, shift, ctrl)` is the app's selection rule (plain / Ctrl toggle /
  Shift range from the anchor). Reuses the tile's `TILE_ICON_BASE` / `TILE_COLUMN_BASE` /
  `TILE_LINE_BASE`. 10 tests.
- **reading_pane** (new): subject, sender link + date, the `InfoBar` notice (its action reports
  `on_load_images`; unset, the bar keeps its own `on_action`), "key: value" field rows (the details
  pane's `PANE_ROW_BASE` / `PANE_KEY_BASE` / `PANE_VALUE_BASE`), attachment `Chip`s
  (`on_attachment(index, name)`), the body `Dom` on paper, the people footer (`Avatar`s + a link,
  `on_link(People)`); the sender link reports `on_link(Sender)`. A `Document` named by its subject.
- **todo_bar** (new): the `DatePicker` inline with today ringed (`on_pick(DatePicked, date)` - a
  pick, and the arrows / PageUp / PageDown turning the month), the appointments (links,
  `on_appointment`) or the "No upcoming appointments." line, the task line (`TextInput`, Enter
  `TaskAdded(text)` - `task_key_adds`), the tasks (a `CheckBox` named by the title ->
  `TaskToggled(index, id)`, the title a link -> `TaskOpened`, the due date; a done row dimmed). A
  `Grouping` named by `accessibility_name` or "To-Do bar".
- **module_switcher** (new): the module `Button`s (icon + label, the look's chassis injected the
  status bar's way), the active one pushed in; `PageTabList` / `PageTab` + `Selected`; one Tab
  stop, Up / Down wrap, Home / End; the chevron Button ("Collapse / Expand the navigation pane")
  -> `on_collapse(bool)`; collapsed = glyphs only, the names stay. `on_select(index)`.
- **wizard_layout** (new): the `Stepper` rail (a step click -> `Step(i)`), the page with its
  title, Cancel | spacer | Back | Next or Finish (Back inert on step 0, Next / Finish inert while
  `can_go_next` is unset, Cancel hidden without a label); ONE callback with `WizardEvent { step,
  kind }`. A `Grouping` named "<title>: step i of n, <step>". Nothing equivalent existed
  (dialog.rs is a popup, form.rs is form data, stepper.rs is the rail alone).

## 4. Showcase (`examples/azul-widgets/src/mail.rs`, wired in `lib.rs`; this commit)

A "Mail" section with the three panes side by side (MessageList with "Posteingang durchsuchen
(Strg+E)", "Anordnen nach: Datum", "Neu nach alt"; ReadingPane with the "Klicken Sie hier, um
Bilder herunterzuladen ..." InfoBar until the action loads them; ToDoBar with "Keine anstehenden
Termine." and "Neue Aufgabe eingeben"), the ModuleSwitcher (E-Mail / Kalender / Kontakte /
Aufgaben, collapsible), the StatusBar ("Filter angewendet" | "Uebermittlungsfehler" / "Verbunden"
toggling on click, views, zoom) and the WizardLayout ("Konto hinzufuegen": Konto / Server /
Fertig, Zurueck / Weiter / Fertig stellen / Abbrechen). Every value is the app's (`MailDemo`), and
a status line shows the last report. It compiles only after api.json carries the new API (section
6) - the cards are written against the names listed there. The toolbar's "Dark" segment contrast
bug was not touched (known).

## 5. Commits

| commit | what |
|---|---|
| `822b65c94` | progress checkpoint |
| `028879ee7` | test: inline calendar with today, list view lazy-load hook, status bar sync, info bar (RED) |
| `c06c8bdca` | feat: the four above (GREEN) |
| `4f643014e` | test: message list, reading pane, to-do bar, module switcher, wizard layout (RED) |
| `d8525a81b` | feat: the five builds + flat / flora looks, `decl::border_top` (GREEN) |
| (this) | showcase "Mail" section, report, progress |

Combined RED pass: the genuine REDs are `028879ee7` and `4f643014e`.

## 6. api.json (never edited by hand; for `autofix add` / patch)

Docs ASCII; constructors `create*`; fields in decreasing alignment (new fields near the end);
callback args `CallbackType` with the widget's `..Callback::create(cb).to_core()` body, like the
neighbouring `with_on_row_click`-style entries; `&str` -> `String`.

- `DatePicker`: fields `today: OptionDatePickerState` (between `name` and `mode`), `inline: bool`
  (last); fns `set_inline(refmut, inline: bool)`, `with_inline(value, inline: bool) ->
  DatePicker`, `set_today(refmut, year: u32, month: u32, day: u32)`, `with_today(value, ...) ->
  DatePicker`. option `OptionDatePickerState` (repr `C, u8`, `[None, Some]`, Copy).
- `ListView`: fns `set_on_lazy_load_scroll(refmut, data: RefAny, callback:
  ListViewOnLazyLoadScrollCallbackType)`, `with_on_lazy_load_scroll(value, ...) -> ListView`.
- `StatusBarTheme`: field `sync_error: ColorU` (last). `StatusBarStyle`: field
  `sync_icon_error_style: OptionCssPropertyWithConditionsVec` (last), fn
  `resolved_sync_icon_error_style(ref) -> CssPropertyWithConditionsVec`. enum `StatusBarSyncKind`
  (repr C): `Connected`, `Syncing`, `Error`, `Offline`; fn `icon(value) -> String`. struct
  `StatusBarSync` (repr C): `label: String`, `on_click: OptionButtonOnClick`, `kind:
  StatusBarSyncKind`; fns `create(label: String, kind: StatusBarSyncKind)`, `set_on_click(refmut,
  data: RefAny, on_click: ButtonOnClickCallbackType)`, `with_on_click(value, ...)`. option
  `OptionStatusBarSync`. `StatusBar`: field `sync: OptionStatusBarSync` (between `style` and
  `theme`); fns `set_sync(refmut, sync: StatusBarSync)`, `with_sync(value, sync) -> StatusBar`.
- `InfoBar` (repr C): `icon: String`, `text: String`, `action: String`, `on_action:
  OptionButtonOnClick`, `kind: AlertKind`, `theme: OptionUiTheme`; fns `create(text: String)`,
  `set_icon` / `with_icon(icon: String)`, `set_action` / `with_action(action: String)`, `set_kind` /
  `with_kind(kind: AlertKind)`, `set_on_action` / `with_on_action(data: RefAny, callback:
  ButtonOnClickCallbackType)`, `set_theme` / `with_theme(theme: UiTheme)`, `swap_with_default`,
  `dom(value) -> Dom`. option `OptionInfoBar`.
- `MessageListEventKind` (repr C): `Select`, `Open`, `Flag`, `Delete`, `Sort`, `SortDirection`,
  `Search`, `Scope`, `Scroll`. `MessageListEvent` (repr C): `text: String`, `id: u64`, `index:
  usize`, `end: usize`, `kind: MessageListEventKind`, `shift: bool`, `ctrl: bool`; fn
  `create(kind, index: usize, id: u64)`. callback `MessageListOnEventCallbackType =
  (RefAny, CallbackInfo, MessageListEvent) -> Update` with `MessageListOnEventCallback`,
  `MessageListOnEvent { refany, callback }`, `OptionMessageListOnEvent`.
  `MessageRowKind` (repr C): `Message`, `Group`. `MessageRow` (repr C): `id: u64`, `from`,
  `subject`, `preview`, `date`, `icon: String`, `kind: MessageRowKind`, `unread`, `flagged`,
  `has_attachment`, `selected: bool`; fns `create(id: u64, from: String, subject: String)`,
  `create_group(title: String)`, `with_preview`, `with_date`, `with_icon(String)`, `with_unread`,
  `with_flagged`, `with_attachment`, `with_selected(bool)`. vec `MessageRowVec` (+ Destructor,
  DestructorType, Slice, `OptionMessageRow`). `MessageListSelection` (repr C): `rows: U32Vec`,
  `anchor: u32`; fns `create()`, `apply(value, index: u32, shift: bool, ctrl: bool) ->
  MessageListSelection`, `contains(ref, index: u32) -> bool`, `len(ref) -> usize`, `is_empty(ref)
  -> bool`. `MessageList` (repr C): `rows: MessageRowVec`, `scopes: StringVec`, `search`,
  `search_placeholder`, `sort_label`, `sort_field`, `sort_direction_label: String`, `on_select`,
  `on_open`, `on_flag`, `on_delete`, `on_sort`, `on_search`, `on_scope`, `on_scroll:
  OptionMessageListOnEvent`, `total_rows`, `first_row`, `row_height`, `scope: usize`, `theme:
  OptionUiTheme`, `sort_descending: bool`; fns `create(rows: MessageRowVec)`, `set_window` /
  `with_window(first_row: usize, total_rows: usize)`, `set_row_height` / `with_row_height(usize)`,
  `set_scopes` / `with_scopes(scopes: StringVec, active: usize)`, `set_search` /
  `with_search(String)`, `set_search_placeholder` / `with_search_placeholder(String)`, `set_sort` /
  `with_sort(label: String, field: String, descending: bool)`, `set_sort_direction_label` /
  `with_sort_direction_label(String)`, `set_theme` / `with_theme`, `set_on_<hook>` /
  `with_on_<hook>(data: RefAny, callback: MessageListOnEventCallbackType)` for the eight hooks,
  `swap_with_default`, `dom`.
- `ReadingPaneEventKind` (repr C): `Sender`, `LoadImages`, `Attachment`, `People`.
  `ReadingPaneEvent` (repr C): `text: String`, `index: usize`, `kind`. callback
  `ReadingPaneOnEventCallbackType = (RefAny, CallbackInfo, ReadingPaneEvent) -> Update` (+
  `ReadingPaneOnEventCallback`, `ReadingPaneOnEvent`, `OptionReadingPaneOnEvent`). `ReadingPane`
  (repr C): `subject`, `sender`, `date: String`, `fields: StringPairVec`, `attachments:
  StringVec`, `info_bar: OptionInfoBar`, `body: OptionDom`, `people: StringVec`, `people_line:
  String`, `on_link`, `on_load_images`, `on_attachment: OptionReadingPaneOnEvent`, `theme`; fns
  `create(subject: String, sender: String)`, `set_date` / `with_date`, `set_fields` /
  `with_fields(StringPairVec)`, `add_field(refmut, key: String, value: String)` /
  `with_field(value, key, value)`, `set_attachments` / `with_attachments(StringVec)`,
  `set_info_bar` / `with_info_bar(InfoBar)`, `set_body` / `with_body(body: Dom)`, `set_people` /
  `with_people(people: StringVec, line: String)`, `set_theme` / `with_theme`, `set_on_link` /
  `with_on_link`, `set_on_load_images` / `with_on_load_images`, `set_on_attachment` /
  `with_on_attachment` (`data: RefAny, callback: ReadingPaneOnEventCallbackType`),
  `swap_with_default`, `dom`.
- `ToDoTask` (repr C): `id: u64`, `title`, `due: String`, `done: bool`; fns `create(id: u64,
  title: String)`, `with_due(String)`, `with_done(bool)`. vec `ToDoTaskVec` (+ `OptionToDoTask`).
  `ToDoBarEventKind` (repr C): `DatePicked`, `TaskAdded`, `TaskToggled`, `TaskOpened`,
  `AppointmentOpened`. `ToDoBarEvent` (repr C): `text: String`, `id: u64`, `index: usize`,
  `date: DatePickerState`, `kind`. callback `ToDoBarOnEventCallbackType = (RefAny, CallbackInfo,
  ToDoBarEvent) -> Update` (+ wrapper, option). `ToDoBar` (repr C): `appointments: StringVec`,
  `appointments_empty`, `task_placeholder`, `task_text: String`, `tasks: ToDoTaskVec`,
  `accessibility_name: OptionString`, `on_pick`, `on_task`, `on_appointment:
  OptionToDoBarOnEvent`, `calendar: DatePickerState`, `today: OptionDatePickerState`, `theme`;
  fns `create(year: u32, month: u32, day: u32)`, `set_today` / `with_today(y, m, d)`,
  `set_appointments` / `with_appointments(StringVec)`, `set_appointments_empty` /
  `with_appointments_empty(String)`, `set_task_line` / `with_task_line(placeholder: String, text:
  String)`, `set_tasks` / `with_tasks(ToDoTaskVec)`, `set_accessibility_name` /
  `with_accessibility_name(String)`, `set_theme` / `with_theme`, `set_on_pick` / `with_on_pick`,
  `set_on_task` / `with_on_task`, `set_on_appointment` / `with_on_appointment` (`data: RefAny,
  callback: ToDoBarOnEventCallbackType`), `swap_with_default`, `dom`.
- `SwitcherModule` (repr C): `label: String`, `icon: String`; fn `create(label, icon)`. vec
  `SwitcherModuleVec` (+ `OptionSwitcherModule`). callbacks `ModuleSwitcherOnSelectCallbackType =
  (RefAny, CallbackInfo, usize) -> Update`, `ModuleSwitcherOnCollapseCallbackType = (RefAny,
  CallbackInfo, bool) -> Update` (+ wrappers, options). `ModuleSwitcher` (repr C): `modules:
  SwitcherModuleVec`, `on_select: OptionModuleSwitcherOnSelect`, `on_collapse:
  OptionModuleSwitcherOnCollapse`, `active: usize`, `theme`, `collapsed: bool`; fns
  `create(modules)`, `set_active` / `with_active(usize)`, `set_collapsed` / `with_collapsed(bool)`,
  `set_theme` / `with_theme`, `set_on_select` / `with_on_select(data, callback:
  ModuleSwitcherOnSelectCallbackType)`, `set_on_collapse` / `with_on_collapse(data, callback:
  ModuleSwitcherOnCollapseCallbackType)`, `swap_with_default`, `dom`.
- `WizardEventKind` (repr C): `Back`, `Next`, `Finish`, `Cancel`, `Step`. `WizardEvent` (repr C,
  Copy): `step: usize`, `kind`. callback `WizardOnEventCallbackType = (RefAny, CallbackInfo,
  WizardEvent) -> Update` (+ wrapper, option). `WizardLayout` (repr C): `steps: StringVec`,
  `page: OptionDom`, `title`, `back_label`, `next_label`, `finish_label`, `cancel_label: String`,
  `on_event: OptionWizardOnEvent`, `current_step: usize`, `theme`, `can_go_next: bool`; fns
  `create(title: String, steps: StringVec)`, `set_page` / `with_page(page: Dom)`,
  `set_current_step` / `with_current_step(usize)`, `set_labels` / `with_labels(back, next, finish,
  cancel: String)`, `set_can_go_next` / `with_can_go_next(bool)`, `set_theme` / `with_theme`,
  `set_on_event` / `with_on_event(data, callback: WizardOnEventCallbackType)`, `is_last_step(ref)
  -> bool`, `swap_with_default`, `dom`.
- Rust-only (no FFI): `list_view::scroll_settled_hook` / `scroll_window_of` (pub(crate)),
  `decl::border_top`, every `*Look` struct and `*_BASE` static, `todo_bar::task_key_adds`.

## 7. Least-sure-to-compile spots (read carefully, not compiled)

1. `azul_css::impl_option!(DatePickerState, OptionDatePickerState, [Debug, Copy, Clone, PartialEq,
   Eq, PartialOrd, Ord, Hash])` (date_picker.rs) - the derive list for a Copy option; the tile's
   `OptionTileCapacity` uses the same shape without `copy = false`.
2. `message_list.rs`: `row_identity(&mut info, ..)` uses `CallbackInfo::get_dataset(&mut self,
   node)`; `info.get_node_size(rows_box).map_or(0.0, |s| s.height)`; the `#[allow]` on a `let`
   statement (attributes on statements are stable) - if clippy objects, move it to the fn.
3. `message_list.rs` tests: `n.root.get_style().iter_inline_properties()` and
   `CssProperty::Height(h)` with `format!("{h:?}").contains("4000")` (the spacer height) - the
   Debug text of a `PixelValue` must contain the digits; if it is scaled (`4000000`), it still does.
4. `CallbackChange::SetNodeStyle { node_id, style, .. }` is matched by `NodeId` in the date_picker
   test (`Some(*node_id) == node.node.into_crate_internal()`), styles compared as Debug strings.
5. `module_switcher.rs`: the module Button's root is post-processed with `with_tab_index`,
   `with_accessibility_info` (PageTab replaces PushButton) and `with_callback(Focus(VirtualKeyDown),
   data, CoreCallback {..})` - `Dom::with_callback` appends (`NodeData::add_callback`).
6. `reading_pane.rs`: `InfoBar.on_action = Some(ButtonOnClick { refany, callback:
   ButtonOnClickCallback::from(on_load_images_click as ButtonOnClickCallbackType) }).into()`.
7. `statusbar.rs` `sync_dom`: `b.alt = label` names the Button (the `with_form_semantics` path
   sets the a11y name from a non-empty `alt`); the sync tests look for a node NAMED by the label.
8. flora.rs appended section uses the module's private consts `RADIO_GROUP_HOVER_LIGHT / _DARK`
   and `alert_stone` (same module), `super::style_kit::font_size`, `decl::border_top`.
9. The showcase (`examples/azul-widgets/src/mail.rs`) is written against api.json names that do
   not exist yet (`StatusBar::create` as the bindings call `new`); it passes `Vec<T>` where the
   bindings take a `TVec` the way `Accordion::create_with_sections(vec![..])` does, reads
   `MessageListSelection.rows.as_ref().first()` (a `U32Vec` through the bindings - if `as_ref` is
   not generated, replace with `contains` over the rows), and uses `"..".into()` for `String`.
10. The theme-contrast lint (`widgets::theme_contrast`) may object to a secondary ink on a strip
    in dark mode for the new looks (the sort band's `LIGHT_SOFT1 / DARK_SOFT1` on `DARK_STRIP`, the
    task_due / preview inks on the page); if it does, lift the ink one step (`SOFT1 -> INK2`).
11. `wizard_layout.rs`: `(current_step > 0).then_some(on_back as ButtonOnClickCallbackType)`
    (`bool::then_some`), `can_go_next.then_some(..)`.

## 8. Test commands (for the parent)

```
cargo test --release -p azul-layout --lib widgets::date_picker::inline_and_today_tests
cargo test --release -p azul-layout --lib widgets::list_view::lazy_load_tests
cargo test --release -p azul-layout --lib widgets::statusbar::sync_tests
cargo test --release -p azul-layout --lib widgets::info_bar
cargo test --release -p azul-layout --lib widgets::message_list
cargo test --release -p azul-layout --lib widgets::reading_pane
cargo test --release -p azul-layout --lib widgets::todo_bar
cargo test --release -p azul-layout --lib widgets::module_switcher
cargo test --release -p azul-layout --lib widgets::wizard_layout
cargo test --release -p azul-layout --lib widgets::          # every widget test + the manifest lints
cargo test --release -p azul-layout --lib widgets::date_picker   # the existing suite with the new fields
cargo test --release -p azul-layout --lib widgets::statusbar     # the existing suite with `sync`
```
Showcase, after autofix: `cargo build --release -p azul-dll --features build-dll`, stage
`target/azul-lib`, `AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzWidgets`;
headless through `run_capped.sh` only (it has a `<video>` card).

## 9. What is left (for MAIL2, which builds AzMail on these)

- The window itself: Ribbon (Datei / Start / Senden-Empfangen / Ordner / Ansicht with the groups
  of the reference) + Backstage ("Datei > Konto hinzufuegen" = the WizardLayout in a Backstage
  page), the navigation pane (Accordion Groups "Favoriten" + a TreeView per account - the tree
  has no unread-count badge yet: the tree_view would need a `count` / `badge` per node drawn in
  the accent, like `AccordionSection::count`; a small RED-first addition), the three panes in a
  SplitPane, the StatusBar with `sync`.
- The MessageList's window: the app keeps `MessageListSelection`, renders `first_row..` of its
  sorted, grouped flat list (group headers are rows: `MessageRow::create_group`), rebuilds on
  `Scroll { index, end }` with a margin of rows, and on a `Select` beyond the window moves the
  window there. Every row has the same `row_height`; rows of varying height are not supported.
- Dates / sort fields / scope semantics, the sender-address parsing of the sender line, the
  sanitized HTML body (the mail renderer is R1_MAIL_RENDER's), IMAP / SMTP, "load images".
- Keyboard shortcut Ctrl+E focusing the search box is the app's (a window-level hotkey).

Twins noticed (reported, not unified): every widget test module copies the small `texts` /
`node_labelled` / `id` helpers the house tests already copy per file (tile, address_bar); a
shared `theme_checks` twin for `StyledDom` lookups would retire them. The per-theme strip helpers
(`flat_strip_below` / `flora_strip_below`) are deliberately per theme (different tokens).
