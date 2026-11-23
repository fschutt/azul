//! What the app shares with every window of it.
//!
//! An app makes a few choices that belong to no window: its light / dark mode (`AppConfig::mode`,
//! switched at run time by `CallbackInfo::set_mode`), its language (`CallbackInfo::set_locale`),
//! and the platform's natural-scrolling preference. A window that opens AFTER a switch has to
//! start in what the app chose, and window creation has no path back to the window whose
//! callback switched - so the choice lives in one [`AppShared`] that the app creates
//! (`AppInternal::create`) and hands to every window it builds, next to the undo manager.
//!
//! These used to be process-wide statics of the layout crate (`set_app_mode`, `set_app_locale`,
//! ...), which made every test that switched one race every other test of the process, and made a
//! process that ran two apps share them. Now each app - and each test - owns its own.
//!
//! The other half of what a new window takes from its app is [`AppShared::seed_window`]: the
//! settings of the config and the choices as they stand now, in ONE place for every shell.
//!
//! The app also owns what the ENVIRONMENT asked of it, read once at its start
//! ([`AppSettings`], `App::create`): every window reaches it through its `AppShared`
//! ([`AppShared::settings`], `window.common.app.settings()`) and no layer below asks the process
//! environment again.

use std::sync::{
    atomic::{AtomicU8, Ordering},
    Mutex, PoisonError,
};

use azul_core::{
    resources::{AppConfig, NaturalScroll},
    window::{DarkLightMode, OptionDarkLightMode},
};
use azul_layout::{appenv::GpuProvisionHooks, window::LayoutWindow};

use crate::desktop::app_settings::AppSettings;

/// What the app chose and configured that every window of it reads: the light / dark mode and the
/// language it chose (and may switch), the platform's natural-scrolling preference, the two
/// config flags the platform layers read at an event, and the GPU provisioning hooks.
///
/// Shared as `Arc<AppShared>`: one per [`App`](crate::desktop::app::App), cloned into every
/// window the run loop builds (`CommonWindowState::app`).
#[derive(Debug)]
pub struct AppShared {
    /// The mode: `0` follows the desktop, `1` pins light, `2` pins dark.
    mode: AtomicU8,
    /// The language the app chose; `None` follows the system's.
    locale: Mutex<Option<String>>,
    /// The platform's natural-scrolling preference: `0` unknown, `1` classic, `2` natural.
    system_natural_scroll: AtomicU8,
    /// `AppConfig::synthesize_pinch_from_ctrl_wheel`.
    synthesize_pinch_from_ctrl_wheel: bool,
    /// `AppConfig::expose_system_media_controls`.
    expose_system_media_controls: bool,
    /// The driver-provisioning entry points the CPU dialogs call, set for a running app only.
    gpu_provision: Option<GpuProvisionHooks>,
    /// What the environment asked of the app, read once at its start. An app that never read the
    /// process (a test, a tool) holds the empty environment's.
    settings: AppSettings,
}

/// `0` follows the desktop, `1` pins light, `2` pins dark.
const fn mode_code(mode: OptionDarkLightMode) -> u8 {
    match mode {
        OptionDarkLightMode::None => 0,
        OptionDarkLightMode::Some(DarkLightMode::Light) => 1,
        OptionDarkLightMode::Some(DarkLightMode::Dark) => 2,
    }
}

impl AppShared {
    /// The state of an app from its config alone: the mode it starts in, the two flags, no
    /// language chosen yet, no natural-scrolling preference known, no GPU hooks, and the empty
    /// environment's settings. Nothing is read from the platform, so a test that builds windows
    /// from a config reads nothing of the machine it runs on.
    #[must_use]
    pub fn new(config: &AppConfig) -> Self {
        Self::with_settings(config, AppSettings::default())
    }

    /// [`Self::new`] for an app whose environment asked for `settings` (a test of a window that
    /// must see a switch on, a host that reads its own source).
    #[must_use]
    pub fn with_settings(config: &AppConfig, settings: AppSettings) -> Self {
        Self {
            mode: AtomicU8::new(mode_code(config.mode)),
            locale: Mutex::new(None),
            system_natural_scroll: AtomicU8::new(0),
            synthesize_pinch_from_ctrl_wheel: config.synthesize_pinch_from_ctrl_wheel,
            expose_system_media_controls: config.expose_system_media_controls,
            gpu_provision: None,
            settings,
        }
    }

    /// The state of a RUNNING app ([`Self::with_settings`], with the `settings` its environment
    /// asked for, read once by `App::create`), plus what only a running app has: the platform's
    /// natural-scrolling preference when the config asks for it (`NaturalScroll::System`), read
    /// once here, and the GPU provisioning hooks.
    #[must_use]
    pub fn for_app(config: &AppConfig, settings: AppSettings) -> Self {
        let mut shared = Self::with_settings(config, settings);
        if config.natural_scroll == NaturalScroll::System {
            shared.set_system_natural_scroll(
                crate::desktop::extra::natural_scroll::read_system_preference(),
            );
        }
        shared.gpu_provision = Some(gpu_provision_hooks());
        shared
    }

