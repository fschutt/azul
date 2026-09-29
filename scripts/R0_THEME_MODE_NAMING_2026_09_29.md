# R0 - theme vs mode naming (2026-09-29)

Branch `wt/r0-theme-mode-naming`, from `0a326afe5`. Step 0 of the theme refactor.

User ruling: "make a split between „theme" and „mode" (dark / light / system). Thats the naming
consistency". **Theme** = the app theme (flat / flora / native / user themes). **Mode** = light /
dark / system.

## What changed

| old | new |
|---|---|
| `LayoutCallbackInfo::get_theme() -> DarkLightMode` | `LayoutCallbackInfo::get_mode() -> DarkLightMode` |
| `LayoutCallbackInfo::get_theme_name() -> AzString` | `LayoutCallbackInfo::get_theme() -> AzString` |
| `CallbackInfo::set_color_scheme(scheme)` | `CallbackInfo::set_mode(mode)` |
| `CallbackInfo::get_color_scheme()` | `CallbackInfo::get_mode()` |
| `CallbackInfo::get_resolved_color_scheme()` | `CallbackInfo::get_resolved_mode()` |
| `AppConfig.color_scheme` (same position, still padding-free) | `AppConfig.mode` |
| `AppConfig::with_color_scheme(scheme)` / `set_color_scheme(scheme)` | `AppConfig::with_mode(mode)` / `set_mode(mode)` |
| `CallbackChange::SetColorScheme { scheme }` | `CallbackChange::SetMode { mode }` |
| `RelayoutReason::ThemeChange` (light / dark, value 3) | `RelayoutReason::ModeChange = 3` |
| `RelayoutReason::AppThemeChange` (value 6, after `Other`) | `RelayoutReason::ThemeChange = 6` |

`RelayoutReason` now has an explicit value on every variant (0..=6, order unchanged).
`animates_moves` stays exhaustive: `Resize | ModeChange | ThemeChange => false`.

**Binding users:** `RelayoutReason::ThemeChange` now means the APP theme (value 6). A callback
that matched `ThemeChange` for a light / dark rebuild must now match `ModeChange`. It still
compiles, but it now fires on the other event.

Internals renamed so they match:

- `layout/src/window.rs`: `APP_COLOR_SCHEME` -> `APP_MODE`, `set_app_color_scheme` ->
  `set_app_mode`, `app_color_scheme` -> `app_mode`, `resolve_window_theme(_with)` ->
  `resolve_window_mode(_with)`, `LayoutWindow.color_scheme` -> `LayoutWindow.mode`,
  `window_theme_for` -> `window_mode_for`, `color_scheme_change_needs_new_dom` ->
  `mode_change_needs_new_dom`.
- dll: `CommonWindowState::app_color_scheme` -> `app_mode`, `resolved_window_theme` ->
  `resolved_window_mode`. The `PlatformWindow` methods `color_scheme_change_tier` ->
  `mode_change_tier`, `mirror_app_color_scheme` -> `mirror_app_mode`, and
  `adopt_app_color_scheme(_deferred / _in_other_windows)` -> `adopt_app_mode(_deferred /
  _in_other_windows)`. The four backend overrides changed too (macOS, Windows, X11, Wayland ×2).
  - The light / dark rebuilds (`adopt_system_style`, `mode_change_tier`,
    `adopt_app_mode_deferred`, iOS, Android, headless `set_system_theme`) are now tagged
    `ModeChange`. This is the old value 3, so nothing behaves differently.
  - The app theme's rebuild (`SetTheme` arm, `common/layout.rs`) is tagged `ThemeChange`.
- The E2E runner's `SetMode` arm.
- The doc comment of every renamed item, and the comments next to it, now call light / dark a
  "mode". The one misplaced doc comment (`mirror_app_color_scheme`'s, which sat above
  `color_scheme_change_tier`) moved onto `mirror_app_mode`.

Tests:

