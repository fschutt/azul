//! What the windows of ONE app share: the queues between the platform and the windows.
//!
//! The platform answers on its own threads (a fingerprint sheet, a notification click, a
//! gamepad poll) and a window's callbacks ask the platform for things; neither can hold a
//! `CallbackInfo` or a `LayoutWindow`. [`AppServices`] is the set of queues and slots that
//! carry those messages, owned by the app (`azul-dll`'s `App` builds one) and cloned into
//! every window it opens ([`LayoutWindow::join_app`]), and from there into the callbacks it
//! hands the platform. Nothing is looked up by name: two apps in a process, two windows that
//! never joined one, or two tests, each have queues of their own.

use alloc::sync::Arc;

use super::{
    app_target::ActivationClock,
    biometric::BiometricChannels,
    clipboard::SystemClipboard,
    device_events::SessionLog,
    eyedropper::EyedropperChannel,
    gamepad::GamepadStates,
    geolocation::GeolocationChannel,
    global_hotkey::{SharedGlobalHotkeys, WindowHotkeys},
    hid::HidChannel,
    keyring::KeyringChannels,
    media_keys::MediaKeyQueues,
    notification::NotificationChannels,
    permission::AsyncPermissionResults,
    sensors::SensorChannel,
    tray_event::TrayEvents,
    window_activation::RaiseRequests,
};
use crate::window::LayoutWindow;

/// The queues and slots the windows of one app share. Cloning gives another handle on the SAME
/// set; [`AppServices::new`] (or `default`) builds a set of its own.
#[derive(Debug, Clone, Default)]
pub struct AppServices {
    /// The platform's biometric prompt.
    pub biometric: BiometricChannels,
    /// The platform's keyring.
    pub keyring: KeyringChannels,
    /// The screen eyedropper (the picks in flight and their answers).
    pub eyedropper: EyedropperChannel,
    /// The outcomes of permission prompts the platform resolved.
    pub permission: AsyncPermissionResults,
    /// The gamepad states the platform polled.
    pub gamepad: GamepadStates,
    /// The motion sensors' readings and the proximity answer.
    pub sensors: Arc<SensorChannel>,
    /// The raw HID reports and the device list.
    pub hid: Arc<HidChannel>,
    /// The location fixes and errors.
    pub geolocation: Arc<GeolocationChannel>,
    /// The media keys, seeks and system audio changes the platform's media controls report.
    pub media_keys: MediaKeyQueues,
    /// The "raise this window" requests of the threads that cannot raise it themselves.
    pub raise: RaiseRequests,
    /// The tray's events and the one whose delivery runs now.
    pub tray: TrayEvents,
    /// The session changes (lock, sleep) the platform reports.
    pub session: SessionLog,
    /// The native notifications: requests, events, deliveries, the app handler.
    pub notifications: NotificationChannels,
    /// The app's global hotkeys.
    pub hotkeys: SharedGlobalHotkeys,
    /// The clock that orders the app's windows by creation and focus.
    pub activation_clock: ActivationClock,
    /// The shell's system clipboard reader.
    pub clipboard: SystemClipboard,
    /// The workers of the app's closed windows that are still stopping.
    #[cfg(all(feature = "std", feature = "text_layout"))]
    pub stopping_workers: super::thread_owner::StoppingWorkers,
}

impl AppServices {
    /// A new set no window has joined yet, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl LayoutWindow {
    /// Make this window one of the app behind `services`: it uses the app's queues from now
    /// on instead of its own, and takes its place in the app's global hotkeys. Whatever the
    /// window queued before is dropped, so a window joins before any callback of it runs (the
    /// shells call it when they create the window).
    pub fn join_app(&mut self, services: &AppServices) {
        self.device_event_manager.use_session_log(&services.session);
        self.global_hotkeys = WindowHotkeys::new(services.hotkeys.clone());
        self.app_services = services.clone();
    }
}

#[cfg(test)]
#[path = "app_services_test.rs"]
mod app_services_test;
