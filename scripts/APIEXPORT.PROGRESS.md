# APIEXPORT progress (wave 5, 2026-10-02) - branch wt/apiexport from 2e92c759b

## DONE (commit hashes)
- eea176b5e test RED primary modifier (core events_test, message_list, split_pane; roving::test_support::command_keys)
- ef538f5a0 core KeyModifiers::primary_down / primary_down_for; KeyboardState::primary_down via derived_modifiers
- 214f00028 widgets: 13 sites -> primary_down, 3 `modifiers()` twins deleted
- cbd526beb apps: 25 sites -> KeyModifiers::primary_down; e2e scripts `primary=True`

## api.json list (accumulating; for the report)
- KeyModifiers.primary_down (dom; self ref -> bool)
- KeyboardState.primary_down / shift_down / ctrl_down / alt_down / super_down / is_key_down(key: VirtualKeyCode) (dom)

## IN PROGRESS
- item 2: ShellThemeAccent::colors export + AzShow themes.rs

## NEXT
2. ShellThemeAccent::colors export, delete AzShow copy
3. ButtonOnClick::create, StatusBarZoom ctor/setters/range (RED), Ribbon with_items
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
