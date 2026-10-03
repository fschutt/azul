# C1 - the clear colour and the native chrome follow the MODE the window shows (2026-09-29)

Branch `wt/c1-mode-clear-chrome`, from `0a326afe5`. Closes W4 open items 6.3 and 6.4
(`scripts/W4_SCHEME_OVERRIDE_DEMO_2026_09_29.md` §6).

## 1. What was built

### 1.1 ONE clear colour - `common::window_clear_color` (`dll/src/desktop/shell2/common/mod.rs`)

`window_clear_color(background_color, mode, system_style, follow_system_background, transparent) -> ColorU`:

1. `transparent` (a material): `(0,0,0,0)`;
2. the window's `background_color` (the app's own, or the one its mode derived - 1.2);
3. `follow_system_background` (a real window): `mode_background(mode, style, None, None)` - the
   palette of the mode the window SHOWS (`SystemStyle::colors_for_theme`), resolved like the
   cascade resolves `system:window-background`;
4. the fixed offscreen pair (white / `[42,46,50]`, unchanged - headless reference images).

Opaque answers are alpha 255 (the CPU path already forced it; WebRender now does too).

Every renderer uses it:

| renderer | where | before |
|---|---|---|
| CPU compositor (headless, macOS, X11, Wayland, Windows, iOS, Android) | `headless/mod.rs` `CpuBackend::render_frame`, mode = `layout_window.window_theme_for(ws.theme)` | app bg, else the DESKTOP palette, else by `ws.theme` |
| WebRender, creation | `wr_translate2::default_renderer_options` | inline: bg else by the unseeded `options.window_state.theme` |
| WebRender, per frame | `CommonWindowState::sync_renderer_clear_color` before `Renderer::render` in `macos/mod.rs`, `linux/x11/mod.rs`, `linux/wayland/mod.rs`, `windows/mod.rs`; `set_clear_color` + `force_redraw` when the colour moved | never updated |
| Wayland backbuffer clear (age 0) | `linux/wayland/mod.rs` | hard-coded Adwaita grey `(0.937,0.941,0.945,1)` - opaque even for a transparent window |

`CommonWindowState::clear_color()` is the one call shape for a desktop window (follow = true,
transparent = material != Opaque).

### 1.2 ONE derivation + ONE mover for the window background

- `common::mode_background(mode, style, light, dark) -> ColorU`: the app's per-mode background
  (`WindowCreateOptions::background_color_light/_dark`), else the palette's for that mode
  (`SystemColorRef::WindowBackground.resolve_for_theme(colors_for_theme(mode))`). Always a colour:
  `None` in `background_color` keeps meaning "no background" (material / offscreen).
- Creation: `resolve_initial_background_color` seeds from the mode the window WILL show
  (`initial_window_theme(options.theme, desktop)` - the decision `CommonWindowState::new` makes),
  not from `SystemStyle::theme`.
- `CommonWindowState::move_mode_background(derived_from)`: if the background is what
  `derived_from` derives for EITHER mode, it becomes what the current style derives for the mode
  shown now; an app-set background stays. "Either mode" makes it order-independent (the desktop
  adopters write the theme and run the pass before `adopt_system_style` gets the new style).
- `CommonWindowState::write_shown_mode(mode)`: writes `ws.theme` and moves the background.
  Callers: the `SetColorScheme` arm, `adopt_app_color_scheme(_deferred)`, headless
  `set_system_theme`, iOS `adopt_device_appearance`, Android `drain_pending_theme` (the last three
  never reach `adopt_system_style`).
- `ModifyWindowState`: after the state copy, on a theme change, `move_mode_background(held)`.
- `adopt_system_style`: its own background block (moved only between the two desktop palettes,
  compared against `old_style.colors.window_background`, and overwrote an app background with the
  per-mode one) is replaced by `move_mode_background(&old_style)`.

### 1.3 Native chrome

- `CommonWindowState::native_chrome_mode() -> Option<DarkLightMode>` (platform-agnostic, tested
  headless): force the chrome into the shown mode while a pin is active, or while the window shows
  another mode than the desktop's (its own `WindowCreateOptions::theme` seed); `None` = inherit.
  "A pin" = `resolve_window_theme(app, Light) == resolve_window_theme(app, Dark)` - the one
  decision asked for both desktops (covers the app pin and `AZ_THEME` without re-implementing
  the precedence).
- macOS `MacOSWindow::sync_native_appearance()`: `NSWindow.appearance` = `appearanceNamed:
  "NSAppearanceNameDarkAqua"/"NSAppearanceNameAqua"` or nil; cached in the new field
  `applied_chrome_mode`. Called at creation (before the first draw), at the TOP of every
  `sync_window_state` (before the diff: a pin that agrees with the window's mode moves no state
  yet must force the chrome), and for each other window in `adopt_app_color_scheme_in_other_windows`.
  Only the WINDOW's appearance - never `NSApp.appearance`, which the desktop probe reads. Name
  literals, no dlsym (so the double-deref trap does not apply).
