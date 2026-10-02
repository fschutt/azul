# DEDUP review - AREA: WIDGETS_API (engine side of the range + API surface apps see)

Reviewed commit: 53c978b33 (branch fix/input-bugs-2026-09-19), range 112cf8342..HEAD (876 commits).
Scope: `git diff 112cf8342..HEAD -- layout/src/widgets core/src css/src dll/src api.json doc/src` + apps' `use azul::` surface.
Status: COMPLETE (33 findings, prioritized list and unverified items at the end)


## Index (kind / effort / risk / who benefits) - details below, F1..F33

| # | Kind | Title | Effort | Risk | Benefits |
|---|------|-------|--------|------|----------|
| F1 | 4/5 | impl_option!/impl_result! unqualified inner calls -> 77 (+9) hand imports | S | low (page_breaks.rs dirty elsewhere) | every widget file |
| F2 | 1 | ModuleSwitcher vs ShellNavigationPane's module switcher | M | low-med | mail, drive, contacts, calendar, notes, tasks, shells |
| F3 | 1 | 9 private `hook()` CoreCallbackData builders, no core ctor | S | none | widgets |
| F4 | 1 | single-property style helpers per widget; decl.rs vs style_kit.rs | M | low | widgets |
| F5 | 1 | HTML/XML escaping x8, no core encoder | S | none | mail, widgets, dll |
| F6 | 2 | common list selection model (Drive/Tasks/Show x2) | M | low | drive, tasks, show, contacts |
| F7 | 2 | virtual-row window helpers inside list_view.rs | S | none | mail, notes, review |
| F8 | 2 | key/value rows (DetailsPane/ReadingPane/WizardSummary + 5 app helpers) | S-M | none | calc, meet, contacts, writer |
| F9 | 2/3 | primary-modifier rule re-derived ~30x; no command table | S + M | macOS Ctrl behaviour change | all apps |
| F10 | 1/3 | byte-size formatting x4 | S | none | drive, mail, tasks |
| F11 | 2 | apps format dates by hand; ICU formatting unused | M | none | mail, notes, tasks, contacts |
| F12 | 3 | ShellThemeAccent::colors not exported -> AzShow copies ramps | S | none | show, setup, meet |
| F13 | 3 | by-value CallbackInfo -> 3 `reborrow_info` copies | S | none | writer, notes, sheets |
| F14 | 3 | no text/attribute accessor -> 3 unsafe `box_str`, href-in-class | S | none | writer, notes, mail |
| F15 | 3 | ButtonOnClick has no ctor; StatusBarZoom no hook setters | S | none | sheets, show, writer |
| F16 | 3 | free helpers not bindable (fit_within, palette_matches, anchor tuple) | S | none | videocut, show |
| F17 | 4 | 14 widget *VecSlice misfiled; autofix rule bug (3 copies of vec rule) | S | none | binding users |
| F18 | 4 | changeset types over 5 modules; `misc` junk drawer; video types in widgets | S | none | editor apps, bindings |
| F19 | 4 | general widgets filed as `Shell*` in `shells` | M | import churn | 10+ apps |
| F20 | 4 | MessageList is a generic list under a mail name | M | rename churn | mail, notes, contacts, tasks |
| F21 | 4 | N per-kind setters of one callback type (MessageList x8 ...) | S | none | mail, notes, tasks, calendar |
| F22 | 4 | naming: kind setters, severity enums, event names, search rules, new/create | S each | low | bindings |
| F23 | 5 | prose-wrap table test loosened (>=3 -> >=2, union of baselines) | S | none | engine |
| F24 | 5 | E-INLINE bug sidestepped without a RED test | S | none | engine |
| F25 | 5 | widget tests: 15 `texts()`, 13 `id()` copies | S | none | widgets |
| F26 | 5 | micromail 0.1 + 0.2 both in Cargo.lock | S | low | supply chain |
| F27 | 1 | PathInput re-implements FileInput's picker | S-M | low | setup, settings dialog |
| F28 | 1 | look-merge (`follow_look`/`look_for`) copied 5x | S | none | widgets |
| F29 | 1 | S-shells re-declare OfficeShell chrome (~98 setters) | M | call-order churn | shell users |
| F30 | 2 | no Toolbar widget (4 hand-built) | S-M | none | contacts, notes, review |
| F31 | 1 | two char-reference decoders in core | S | low | engine |
| F32 | 1/3 | hex colour format/parse re-made; to_hash wrong shape | S | none | show, photo, writer, sheets |
| F33 | 1 | Alert / Toast / InfoBar near-clones | M | low | all |

## Kind 4 / 5 - macro hygiene (the impl_option! follow-up)

### F1. `impl_option!` / `impl_result!` call their `*_inner!` helpers unqualified -> 77 files import `impl_option_inner` by hand
- Evidence: `css/src/macros.rs:1101,1124,1147` (`impl_option_inner!($struct_type, $struct_name);` in each of the 3
  `impl_option!` arms) and `css/src/macros.rs:1211,1229,1248` (`impl_result_inner!(..)` in each `impl_result!` arm).
  No `$crate::` anywhere in `css/src/macros.rs`.
- Count (verified with `grep -rln --include='*.rs' impl_option_inner core css layout dll examples doc`, minus
  macros.rs): **77 files** = 49 in `layout/src/widgets/**`, 17 elsewhere in `layout/src` (thread, image, callbacks,
  fluent, http, file, fmt, window_state, desktop/dialogs, managers/{changeset,drag_drop,gesture,scroll_state,undo_redo,
  selection}, solver3/{page_breaks,pagination}), 8 in `dll/src` (unified/{video_codec,sqlite,audio},
  desktop/extra/{video_codec/pipeline, video_codec/mod, sqlite/mod, audio/mod, iroh/types}), 2 in `core/src`
  (hid.rs, json.rs - redundant there: core already has `#[macro_use] extern crate azul_css;` at core/src/lib.rs:80),
  1 in `css/src/props/layout/dimensions.rs`. 22 of the 77 were ADDED in the range. 9 files also import
  `impl_result_inner` by hand (core/src/{json,url}.rs, layout/src/{http,file,document_edit,image}.rs,
  layout/src/xml/svg.rs, dll/src/unified/sqlite.rs, dll/src/desktop/extra/sqlite/mod.rs).
- The import style is also split: 17 files call `azul_css::impl_option!` qualified (22 call sites) - yet still need
  the bare `impl_option_inner` import - and 44 call `impl_option!` unqualified (import both). Four files carry a
  comment explaining the workaround: `layout/src/widgets/{map.rs:37, microphone.rs:28, video.rs:27,
  capture_common.rs:32}` ("for impl_widget_callback!'s impl_option!"). `impl_widget_callback!`
  (layout/src/widgets/mod.rs:20) itself already writes `azul_css::impl_option!` (line ~42) but every caller
  still has to import `impl_option_inner` (and `RefAny`, which the macro also names unqualified).
- Fix (S): in css/src/macros.rs replace the 3 `impl_option_inner!` calls with `$crate::impl_option_inner!` and the
  3 `impl_result_inner!` calls with `$crate::impl_result_inner!`; in `impl_widget_callback!` write
  `azul_core::refany::RefAny` instead of bare `RefAny`. Then delete `impl_option_inner` / `impl_result_inner`
  from the 77 + 9 import lists (otherwise they become unused-import warnings), optionally normalising all
  callers to `azul_css::impl_option!` so the `impl_option` import goes too. Mechanical (sed over the file list);
  `#[macro_export]` macros resolve `$crate::name!` at the crate root, works inside azul-css itself too.
- Risk: low (pure macro path); the only conflict risk is `layout/src/solver3/page_breaks.rs`, which the OTHER
  session has modified in the working tree - leave that one file for last / coordinate.
- Benefits: every widget file and every future widget (agents copy the 6-line import block per file).

## Kind 1 - DUPLICATION (widgets vs widgets, engine vs engine)

### F2. Two "Outlook 2010 module buttons" widgets: `ModuleSwitcher` and the switcher inside `ShellNavigationPane`
- Evidence: `layout/src/widgets/module_switcher.rs` (798 lines, new in range, b33de3cb0) - `SwitcherModule {label, icon}`
  (:120), `ModuleSwitcher {modules, on_select, on_collapse, active, theme, collapsed}` (:158), `on_module_click`
  (:384), `on_chevron_click` (:402), `on_module_key` (:418). `layout/src/widgets/shells/navigation_pane.rs`
  (new in range, d5e1428f6) - `ShellNavigationModule {label, icon, badge}` (:238), `modules / active_module /
  collapsed` on the pane (:294-426), `on_module_click` (:609), `on_collapse_click` (:621), `on_module_key` (:636),
  `module_node` (:681), `footer` (:811). Both module docs say "Outlook 2010's module buttons ... collapses to a
  strip of glyphs".
