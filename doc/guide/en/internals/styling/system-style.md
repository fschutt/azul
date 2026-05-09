---
slug: internals/styling/system-style
title: System Style Discovery
language: en
canonical_slug: internals/styling/system-style
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Discovering OS theme, accent, fonts, and a11y settings
prerequisites: [code-organization]
tracked_files:
  - css/src/system.rs
  - dll/src/desktop/shell2/linux/system_style.rs
  - dll/src/desktop/shell2/macos/system_style.rs
  - dll/src/desktop/shell2/windows/system_style.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - SystemStyle
  - SystemColors
  - Platform
  - InputMetrics
  - AppConfig
  - ColorU
---

# System Style Discovery

## Overview

`SystemStyle` is the shared bag of OS-derived values — colours, fonts, scrollbar look, double-click time, reduced-motion preference, accent colour, OS version. *WIP — the runtime discovery paths in `dll/src/desktop/shell2/*/system_style.rs` work but are platform-pinned (no portable abstraction yet); the compile-time defaults in `css::system::defaults` are stable.* It is `#[repr(C)]`, FFI-safe, populated once at app start, and read everywhere as `Arc<SystemStyle>`. CSD, menu rendering, scrollbar drawing, focus ring drawing, and most widgets all consult it.

The struct serves four roles: it provides the user-agent default values that go into the cascade (system colours, system fonts, scrollbar metrics), it carries OS-level metrics callbacks may want to read (double-click interval, scroll wheel lines), it exposes the user's accessibility preferences (`prefers_reduced_motion`, `prefers_high_contrast`), and it holds an optional `app_specific_stylesheet` for ricing — a CSS file at `~/.config/azul/styles/<exe-name>.css` that's loaded at start and applied last in the cascade.

This page describes the shape of the struct, where each platform's discovery happens, and the compile-time defaults that act as fallback. For the cascade machinery that consumes UA values, see [Cascade, Inheritance, Restyle](cascade.md).

## The shape of SystemStyle

```rust,ignore
#[repr(C)]
pub struct SystemStyle {
    pub fonts: SystemFonts,
    pub metrics: SystemMetrics,
    pub linux: LinuxCustomization,
    pub platform: Platform,
    pub focus_visuals: FocusVisuals,
    pub language: AzString,                 // BCP 47, e.g. "en-US"
    pub app_specific_stylesheet: Option<Box<Css>>,
    pub scrollbar: Option<Box<ComputedScrollbarStyle>>,
    pub scroll_physics: ScrollPhysics,
    pub theme: Theme,                        // Light | Dark
    pub os_version: OsVersion,
    pub prefers_reduced_motion: BoolCondition,
    pub prefers_high_contrast:  BoolCondition,
    pub accessibility: AccessibilitySettings,
    pub input: InputMetrics,
    pub text_rendering: TextRenderingHints,
    pub scrollbar_preferences: ScrollbarPreferences,
    pub visual_hints: VisualHints,
    pub animation: AnimationMetrics,
    pub colors: SystemColors,
    pub icon_style: IconStyleOptions,
    pub audio: AudioMetrics,
}
```

`Box<Css>` and `Box<ComputedScrollbarStyle>` are heap-indirected so the struct's FFI size is stable across feature flags.

`Platform` is one of `Windows | MacOs | Linux(DesktopEnvironment) | Android | Ios | Unknown`. `Platform::current()` is the compile-time `cfg(target_os)` answer. The runtime discovery in `dll/` overrides it on Linux to fill in the actual desktop env.

## The discovery pipeline

There is no portable `discover()` in `azul-css`. The crate exposes only `SystemStyle::detect()`, which is a thin wrapper over the compile-time defaults:

```rust,ignore
pub fn detect() -> Self {
    Self::default_for_platform()
}

pub fn default_for_platform() -> Self {
    #[cfg(target_os = "windows")] { defaults::windows_11_light() }
    #[cfg(target_os = "macos")]   { defaults::macos_modern_light() }
    #[cfg(target_os = "linux")]   { defaults::gnome_adwaita_light() }
    #[cfg(target_os = "android")] { defaults::android_material_light() }
    #[cfg(target_os = "ios")]     { defaults::ios_light() }
    // ...
}
```

Real OS discovery lives in `azul-dll`. The single dispatch point is `discover_system_style`:

```rust,ignore
pub(crate) fn discover_system_style() -> azul_css::system::SystemStyle {
    #[cfg(target_os = "macos")]   { shell2::macos::system_style::discover() }
    #[cfg(target_os = "windows")] { shell2::windows::system_style::discover() }
    #[cfg(target_os = "linux")]   { shell2::linux::system_style::discover() }
    #[cfg(not(...))]              { azul_css::system::SystemStyle::detect() }
}
```

