# APIEXPORT - wave 5 report (2026-10-02)

Branch `wt/apiexport` from `2e92c759b`. Nothing compiled (house rules); every RED test that names a new
method does not compile until its GREEN commit. The api.json entries are NOT added - they are listed below in
`azul-doc autofix add` terms for the parent. Until autofix runs, the APPS do not compile (they call the new
exports through the generated `azul` crate); core / css / layout / their tests do.

## What was built (by brief item, with the review finding ids)

1. **Primary modifier** (DEDUP_WIDGETS_API F9, DEDUP_OFFICE D14 / A5)
   - core: `KeyModifiers::primary_down()` (+ Rust-only `primary_down_for(mac)`); `KeyboardState::primary_down`
     now reads the same rule (`derived_modifiers().primary_down()`); docs on the KeyboardState modifier getters.
   - widgets: 13 `ctrl_down() || super_down()` sites -> `primary_down()` (cell_grid x2, message_list x2,
     selection_adorner x2, thumbnail_strip x2, timeline x2, slider, split_pane); the three byte-identical
     `modifiers(info)` helpers are gone. Left on purpose: combobox / roving / office_shell / shortcut_recorder
     ("ANY modifier held"), color_input (already convention-aware).
   - apps: 25 `m.ctrl || m.meta` sites in 21 files -> `m.primary_down()`.
   - E2E: the app scripts pressed Ctrl for shortcuts on every host; each key helper takes `primary=True`
     (Cmd on darwin, Ctrl elsewhere): azlin_e2e (shared), calendar, drive (+ its Ctrl+click), notes, photo,
     sheets, show, videocut, shells.
   - No CallbackInfo convenience: `info.get_key_modifiers().primary_down()` is one call (decision).
2. **ShellThemeAccent::colors** (DEDUP_OFFICE D1 / A1, DEDUP_WIDGETS_API F12): nothing to write in Rust (the
   method and struct exist); AzShow's 20-colour copy is gone - `themes::FAMILIES` pairs each deck name with its
   `ShellThemeAccent`, `stone(i)` / `paper()` read the shell's ramp, `accent(index)` replaced lib.rs's own match.
3. **Constructors / zoom range** (DEDUP_OFFICE D28 / A2 / A3 / A4 / D9, DEDUP_WIDGETS_API F15)
   - `impl_widget_callback!` emits `<Wrapper>::create(refany, callback)` for every widget hook (92).
   - `StatusBarZoom::create(percent, min, max)`, `set_range` / `with_range` (reversed pair ordered),
     `set_percent`, `set/with_on_zoom_out`, `set/with_on_zoom_in`, `set/with_on_slider_change`,
     `set/with_show_label`. RED first (`the_zoom_slider_spans_the_range_the_app_gives`).
   - AzWriter (10..190), AzShow (10..400, new `app::ZOOM_MIN/MAX`), AzSheets (10..400, new `ZOOM_MIN/MAX`)
     build their zoom with it; the `button_click` / `click` / `zoom_click` literals are gone.
   - Ribbon: `RibbonGroup/RibbonColumn/RibbonRow::add_items/with_items(RibbonItemVec)`,
     `RibbonTab::add_groups/with_groups(RibbonGroupVec)`; the folds in AzDrive / AzSheets / AzShow / AzTasks /
     AzWriter use them.
