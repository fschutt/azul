# T1 - minimal app theme for the `@theme` widget migration (PROGRESS)

Branch `wt/t1-app-theme`, cut from `fix/input-bugs-2026-09-19` @ `2892031b7`.
Final report: `scripts/T1_APP_THEME_2026_09_29.md`. Nothing compiled (house rule).

## DONE (all)
- `a20f4f912` plan checkpoint
- `56ea1eea3` RED css: `css/tests/app_theme_selects_theme_blocks.rs` + unit tests in `css/src/dynamic_selector.rs`
- `e1610b42c` RED core: `AppConfig.theme` (padding test + default), `app_theme_tests` in `core/src/callbacks_test.rs`
- `ff811a074` RED layout: `layout/tests/app_theme_override.rs` (+ all.rs), `set_theme` unit test in `layout/src/callbacks.rs`
- `a8850021c` RED dll: `dll/tests/app_theme_headless.rs` (new test target)
- `fe6356f51` css impl: from_block_name, theme_chain + matcher, DEFAULT_APP_THEME, app_theme_chain, theme_conditions!, helpers
- `7bf0475d3` core impl: AppConfig.theme + builders, `azul_core::app_theme` (global + ThemeScope), get_theme_name, AppThemeChange
- `a7ff5599d` layout impl: LayoutWindow::app_theme -> context chain, form-controls ThemeScope, SetTheme + CallbackInfo set/get_theme, E2E arm
- `30ed7e1ca` layout: early `resolve_form_controls` pass scoped too
- `3e64b9490` dll impl: App::create publishes, SetTheme handler, regenerate_layout adopts + scopes + full path
- `7abf8991f` layout: `app_theme` in the two exhaustive LayoutWindow field audits
- `d68a4a7ee` docs: styling guide
- `22995643b` test: identical DOM still repaints (pins the full-path hunk)
- final commit: report `scripts/T1_APP_THEME_2026_09_29.md` + this file

## IN PROGRESS
- nothing

## NEXT (parent)
- compile + suites (report section 5), api.json autofix (section 6), decide the open questions (section 7),
  then the migration step 0 (UiTheme::name/from_name/current + theme_pairs lint pairing on theme names, section 4)

## Decisions
- `LayoutCallbackInfo::get_theme()` is TAKEN (the colour scheme, the demo calls it) -> the layout-time name getter
  is `LayoutCallbackInfo::get_theme_name()`; `CallbackInfo::get_theme()` returns the name (as asked).
- Widgets' `dom()` has no `LayoutCallbackInfo`: the theme a DOM is BUILT for is ambient
  (`azul_core::app_theme::current_theme()`: a scope entered around every DOM build of a window, else the app's
  published theme).
- A window adopts the app theme in ONE place, `common::layout::regenerate_layout` (no per-shell code); a theme
  rebuild takes the full path.

## Open questions
- FLIP on AppThemeChange (animate_moves), `get_theme_name` naming, unknown-theme sharp edge until the §7 floor
