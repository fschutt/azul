# C1_MODE_CLEAR_CHROME - progress (branch wt/c1-mode-clear-chrome, from 0a326afe5)

W4 open items 6.3 (window clear colour under a pin) and 6.4 (native chrome under a pin).

## DONE
- d00a083a9 RED: clear-colour tests in `dll/tests/color_scheme_headless.rs`
- c818f932e fix: `common::window_clear_color` (CPU per frame, WR creation, WR per frame via
  `CommonWindowState::sync_renderer_clear_color`, Wayland backbuffer clear), `common::scheme_background`,
  `CommonWindowState::{write_shown_mode, move_scheme_background, clear_color}`, creation seed by resolved mode
- f8ec37c96 progress
- 5bd05acef RED: `native_chrome_mode` tests (compile-error RED: new API)
- 8af21e92b fix: `CommonWindowState::native_chrome_mode` + macOS `NSWindow.appearance` (`sync_native_appearance`)
- 828bf2f77 RED: titlebar fallback title colour follows the window's mode
- ec0928268 fix: titlebar fallback = light default with its dark twin
- 72252aa12 rename to the mode vocabulary (`mode_background`, `move_mode_background`)
- report `scripts/C1_MODE_CLEAR_CHROME_2026_09_29.md`

## IN PROGRESS
- (none)

## NEXT
- parent: compile + run the suites listed in the report §5; manual checks §6

## Open questions
- W4 suspected bug (report §7.1): `adopt_app_color_scheme` never restyles OTHER windows
  (the pass consumes the theme delta before `adopt_system_style(held)` reads it).