4. **The rest of the exports**
   - `ColorU::to_hex()`, `ColorU::parse_hex(text)`, `ColorU::parse_css(text)` (`None` on a typo where
     `from_str` answers black) - DEDUP_OFFICE D11 / A6 / S3, DEDUP_WIDGETS_API F32. The bodies moved from
     color_input (whose `color_to_hex` / `color_from_hex` now delegate, 20 callers keep the names);
     theme_scope's upper-case `hex()` twin is gone (accent custom properties are lower case now, test
     updated). Apps: AzShow `model::Color::hex/parse`, AzPhoto / AzWidgets / AzWriter `hex` (opaque output
     kept), AzSheets `model::parse_hex` deleted.
   - `RawImage::create_rgba8(w, h, pixels, premultiplied)`, `RawImage::resized(w, h)` (DEDUP_OFFICE D15 / A7):
     `image_scale::thumbnail` = `fit_within` + `image_scale::resized` (one body). Apps: AzPhoto x2, AzPaint x2,
     AzReview, AzWidgets, AzVideoCut use `create_rgba8`; AzVideoCut's nearest `scale_to` resamples through
     `resized`.
   - `DiskSpace::format_bytes(bytes)` (DEDUP_OFFICE D24 / A10, DEDUP_WIDGETS_API F10, DEDUP_EDITORS D8):
     moved from `tile.rs` to `layout/src/file.rs`; the 4 copies (AzDrive `format_size` keeps its Option
     wrapper, AzMail `human_size`, AzTasks `size_text`, AzPhoto's inline KB) call it. Explorer's rule.
   - `Button` disabled / toggled (DEDUP_OFFICE A11): fields `disabled_reason: AzString`, `toggled: OptionBool`;
     `set/with_disabled`, `is_disabled`, `set/with_toggled`. Disabled: click + form action dropped, both themes
     drop hover / pressed paint and dim (`button::disabled_style`), `mark_disabled` adds
     `BUTTON_DISABLED_CLASS`, Unavailable + the reason as description, tooltip on hover / click. Toggled:
     CheckedTrue / CheckedFalse, the theme's pressed face at rest (`flat::button_toggled_face`,
     `flora::button_toggled_face`, appended at the end of both theme files). RibbonButton's disabled machinery
     MOVED into button.rs (one implementation); RibbonButton hands its reason to the Button and adds only
     `RIBBON_DISABLED_CLASS`. AzCalculator's dimmed-div digit keys are disabled Buttons.
   - `NodeData::get_attribute(name)` / `get_attributes()` (DEDUP_EDITORS D4, DEDUP_WIDGETS_API F14):
     `CallbackInfo::get_node_attribute`'s 60-line match moved into core; it now delegates. AzMail keeps a
     link's address in the `href` attribute (the `azmail-href:` class and `LINK_CLASS_PREFIX` are gone; a
     pasted link keeps its address now).
   - `NodeType::get_text()` (DEDUP_EDITORS D3 / B9): the three `unsafe fn box_str` (AzWriter x4 sites,
     AzNotes x2, AzMail x1) are gone.
   - `TextAreaState::get_text` (existed, not exported; DEDUP_EDITORS D2 / B8): AzTasks `area_text`, AzCalendar,
     AzContacts, AzShow `chars_of`, AzWidgets `text_area_text` deleted.
   - `DatePickerWeekStart { Sunday, Monday }` + `DatePicker::week_start` / `set/with_week_start`
     (DEDUP_EDITORS D10 / B25), RED first; the private `WeekStart` became the public enum. AzCalendar's
     navigator is Monday-first (one call).
   - `GlobalHotkey::matches(&KeyboardState)` (DEDUP_EDITORS D11 / B18), RED first. No app adopted it yet
     (that is the CommandTable work, wave 6).
5. **reborrow_info** (DEDUP_OFFICE D8, DEDUP_WIDGETS_API F13): the three copies (AzWriter, AzNotes, AzSheets)
   are `*info`.

## api.json - for the parent's autofix (never edited by hand here)

Run `azul-doc autofix add <Type.method>` for each line (module in brackets is where the class lives / should
land). Rust signatures are idiomatic; autofix's rules turn `&str` into `String` + `.as_str()`, `Option<T>`
returns into `OptionT` + `.into()`, `String` returns into `String` + `.into()`, `&T` args into a re-borrowed
pointer, and `C: Into<XCallback>` widget-callback args into `XCallback` (the generated binding takes the
`XCallbackType` fn pointer, like `Button.with_on_click`).

