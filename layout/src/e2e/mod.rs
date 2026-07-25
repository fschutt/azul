//! Debug / E2E server, ported into `azul-layout` from the DLL's
//! `desktop::shell2::common::debug_server`.
//!
//! The whole module is gated behind the `e2e-server` feature (declared NOT in
//! `default`), so the lean published crate is byte-for-byte unaffected. The
//! ~12k-line op-dispatch implementation lives verbatim in [`full`]:
//! `process_debug_event` (the op dispatcher), the `DebugEvent` enum, the `E2e*`
//! JSON schema types and the scenario runner (`resume_e2e_continuation`).
//!
//! Three OS/DLL-coupled call sites in [`full`] are injected through [`hooks`] so
//! the core dispatch stays host-agnostic: the native window screenshot, the
//! process-global mount-XML swap, and the embedded Material Icons font bytes.
//! The DLL installs real implementations via [`hooks::set_host_hooks`]; headless
//! callers (the `e2e_json` test, [`run_e2e_test`]) get the no-op / `None`
//! defaults.
//!
//! The HTTP transport (TcpListener + spmc channel + request handlers) is gated
//! behind the `e2e-server-http` sub-feature; the platform timer registration
//! (`register_debug_timer`, which needs a `dyn PlatformWindow` that only exists
//! in the DLL) behind `e2e-server-platform`. Neither is needed by the library
//! API path.

mod full;
pub use full::*;

mod runner;
pub use runner::run_e2e_test;

mod report;
pub use report::{load_e2e_tests, render_report, E2eVerdict};

pub mod hooks {
    //! Dependency-injection seam for the three host-coupled call sites in
    //! [`super::full`]. See the module docs above.

    use std::sync::RwLock;

    use azul_layout::callbacks::CallbackInfo;

    /// Host-supplied implementations for the call sites the layout crate cannot
    /// satisfy on its own. Each is optional: `None` selects the headless
    /// default (error / no-op / `None`).
    #[derive(Clone, Copy, Debug)]
    pub struct E2eHostHooks {
        /// Grab a real window screenshot as a base64 data-URI. `None` (headless)
        /// makes the `screenshot` op return an error.
        pub take_native_screenshot_base64:
            Option<fn(&mut CallbackInfo) -> Result<String, String>>,
        /// Store / clear the process-global mount XML that the shell swaps in on
        /// the next relayout. `None` makes `mount` / `unmount` a no-op.
        pub set_mount_xml: Option<fn(Option<String>)>,
        /// Bytes of the embedded Material Icons TTF (served by the HTTP route).
        pub get_material_icons_font_bytes: Option<fn() -> Option<&'static [u8]>>,
    }

    impl E2eHostHooks {
        /// All-headless defaults.
        pub const NONE: Self = Self {
            take_native_screenshot_base64: None,
            set_mount_xml: None,
            get_material_icons_font_bytes: None,
        };
    }

    impl Default for E2eHostHooks {
        fn default() -> Self {
            Self::NONE
        }
    }

    static HOST_HOOKS: RwLock<E2eHostHooks> = RwLock::new(E2eHostHooks::NONE);

    /// Install host hooks. Called once by the DLL at startup; a headless caller
    /// may call it to override individual seams (e.g. capture screenshots).
    pub fn set_host_hooks(hooks: E2eHostHooks) {
        if let Ok(mut h) = HOST_HOOKS.write() {
            *h = hooks;
        }
    }

    fn get() -> E2eHostHooks {
        HOST_HOOKS.read().map(|h| *h).unwrap_or(E2eHostHooks::NONE)
    }

    /// Screenshot seam (`screenshot` op). Errors headlessly.
    pub(crate) fn take_native_screenshot_base64(
        ci: &mut CallbackInfo,
    ) -> Result<String, String> {
        match get().take_native_screenshot_base64 {
            Some(f) => f(ci),
            None => Err(
                "native screenshot unavailable (no e2e host hook installed)".to_string(),
            ),
        }
    }

    /// Mount-XML seam (`mount` / `unmount` ops). No-op headlessly unless a hook
    /// (e.g. the headless runner's) is installed.
    pub(crate) fn set_mount_xml(xml: Option<String>) {
        if let Some(f) = get().set_mount_xml {
            f(xml);
        }
    }

    /// Material Icons font seam (HTTP `/material-icons.ttf` route, served by the
    /// DLL-supplied web server).
    #[cfg(feature = "e2e-server-platform")]
    pub(crate) fn get_material_icons_font_bytes() -> Option<&'static [u8]> {
        get().get_material_icons_font_bytes.and_then(|f| f())
    }
}