- New: `dll/tests/theme_and_mode_are_two_names.rs`, the RED commit. It pins the API shape:
  - the fn-pointer signatures of every renamed method;
  - `AppConfig::default().with_mode(..)` sets `.mode` and leaves `.theme` alone;
  - the `RelayoutReason` values 0..=6;
  - a headless run: `layout()` reads `get_mode()` (light) and `get_theme()` (`"flat"`). An
    `AfterMount` callback reads `get_mode()` (`None`) and `get_resolved_mode()` (light), then calls
    `set_mode(dark)`. The window turns dark, `LayoutWindow.mode == Some(Dark)`, and the rebuilt
    `layout()` reads dark.
- `dll/tests/color_scheme_headless.rs` became **`dll/tests/mode_headless.rs`** (`git mv`). Its
  test names, helpers and assertion texts now say "mode" (`a_mode_switch_restyles_...`,
  `a_layout_that_read_the_mode_is_rebuilt_...`, `a_window_opened_after_the_switch_starts_in_the_apps_mode`,
  `modify_window_state_with_a_new_mode_switches_the_window`).
- `dll/tests/backend_feature_parity.rs`: the scan key is now `RelayoutReason::ModeChange`, for
  the backends and for `adopt_system_style`'s body. Without that change it would fail.
- `dll/tests/app_theme_headless.rs`: uses `get_theme` and `RelayoutReason::ThemeChange`.
- `layout/tests/app_color_scheme_override.rs`: identifiers and the three "scheme" test names now
  say "mode". The FILE NAME is unchanged, so `layout/tests/all.rs` stays untouched (the other
  session edits it).
- `core/src/callbacks_test.rs`:
  - `get_theme_declares_the_polarity_...` -> `get_mode_declares_the_mode_...`;
  - `get_theme_name_answers_the_scope_...` -> `get_theme_answers_the_scope_and_leaves_the_mode_getter_alone`;
  - the round-trip test covers all seven reasons.
- `layout/src/callbacks.rs` unit test: `set_color_scheme_queues_...` ->
  `set_mode_queues_the_choice_and_the_getters_tell_choice_from_result`.
- `core/src/resources_test.rs`: `mode` in the padding destructure.

Other changes:

- Demos:
  - AzWidgets: the toolbar segment is labelled "Mode" (a11y name "Mode"), and the code uses
    `on_mode`, `mode_index`, `set_mode`, `get_mode` and `AppConfig::with_mode`.
  - AzWriter: `info.get_mode()`.
- Guide:
  - `doc/guide/en/styling/themes.md`: `@theme light` / `@theme dark` are the window's mode. A new
    paragraph explains theme vs mode and names each API.
  - `doc/guide/en/styling.md`: the `@theme <variant>` line says the same.
- Preflight: `scripts/preflight_contracts.py` check 8, `mode-naming`. It fails on any `pub fn`
  whose name contains `color_scheme` in `core/src`, `layout/src` or `dll/src` (comments are
  stripped first). The private `color-scheme` readers of the XDG portal stay.
  - Verified against `0a326afe5`: it finds the 9 old fns.
  - At this branch's tip it finds none: `python3 scripts/preflight_contracts.py` prints OK.

## Commits

| hash | what |
|---|---|
| `edcacd820` | RED: `dll/tests/theme_and_mode_are_two_names.rs` (does not compile before the rename) |
| `fb98155ea` | core: `LayoutCallbackInfo`, `RelayoutReason`, `AppConfig`, core tests |
| `e3ab1387c` | layout: `CallbackInfo`, `CallbackChange::SetMode`, window.rs internals, e2e runner, tests |
| `f7d933ddc` | dll: app.rs, event.rs, common/layout.rs, every shell, dll tests (incl. the `git mv`), demos |
| `bcbb6008d` | guide + preflight check 8 |
| `d49475cbc` | `get_mode`'s doc comment made ASCII (it is copied into api.json) |
| (this) | this report + the progress file |

## api.json (the parent applies these; nothing was edited by hand)