`App::create` calls this once and stores the result in the `AppConfig`. Per-platform priority chains:

- **macOS.** dlopen AppKit, then Objective-C runtime, then fall back to `defaults::macos_modern_*` if dlopen fails.
- **Windows.** LoadLibrary `user32.dll` and `dwmapi.dll`, then fall back to `defaults::windows_11_light` if any DLL fails.
- **Linux.** XDG Desktop Portal (D-Bus, raw socket), then CLI discovery (`gsettings`, `kreadconfig5`, Hyprland config, pywal cache), then `defaults::gnome_adwaita_light`.

Every backend **starts** by cloning a hard-coded default and then mutates fields based on what the OS actually returned. This means a query failure for a single value (e.g. accent colour) leaves the rest of the style intact.

## macOS: dlopen + Objective-C

```rust,ignore
pub(crate) fn discover() -> SystemStyle {
    let lib = match ObjcLib::load() {
        Some(l) => l,
        None => return defaults::macos_modern_light(),
    };
    let mut style = defaults::macos_modern_light();
    unsafe {
        // 1. theme   — [[NSApplication sharedApplication] effectiveAppearance]
        // 2. colours — [NSColor labelColor], etc. (15 semantic colours)
        // 3. fonts   — [NSFont systemFontOfSize:0], monospacedSystemFontOfSize:weight:
        // 4. input   — [NSEvent doubleClickInterval]
        // 5. scrolls — [NSScroller preferredScrollerStyle]
        // 6. a11y    — [[NSWorkspace sharedWorkspace] accessibilityDisplay…]
        // 7. version — [[NSProcessInfo processInfo] operatingSystemVersion]
        // 8. locale  — [[NSLocale currentLocale] localeIdentifier]
    }
    // visual_hints fixed by HIG: show_button_images = false, show_menu_images = true
    style
}
```

`ObjcLib` is a hand-rolled `dlopen` wrapper that resolves `objc_msgSend`, `objc_getClass`, and `sel_registerName` from `libobjc.A.dylib` plus the `NS*` symbols from `AppKit.framework`. **No `objc2` linkage**. This code path runs even if the user disables every feature. The fallback notes that `objc_msgSend` returning floats is ABI-different on x86_64 (`fpret`); the implementation targets arm64 and accepts default-value fallback on x86_64.

The Apple version-numbering jump is encoded literally:

```rust,ignore
26 => OsVersion::MACOS_TAHOE,    // Apple skipped 16-25 in 2025
15 => OsVersion::MACOS_SEQUOIA,
14 => OsVersion::MACOS_SONOMA,
// ...
```

## Windows: LoadLibrary + GetProcAddress

```rust,ignore
pub(crate) fn discover() -> SystemStyle {
    let u32_lib = match User32::load() { /* loads user32.dll */ };
    let mut style = defaults::windows_11_light();
    // GetDoubleClickTime / GetSystemMetrics(SM_CXDOUBLECLK, SM_CXDRAG)
    // GetCaretBlinkTime
    // SystemParametersInfoW(SPI_GETCARETWIDTH | SPI_GETWHEELSCROLLLINES |
    //                       SPI_GETMOUSEHOVERTIME | SPI_GETFONTSMOOTHING |
    //                       SPI_GETFONTSMOOTHINGTYPE)
    // GetSysColor(COLOR_WINDOW=5, COLOR_WINDOWTEXT=8, COLOR_HIGHLIGHT=13,
    //             COLOR_HIGHLIGHTTEXT=14, COLOR_BTNFACE=15, COLOR_BTNTEXT=18,
    //             COLOR_GRAYTEXT=17)
    style
}
```

