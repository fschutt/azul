//! The colour a window's canvas is cleared to, and the window background a mode derives: what every
//! renderer asks (the CPU frame, `WebRender`, the platform backdrop).

use azul_core::window::DarkLightMode;

/// The window background a MODE derives - THE derivation, for the creation
/// seed and for every mode change that moves a seeded background
/// (`CommonWindowState::move_mode_background`):
///
/// 1. the app's own background for that mode (`WindowCreateOptions::background_color_light` /
///    `background_color_dark`);
/// 2. else the system palette's window background for that mode, resolved the way the cascade
///    resolves `system:window-background` in a window of that mode:
///    `SystemStyle::colors_for_theme` (the desktop's own colour when the desktop is in that mode,
///    the keyword's default for it otherwise) - so the canvas matches a body painted in the
///    keyword.
///
/// Always a colour: `None` in a window's `background_color` means "no
/// background" (a material, an offscreen canvas), never "derive one".
#[must_use]
pub fn mode_background(
    mode: DarkLightMode,
    system_style: &azul_css::system::SystemStyle,
    light: azul_css::props::basic::OptionColorU,
    dark: azul_css::props::basic::OptionColorU,
) -> azul_css::props::basic::ColorU {
    let is_dark = mode == DarkLightMode::Dark;
    let own = if is_dark { dark } else { light };
    own.into_option().unwrap_or_else(|| {
        let theme = if is_dark {
            DarkLightMode::Dark
        } else {
            DarkLightMode::Light
        };
        azul_css::props::basic::color::SystemColorRef::WindowBackground
            .resolve_for_theme(&system_style.colors_for_theme(theme), is_dark)
    })
}

/// THE colour a window's canvas is cleared to - what shows wherever the app's
/// content does not paint. Every renderer asks here: the CPU compositor per
/// frame (`CpuBackend::render_frame`, the canvas of headless, macOS, X11,
/// Wayland, Windows, iOS and Android), `WebRender` at creation
/// (`wr_translate2::default_renderer_options`) and whenever the answer moves
/// (`CommonWindowState::sync_renderer_clear_color`).
///
/// In precedence order:
///
/// 1. `transparent` (a background material the platform composites): transparent black, so the
///    material shows through;
/// 2. the window's `background_color` - the app's own, or the one [`mode_background`] derived
///    for the window's mode (seeded at creation, moved with every mode change);
/// 3. `follow_system_background` (a real window on a desktop): the system palette's window
///    background for `mode` - the mode the window SHOWS, after the app / `AZ_THEME` pin, never the
///    desktop's palette as such;
/// 4. a fixed light / dark pair: offscreen output (screenshots, PDF export, the reference images
///    tests diff against) must not change colour with the machine that renders it.
///
/// An opaque answer is fully opaque: an opaque window has nothing behind it
/// to blend with.
#[must_use]
pub fn window_clear_color(
    background_color: azul_css::props::basic::OptionColorU,
    mode: DarkLightMode,
    system_style: Option<&azul_css::system::SystemStyle>,
    follow_system_background: bool,
    transparent: bool,
) -> azul_css::props::basic::ColorU {
    use azul_css::props::basic::{ColorU, OptionColorU};

    if transparent {
        return ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }
    let color = match (background_color.into_option(), system_style) {
        (Some(color), _) => color,
        (None, Some(style)) if follow_system_background => {
            mode_background(mode, style, OptionColorU::None, OptionColorU::None)
        }
        (None, _) if mode == DarkLightMode::Dark => ColorU {
            r: 42,
            g: 46,
            b: 50,
            a: 255,
        },
        (None, _) => ColorU::WHITE,
    };
    ColorU { a: 255, ..color }
}

