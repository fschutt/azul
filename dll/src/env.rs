//! The ONE reader of the process environment in azul-dll (and, with a tool's `main`, in the
//! whole workspace).
//!
//! Every library crate (azul-css, azul-core, azul-layout, webrender) takes a typed settings value
//! built by a pure `from_lookup(&dyn azul_css::env::EnvSource)`. This module is the source that
//! reads the real process: construct a [`ProcessEnv`], build the settings once at the start
//! (`App::create`), and hand the values down. A change of the environment after that is not seen.
//!
//! Nothing else in the dll calls `std::env::var` or `var_os`; the gate
//! `examples/azul-srcgate/tests/env_gate.rs` counts the reads per file and the count of this file
//! is its two.

use azul_css::env::EnvSource;

/// The process environment, read at the moment of the call.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn var_os(&self, name: &str) -> Option<std::ffi::OsString> {
        std::env::var_os(name)
    }
}

/// Reads what azul-css and azul-core ask of the environment `env`, once, and installs what their
/// context-free code needs: the theme and mode pins, the log filter, the profiling gate, the
/// diagnostic dials and the test hooks. `App::create` calls it first, before the first log
/// record, with a [`ProcessEnv`] (and installs the user's rice from the same environment).
///
/// What a library crate keeps for itself is read-only after this call and the first install of
/// a process wins, so a change of the environment later is not seen.
pub fn install_settings(env: &dyn EnvSource) {
    azul_css::settings::CssSettings::from_lookup(env).install();
    azul_core::settings::CoreSettings::from_lookup(env).install();
}

/// `AppConfig::create` of the C API and of the `azul` crate (api.json names this function): the
/// config of `azul_core::resources::AppConfig::create`, which reads no environment, with the
/// default text anti-aliasing replaced by what `AZ_TEXT_AA` and the `AZ_LCD_*` variables of
/// the process environment ask for. An app that sets it afterwards decides for itself.
#[must_use]
pub fn app_config_create() -> azul_core::resources::AppConfig {
    let mut config = azul_core::resources::AppConfig::create();
    config.text_antialiasing = azul_core::resources::TextAntialiasing::from_lookup(&ProcessEnv);
    config
}
