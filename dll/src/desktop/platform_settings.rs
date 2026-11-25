//! The platform lookups of the dll, typed: which session the app runs in, which desktop it
//! runs on, and where the user's files live.
//!
//! Every variable here used to be read with `std::env::var` at the place that needed it (a
//! `HOME` in six functions of the Linux style discovery, `DISPLAY` in four files). They are read
//! ONCE, at the start of the app, through the one process reader (`crate::env::ProcessEnv`), into
//! a [`PlatformEnv`] that the app carries (`AppSettings::platform`), and the code that needs a
//! value reads a field. A change of the environment after that is not seen.
//!
//! This file is the one place of the dll that knows which variable means what. It is also an
//! [`EnvSource`] over exactly the names it holds, so a function of another crate that takes a
//! source (`azul_css::system::detect_linux_desktop_env`) is handed the snapshot and cannot read
//! anything the app did not read at its start.
//!
//! Values that are paths (`HOME`, the XDG directories, `PATH`, the Windows directories) are kept
//! as the OS gave them ([`OsString`], a non-UTF-8 path survives); the accessors named after the
//! variable give the text view (`None` when the value is not UTF-8, as `std::env::var(..).ok()`).
//! Everything else is text.

use std::ffi::{OsStr, OsString};

use azul_css::env::EnvSource;

/// The text view of an OS-string field: `None` when it is unset or not UTF-8.
fn text(value: &Option<OsString>) -> Option<&str> {
    value.as_deref().and_then(OsStr::to_str)
}

/// What the process environment says about the session, the desktop and the user's directories,
/// read once at the start of the app.
///
/// `Default` is the empty environment (every field `None`): what a test, a headless build and a
/// host that configured nothing get.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformEnv {
    // ---- the display session ----
    /// `DISPLAY`: the X11 display (`:0`, `host:0`).
    pub display: Option<String>,
    /// `WAYLAND_DISPLAY`: the name of the Wayland socket.
    pub wayland_display: Option<String>,
    /// `WAYLAND_DISPLAY_WIDTH`: the width of a display a compositor gave no size for.
    pub wayland_display_width: Option<String>,
    /// `WAYLAND_DISPLAY_HEIGHT`: the height of a display a compositor gave no size for.
    pub wayland_display_height: Option<String>,
    /// `XDG_SESSION_TYPE`: `x11` or `wayland` (as the login manager set it).
    pub xdg_session_type: Option<String>,

    // ---- the desktop ----
    /// `XDG_CURRENT_DESKTOP`: the desktop's name (`GNOME`, `KDE`, `ubuntu:GNOME`).
    pub xdg_current_desktop: Option<String>,
    /// `DESKTOP_SESSION`: the session's name, the fallback of `XDG_CURRENT_DESKTOP`.
    pub desktop_session: Option<String>,
    /// `GNOME_DESKTOP_SESSION_ID`: set in a GNOME session.
    pub gnome_desktop_session_id: Option<String>,
    /// `KDE_FULL_SESSION`: set in a KDE session.
    pub kde_full_session: Option<String>,
    /// `HYPRLAND_INSTANCE_SIGNATURE`: set in a Hyprland session.
    pub hyprland_instance_signature: Option<String>,
    /// `SWAYSOCK`: the socket of a running Sway.
    pub swaysock: Option<String>,
    /// `I3SOCK`: the socket of a running i3.
    pub i3sock: Option<String>,
    /// `DBUS_SESSION_BUS_ADDRESS`: where the session bus is (`unix:path=/run/user/1000/bus`).
    pub dbus_session_bus_address: Option<String>,
    /// `FLATPAK_ID`: the app's Flatpak id inside a Flatpak sandbox.
    pub flatpak_id: Option<String>,

    // ---- the language ----
    /// `LANGUAGE`: the colon-separated list of languages, most wanted first.
    pub language: Option<String>,
    /// `LANG`: the default locale.
    pub lang: Option<String>,
    /// `LC_ALL`: the locale that overrides every category.
    pub lc_all: Option<String>,
    /// `LC_CTYPE`: the locale of the character classes (the Compose file's).
    pub lc_ctype: Option<String>,
    /// `LC_MESSAGES`: the locale of the messages.
    pub lc_messages: Option<String>,

    // ---- the cursor ----
    /// `XCURSOR_THEME`: the cursor theme every Linux window manager honours.
    pub xcursor_theme: Option<String>,
    /// `XCURSOR_SIZE`: the cursor size in pixels.
    pub xcursor_size: Option<String>,

    // ---- the user's directories ----
    /// `HOME`: the user's home directory.
    pub home: Option<OsString>,
    /// `XDG_CACHE_HOME`: where a user's caches go (`~/.cache` when unset).
    pub xdg_cache_home: Option<OsString>,
    /// `XDG_DATA_HOME`: where a user's data goes (`~/.local/share` when unset).
    pub xdg_data_home: Option<OsString>,
    /// `XDG_CONFIG_DIRS`: the system-wide configuration directories, colon-separated.
    pub xdg_config_dirs: Option<OsString>,
    /// `GTK_CONFIG_HOME`: the GTK settings file the user named.
    pub gtk_config_home: Option<OsString>,
    /// `PATH`: where executables are looked for.
    pub path: Option<OsString>,
    /// `LOCALAPPDATA`: the local application data directory (Windows).
    pub local_app_data: Option<OsString>,
    /// `USERPROFILE`: the user's profile directory (Windows).
    pub user_profile: Option<OsString>,
    /// `TEMP`: the temporary directory (Windows).
    pub temp: Option<OsString>,
}