- Confirmation: read both key handlers side by side - same roving shape (`roving::plain_key` -> Up/Down/Home/End ->
  `roving::items_of(parent, CLASS)` -> `step_target` -> `move_stop`), renamed locals. DIVERGED: ModuleSwitcher wraps
  at the ends (`step_target(.., true)`, :436), the pane does not (`.., false`, :655); the pane has a badge per
  module, ModuleSwitcher has none; ModuleSwitcher reports via two callbacks (`on_select(usize)`,
  `on_collapse(bool)`), the pane via one event enum. The pane is NOT built from ModuleSwitcher.
- Adoption: `ShellNavigationPane` is used by 7 apps (azul-contacts, -calendar, -mail, -notes, -drive, -shells,
  -tasks); `ModuleSwitcher` only by the showcase `examples/azul-widgets/src/mail.rs`.
- Proposal (M): make the pane's footer `ModuleSwitcher::dom()` (add `badge: AzString` to `SwitcherModule`,
  pick one wrap rule - APG tabs wrap, so `true`), drop `ShellNavigationModule` (or make it a type alias in
  api.json terms: rename to one `NavigationModule`), forward ModuleSwitcher's two callbacks into the pane's
  `ModuleSelected` / `CollapseToggled` events. Removes ~250 lines + one api.json type pair.
- Risk: low-medium (pane look parts `nav_module*` vs `ModuleSwitcherLook` must be merged; e2e scripts of AzMail /
  AzDrive click the module buttons by name - names stay the same).

### F3. Nine private copies of `fn hook(event, cb, refany) -> CoreCallbackData` (no constructor in core)
- Evidence: `layout/src/widgets/shortcut_recorder.rs:432`, `cell_grid.rs:2659`, `timeline.rs:1062`,
  `thumbnail_strip.rs:608`, `shells/navigation_pane.rs:667`, `shells/command_palette.rs:621`,
  `shells/mobile_shell.rs:478`, `shells/settings_layout.rs:503` (+ `tile.rs:420`, a variant taking an Option
  hook). All nine files are new in the range. Across `layout/src/widgets` there are 134 hand-written
  `CoreCallbackData { event, callback: CoreCallback { cb, ctx: OptionRefAny::None }, refany }` literals in 40
  files. `core/src/callbacks.rs` has no `impl CoreCallbackData` (only `impl CoreCallbackDataVec`, :2153).
- Confirmation: printed all nine bodies - identical struct literal; DIVERGED only in signature: `(event, cb: usize,
  refany: RefAny)` x4, `(event, cb: extern fn, data: RefAny)` (timeline), `(event, cb: extern fn, data: &RefAny)`
  (cell_grid), `(event, data: &RefAny, cb: usize)` (thumbnail_strip - argument order swapped).
- Proposal (S): `impl CoreCallbackData { pub fn create(event: EventFilter, refany: RefAny, cb: usize) -> Self }`
  in core/src/callbacks.rs (plus a `from_fn` taking `extern "C" fn(RefAny, CallbackInfo) -> Update`), replace the
  nine helpers; optionally sweep the 134 literals. Not an api.json change (Dom::with_callback already covers apps).
- Risk: none.

### F4. Single-property style helpers re-made per widget; two parallel theme-declaration kits
- Evidence (single-property): `layout/src/widgets/timeline.rs:1578-1612` (`type P`, `grow`, `no_shrink`,
  `display_flex`, `flex_direction`, `position`, `overflow_x_hidden`, `overflow_y_hidden`, `nowrap`) and
  `:1740-1768` (`classes`, `px_width`, `px_height`, `px_left`, `px_top`, `px_bottom`); `cell_grid.rs:1982`
  (`simple`), `:2178-2190` (`px_width`, `px_height`, `px_min_width` - identical bodies modulo `LayoutWidth::px`
  vs `LayoutWidth::Px(PixelValue::px)`); `const fn simple(p)` again at `dialog_kit.rs:452` and
  `shells/mod.rs:435`; `fn classes(&[&str]) -> IdOrClassVec` exists in 24 widget files (datetime_local,
  titlebar, radio_group, switch, drop_down, check_box, date_picker, pagination, timeline, dialog, menubar,
  file_input, text_input, time_picker, color_input, spinner, tabs, button, frame, label, slider, card,
  progressbar + `dialog_kit::class`). 213 inline `const_display(LayoutDisplay::Flex)` constructions in 40 widget
  files (ribbon.rs alone 38, shells/mod.rs 15, wizard_layout.rs 8).
- Evidence (two kits, both pre-range but both used by in-range code): `layout/src/widgets/themes/decl.rs` (519
  lines, `pub(crate)`, used by 21 widget files) and `layout/src/widgets/themes/style_kit.rs` (506 lines, `pub`,
  used by 19 files) define the same helpers: `decl::fill(color)` == `style_kit::bg(color)`, `layers` == `layers`,
  `ink` == `ink`, `themed_fill` == `themed_bg`, `themed_ink`, `themed_layers`, `radius`, `padding`,
  `focus_halo`, `hover_fill` == `hover_bg`, `hover_layers`, `active_fill` == `active_bg`, `active_layers`,
  `hover_ink`. Misleading: `decl::fill(ColorU)` is a BACKGROUND, `style_kit::fill()` is `width/height: 100%`.
- Confirmation: read both files' heads and bodies (decl.rs:47-100 vs style_kit.rs:68-135): same constructors.
- Proposal (M): one `widgets::themes::decl` (merge style_kit into it, keep `style_kit` names as `pub use`
  aliases for one release), plus a `widgets::themes::decl::layout` section with the const single-property
  helpers (`display_flex`, `flex_row`, `flex_column`, `grow(n)`, `no_shrink`, `min_w0`, `min_h0`,
  `px_width/height/left/top/bottom`, `overflow_hidden`, `nowrap`, `pointer`, `no_select`) and ONE `classes()`;
  shells/mod.rs's `*_BASE` statics and dialog_kit's become compositions of those. Rename `style_kit::fill()` to
  `fill_box()` in the merge.
- Risk: low (pure refactor, the theme lints in `shells::shell_lints` and `theme_checks` catch drift).

