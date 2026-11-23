//! ONE value that holds everything the environment asks of an app, built once at the start.
//!
//! The environment is read in one place ([`crate::env::ProcessEnv`]); every setting is a typed
//! value built by a pure `from_lookup(&dyn EnvSource)` in the crate that owns it. [`AppSettings`]
//! gathers the settings of the dll's own layers ([`ShellSettings`] for the app start and the
//! shells, [`WebRenderSettings`] for the WebRender path) and is the one carrier: `App::create` builds
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
//! (the platform layer's group D3 and D5), and the css and core settings.
//!
//! The settings of azul-layout ([`LayoutSettings`]: the solver, the text, and the groups of the
//! window, the render path, the services and the E2E engine) reach every window through
//! [`AppShared::seed_window`](super::shell2::common::app_shared::AppShared::seed_window) (it
//! calls `LayoutWindow::set_settings` for each window before its first pass), and the app's font
//! manager is made with the text group (`FontManager::with_settings`). A window that is not
//! seeded runs on the defaults, which is the behaviour of an empty environment.

use azul_css::env::{EnvSource, NoEnv};
use azul_layout::settings::LayoutSettings;

#[cfg(az_x11)]
use super::shell2::linux::linux_settings::LinuxSettings;
use super::{
    platform_settings::PlatformEnv, shell_settings::ShellSettings,
    webrender_settings::WebRenderSettings,
};

/// Everything the environment asks of an app, read once at its start.
// Not `Eq`: the settings of the layout engine hold values that are only `PartialEq`.
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    /// The settings of azul-layout: the solver, the text, and the groups of the other clusters.
    pub layout: LayoutSettings,
    /// The app start and the platform shells.
    pub shell: ShellSettings,
    /// The WebRender path: the texture cache, the display-list translation, the transaction.
    pub webrender: WebRenderSettings,
    /// The platform lookups: the session, the desktop, the user's directories.
    pub platform: PlatformEnv,
    /// The Linux shell's switches (X11 and Wayland); where the X11 backend is built.
    #[cfg(az_x11)]
    pub linux: LinuxSettings,
}

impl AppSettings {
    /// What the environment `env` asks for; an empty one asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            layout: LayoutSettings::from_lookup(env),
            shell: ShellSettings::from_lookup(env),
            webrender: WebRenderSettings::from_lookup(env),
            platform: PlatformEnv::from_lookup(env),
            #[cfg(az_x11)]
            linux: LinuxSettings::from_lookup(env),
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
