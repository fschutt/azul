# APIEXPORT progress (wave 5, 2026-10-02) - branch wt/apiexport from 2e92c759b

## DONE (commit hashes)
- (none yet)

## IN PROGRESS
- reading sources (KeyboardState::primary_down, ShellThemeAccent, StatusBarZoom, ColorU, ...)

## NEXT
1. primary_down export + widget/app `ctrl || meta` sweep (RED tests first)
2. ShellThemeAccent::colors export, delete AzShow copy
3. ButtonOnClick::create, StatusBarZoom ctor/setters/range (RED), Ribbon with_items
4. ColorU::to_hex/parse_hex/try_from_css; RawImage::create_rgba8/resized; format_bytes; Button disabled/toggled;
   NodeData attribute getter; text-node accessor; TextAreaState.get_text; DatePicker.with_week_start; GlobalHotkey.matches
5. reborrow_info -> *info

## Decisions
