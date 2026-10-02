# APIEXPORT progress (wave 5, 2026-10-02) - branch wt/apiexport from 2e92c759b

## DONE (commit hashes)
- eea176b5e test RED primary modifier (core events_test, message_list, split_pane; roving::test_support::command_keys)
- ef538f5a0 core KeyModifiers::primary_down / primary_down_for; KeyboardState::primary_down via derived_modifiers
- 214f00028 widgets: 13 sites -> primary_down, 3 `modifiers()` twins deleted
- cbd526beb apps: 25 sites -> KeyModifiers::primary_down; e2e scripts `primary=True`
- 920fadf6c AzShow themes.rs reads ShellThemeAccent::colors (FAMILIES table, stone(i), paper(), accent(index))
- 0f8f43f10 RED statusbar zoom range; 8c90ae0e1 impl_widget_callback! emits <Wrapper>::create(refany, callback)
- fd73c7255 StatusBarZoom create / set_range / with_range / hook setters / show_label
- 21860f483 Writer / Show / Sheets status bar zoom via the new API (Show+Sheets 10..400)
- 00c6b10ce ribbon with_items / add_items / with_groups / add_groups; 3f085f354 apps' ribbon folds -> with_items
- fbd8b8722 RED ColorU hex/parse_css; 9012fd28e ColorU::to_hex/parse_hex/parse_css (color_input delegates, theme_scope
  hex twin gone, accent props lower case); 66af319ba apps (Show model, Photo, Widgets, Writer, Sheets parse_hex gone)
- 75862973d RED RawImage; b92c06baa RawImage::create_rgba8 / resized (thumbnail = fit_within + resized);
  c5fe179c6 apps create_rgba8 (Photo x2, Paint x2, Review, Widgets, VideoCut) + VideoCut scale_to via resized
- 398bdb97d DiskSpace::format_bytes (moved from tile.rs); f3a9dd224 apps (Drive, Mail, Tasks, Photo)
- 7ac3ecf90 RED Button disabled/toggled; aed9c4dc5 Button disabled_reason + toggled (themes: button_toggled_face
  appended in flat.rs/flora.rs; ribbon's disabled machinery moved into button.rs); 57079464d AzCalculator disabled key
- 2944230c4 RED attributes; 9f25d3ffd NodeData::get_attribute/get_attributes (CallbackInfo::get_node_attribute
  delegates); ca8319fd4 AzMail href in attribute (LINK_CLASS_PREFIX gone)
- e5f0fc51a NodeType::get_text (+test); 3a11557b3 box_str x3 gone (Writer/Notes/Mail)
- 19f900e92 TextAreaState.get_text adoption (Tasks, Calendar, Contacts, Show, Widgets)
- 6a6c5fc3e RED DatePicker week start; 4a4b30026 DatePickerWeekStart + with_week_start; 12c50f0d1 AzCalendar navigator
- 2b5e9fa6a RED GlobalHotkey.matches; 458402b05 GlobalHotkey::matches
- 34ac7c700 reborrow_info x3 -> *info

## api.json list (accumulating; for the report)
- KeyModifiers.primary_down (dom; self ref -> bool)
- KeyboardState.primary_down / shift_down / ctrl_down / alt_down / super_down / is_key_down(key: VirtualKeyCode) (dom)
- ShellThemeAccentColors (struct, shells) + ShellThemeAccent.colors(dark: bool) -> ShellThemeAccentColors
- ButtonOnClick.create(data: RefAny, callback: ButtonOnClickCallback) (ctor, widgets);
  SliderOnValueChange.create(data, callback: SliderOnValueChangeCallback)
- StatusBarZoom.create(percent, min, max) ctor; set_percent, set_range, with_range, set/with_on_zoom_out,
  set/with_on_zoom_in, set/with_on_slider_change, set/with_show_label
- RibbonColumn / RibbonRow / RibbonGroup .add_items / .with_items(items: RibbonItemVec); RibbonTab .add_groups / .with_groups(RibbonGroupVec)
- ColorU.to_hex (self ref -> String), ColorU.parse_hex(text: String) -> OptionColorU, ColorU.parse_css(text: String) -> OptionColorU (css)
- RawImage.create_rgba8(width: u32, height: u32, pixels: U8Vec, premultiplied_alpha: bool) ctor; RawImage.resized(width: u32, height: u32) -> OptionRawImage (image)
- DiskSpace.format_bytes(bytes: u64) -> String (static, file)
- Button: struct fields disabled_reason (AzString, after on_click) + toggled (OptionBool, last);
  set_disabled / with_disabled(reason: String) / is_disabled / set_toggled / with_toggled(bool)
- NodeData.get_attribute(name: String) -> OptionString, NodeData.get_attributes() -> AttributeTypeVec (dom)
- NodeType.get_text() -> OptionString (dom)
- TextAreaState.get_text() -> String (widgets)
- DatePickerWeekStart (enum, widgets); DatePicker field week_start (after mode); set_week_start / with_week_start
- GlobalHotkey.matches(keyboard: KeyboardState ref) -> bool (app)

## IN PROGRESS
- nothing: DONE. Report committed: scripts/APIEXPORT_2026_10_02.md (786a8ffd7, b904379c6, 651b01170).

## NEXT
- (parent) autofix the api.json list in the report, compile, run the listed tests.

## Decisions
- No CallbackInfo convenience for the primary modifier: `info.get_key_modifiers().primary_down()` is one call;
  widgets read `info.get_current_keyboard_state().primary_down()` (the pressed set, which the test harness fills).
- `KeyModifiers::primary_down_for(mac)` stays Rust-only (tests); not in the api.json list.
- Left on purpose: combobox / roving / office_shell / shortcut_recorder test "ANY modifier held"; color_input
  already follows the conventions.
- E2E scripts: `primary=True` kwarg on each key helper (Cmd on darwin, Ctrl elsewhere) - the apps now ignore Ctrl
  on macOS, so the scripts had to follow.
- Overwrote scratchpad/apiq.py (an older helper of the same name) by accident; my tools live in scratchpad/apiexport/.
- StatusBarZoom office_2013() keeps [10, 190]; apps give their range. Sheets' slider still has no hook (a drag
  moves only the thumb) - pre-existing, wave 6.
- Ribbon app helpers (column/row/group/tab per app) stay; only their folds became with_items.