**Until these are applied, azul-dll does not compile, not even its integration tests.** Its
default features include `link-static`, which pulls in `cabi_export` and the generated
`target/codegen` code. That generated C-ABI calls `object.get_theme_name()`,
`object.get_color_scheme()` and the other old names, and it expects
`LayoutCallbackInfo::get_theme` to return `DarkLightMode`. Apply the items one method at a time
(`autofix add` wipes the patch dir), then run `codegen all` (set `AZ_CODEGEN_DIR` in a worktree).
Only then run the dll tests. azul-core and azul-layout compile without the api.json update.

Methods (`autofix remove` the old one, then `autofix add` the new one, which copies the doc from
the now-ASCII Rust source). Order matters for the `LayoutCallbackInfo` pair:

1. `LayoutCallbackInfo.get_theme` -> `LayoutCallbackInfo.get_mode`
   - Remove the old `get_theme` FIRST.
   - Returns `DarkLightMode`, fn_body `object.get_mode()`.
   - The old entry had `"priority": 70.0`; carry it over if wanted.
2. `LayoutCallbackInfo.get_theme_name` -> `LayoutCallbackInfo.get_theme`
   - Returns `String`, fn_body `object.get_theme().into()`.
3. `CallbackInfo.get_color_scheme` -> `CallbackInfo.get_mode` (returns `OptionDarkLightMode`).
4. `CallbackInfo.get_resolved_color_scheme` -> `CallbackInfo.get_resolved_mode` (returns
   `DarkLightMode`).
5. `CallbackInfo.set_color_scheme` -> `CallbackInfo.set_mode`
   - Arg `mode: OptionDarkLightMode`, fn_body `object.set_mode(mode)`.
6. `AppConfig.with_color_scheme` -> `AppConfig.with_mode`
   - `self: value`, arg `mode: OptionDarkLightMode`, returns `AppConfig`.
7. `AppConfig.set_color_scheme` -> `AppConfig.set_mode`
   - `self: refmut`, arg `mode: OptionDarkLightMode`.

Types (the plain `autofix` drift pass should find both; order preserved via
`replace_struct_fields` / `replace_enum_variants`):

8. `AppConfig.color_scheme` -> `AppConfig.mode`. Same position, between `theme` and
   `log_level`, type `OptionDarkLightMode`.
9. `RelayoutReason` variants, in order:
   `[Initial, RefreshDom, Resize, ModeChange, RouteChange, Other, ThemeChange]`.
   - Was `[..., ThemeChange, RouteChange, Other, AppThemeChange]`.
   - The values are unchanged: `RelayoutReason.ThemeChange` -> `RelayoutReason.ModeChange` (3),
     `RelayoutReason.AppThemeChange` -> `RelayoutReason.ThemeChange` (6).

Doc-only refreshes. Plain sync does not refresh docs: re-add the method, or patch the doc lines.

10. `CallbackInfo.get_theme`: its doc names `LayoutCallbackInfo::get_theme_name`. Re-add it; the
    Rust doc now says `LayoutCallbackInfo::get_theme`.
11. `CallbackInfo.set_theme`: its doc names `Self::set_color_scheme` and
    `RelayoutReason::AppThemeChange`. Re-add it; the Rust doc now says `Self::set_mode` /
    `RelayoutReason::ThemeChange`.
12. `LayoutCallbackInfo.depends_on_system_style`: api.json line ~19797 says "use
    `get_theme()`". Change it to "use `get_mode()`" **by patch**. Do NOT re-add this one: its
    Rust doc has pre-existing em dashes.
13. `SystemStyleDependency.Theme` variant doc (~54385): "(`LayoutCallbackInfo.get_theme()`)" ->
    "(`LayoutCallbackInfo.get_mode()`)".
14. `RelayoutReason` type doc: "theme toggle" -> "light / dark mode switch, app theme switch"
    (as in the Rust doc).

### Examples calling the old generated names

- `examples/azul-widgets/src/lib.rs` (`set_color_scheme`, `with_color_scheme`, `get_theme`) and
  `examples/azul-writer/src/lib.rs` (`get_theme`). Both are updated on this branch, so they build
  only after the api.json items above plus `codegen all`.
