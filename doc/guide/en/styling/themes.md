---
slug: styling/themes
title: System Themes
language: en
canonical_slug: styling/themes
audience: external
maturity: wip
guide_order: 42
topic_only: false
prerequisites: [styling]
tracked_files:
  - css/src/system.rs
  - css/src/dynamic_selector.rs
  - css/src/props/basic/color.rs
  - css/src/props/basic/font.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T17:30:00Z
---

# System Themes

> **WIP** — the discovery surface (theme, accent, fonts, accessibility) is
> wired up and parses. The user-facing CSS hooks (`system:*` colors,
> `system:*` fonts, `@theme`) work today. App-level overrides (`AppConfig`
> mock environment, ricing files) are still being stabilized.

The styled DOM resolves theme-dependent values against a `SystemStyle`
populated at startup by `App::new`. Apps can either let CSS reference the
detected values via `system:*` keywords / `@theme` blocks, or read the
`SystemStyle` directly from `AppConfig::system_style` to make Rust-side
decisions.

## SystemStyle

`SystemStyle` (`css/src/system.rs:93`) is the single struct that holds
everything Azul knows about the host environment:

```rust,ignore
pub struct SystemStyle {
    pub fonts: SystemFonts,
    pub colors: SystemColors,
    pub metrics: SystemMetrics,
    pub theme: Theme,                   // Light | Dark
    pub platform: Platform,             // Windows | MacOs | Linux(de) | ...
    pub language: AzString,             // BCP 47, e.g. "en-US"
    pub accessibility: AccessibilitySettings,
    pub prefers_reduced_motion: BoolCondition,
    pub prefers_high_contrast: BoolCondition,
    pub scroll_physics: ScrollPhysics,
    pub focus_visuals: FocusVisuals,
    pub animation: AnimationMetrics,
    pub input: InputMetrics,
    pub text_rendering: TextRenderingHints,
    pub icon_style: IconStyleOptions,
    pub audio: AudioMetrics,
    pub linux: LinuxCustomization,
    pub app_specific_stylesheet: Option<Box<Stylesheet>>,
    // ...
}
```

To get the current style:

```rust,ignore
use azul::prelude::*;
let s = SystemStyle::detect();        // hard-coded defaults per OS
println!("theme = {:?}", s.theme);
println!("accent = {:?}", s.colors.accent);
```

`SystemStyle::detect()` returns compile-time defaults appropriate for the
current platform. Live discovery (querying NSColor, gsettings,
SystemParametersInfo, fontconfig) happens in the desktop shell and the
result is stored in `AppConfig::system_style` before windows open. Override
discovery in tests by assigning to `AppConfig::system_style` before calling
`App::new`.

## Theme detection in CSS

Wrap rules in `@theme(dark)` or `@theme(light)` to switch on the user's
preference:

```css
body {
    background: white;
    color: #111;
}

@theme(dark) {
    body {
        background: #1e1e1e;
        color: #ddd;
    }
}
```

Conditions stack with `@media`:

```css
@media (max-width: 600px) {
    @theme(dark) {
        .sidebar { background: #000; }
    }
}
```

The condition is evaluated against `SystemStyle::theme` at layout time. A
window can override the theme via `WindowCreateOptions::theme`.

## System colors

CSS color values can name a system color rather than a literal RGB triple.
The reference is resolved against `SystemStyle::colors`
(`css/src/props/basic/color.rs:879`):

| Keyword | Resolves to |
|---|---|
| `system:text` | Primary text color |
| `system:background` | Document background |
| `system:accent` | User's accent color |
| `system:accent-text` | Text color on accent surfaces |
| `system:button-face` | Button background |
| `system:button-text` | Button text |
| `system:window-background` | Window chrome background |
| `system:selection-background` | Selected text background |
| `system:selection-text` | Selected text foreground |

```css
button {
    background: system:button-face;
    color: system:button-text;
    border: 1px solid system:accent;
}

button:focus {
    outline: 2px solid system:accent;
}

::selection {
    background: system:selection-background;
    color: system:selection-text;
}
```

When the host doesn't expose a particular system color (e.g. minimal Linux
desktops), the resolver falls back to the literal color the property was
declared with, or to a sensible per-property default.

## System fonts

