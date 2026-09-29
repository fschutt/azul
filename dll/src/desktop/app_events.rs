//! The app-level event collector: tray menu clicks, plain tray clicks and
//! native-notification events, taken out of their mailboxes in ONE place and
//! run against ONE window picked by ONE rule. (Global-hotkey presses belong
//! to the window that declared them: the App's hotkey manager routes those,
//! `desktop::global_hotkey::pump_*`.)
//!
//! # Why one collector
//!
//! The three sources are the same shape - an OS callback that cannot hold a
//! `CallbackInfo` parks something in a process-wide mailbox, and the run
//! loop runs the app's callback later through `invoke_menu_callback` - but
//! each run loop used to pump them one by one, in different places:
//!
//! * the macOS manual loop pumped the tray only at the TOP of an iteration, so a pick handled
//!   inside that iteration's `sendEvent:` waited for the NEXT event;
//! * the Win32 loop never pumped the tray at all;
//! * nothing drained the tray EVENT mailbox, so a click on the icon (anything without a menu
//!   callback of its own) never reached the app;
//! * each source picked "the first window" of the platform registry: pointer order on macOS, a
//!   `HashMap` on Linux, `HWND` order on Windows.
//!
//! Now every loop calls its platform's `deliver_to_*_windows` (or
//! [`deliver_to`] with the tray-only stub), which services the sources
//! (`loop_waker::service_sources`), collects every mailbox, and runs the
//! lot against the window `azul_layout::managers::app_target` picks: the
//! most recently focused window, else the oldest.

use azul_core::{events::ProcessEventResult, menu::CoreMenuCallback};
use azul_layout::managers::{
    app_target::{pick_app_target, AppTargetCandidate},
    notification::NotificationDelivery,
    tray_event::{with_current_tray_event, TrayDelivery},
};

use crate::desktop::shell2::common::event::{CommonWindowState, MenuInvocation, PlatformWindow};

/// Everything the app-level sources owe the app since the last collection.
#[derive(Debug, Default)]
pub(crate) struct AppEvents {
    /// Tray menu items that carry a callback.
    tray_menu: Vec<CoreMenuCallback>,
    /// Every other tray event (a click on the icon, a middle click, a
    /// scroll, a menu item without a callback), routed to the tray's own
    /// callback.
    tray: Vec<TrayDelivery>,
    /// Notification events, routed to their notification's callback.
    notifications: Vec<NotificationDelivery>,
    /// `AZ_RICING=watch` saw a rice file change (`azul_css::rice::poll_watch`,
    /// on the watcher thread): every window rebuilds with the reloaded rice.
    rice_reloaded: bool,
}

impl AppEvents {
    /// Service the sources (acknowledge the loop waker, drain D-Bus, read
    /// the hotkey grab connection into the App's sink) and take every
    /// mailbox. Event-loop thread.
    #[must_use]
    pub(crate) fn collect() -> Self {
        crate::desktop::loop_waker::service_sources();
        Self {
            // First: on macOS the tray pump also files callback-less menu
            // items and icon clicks into the tray event mailbox, which the
            // next field drains.
            tray_menu: crate::desktop::tray::pump_tray(),
            tray: crate::desktop::tray::take_tray_deliveries(),
            notifications: crate::desktop::notifications::pump_notifications(),
            rice_reloaded: azul_css::rice::take_reload_signal(),
        }
    }

    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.tray_menu.is_empty()
            && self.tray.is_empty()
            && self.notifications.is_empty()
            && !self.rice_reloaded
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.tray_menu.len()
            + self.tray.len()
            + self.notifications.len()
            + usize::from(self.rice_reloaded)
    }

    /// Run everything against `window`, in the order the sources were
    /// collected. Returns the strongest result any callback asked for.
    pub(crate) fn invoke<W: PlatformWindow>(self, window: &mut W) -> ProcessEventResult {
        let Self {
            tray_menu,
            tray,
            notifications,
            rice_reloaded,
        } = self;
        let mut result = ProcessEventResult::DoNothing;
        if rice_reloaded {
            // The app-theme rebuild: this window now, the others through the
            // registry walk; each adopts the reloaded rice in
            // `regenerate_layout` (its rice generation lags).
            result = result.max(window.rebuild_all_windows_for_app_theme());
        }
        for callback in tray_menu {
            result = result.max(window.invoke_menu_callback(
                callback,
                MenuInvocation::Native { site: "tray_menu" },
            ));
        }
        for delivery in tray {
            let TrayDelivery { callback, event } = delivery;
            // Installed for the call, so the callback can ask
            // `CallbackInfo::get_tray_event` which event it runs for.
            result = result.max(with_current_tray_event(&event, || {
                window.invoke_menu_callback(callback, MenuInvocation::Native { site: "tray" })
            }));
        }
        if !notifications.is_empty()
            && crate::desktop::notifications::invoke_deliveries(window, notifications)
        {
            result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }
        result
    }
}

/// Collect and run against a window the caller already has - the tray-only
/// app's headless stub, which is the only window such an app has.
pub(crate) fn deliver_to<W: PlatformWindow>(window: &mut W) -> ProcessEventResult {
    let events = AppEvents::collect();
    if events.is_empty() {
        return ProcessEventResult::DoNothing;
    }
    events.invoke(window)
}

