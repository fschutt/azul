# W4 scheme override + demo toggles - progress (branch wt/w4-scheme-override-demo, cut from fix/input-bugs-2026-09-19)

## DONE
- 407c371d6 RED tests: layout/tests/app_color_scheme_override.rs (+ all.rs append),
  layout/src/callbacks.rs unit test, dll/tests/color_scheme_headless.rs

- 89e89374c engine impl (core AppConfig field, layout global + resolver + LayoutWindow.color_scheme,
  dynamic_selector_context, CallbackChange::SetColorScheme + CallbackInfo API, e2e runner arm)
- cbe937c0c shell impl (dll: initial_window_theme(_for), CommonWindowState.desktop_theme +
  adopt_desktop_theme, SetColorScheme handler, adopt_system_style decision, fan-out overrides
  incl. Wayland deferred, OS probes on all 6 shells, App::create publish)

- 159ba19d3 demo toolbar (examples/azul-widgets: lib.rs, notifications.rs, hotkeys.rs)

- final report scripts/W4_SCHEME_OVERRIDE_DEMO_2026_09_29.md (committed with this checkpoint)

## IN PROGRESS
- nothing: task complete, waiting for the parent's autofix + compile + RED pass

## NEXT
- (parent) azul-doc autofix for the 6 API items in the report (AppConfig grows 8 B), codegen,
  compile, run the suites listed in the report (incl. the NEW `-p azul-dll --test
  color_scheme_headless`), RED pass by reverting 89e89374c + cbe937c0c
- (parent) after merging the base's themed widgets: wire the demo list in report section 5

## Design decisions (so a resumed session does not re-derive them)
- TYPE: reuse `OptionDarkLightMode` (None = follow the desktop, Some = pin). A new 4-byte repr(C)
  `ColorScheme` enum in AppConfig leaves 4 bytes of tail padding (the 4-aligned tail is 24 B;
  +4 = 28 -> 32) and fails `app_config_has_no_padding_between_its_fields`; OptionDarkLightMode is
  8 B / align 4 and lands on 32 exactly. It is also the window-level request's type already
  (`WindowCreateOptions.theme`).
- ONE decision fn: `azul_layout::window::resolve_window_theme(app, window)` = env AZ_THEME >
  app pin > the window's own (desktop-following) theme. Called by dynamic_selector_context (I1),
  initial_window_theme, CommonWindowState::resolved_window_theme, CallbackInfo getters.
- precedence: env > app > window request (creation seed, as today) > desktop.
- `ws.theme` stays THE effective theme (resolved). Desktop's own theme kept in
  `CommonWindowState.desktop_theme`, written by every OS probe through `adopt_desktop_theme`.
- app choice: process-global `APP_COLOR_SCHEME` (App::create publishes AppConfig.color_scheme,
  SetColorScheme writes it) + per-window mirror `LayoutWindow.color_scheme` (tests isolate on it).
- trigger (I7): ws.theme write -> adopt_system_style; a window-theme delta alone rebuilds only
  when `layout()` read the scheme (recorded Theme facet / Everything); else restyle.
- other windows: new trait method `adopt_app_color_scheme_in_other_windows` (per-shell registry
  loops: macOS, Win32, X11, Wayland window + popup); each calls `adopt_app_color_scheme`.

## Open questions
- none blocking
