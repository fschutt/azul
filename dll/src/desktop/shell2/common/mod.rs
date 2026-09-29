//! Common platform-agnostic code shared by all shell2 platform backends
//! (macOS, Linux/Wayland, Linux/X11, Windows).
//!
//! # Submodules
//!
//! - **accessibility** — cross-platform accessibility action queue (headless / iOS / Android)
//! - **compositor** — GPU/software compositor selection and rendering context
//! - **cpu_compositor** — CPU-only fallback compositor
//! - **dlopen** — Runtime dynamic library loading
//! - **error** — Error types for compositor, dlopen, and window operations
//! - **debug_server** — Built-in debug/inspector server
//! - **event** — Window event handling and hit-testing
//! - **layout** — Layout generation and incremental relayout

pub mod compositor;
pub mod cpu_compositor;
pub mod dlopen;
pub mod error;
pub mod gl_loader;

// Unified cross-platform modules
pub mod accessibility;
pub mod capability_pump;
pub mod clipboard;
pub mod debug_server;
#[cfg(feature = "e2e-test")]
pub mod e2e_test;
pub mod event;
pub mod layout;
/// The runtime gate for every `log_*!` macro, plus RAII enter/exit spans.
/// Logging is gated here by atomics — never by a cargo feature.
pub mod log_gate;
pub mod seats;
pub mod transient;
/// The X11 backend on a non-Linux host (`x11-macos`): library names per host,
/// the macOS windowing request, and the "X11 drives this process" flag.
pub mod x11_host;

// Re-exports for convenience
/// Re-exported from `azul_core::window` — the list moved there so the
/// headless E2E runner shares the exact resize decision the shells make.
pub use azul_core::window::CSS_BREAKPOINTS;
pub use compositor::{
    check_gpu_blacklist, AzBackend, Compositor, CompositorMode, GpuCheckResult, GpuInfo,
    RenderContext,
};
pub use cpu_compositor::CpuCompositor;
pub use dlopen::DynamicLibrary;
pub use error::{CompositorError, DlError, WindowError};
pub use event::{CommonWindowState, HitTestNode, PlatformWindow};
pub use layout::{generate_frame, regenerate_layout};

/// Seed the window's opaque canvas colour at CREATION time.
///
/// An explicit `background_color` wins and is left alone - it is the app's.
/// Otherwise the window starts on the background its MODE derives
/// ([`scheme_background`]), for the mode it will show: the one
/// [`event::initial_window_theme`] resolves - the decision
/// `CommonWindowState::new` makes a moment later - so a window opened under a
/// dark app / `AZ_THEME` pin on a light desktop starts dark. (It used to pick
/// by the DESKTOP's theme and start on the desktop's canvas.)
///
/// Only applies to an `Opaque` window - a material (blur/acrylic/mica) is
/// composited by the platform and must keep `background_color` unset so the
/// renderer clears to transparent and the material shows through.
///
/// The per-mode pair is kept (rather than collapsed here) so a mode change
/// while the window is open can re-derive:
/// `CommonWindowState::move_scheme_background` moves a background seeded
/// here with the mode, and leaves one the app set where it is.
pub fn resolve_initial_background_color(
    options: &mut azul_layout::window_state::WindowCreateOptions,
    system_style: &azul_css::system::SystemStyle,
) {
    use azul_core::window::WindowBackgroundMaterial;

    if options.window_state.background_color.is_some() {
        return;
    }
    if !matches!(
        options.window_state.flags.background_material,
        WindowBackgroundMaterial::Opaque
    ) {
        return;
    }
    let mode = event::initial_window_theme(options.theme, system_style.theme);
    options.window_state.background_color =
        azul_css::props::basic::OptionColorU::Some(scheme_background(
            mode,
            system_style,
            options.background_color_light,
            options.background_color_dark,
        ));
}

/// The window background a MODE derives - THE derivation, for the creation
/// seed and for every mode change that moves a seeded background
/// (`CommonWindowState::move_scheme_background`):
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
pub fn scheme_background(
    mode: azul_core::window::WindowTheme,
    system_style: &azul_css::system::SystemStyle,
    light: azul_css::props::basic::OptionColorU,
    dark: azul_css::props::basic::OptionColorU,
) -> azul_css::props::basic::ColorU {
    let is_dark = mode == azul_core::window::WindowTheme::DarkMode;
    let own = if is_dark { dark } else { light };
    own.into_option().unwrap_or_else(|| {
        let theme = if is_dark {
            azul_css::system::Theme::Dark
        } else {
            azul_css::system::Theme::Light
        };
        azul_css::props::basic::color::SystemColorRef::WindowBackground
            .resolve_for_theme(&system_style.colors_for_theme(theme), is_dark)
    })
}

/// THE colour a window's canvas is cleared to - what shows wherever the app's
/// content does not paint. Every renderer asks here: the CPU compositor per
/// frame (`CpuBackend::render_frame`, the canvas of headless, macOS, X11,
/// Wayland, Windows, iOS and Android), WebRender at creation
/// (`wr_translate2::default_renderer_options`) and whenever the answer moves
/// (`CommonWindowState::sync_renderer_clear_color`).
///
/// In precedence order:
///
/// 1. `transparent` (a background material the platform composites): transparent black, so the
///    material shows through;
/// 2. the window's `background_color` - the app's own, or the one [`scheme_background`] derived
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
    mode: azul_core::window::WindowTheme,
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
            scheme_background(mode, style, OptionColorU::None, OptionColorU::None)
        }
        (None, _) if mode == azul_core::window::WindowTheme::DarkMode => ColorU {
            r: 42,
            g: 46,
            b: 50,
            a: 255,
        },
        (None, _) => ColorU::WHITE,
    };
    ColorU { a: 255, ..color }
}