impl PlatformEnv {
    /// Reads every platform variable once from `env`: the process (`ProcessEnv`) at the start of
    /// the app, a `MapEnv` in a test, `NoEnv` for the empty environment.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let text = |name: &str| env.var(name);
        let path = |name: &str| env.var_os(name);
        Self {
            display: text("DISPLAY"),
            wayland_display: text("WAYLAND_DISPLAY"),
            wayland_display_width: text("WAYLAND_DISPLAY_WIDTH"),
            wayland_display_height: text("WAYLAND_DISPLAY_HEIGHT"),
            xdg_session_type: text("XDG_SESSION_TYPE"),
            xdg_current_desktop: text("XDG_CURRENT_DESKTOP"),
            desktop_session: text("DESKTOP_SESSION"),
            gnome_desktop_session_id: text("GNOME_DESKTOP_SESSION_ID"),
            kde_full_session: text("KDE_FULL_SESSION"),
            hyprland_instance_signature: text("HYPRLAND_INSTANCE_SIGNATURE"),
            swaysock: text("SWAYSOCK"),
            i3sock: text("I3SOCK"),
            dbus_session_bus_address: text("DBUS_SESSION_BUS_ADDRESS"),
            flatpak_id: text("FLATPAK_ID"),
            language: text("LANGUAGE"),
            lang: text("LANG"),
            lc_all: text("LC_ALL"),
            lc_ctype: text("LC_CTYPE"),
            lc_messages: text("LC_MESSAGES"),
            xcursor_theme: text("XCURSOR_THEME"),
            xcursor_size: text("XCURSOR_SIZE"),
            home: path("HOME"),
            xdg_cache_home: path("XDG_CACHE_HOME"),
            xdg_data_home: path("XDG_DATA_HOME"),
            xdg_config_dirs: path("XDG_CONFIG_DIRS"),
            gtk_config_home: path("GTK_CONFIG_HOME"),
            path: path("PATH"),
            local_app_data: path("LOCALAPPDATA"),
            user_profile: path("USERPROFILE"),
            temp: path("TEMP"),
        }
    }

    /// The platform lookups of THIS process, read now.
    ///
    /// For the entry points that have no app to carry a [`PlatformEnv`]: a public C-ABI function
    /// of a service (`PowerState::query`, `VideoStartupCheck::run`) is called by a host with no
    /// window and no `App` in reach. Everything that has an app or a window takes the value the
    /// app read at its start.
    #[must_use]
    pub fn of_process() -> Self {
        Self::from_lookup(&crate::env::ProcessEnv::new())
    }

    /// `HOME` as text (`None` when unset or not UTF-8).
    #[must_use]
    pub fn home(&self) -> Option<&str> {
        text(&self.home)
    }

    /// `XDG_CACHE_HOME` as text.
    #[must_use]
    pub fn xdg_cache_home(&self) -> Option<&str> {
        text(&self.xdg_cache_home)
    }

    /// `XDG_DATA_HOME` as text.
    #[must_use]
    pub fn xdg_data_home(&self) -> Option<&str> {
        text(&self.xdg_data_home)
    }

    /// `XDG_CONFIG_DIRS` as text.
    #[must_use]
    pub fn xdg_config_dirs(&self) -> Option<&str> {
        text(&self.xdg_config_dirs)
    }

    /// `GTK_CONFIG_HOME` as text.
    #[must_use]
    pub fn gtk_config_home(&self) -> Option<&str> {
        text(&self.gtk_config_home)
    }

    /// `LOCALAPPDATA` as text.
    #[must_use]
    pub fn local_app_data(&self) -> Option<&str> {
        text(&self.local_app_data)
    }

    /// `USERPROFILE` as text.
    #[must_use]
    pub fn user_profile(&self) -> Option<&str> {
        text(&self.user_profile)
    }

    /// `TEMP` as text.
    #[must_use]
    pub fn temp(&self) -> Option<&str> {
        text(&self.temp)
    }

    /// Whether there is an X11 session to ask (the idle time, an X display): an X display is
    /// set and nothing says the session is a Wayland one - no Wayland socket, and the login
    /// manager did not say `wayland`.
    #[must_use]
    pub fn is_x11_session(&self) -> bool {
        self.wayland_display.is_none()
            && self.xdg_session_type.as_deref() != Some("wayland")
            && self.display.is_some()
    }

    /// The locale whose Compose file the X input method compiles: the first non-empty of
    /// `LC_ALL`, `LC_CTYPE`, `LANG` (the precedence `setlocale(LC_CTYPE, "")` uses), `"C"` when
    /// none is set.
    #[must_use]
    pub fn compose_locale(&self) -> &str {
        [&self.lc_all, &self.lc_ctype, &self.lang]
            .into_iter()
            .filter_map(|v| v.as_deref())
            .find(|v| !v.is_empty())
            .unwrap_or("C")
    }
}