| Add | Module | Rust signature (source) |
|---|---|---|
| `KeyModifiers.primary_down` | dom | `fn primary_down(&self) -> bool` (core/src/events.rs) |
| `KeyboardState.primary_down`, `.shift_down`, `.ctrl_down`, `.alt_down`, `.super_down` | dom | `fn x(&self) -> bool` (core/src/window.rs) |
| `KeyboardState.is_key_down` | dom | `fn is_key_down(&self, key: VirtualKeyCode) -> bool` |
| `ShellThemeAccent.colors` (pulls in the struct `ShellThemeAccentColors`: accent, deep, soft, glow, on_accent: ColorU, repr C) | shells | `const fn colors(self, dark: bool) -> ShellThemeAccentColors` (theme_scope.rs) |
| `ButtonOnClick.create` (constructor) | widgets | `fn create<I: Into<ButtonOnClickCallback>>(refany: RefAny, callback: I) -> ButtonOnClick` (macro, widgets/mod.rs) |
| `SliderOnValueChange.create` (constructor) | widgets | same shape, `SliderOnValueChangeCallback` |
| `StatusBarZoom.create` (constructor) | widgets | `fn create(percent: f32, min: f32, max: f32) -> Self` (statusbar.rs) |
| `StatusBarZoom.set_percent`, `.set_range`, `.with_range` | widgets | `(&mut self, percent: f32)`, `(&mut self, min: f32, max: f32)`, `(self, min, max) -> Self` |
| `StatusBarZoom.set_on_zoom_out`, `.with_on_zoom_out`, `.set_on_zoom_in`, `.with_on_zoom_in` | widgets | `(data: RefAny, callback: C: Into<ButtonOnClickCallback>)` |
| `StatusBarZoom.set_on_slider_change`, `.with_on_slider_change` | widgets | `(data: RefAny, callback: C: Into<SliderOnValueChangeCallback>)` |
| `StatusBarZoom.set_show_label`, `.with_show_label` | widgets | `(show_label: bool)` |
| `RibbonGroup.add_items`, `.with_items`; `RibbonColumn.add_items`, `.with_items`; `RibbonRow.add_items`, `.with_items` | widgets | `(items: RibbonItemVec)` (ribbon.rs) |
| `RibbonTab.add_groups`, `.with_groups` | widgets | `(groups: RibbonGroupVec)` |
| `ColorU.to_hex` | css | `fn to_hex(&self) -> String` (css/src/props/basic/color.rs) |
| `ColorU.parse_hex`, `ColorU.parse_css` (statics returning `OptionColorU`) | css | `fn parse_hex(text: &str) -> Option<ColorU>`; `parse_css` behind the `parser` feature |
| `RawImage.create_rgba8` (constructor) | image | `fn create_rgba8(width: u32, height: u32, pixels: U8Vec, premultiplied_alpha: bool) -> Self` (core/src/resources.rs) |
| `RawImage.resized` | image | `fn resized(&self, width: u32, height: u32) -> Option<RawImage>` |
| `DiskSpace.format_bytes` (static) | file | `fn format_bytes(bytes: u64) -> String` (layout/src/file.rs) |
| `Button.set_disabled`, `.with_disabled`, `.is_disabled`, `.set_toggled`, `.with_toggled` | widgets | `(reason: AzString)`, `-> bool`, `(toggled: bool)` (button.rs) |
| `Button` struct_fields refresh | widgets | NEW fields `disabled_reason: AzString` (after `on_click`) and `toggled: OptionBool` (last) - the dll's ABI size checks fail until api.json has them |
| `NodeData.get_attribute`, `NodeData.get_attributes` | dom | `fn get_attribute(&self, name: &str) -> Option<AzString>`; `fn get_attributes(&self) -> AttributeTypeVec` (core/src/dom.rs) |
| `NodeType.get_text` | dom | `fn get_text(&self) -> Option<AzString>` |
| `TextAreaState.get_text` | widgets | `fn get_text(&self) -> String` (text_area.rs, existed) |
| `DatePickerWeekStart` (enum Sunday, Monday; repr C) | widgets | date_picker.rs |
| `DatePicker` struct_fields refresh + `DatePicker.set_week_start`, `.with_week_start` | widgets | NEW field `week_start: DatePickerWeekStart` (after `mode`); `(week_start: DatePickerWeekStart)` |
| `GlobalHotkey.matches` | app | `fn matches(&self, keyboard: &KeyboardState) -> bool` (core/src/global_hotkey.rs) |

Rust-only on purpose: `KeyModifiers::primary_down_for(mac)`, `BUTTON_DISABLED_CLASS`,
`button::{disabled_style, mark_disabled, DisabledReason}` (pub(crate)), `image_scale::resized`.

## Least sure to compile (read these first)

- **Every app** until autofix adds the exports: they call `KeyModifiers::primary_down`, `ShellThemeAccent::colors`,
  `StatusBarZoom::create/with_*`, `with_items`, `ColorU::to_hex/parse_hex`, `RawImage::create_rgba8/resized`,
  `DiskSpace::format_bytes`, `Button::with_disabled`, `NodeData::get_attribute`, `NodeType::get_text`,
  `TextAreaState::get_text`, `DatePicker::with_week_start` through the generated crate. Assumed generated
  shapes: `with_on_zoom_out<I0: Into<RefAny>>(self, data, callback: ButtonOnClickCallbackType)` (as
  `Button.with_on_click`), statics taking `I0: Into<String>` (a `&str` passes), `OptionX::into_option()`.
