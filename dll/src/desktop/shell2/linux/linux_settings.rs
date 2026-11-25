//! The switches of the Linux shell (X11 and Wayland), typed.
//!
//! Each used to be read with `std::env::var` where it was needed - some of them on every frame
//! (`AZ_BB_DEBUG`, `AZ_PRESENT_VERIFY`) or in a `OnceLock` of their own (`AZ_WL_TRACE`,
//! `AZ_NO_PACE`). They are read once at the start of the app (`AppSettings::linux`) and a window
//! reads a field. This file is the one place of the Linux shell that knows which variable means
//! what; the platform lookups (`DISPLAY`, `HOME`, ...) are `PlatformEnv`'s, and the switches the
//! shells share (`AZ_BACKEND`, `AZ_WINDOW`, `AZ_HIT_DEBUG`, `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER`)
//! are `ShellSettings`'.
//!
//! `Default` is the empty environment (`from_lookup(&NoEnv)`), which is what an app that
//! configured nothing runs with: no trace, frame pacing on, MIT-SHM on, GNOME menus on.

use azul_css::env::{EnvSource, NoEnv};

/// The Linux shell's switches, read once at the start of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxSettings {
    /// `AZ_WL_TRACE` is set (to anything): the Wayland platform trace is on whatever `AZ_LOG`
    /// says, for the case where you want ONLY that trace.
    pub wl_trace: bool,
    /// `AZ_BB_DEBUG` is set: one line per frame about the native Wayland back buffer
    /// (slots, damage, catch-up).
    pub bb_debug: bool,
    /// `AZ_BB_PROBE=x,y` as typed: print the four bytes of the pixel `x,y` of a back-buffer
    /// slot at the stages of a frame.
    pub bb_probe: Option<String>,
    /// `AZ_PRESENT_VERIFY` is set: after a partial copy to a Wayland slot, compare the slot with
    /// the pixmap everywhere (finds a stale slot).
    pub present_verify: bool,
    /// `AZ_NO_PACE=1`: no frame-pacing gate on X11 (render as often as asked).
    pub no_pace: bool,
    /// `AZ_X11_SHM` is not `0`: the X11 CPU present may upload through MIT-SHM (the default).
    pub x11_shm: bool,
    /// `AZ_DISABLE_GNOME_MENUS=1`: client-side menus instead of the GNOME shell's.
    pub disable_gnome_menus: bool,
    /// `AZ_GNOME_MENU_DEBUG=1`: log the steps of the GNOME menu integration.
    pub gnome_menu_debug: bool,
    /// `AZ_MAP_DEBUG` is set: X11 also traces the accessibility actions it polls (the one switch
    /// of the map trace that reaches the window code).
    pub a11y_debug: bool,
}

impl LinuxSettings {
    /// Reads the switches from `env`: the process at the start of the app, a `MapEnv` in a test.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let is_one = |name: &str| env.var(name).is_some_and(|v| v == "1");
        Self {
            wl_trace: env.is_set("AZ_WL_TRACE"),
            bb_debug: env.var("AZ_BB_DEBUG").is_some(),
            bb_probe: env.var("AZ_BB_PROBE"),
            present_verify: env.is_set("AZ_PRESENT_VERIFY"),
            no_pace: is_one("AZ_NO_PACE"),
            x11_shm: !env.var("AZ_X11_SHM").is_some_and(|v| v == "0"),
            disable_gnome_menus: is_one("AZ_DISABLE_GNOME_MENUS"),
            gnome_menu_debug: is_one("AZ_GNOME_MENU_DEBUG"),
            a11y_debug: env.var("AZ_MAP_DEBUG").is_some(),
        }
    }
}

/// The empty environment: nothing traced, frame pacing on, MIT-SHM on, GNOME menus on.
impl Default for LinuxSettings {
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "linux_settings_test.rs"]
mod tests;
