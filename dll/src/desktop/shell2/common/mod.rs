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

/// Resolve the window's opaque canvas colour at CREATION time.
///
/// The rule, once, because every backend needs it and each copy is a chance to
/// drift: an explicit `background_color` wins; otherwise the per-theme override
/// for whichever theme the system is in right now; otherwise the system's own
/// window background.
///
/// Only applies to an `Opaque` window — a material (blur/acrylic/mica) is
/// composited by the platform and must keep `background_color` unset so the
/// renderer clears to transparent and the material shows through.
///
/// The per-theme pair is kept (rather than collapsed here) so a theme change
/// while the window is open can re-resolve; this only seeds the first frame.
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
    let per_theme = if system_style.theme == azul_css::system::Theme::Dark {
        options.background_color_dark
    } else {
        options.background_color_light
    };
    options.window_state.background_color = if per_theme.is_some() {
        per_theme
    } else {
        system_style.colors.window_background
    };
}
