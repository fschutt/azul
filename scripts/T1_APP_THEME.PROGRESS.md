# T1 - minimal app theme for the `@theme` widget migration (PROGRESS)

Branch `wt/t1-app-theme`, cut from `fix/input-bugs-2026-09-19` @ `2892031b7`.
Final report: `scripts/T1_APP_THEME_2026_09_29.md`. Nothing compiled (house rule).

## DONE
- `a20f4f912` plan checkpoint
- `56ea1eea3` RED css: `css/tests/app_theme_selects_theme_blocks.rs` + unit tests in `css/src/dynamic_selector.rs`
- `e1610b42c` RED core: `AppConfig.theme` (padding test + default), `app_theme_tests` in `core/src/callbacks_test.rs`
- `ff811a074` RED layout: `layout/tests/app_theme_override.rs` (+ all.rs), `set_theme` unit test in `layout/src/callbacks.rs`
- `a8850021c` RED dll: `dll/tests/app_theme_headless.rs` (new test target)

## IN PROGRESS
- implementation, css first

## NEXT
1. css: `@theme(name)` -> `ThemeCondition::Custom(name)` (both parsers); `DynamicSelectorContext.theme_chain`
   (`[name]`), `Custom(x)` live iff `x` in the chain; `DEFAULT_APP_THEME = "flat"`; declaration helpers
   (`theme_conditions!`, `in_theme`, `theme_names`, `without_theme_names`, `is_light_half` accepts names)
2. core: `AppConfig.theme` + `with_theme`/`set_theme` (padding test); `azul_core::app_theme` (global + DOM-build
   scope); `LayoutCallbackInfo::get_theme_name`; `RelayoutReason::AppThemeChange`
3. layout: `LayoutWindow::app_theme` mirror -> context; `CallbackChange::SetTheme`; `CallbackInfo::set_theme` /
   `get_theme`; form-controls scope; E2E runner arm
4. dll: `SetTheme` handler (rebuild, fan-out), mirror sync + reason upgrade + scope in `regenerate_layout`,
   `App::create` publishes
5. report (+ migration recipe)

## Decisions so far
- `LayoutCallbackInfo::get_theme()` is TAKEN (W4: returns the colour scheme `WindowTheme`, the demo calls it) ->
  the layout-time name getter is `LayoutCallbackInfo::get_theme_name()`; `CallbackInfo::get_theme()` is free
  and returns the name (as asked).
- Widgets' `dom()` has no `LayoutCallbackInfo`: the theme a DOM is being BUILT for is ambient
  (`azul_core::app_theme::current_theme()`: a scope the engine installs around every DOM build of a window,
  else the app's published theme), same seam shape as the style-dependency recorder.

## Open questions
- (see report)
