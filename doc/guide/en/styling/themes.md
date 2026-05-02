---
slug: styling/themes
title: System Themes
language: en
canonical_slug: styling/themes
audience: external
maturity: wip
guide_order: 42
topic_only: false
short_desc: System-aware styling — `system:*` colors and fonts, `@theme` light/dark, `@os`, and accessibility queries that re-evaluate per frame.
prerequisites: [styling]
tracked_files:
  - css/src/system.rs
  - css/src/dynamic_selector.rs
  - css/src/props/basic/color.rs
  - css/src/props/basic/font.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# System Themes

> **WIP** — discovery (theme, accent, fonts, accessibility) is wired up
> across all desktop platforms. The user-facing CSS hooks (`system:*`
> colors, `system:*` fonts, `@theme dark`) work today. Some discovered
> values still arrive via CLI wrappers; the FFI-direct paths and ricing
> overrides are still being stabilized.

A native-feeling app reads its colors, fonts, and metrics from the host OS.
Azul exposes those values through three CSS hooks:

- `system:<color>` for colors that follow the user's accent and theme.
- `system:<font>` for the platform's UI / monospace / serif fonts.
- `@theme dark { ... }` and `@os <name>` for conditional rules that
  re-evaluate per frame when the user toggles dark mode or moves to a
  different desktop environment.

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    background: system:window-background;
    color: system:text;
    font-family: system:ui;
    border: 1px solid system:accent;
    @theme dark {
        background: #1c1c1e;
        color: #f0f0f0;
    }
