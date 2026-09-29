# C1_MODE_CLEAR_CHROME - progress (branch wt/c1-mode-clear-chrome, from 0a326afe5)

W4 open items 6.3 (window clear colour under a pin) and 6.4 (native chrome under a pin).

## DONE
- (none yet)

## IN PROGRESS
- RED tests in `dll/tests/color_scheme_headless.rs` (clear colour, creation seed, app background survives)

## NEXT
1. fix: ONE clear-colour function `common::window_clear_color` (CPU compositor, WebRender creation,
   WebRender per frame via `CommonWindowState::sync_renderer_clear_color`, Wayland backbuffer clear)
2. fix: ONE derivation `common::scheme_background` + ONE mover `CommonWindowState::move_scheme_background`
   (creation seed, SetColorScheme, ModifyWindowState, adopt_app_color_scheme(_deferred), adopt_system_style)
3. RED + fix: `CommonWindowState::native_chrome_mode` + macOS `NSWindow.appearance`
4. RED + fix: Linux CSD default title colour follows the window's mode (titlebar.rs)
5. report `scripts/C1_MODE_CLEAR_CHROME_2026_09_29.md`

## Open questions
- (none yet)
