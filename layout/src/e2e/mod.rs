//! Debug / E2E server, ported into `azul-layout` from the DLL's
//! `desktop::shell2::common::debug_server`.
//!
//! The whole module is gated behind the `e2e-server` feature (declared NOT in
//! `default`), so the lean published crate is byte-for-byte unaffected. The
//! ~12k-line op-dispatch implementation lives verbatim in [`full`]:
//! `process_debug_event` (the op dispatcher), the `DebugEvent` enum, the `E2e*`
//! JSON schema types and the scenario runner (`resume_e2e_continuation`).
//!
//! ONE call site in [`full`] is injected through [`hooks`] so the core dispatch
//! stays host-agnostic: the native window screenshot. The DLL installs the real
//! implementation via [`hooks::set_host_hooks`]; headless callers (the
//! `e2e_json` test, [`run_e2e_test`]) get the `None` default, which ERRORS
//! rather than pretending a screenshot was taken.
//!
//! The `DebugRequest` plumbing (spmc channel + `handle_event_request` + the
//! server statics) is gated behind the `e2e-server-http` sub-feature; it is not
//! needed by the library API path. The pieces that CANNOT live here at all —
//! the TCP listener that serves the debugger UI out of the DLL build script's
//! `OUT_DIR`, and `register_debug_timer(&mut dyn PlatformWindow)` — live in the
//! DLL's `debug_server::platform` and call back into this module.

mod full;
pub use full::*;

mod cpu_backend;

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
        pub take_native_screenshot_base64: Option<fn(&mut CallbackInfo) -> Result<String, String>>,
    }

    impl E2eHostHooks {
        /// All-headless defaults.
        pub const NONE: Self = Self {
            take_native_screenshot_base64: None,
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
    pub(crate) fn take_native_screenshot_base64(ci: &mut CallbackInfo) -> Result<String, String> {
        match get().take_native_screenshot_base64 {
            Some(f) => f(ci),
            None => Err("native screenshot unavailable (no e2e host hook installed)".to_string()),
        }
    }
}
