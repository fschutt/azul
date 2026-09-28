//! System-wide ("global") hotkeys - platform dispatch.
//!
//! The model lives in `azul_core::global_hotkey`, the App-owned manager that
//! reconciles the declared set against the OS in
//! `azul_layout::managers::global_hotkey`; this module is the OS plumbing and
//! the run-loop hook.
//!
//! | | macOS | Windows | Linux, X11 session | Linux, Wayland session |
//! |---|---|---|---|---|
//! | mechanism | Carbon `RegisterEventHotKey` | `RegisterHotKey` | `XGrabKey` on the root window | portal `GlobalShortcuts` |
//! | permission | none | none | none | the desktop may ask the user |
//! | a press arrives as | an NSEvent -> Carbon handler | `WM_HOTKEY` on a message-only window | `KeyPress` on a 2nd X connection | `Activated` on a D-Bus thread |
//! | wakes the loop by itself | yes | yes | no - the loop polls (or watches `wake_fd`) | no - the loop polls (or attaches a waker) |
//! | "another app owns it" | `eventHotKeyExistsErr` | `ERROR_HOTKEY_ALREADY_REGISTERED` | `BadAccess` | the desktop decides |
//! | can be absent | no | no | no `$DISPLAY` | no portal backend (probe) |
//!
//! iOS, Android and the web have no global hotkeys: no backend is installed
//! and every declaration reads `Failed(Unsupported)`. A headless run
//! installs the simulated backend, which grabs nothing and fires through
//! `SharedGlobalHotkeys::simulate` (and the `AZ_E2E` `global_hotkey` op).
//!
//! # Which App
//!
//! `App::run` makes its `SharedGlobalHotkeys` the event-loop thread's current
//! App for the whole run ([`app`]); the pumps and the probe below act on it.
//!
//! # Delivery
//!
//! The backends only park presses in their sink. Each run loop calls its
//! pump per iteration, which syncs the manager and runs the pressed
//! accelerators' callbacks through `PlatformWindow::invoke_menu_callback` -
//! the route a tray menu click takes, and for the same reason: a
//! `CallbackInfo` needs a window.

use azul_core::events::ProcessEventResult;
use azul_layout::managers::global_hotkey::{
    self as manager, GlobalHotkeyBackend, GlobalHotkeyProbe, HotkeyDelivery, HotkeySink,
    SharedGlobalHotkeys,
};

use crate::desktop::shell2::common::event::{MenuInvocation, PlatformWindow};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

/// This platform's backend for this session, reporting through `sink`, or
/// `None` where there is none (iOS, Android). Nothing is grabbed until the
/// manager asks for the first accelerator.
///
/// On Linux the SESSION decides, not azul's own window backend: under
/// Wayland an X grab only sees keys while an XWayland window has the focus,
/// which is no global hotkey at all - so a Wayland session always goes
/// through the portal, even when azul draws its windows through XWayland.
#[must_use]
pub fn platform_backend(sink: HotkeySink) -> Option<Box<dyn GlobalHotkeyBackend>> {
    #[cfg(target_os = "macos")]
    let backend: Box<dyn GlobalHotkeyBackend> = Box::new(macos::CarbonBackend::new(sink));
    #[cfg(target_os = "windows")]
    let backend: Box<dyn GlobalHotkeyBackend> = Box::new(windows::Win32Backend::new(sink));
    #[cfg(target_os = "linux")]
    let backend: Box<dyn GlobalHotkeyBackend> = if wayland_session() {
        Box::new(portal::PortalBackend::new(sink))
    } else {
        Box::new(x11::X11Backend::new(sink))
    };
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        Some(backend)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = sink;
        None
    }
}

#[cfg(target_os = "linux")]
fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty())
}

/// The App whose loop runs on this thread (`App::run` enters it).
#[must_use]
pub fn app() -> Option<SharedGlobalHotkeys> {
    SharedGlobalHotkeys::current()
}

/// Choose this platform's backend for the current App WITHOUT building it:
/// the manager builds it at its first sync - unless the run turns out
/// headless first and installs the simulation, in which case the real OS is
/// never touched. Called by `App::run`.
pub fn choose_platform_backend() {
    if let Some(app) = app() {
        app.set_pending_backend(platform_backend);
    }
}

/// Swap in the headless simulation. Called when a run turns out headless.
pub fn install_simulated_backend() {
    if let Some(app) = app() {
        app.install_simulated_backend();
        crate::plog_debug!(
            "[global-hotkey] headless run: simulated backend installed, nothing is grabbed at \
             the OS"
        );
    }
}

/// Must the run loop wake up by itself right now? True while something is
/// grabbed on a backend whose presses arrive on something the loop does not
/// wait on (X11's second connection, the portal's D-Bus thread, the headless
/// simulation) and no waker is attached. The X11 / Wayland / headless loops
/// cap their park on this, exactly as they do for a live tray.
#[must_use]
pub fn needs_loop_polling() -> bool {
    app().is_some_and(|app| app.needs_loop_polling())
}

/// The capability probe's answer, for [`crate::desktop::extra::capability`].
/// Pure: it never installs a backend. With a backend installed for the
/// current App it asks that one (a headless run reports the simulation);
/// otherwise it asks the platform's.
#[must_use]
pub fn probe() -> GlobalHotkeyProbe {
    if let Some(app) = app() {
        if app.backend_name().is_some() {
            return app.probe();
        }
    }
    platform_probe()
}

