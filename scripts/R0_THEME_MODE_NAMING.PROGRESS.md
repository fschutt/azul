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
- f7d933ddc dll: app.rs, event.rs (app_mode, resolved_window_mode, mode_change_tier,
  mirror_app_mode, adopt_app_mode(_deferred/_in_other_windows), SetMode arm, ModeChange /
  ThemeChange tags), common/layout.rs, every shell, dll tests (git mv color_scheme_headless.rs ->
  mode_headless.rs; app_theme_headless; backend_feature_parity scan key -> ModeChange), examples
  (AzWidgets toolbar "Mode" segment, AzWriter get_mode).

- bcbb6008d guide (styling/themes.md: theme vs mode paragraph; styling.md `@theme` line) and
  scripts/preflight_contracts.py `check_mode_naming` (9 hits on 0a326afe5, 0 now).
- d49475cbc get_mode doc ASCII (api.json copies it).
- (next commit) report scripts/R0_THEME_MODE_NAMING_2026_09_29.md.

## IN PROGRESS
- nothing: task complete; parent applies api.json + compiles.

## NEXT
- (parent) api.json items 1-14 from the report, `codegen all`, then the test commands there.
- (follow-up task) the TYPE sweep table in the report (WindowTheme -> WindowMode, ...).

## Open questions
- none