- No example in any other language calls these names. I grepped `examples/` for `getTheme`,
  `get_theme`, `GetTheme`, `getThemeName`, `colorScheme`, `color_scheme`, `ColorScheme`,
  `SetColorScheme`, `ThemeChange`, `AppThemeChange` and `RelayoutReason`. The only other hits
  were the widget `with_theme(UiTheme)` / `MapTheme` builders, which are unrelated.
- `doc/src/docgen` has a JS `getTheme()`: that is the website's own light / dark toggle, not
  the binding.

## Least sure to compile

1. `RelayoutReason` now has explicit discriminants (`Initial = 0` under `#[default]`, ...). This
   is valid Rust. It is the first enum in core/css/layout with explicit values; syn-based
   autofix reads only the idents.
2. `dll/tests/theme_and_mode_are_two_names.rs`:
   - fn-pointer coercions of methods, including the `const fn CallbackInfo::get_mode` and the
     `#[allow(clippy::unused_self)] LayoutCallbackInfo::get_theme`;
   - `matches!(change, CallbackChange::SetMode { mode } if mode == PIN_DARK)`;
   - `*self.callback_seen.lock()` copying an `Option<(OptionDarkLightMode, DarkLightMode)>` (all
     parts `Copy`).
3. Behaviour risk in that test's headless run (not a compile risk). `set_mode` is called from an
   `AfterMount` callback. Its `SetMode` arm runs a nested `process_window_events(0)` from inside
   `dispatch_pending_lifecycle_events`. This is the same path `ModifyWindowState` takes from any
   callback, but no test drove it from a lifecycle callback before. If the third
   `regenerate_layout` does not see dark, the test's frame-2 assertion is the one to look at.
4. The dll only compiles once api.json is patched (see above). That is expected, not a bug.

## Test commands (parent)

```
cargo test --release -p azul-core --lib callbacks
cargo test --release -p azul-core --lib app_config_has_no_padding_between_its_fields
cargo test --release -p azul-layout --lib set_mode_queues_the_choice
cargo test --release -p azul-layout --test all app_color_scheme_override
cargo test --release -p azul-dll --test theme_and_mode_are_two_names
cargo test --release -p azul-dll --test mode_headless        # was: --test color_scheme_headless
cargo test --release -p azul-dll --test app_theme_headless
cargo test --release -p azul-dll --test backend_feature_parity
cargo test --release -p azul-dll --lib --features build-dll the_apps_pin_beats
python3 scripts/preflight_contracts.py
```

After the api.json items and `codegen all`, build AzWidgets and AzWriter as well.

RED pass: `edcacd820` alone does not compile. The rename is what makes it compile, so a
revert-to-RED is not meaningful for this change. The preflight check was verified RED on
`0a326afe5` (9 hits) and is GREEN at the tip.

## Files other tasks own that I touched (rename lines only)

- `dll/src/desktop/shell2/common/event.rs` (C1). Touched only:
  - `initial_window_theme` doc and body;
  - `app_mode`, `resolved_window_mode` and their call sites;
  - the `ModifyWindowState` resolve, `mode_change_tier` call, `SetMode` arm, `SetTheme` tag,
    the `adopt_system_style` comment and tags;
  - the `mode_change_tier` / `mirror_app_mode` / `adopt_app_mode*` block;
  - one test doc and the `desktop_theme` field doc.
- The shells (C1). Touched only the `adopt_app_mode*` overrides and calls and the
  `RelayoutReason` tags, plus comment lines naming the app's pin:
  - `macos/mod.rs`, `macos/system_style.rs`;
  - `windows/mod.rs`;
  - `linux/x11/mod.rs`, `linux/wayland/mod.rs`, `linux/system_style.rs`;
  - `ios/mod.rs`, `android/mod.rs`, `headless/mod.rs`.
- Not touched: `layout/src/widgets/{segmented,stepper,pagination,date_picker}.rs` (V1),
  `css/src/dynamic_selector.rs` (R2, R3), the env vars (`AZ_THEME`, `AZ_RICING`), the CSS
  syntax, `layout/tests/all.rs`, `page_breaks.rs`.

## Left: the follow-up TYPE sweep (not done - types were out of scope)

Approximate use counts per crate (regex counts, tests and comments included):