Font families can also reference roles instead of fixed family names. These
match the `SystemFontType` keywords in `css/src/system.rs:172`:

| Keyword | Role |
|---|---|
| `system:ui` | Default UI font (SF Pro, Segoe UI, Cantarell) |
| `system:ui:bold` | Bold variant of the UI font |
| `system:monospace` | Code / terminal font (SF Mono, Consolas, Ubuntu Mono) |
| `system:monospace:bold`, `system:monospace:italic` | Monospace variants |
| `system:title` | Window title font |
| `system:title:bold` | Bold variant |
| `system:menu` | Menu item font |
| `system:small` | Caption / small UI font |
| `system:serif`, `system:serif:bold` | Reading-content serif (New York, Georgia) |

```css
body { font-family: "system:ui", sans-serif; }
code { font-family: "system:monospace", monospace; }
.dialog-title { font-family: "system:title:bold"; font-size: 14px; }
```

The literal family names listed after the system keyword act as fallbacks
when the platform can't supply that role.

## Accessibility queries

Reduced-motion and high-contrast preferences gate animation rules:

```css
.spinner {
    animation: rotate 1s linear infinite;
}

@media (prefers-reduced-motion: reduce) {
    .spinner { animation: none; }
}

@media (prefers-contrast: more) {
    body { color: black; background: white; }
    button { border-width: 2px; }
}
```

These conditions are AND-combined with the surrounding `@media` predicate.
The current value comes from `SystemStyle::prefers_reduced_motion` and
`SystemStyle::prefers_high_contrast`.

## Per-OS rules

When a rule should only apply on a specific platform, use `@os`:

```css
button {
    border-radius: 4px;
}

@os(macos) {
    button { border-radius: 6px; }
}

@os(linux) {
    button {
        font-family: "system:ui";
        padding: 6px 14px;
    }
}

@os-version(windows >= win-11) {
    .titlebar { backdrop-filter: blur(20px); }
}
```

Recognized OS names: `windows`, `macos`, `linux`, `android`, `ios`. Version
comparisons accept both numeric (`>= 11`) and codename (`>= sonoma`) forms;
see `css/src/dynamic_selector.rs:422` for the codename map.

## Reading SystemStyle from Rust

When CSS isn't enough — for example, when computing a derived color or
choosing an icon set — read the `SystemStyle` directly. The recommended
pattern is to clone it out of `AppConfig` at startup and store the relevant
fields inside the application's `RefAny` so callbacks can reach them:

```rust,ignore
use azul::prelude::*;

struct AppData {
    accent: ColorU,
    is_dark: bool,
}

fn main() {
    let mut config = AppConfig::default();
    let style = SystemStyle::detect();
    let data = RefAny::new(AppData {
        accent: style.colors.accent
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 120, 215)),
        is_dark: matches!(style.theme, Theme::Dark),
    });
    config.system_style = style;
    let app = App::new(data, config);
    app.run(WindowCreateOptions::new(layout));
}
```

`SystemStyle` is `Clone` and FFI-safe, so it can be stashed inside
application data when constructed once at startup. For most apps,
`@theme(dark)` and `system:*` keywords in CSS handle theme switching
without ever reaching into the struct.

## App-specific user override

When the `io` feature is enabled, Azul also looks for a user-supplied
stylesheet at:

| Platform | Path |
|---|---|
| macOS, Linux | `~/.config/azul/styles/<app_name>.css` |
| Windows | `%APPDATA%\azul\styles\<app_name>.css` |

If found, it's parsed and stored in
`SystemStyle::app_specific_stylesheet`. Apps can append it to their own
`Css` to let end users theme the app:

```rust,ignore
let mut css = Css::from_string(BUILTIN_CSS.into());
if let Some(user) = app_config.system_style.app_specific_stylesheet.as_ref() {
    css.stylesheets.push((**user).clone());
}
let styled = body.style(css);
```

Disable this lookup with `AZUL_DISABLE_RICING=1` in the environment.

## Where to look next

- [Styling with CSS](../styling.md) — the cascade, selectors, attaching CSS.
- [CSS Properties Cheatsheet](properties.md) — every property and value.
- [Accessibility](../accessibility.md) — exposing UI to assistive tech
  alongside the prefers-* queries above.
