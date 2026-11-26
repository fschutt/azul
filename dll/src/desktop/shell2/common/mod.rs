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
/// What happens to a phone's GL context, surface and renderer when the surface goes, the context is lost, the
/// app goes to the background or the system wants memory back: a pure reducer both phone shells follow (the
/// platform's executor performs its actions), tested against a fake platform that enforces the platforms' rules.
pub mod gpu_lifecycle;
/// Which renderer a phone window gets: `AZ_BACKEND` and `hw_accel`, what the GPU bring-up said stage by
/// stage, the one decision, the failure plan of a test run (`AZ_GPU_FAIL`) and the one line the QA harness reads.
pub mod gpu_select;

// Unified cross-platform modules
pub mod accessibility;
/// What the app chose (its mode, its language) and configured, shared by every window of it.
pub mod app_shared;
pub mod capability_pump;
/// Keeping a window out of screen captures (`WindowFlags::exclude_from_capture`): the one decision every
/// shell makes about it, and the values the platforms take.
pub mod capture_exclusion;
pub mod clipboard;
pub mod debug_server;
#[cfg(feature = "e2e-test")]
pub mod e2e_test;
pub mod event;
/// What the first frame of a window owes layout: none where the constructor ran one (the startup timeline
/// counted a second one on macOS), the one pure decision, unit-tested.
pub mod first_frame;
/// When a vsync-driven frame pump runs (the macOS display link): the one
/// pure decision, unit-tested.
pub mod frame_pacer;
/// Whether a pass of the GPU path waits for the renderer's threads after its
/// send (audit A4-31): the one pure decision, unit-tested.
pub mod frame_schedule;
pub mod layout;
/// The runtime gate for every `log_*!` macro, plus RAII enter/exit spans.
/// Logging is gated here by atomics — never by a cargo feature.
pub mod log_gate;
/// A top-level window owned by the window that opened it: above it, minimised and
/// closed with it.
pub mod owned_window;
/// Ending the process from a thread that is not the UI thread: the worker
/// asks, the run loop exits with everything else stopped.
pub mod process_exit;
pub mod seats;
/// The operating system asks the app to quit (a logout, a shutdown): every window's close
/// protocol runs.
pub mod session_end;
/// What the start of an app asks of the system and of subprocesses, as pure decisions (a command-line fallback only where
/// the native query did not answer; the per-step log of `App::create`).
pub mod style_discovery;
/// A closing window hands the app's background threads to a window that stays open.
pub mod thread_handover;
pub mod transient;
/// `<webview>`: the backend trait every platform's native web view
/// implements, the headless one, and the pump that runs them.
pub mod webview;
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
use azul_css::system::DarkLightMode;

/// Seed the window's opaque canvas colour at CREATION time.
///
/// An explicit `background_color` wins and is left alone - it is the app's.
/// Otherwise the window starts on the background its MODE derives
/// ([`mode_background`]), for the mode it will show: the one
/// [`event::initial_window_theme`] resolves - the decision
/// `CommonWindowState::new` makes a moment later - so a window opened under a
/// dark app / `AZ_THEME` pin on a light desktop starts dark. (It used to pick
/// by the DESKTOP's theme and start on the desktop's canvas.)
///
/// `app_mode` is the app's mode as its [`app_shared::AppShared`] holds it.
///
/// Only applies to an `Opaque` window - a material (blur/acrylic/mica) is
/// composited by the platform and must keep `background_color` unset so the
/// renderer clears to transparent and the material shows through.
///
/// The per-mode pair is kept (rather than collapsed here) so a mode change
/// while the window is open can re-derive:
/// `CommonWindowState::move_mode_background` moves a background seeded
/// here with the mode, and leaves one the app set where it is.
pub fn resolve_initial_background_color(
    options: &mut azul_layout::window_state::WindowCreateOptions,
    system_style: &azul_css::system::SystemStyle,
    app_mode: azul_core::window::OptionDarkLightMode,
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
    let mode = event::initial_window_theme(app_mode, options.mode, system_style.mode);
    options.window_state.background_color =
        azul_css::props::basic::OptionColorU::Some(mode_background(
            mode,
            system_style,
            options.background_color_light,
            options.background_color_dark,
        ));
}

pub use azul_layout::window_background::{mode_background, window_clear_color};

/// `window_id`, or the first free `<window_id>-2`, `-3`, ... where `taken` says another open
/// window has it. The debug server reaches a window by its id, and every popup is
/// `azul-transient`, every menu `azul-menu`: a dialog's popover or a submenu under the same id as
/// the window beside it could not be reached at all.
#[must_use]
pub fn free_window_id(window_id: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(window_id) {
        return window_id.to_string();
    }
    (2usize..)
        .map(|n| format!("{window_id}-{n}"))
        .find(|id| !taken(id))
        .unwrap_or_else(|| window_id.to_string())
}

#[cfg(test)]
mod window_id_tests {
    use super::free_window_id;

    /// The debug server reaches a window by its id: a window opened under an id an open window
    /// has already (every popup is `azul-transient`, every menu `azul-menu`) takes the first
    /// free `-2`, `-3`, ..., and a free id stays as it is.
    #[test]
    fn a_window_under_a_taken_id_takes_the_first_free_number() {
        let open = ["azul-window", "azul-transient", "azul-transient-2"];
        let taken = |id: &str| open.contains(&id);
        assert_eq!(free_window_id("azul-transient", taken), "azul-transient-3");
        assert_eq!(free_window_id("azul-menu", taken), "azul-menu");
        assert_eq!(
            free_window_id("azul-window", |id| id == "azul-window"),
            "azul-window-2"
        );
    }
}
