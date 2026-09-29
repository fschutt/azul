---
slug: styling/themes
title: System Themes
language: en
canonical_slug: styling/themes
audience: external
maturity: wip
guide_order: 72
topic_only: false
short_desc: System colors, `@theme`, `@os`, and accessibility queries
prerequisites: [styling]
tracked_files:
  - css/src/system.rs
  - css/src/dynamic_selector.rs
  - css/src/theme_chain.rs
  - css/src/props/basic/color.rs
  - css/src/props/basic/font.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - SystemStyle
  - SystemColors
  - SystemFontType
  - ThemeCondition
  - AccessibilitySettings
  - Css
  - ColorU
---

# System Themes

## Overview

*WIP.* Discovery (theme, accent, fonts, accessibility) is wired up across all desktop platforms. The user-facing CSS hooks (`system:*` colors, `system:*` fonts, `@theme dark`) work today. Some discovered values still arrive via CLI wrappers; the FFI-direct paths and ricing overrides are still being stabilized.

A native-feeling app reads its colors and fonts from the host OS.
Azul exposes those values through three CSS hooks:

- `system:<color>` for colors that follow the user's accent and theme.
- `system:<font>` for the platform's UI, monospace, or serif fonts.
- `@theme dark { ... }` and `@os <name>` for conditional rules that
  re-evaluate per frame when the user toggles dark mode or moves to a
  different desktop environment.

```css
background: system:window-background;
color: system:text;
font-family: system:ui;
border: 1px solid system:accent;
@theme dark {
    background: #1c1c1e;
    color: #f0f0f0;
}
```

```azul-render screenshot=themes-light slideshow=themes-toggle width=400 height=180 subtitle="Light theme"
<html>
<head><style>
body { font-family: sans-serif; padding: 20px; background: #fafafa; }
.card { background: white; color: #222; border: 1px solid #1976d2; padding: 16px; border-radius: 6px; }
</style></head>
<body><div class="card">Adaptive surface, light theme</div></body>
</html>
```

```azul-render screenshot=themes-dark slideshow=themes-toggle width=400 height=180 subtitle="Dark theme, OS-supplied palette"
<html>
<head><style>
body { font-family: sans-serif; padding: 20px; background: #0d0d0f; }
.card { background: #1c1c1e; color: #f0f0f0; border: 1px solid #0a84ff; padding: 16px; border-radius: 6px; }
</style></head>
<body><div class="card">Adaptive surface, dark theme</div></body>
</html>
```

## System colors

Use a `system:<name>` keyword wherever a color is accepted. The framework
resolves it at frame time using the user's current settings:

- `system:text`. Primary text color.
- `system:background`. Content background.
- `system:accent`. The user's accent color (Windows, macOS, GNOME).
- `system:accent-text`. Readable text on an accent fill.
- `system:button-face`. Button or control background.
- `system:button-text`. Button or control text.
- `system:window-background`. Window chrome background.
- `system:selection-background`. Selected-text background.
- `system:selection-text`. Selected-text foreground.

The resolver picks the user's current value if the OS reported one, and
falls back to a standard color otherwise. The full collection (link,
separator, grid, sidebar, and inactive-window variants) is on
`SystemColors`.

```rust,no_run
use azul::prelude::*;
let _ = Dom::create_button("Save", SmallAriaInfo::label("Save")).with_css("
    background: system:button-face;
    color: system:button-text;
    border: 1px solid system:accent;
    padding: 6px 14px;
    :hover { background: system:accent; color: system:accent-text; }
");
```

## System fonts

`system:<role>` keywords pick the right face on each platform.
`SystemFontType` enumerates the roles:

- `system:ui`. macOS: SF Pro Text. Windows: Segoe UI Variable. Linux: Cantarell.
- `system:ui:bold`. macOS: SF Pro Text Bold. Windows: Segoe UI Bold. Linux: Cantarell Bold.
- `system:monospace`. macOS: SF Mono or Menlo. Windows: Cascadia Mono or Consolas. Linux: Ubuntu Mono or DejaVu Sans Mono.
- `system:monospace:bold`. macOS: Menlo Bold. Windows: Cascadia Mono Bold. Linux: Ubuntu Mono Bold.
- `system:monospace:italic`. macOS: Menlo Italic. Windows: Cascadia Mono Italic. Linux: Ubuntu Mono Italic.
- `system:title`. macOS: SF Pro Display. Windows: Segoe UI Variable Display. Linux: Cantarell.
- `system:title:bold`. macOS: SF Pro Display Bold. Windows: Segoe UI Variable Display Bold. Linux: Cantarell Bold.
- `system:menu`. macOS: SF Pro Text. Windows: Segoe UI. Linux: Cantarell.
- `system:small`. macOS: SF Pro Text 11pt. Windows: Segoe UI 9pt. Linux: Cantarell 9pt.
- `system:serif`. macOS: New York. Windows: Cambria. Linux: DejaVu Serif.
- `system:serif:bold`. macOS: Georgia Bold. Windows: Cambria Bold. Linux: DejaVu Serif Bold.

