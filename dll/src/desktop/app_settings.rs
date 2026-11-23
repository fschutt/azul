//! ONE value that holds everything the environment asks of an app, built once at the start.
//!
//! The environment is read in one place ([`crate::env::ProcessEnv`]); every setting is a typed
//! value built by a pure `from_lookup(&dyn EnvSource)` in the crate that owns it. [`AppSettings`]
//! gathers the settings of the dll's own layers ([`ShellSettings`] for the app start and the
//! shells, [`RenderSettings`] for the WebRender path) and is the one carrier: `App::create` builds
//! it from the process once, the app's [`AppShared`](super::shell2::common::app_shared::AppShared)
//! owns it, and every window reads it through that
//! (`window.common.app.settings().shell.hit_debug`). Nothing downstream asks the environment
//! again, and a change of the environment after the start is not seen.
//!
//! [`AppSettings::default`] is the behaviour of an EMPTY environment (`from_lookup(&NoEnv)`): what
//! a test, a tool or a host that builds an app without reading the process gets
//! (`AppShared::new`).
//!
//! The settings of the other layers hang on this struct as a field each, with one line in
//! [`AppSettings::from_lookup`]: the platform lookups, the Linux shell's and the extras' switches
//! (the platform layer's group D3 and D5), and the css, core and layout settings.

use azul_css::env::{EnvSource, NoEnv};

use super::{render_settings::RenderSettings, shell_settings::ShellSettings};

/// Everything the environment asks of an app, read once at its start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    /// The app start and the platform shells.
    pub shell: ShellSettings,
    /// The WebRender path.
    pub render: RenderSettings,
}

impl AppSettings {
    /// What the environment `env` asks for; an empty one asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            shell: ShellSettings::from_lookup(env),
            render: RenderSettings::from_lookup(env),
        }
    }
}

impl Default for AppSettings {
    /// The empty environment's.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "app_settings_test.rs"]
mod tests;