fn probe_result(backend: &'static str, result: Result<(), String>) -> GlobalHotkeyProbe {
    match result {
        Ok(()) => GlobalHotkeyProbe {
            available: true,
            backend,
            reason: String::new(),
        },
        Err(reason) => GlobalHotkeyProbe {
            available: false,
            backend,
            reason,
        },
    }
}

fn platform_probe() -> GlobalHotkeyProbe {
    #[cfg(target_os = "macos")]
    {
        probe_result(macos::NAME, macos::probe())
    }
    #[cfg(target_os = "windows")]
    {
        probe_result(windows::NAME, windows::probe())
    }
    #[cfg(target_os = "linux")]
    {
        if wayland_session() {
            probe_result(portal::NAME, portal::probe())
        } else {
            probe_result(x11::NAME, x11::probe())
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = probe_result;
        GlobalHotkeyProbe {
            available: false,
            backend: "none",
            reason: String::from("no global-hotkey backend exists on this platform"),
        }
    }
}

/// Poll the backend, sync the manager and take every press waiting to run.
fn take_deliveries(app: &SharedGlobalHotkeys) -> Vec<HotkeyDelivery> {
    let mut manager = app.lock();
    manager.poll_backend();
    let _ = manager.sync();
    manager.take_deliveries()
}

/// Run one press against `window`, with the press readable through
/// `CallbackInfo::get_global_hotkey_event` while its callback runs.
pub(crate) fn deliver<W: PlatformWindow>(
    window: &mut W,
    delivery: HotkeyDelivery,
) -> ProcessEventResult {
    crate::plog_debug!(
        "[global-hotkey] {} pressed",
        delivery.event.hotkey.to_display_string().as_str()
    );
    let callback = delivery.callback;
    manager::with_delivered_event(delivery.event, move || {
        window.invoke_menu_callback(
            callback,
            MenuInvocation::Native {
                site: "global_hotkey",
            },
        )
    })
}

/// Run the backend's per-iteration work, then every press waiting to run,
/// against `window`. Returns the strongest result the callbacks asked for;
/// `DoNothing` when nothing was pressed.
pub fn deliver_fired<W: PlatformWindow>(window: &mut W) -> ProcessEventResult {
    let Some(app) = app() else {
        return ProcessEventResult::DoNothing;
    };
    let mut result = ProcessEventResult::DoNothing;
    for delivery in take_deliveries(&app) {
        result = result.max(deliver(window, delivery));
    }
    result
}

/// macOS: deliver against the first window. Called from both macOS run
/// loops (`NSApplication::run`'s drain timer and the manual loop), next to
/// the tray pump.
#[cfg(target_os = "macos")]
pub(crate) fn pump_into_first_macos_window() {
    let Some(app) = app() else {
        return;
    };
    let deliveries = take_deliveries(&app);
    if deliveries.is_empty() {
        return;
    }
    let window_ptrs = crate::desktop::shell2::macos::registry::get_all_window_ptrs();
    let Some(&wptr) = window_ptrs.first() else {
        crate::plog_debug!("[global-hotkey] a hotkey fired but no window exists to run it");
        return;
    };
    let window = unsafe { &mut *wptr };
    let mut result = ProcessEventResult::DoNothing;
    for delivery in deliveries {
        result = result.max(deliver(window, delivery));
    }
    if !matches!(result, ProcessEventResult::DoNothing) {
        window.request_redraw();
    }
}

/// Windows: deliver against the first window. Called from the run loop
/// after the message drain (which is what ran the `WM_HOTKEY` window
/// procedure); a callback's rebuild is picked up by the loop's render pass
/// right after.
#[cfg(target_os = "windows")]
pub(crate) fn pump_into_first_win32_window() {
    use crate::desktop::shell2::windows::registry;
    let Some(app) = app() else {
        return;
    };
    let deliveries = take_deliveries(&app);
    if deliveries.is_empty() {
        return;
    }
    let Some(wptr) = registry::get_all_window_handles()
        .first()
        .and_then(|hwnd| registry::get_window(*hwnd))
    else {
        return;
    };
    let window = unsafe { &mut *wptr };
    for delivery in deliveries {
        let _ = deliver(window, delivery);
    }
}

/// X11 / Wayland: poll the grab connection, then deliver against the first
/// window.
#[cfg(az_x11)]
pub(crate) fn pump_into_first_linux_window() {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};
    let Some(app) = app() else {
        return;
    };
    let deliveries = take_deliveries(&app);
    if deliveries.is_empty() {
        return;
    }
    let Some(wptr) = registry::get_all_window_ids()
        .first()
        .and_then(|id| unsafe { registry::get_window(*id) })
    else {
        return;
    };
    match unsafe { &mut *wptr } {
        LinuxWindow::X11(w) => {
            let mut result = ProcessEventResult::DoNothing;
            for delivery in deliveries {
                result = result.max(deliver(w, delivery));
            }
            if !matches!(result, ProcessEventResult::DoNothing) {
                w.request_redraw();
            }
        }
        #[cfg(target_os = "linux")]
        LinuxWindow::Wayland(w) => {
            let mut result = ProcessEventResult::DoNothing;
            for delivery in deliveries {
                result = result.max(deliver(w, delivery));
            }
            if !matches!(result, ProcessEventResult::DoNothing) {
                w.request_redraw();
            }
        }
    }
}
