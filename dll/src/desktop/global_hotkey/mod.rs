//! System-wide ("global") hotkeys - platform dispatch.
//!
//! The model lives in `azul_core::global_hotkey`, the process-wide registry
//! and the fire mailbox in `azul_layout::managers::global_hotkey`; this
//! module is the OS plumbing and the run-loop hook.
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
//! iOS, Android and the web have no global hotkeys: nothing is installed and
//! registration answers `Unsupported`. A headless run installs the
//! simulated backend, which grabs nothing and fires through
//! `azul_layout::managers::global_hotkey::simulate` (and the `AZ_E2E`
//! `global_hotkey` op).
//!
//! # Delivery
//!
//! The backends only park ids. Each run loop calls one of the
//! `pump_into_*` functions per iteration, which runs the fired hotkeys'
//! callbacks against the app's FIRST window through
//! `PlatformWindow::invoke_menu_callback` - the exact route a tray menu
//! click takes, and for the same reason: a `CallbackInfo` needs a window,
//! and a hotkey belongs to the app, not to one of them.

use azul_core::events::ProcessEventResult;
use azul_layout::managers::global_hotkey::{self as registry, GlobalHotkeyBackend};

use crate::desktop::shell2::common::event::{MenuInvocation, PlatformWindow};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

/// The OS backend for this platform and session, or `None` where there is
/// none (iOS, Android).
///
/// On Linux the SESSION decides, not azul's own window backend: under
/// Wayland an X grab only sees keys while an XWayland window has the focus,
/// which is no global hotkey at all - so a Wayland session always goes
/// through the portal, even when azul draws its windows through XWayland.
#[must_use]
pub fn platform_backend() -> Option<GlobalHotkeyBackend> {
    #[cfg(target_os = "macos")]
    {
        return Some(macos::backend());
    }
    #[cfg(target_os = "windows")]
    {
        return Some(windows::backend());
    }
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty());
        return Some(if wayland {
            portal::backend()
        } else {
            x11::backend()
        });
    }
    #[allow(unreachable_code)]
    None
}

/// Install this platform's backend unless one is installed already. Called
/// from `App::create`. Side-effect free: the OS resources (the Carbon
/// handler, the message-only window, the X connection, the portal listener)
/// are created at the first registration.
pub fn install_platform_backend() {
    if let Some(backend) = platform_backend() {
        let _ = registry::install_backend_if_none(backend);
    }
}

/// Swap in the headless simulation. Called when a run turns out headless;
/// registrations made before `run()` move onto it (and their OS grabs are
/// released).
pub fn install_simulated_backend() {
    let refused = registry::install_backend(registry::simulated_backend());
    debug_assert!(refused.is_empty(), "the simulated backend refuses nothing");
    crate::plog_debug!(
        "[global-hotkey] headless run: simulated backend installed, nothing is grabbed at the OS"
    );
}

/// Must the run loop wake up by itself right now? True while a hotkey is
/// registered on a backend whose presses arrive on something the loop does
/// not wait on - only the headless simulation now, whose presses come from a
/// test thread that has no handle to the headless loop's condvar. The X11
/// and Wayland loops wait on their backends instead (`loop_waker`).
#[must_use]
pub fn needs_loop_polling() -> bool {
    registry::needs_loop_polling()
}

/// The descriptor that announces a press, for the Linux loops' poll set:
/// the X11 grab connection once a registration opened it. The portal needs
/// none (its listener raises `loop_waker::wake`); Carbon and Win32 wake their
/// loops through the OS.
#[cfg(target_os = "linux")]
#[must_use]
pub fn loop_wait_fd() -> Option<i32> {
    x11::connection_fd()
}

/// See the Linux [`loop_wait_fd`]: no other platform's backend needs one.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub fn loop_wait_fd() -> Option<i32> {
    None
}

/// Has the grab connection already read presses that its fd therefore no
/// longer announces? `loop_waker::must_not_park` asks before a Linux loop
/// parks.
#[cfg(target_os = "linux")]
#[must_use]
pub fn has_buffered_input() -> bool {
    x11::has_queued_events()
}

/// See the Linux [`has_buffered_input`]: Carbon and Win32 buffer nothing
/// the OS does not announce itself.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub fn has_buffered_input() -> bool {
    false
}

/// The capability probe's answer, for [`crate::desktop::extra::capability`].
#[must_use]
pub fn probe() -> registry::GlobalHotkeyProbe {
    install_platform_backend();
    registry::probe()
}

/// Run the backend's per-iteration work, then every hotkey fired since the
/// last call, against `window`. Returns the strongest result the callbacks
/// asked for; `DoNothing` when nothing fired.
pub fn deliver_fired<W: PlatformWindow>(window: &mut W) -> ProcessEventResult {
    registry::poll_backend();
    let fired = registry::take_fired();
    let mut result = ProcessEventResult::DoNothing;
    for fire in fired {
        crate::plog_debug!(
            "[global-hotkey] {} fired (id {})",
            fire.hotkey.to_display_string().as_str(),
            fire.id.id
        );
        result = result.max(window.invoke_menu_callback(
            fire.callback,
            MenuInvocation::Native {
                site: "global_hotkey",
            },
        ));
    }
    result
}

/// Poll the backend and report whether anything waits to be delivered - the
/// cheap check a loop makes before it looks up a window.
fn poll_and_check() -> bool {
    registry::poll_backend();
    registry::has_pending_fires()
}

/// macOS: deliver against the first window. Called from both macOS run
/// loops (`NSApplication::run`'s drain timer and the manual loop), next to
/// the tray pump.
#[cfg(target_os = "macos")]
pub(crate) fn pump_into_first_macos_window() {
    if !poll_and_check() {
        return;
    }
    let window_ptrs = crate::desktop::shell2::macos::registry::get_all_window_ptrs();
    let Some(&wptr) = window_ptrs.first() else {
        crate::plog_debug!("[global-hotkey] a hotkey fired but no window exists to run it");
        return;
    };
    let window = unsafe { &mut *wptr };
    if !matches!(deliver_fired(window), ProcessEventResult::DoNothing) {
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
    if !poll_and_check() {
        return;
    }
    let Some(wptr) = registry::get_all_window_handles()
        .first()
        .and_then(|hwnd| registry::get_window(*hwnd))
    else {
        return;
    };
    let window = unsafe { &mut *wptr };
    let _ = deliver_fired(window);
}

/// X11 / Wayland: poll the grab connection, then deliver against the first
/// window.
#[cfg(az_x11)]
pub(crate) fn pump_into_first_linux_window() {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};
    if !poll_and_check() {
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
            if !matches!(deliver_fired(w), ProcessEventResult::DoNothing) {
                w.request_redraw();
            }
        }
        #[cfg(target_os = "linux")]
        LinuxWindow::Wayland(w) => {
            if !matches!(deliver_fired(w), ProcessEventResult::DoNothing) {
                w.request_redraw();
            }
        }
    }
}