/// The platform variables as a source: exactly the names above answer, everything else is unset.
impl EnvSource for PlatformEnv {
    fn var(&self, name: &str) -> Option<String> {
        let own = |v: &Option<String>| v.clone();
        let os = |v: &Option<OsString>| text(v).map(str::to_string);
        match name {
            "DISPLAY" => own(&self.display),
            "WAYLAND_DISPLAY" => own(&self.wayland_display),
            "WAYLAND_DISPLAY_WIDTH" => own(&self.wayland_display_width),
            "WAYLAND_DISPLAY_HEIGHT" => own(&self.wayland_display_height),
            "XDG_SESSION_TYPE" => own(&self.xdg_session_type),
            "XDG_CURRENT_DESKTOP" => own(&self.xdg_current_desktop),
            "DESKTOP_SESSION" => own(&self.desktop_session),
            "GNOME_DESKTOP_SESSION_ID" => own(&self.gnome_desktop_session_id),
            "KDE_FULL_SESSION" => own(&self.kde_full_session),
            "HYPRLAND_INSTANCE_SIGNATURE" => own(&self.hyprland_instance_signature),
            "SWAYSOCK" => own(&self.swaysock),
            "I3SOCK" => own(&self.i3sock),
            "DBUS_SESSION_BUS_ADDRESS" => own(&self.dbus_session_bus_address),
            "FLATPAK_ID" => own(&self.flatpak_id),
            "LANGUAGE" => own(&self.language),
            "LANG" => own(&self.lang),
            "LC_ALL" => own(&self.lc_all),
            "LC_CTYPE" => own(&self.lc_ctype),
            "LC_MESSAGES" => own(&self.lc_messages),
            "XCURSOR_THEME" => own(&self.xcursor_theme),
            "XCURSOR_SIZE" => own(&self.xcursor_size),
            "HOME" => os(&self.home),
            "XDG_CACHE_HOME" => os(&self.xdg_cache_home),
            "XDG_DATA_HOME" => os(&self.xdg_data_home),
            "XDG_CONFIG_DIRS" => os(&self.xdg_config_dirs),
            "GTK_CONFIG_HOME" => os(&self.gtk_config_home),
            "PATH" => os(&self.path),
            "LOCALAPPDATA" => os(&self.local_app_data),
            "USERPROFILE" => os(&self.user_profile),
            "TEMP" => os(&self.temp),
            _ => None,
        }
    }

    fn var_os(&self, name: &str) -> Option<OsString> {
        match name {
            "HOME" => self.home.clone(),
            "XDG_CACHE_HOME" => self.xdg_cache_home.clone(),
            "XDG_DATA_HOME" => self.xdg_data_home.clone(),
            "XDG_CONFIG_DIRS" => self.xdg_config_dirs.clone(),
            "GTK_CONFIG_HOME" => self.gtk_config_home.clone(),
            "PATH" => self.path.clone(),
            "LOCALAPPDATA" => self.local_app_data.clone(),
            "USERPROFILE" => self.user_profile.clone(),
            "TEMP" => self.temp.clone(),
            other => self.var(other).map(OsString::from),
        }
    }
}

#[cfg(test)]
#[path = "platform_settings_test.rs"]
mod tests;
