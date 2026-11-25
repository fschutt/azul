//! The platform variables the services of this crate ask of the environment: where the user's home,
//! configuration and data folders are, which windowing system the session runs, whether the program
//! was installed as a snap.
//!
//! Nothing here reads the process: [`PlatformEnv::from_lookup`] takes a `&dyn EnvSource`
//! (`azul_css::env`) and the one process reader is `azul_dll::env::ProcessEnv`. `Default` is the
//! behaviour of an EMPTY environment (nothing is set), which is also what `from_lookup(&NoEnv)`
//! answers. The paths keep their bytes where the lookup can say so (the `std` feature); the markers
//! are "is it set at all".
//!
//! The users: `icon_remap` (`~/.azul`), the telemetry client (its config and data folders, the home
//! folder it strips from a backtrace, the windowing system it reports), the updater (the data folder
//! of its state, the snap marker) and the e2e project folder (`~/` in a path).

use std::ffi::{OsStr, OsString};

use azul_css::env::EnvSource;

/// The value of `name` as an OS string: a path keeps its bytes when the lookup can say so (azul-css
/// with its `std` feature, which this crate's own `std` feature switches on), else it is the text.
pub(crate) fn os_var(env: &dyn EnvSource, name: &str) -> Option<OsString> {
    #[cfg(feature = "std")]
    {
        env.var_os(name)
    }
    #[cfg(not(feature = "std"))]
    {
        env.var(name).map(OsString::from)
    }
}

/// The platform variables of one process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformEnv {
    /// `HOME`.
    pub home: Option<OsString>,
    /// `USERPROFILE` (Windows).
    pub user_profile: Option<OsString>,
    /// `XDG_CONFIG_HOME`.
    pub xdg_config_home: Option<OsString>,
    /// `XDG_DATA_HOME`.
    pub xdg_data_home: Option<OsString>,
    /// `APPDATA` (Windows, the roaming folder).
    pub app_data: Option<OsString>,
    /// `LOCALAPPDATA` (Windows).
    pub local_app_data: Option<OsString>,
    /// `SNAP` is set: the program runs confined as a snap.
    pub snap: bool,
    /// `WAYLAND_DISPLAY` is set: a Wayland compositor serves the session.
    pub wayland_display: bool,
    /// `DISPLAY` is set: an X11 server serves the session.
    pub display: bool,
}

impl PlatformEnv {
    /// What the environment `env` says; an empty environment says nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            home: os_var(env, "HOME"),
            user_profile: os_var(env, "USERPROFILE"),
            xdg_config_home: os_var(env, "XDG_CONFIG_HOME"),
            xdg_data_home: os_var(env, "XDG_DATA_HOME"),
            app_data: os_var(env, "APPDATA"),
            local_app_data: os_var(env, "LOCALAPPDATA"),
            snap: env.is_set("SNAP"),
            wayland_display: env.is_set("WAYLAND_DISPLAY"),
            display: env.is_set("DISPLAY"),
        }
    }

    /// The user's home folder as the environment names it: `HOME`, else `USERPROFILE`. A `HOME`
    /// that is set but empty is returned as it is (the caller decides what an empty home means).
    #[must_use]
    pub fn home_dir(&self) -> Option<&OsStr> {
        self.home.as_deref().or(self.user_profile.as_deref())
    }
}

#[cfg(test)]
#[path = "platform_test.rs"]
mod tests;