- Windows: unchanged (DWM caption already follows `ws.theme`).
- Linux CSD (and the macOS `NoTitleAutoInject` bar): `Titlebar::from_system_style(_csd)` falls back
  to `DEFAULT_TITLE_COLOR_LIGHT`, which `build_title_style` already gives its `@theme dark` twin -
  the cascade picks by the WINDOW's mode, restyle-proof. It picked by `system_style.theme` and
  baked the desktop's mode in.

## 2. Commits

| commit | what |
|---|---|
| `d00a083a9` | RED: 7 tests in `dll/tests/color_scheme_headless.rs` (5 red, 2 guards) + `make_desktop_window` helpers |
| `c818f932e` | fix: `window_clear_color`, `mode_background` (then `scheme_background`), mover, WR sync, Wayland clear, creation seed |
| `f8ec37c96` | progress checkpoint |
| `5bd05acef` | RED: 2 `native_chrome_mode` tests (red as a compile error - the fn is new API) |
| `8af21e92b` | fix: `native_chrome_mode` + macOS `NSWindow.appearance` |
| `828bf2f77` | RED: `the_fallback_title_colour_follows_the_windows_mode_not_the_desktops` (replaces the test that stated the desktop rule) |
| `ec0928268` | fix: titlebar fallback; `from_system_style_csd_zeroes_the_padding_and_keeps_everything_else` now expects the light default |
| `72252aa12` | rename `scheme_background` -> `mode_background`, `move_scheme_background` -> `move_mode_background` (naming ruling) |

## 3. api.json

No changes. Everything new is Rust-internal to `azul-dll` (`shell2::common` free functions,
`CommonWindowState` methods, a private `MacOSWindow` field/method); the `Titlebar` constructors
keep their signatures (behaviour only).

## 4. Least sure to compile

1. `macos/mod.rs` `sync_native_appearance`: `msg_send![objc2::class!(NSAppearance), appearanceNamed: name]`
   with `name: &'static NSString` (`ns_string!`) returning `*mut NSObject`, and
   `msg_send![&*self.window, setAppearance: appearance]` with a `*mut NSObject` (null for nil).
   Same shapes as the existing `NSRunLoop currentRunLoop` / `performSelector:withObject:` calls.
2. `CommonWindowState::sync_renderer_clear_color`: `renderer.set_clear_color(wr_translate2::
   wr_translate_color_f(azul_css::props::basic::ColorF::from(color)))` - relies on
   `From<ColorU> for ColorF` (css) and `webrender::Renderer::{set_clear_color, force_redraw}`.
3. `windows/mod.rs`, `linux/x11/mod.rs`, `linux/wayland/mod.rs`, `ios/mod.rs`, `android/mod.rs`:
   one-line calls, uncompiled on those targets (`self.common.sync_renderer_clear_color()`,
   `common.write_shown_mode(theme)`). Wayland: `let clear = ...` is used only in the GPU arm.
4. `wr_translate2::default_renderer_options`: the local `use webrender::{api::ColorF as WrColorF, ..}`
   was dropped; `wr_translate_color_f` / `CssColorF` are the file-level imports further down.
5. `layout/src/widgets/titlebar.rs` test helper `title_colour_in`: builds a
   `DynamicSelectorContext { theme, ..Default::default() }` and filters with
   `CssPropertyWithConditions::matches`.

## 5. Test commands for the parent

```
cargo test --release -p azul-dll --test color_scheme_headless
cargo test --release -p azul-layout --lib titlebar
cargo test --release -p azul-dll --test backend_feature_parity   # source scans adopt_system_style's body
cargo test --release -p azul-dll --test headless_lifecycle
cargo test --release -p azul-dll --test present_path             # CPU present path, clear colour untouched offscreen
cargo test --release -p azul-dll --lib initial_window_theme
```

New tests (all in `color_scheme_headless` unless noted):
`a_dark_pin_clears_the_window_dark_on_a_light_desktop`,
`a_window_opened_under_a_dark_pin_starts_on_a_dark_canvas`,
`an_unseeded_canvas_takes_the_system_background_of_the_mode_the_window_shows`,
`a_per_mode_background_follows_the_pin`,
`a_background_the_app_set_survives_a_mode_change` (guard),
`switching_back_to_system_returns_the_desktop_background` (guard),
`modify_window_state_with_a_new_theme_moves_the_seeded_background`,
`the_native_chrome_is_forced_into_a_pinned_mode_and_inherits_otherwise`,
`a_pin_that_matches_the_desktop_still_holds_the_chrome_when_the_desktop_flips`,
`the_fallback_title_colour_follows_the_windows_mode_not_the_desktops` (layout lib, titlebar.rs).

`HeadlessWindow` itself still does not seed a background and keeps `follow_system_background =
false` (reproducible offscreen output); the tests open a window the way a desktop shell does
(`make_desktop_window`: `resolve_initial_background_color` + follow = true).

