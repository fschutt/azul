# W4 - public colour-scheme override + AzWidgets demo toggles (2026-09-29)

Branch `wt/w4-scheme-override-demo`, cut from `fix/input-bugs-2026-09-19` at `5f11dbdda`
(the base has since moved to `b9cc36bee`: W3a/W3b widget theming + api.json sync; the only
file both touched is `layout/tests/all.rs`, where both APPEND one module pair at the end).
Nothing was compiled (house rule); see "Least sure to compile" below.

## 1. What an app gets

```rust
// startup (default: follow the desktop)
let config = AppConfig::create().with_color_scheme(OptionDarkLightMode::Some(DarkLightMode::Dark));
// runtime, from any callback - every window of the app, and every window opened later
info.set_color_scheme(OptionDarkLightMode::None);          // back to "System"
let choice   = info.get_color_scheme();                  // OptionDarkLightMode: the CHOICE
let resolved = info.get_resolved_color_scheme();         // DarkLightMode: what the window SHOWS
// in layout(): what the window shows, and reading it declares the dependency
let dark = info.get_theme() == DarkLightMode::Dark;
```

A switch is a RESTYLE of the retained DOM (colours only; `layout()` does not run) unless the
window's last `layout()` READ the scheme (`LayoutCallbackInfo::get_theme` records
`SystemStyleDependency::Theme`, `get_system_style` records `Everything`); then that window's
`layout()` runs again - the `depends_on_locale` rule. A desktop light/dark flip while the app
is pinned does not move the app; switching back to System lands on the desktop's CURRENT theme
at once (no desktop event needed). `AZ_THEME=light|dark` still outranks everything.

## 2. Design

### 2.1 Type: reuse `OptionDarkLightMode` (None = follow the desktop, Some = pin)

Justification, in order of weight:

1. **The no-padding rule.** `AppConfig` is `repr(C)` and must carry zero padding
   (`core/src/resources_test.rs::app_config_has_no_padding_between_its_fields`). Its 4-aligned
   tail is `log_level + natural_scroll + termination_behavior` (12 B) + `remote_control`
   (8 B, align 2) + 4 bools = 24 B. A new 4-byte `repr(C)` `ColorScheme` enum makes it 28 B
   -> 4 bytes of TAIL padding, test red. `OptionDarkLightMode` (`repr(C, u8)` tag + 4-aligned
   `DarkLightMode`) is 8 B / align 4 -> 32 B, zero padding. The alternatives were a reserved
   field or an unrelated 4-byte setting (both worse).
2. **One vocabulary.** It is already the type of the window-level request,
   `WindowCreateOptions::theme` ("`None` follows the system"), so the app-level and the
   window-level requests read the same.
3. No new api.json class.

Cost: `OptionDarkLightMode::Some(DarkLightMode::Dark)` is wordier than `ColorScheme::Dark` in
the bindings. If the parent prefers a dedicated enum anyway, it has to come with a second
4-byte AppConfig field or an explicit `_reserved: u32`.

### 2.2 ONE decision function (I1)

`azul_layout::window::resolve_window_theme(app, window)` (testable core
`resolve_window_theme_with(env, app, window)`):

```text
AZ_THEME env pin  >  the app's choice (Some)  >  the window's own theme  >  (the desktop)
```

Every "which theme?" goes through it: `LayoutWindow::dynamic_selector_context` (via
`LayoutWindow::window_theme_for`), `initial_window_theme(_for)` at creation,
`CommonWindowState::resolved_window_theme` (desktop adoption + the app switch),
`CallbackInfo::get_resolved_color_scheme`. It is idempotent, so the context resolving again
what the shells already resolved into `ws.theme` is a no-op for them and keeps the E2E runner /
tests from bypassing the pin. The `system:` palette follows the RESOLVED scheme
(`colors_for_theme`, unchanged): a dark pin on a light desktop resolves `system:*` to the dark
keyword defaults; on a dark desktop, to the desktop's own dark palette.

### 2.3 Precedence between the app and a window - DECIDED: app > window

- There is no persistent per-window pin today. `WindowCreateOptions::theme` is a creation
  SEED ("the OS theme watchers keep following the system afterwards either way"), and
  `modify_window_state` with a new theme does not even write it (see 6.1).
- `set_color_scheme` is documented as switching EVERY window; a window-level pin above it
  would make "every" have invisible exceptions.
- So: env > app > the window's own seed (only while the app follows the desktop, and only until
  the desktop next changes) > desktop. Switching the app back to System follows the DESKTOP
  (the seed is not remembered).