/// Describe one window for [`pick_app_target`]. Menus and tooltips are
/// transient: a target only when no other window is open.
fn candidate<K: Copy>(key: K, common: &CommonWindowState) -> AppTargetCandidate<K> {
    use azul_core::window::WindowType;
    AppTargetCandidate {
        key,
        order: common.app_order,
        transient: matches!(
            common.current_window_state().flags.window_type,
            WindowType::Menu | WindowType::Tooltip
        ),
    }
}

/// Collected events had nowhere to run. Notification deliveries are parked
/// (`notifications::defer_deliveries`) - the tap that launches an app arrives
/// before its first window - and the next pump that has a window runs them.
/// The rest is said, not silent: a dropped callback is indistinguishable from
/// a broken one otherwise.
fn report_undelivered(events: AppEvents) {
    // A rice reload with no window to rebuild needs nothing: a window built
    // later styles with the rice as it stands then.
    let AppEvents {
        tray_menu,
        tray,
        notifications,
        rice_reloaded: _,
    } = events;
    if !notifications.is_empty() {
        crate::desktop::notifications::defer_deliveries(notifications);
    }
    let dropped = tray_menu.len() + tray.len();
    if dropped > 0 {
        crate::plog_debug!("[app-events] {dropped} tray event(s) had no window to run against");
    }
}

/// macOS: the RunForever drain timer and both points of the manual loop (the
/// top, and right before `runMode:beforeDate:` parks).
#[cfg(target_os = "macos")]
pub(crate) fn deliver_to_macos_windows() {
    use crate::desktop::shell2::macos::registry;

    let events = AppEvents::collect();
    if events.is_empty() {
        return;
    }
    let candidates: Vec<AppTargetCandidate<*mut crate::desktop::shell2::macos::MacOSWindow>> =
        registry::get_all_window_ptrs()
            .into_iter()
            .filter(|p| !p.is_null())
            .map(|p| candidate(p, unsafe { &(*p).common }))
            .collect();
    let Some(wptr) = pick_app_target(&candidates) else {
        report_undelivered(events);
        return;
    };
    // Safe: registry pointers stay valid while registered, and this runs on
    // the main thread between event dispatches, with no other borrow live.
    let window = unsafe { &mut *wptr };
    if !matches!(events.invoke(window), ProcessEventResult::DoNothing) {
        window.request_redraw();
    }
}

/// Win32: after the thread-queue drain, which ran the `WM_HOTKEY` and
/// notify-window procedures that filled the mailboxes.
#[cfg(target_os = "windows")]
pub(crate) fn deliver_to_win32_windows() {
    use crate::desktop::shell2::windows::registry;

    let events = AppEvents::collect();
    if events.is_empty() {
        return;
    }
    let candidates: Vec<AppTargetCandidate<*mut crate::desktop::shell2::windows::Win32Window>> =
        registry::get_all_window_handles()
            .into_iter()
            .filter_map(registry::get_window)
            .map(|p| candidate(p, unsafe { &(*p).common }))
            .collect();
    let Some(wptr) = pick_app_target(&candidates) else {
        report_undelivered(events);
        return;
    };
    // Safe: see the macOS twin; the Win32 loop runs this outside any window
    // procedure.
    let window = unsafe { &mut *wptr };
    if !matches!(events.invoke(window), ProcessEventResult::DoNothing) {
        window.request_redraw();
    }
}

/// X11 / Wayland: at the top of every loop iteration. The loops park on the
/// sources' descriptors (`loop_waker::wait_fds`), so this runs as soon as
/// one of them has something.
///
/// A click on a notification also raises the window the events run
/// against: the freedesktop server sends an `ActivationToken` just before
/// the click's `ActionInvoked`, and on Wayland spending that token
/// (`xdg_activation_v1.activate`) is the only way an app may take focus for
/// a click that carries no input serial. It is taken right after the
/// collection - which dispatched the D-Bus signals that bring it - also when
/// the click has no callback to run, and never left behind for the next,
/// unrelated delivery. X11 drops it (a startup id there).
#[cfg(az_x11)]
pub(crate) fn deliver_to_linux_windows() {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};

    let events = AppEvents::collect();
    let activation_token = crate::desktop::notifications::take_activation_token();
    if events.is_empty() && activation_token.is_none() {
        return;
    }
    let candidates: Vec<AppTargetCandidate<*mut LinuxWindow>> = registry::get_all_window_ids()
        .into_iter()
        .filter_map(|id| unsafe { registry::get_window(id) })
        .map(|p| {
            let common = match unsafe { &*p } {
                LinuxWindow::X11(w) => &w.common,
                #[cfg(target_os = "linux")]
                LinuxWindow::Wayland(w) => &w.common,
            };
            candidate(p, common)
        })
        .collect();
    let Some(wptr) = pick_app_target(&candidates) else {
        report_undelivered(events);
        return;
    };
    // Safe: see the macOS twin; the Linux loop runs this before it polls its
    // windows, with no window borrowed.
    match unsafe { &mut *wptr } {
        LinuxWindow::X11(w) => {
            if !matches!(events.invoke(w), ProcessEventResult::DoNothing) {
                w.request_redraw();
            }
        }
        #[cfg(target_os = "linux")]
        LinuxWindow::Wayland(w) => {
            // Raise first, so what the callback shows is in front.
            if let Some(token) = activation_token.as_deref() {
                if !w.activate_with_token(token) {
                    crate::plog_debug!(
                        "[notifications] the activation token of a notification click could \
                         not be spent: the compositor has no xdg_activation_v1"
                    );
                }
            }
            if !matches!(events.invoke(w), ProcessEventResult::DoNothing) {
                w.request_redraw();
            }
        }
    }
}