## 6. Manual checks (cannot run headless)

Build the example once: `cargo build --release --manifest-path examples/rust/Cargo.toml --bin hello-world`.
Its body paints no background, so the whole window shows the clear colour.

macOS, desktop in LIGHT appearance:

1. `AZ_THEME=dark AZ_BACKEND=cpu <hello-world>`: the titlebar and traffic lights are DARK
   (DarkAqua); the window canvas is dark grey `(50,50,50)` (the `system:window-background` dark
   default), not white. Before: light titlebar, white canvas.
2. The same with `AZ_BACKEND=gpu` (WebRender): same result.
3. While 1 or 2 runs, System Settings > Appearance > Dark, then Light: the titlebar stays dark
   throughout; the canvas becomes the desktop's own dark window background while the desktop is
   dark, `(50,50,50)` again when it is light.
4. `AZ_THEME=light` on a DARK desktop: light titlebar, canvas `(236,236,236)`.
5. No pin (`AZ_THEME` unset), `AZ_BACKEND=gpu`, start light, flip the desktop to Dark: the canvas
   turns dark at once (before: WebRender kept clearing to the creation colour, white) and the
   titlebar follows the desktop (appearance stays nil / inherits).
6. Optional log check: `AZ_LOG=debug` shows no appearance-probe churn after 1 (the
   `viewDidChangeEffectiveAppearance` the window appearance set triggers finds the desktop
   unchanged).

Linux (Wayland, `AZ_BACKEND=gpu`), GNOME light: `AZ_THEME=dark <hello-world>` - the undrawn
backbuffer and the canvas are dark (before: Adwaita grey / white). For the CSD fallback title,
run on a desktop that reports no text colour (or a test build with `colors.text = None`) and
check the title is `#4c4c4c` in a light window and `#e5e5e5` in a dark one whatever the desktop.

## 7. Left / found on the way

1. **Suspected W4 bug (reasoned, not run): the app-wide switch does not restyle OTHER windows.**
   `adopt_app_color_scheme` writes the theme, runs `process_window_events(0)` - whose tail
   CONSUMES the delta (`set_previous_window_state(current)`) - and then calls
   `adopt_system_style(held)`, whose first test is
   `if *old_style == *new_style && !theme_delta { return false; }` with `theme_delta` read from
   that very baseline: always false there, so it returns before any restyle / rebuild request.
   Every shell's desktop path has the same order (pass, then `adopt_system_style`), but there
   the style changed, so it proceeds. Multi-window only (macOS / Win32 / X11 / Wayland walks), so
   no headless test can see it. Fix sketch: read the theme delta before the pass (pass it in), or
   have `adopt_app_color_scheme` run `color_scheme_change_tier()` + the restyle branch itself. The
   canvas of those windows is right regardless (the mover runs in `write_shown_mode`).
2. **Linux CSD beyond the fallback:** a text colour the desktop DID report, the titlebar
   background (`metrics.titlebar.background_active`) and the whole CSD stylesheet
   (`SystemStyle::create_csd_stylesheet`) are still baked from the desktop palette, so a pinned
   window keeps a desktop-coloured CSD bar. Real fix: build the CSD from `system:` keywords (they
   resolve against the window's mode) or from `colors_for_theme(mode)` with twins.
3. Palette holes: a pin opposite to the desktop gets the keyword defaults
   (`(50,50,50)` / `(236,236,236)`), by the documented `colors_for_theme` rule (no guessing what
   the desktop would show in the other mode). A platform whose probe leaves `window_background`
   empty in its OWN mode now seeds `(236,236,236)` / `(50,50,50)` instead of clearing white /
   `[42,46,50]` - every current probe fills it.
4. Twins noticed (not merged): the Windows `WM_SETTINGCHANGE` arm maps `new_style.theme` to a
   `DarkLightMode` inline instead of calling `desktop_window_theme`; `mode_background` and
   `LayoutWindow::dynamic_selector_context` each map `DarkLightMode`/`ThemeCondition` -> `Theme`
   inline (no shared `DarkLightMode -> Theme` helper exists; `desktop_window_theme` is the inverse).
5. Transient popups: `popup_create_options` sets `options.theme = None`, so a popup of a window
   whose mode came from its own seed (no app pin) resolves to the desktop's mode (pre-existing;
   under a pin both agree).

## 8. Parallel-work notes (R0 rename)

My new names already use "mode" (`window_clear_color`, `mode_background`,
`move_mode_background`, `write_shown_mode`, `native_chrome_mode`, `sync_native_appearance`).
They call today's names that R0 renames: `initial_window_theme` (common/mod.rs),
`resolve_window_theme` + `app_color_scheme()` (`native_chrome_mode`), and the test file uses
`set_app_color_scheme` / the `set_color_scheme` helper. Edited lines R0 will also touch: the
`SetColorScheme` arm and `adopt_app_color_scheme(_deferred)` bodies in `common/event.rs`.
