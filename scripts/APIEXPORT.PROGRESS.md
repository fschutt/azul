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

## api.json list (accumulating; for the report)
- KeyModifiers.primary_down (dom; self ref -> bool)
- KeyboardState.primary_down / shift_down / ctrl_down / alt_down / super_down / is_key_down(key: VirtualKeyCode) (dom)
- ShellThemeAccentColors (struct, shells) + ShellThemeAccent.colors(dark: bool) -> ShellThemeAccentColors
- ButtonOnClick.create(data: RefAny, callback: ButtonOnClickCallback) (ctor, widgets);
  SliderOnValueChange.create(data, callback: SliderOnValueChangeCallback)
- StatusBarZoom.create(percent, min, max) ctor; set_percent, set_range, with_range, set/with_on_zoom_out,
  set/with_on_zoom_in, set/with_on_slider_change, set/with_show_label
- RibbonColumn / RibbonRow / RibbonGroup .add_items / .with_items(items: RibbonItemVec); RibbonTab .add_groups / .with_groups(RibbonGroupVec)

## IN PROGRESS
- item 4: ColorU::to_hex / parse_hex / try_from_str

## NEXT
4. ColorU::to_hex/parse_hex/try_from_str; RawImage::create_rgba8/resized; format_bytes; Button disabled/toggled;
   NodeData attribute getter; text-node accessor; TextAreaState.get_text; DatePicker.with_week_start; GlobalHotkey.matches
5. reborrow_info -> *info

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
