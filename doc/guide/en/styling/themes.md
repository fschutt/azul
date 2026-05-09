---
slug: styling/themes
title: System Themes
language: en
canonical_slug: styling/themes
audience: external
maturity: wip
guide_order: 42
topic_only: false
short_desc: System colors, `@theme`, `@os`, and accessibility queries
prerequisites: [styling]
tracked_files:
  - css/src/system.rs
  - css/src/dynamic_selector.rs
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

- `@theme light`: system reports light theme.
- `@theme dark`: system reports dark theme.
- `@theme <name>`: a custom string treated as user-defined. The system
  resolver doesn't emit it on its own.

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

## Application-specific overrides

A few escape hatches when the discovery isn't enough:

- **Inline override**: a `with_css_property(...)` call wins over the
  cascade for that node.
- **Css override**: stack a second `Css` via `style(css)`. Later rule
  blocks win at equal `(priority, specificity)`.
- **End-user ricing**: when the `io` feature is enabled and
  `AZUL_DISABLE_RICING` is unset, azul reads
  `~/.config/azul/styles/<app>.css` (or `%APPDATA%\azul\styles\<app>.css`)
  at startup and applies it as the *last* stylesheet. This lets a user
  retheme an installed azul app without recompiling. Disable it for a
  given install with `AZUL_DISABLE_RICING=1`.

The Linux-specific `AZUL_SMOKE_AND_MIRRORS` env var skips the standard
GNOME or KDE detection and prefers the riced-desktop sources (Hyprland
config, pywal cache).

## Coming Up Next

- [Styling Text](text-and-fonts.md) — Font family, size, weight, alignment, decoration, and the system font keywords
- [Icon Packs](icon-packs.md) — Register icons and use them with `Dom::create_icon` or `<icon>`
- [Accessibility](../accessibility.md) — Screen reader integration and ARIA roles