The framework walks a fallback chain at font resolution and falls through
to the `sans-serif`, `monospace`, or `serif` generics if none match.

```css
font-family: system:ui;
font-size: 14px;
```

## @theme adaptation

`@theme <variant> { ... }` blocks evaluate per frame. The variant matches
the system's current preference (light or dark) and updates the moment the
user toggles their OS-wide setting (no DOM rebuild required):

```css
background: white;
color: #1a1a1a;
@theme dark {
    background: #1c1c1e;
    color: #f0f0f0;
}
@theme custom-high-contrast {
    background: black;
    color: yellow;
}
```

The variants follow `ThemeCondition`:

- `@theme light`: the window is in light mode.
- `@theme dark`: the window is in dark mode.
- `@theme <name>` / `@theme(<name>)`: the APP THEME, a name such as `flat`
  (the default) or `flora`. The block applies only while that theme is in
  the app's theme chain: `AppConfig::with_theme("flora")` at startup,
  `CallbackInfo::set_theme("flat")` at runtime, or the user's `AZ_THEME`
  (see [Choosing the theme and the mode from the
  environment](#choosing-the-theme-and-the-mode-from-the-environment)).
  Widgets carry one block per theme they know, so one switch restyles all
  of them. A theme switch rebuilds every window's DOM (a theme may change a
  widget's structure), while a light / dark mode switch only repaints. Nest
  `@theme dark` inside a theme block for that theme's dark mode.

The app theme is the head of a *theme chain*, most specific first, like a
locale fallback list (`fr-CA`, then `fr`, then `en`). A spin-off theme
names its base before a colon: `xyz:pink` expands to `xyz:pink`, then
`xyz`, and every chain ends in the default theme, `flat`. So
`AppConfig::with_theme("xyz:pink")` makes the chain
`[xyz:pink, xyz, flat]`: `@theme(xyz:pink)` blocks apply, `@theme(xyz)`
blocks apply where the spin-off says nothing, and an app theme nobody wrote
a block for looks like the default theme. A theme's file header can name
further fallbacks (`fallback: native`); they are appended in chain order,
each theme once, and a cycle is cut with a warning. The mode's words -
`light`, `dark`, `system`, `auto` - are never theme names: in a chain they
are an error, logged and dropped.

A THEME and a MODE are two settings. The theme is the app's look (`flat`,
`flora`, ...). The mode is light / dark / system: by default ("system") every
window follows the desktop's light or dark, and
`AppConfig::with_mode(OptionWindowTheme::Some(WindowTheme::DarkMode))` at
startup or `CallbackInfo::set_mode(..)` at runtime pins every window of the
app to one (`None` follows the desktop again). `CallbackInfo::get_mode` reads
that choice back and `CallbackInfo::get_resolved_mode` the light or dark it
gives. Inside `layout()`, `LayoutCallbackInfo::get_mode()` returns the light
or dark the window shows (and makes a mode switch re-run that `layout()`),
while `LayoutCallbackInfo::get_theme()` returns the app theme's name.
`RelayoutReason` says which one changed: `ModeChange` or `ThemeChange`.

For typical apps, define the base style for light mode and override
selected properties under `@theme dark`. Combine with `@os` for
platform-flavoured dark mode (a Mac-style sheen vs. a Windows-style flat
fill).

## @os

A single rule covers OS family, version, and Linux desktop environment.
The grammar is `@os(<family>[:<de>] [<op> <version>])`. Each clause is
optional; `op` is `>=`, `<=`, or `=`.

OS families:

- `windows`. Windows desktop.
- `macos`. macOS.
- `ios`. iOS.
- `apple`. macOS or iOS.
- `linux`. Any Linux desktop.
- `android`. Android.
- `web`. WASM target.
- `any`. Always matches.

Family-only rules also accept the bare-identifier form: `@os linux { … }`
is the same as `@os(linux) { … }`.

```css
/* family only */
@os(linux)               { font-family: 'Cantarell'; }
@os(windows)             { font-family: 'Segoe UI'; }

/* family + version — codename or bare number both work */
@os(windows >= 11)       { font-family: 'Segoe UI Variable Text'; }
@os(macos >= big-sur)    { font-family: '.SF NS'; }
@os(linux >= 6)          { /* kernel 6.0+ */ }

/* Linux desktop environment */
@os(linux:gnome)         { font-family: 'Cantarell'; }
@os(linux:kde)           { font-family: 'Noto Sans'; }

/* family + DE + DE version */
@os(linux:gnome > 40)    { padding-inline-start: 16px; }
```

Comparisons across OS families always evaluate to false.
`@os(macos >= sonoma)` on Windows is just inert, not a parse error.

Desktop-environment versions only match when the runtime knows the DE's
version number; until detection is wired up for a given DE, the
`@os(linux:de > N)` form will not match.

Version synonyms are accepted permissively: bare numbers (`11`),
prefixed forms (`win-11`, `win11`, `windows-11`), and codenames where
they exist (`big-sur`, `monterey`, `sonoma`) all map to the same
underlying version. Linux accepts `5`, `5.4`, and `5.4.10`.

## Accessibility queries

These map to the OS's accessibility settings. They live on
`AccessibilitySettings` and re-evaluate per frame.

```css
transition: background 200ms ease;
@media (prefers-reduced-motion) {
    transition: none;
}
@media (prefers-contrast) {
    background: black;
    color: white;
    border: 2px solid white;
}
```

- `@media (prefers-reduced-motion)`. Source: macOS `AXReduceMotion`, Windows `SPI_GETCLIENTAREAANIMATION`, Linux `enable-animations`.
- `@media (prefers-contrast)`. Source: macOS `AXIncreaseContrast`, Windows `SPI_GETHIGHCONTRAST`, Linux `high-contrast`.

Honour `prefers-reduced-motion` for any non-essential animation.

## @media viewport queries

Standard CSS:

```css
padding: 24px;
font-size: 16px;
@media (max-width: 640px) {
    padding: 12px;
    font-size: 14px;
}
@media (orientation: portrait) {
    flex-direction: column;
}
```

The viewport size comes from the current window. On a multi-window app
each window has its own viewport, evaluated independently.

## @lang(<bcp47>)

Match the system locale. Prefix matching: `@lang(de)` matches `de`,
`de-DE`, `de-AT`. Useful for locale-specific quotes, hyphenation, and
typographic conventions:

```css
quotes: '\u{201C}' '\u{201D}';
@lang(de) { quotes: '\u{201E}' '\u{201C}'; }
@lang(fr) { quotes: '\u{00AB} ' ' \u{00BB}'; }
```

The active locale field is `SystemStyle.language` (BCP 47, e.g.,
`"en-US"`).

## Reading discovered values from Rust

The full snapshot is `SystemStyle`. Every field ends up populating the
dynamic selectors and the `system:*` resolver. In Rust code you generally
don't need to touch it. Stick with `system:*` and `@theme` or `@os` in
your CSS. Those expressions stay ergonomic and re-evaluate automatically.

## How user theming layers with component CSS

The cascade has three layers, from outermost to innermost:

1. **System discovery** (the `system:*` keywords and `@theme dark`
   condition). Resolved per frame from the running OS, so a theme
   toggle takes effect on the next paint without a re-layout.
2. **End-user ricing** — the CSS files the user dropped into
   `~/.azul/css/<theme>/` (and the older per-app
   `~/.config/azul/styles/<app>.css`), each at the priority its header
   asks for, `base` by default. See [Ricing (User Themes)](ricing.md).
3. **Application CSS** — every component-level `Css` attached via
   `Dom::style(...)` on a subtree root, plus inline rules attached
   via `Dom::with_css(...)` on individual nodes. CSS lives on the
   tree the layout callback returns; there is no global stylesheet
   passed to `App::create`.

Components don't fight user theming because their selectors target
component-internal classes (`.shadcn-card`, `.my-row`) while user
theming targets the `system:*` color and font hooks. As long as a
component reads its colors from `system:*` instead of hard-coding
hex values, a user's rice can repaint the component without the
component's source changing.

A few escape hatches when the discovery isn't enough:

- **Inline override on a node**: `Dom::with_css_property(...)` wins
  the cascade for that node.
- **Subtree override via component CSS**: stack a second `Css` via
  `Dom::style(css)`. Later rule blocks win at equal
  `(priority, specificity)`. See [Styling › Two ways to attach
  styles](../styling.md#two-ways-to-attach-styles).

## Controlling end-user customization

Azul has a single env var for the entire end-user-customization
layer: `AZ_RICING`.

Unset (the default) means the framework loads the user's rice
([Ricing (User Themes)](ricing.md)) if there is any, and on Linux runs
the standard detection chain (`KDE > GNOME > riced-desktop > defaults`).
This is the right behavior for a normal install on a normal
desktop.

`AZ_RICING=off` (aliases: `disabled`, `none`, `0`) skips both the
rice and the riced-desktop sources. Pick this for a kiosk
build, a CI runner, any install that must not pick up local
theme customization, and before reporting a bug: a bug that
reproduces with `AZ_RICING=off` is the app's. The cascade still runs
`system:*` resolution and `@theme` conditions — disabling ricing only
stops the *user-supplied* layer; the OS-supplied palette is still
honored.

`AZ_RICING=watch` (aliases: `live`, `reload`) loads the rice and
rebuilds every window when a rice file changes, for writing a theme.

`AZ_RICING=force` (aliases: `prefer`, `aggressive`, `1`) reorders the
Linux detection chain so riced-desktop sources (Hyprland config,
pywal cache, i3/sway) win over the GNOME and KDE paths. Use this
when `XDG_CURRENT_DESKTOP` still reports `gnome` but the actual
session is a tiling WM with a custom palette. The rice still
loads in this mode.

## Choosing the theme and the mode from the environment

Two variables choose the look of every Azul app a user runs. Both are read
once, at startup.

"Theme" and "mode" are separate axes. The THEME is the app theme (`flat`,
`flora`, a user's `xyz:pink`); the MODE is light / dark / system.

- `AZ_THEME=<theme>` names the head of the theme chain, and outranks the
  app's own choice:

  ```text
  AZ_THEME  >  AppConfig::with_theme / CallbackInfo::set_theme  >  flat
  ```

  `AZ_THEME=xyz:pink` gives every window the chain `[xyz:pink, xyz, flat]`,
  whatever theme the app asked for; a `set_theme` call while it is set
  changes nothing.
- `AZ_MODE=light|dark|system` pins the mode, for deterministic rendering
  (screenshots, reftests, CI):

  ```text
  AZ_MODE  >  AppConfig::color_scheme / CallbackInfo::set_color_scheme  >  the window's  >  the desktop's
  ```

  It reaches everything that has a light / dark polarity: `@theme dark` and
  `prefers-color-scheme` blocks, the `system:*` palette, the window
  background. `system` (or `auto`) pins nothing: the app and the desktop
  decide, and azul re-evaluates `@theme` on the next frame when the user
  toggles the platform's own setting (macOS *General > Appearance*, Windows
  *Personalization > Colors > Choose your mode*, GNOME *Settings >
  Appearance*).

`AZ_THEME=light` and `AZ_THEME=dark` were the mode pin before `AZ_THEME`
named the theme. For one release they still pin the mode, and the app logs
once at startup that the spelling is deprecated and `AZ_MODE` replaces it.
When both are set, `AZ_MODE` decides the mode.

```bash
AZ_MODE=dark ./my_app              # dark, whatever the desktop says
AZ_THEME=xyz:pink ./my_app         # the xyz:pink theme, over xyz, over flat
AZ_THEME=flora AZ_MODE=light ./my_app
```

## Previewing on a different platform

Themes vary not just by light/dark but by OS conventions: a button on
iOS doesn't look like a button on Windows 11, and `system:*` colors
resolve differently on each. The `main()` function can override the
detected environment between `AppConfig::create()` and `App::create()`,
forcing the cascade to evaluate as if the app were running on a
different platform:

```rust,no_run
use azul::prelude::*;

fn main() {
    let mut config = AppConfig::create();

    // Pretend this app is running on iOS, dark theme, with a French
    // locale, on a 360x780 viewport. Useful for screenshot diffing,
    // designer review, or "does my app look OK on Android?" checks.
    config.mock_css_environment = OptionCssMockEnvironment::Some(
        CssMockEnvironment {
            os: OptionOsCondition::Some(OsCondition::Ios),
            theme: OptionThemeCondition::Some(ThemeCondition::Dark),
            language: OptionString::Some("fr-FR".into()),
            viewport_width: OptionF32::Some(360.0),
            viewport_height: OptionF32::Some(780.0),
            ..Default::default()
        }
    );

    // Or start from the detected style and override what you want to pin:
    // let mut style = SystemStyle::detect();
    // style.metrics.corner_radius = OptionF32::Some(0.0);
    // config.system_style = style;

    let app = App::create(initial_data, config);
    app.run(WindowCreateOptions::new(layout));
}
```

Every field on `CssMockEnvironment` is optional — the ones not set
fall back to auto-detected values. Combined with a `SystemStyle`
whose fields you set yourself (`SystemStyle::detect()` is the
starting point the FFI exposes; the curated platform presets in
`css::system::defaults` are Rust-internal),
this gives a single binary the ability to render its UI as if it
were running on any supported target — useful for screenshot
testing, designer review, and "what would my app look like, pixel by
pixel, on iOS?" sanity checks.

The override is read at app startup; it doesn't change the actual
runtime windowing backend (`AZ_BACKEND` does that, and that env var
isn't a theming concern — it picks `x11` vs `wayland` etc. for the
real OS the app is actually running on). For the full list of `AZ_*`
runtime env vars (debug server, profiling, layout tracing, headless
rendering), see [Debugging](../debugging.md).