- Open question for Felix: platform convention (NSWindow.appearance over NSApp.appearance,
  WinUI element RequestedTheme over Application, Android local night mode over default) puts a
  window-level pin ABOVE the app. If a real per-window pin is ever added, that is where it
  belongs; it would be one more parameter to `resolve_window_theme`.

### 2.4 Where the state lives

- **App choice:** process-global `APP_COLOR_SCHEME` (`set_app_color_scheme` /
  `app_color_scheme` in `layout/src/window.rs`, next to the other App-global settings).
  `App::create` publishes `AppConfig::color_scheme`; `SetColorScheme` writes it. Global because
  a window opened later must start in it and window creation has no path back to the window
  that switched.
- **Per-window mirror:** `LayoutWindow::color_scheme`, seeded from the global in
  `LayoutWindow::new`, updated by the switch. The cascade reads the MIRROR - layout tests pin
  one window without touching the global (tests run in parallel in one process).
- **Desktop:** `CommonWindowState::desktop_theme` (private, getter `desktop_theme()`), seeded
  from the probed `SystemStyle::theme`. `ws.theme` stays THE effective (resolved) theme - what
  `LayoutCallbackInfo::theme`, the DWM caption and every reader see.

### 2.5 The trigger (I7) and the rebuild rule

- `CommonWindowState::adopt_desktop_theme(desktop)`: THE seam all six appearance probes report
  through (macOS `adopt_probed_theme`, Linux `adopt_observed_theme`, Windows
  `WM_SETTINGCHANGE/WM_THEMECHANGED`, iOS `adopt_device_appearance`, Android
  `drain_pending_theme`, headless `set_system_theme`). Returns the window theme to write, or
  None when the desktop did not move or the app pins. macOS/Linux still re-discover the style
  when the desktop moved (pinned or not), so System lands on the right palette at once.
  Side effect (intended): a poll that re-asserts an unchanged desktop no longer overrides a
  window's `WindowCreateOptions::theme` seed (macOS used to override it on the first 2 s poll).
- `adopt_system_style` (the one trigger): `needs_full = (theme_delta &&
  layout-read-the-scheme) || (style_changed && system_style_change_needs_full_regeneration)`.
  A desktop flip on an app that follows it still rebuilds (the style changed, `old.theme !=
  new.theme`, unchanged policy); a window-theme delta ALONE is paint-only unless read.
- `system_style_change_needs_full_regeneration`: while pinned (app or env), a desktop polarity
  flip is not a theme change for the window; the rest is weighed under one polarity
  (`new.clone()` with `theme = old.theme`).
- `LayoutWindow::color_scheme_change_needs_new_dom()`: `layout_results.is_empty() ||
  recorded_style_dependencies.contains(Theme)`.

### 2.6 The runtime switch (`CallbackChange::SetColorScheme`, dll `apply_user_change`)

1. `set_app_color_scheme` (windows built from now on start in it);
2. `adopt_app_color_scheme_in_other_windows()` - per-shell registry walks (macOS, Win32 + DWM
   caption, X11, Wayland) calling `adopt_app_color_scheme()` on each OTHER window: mirror,
   snapshot, `ws.theme` write, `process_window_events(0)` (ThemeChanged), `adopt_system_style`
   (restyle or rebuild), redraw - the macOS appearance-notification shape. Headless / iOS /
   Android: default no-op (one window).
   Wayland: an `xdg_popup` and its parent own each other, so there the walk uses
   `adopt_app_color_scheme_deferred()` (theme written, delta discarded, `ThemeChange` rebuild
   requested at the next frame; no ThemeChanged event).
3. this window, MID-PASS (the `ModifyWindowState` shape): baseline = current, write, nested
   `process_window_events(0)`, drop incremental caches, return `ShouldIncrementalRelayout`
   (or `ShouldRegenerateDomCurrentWindow` + `request_regeneration(ThemeChange)` when layout()
   read the scheme). `DoNothing` when the window already shows the target.

The E2E runner (one window, no layout callback) sets the mirror, writes its own window state
and returns `ShouldIncrementalRelayout`; it never publishes the global.

## 3. Commits (in order)