ClearType is detected via `SPI_GETFONTSMOOTHINGTYPE`. When the smoothing type is `FE_FONTSMOOTHINGCLEARTYPE` the subpixel layout is set to `Rgb` (ClearType's horizontal default), otherwise `None`. The BGR and vertical variants of `SubpixelType` are not currently produced by Windows discovery.

## Linux: D-Bus first, then CLI, then defaults

```rust,ignore
pub(crate) fn discover() -> SystemStyle {
    // 1. XDG Desktop Portal via raw D-Bus
    if let Some((color_scheme, accent_rgb)) = query_xdg_portal() {
        let mut style = match color_scheme {
            1 => defaults::gnome_adwaita_dark(),
            _ => defaults::gnome_adwaita_light(),
        };
        if let Some((r, g, b)) = accent_rgb {
            style.colors.accent = OptionColorU::Some(ColorU::new_rgb(...));
        }
        discover_linux_extras(&mut style);  // gsettings: cursor theme etc.
        // ... + language, OS version, reduced-motion, ricing
        return style;
    }

    // 2. CLI discovery
    let force_riced = matches!(
        azul_css::system::ricing_mode(),
        azul_css::system::RicingMode::Force,
    );
    let mut style = if force_riced {
        // AZ_RICING=force: try riced first
        discover_riced_style()
            .or_else(|_| discover_kde_style())
            .or_else(|_| discover_gnome_style())
            .unwrap_or_else(|_| defaults::gnome_adwaita_light())
    } else {
        let de = detect_linux_desktop_env();
        match &de {
            DesktopEnvironment::Kde   => discover_kde_style().or_else(|_| discover_gnome_style())...,
            DesktopEnvironment::Gnome => discover_gnome_style().or_else(|_| discover_kde_style())...,
            DesktopEnvironment::Other(_) => discover_riced_style()
                .or_else(|_| discover_gnome_style())
                .or_else(|_| discover_kde_style())...,
        }
    };
    // ... + ricing
    style
}
```

### XDG Desktop Portal (raw D-Bus)

The Linux discovery path implements just enough of the D-Bus wire protocol to call `org.freedesktop.portal.Settings.Read`, with no `zbus` or `dbus` crate dependency. It reads two keys from `org.freedesktop.appearance`: `color-scheme` (uint32: 0 / 1 / 2 = no-pref / dark / light) and `accent-color` (variant of three doubles).

### Per-DE CLI discovery

- **`discover_gnome_style`.** Reads from `gsettings get org.gnome.desktop.interface ...` for color-scheme, gtk-theme, font-name, monospace-font-name, accent-color, cursor-theme, and cursor-size.
- **`discover_kde_style`.** Reads from `kreadconfig5 --file kdeglobals --group ... --key ...` for `ColorScheme`, `Font`, `ColorEffects:Disabled`, etc.
- **`discover_riced_style`.** Parses `$XDG_CONFIG_HOME/hypr/hyprland.conf`, `$HOME/.cache/wal/colors.json`, `i3/config`, and `sway/config`.

`AZ_RICING=force` reorders the chain so a tiling-WM user with a GNOME session set in `XDG_CURRENT_DESKTOP` still gets their pywal palette. `AZ_RICING=off` skips the riced sources entirely.

### Linux extras

`discover_linux_extras` runs after either path completes and fills in fields that aren't in the portal's API but ARE in `gsettings`: cursor theme, cursor size, icon theme, GTK theme name, and titlebar button layout (`"close,minimize,maximize:"`).

## Compile-time defaults

`css::system::defaults` has constructors that each return a fully-populated `SystemStyle`. They serve four roles:

1. Backend fallback when dlopen / D-Bus fails.
2. Headless / test rendering.
3. The `feature = "io"` is off and runtime discovery is unavailable.
4. Nostalgia themes that aren't reachable from real OS settings.

Available constructors:

- **Modern.** `windows_11_light` / `dark`, `macos_modern_light` / `dark`, `gnome_adwaita_light` / `dark`, `kde_breeze_light`.
- **Mobile.** `android_material_light`, `ios_light`.
- **Nostalgia.** `windows_7_aero`, `windows_xp_luna`, `macos_aqua`, `gtk2_clearlooks`, `android_holo_dark`.

The nostalgia constructors are public so applications can opt-in via `SystemStyle::with_*` if they want a vintage theme. They aren't reachable from runtime discovery. `OsVersion::WIN_XP` is never produced by `discover()`.

Each constructor uses `..Default::default()` to fill the long tail of fields. This is safe because `SystemStyle` derives `Default` and every nested type (`InputMetrics`, `AnimationMetrics`, `AccessibilitySettings`, ...) implements its own `Default` with sensible values (e.g. `InputMetrics::double_click_time_ms = 500`).

## App-specific ricing

After discovery succeeds, every Linux path calls:

```rust,ignore
fn load_app_specific_stylesheet() -> Option<Css> {
    if !azul_css::system::ricing_enabled() { return None; }
    let exe_name = std::env::current_exe()?.file_stem()?.to_string_lossy().into_owned();
    let config_dir = get_config_dir()?;          // $XDG_CONFIG_HOME or ~/.config
    let css_path = format!("{}/azul/styles/{}.css", config_dir, exe_name);
    let css_str = std::fs::read_to_string(&css_path).ok()?;
    let (css, _warnings) = new_from_str(&css_str);
    if css.is_empty() { None } else { Some(css) }
}
```

The result lands in `app_specific_stylesheet`. Parser warnings are discarded. Invalid user CSS does not abort discovery.

- **Linux.** `$XDG_CONFIG_HOME/azul/styles/<exe>.css`, else `~/.config/azul/styles/<exe>.css`.
- **macOS.** `~/Library/Application Support/azul/styles/<exe>.css`.
- **Windows.** `%APPDATA%\azul\styles\<exe>.css`.

`exe` is the `Path::file_stem()` of the running executable. `myapp` matches both `myapp` and `myapp.exe`.

## Css generators on SystemStyle

`SystemStyle` carries two CSS-emitting methods used by the menu / CSD pipeline (see [Menus and Client-Side Decorations](../windowing/menus-and-csd.md)):

- **`create_csd_stylesheet() -> Css`.** Emits `.csd-titlebar`, `.csd-title`, `.csd-buttons`, `.csd-button`, `.csd-button:hover`, `.csd-close:hover`, plus macOS and Linux specialisations.
- **`create_menu_stylesheet() -> Css`.** Emits `.menu-container`, `.menu-item`, `.menu-item:hover`, `.menu-item-disabled` / `-greyed`, `.menu-item-icon`, `.menu-item-label`, `.menu-item-shortcut`, `.menu-item-arrow`, and `.menu-separator`. Defined as an extension trait `SystemStyleMenuExt`.

Both build a `String` and parse it back to a `Css` via `parser2::new_from_str`, then tag every rule with `rule_priority::SYSTEM` so author CSS overrides win. This is fragile — `format!` typos are caught only at parse time, not compile time. Errors are log-routed via `log_debug!(LogCategory::General, ...)` but the build still succeeds with whatever rules parsed correctly.

## Detecting the desktop environment and language

Two helpers in `azul-css` work without `azul-dll`:

- `detect_linux_desktop_env` checks `XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, then specific markers (`GNOME_DESKTOP_SESSION_ID`, `KDE_FULL_SESSION`, `HYPRLAND_INSTANCE_SIGNATURE`, `SWAYSOCK`, `I3SOCK`). Returns `DesktopEnvironment::{Gnome, Kde, Other(name)}`.
- `detect_system_language` checks `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG` in priority, strips `.UTF-8` suffixes and `:`-separated alternatives, and normalises `de_DE` to `de-DE`. Returns `"en-US"` on failure. Native discovery overrides this on macOS (via `NSLocale`) and Windows.

## Where SystemStyle is read

The struct is consumed in:

- `dll/src/desktop/csd.rs` for titlebar styling and the menu-bar dropdown callback.
- `dll/src/desktop/menu_renderer.rs` for menu colour and font lookup.
- `layout/src/widgets/scrollbar.rs` for scrollbar visual style and `ScrollbarPreferences.visibility`.
- `layout/src/widgets/titlebar.rs` for `Titlebar::from_system_style_csd` and `tm.buttons` / `tm.button_side`.
- `layout/src/managers/scroll_state.rs` for `ScrollPhysics` (momentum and overscroll) per platform.
- Anywhere a callback wants the live system theme: every `CallbackInfo` exposes `get_system_style() -> Arc<SystemStyle>`.

Not by reading `SystemStyle` directly, but by walking `Arc<SystemStyle>`. Cloning is one atomic refcount bump, so widgets pass the Arc around freely.

## Adding a field

The cross-cutting checklist when extending `SystemStyle`:

1. Add the field to the struct in `css/src/system.rs`. Make it `Default`-able.
2. Update `SystemStyle::to_json_string` so debug output matches.
3. Set the field in every `defaults::*` constructor that should differ from `Default`.
4. Wire each native discovery path:
   - macOS: `dll/src/desktop/shell2/macos/system_style.rs::discover`
   - Windows: `dll/src/desktop/shell2/windows/system_style.rs::discover`
   - Linux: pick the right helper inside `dll/src/desktop/shell2/linux/system_style.rs` (`discover_gnome_style`, `discover_kde_style`, `discover_linux_extras`, etc.)
5. If exposed via FFI, add a `repr(C)` Option-wrapper if the type isn't already FFI-safe, and regenerate `api.json`.

## See also

- [Cascade, Inheritance, Restyle](cascade.md) — UA values from `SystemStyle` feed the cascade as priority-1 defaults.
- [Menus and Client-Side Decorations](../windowing/menus-and-csd.md) — consumer of `create_csd_stylesheet` and `create_menu_stylesheet`.
- [Accessibility Backends](../windowing/accessibility.md) — consumer of `accessibility` and `prefers_*` fields.
- [Styling Subsystem](../styling.md) — parent overview of the styling pipeline.

## Coming Up Next

- [Accessibility Backends](../windowing/accessibility.md) — Per-platform a11y back-ends - UIA, AT-SPI, NSAccessibility
- [Shell2 Common Layer](../windowing/common.md) — Shared shell infrastructure across platforms
- [Cascade, Inheritance, Restyle](cascade.md) — Selector matching, specificity, and computed values