- `core/src/dom.rs` `NodeData::get_attribute`: or-patterns over `(name, attr)` tuples grouped by payload type
  (all `&AzString` / all `&i32` / all `&bool`) and a nested `AriaState(nv) | AriaProperty(nv)`.
- `examples/azul-videocut/src/render.rs` `scale_to`: moves `scaled.pixels` out of a generated `RawImage` (fine
  while the generated `RawImage` has no `Drop`; it has none today).
- `examples/azul-writer/src/document.rs`: `NodeType` is now a `#[cfg(test)] use` at the top for the test
  modules' `use super::*` (non-test code no longer names it).
- `examples/azul-show/src/themes.rs`: `const FAMILIES: [(&str, ShellThemeAccent); 5]` and
  `*FAMILIES.get(i)?` need the generated enum to stay `Copy` (it derives Copy today).
- `layout/src/widgets/themes/{flat,flora}.rs` `button()`: `btn.is_disabled()` is called after
  `btn.on_click.into_option()` - fine because `into_option(&self)` clones for non-Copy options.
- The two flat/flora appended `button_toggled_face` fns use `CssPropertyWithConditions::themed(..).to_vec()`.

## Test commands (parent)

```sh
cargo test --release -p azul-core the_primary_modifier_is_cmd_on_a_mac_and_ctrl_elsewhere
cargo test --release -p azul-core an_rgba8_image_resizes_to_exactly_the_size_asked_for
cargo test --release -p azul-core a_nodes_attributes_read_back_by_their_html_name
cargo test --release -p azul-core a_text_nodes_text_reads_back_and_other_nodes_have_none
cargo test --release -p azul-css to_hex_and_parse_hex_round_trip
cargo test --release -p azul-css parse_css_reports_a_text_that_is_no_colour
cargo test --release -p azul-layout --lib only_the_platforms_primary_modifier_moves_the_focus_without_selecting
cargo test --release -p azul-layout --lib the_arrow_keys_move_a_focused_divider
cargo test --release -p azul-layout --lib the_zoom_slider_spans_the_range_the_app_gives
cargo test --release -p azul-layout --lib a_hook_made_with_create_is_the_one_with_on_click_stores
cargo test --release -p azul-layout --lib with_items_appends_the_list_in_order
cargo test --release -p azul-layout --lib a_disabled_button_is_dimmed_inert_and_says_why
cargo test --release -p azul-layout --lib a_toggled_button_is_announced_pressed_and_shows_the_pressed_face_in_both_themes
cargo test --release -p azul-layout --lib a_monday_week_start_lays_the_day_grid_monday_first
cargo test --release -p azul-layout --lib bytes_read_like_a_file_manager_writes_them
cargo test --release -p azul-layout --test all a_key_event_matches_exactly_the_hotkey_it_spells
# then the full suites (core, css, layout --lib, layout --test all, dll --lib --features build-dll):
# touched widget suites: message_list, thumbnail_strip, split_pane, cell_grid, timeline, selection_adorner,
# slider, statusbar, button, ribbon, color_input, theme_scope (accent props now lower case), tile,
# wizard_pages, date_picker, file_input.
# after autofix + dylib: the app crates' tests (AzShow themes test, AzDrive format_size, AzSheets, AzTasks,
# AzWriter, AzNotes, AzMail, AzCalculator, AzVideoCut render_tests) and the E2E scripts with `primary=True`
# (azcalculator, azcalendar, azcontacts, azdrive, aznotes, azphoto, azsheets, azshow, azvideocut, shells).
```

## Files of other wave-5 areas touched (minimal edits)

- RTE (editors): `examples/azul-notes/src/editor.rs` (box_str swap x2, `primary_down` x2),
  `examples/azul-mail/src/editor.rs` (box_str swap, href attribute: `link_dom` / `link_of` + module doc),
  `examples/azul-writer/src/document.rs` (box_str swap x4), `examples/azul-notes/src/ui.rs` (`primary_down`,
  reborrow_info), `examples/azul-mail/src/ui_compose.rs` (`primary_down`).
- PIM (Calendar / Tasks / Contacts): `examples/azul-calendar/src/{chrome.rs (with_week_start), editor_ui.rs,
  lib.rs, timegrid.rs}` (one line each), `examples/azul-tasks/src/{lib.rs, list.rs, detail.rs, chrome.rs}`,
  `examples/azul-contacts/src/ui.rs`.
