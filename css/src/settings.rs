//! The settings of azul-css that come from the environment, and the one place of the crate that
//! names their variables.
//!
//! Nothing here reads the process: every constructor takes a `&dyn EnvSource`
//! ([`crate::env`]). The dll builds them once at the start from its process reader and installs
//! what the library asks for without a context ([`CssSettings::install`]); a library user who
//! never does gets [`CssSettings::default`], which is the behaviour of an EMPTY environment.

use alloc::{string::String, vec::Vec};

use crate::{
    accessibility::AccessibilityPreferences, env::EnvSource, system::RicingMode,
    theme_chain::ThemeEnv,
};

/// `AZ_RICING`, `AZ_THEME`, `AZ_MODE` and `AZ_A11Y_*`: what the environment asks of the user's rice,
/// of the theme chain and of the accessibility preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssSettings {
    /// `AZ_RICING`: off, default, force or watch ([`RicingMode`]).
    pub ricing: RicingMode,
    /// `AZ_THEME` and `AZ_MODE`: the pins of the theme and of the light / dark mode.
    pub theme: ThemeEnv,
    /// `AZ_A11Y_<FIELD>`: the accessibility preferences the environment pins, whatever the
    /// system reports ([`AccessibilityPreferences::from_lookup`]); empty pins nothing.
    pub accessibility: AccessibilityPreferences,
    /// What was wrong with an `AZ_A11Y_*` variable (a word that means nothing, a number out of
    /// range): the app start logs each.
    pub accessibility_notes: Vec<String>,
}

impl CssSettings {
    /// What the environment `env` asks for; an empty one asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let (accessibility, accessibility_notes) = AccessibilityPreferences::from_lookup(env);
        Self {
            ricing: RicingMode::from_lookup(env),
            theme: ThemeEnv::from_lookup(env),
            accessibility,
            accessibility_notes,
        }
    }

    /// Puts the theme and mode pins where style resolution finds them.
    ///
    /// That is [`crate::theme_chain::install_env`]; the first install of a process wins. The
    /// rice is installed apart ([`crate::rice::install`]), because it also needs the user's home.
    pub fn install(&self) {
        crate::theme_chain::install_env(self.theme.clone());
    }
}

impl RicingMode {
    /// The mode `AZ_RICING` names in `env` ([`crate::system::ricing_mode_from`]); unset is
    /// [`RicingMode::Default`].
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        crate::system::ricing_mode_from(env.var("AZ_RICING").as_deref())
    }
}

impl ThemeEnv {
    /// The request of `AZ_THEME` and `AZ_MODE` in `env` ([`ThemeEnv::from_values`]).
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let theme = env.var("AZ_THEME");
        let mode = env.var("AZ_MODE");
        Self::from_values(theme.as_deref(), mode.as_deref())
    }
}

/// What the desktop session says about itself: the variables
/// [`crate::system::detect_linux_desktop_env`] and [`crate::system::detect_system_language`]
/// read, as data.
// Each marker is its own variable of the session; a set of enums would only rename them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemEnv {
    /// `XDG_CURRENT_DESKTOP`.
    pub current_desktop: Option<String>,
    /// `DESKTOP_SESSION`.
    pub desktop_session: Option<String>,
    /// `GNOME_DESKTOP_SESSION_ID` is set.
    pub gnome_session: bool,
    /// `KDE_FULL_SESSION` is set.
    pub kde_session: bool,
    /// `HYPRLAND_INSTANCE_SIGNATURE` is set.
    pub hyprland: bool,
    /// `SWAYSOCK` is set.
    pub sway: bool,
    /// `I3SOCK` is set.
    pub i3: bool,
    /// `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` and `LANG`, in the order of their priority, the ones
    /// that are set.
    pub languages: Vec<String>,
}

impl SystemEnv {
    /// What the environment `env` says.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            current_desktop: env.var("XDG_CURRENT_DESKTOP"),
            desktop_session: env.var("DESKTOP_SESSION"),
            gnome_session: env.var("GNOME_DESKTOP_SESSION_ID").is_some(),
            kde_session: env.var("KDE_FULL_SESSION").is_some(),
            hyprland: env.var("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
            sway: env.var("SWAYSOCK").is_some(),
            i3: env.var("I3SOCK").is_some(),
            languages: ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .filter_map(|name| env.var(name))
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
