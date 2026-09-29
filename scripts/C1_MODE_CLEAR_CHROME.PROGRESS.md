# C1_MODE_CLEAR_CHROME - progress (branch wt/c1-mode-clear-chrome, from 0a326afe5)

W4 open items 6.3 (window clear colour under a pin) and 6.4 (native chrome under a pin).

## DONE
- d00a083a9 RED: clear-colour tests in `dll/tests/color_scheme_headless.rs`
- c818f932e fix: `common::window_clear_color` (CPU per frame, WR creation, WR per frame via
  `CommonWindowState::sync_renderer_clear_color`, Wayland backbuffer clear), `common::scheme_background`,
  `CommonWindowState::{write_shown_mode, move_scheme_background, clear_color}`, creation seed by resolved mode

## IN PROGRESS
- RED: `native_chrome_mode` decision test (the fn body is parked in the scratchpad:
  `c1_native_chrome_mode.rs`, goes back after `adopt_desktop_theme` in event.rs)

## NEXT
3. RED + fix: `CommonWindowState::native_chrome_mode` + macOS `NSWindow.appearance`
4. RED + fix: Linux CSD default title colour follows the window's mode (titlebar.rs)
5. report `scripts/C1_MODE_CLEAR_CHROME_2026_09_29.md`

## Open questions
- (none yet)
