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
