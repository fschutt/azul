//! The services of the App that runs in this process, for the platform code that has no
//! handle to it.
//!
//! The windows, the callbacks and the backends the pump starts are handed their App's
//! [`AppServices`] explicitly. A few layers are not: the platform's own callbacks (a
//! `UNUserNotificationCenter` delegate, an AppKit tray action, a D-Bus signal handler, the MPRIS
//! thread, a message window's procedure), which the OS calls with no context but the handler they
//! registered, and the run loops' collectors and wakers (`app_events`, `loop_waker`, the
//! hotkey pumps), which are themselves process-wide by construction (one loop per process, one
//! window registry per process). They read the App's services here, where `App::run` put them.
//!
//! One App runs per process: a later [`register`] replaces the earlier one (an App's services
//! live as long as the App, and nothing registered here outlives the next run).

use std::sync::RwLock;

use azul_layout::managers::app_services::AppServices;

/// The App that runs in this process. OS-forced: see the module docs.
static SERVICES: RwLock<Option<AppServices>> = RwLock::new(None);

/// Make `services` the App's of this process: what the platform callbacks and the run loops'
/// collectors reach. `App::run` calls it before any window or run loop exists, so the tap that
/// launched the app finds it.
pub fn register(services: &AppServices) {
    *SERVICES
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(services.clone());
}

/// The App's services, if an App runs in this process.
#[must_use]
pub fn services() -> Option<AppServices> {
    SERVICES
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