");
```

```azul-render screenshot=themes-light slideshow=themes-toggle width=400 height=180 subtitle="Light theme — system colors resolve to OS defaults"
<html>
<head><style>
body { font-family: sans-serif; padding: 20px; background: #fafafa; }
.card { background: white; color: #222; border: 1px solid #1976d2; padding: 16px; border-radius: 6px; }
</style></head>
<body><div class="card">Adaptive surface — light theme</div></body>
</html>
```

```azul-render screenshot=themes-dark slideshow=themes-toggle width=400 height=180 subtitle="Dark theme — same selectors, OS-supplied palette"
<html>
<head><style>
body { font-family: sans-serif; padding: 20px; background: #0d0d0f; }
.card { background: #1c1c1e; color: #f0f0f0; border: 1px solid #0a84ff; padding: 16px; border-radius: 6px; }
</style></head>
<body><div class="card">Adaptive surface — dark theme</div></body>
</html>
```

## System colors

Use a `system:<name>` keyword wherever a color is accepted. The framework
resolves it at cascade time using the user's current settings — no work on
your side. The set is `SystemColorRef` at `css/src/props/basic/color.rs:879`:

| Keyword | Resolves to |
|---|---|
| `system:text` | Primary text color |
| `system:background` | Content background |
| `system:accent` | The user's accent color (Windows / macOS / GNOME) |
| `system:accent-text` | Readable text on an accent fill |
| `system:button-face` | Button / control background |
| `system:button-text` | Button / control text |
| `system:window-background` | Window chrome background |
| `system:selection-background` | Selected-text background |
| `system:selection-text` | Selected-text foreground |

The resolver picks the user's current value if the OS reported one, and
falls back to a standard color if the discovery returned `None`. The full
collection — including link, separator, grid, sidebar, and inactive-window
variants — is on `SystemColors` (`css/src/system.rs:320`).

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_button("Save", SmallAriaInfo::label("Save")).with_css("
    background: system:button-face;
    color: system:button-text;
    border: 1px solid system:accent;
    padding: 6px 14px;
    :hover { background: system:accent; color: system:accent-text; }
");
```

## System fonts

`system:<role>` keywords pick the right face on each platform. From
`SystemFontType` at `css/src/system.rs:172`:

| Keyword | macOS | Windows | Linux (default) |
|---|---|---|---|
| `system:ui` | SF Pro Text | Segoe UI Variable | Cantarell |
| `system:ui:bold` | SF Pro Text Bold | Segoe UI Bold | Cantarell Bold |
| `system:monospace` | SF Mono / Menlo | Cascadia Mono / Consolas | Ubuntu Mono / DejaVu Sans Mono |
| `system:monospace:bold` | Menlo Bold | Cascadia Mono Bold | Ubuntu Mono Bold |
| `system:monospace:italic` | Menlo Italic | Cascadia Mono Italic | Ubuntu Mono Italic |
| `system:title` | SF Pro Display | Segoe UI Variable Display | Cantarell |
| `system:title:bold` | SF Pro Display Bold | Segoe UI Variable Display Bold | Cantarell Bold |
| `system:menu` | SF Pro Text | Segoe UI | Cantarell |
| `system:small` | SF Pro Text 11pt | Segoe UI 9pt | Cantarell 9pt |
| `system:serif` | New York | Cambria | DejaVu Serif |
| `system:serif:bold` | Georgia Bold | Cambria Bold | DejaVu Serif Bold |

Fallback chains live in `SystemFontType::get_fallback_chain`
(`css/src/system.rs:1057`); the framework walks the chain at font
resolution and falls through to `sans-serif` / `monospace` / `serif`
generics if none match.

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    font-family: system:ui;
    font-size: 14px;
");
let _ = Dom::create_pre().with_css("
    font-family: system:monospace;
    font-size: 13px;
");
```

## `@theme` adaptation

`@theme <variant> { ... }` blocks evaluate per frame. The variant matches
the system's current preference (light/dark) and updates the moment the
user toggles their OS-wide setting — no `regenerate_dom()` required. The
match table is `ThemeCondition` at `css/src/dynamic_selector.rs:601`:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
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
");
```

The variants:

- `@theme light` — system reports light theme.
- `@theme dark` — system reports dark theme.
- `@theme <name>` — custom string passed to `AppConfig::theme_override`
  (treat this as user-defined; the system resolver doesn't emit it).

For typical apps, define the base style for light mode and override
selected properties under `@theme dark`. Combine with `@os` for
platform-flavoured dark mode (a Mac-style sheen vs a Windows-style flat
fill).

## `@os` and `@os-version`

`@os <name> { ... }` matches the host platform. The name set is
`OsCondition` at `css/src/dynamic_selector.rs:185`:

| Name | Matches |
|---|---|
| `windows` | Windows desktop |
| `macos` | macOS |
| `ios` | iOS |
| `apple` | macOS or iOS |
| `linux` | any Linux desktop |
| `android` | Android |
| `web` | WASM target |
| `any` | always matches |

`@os-version` narrows further. Versions use named constants — readable
enough that you don't need to remember NT numbers:

```rust,no_run
# use azul::prelude::*;
// CSS-level: @os-version(>= win-11) or @os-version(linux gnome)
let _ = Dom::create_div().with_css("
    font-family: 'Segoe UI';
    @os-version(>= win-11) { font-family: 'Segoe UI Variable Text'; }
    @os-version(>= macos-bigsur) { font-family: '.SF NS'; }
    @os-version(linux gnome) { font-family: 'Cantarell'; }
    @os-version(linux kde) { font-family: 'Noto Sans'; }
");
```

The full version constant set is in `css/src/dynamic_selector.rs:307`
(`WIN_*`, `MACOS_*`, `IOS_*`, `LINUX_*`, ...). Comparisons across OS
families always evaluate to false — `@os-version(>= macos-sonoma)` on
Windows is just inert, not a parse error.

## Accessibility queries

These map to the OS's accessibility settings. They live in
`AccessibilitySettings` (`css/src/system.rs:275`) and re-evaluate per frame.

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    transition: background 200ms ease;
    @media (prefers-reduced-motion) {
        transition: none;
    }
    @media (prefers-contrast) {
        background: black;
        color: white;
        border: 2px solid white;
    }
");
```

| Query | Source |
|---|---|
| `@media (prefers-reduced-motion)` | macOS `AXReduceMotion`, Windows `SPI_GETCLIENTAREAANIMATION`, Linux `enable-animations` |
| `@media (prefers-contrast)` | macOS `AXIncreaseContrast`, Windows `SPI_GETHIGHCONTRAST`, Linux `high-contrast` |

Honour `prefers-reduced-motion` for any non-essential animation. The OS
settings page is the canonical opt-out — apps that ignore it fail
platform-store certifications.

## `@media` viewport queries

Standard CSS:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    padding: 24px;
    font-size: 16px;
    @media (max-width: 640px) {
        padding: 12px;
        font-size: 14px;
    }
    @media (orientation: portrait) {
        flex-direction: column;
    }
");
```

The viewport size comes from the current window. On a multi-window app each
window has its own viewport, evaluated independently.

## `@lang(<bcp47>)`

Match the system locale. Prefix matching: `@lang(de)` matches `de`,
`de-DE`, `de-AT`. Useful for locale-specific quotes, hyphenation, and
typographic conventions:

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_div().with_css("
    quotes: '\u{201C}' '\u{201D}';
    @lang(de) { quotes: '\u{201E}' '\u{201C}'; }
    @lang(fr) { quotes: '\u{00AB} ' ' \u{00BB}'; }
");
```

The active locale is `SystemStyle::language` (BCP 47, e.g., `"en-US"`).
Detection runs at startup; a future revision will subscribe to
locale-change notifications.

## Reading discovered values from Rust

The full snapshot is `SystemStyle` (`css/src/system.rs:93`). Every field
ends up populating the dynamic selectors and the `system:*` resolver. From
a callback you can read it via `CallbackInfo::get_system_style()`:

```rust,ignore
# use azul::prelude::*;
extern "C" fn cb(_data: RefAny, info: CallbackInfo) -> Update {
    let sys = info.get_system_style();
    let accent = sys.colors.accent.as_option().copied();
    let dark = sys.theme == Theme::Dark;
    // ... pick a Dom shape based on it ...
    Update::DoNothing
}
```

In Rust code you generally don't need to. Stick with `system:*` and
`@theme` / `@os` in your CSS — those expressions stay ergonomic and
re-evaluate automatically.

## Application-specific overrides

Three escape hatches when the discovery isn't enough:

- **Inline override**: a `with_css_property(...)` call wins over the
  cascade for that node.
- **Stylesheet override**: stack a second `Css` via `style(css)` — later
  stylesheets win at equal specificity.
- **End-user ricing**: when the `io` feature is enabled and
  `AZUL_DISABLE_RICING` is unset, azul reads
  `~/.config/azul/styles/<app>.css` (or `%APPDATA%\azul\styles\<app>.css`)
  at startup and applies it as the *last* stylesheet. This lets a user
  retheme an installed azul app without recompiling. Disable it for a
  given install with `AZUL_DISABLE_RICING=1`.

The Linux-specific `AZUL_SMOKE_AND_MIRRORS` env var skips the standard
GNOME / KDE detection and prefers the riced-desktop sources (Hyprland
config, pywal cache).

## Where to read the source

- `css/src/system.rs:93` — `SystemStyle` (top-level snapshot)
- `css/src/system.rs:320` — `SystemColors` (the resolved palette)
- `css/src/system.rs:172` — `SystemFontType` and platform fallback chains
- `css/src/system.rs:275` — `AccessibilitySettings`
- `css/src/props/basic/color.rs:879` — `SystemColorRef` (`system:*` parsing)
- `css/src/dynamic_selector.rs:50` — `DynamicSelector` (the at-rule AST)
- `css/src/dynamic_selector.rs:601` — `ThemeCondition`
- `css/src/dynamic_selector.rs:185` — `OsCondition`
- `css/src/dynamic_selector.rs:307` — OS version constants