### F5. HTML / XML text escaping written 8 times; core has a decoder but no encoder
- Evidence: `layout/src/widgets/cell_grid.rs:1851` (`html_escape`, in range, & < > "),
  `layout/src/managers/selection.rs:274-279` (inline `.replace` chain, & < > only - no `"`),
  `layout/src/xml/mod.rs:1203` (`escape`), `layout/src/e2e/builder.rs:2731` (`escape_xml`, + '),
  `layout/src/managers/notification.rs:620` (`xml_escape`, + ', drops C0 controls),
  `dll/src/web/html_render.rs:1090,1103` (`html_escape` & < >, `html_escape_attr` + "),
  `examples/azul-mail/src/compose.rs:693` (`escape_html`, byte-identical to cell_grid's),
  `examples/azul-mail/src/html.rs:907` (`escape_into(out, s, attribute)`).
- Confirmation: printed all bodies; cell_grid.rs:1851-1863 vs azul-mail compose.rs:693-705 are identical (renamed).
  `core/src/xml_html.rs` (new in range) exports `decode_character_references` (:182) but no encoder.
- Proposal (S): `pub fn encode_text(s: &str) -> String` / `encode_attribute(s)` next to
  `decode_character_references` in core/src/xml_html.rs (attribute = also `"` and `'`; text = & < >), export
  through api.json as statics on an existing `xml` class (api.json binds only class members, no free fns) so
  AzMail drops both copies; layout/dll call sites switch. selection.rs's clipboard HTML misses `"` - harmless
  in text nodes, but its `style="..."` attribute values are format!-ed from numbers, so fine.
- Risk: none.

## Kind 2 - REUSE CANDIDATES (shared infrastructure the widgets/apps lack)

### F6. A common list SELECTION model: written 4x in apps, the widgets hand the rule to the app
- Evidence: `examples/azul-drive/src/model.rs:135-332` (`Selection {keys, anchor, focus}` with `click`, `toggle`,
  `extend`, `add_range` (Ctrl+Shift), `select_all`, `clear`, `invert`, `retain`, `set`, `step` (arrow keys with
  Shift/Ctrl) + tests :829,:840 - the most complete); `examples/azul-tasks/src/state.rs:367-398`
  (`select(id, shift, ctrl)`, no Ctrl+Shift, no keyboard step); `examples/azul-show/src/editor.rs:137-167`
  (`rail_select(index, shift, ctrl)` - DIVERGED: Ctrl refuses to deselect the last slide, keeps the vec sorted)
  and `:282` (`select_element`, same rule for canvas objects). Widgets: `ThumbnailStrip` reports `{index, shift,
  ctrl}` and its doc says "`ctrl` toggles - the app's selection rule" (`layout/src/widgets/thumbnail_strip.rs:14-15,
  224-227, 848-856`); `CellGrid` keeps its own 2-D anchor/range (`cell_grid.rs:238, 1557-1592`); MessageList is
  single-select (`selected_index`).
- Confirmation: read all four app copies; same anchor algorithm (`(lo, hi) = minmax(anchor, index)` ->
  `order[lo..=hi]`), renamed fields.
- Proposal (M): `ListSelection` (non-UI, `layout/src/widgets/selection_model.rs` or core) - `{ items: U64Vec,
  anchor: OptionU64, focus: OptionU64 }`, `click(key)`, `toggle(key)`, `extend(key, order: &U64Vec)`,
  `add_range`, `select_all(order)`, `clear`, `invert(order)`, `retain(order)`, `step(delta, extend, keep, order)`,
  `contains`, `single`; port Drive's tests. Export in api.json (`widgets`). Then ThumbnailStrip / ListView /
  MessageList (multi-select mode) can optionally apply it themselves (`with_selection(ListSelection)`), and
  AzDrive / AzTasks / AzShow (rail) / AzContacts (list) drop their copies. Keys as u64 (apps map string ids).
- Risk: low (pure model, tested); apps keep their own copy until they migrate.

### F7. The virtualised-row window lives inside `list_view.rs` as `pub(crate)` helpers
- Evidence: `layout/src/widgets/list_view.rs:909-935` (`ListView::visible_row_range`), `:1338-1395`
  (`scroll_settled_hook`, `scroll_window_of` - "Shared by every virtualised list"). Reused by
  `message_list.rs:67, 1313-1324, 1664` (+ its own `spacer` :1340). `cell_grid.rs` has its own band geometry
  (`axis_bands` :1211, `band_at` :1337, `scroll_by` :1622, variable sizes + frozen panes - legitimately
  different). `examples/azul-review/src/ui.rs:410-453` hand-rolls the same first-visible arithmetic in a
  `VirtualView` callback (`first_visible = scroll_offset.x / stride`).
- Proposal (S): move the three helpers + `spacer(rows, row_height)` to `layout/src/widgets/virtual_rows.rs`
  (pure, `pub(crate)`), and export `visible_row_range(scroll_y, viewport_h, row_h, total) -> (usize, usize)` as
  a free fn / `VirtualRows::visible_range` in api.json for apps that drive a `VirtualView` themselves.
- Risk: none.

### F8. "Labelled field" / key-value rows built per widget and per app
- Evidence (widgets, all `StringPairVec` -> rows): `details_pane.rs:63-75, 306-330` (`properties`, key + ":"),
  `reading_pane.rs:153-162, 325-339` (`fields`, key + ":"), `wizard_pages.rs:1427-1447, 1492-1493`
  (`WizardSummaryPage.rows`). Apps: `examples/azul-calculator/src/ui.rs:951` `labelled` (row, 110px label),
  `examples/azul-meet/src/ui.rs:771` `labelled` (column), `examples/azul-widgets/src/lib.rs:107` `labelled` /
  `captioned`, `examples/azul-contacts/src/ui.rs:530` `field_row` (96px label), `examples/azul-writer/src/
  backstage_ui.rs:116` `property_row`.
- Proposal (S-M): one `PropertyList` widget (`layout/src/widgets/property_list.rs`): `create(StringPairVec)`,
  `with_key_width(px)`, `with_layout(Row|Stacked)`, `with_colon(bool)`, theme; plus `LabelledField::create(label,
  control: Dom)` that also sets the control's accessible name (what azul-widgets' `labelled` does by hand).
  DetailsPane / ReadingPane / WizardSummaryPage render through it. Lower value than F6.

### F9. No shared command / shortcut table; the "primary modifier" rule is re-derived (wrongly) ~30 times
- Evidence (rule): `core/src/window.rs:653` `KeyboardState::primary_down()` (Cmd on macOS, Ctrl elsewhere; the
  engine's menu accelerators use it via `core/src/menu.rs:47 accelerator_matches`) is NOT in api.json
  (`KeyboardState` and `KeyModifiers` export no functions; `VirtualKeyCodeCombo` none either). Apps therefore
  write `let primary = mods.ctrl || mods.meta;` - 21 app files (appkit/ui, calculator/ui, calendar/{lib,
  editor_ui,timegrid}, contacts/ui, drive/{actions,ui_panes,ui_view x2}, mail/{ui_main,ui_compose}, meet/lib,
  notes/{ui,editor x2}, photo/canvas, sheets/lib, shells/lib, show/lib:426, tasks/{lib,list}, videocut/lib x2).
  The widgets do the same with `ks.ctrl_down() || ks.super_down()` although they could call `primary_down()`:
  `selection_adorner.rs:2208,2328`, `cell_grid.rs:2902,3183`, `timeline.rs:1058`, `message_list.rs:1056,1141`,
  `thumbnail_strip.rs:856,941`, `combobox.rs:1370`, `slider.rs:496`, `split_pane.rs:872` (0 widget calls to
  `primary_down`). Three widgets carry the identical helper `fn modifiers(info) -> (bool, bool)`:
  `timeline.rs:1056`, `message_list.rs:1054`, `selection_adorner.rs:2206` (diffed: byte-identical bodies).
  DIVERGENCE: on macOS `ctrl || meta` makes Ctrl+S save / Ctrl+click toggle, where the engine's own menus and
  `primary_down` only accept Cmd (Ctrl+click is the macOS secondary click).
- Evidence (table): the shortcut LABEL is a free string in `ShellPaletteCommand.shortcut`
  (`shells/command_palette.rs:136`), menus carry `VirtualKeyCodeCombo` accelerators, `GlobalHotkey` has
  `parse` / `to_display_string`, and every app keeps its own key->command match: `examples/azul-drive/src/keys.rs`
  (own `Key` / `Mods` mirror of VirtualKeyCode), `examples/azul-photo/src/canvas.rs:339 shortcut(key, Mods, tool)`,
  `examples/azul-tasks/src/chrome.rs:158 Command::shortcut() -> &str`, `examples/azul-notes/src/ui.rs:205-245`
  (match on `(key, primary, shift)`), `examples/azul-show/src/lib.rs:420`, `examples/azul-appkit/src/shortcuts.rs`
  (`Shortcut {group, keys: "Mod+C", action}` + `display_keys(keys, mac)` - a twin of
  `GlobalHotkey::to_display_string`; appkit is used by calculator + contacts only).
- Proposal: (S) export `KeyboardState::primary_down`, `shift_down`, `alt_down`, `is_key_down` and a
  `VirtualKeyCodeCombo::matches(&KeyboardState, VirtualKeyCode) -> bool` (wrapping `accelerator_matches`) in
  api.json; replace the 13 widget sites with `primary_down()` and delete the 3 `modifiers()` copies.
  (M) a `CommandTable` widget-side model (`layout/src/widgets/command_table.rs`): `Command {id: u32, label,
  icon, category, shortcut: OptionGlobalHotkey}`, `CommandTable::find(&KeyboardState, VirtualKeyCode) ->
  OptionU32`, `to_palette() -> ShellPaletteCommandVec` (shortcut text from `to_display_string`), `to_menu_items`,
  `to_settings_rows`. Adopters: every app with a palette (notes, shells, tasks) or a window key handler
  (calendar, drive, notes, sheets, show, tasks, photo); appkit's `Shortcut` table folds into it.
- Risk: (S) behaviour change on macOS for Ctrl+letter (intended); (M) none until adopted.

### F10. Byte-size formatting: one exported-by-accident widget fn, three app copies, four different outputs
- Evidence: `layout/src/widgets/tile.rs:141-160` `pub fn format_bytes(u64)` ("1.5 KB", no decimal >= 10; also
  used by `wizard_pages.rs:63,1079,1093,1282,1331` - a widget importing a formatter from another widget file);
  `examples/azul-drive/src/browse.rs:261-279` `format_size(Option<u64>)` (always 1 decimal - same UNITS table,
  same loop, renamed); `examples/azul-mail/src/ui_main.rs:1142` `human_size(usize)` (KB rounded UP, MB 1 dec);
  `examples/azul-tasks/src/detail.rs:585` `size_text(u64)` (KB truncated, MB 1 dec). Not in api.json (api.json has
  no free functions; `TileCapacity::label` is the only exported user).
- Confirmation: diffed tile.rs:145-160 vs browse.rs:263-279 - identical except the `< 10.0` branch.
- Proposal (S): move `format_bytes` to `layout/src/file.rs` next to `DiskSpace` and export it as a static
  `DiskSpace::format_bytes(bytes: u64) -> AzString` (module `file`); AzDrive / AzMail / AzTasks call it (AzDrive
  keeps its `Option` wrapper). Pick the tile rule (Explorer's).
- Risk: none (display only; AzDrive's e2e may assert "5.0 MB" style strings - check `examples/azul-drive/scripts`).

### F11. Apps format dates by hand although the engine exports ICU date formatting
- Evidence: api.json `CallbackInfo::{format_date, format_time, format_datetime, format_list, pluralize}` and
  `IcuLocalizerHandle::*` exist; `grep -rln icu examples/*/src` finds only `examples/rust/src/icu_demo.rs` (and a
  false hit in meet). Instead: `examples/azul-mail/src/listing.rs:62-70` (English weekday names, "Today" /
  "Yesterday" groups), `examples/azul-contacts/src/contact.rs:90` (English month names),
  `examples/azul-storage/src/time.rs:6` (month abbreviations), `layout/src/widgets/date_picker.rs:456,480,624`
  (private `month_name` / `weekday_name` / abbreviations), plus each app's `short_date` / `day_label`
  (notes model, tasks model, drive `format_modified`).
- Proposal (M): not a dedup of code so much as of locale logic - export `DatePicker`'s month/weekday names as
  `IcuLocalizerHandle`-backed statics, and give the relative-date rule ("Today", "Yesterday", weekday within a
  week, else short date) one home: `IcuLocalizerHandle::format_relative_date(date, today) -> AzString`. Apps
  that format in model code (tested without a window) can keep English fallbacks. Low priority (next wave 3).

## Kind 3 - API.JSON EXPOSURE (things apps copy or work around)

### F12. `ShellThemeAccent::colors` / `ShellThemeAccentColors` not exported -> AzShow copies the 5 ramps
- Evidence: `layout/src/widgets/shells/theme_scope.rs:41-133` (`ShellThemeAccent`, `ShellThemeAccentColors`,
  `ALL`, `name() -> &'static str`, `from_name(&str)`, private `light()`, `pub const fn colors(dark)`).
  api.json: `shells.ShellThemeAccent` has 0 ctors / 0 fns, `ShellThemeAccentColors` is MISSING.
  `examples/azul-show/src/themes.rs:7-66` spells out `STONES` - all 20 colours identical to theme_scope.rs:104-108
  (verified value by value: Blue 2F4A85/1E3260/E0E4EE/7A93C6 ... Slate 4A5C6B/354551/DEE3E7/8AA0B0) and
  `PAPER` = `ON_ACCENT` F4F2EA; its doc says "spelled out here because ShellThemeAccent::colors is not in the
  public API yet". Related: `examples/azul-setup/src/lib.rs:274` and `shells/settings_dialog.rs:1573,1743`
  hard-code the Blue accent `(47, 74, 133)` as a default colour; `examples/azul-meet/src/routes.rs:759`
  writes the name "leaf" by hand (would be `ShellThemeAccent::name`).
- Proposal (S): autofix-add `ShellThemeAccentColors` (struct, `shells`) and `ShellThemeAccent::colors(dark: bool)`;
  add FFI-friendly `ShellThemeAccent::name_string() -> AzString` and `from_name(AzString) ->
  OptionShellThemeAccent` (the `&'static str` / `&str` signatures cannot be bound). Then
  `themes::STONES` reads `ShellThemeAccent::colors(false)`.
- Benefiting apps: AzShow (deletes ~40 lines), AzSetup, AzMeet; any app with an accent picker.

### F13. `Pdf::from_dom_in_callback` takes `CallbackInfo` BY VALUE -> 3 apps hand-write a field-by-field copy
- Evidence: api.json `pdf.Pdf.from_dom_in_callback` args `[self ref, callback_info: CallbackInfo (by value),
  dom, w, h]`, body `object.from_dom_in_callback(&callback_info, ...)` - the Rust fn takes `&CallbackInfo`
  (dll/src/desktop/extra/pdf/mod.rs:140-142). Same by-value shape for `ProgressBar::update_progress`,
  `StatusBar::update_segment_label`, `Dialog::close_from`, `Dialog::request_close_from` (Rust: `&mut
  CallbackInfo`). Copies of `fn reborrow_info(info: &CallbackInfo) -> CallbackInfo { ref_data, hit_dom_node,
  cursor_relative_to_item, cursor_in_viewport, changes }`: `examples/azul-writer/src/lib.rs:630`,
  `examples/azul-notes/src/ui.rs:1237`, `examples/azul-sheets/src/lib.rs:2546` ("the pattern AzWriter's PDF
  export uses") - identical bodies, and each breaks if `CallbackInfo` gains a field.
- Confirmation: the generated `CallbackInfo` is `#[derive(Copy, Clone)]` (target/codegen/azul.rs:5383-5386), so
  `reborrow_info(info)` is just `*info` - which AzPhoto (`examples/azul-photo/src/canvas.rs:118`) and AzPaint
  (`examples/azul-paint/src/lib.rs:1241`) already write.
- Proposal (S): app side, replace the 3 helpers with `*info`. API side (consistency, optional): declare the 5
  args as `&mut CallbackInfo` / `&CallbackInfo` in api.json so the bindings mirror the Rust signatures and no
  caller needs to know CallbackInfo is a copyable pointer bag.

### F14. Text of a text node: no accessor -> 3 apps `unsafe`-deref `BoxOrStaticString`
- Evidence: `fn box_str(s: &BoxOrStaticString) -> &str { unsafe { match s { Boxed(p) => (**p).as_str(), Static(p)
  => (**p).as_str() } } }` in `examples/azul-writer/src/document.rs:36` (used :454,:545,:772,:1203),
  `examples/azul-notes/src/editor.rs:416` ("copied from AzWriter"), `examples/azul-mail/src/editor.rs:57`.
  api.json: `css.BoxOrStatic` / `BoxOrStaticString` (a type_alias) have no functions; `dom.NodeType` has none.
  Rust has `BoxOrStatic::as_ref()` (css/src/css.rs:645) and `NodeType::format()`.
- Proposal (S): export `NodeType::get_text() -> OptionString` (text node payload, owned) - or a non-generic
  `BoxOrStaticString::as_string() -> String` - in api.json; delete the 3 unsafe helpers.
- Also: `NodeData` exports `set_attributes`/`with_attribute(s)` but no getter (Rust has `NodeData::attributes()`
  core/src/dom.rs:2862), so AzMail stores a link's href in a CLASS (`azmail-href:<address>`,
  examples/azul-mail/src/editor.rs:16-18). Export `NodeData::get_attributes() -> AttributeTypeVec` (S).

### F15. Callback structs built by hand: `ButtonOnClick` has no constructor, `StatusBarZoom` no hook setters
- Evidence: `ButtonOnClick { data, callback: ButtonOnClickCallback { cb, callable: OptionRefAny::None } }` in
  `examples/azul-sheets/src/lib.rs:1655-1667` (`zoom_click`), `examples/azul-show/src/views.rs:650-657` (`click`),
  `examples/azul-writer/src/editor_ui.rs:287-296` (`button_click`); `SliderOnValueChangeCallback {..}` x2 likewise.
  api.json: `widgets.ButtonOnClick` 0 ctors / 0 fns, `dom.ButtonOnClickCallback` 0 ctors;
  `StatusBarZoom` exports only `office_2013` + `with_percent` - its `on_zoom_in` / `on_zoom_out` fields are
  written directly.
- Proposal (S): `ButtonOnClick::create(data: RefAny, cb: ButtonOnClickCallbackType)` (generic for every
  `impl_widget_callback!` wrapper: emit a `create(refany, cb)` on the `$callback_wrapper` in the macro and let
  autofix pick it up), `StatusBarZoom::with_on_zoom_in / with_on_zoom_out / with_on_reset`.

### F16. Pure helpers the apps re-implement because api.json cannot bind free functions
- `core/src/image_scale.rs:659 fit_within` vs `examples/azul-videocut/src/render.rs:99 fit_within` (same
  signature; DIVERGED: core returns (0,0) for a zero side, the copy clamps to 1x1; rounding identical) -
  export as `RawImage::fit_within(w, h, max_w, max_h) -> (u32,u32)`-shaped static (a small `LayoutSize`-like
  return struct, since tuples do not bind). `RawImage::thumbnail` IS exported now (DRIVE2's note is resolved).
- `tile::format_bytes` (F10), `ListView::visible_row_range` (F7), `palette_matches`
  (`shells/command_palette.rs:227`) - all `pub fn` without a class; give each a static on its type.
- `SelectionAdorner` `AdornerHandle::anchor()` returns a tuple, not exported (SHOW report :175) - return a
  `LogicalPosition`-shaped struct instead.

## Kind 4 - SEMANTICS / PLACEMENT

### F17. 14 widget `*VecSlice` types misfiled by keyword, and autofix's move check would keep them there
- Evidence (api.json vs `doc/src/autofix/module_map.rs:847 widget_module_for`, which says "a `*VecSlice` stays
  with its widget"): `css.CellGridRangeVecSlice`, `css.CellGridSizeVecSlice`, `css.TimelineClipVecSlice`,
  `task.ToDoTaskVecSlice`, `option.WizardOptionVecSlice`, `component.WizardComponentVecSlice` (all 6 NEW in the
  range) + 8 older node_graph slices in `dom` (`InputConnectionVecSlice`, `InputNodeAndIndexVecSlice`,
  `InputOutputTypeIdInfoMapVecSlice`, `InputOutputTypeIdVecSlice`, `NodeIdNodeMapVecSlice`,
  `NodeTypeFieldVecSlice`, `NodeTypeIdInfoMapVecSlice`, `OutputNodeAndIndexVecSlice`). Verified by running the
  widget rule over every api.json class with an `azul_layout::widgets::` external path: exactly these 14 differ.
- Root cause: `get_correct_module_with_path` (module_map.rs:886-912) treats `*vecslice` as STRUCTURAL and returns
  `determine_module`'s keyword answer BEFORE it consults `widget_module_for`; `determine_module` has no vecslice
  rule, so "Grid" -> css, "Task" -> task, "Option" -> option, "Component" -> component, "Node"/"Input" -> dom.
  The test `the_move_check_uses_the_widget_rule_of_the_add_command` (:1278) only uses
  `ShellPaletteCommandVecSlice`, which matches no keyword - so it passes.
- Twin that caused it: the "is a Vec-family name" test is written three times in module_map.rs -
  `determine_module` :756-760, `widget_module_for` :859-863, `get_correct_module_with_path` :895-900 - and only
  the third lists `vecslice`. One `fn vec_family(lower: &str) -> bool` (and one `is_structural`) used by all
  three removes the divergence; `doc/src/autofix/workspace.rs:936-960` (`is_vec_type`, `is_option_type`) and
  `doc/src/codegen/v2/lang_cpp/common.rs:365,391` carry further copies of the same suffix rules.
- Proposal (S): in `get_correct_module_with_path` run the widget rule before the structural shortcut (or drop
  `vecslice` from `is_structural`); add `CellGridRangeVecSlice` / `ToDoTaskVecSlice` to that test; then
  `azul-doc autofix` moves the 14 into `widgets`. Python/C users today import `from azul.css import
  CellGridRangeVecSlice`.

### F18. One Rust module, five api.json modules: the document-edit (changeset) types
- Evidence: all from `azul_layout::managers::changeset`: `misc.{DocOpInsertChildren, DocOpRemoveChildren,
  DocOpReplaceChildren, EditResumePoint}`, `dom.{DocOpMergeNodes, DocOpSplitNode}`, `css.{DocOpWrapRange,
  DocOpUnwrapRange, DocumentOperation, NodePosition}`, `callbacks.DocumentChangeset`; plus `error.
  DocumentEditError`, `dom.DomSplit` (layout::document_edit), `css.{DocumentPosition, DocumentTextEdit}` and
  `dom.DocumentSelectionSpan` (core::selection), `css.DocumentTextEditVecSlice` vs `dom.
  DocumentSelectionSpanVecSlice`. Pre-range placement, but the range's editor apps import them from all five:
  `examples/azul-mail/src/editor.rs:20-24` (`callbacks::DocumentChangeset`, `css::{BoxOrStaticString,
  DocOpWrapRange, DocumentOperation, NodePosition}`, `misc::EditResumePoint`), AzWriter likewise + `dom::
  {DocOpMergeNodes, DocOpSplitNode, DomSplit}`.
- Proposal (S, autofix-only): a `document` (or `edit`) module in module_map (`difficult_type_module` prefix
  entries `DocOp*`, `Document*`, `NodePosition`, `EditResumePoint`, `DomSplit`), then re-run autofix. The other
  `misc` residents have obvious homes too: `Transient{Anchor,Dismiss,Dock,Tearoff}` -> `window` (with
  `TransientWindowConfig`), `MediaControl{Kind,Request}` / `PlaybackState` -> `audio` or a `media` module,
  `Permission{State,Quality}` / `Capability` -> `app`, `InstallKind` / `ReleaseInfo` -> `app` (updater),
  `PaginationSnapshot` -> `pdf` (with `PaginationInfo`). That empties `misc` (17 types).
- Also pre-range: `ConsumerFrame`, `FrameConsumer`, `FrameConsumerVecSlice` (azul_core::video) and `ZombieFrame`
  (azul_core::resources) sit in `widgets`; they belong in `video`.

### F19. General-purpose widgets filed as "shells" with a `Shell` prefix
- Evidence: `layout/src/widgets/shells/{command_palette, navigation_pane, settings_layout, settings_dialog,
  empty_state, theme_scope}.rs` export `ShellCommandPalette`, `ShellNavigationPane`, `ShellSettingsLayout`,
  `ShellSettingsDialog`, `ShellEmptyState`, `ShellThemeScope` (+ `ShellPaletteCommand`, `ShellSetting*`,
  `ShellNavigation*`), all in api.json `shells` because `widget_module_for` files anything under
  `widgets::shells::` there (module_map.rs:870). They are not shells (layouts of panes): `ShellEmptyState` is
  used inside lists (`examples/azul-tasks/src/list.rs:363-385`, `examples/azul-videocut/src/lib.rs:1165-1180`),
  `ShellCommandPalette` / `ShellSettingsLayout` inside Backstages and overlays. Meanwhile the real shells are
  suffix-named (`OfficeShell`, `MobileShell`, `CallShell`, ...) and OfficeShell's callbacks are prefix-named
  `ShellOnPaneFocus` / `ShellOnPaneResize` (not `OfficeShellOnPaneFocus`).
- Proposal (M, unreleased API so cheap now): move the six files to `layout/src/widgets/` and drop the prefix:
  `CommandPalette`, `PaletteCommand`, `NavigationPane`, `SettingsLayout`, `SettingsDialog`, `EmptyState`,
  `ThemeScope`, `ThemeAccent`; rename `ShellOnPaneFocus` -> `OfficeShellOnPaneFocus`. Apps change only `use`
  lines (azul::shells -> azul::widgets). Do it together with F2 (NavigationPane absorbs ModuleSwitcher).
- Risk: churn across 10+ apps' imports; do it in one autofix pass.

### F20. `MessageList` has become a generic summary list under a mail name
- Evidence: `layout/src/widgets/message_list.rs:256-266` `MessageListMark::{Flag, Pin, None}` ("Pin: a notes
  list"), `MessageRow {id, from, subject, preview, date, icon, kind, unread, flagged, has_attachment, selected}`
  (:297-320). `examples/azul-notes/src/ui.rs:582-606` builds its note list from it: `MessageRow::create(id,
  note.display_title(), preview)` (title in `from`, preview in `subject`), `flagged` = pinned,
  `with_mark(MessageListMark::Pin)`, `with_search_placeholder("Search notes")`. AzContacts / AzTasks hand-build
  their own lists (no list widget in either: `grep ::create` shows none) though both need search + sort +
  group headers + virtualisation, which MessageList already has.
- Proposal (M): rename to a neutral `SummaryList` / `SummaryRow { id, title, subtitle, preview, date, icon, kind,
  emphasized, marked, has_attachment, selected }`, `SummaryListMark`; AzMail keeps its semantics, AzNotes stops
  mis-labelling fields, AzContacts / AzTasks can adopt it. Combine with F21 (one `on_event`) and F6 (selection).

### F21. Three in-range widgets register the SAME callback type through N per-kind setters
- Evidence: `MessageList` has 8 setters (`with_on_select/open/flag/delete/sort/search/scope/scroll`) that all take
  `MessageListOnEventCallback` (the event carries `MessageListEventKind`); `ReadingPane` 3
  (`on_link/attachment/load_images`, all `ReadingPaneOnEventCallback`); `ToDoBar` 3 (`on_pick/task/appointment`,
  all `ToDoBarOnEventCallback`). Every caller passes the same fn to each: `examples/azul-mail/src/
  ui_main.rs:953-960` (8x `on_list_event`), `:1135-1137` (3x `on_reading_event`), `examples/azul-notes/src/
  ui.rs:601-606` (6x), `examples/azul-tasks/src/chrome.rs:513-515`, `examples/azul-mail/src/ui_main.rs:682-683`,
  `examples/azul-calendar/src/chrome.rs:387-389`, `examples/azul-widgets/src/mail.rs:181-183`. The other 12 new
  event-style widgets (AddressBar, Timeline, ThumbnailStrip, CellGrid, SelectionAdorner, WizardLayout,
  ShortcutRecorder, MessageBox & co, ShellNavigationPane, ShellSettingsDialog) use ONE `with_on_event`.
- Proposal (S): add `with_on_event` / `set_on_event` to the three (sets every slot), keep the per-kind setters
  as filters (a per-kind setter still lets an app ignore e.g. Scroll); migrate the callers. ModuleSwitcher's
  `on_select(usize)` / `on_collapse(bool)` pair is the opposite inconsistency (F2).

### F22. Smaller naming inconsistencies across the widget API
- Kind setters: `Alert::set_kind` / `with_alert_kind`, `Badge::set_kind` / `with_badge_kind`, `Chip` /
  `with_chip_kind`, `Toast` / `with_toast_kind`, `Spinner::set_size` / `with_spinner_size` (pre-range), while the
  in-range `InfoBar::with_kind(AlertKind)` uses the plain name - pick `with_kind` everywhere (S).
- One severity concept, several enums: `AlertKind` == `ToastKind` {Info, Success, Warning, Danger} and
  `BadgeKind` == `ChipKind` {Default, Primary, Success, Danger, Warning, Info} (identical variant lists,
  `alert.rs:94`, `toast.rs:119`, `badge.rs:36`, `chip.rs:113`); the in-range `MessageBoxKind` {Info, Warning,
  Error, Question} (`standard_dialogs.rs:426`) says `Error` where the others say `Danger`. InfoBar already reuses
  `AlertKind` (good). Proposal: `ToastKind` -> alias of `AlertKind`, `ChipKind` -> `BadgeKind` (M, breaking).
- Event type names: `ShellSettingsDialog` reports `ShellSettingsEvent` / `ShellSettingsEventKind` (not
  `...DialogEvent`), `WizardLayout` reports `WizardEvent` via `WizardOnEvent` (the widget is `WizardLayout`),
  the pre-range node graph / video / map callbacks are `OnNodeAddedCallbackType`, `OnVideoStatusCallbackType`,
  `MapViewportChangedCallbackType` (18 of 92 widget callback types do not follow `<Widget>On<Event>Callback`).
- Two search rules: `palette_matches` (subsequence, `shells/command_palette.rs:227`) vs `dialog_kit::
  find_ignore_case` / `contains_ignore_case` (substring, `dialog_kit.rs:380-409`); `ShellSettingsLayout` mixes
  both in one search (section titles by subsequence, keywords by substring, `settings_layout.rs:~660-666`) while
  `ShellSettingsDialog` uses substring only (`settings_dialog.rs:388-402`). Pick one per purpose and say so in
  the docs (S).
- create vs new: api.json says `create` for every widget (145 ctors), but 30 pre-range Rust constructors are
  still `new` and api.json maps them (`TreeView::create` -> fn_body `TreeView::new(root)`; ribbon.rs x9,
  statusbar.rs x3, backstage.rs x2, tooltip, titlebar, drop_down, combobox, tabs). All in-range widgets use
  `create` in Rust too - rename the 30 (keep `new` as `#[deprecated]` alias for one release) so Rust users of
  azul-layout and binding users see one name (S, mechanical + autofix re-scan of fn_body).
- Item types with `with_*` but no `set_*` (C / Python callers cannot mutate in place): `MessageRow` (7),
  `ThumbnailItem` (5), `TimelineClip` (5), `TimelineTrack` (5), `WizardComponent` (4), `ShellSetting` (4),
  `ToDoTask` (2) - all in range; the house style elsewhere pairs them (S, mechanical).

## Kind 5 - OTHER (tests loosened, dead weight, deps)

### F23. Loosened table test: `prose_cells_wrap_inside_a_220px_table` relaxed from `>= 3` to `>= 2` lines
- Evidence: `layout/tests/a_narrow_table_wraps_its_cells_to_fit.rs:113-126`, commit `407cc8c98` (2026-10-02,
  "two lines per cell is where Chrome wraps the 220px prose table"): `lines.len() >= 3` -> `lines.len() >= 2`.
  The commit body argues the layout is right (Times 16px: columns ~120.5 / ~93.5px of the 214px, each cell on
  two lines, baselines [18, 36] = Chrome).
- What is wrong with `>= 2`: `lines` is the deduplicated set of baselines over BOTH cells, so it passes as soon
  as ONE cell wraps. A wrong distribution - cell B left at its max-content (155.7px, one line) and cell A squeezed
  into the remaining ~58px (three or more lines) - keeps the table at 220px, keeps every pen inside it, and
  passes; so does A wrapped / B unwrapped at the same baseline. The test no longer checks the column
  distribution (CSS Tables 3 3.9.3) that the commit body says is now Chrome's, nor that BOTH cells wrap.
- The right assertion (the claimed Chrome result, pinned per cell): give the cells ids (`<td id="a">`,
  `<td id="b">`), partition the glyph pens by the cells' boxes (`get_node_position` / `get_node_size`), and
  assert (1) EACH cell's pens sit on exactly two distinct baselines and the two cells share them
  (`a_lines == b_lines`, i.e. `[18, 36]`), (2) the cell widths match the CSS Tables 3 3.9.3 distribution:
  `width_i ~= min_i + (214 - sum(min)) * (max_i - min_i) / sum(max - min)` within 1.5px - with min / max
  measured in the same test from one-cell tables (`width: min-content` / `max-content`), which keeps it
  font-independent - and (3) no pen of cell A lies right of cell B's left edge. If the face really varies
  per machine, assert "each cell >= 2 lines" per cell rather than the union.

### F24. Second test sidestepped instead of a RED test for the bug it found
- Evidence: commit `56b105f60` (2026-10-02, TABLE-A) rewrote `an_auto_table_of_long_content_is_capped_by_its_
  container` and `max_width_caps_an_auto_table` to measure prose (`table_markup::prose`) because space-separated
  inline-blocks report min-content = SUM of the boxes (an inline intrinsic-sizing bug, "E-INLINE",
  scripts/TABLE_A_2026_10_01.md:284-290). Fine for the table tests, but I found no RED test that pins E-INLINE
  itself (`grep -rln inline-block layout/tests` + min-content: none targets it). Next wave: add the probe the
  report describes (`<div style="display:inline-block"><i ib 50/> <i ib 50/></div>` in a 60px container ->
  wraps, width 50) as a RED test so the bug is not lost.

### F25. Widget unit tests: 15 copies of `texts()`, 13 of `id()`, 37 `text_of` helpers
- Evidence: `fn texts(node: &Dom, out: &mut Vec<String>)` in shortcut_recorder, cell_grid, tree_view, todo_bar,
  message_list, standard_dialogs, address_bar, thumbnail_strip, reading_pane, module_switcher (:637), info_bar,
  path_input, wizard_pages, wizard_layout, shells/settings_dialog (DIVERGED: module_switcher skips empty strings,
  todo_bar :863 keeps them); `fn id(n: NodeId) -> DomNodeId` in 13 files. The shared test-support module
  `layout/src/widgets/themes/theme_checks.rs` (nodes, has_class, find, find_all, focusable, a11y_outline ...)
  has neither; `roving::test_support` has the keyboard driver. MAILWIDGETS (scripts/MAILWIDGETS_2026_09_30.md:
  307-309) already flagged it.
- Proposal (S): `theme_checks::{texts(dom) -> Vec<String> (non-empty), dom_node_id(n)}`; delete ~28 copies.

### F26. Two `micromail` versions in Cargo.lock
- Evidence: `Cargo.lock:5594` micromail 0.1.0 (from `layout/Cargo.toml:143`, feature `crash-mail`,
  `layout/src/telemetry/crash_mail.rs`) and `Cargo.lock:5609` micromail 0.2.0 (from
  `examples/azul-mail/Cargo.toml:42`). The range's commit dfa3e14b8 moved AzMail to 0.2.0 from crates.io but
  left layout on 0.1.
- Proposal (S): bump `layout/Cargo.toml` to `micromail = "0.2"` (check the API delta for the SMTP send in
  crash_mail.rs), one copy in the tree / one vet entry.
- Note: the brief's `dll/src/desktop/extra/{mail,storage}` do not exist - mail is micromail (crate) +
  examples/azul-mail, storage is the app crate `examples/azul-storage` (used by 9 apps via path deps). The dll
  side of the range (video_codec container / demux / VideoToolbox) had its AVCC / Annex-B twins already unified
  (container.rs:45-116 `annexb_nals`, `append_avcc_as_annexb`, `annexb_to_avcc`; videotoolbox.rs:645 calls
  the shared fn; container.rs:213 reuses `demux::demux_mp4_h264`) - verified, nothing to add.

## More widget-vs-widget findings (appended after the first pass)

### F27. `PathInput`'s Browse re-implements `FileInput`'s picker request / resume
- Evidence: `layout/src/widgets/path_input.rs:264-311` (`on_browse`: read title + start dir from the RefAny,
  `ResumeCallback::create(on_picked)`, `FileDialog::open_directory` / `open_file(.., OptionFileTypeList::None,
  ..)`; `on_picked`: `FileOpenResult::downcast(result)` -> `picked.path.into_option()` -> report) vs
  `layout/src/widgets/file_input.rs:392-470` (`fileinput_on_click`: same read, `FileDialog::open_file` /
  `open_multiple_files`, `fileinput_on_file_picked`: the same downcast / into_option chain). The doc of
  path_input says so: "the answer arrives in [`on_picked`] as a fresh activation (FileInput's shape)".
  DIVERGED: PathInput can pick a DIRECTORY, FileInput cannot; FileInput has `accept` filters and `multiple`,
  PathInput neither.
- Proposal (S-M): give `FileInput` a `directory: bool` (`with_directory`) and build PathInput as `TextInput` +
  `FileInput` (label "Browse...") with the FileInput's `on_path_change` forwarded to `PathInput::on_change`.
  Removes the second picker path (~50 lines) and gives PathInput filters for free (a "Save to" file).

### F28. Two mechanisms for "follow the app theme", and the look-merge one is copied per widget
- Evidence: 35 widget files use `theme_blocks::follow_app_theme(self, flat::x, flora::x)` (build twice, merge
  DOMs - `themes/theme_blocks.rs:436`); 16 merge two LOOK structs field by field with `follow_props` and build
  once. The look-merge is hand-written each time: `shells/mod.rs:250-326` (`follow_look` + `macro_rules!
  merged` + `look_for` :332), `dialog_kit.rs:136-202` (the same `follow_look` + `merged!` macro + `look_for`,
  identical shape, other field list), `frame.rs:461-477`, `thumbnail_strip.rs:555-575`,
  `selection_adorner.rs:1892-1915` (each: `let (a, b) = (flat::x_look(), flora::x_look()); let both = |x, y|
  follow_props(x, y).into_library_owned_vec(); XLook { f: both(..), .., marker: match structure {..} }`).
- Proposal (S): one `impl_theme_look!(XLook, flat::x_look, flora::x_look, [fields..])` macro in
  `themes/theme_blocks.rs` that emits `look_for(OptionUiTheme) -> XLook` and `follow_look(UiTheme) -> XLook`;
  the five call sites shrink to one line each, and new widgets stop copying it. (Which of the two mechanisms a
  widget should use is documented in thumbnail_strip / selection_adorner - "the content is built once" - keep
  both, but say so in `themes/mod.rs`.)

### F29. The S-shells re-declare OfficeShell's chrome: ~98 identical setters over 10 files
- Evidence: every preset shell in `layout/src/widgets/shells/` carries its own `title_row / ribbon / backstage /
  status_bar / on_pane_focus / on_pane_resize / theme` fields with set_/with_ pairs, then converts itself with
  `office_shell(self) -> OfficeShell`: chrome setters per file - browser 14, document 14, pim 14, canvas 10,
  developer 10, records 10, media 8, timeline 8, call 6, utility 4 (98 methods, ~7 lines each). The function-body
  hash over `layout/src/widgets/**` finds `set_on_pane_focus` and `set_on_pane_resize` byte-identical in 9 files
  (developer_shell.rs:157/173, timeline_shell.rs:142/158, browser_shell.rs:246/262, pim_shell.rs:195/211,
  call_shell.rs:223/239, records_shell.rs:157/173, document_shell.rs:191/207, canvas_shell.rs:231/247,
  media_shell.rs:115/131). Apps already chain the chrome on the converted shell
  (`examples/azul-videocut/src/lib.rs:1186-1197`: `TimelineShell::create(..).with_menu_bar(..).office_shell()
  .with_status_bar(..)`), so the per-preset copies are redundant.
- Proposal (M): presets keep only their panes + layout knobs (ratios, which panes) and `office_shell()`; the
  chrome lives on `OfficeShell` alone (callers write `.office_shell().with_ribbon(..)`), or - if the preset
  builders must keep chrome for Python ergonomics - one `ShellChrome { title_row, ribbon, backstage,
  status_bar, on_pane_focus, on_pane_resize, theme }` struct field + one `with_chrome(ShellChrome)`. ~650 lines
  and ~98 api.json methods fewer.
- Risk: apps calling e.g. `BrowserShell::with_ribbon` before `office_shell()` (AzDrive?) need the reorder.

### F30. No Toolbar widget: four apps hand-build one, none with the APG toolbar keyboard
- Evidence: `examples/azul-contacts/src/ui.rs:1154-1169` (row of `Button::create(label).with_icon(..)` + spacer),
  `examples/azul-notes/src/ui.rs:849-900` (format toolbar: pressed state via `ButtonType::Primary`, hand-made
  1px separators, `with_accessibility_name("Formatting")`), `examples/azul-review/src/ui.rs:237-275` (swatch
  bar, raw divs + MouseUp callbacks), `examples/azul-widgets/src/lib.rs:534`. The shells only expose a
  `toolbar_row` LOOK part (`shells/mod.rs:217`), no widget. `roving.rs` already implements the one-Tab-stop
  arrow-key pattern used by tabs / segmented / module switcher.
- Proposal (S-M): `layout/src/widgets/toolbar.rs`: `Toolbar::create(name)`, `ToolbarItem::{button(label, icon),
  toggle(label, icon, pressed), separator(), spacer()}`, one `on_event(ToolbarEvent{index})` hook, roving
  focus (APG toolbar: one Tab stop, Left/Right/Home/End), flat/flora looks. Adopters: AzContacts, AzNotes,
  AzReview, the showcase; RibbonButton's `toggled` look is the reference for the pressed state.

### F31. Two character-reference decoders in core
- Evidence: `core/src/xml_html.rs:182` `decode_character_references(s, CharRefMode::{Xml, HtmlText,
  HtmlAttribute})` (new in range; `layout/src/xml/mod.rs:30` routes its loader through it) and
  `core/src/xml.rs:7280` `decode_entities` (pre-range, hand-rolled single pass, patched IN the range to look up
  `html::named_character_reference` for multi-char names, :7253 `html_named_entity` wraps the same table).
  DIVERGED: `decode_entities` deliberately keeps `&nbsp;` verbatim for `prepare_string`'s per-line pass and
  only knows `;`-terminated names (no legacy `&copy` handling, no Windows-1252 numeric repair, `char::from_u32`
  instead of U+FFFD for invalid numbers).
- Proposal (S): `decode_entities(s)` = `decode_character_references(s, CharRefMode::HtmlText)` with `&nbsp;`
  protected (a `keep_nbsp` flag on the mode or a pre/post substitution), delete `decode_numeric_entity` /
  `html_named_entity`; one decoder, one set of tests (xml_test.rs:2117-2160 move onto it). Pair with F5's
  encoder in the same file.

### F32. Hex colour formatting / parsing re-made in widgets and apps; `ColorU::to_hash` is the wrong shape
- Evidence: exported `ColorU::to_hash()` (css/src/props/basic/color.rs:889) always writes `#rrggbbaa`; nobody uses
  it. Re-implementations: `layout/src/widgets/shells/theme_scope.rs:141 hex` (in range; `#RRGGBB`, `#RRGGBBAA`
  when translucent, upper case), `examples/azul-show/src/model.rs:54 hex` (same rule, lower case),
  `examples/azul-photo/src/ui.rs:83`, `examples/azul-widgets/src/forms.rs:350`, `examples/azul-writer/src/
  palette.rs:228` (always `#rrggbb`, alpha dropped). Parsing: `layout/src/widgets/color_input.rs:599
  color_from_hex` (pre-range; 3/4/6/8 digits) next to css's own `parse_css_color` / `parse_color_no_hash`
  (color.rs:2000/2071, exported as `ColorU::from_str`), and `examples/azul-sheets/src/model.rs:184 parse_hex`
  (6 digits only).
- Proposal (S): `ColorU::to_css_hex() -> AzString` (shortest CSS form: `#rrggbb`, `#rrggbbaa` when a < 255) in
  css, exported; theme_scope / AzShow / AzPhoto / AzWriter / showcase call it; `color_from_hex` becomes
  `parse_color_no_hash` behind a `#`-optional wrapper (or is deleted in favour of `ColorU::from_str`), AzSheets
  uses `ColorU::from_str`.

### F33. Three message-strip widgets: `Alert`, `Toast` (pre-range near-clones) and the in-range `InfoBar`
- Evidence: `toast.rs` doc: "A near-clone of Alert (a coloured message box with a 'x' dismiss affordance and a
  `visible` state) that, instead of sitting inline, floats"; `AlertKind` == `ToastKind`; the hash scan finds
  their test helpers `border_colors` (alert.rs:808 / toast.rs:867) and `dismiss_with_a_foreign_payload_is_a_noop`
  (alert.rs:1870 / toast.rs:2293) identical. The range adds `InfoBar` (`info_bar.rs`, 505 lines: glyph + text +
  one action link, reusing `AlertKind`). api: `Alert {create, with_kind (a CTOR), set_kind, with_alert_kind,
  dismissible, on_dismiss}`, `Toast` the same with `with_toast_kind`, `InfoBar {icon, action, kind (method
  `with_kind`), on_action}` - the ctor named `with_kind` is why Alert/Toast need `with_alert_kind`.
- Proposal (M, low priority): one `Alert` with `with_icon`, `with_action(label) + on_action`,
  `with_dismissible`, and `AlertPlacement::{Inline, Strip, Toast}`; `InfoBar` / `Toast` become thin
  constructors (`Alert::strip(..)`, `Alert::toast(..)`); rename the `with_kind` ctor to `create_with_kind` so
  the method can be `with_kind` everywhere.

---

## PRIORITIZED NEXT-WAVE TASK LIST (most value per effort first)

Wave A - small, mechanical, unblock or de-risk everything after them
1. **F1** `$crate::impl_option_inner!` / `$crate::impl_result_inner!` in css/src/macros.rs (6 call sites), fully
   qualify `RefAny` in `impl_widget_callback!`, then drop the hand imports from 77 (+9 result) files. (S; leave
   `layout/src/solver3/page_breaks.rs` - dirty in the other session's tree - for last.)
2. **F17** module_map.rs: one `vec_family()` rule, widget rule before the structural shortcut, extend the test
   with a keyword-matching VecSlice; re-run autofix -> 14 `*VecSlice` types move to `widgets`. (S)
3. **F23** restore a real assertion in `prose_cells_wrap_inside_a_220px_table` (per-cell two baselines, shared
   baselines, 3.9.3 column widths). (S) + **F24** a RED test for E-INLINE. (S)
4. **F12** export `ShellThemeAccentColors` + `ShellThemeAccent::colors` / `name_string` / `from_name`;
   AzShow's `STONES` reads them. (S)
5. **F13** replace the 3 `reborrow_info` copies with `*info` (AzWriter / AzNotes / AzSheets); optionally make
   the 5 by-value `CallbackInfo` args references in api.json. (S)
6. **F14** export `NodeType::get_text() -> OptionString` and `NodeData::get_attributes()`; delete the 3
   unsafe `box_str` copies and AzMail's href-in-a-class workaround. (S)
7. **F15** `ButtonOnClick::create(data, cb)` (emit it from `impl_widget_callback!` for every wrapper) and
   `StatusBarZoom::with_on_zoom_in/out`; delete the hand-built structs in AzSheets / AzShow / AzWriter. (S)
8. **F9 (S part)** export `KeyboardState::{primary_down, shift_down, alt_down, is_key_down}` +
   `VirtualKeyCodeCombo::matches`; the 13 widget sites use `primary_down()`, the 3 `modifiers()` copies go;
   apps switch from `ctrl || meta` as they are touched. (S)
9. **F3** `CoreCallbackData::create(event, refany, cb)` in core; delete 9 (+ message_list's `click`) `hook()`
   copies. (S)
10. **F21** `with_on_event` on MessageList / ReadingPane / ToDoBar; migrate AzMail, AzNotes, AzTasks,
    AzCalendar, showcase. (S)
11. **F10** `DiskSpace::format_bytes` (move from tile.rs, export); AzDrive / AzMail / AzTasks call it. (S)
12. **F5 + F31** `encode_text` / `encode_attribute` next to `decode_character_references`; `decode_entities`
    becomes a mode of the new decoder. (S)
13. **F26** layout's micromail 0.1 -> 0.2 (one version in Cargo.lock). (S)
14. **F25** `texts()` / `dom_node_id()` into `themes::theme_checks`; delete ~28 test-helper copies. (S)
15. **F28** `impl_theme_look!` macro; shells / dialog_kit / frame / thumbnail_strip / selection_adorner use it. (S)
16. **F18** autofix-only: a `document` module for the changeset types, empty `misc` (Transient* -> window,
    Media* -> audio/media, Permission*/Capability/InstallKind/ReleaseInfo -> app, PaginationSnapshot -> pdf),
    video frame types out of `widgets`. (S)
17. **F32** `ColorU::to_css_hex()`; theme_scope + 4 apps; `color_from_hex` -> css parser. (S)
18. **F7** `widgets/virtual_rows.rs` (+ an exported `visible_row_range` static). (S)

Wave B - medium refactors that remove whole parallel implementations
19. **F2** NavigationPane's footer = ModuleSwitcher (badge added, one wrap rule); drop ShellNavigationModule. (M)
20. **F6** `ListSelection` model (port AzDrive's `Selection` + tests), export; AzDrive / AzTasks / AzShow /
    AzContacts adopt; ThumbnailStrip / MessageList can apply it. (M)
21. **F29** S-shell presets stop re-declaring OfficeShell's chrome (~98 setters). (M)
22. **F19 + F20** de-prefix the general widgets out of `shells` (CommandPalette, NavigationPane, SettingsLayout,
    SettingsDialog, EmptyState, ThemeScope) and rename MessageList -> SummaryList; one autofix pass, apps change
    `use` lines. (M)
23. **F4** merge `themes/style_kit.rs` into `themes/decl.rs` (rename `style_kit::fill()` -> `fill_box()`), add
    the const single-property layout helpers, rebuild shells / dialog_kit bases from them. (M)
24. **F9 (M part)** `CommandTable` (palette, menus, settings shortcut rows and window key dispatch from one
    table); appkit's `Shortcut` folds into it. (M)
25. **F27** PathInput = TextInput + FileInput (FileInput gains `directory`). (S-M)
26. **F30** `Toolbar` widget (APG toolbar keyboard); AzContacts / AzNotes / AzReview / showcase adopt. (S-M)

Wave C - polish
27. **F22** naming: `with_kind` everywhere (rename the Alert/Toast `with_kind` ctors), `ToastKind` ->
    `AlertKind`, `ChipKind` -> `BadgeKind`, `MessageBoxKind::Error` vs `Danger`, `ShellSettingsDialogEvent`,
    `WizardLayoutEvent`, `OfficeShellOnPaneFocus`, set_/with_ pairs on the in-range item types, Rust `new` ->
    `create` for the 30 older widgets, one documented search rule. (S each)
28. **F33** Alert / Toast / InfoBar -> one Alert with placement + action. (M)
29. **F11** relative-date formatting on `IcuLocalizerHandle`; DatePicker's month/weekday names from ICU. (M)
30. **F8** `PropertyList` / `LabelledField`. (S-M)
31. **F16** remaining statics for free helpers (`fit_within`, `palette_matches`, `AdornerHandle::anchor` as a
    struct). (S)

## SEEN BUT NOT VERIFIED (or out of this area)

- `dll/src/unified/pdf.rs` (the wasm stub of `Pdf`) has no `to_svg_pages` although api.json's `Pdf.to_svg_pages`
  calls it (pre-range, 82171735d); harmless unless the wasm build compiles the C-ABI wrappers - not built here.
- `impl_widget_callback!` + `impl_managed_callback!` are two separate invocations per callback (96 + 93 in
  layout/src/widgets, 28 new in the range, ~15 lines each, every invoker / thunk / setter name spelled out by
  hand); one combined macro (or codegen-emitted invoker names) would cut ~1,000 lines - not designed here.
- Pre-range twins the hash scan found (not in the range, listed for completeness): `push_box_border`
  (statusbar.rs:415 / quick_access.rs:289), `push_row_center` (statusbar.rs:375 / quick_access.rs:251),
  `merged_style` (ribbon, backstage, statusbar, quick_access), 26 copies of the test helper `layout_result`,
  18 of `text_of`, 12 of `classes` in pre-range widget tests.
- Apps that build their own "Appearance" settings section (7 apps: meet, sheets (as buttons), shells, tasks,
  show, calendar, photo - besides appkit's own) although `azul-appkit::ui::appearance_section` exists - the appkit is used by
  only 2 apps (calculator, contacts); this belongs to the APPS dedup, noted because a stock
  `ShellSettingsSection::appearance(theme, mode, accent, cb)` would be the widget-side fix.
- AzSheets' Find panel (`examples/azul-sheets/src/lib.rs:1758-1767`) vs the exported `FindReplaceDialog` -
  not compared in detail.
- `layout/src/managers/selection.rs:274-279` HTML clipboard flavour escapes `& < >` but not `"` - safe for text
  nodes as written; not tested here.
- The two "follow the app theme" mechanisms (F28) - whether every widget picked the right one was not checked.
- dll `headless/mod.rs` (+2234 / -713 in range) was not reviewed for duplication with `layout/src/headless.rs`
  (ENGINE / PLATFORM area).
