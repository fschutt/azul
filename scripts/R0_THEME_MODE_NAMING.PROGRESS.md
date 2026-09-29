# R0_THEME_MODE_NAMING - progress

Branch `wt/r0-theme-mode-naming` (from 0a326afe5). "Theme" = app theme (flat / flora / native / user);
"mode" = light / dark / system.

## DONE
- edcacd820 RED: `dll/tests/theme_and_mode_are_two_names.rs` (does not compile before the rename).
- (next commit) core: LayoutCallbackInfo get_mode / get_theme, RelayoutReason ModeChange = 3 /
  ThemeChange = 6, AppConfig.mode + with_mode / set_mode, core tests + docs.

## IN PROGRESS
- layout crate: CallbackInfo, CallbackChange::SetMode, window.rs internals, e2e runner, tests.

## NEXT
1. LayoutCallbackInfo::get_theme -> get_mode, then get_theme_name -> get_theme.
2. CallbackInfo *_color_scheme -> *_mode; AppConfig.color_scheme -> mode (+ with_/set_);
   CallbackChange::SetColorScheme { scheme } -> SetMode { mode }.
3. RelayoutReason::ThemeChange -> ModeChange (value 3), AppThemeChange -> ThemeChange (value 6).
4. Internals (layout/src/window.rs, dll shells) -> mode names.
5. dll/tests/color_scheme_headless.rs -> dll/tests/mode_headless.rs.
6. Guide + doc comments; preflight_contracts.py check (no `pub fn *color_scheme*`).
7. Report `scripts/R0_THEME_MODE_NAMING_2026_09_29.md`.

## Open questions
- none