| hash | what |
|---|---|
| `407c371d6` | RED: `layout/tests/app_color_scheme_override.rs` (+ `all.rs` append), unit test in `layout/src/callbacks.rs`, `dll/tests/color_scheme_headless.rs` |
| `96ee870ed` | progress checkpoint |
| `89e89374c` | engine: core `AppConfig` field + builders (+ padding test), layout global + resolver + `LayoutWindow::color_scheme` / `window_theme_for` / `color_scheme_change_needs_new_dom`, context, pinned desktop-flip rule, `CallbackChange::SetColorScheme`, `CallbackInfo` API, E2E runner arm, `get_theme` docs |
| `cbe937c0c` | shells: `initial_window_theme(_for)`, `desktop_theme` + `adopt_desktop_theme`, SetColorScheme handler, `adopt_system_style` rule, fan-out on 4 desktop shells (Wayland deferred), 6 probes, `App::create` publish, env guard on `headless_lifecycle::a_theme_switch_changes_the_theme_and_requests_a_frame` (under `AZ_THEME` the window now rightly keeps the pin), unit test `the_apps_pin_beats_the_windows_request_and_the_probe` |
| `636ee6bb6` | progress checkpoint |
| `159ba19d3` | demo toolbar (colour scheme + widget theme), `.with_theme(theme)` on every themed widget |
| `f6be2fc0f` | progress checkpoint |
| (this) | report + final checkpoint |

RED pass: revert `89e89374c` + `cbe937c0c` -> the three test targets fail to compile (new API)
- that is the RED. Behavioural RED with the API kept: in `89e89374c` revert only the
`dynamic_selector_context` hunk (pin not applied) and the pinned branch of
`system_style_change_needs_full_regeneration`; in `cbe937c0c` revert the `adopt_desktop_theme`
bodies to `(target != ws.theme).then_some(target)` without the pin.

Suites to run: `cargo test --release -p azul-core app_config_has_no_padding`;
`-p azul-layout --test all app_color_scheme`; `-p azul-layout --lib set_color_scheme_queues`;
`-p azul-dll --lib initial_window_theme`; `-p azul-dll --test color_scheme_headless` (NEW test
target, not in the usual list); `-p azul-dll --test headless_lifecycle`;
`-p azul-dll --test backend_feature_parity` (source scans the theme ingress names - kept);
`-p azul-layout --test all azul_widgets_demo` (the demo lint reads the demo source).

## 4. API list for autofix (api.json) - no new types

All docs are ASCII. Field order: `color_scheme` goes AFTER `global_hotkeys_callback` and
BEFORE `log_level` in `AppConfig` (8 B, align 4).

| module / class | item | signature |
|---|---|---|
| app / `AppConfig` | struct field | `color_scheme: OptionDarkLightMode` |
| app / `AppConfig` | fn `with_color_scheme` | `(self: value, scheme: OptionDarkLightMode) -> AppConfig` |
| app / `AppConfig` | fn `set_color_scheme` | `(self: refmut, scheme: OptionDarkLightMode)` |
| callbacks / `CallbackInfo` | fn `set_color_scheme` | `(self: refmut, scheme: OptionDarkLightMode)` |
| callbacks / `CallbackInfo` | fn `get_color_scheme` | `(self: ref) -> OptionDarkLightMode` |
| callbacks / `CallbackInfo` | fn `get_resolved_color_scheme` | `(self: ref) -> DarkLightMode` |

Rust-only (not for api.json): `azul_layout::window::{set_app_color_scheme, app_color_scheme,
resolve_window_theme, resolve_window_theme_with}`, `LayoutWindow::{color_scheme,
window_theme_for, color_scheme_change_needs_new_dom}`, `CallbackChange::SetColorScheme`,
`CommonWindowState::{desktop_theme, app_color_scheme, resolved_window_theme,
adopt_desktop_theme}`, the `PlatformWindow` defaults, `initial_window_theme_for`,
`desktop_window_theme`.

`AppConfig` grows by 8 bytes: api.json MUST be updated (autofix) before any binding build, or
every binding passes a too-small `AppConfig` by value. The demo needs the regenerated Rust
bindings for `CallbackInfo::set_color_scheme` and `AppConfig::with_color_scheme`
(generated as `set_color_scheme<I0: Into<AzOptionWindowTheme>>` etc.; the demo passes the value
directly, which fits either shape).

## 5. Demo wiring list for later (`examples/azul-widgets`)

Wired now (had `with_theme` at the branch point): TextInput, TextArea, Slider, Switch,
CheckBox, DropDown, Avatar, ProgressBar, and every `Button` (Display row x3, Tooltip's
"Hover me", Dialog invoker "Delete file...", dialog body "Keep"/"Delete", popover invoker
"Open popover", dock panel "A tool button", notifications "Post"/"Withdraw", hotkeys
toggle/"Retry"). The theme is `s.widget_theme` (`let theme = ...` at the top of `layout()`;
`dock_zones`, `dialog_body`, `notifications_section`, `hotkey_section` take it as a parameter).

TO WIRE once the base's themed widgets are merged in (`b9cc36bee` already has `with_theme` for
all of these) - add `.with_theme(theme)` before `.dom()`:

| section (lib.rs) | widget | note |
|---|---|---|
| Inputs | `NumberInput::create(s.number)` | |
| Inputs | `ColorInput::create(s.color)` | |
| Selection | `RadioGroup::create(..)` | |
| Selection | `Segmented::create(["Day","Week","Month"])` | |
| toolbar (`fn toolbar`) | both `Segmented`s | |
| Display | `Badge::with_kind` x3, `Chip::with_kind`, `Card::create`, `Divider::create`, `Spinner::create` | |
| Feedback | `Alert::with_kind`, `Tooltip::create`, `Dialog::create` (modal) | |
| Navigation | `Breadcrumb`, `Pagination`, `Stepper`, `Accordion` | |
| Overlays | `Dialog::create` (popover), `SplitPane::create` | |
| Date & Time | `TimePicker::create` | |

Still WITHOUT `with_theme` even on `b9cc36bee`: `ComboBox` (Selection), `DatePicker`
(Date & Time) - wire when they get it. Not used by the demo at all (so nothing to wire unless a
section is added): backstage, file_input, list_view, quick_access, ribbon, statusbar, tabs (the
demo's "Documents" tabs are hand-built divs), titlebar (hand-built), tree_view, and W1's new
input types.

## 6. Found on the way (not fixed - outside this task or in files I must not touch)

1. **`ModifyWindowState` drops `theme`** (`common/event.rs`, the `update_window_state(App, ..)`
   copy list): an app's `modify_window_state` with a new theme requests a `ThemeChange`
   regeneration but never writes the theme. Pre-existing; `set_color_scheme` is now the
   supported way.
2. **Widgets re-implement the decision** (`window_is_dark` in `segmented.rs`, `stepper.rs`,
   `pagination.rs`, `date_picker.rs`: env pin, else `ws.theme`). Consistent today because
   `ws.theme` is resolved, but they should call `info.get_resolved_color_scheme()` (I1).
   Worse: they live-restyle via `set_css_property` with colours BAKED for the current scheme;
   user overrides outrank the cascade, so after a restyle-only scheme switch a clicked
   Segmented/Stepper/Pagination keeps the old scheme's colours until the next DOM rebuild. The
   demo's scheme segment returns `RefreshDom`, which discards them for this page.
3. **Window clear colour under a pin**: `common/mod.rs::resolve_initial_background_color`
   picks light/dark from the DESKTOP theme; `wr_translate2` reads the unseeded
   `options.window_state.theme`; the CPU canvas uses `system_style.colors.window_background`
   (desktop). A pinned app whose body does not cover the window shows the desktop's canvas at
   the edges. Route all three through `colors_for_theme(resolved)`.
4. **macOS native chrome**: nothing sets `NSWindow.appearance`, so a native titlebar stays in
   the desktop appearance under a pin (Windows' DWM caption follows via `sync_window_state` /
   `apply_titlebar_theme`). Linux CSD: `titlebar.rs` picks the default title colour from
   `system_style.theme` (desktop).

## 7. Least sure to compile

1. `dll/src/desktop/shell2/{windows,linux/x11,linux/wayland}/mod.rs` fan-out overrides - copied
   from the neighbouring `request_regeneration_all_windows`, but uncompiled on those targets;
   `w.adopt_app_color_scheme()` / `p.adopt_app_color_scheme_deferred()` rely on the
   `PlatformWindow` trait being in scope there (it is imported at the top of each file);
   `p` is `&mut Box<WaylandPopup>` (auto-deref).
2. iOS / Android probe edits (`adopt_desktop_theme` on `common` / `window.common`) - cfg-gated,
   uncompiled here.
3. `layout/src/callbacks.rs`: `pub const fn get_color_scheme` reading a `Copy` field through the
   `const fn get_layout_window()` - should be fine; drop `const` if the compiler objects.
4. `core/src/resources.rs`: `crate::window::OptionDarkLightMode` in `AppConfig` - make sure the
   `window` module is compiled in every `core` feature set `resources` is.
5. The demo: `azul::option::OptionDarkLightMode`, `azul::window::{UiTheme, DarkLightMode}` paths
   (from the current generated `reexports.rs`), and the new binding methods (regen required).
6. Behavioural risk, not compile: the synchronous fan-out runs another window's event pass and
   restyle while the switching window's `&mut self` is on the stack (the same raw-pointer
   pattern as `request_regeneration_all_windows`, but deeper). A popup/menu window that
   switches the scheme on macOS/Win32/X11 makes its PARENT run a pass that could, in theory,
   reconcile (close) that popup. Only the Wayland popup case is structurally owned and uses
   the deferred path; if this bites elsewhere, switch those walks to
   `adopt_app_color_scheme_deferred` too (costs a rebuild instead of a restyle there).