- MAILHTML: `examples/azul-mail/src/ui_main.rs` (`primary_down`, `human_size` -> `DiskSpace::format_bytes`).
- HYGIENE: `layout/src/widgets/mod.rs` `impl_widget_callback!` gained one `impl $callback_wrapper { create }`
  block after the `impl $callback_value` block (its new path is `azul_core::refany::RefAny`); the macro-path
  sweep (F1) may conflict there - keep both.
- BLOCKS: `layout/src/widgets/{message_list,thumbnail_strip,timeline,selection_adorner}.rs` - the modifier
  lines and the deleted `modifiers()` helpers only.
- Shared theme files: only APPENDED `// ==== button: toggled ====` at the end of `flat.rs` / `flora.rs`, plus
  six lines inside each `button()` (the toggled face before the states, `disabled_style` after them).

## Twins found (reported, not all removed)

- `color_input::{color_to_hex, color_from_hex}` now delegate to `ColorU::{to_hex, parse_hex}`; 20 callers keep
  the old names (a sweep can switch them).
- `ShellThemeAccent::light()` (theme_scope.rs) vs `flora::STONE_*` / `LIGHT_ACC..` - the same ramps twice in
  the widget layer (DEDUP_OFFICE D1 a), untouched.
- The statusbar's two `set_on_click` literals (and ~130 `CoreCallbackData` literals, F3) could use the new
  `<Wrapper>::create`.
- The api.json codegen does not emit the `Deref` that `BoxOrStatic`'s `custom_impls` declares - the root of the
  `box_str` copies; `NodeType::get_text` sidesteps it for text nodes.

## What is left for wave 6

- A `CommandTable` (F9 M part / DEDUP_OFFICE R1): one table -> palette, menus, shortcut help and window key
  dispatch through `GlobalHotkey::matches` (no app uses `matches` yet).
- AzPhoto's tool rail: the Text tool is dimmed but deliberately selectable, and its `.with_accessibility_info`
  override after `.dom()` would drop a Button's Unavailable / CheckedTrue states - needs a Button accessible-name
  setter first, then `with_toggled(selected)` / `with_disabled(..)`.
- `dialog_kit`'s faked disabled `row_button` and RibbonButton's toggle (aria-pressed) can now use the Button
  states.
- AzSheets' zoom slider has no hook (a drag moves only the thumb; pre-existing).
- VideoCut's own `fit_within` stays until a size-returning export (`RawImage::fit_within` -> a size struct).
- The per-app ribbon helpers (`large/small/column/row/group/tab`, D9) into one shared builder.
- `ToDoBar.with_week_start / with_range` (D10), `ShellThemeAccent::name_string / from_name` FFI forms (F12).
- api.json: `Pdf::from_dom_in_callback` & co. take `CallbackInfo` by value (F13, optional `ref`).

## Commits (oldest first)

100c8ba2d progress file; eea176b5e RED primary modifier; ef538f5a0 core primary_down; 214f00028 widgets
primary_down; cbd526beb apps + e2e primary_down; d9c3ddcf9 progress; 920fadf6c AzShow stones;
0f8f43f10 RED zoom range; 8c90ae0e1 hook create(); fd73c7255 StatusBarZoom API; 21860f483 apps zoom;
00c6b10ce ribbon with_items; 3f085f354 apps ribbon folds; 8b0fe3e49 progress; fbd8b8722 RED ColorU;
9012fd28e ColorU hex/parse; 66af319ba apps hex; 75862973d RED RawImage; b92c06baa RawImage create/resized;
c5fe179c6 apps RawImage; 398bdb97d DiskSpace::format_bytes; f3a9dd224 apps bytes; 00ebe6893 progress;
7ac3ecf90 RED Button states; aed9c4dc5 Button disabled/toggled; 57079464d AzCalculator disabled keys;
2944230c4 RED attributes; 9f25d3ffd NodeData::get_attribute; ca8319fd4 AzMail href; e5f0fc51a
NodeType::get_text; 3a11557b3 box_str gone; 362b0c1f1 progress; 19f900e92 TextAreaState::get_text apps;
6a6c5fc3e RED week start; 4a4b30026 DatePickerWeekStart; 12c50f0d1 AzCalendar navigator; 2b5e9fa6a RED
GlobalHotkey::matches; 458402b05 GlobalHotkey::matches; 34ac7c700 reborrow_info; dd5b6b55a progress;
786a8ffd7 + b904379c6 + this commit: the report.
