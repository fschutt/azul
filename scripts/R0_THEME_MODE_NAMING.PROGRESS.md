# R0_THEME_MODE_NAMING - progress

Branch `wt/r0-theme-mode-naming` (from 0a326afe5). "Theme" = app theme (flat / flora / native / user);
"mode" = light / dark / system.

## DONE
- edcacd820 RED: `dll/tests/theme_and_mode_are_two_names.rs` (does not compile before the rename).
- fb98155ea core: LayoutCallbackInfo get_mode / get_theme, RelayoutReason
  ModeChange = 3 / ThemeChange = 6, AppConfig.mode + with_mode / set_mode, core tests + docs.
- e3ab1387c layout: CallbackInfo set_mode / get_mode / get_resolved_mode,
  CallbackChange::SetMode { mode }, window.rs APP_MODE / set_app_mode / app_mode /
  resolve_window_mode(_with) / LayoutWindow.mode / window_mode_for / mode_change_needs_new_dom,
  e2e runner, layout/tests/app_color_scheme_override.rs (file name kept: all.rs untouched).
- (next commit) dll: app.rs, event.rs (app_mode, resolved_window_mode, mode_change_tier,
  mirror_app_mode, adopt_app_mode(_deferred/_in_other_windows), SetMode arm, ModeChange /
  ThemeChange tags), common/layout.rs, every shell, dll tests (git mv color_scheme_headless.rs ->
  mode_headless.rs; app_theme_headless; backend_feature_parity scan key -> ModeChange), examples
  (AzWidgets toolbar "Mode" segment, AzWriter get_mode).

## IN PROGRESS
- guide text + preflight check.

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
