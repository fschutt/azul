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
//! | wakes the loop by itself | yes | yes | yes - the grab connection's fd is in the poll set | yes - the listener raises the loop waker |
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
//! pump per iteration (`pump_macos_windows`, `pump_win32_windows`,
//! `pump_linux_windows`, `pump_headless`), which syncs the manager and runs
//! each press through `PlatformWindow::invoke_menu_callback` - the route a
//! tray menu click takes, because a `CallbackInfo` needs a window - against
//! the window that OWNS it: its declaring window (the most recently focused
//! of several, then the oldest), or for an `AppConfig` hotkey the most
//! recently focused window, then the oldest. Never "the first window" of a
//! registry, whose order is an address / hash order.
//!
//! The pumps also run the relayouts a status change owes (a `layout()` that
//! read a status that moved runs once more), and the `AppConfig`'s derived
//! hotkeys callback when it is due.

use azul_core::events::ProcessEventResult;
use azul_layout::managers::global_hotkey::{
    self as manager, GlobalHotkeyBackend, GlobalHotkeyProbe, HotkeyDelivery, HotkeySink,
    HotkeyTurn, SharedGlobalHotkeys, WindowSeq,
};

use crate::desktop::shell2::common::event::{MenuInvocation, PlatformWindow};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod portal;
/// The portal backend's batch planner - pure, so it is tested on every host.
mod portal_plan;
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

/// For a run loop that can be woken from another thread (an eventfd write, a
/// condvar notify): hand the current App's hotkey sink that waker, so a
/// press arriving on a backend thread (the portal's D-Bus listener, the
/// headless simulation) wakes the loop instead of waiting for its next poll.
///
/// Pass `watches_wake_fds = true` only if the loop ALSO has every fd of
/// [`wake_fds`] in its poll set (X11's grab connection); then
/// [`needs_loop_polling`] answers `false` for every backend and the loop may
/// park indefinitely. This is the integration point for the Linux loop-waker
/// rework: call it once the shared loop waker exists, before the loop parks.
pub fn attach_loop_waker(waker: manager::LoopWaker, watches_wake_fds: bool) {
    if let Some(app) = app() {
        app.attach_loop_waker(waker, watches_wake_fds);
    }
}

/// The fds a Linux loop should poll so a hotkey press wakes it: X11's grab
/// connection once it is open (empty for every other backend, and before
/// the first grab).
#[must_use]
pub fn wake_fds() -> Vec<i32> {
    app().and_then(|app| app.wake_fd()).into_iter().collect()
}