| name | core | css | layout | dll | examples | doc | api.json |
|---|---|---|---|---|---|---|---|
| `DarkLightMode` (type) | 46 | 0 | 137 | 105 | 20 | 2 | 11 |
| `OptionDarkLightMode` | 6 | 0 | 39 | 45 | 6 | 2 | 8 |
| `DarkLightMode::Light/DarkMode` | 37 | 0 | 96 | 69 | 15 | 1 | 0 |
| css `Theme::Light/Dark` (`azul_css::system::Theme`) | 1 | 34 | 63 | 49 | 0 | 0 | 0 |
| `ThemeCondition::Light/Dark` | 75 | 74 | 59 | 1 | 0 | 1 | 0 |
| `SystemStyleDependency::Theme` | 11 | 0 | 6 | 0 | 1 | 0 | 0 |
| `EventType::ThemeChange` | 5 | 0 | 6 | 1 | 0 | 0 | 0 |
| `WindowEventFilter::ThemeChanged` / `ThemeChanged` | 7 | 0 | 0 | 9 | 0 | 4 | 1 |
| `.theme` field reads (FullWindowState / WindowCreateOptions / SystemStyle / LayoutCallbackInfo / DynamicSelectorContext) | 27 | 24 | 334 | 96 | 0 | 19 | 0 |
| `initial_window_theme(_for)` | 0 | 0 | 1 | 17 | 0 | 0 | 0 |
| `desktop_theme` / `adopt_desktop_theme` / `desktop_window_theme` | 0 | 0 | 7 | 25 | 0 | 0 | 0 |
| `HeadlessWindow::set_system_theme` | 0 | 0 | 0 | 13 | 0 | 1 | 0 |
| `MapColorScheme` (map widget) | 0 | 0 | 48 | 0 | 0 | 0 | 0 |

Proposed names:

- `DarkLightMode { LightMode, DarkMode }` -> `WindowMode { Light, Dark }`, and
  `OptionDarkLightMode` -> `OptionWindowMode`.
- The fields `FullWindowState.theme`, `WindowCreateOptions.theme` and
  `LayoutCallbackInfo.theme` -> `.mode`.
- css `system::Theme` -> `system::Mode`, and `SystemStyle.theme` -> `.mode`.
- `ThemeCondition::Light/Dark` stays in the `@theme(dark)` syntax (R2 / R3 own it). The
  `DynamicSelectorContext.theme` field could become `.mode`.
- `SystemStyleDependency::Theme` -> `::Mode`.
- `EventType::ThemeChange` / `WindowEventFilter::ThemeChanged` -> `ModeChange` / `ModeChanged`.
- `initial_window_theme(_for)` -> `initial_window_mode(_for)`, `desktop_theme` ->
  `desktop_mode`, `adopt_desktop_theme` -> `adopt_desktop_mode`, `set_system_theme` ->
  `set_system_mode`.
- `MapColorScheme` -> `MapMode`.

Other things left open:

- **Twins**, for the type sweep to unify. `dll/.../event.rs::desktop_window_theme` maps
  `azul_css::system::Theme` -> `DarkLightMode`, and the same match is hand-rolled four more times:
  - `linux/system_style.rs:~3222`, `macos/system_style.rs:~964`, `windows/mod.rs:~6834`;
  - `layout/src/e2e/runner.rs:~3517`;
  - plus two layout tests.

  One `From<Theme> for WindowMode` in core would replace all of them.
- `layout/tests/app_color_scheme_override.rs` could become `app_mode_override.rs`. That needs a
  one-line `#[path]` / `mod` change in `layout/tests/all.rs`, which I left alone.
- Pre-existing oddity: iOS tags a keyboard-inset change as the light / dark rebuild. It was
  `ThemeChange` and is now `ModeChange`, same value. Worth its own reason (`Other`?) later.
- `doc/guide/en/styling/themes.md` still says "There is no env var that forces dark/light mode"
  (`AZ_THEME` exists). R3 owns the env vars.
- Historical files are not rewritten: dated `scripts/*.md` reports and
  `doc/THEME_CHAIN_ANALYSIS_2026_09_12.md`.