    /// What the environment asked of this app, read once at its start.
    #[must_use]
    pub const fn settings(&self) -> &AppSettings {
        &self.settings
    }

    /// The app's mode: `None` follows the desktop.
    #[must_use]
    pub fn mode(&self) -> OptionDarkLightMode {
        match self.mode.load(Ordering::Relaxed) {
            1 => OptionDarkLightMode::Some(DarkLightMode::Light),
            2 => OptionDarkLightMode::Some(DarkLightMode::Dark),
            _ => OptionDarkLightMode::None,
        }
    }

    /// The app chooses its mode (`CallbackInfo::set_mode`): every window built from now on starts
    /// in it, and the windows already open adopt it through their own trigger.
    pub fn set_mode(&self, mode: OptionDarkLightMode) {
        self.mode.store(mode_code(mode), Ordering::Relaxed);
    }

    /// The language the app chose; `None` follows the system's.
    #[must_use]
    pub fn locale(&self) -> Option<String> {
        self.locale
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The app chooses its language (`CallbackInfo::set_locale`; `None`, or an empty tag, goes
    /// back to the system's): a window built from now on starts in it.
    pub fn set_locale(&self, locale: Option<&str>) {
        let chosen = locale
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string);
        *self.locale.lock().unwrap_or_else(PoisonError::into_inner) = chosen;
    }

    /// The platform's natural-scrolling preference, if one was read or reported: `Some(true)`
    /// natural, `Some(false)` classic.
    #[must_use]
    pub fn system_natural_scroll(&self) -> Option<bool> {
        match self.system_natural_scroll.load(Ordering::Relaxed) {
            1 => Some(false),
            2 => Some(true),
            _ => None,
        }
    }

    /// A backend reports the platform's natural-scrolling preference (Wayland does, with each
    /// pointer axis event): a window built from now on starts with it.
    pub fn set_system_natural_scroll(&self, natural: Option<bool>) {
        let code = match natural {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        self.system_natural_scroll.store(code, Ordering::Relaxed);
    }

    /// Whether Ctrl + wheel is a pinch (`AppConfig::synthesize_pinch_from_ctrl_wheel`).
    #[must_use]
    pub const fn synthesize_pinch_from_ctrl_wheel(&self) -> bool {
        self.synthesize_pinch_from_ctrl_wheel
    }

    /// Whether the app is registered with the platform as a media player
    /// (`AppConfig::expose_system_media_controls`).
    #[must_use]
    pub const fn expose_system_media_controls(&self) -> bool {
        self.expose_system_media_controls
    }

    /// Everything a window takes from its app when it is built, in one place for every shell:
    /// the settings of the config (localization, system animations, natural-scroll mode, the
    /// snapshot the engine services read, E2E hooks, node identity), the GPU hooks, and the
    /// app's choices as they stand now (the mode, the language, the natural-scrolling
    /// preference).
    pub fn seed_window(&self, layout_window: &mut LayoutWindow, config: &AppConfig) {
        layout_window.set_app_localization(config);
        layout_window.set_app_settings(config);
        layout_window.set_app_e2e(config);
        layout_window.set_app_node_identity(config);
        layout_window.gpu_provision = self.gpu_provision;
        layout_window.mode = self.mode();
        layout_window
            .scroll_manager
            .set_system_natural_scroll(self.system_natural_scroll());
        if let Some(locale) = self.locale() {
            let _ = layout_window.set_locale(&locale);
        }
    }
}

/// The driver-provisioning entry points the CPU dialogs call. They live in the dll
/// (`video_codec::provision`) and the dialogs live BELOW it in azul-layout, so the dll hands
/// fn pointers rather than the type. `check` is inspection only; `remediate` is the
/// consent-gated, pkexec-elevated repair.
fn gpu_provision_hooks() -> GpuProvisionHooks {
    GpuProvisionHooks {
        check: || {
            let c = crate::unified::video_codec::provision::VideoStartupCheck::run();
            azul_layout::appenv::GpuProvisionReport {
                hw_decode_ready: c.hw_decode_ready,
                boot_safe: c.boot_safe,
                can_remediate: c.can_remediate,
                needs_reboot: c.needs_reboot,
                summary: c.summary.as_str().to_string(),
                detail: c.detail.as_str().to_string(),
            }
        },
        remediate: |on_step| {
            let o = crate::unified::video_codec::provision::VideoStartupCheck::
                remediate_with_progress(on_step);
            azul_layout::appenv::GpuProvisionOutcome {
                ok: o.ok,
                reboot_required: o.reboot_required,
                message: o.message.as_str().to_string(),
            }
        },
    }
}

#[cfg(test)]
#[path = "app_shared_test.rs"]
mod app_shared_test;