/// Is a press owed that no fd of [`wake_fds`] will announce? One the
/// backend already handed to the sink (the portal's listener, the headless
/// simulation), or X events the grab connection's Xlib already read into its
/// queue (during the `XSync` of a grab), which have left the socket.
/// `loop_waker::must_not_park` asks before a Linux loop parks.
#[must_use]
pub fn has_buffered_input() -> bool {
    app().is_some_and(|app| app.has_buffered_input())
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

/// The sequence number a window declares its global hotkeys under (0, which
/// no window ever gets, for one still without a layout window).
fn seq_of<W: PlatformWindow>(window: &W) -> WindowSeq {
    window
        .get_layout_window()
        .map_or(0, |lw| lw.global_hotkeys.seq())
}

/// Presses with no live window to run against are reported, never run
/// against some other window.
fn report_undeliverable(turn: &HotkeyTurn) {
    for delivery in &turn.undeliverable {
        crate::plog_debug!(
            "[global-hotkey] {} pressed, but its window is gone (or no window exists): dropped",
            delivery.event.hotkey.to_display_string().as_str()
        );
    }
}

/// Run one turn against `windows` (the pointers `turn`'s indices refer to):
/// every press against its owner window, a relayout for every window whose
/// `layout()` read a status that moved, then `after` once per window whose
/// result asks for something. (The Linux pump inlines it: its registry
/// holds an enum of two window types.)
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run_turn<W: PlatformWindow>(
    app: &SharedGlobalHotkeys,
    turn: HotkeyTurn,
    windows: &[*mut W],
    mut after: impl FnMut(&mut W, ProcessEventResult),
) {
    report_undeliverable(&turn);
    let mut results = vec![ProcessEventResult::DoNothing; windows.len()];
    for (index, delivery) in turn.deliveries {
        let Some(&ptr) = windows.get(index) else {
            continue;
        };
        // SAFETY: the registry's pointers are valid for this loop turn; one
        // window is borrowed at a time.
        let window = unsafe { &mut *ptr };
        results[index] = results[index].max(deliver(window, app, delivery));
    }
    for index in turn.relayout {
        let Some(&ptr) = windows.get(index) else {
            continue;
        };
        let window = unsafe { &mut *ptr };
        window.request_regeneration(azul_core::callbacks::RelayoutReason::Other);
        results[index] =
            results[index].max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
    }
    for (index, result) in results.into_iter().enumerate() {
        if matches!(result, ProcessEventResult::DoNothing) {
            continue;
        }
        if let Some(&ptr) = windows.get(index) {
            after(unsafe { &mut *ptr }, result);
        }
    }
}

/// Run one press against `window`, with the press readable through
/// `CallbackInfo::get_global_hotkey_event` while its callback runs.
///
/// An APP-level press that asks for a rebuild changed app state no single
/// window owns: every window rebuilds, and the `AppConfig`'s hotkeys
/// callback re-derives its set.
pub(crate) fn deliver<W: PlatformWindow>(
    window: &mut W,
    app: &SharedGlobalHotkeys,
    delivery: HotkeyDelivery,
) -> ProcessEventResult {
    crate::plog_debug!(
        "[global-hotkey] {} pressed",
        delivery.event.hotkey.to_display_string().as_str()
    );
    let app_level = delivery.target == manager::HotkeySource::App;
    let callback = delivery.callback;
    let result = manager::with_delivered_event(delivery.event, || {
        window.invoke_menu_callback(
            callback,
            MenuInvocation::Native {
                site: "global_hotkey",
            },
        )
    });
    if app_level && !matches!(result, ProcessEventResult::DoNothing) {
        app.mark_app_dirty();
        if matches!(
            result,
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
                | ProcessEventResult::ShouldRegenerateDomAllWindows
        ) {
            window.request_regeneration_all_windows();
        }
    }
    result
}

/// One turn of the headless loop (and of the tray-only stub): poll, sync,
/// run every press against `window`, and - when a sync moved a status this
/// window's last `layout()` read - ask for that pass to run once more.
///
/// Acts on the WINDOW's manager (the App it joined), not on whatever App is
/// current, so a test can drive a window it built directly.
pub fn pump_headless(
    window: &mut crate::desktop::shell2::headless::HeadlessWindow,
) -> ProcessEventResult {
    let Some(shared) = window
        .get_layout_window()
        .map(|lw| lw.global_hotkeys.shared().clone())
    else {
        return ProcessEventResult::DoNothing;
    };
    let turn = shared.begin_turn(&[seq_of(&*window)]);
    report_undeliverable(&turn);
    let mut result = ProcessEventResult::DoNothing;
    for (_, delivery) in turn.deliveries {
        result = result.max(deliver(window, &shared, delivery));
    }
    if !turn.relayout.is_empty() {
        window.request_regeneration(azul_core::callbacks::RelayoutReason::Other);
        result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
    }
    result
}

/// macOS: one hotkey turn over every window. Called from both macOS run
/// loops (`NSApplication::run`'s drain timer and the manual loop), next to
/// the tray pump.
#[cfg(target_os = "macos")]
pub(crate) fn pump_macos_windows() {
    let Some(app) = app() else {
        return;
    };
    let windows = crate::desktop::shell2::macos::registry::get_all_window_ptrs();
    // SAFETY: registry pointers are valid for this loop turn.
    let live: Vec<WindowSeq> = windows.iter().map(|w| seq_of(unsafe { &**w })).collect();
    let turn = app.begin_turn(&live);
    run_turn(&app, turn, &windows, |window, _| window.request_redraw());
}

/// Windows: one hotkey turn over every window. Called from the run loop
/// after the message drain (which is what ran the `WM_HOTKEY` window
/// procedure).
#[cfg(target_os = "windows")]
pub(crate) fn pump_win32_windows() {
    use crate::desktop::shell2::windows::registry;
    let Some(app) = app() else {
        return;
    };
    let windows: Vec<*mut crate::desktop::shell2::windows::Win32Window> =
        registry::get_all_window_handles()
            .into_iter()
            .filter_map(registry::get_window)
            .collect();
    // SAFETY: registry pointers are valid for this loop turn.
    let live: Vec<WindowSeq> = windows.iter().map(|w| seq_of(unsafe { &**w })).collect();
    let turn = app.begin_turn(&live);
    run_turn(&app, turn, &windows, |window, _| window.request_redraw());
}

/// X11 / Wayland: poll the grab connection, then one hotkey turn over every
/// window.
#[cfg(az_x11)]
pub(crate) fn pump_linux_windows() {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};
    let Some(app) = app() else {
        return;
    };
    let windows: Vec<*mut LinuxWindow> = registry::get_all_window_ids()
        .into_iter()
        .filter_map(|id| unsafe { registry::get_window(id) })
        .collect();
    // SAFETY: registry pointers are valid for this loop turn.
    let live: Vec<WindowSeq> = windows
        .iter()
        .map(|w| match unsafe { &**w } {
            LinuxWindow::X11(x) => seq_of(x),
            #[cfg(target_os = "linux")]
            LinuxWindow::Wayland(wl) => seq_of(wl),
        })
        .collect();
    let turn = app.begin_turn(&live);
    report_undeliverable(&turn);
    let mut results = vec![ProcessEventResult::DoNothing; windows.len()];
    for (index, delivery) in turn.deliveries {
        let Some(&ptr) = windows.get(index) else {
            continue;
        };
        let result = match unsafe { &mut *ptr } {
            LinuxWindow::X11(x) => deliver(x, &app, delivery),
            #[cfg(target_os = "linux")]
            LinuxWindow::Wayland(wl) => deliver(wl, &app, delivery),
        };
        results[index] = results[index].max(result);
    }
    for index in turn.relayout {
        let Some(&ptr) = windows.get(index) else {
            continue;
        };
        match unsafe { &mut *ptr } {
            LinuxWindow::X11(x) => {
                x.request_regeneration(azul_core::callbacks::RelayoutReason::Other);
            }
            #[cfg(target_os = "linux")]
            LinuxWindow::Wayland(wl) => {
                wl.request_regeneration(azul_core::callbacks::RelayoutReason::Other);
            }
        }
        results[index] =
            results[index].max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
    }
    for (index, result) in results.into_iter().enumerate() {
        if matches!(result, ProcessEventResult::DoNothing) {
            continue;
        }
        let Some(&ptr) = windows.get(index) else {
            continue;
        };
        match unsafe { &mut *ptr } {
            LinuxWindow::X11(x) => x.request_redraw(),
            #[cfg(target_os = "linux")]
            LinuxWindow::Wayland(wl) => wl.request_redraw(),
        }
    }
}
