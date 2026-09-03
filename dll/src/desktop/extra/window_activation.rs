//! Bring a window to the front, per platform (9h-i-a-ii).
//!
//! The request comes from off the event loop - a desktop's media widget
//! clicking MPRIS `Raise` - and is parked in
//! `azul_layout::managers::window_activation` until the owning window's pass
//! drains it. This is what happens then.
//!
//! # Every platform disagrees about whether an app may do this at all
//!
//! Focus stealing is a real problem, and each platform has taken a different
//! position on it. That is why this is per-backend rather than one call:
//!
//! - **macOS** allows it outright (`activateIgnoringOtherApps:`).
//! - **Windows** allows it only for the app that already owns the foreground,
//!   and silently does nothing otherwise - so the documented fallback is to
//!   flash the taskbar button instead of pretending it worked.
//! - **X11** has no permission model at all: `_NET_ACTIVE_WINDOW` asks the
//!   window manager, and every mainstream WM honours it.
//! - **Wayland** refuses BY DESIGN, and cannot be worked around - see below.

/// Raise the window this handle names.
///
/// Returns `false` when the platform declined or has no way to do it, which
/// the caller logs rather than retries: a refusal is a policy answer, not a
/// transient failure.
pub fn raise_window(handle: azul_core::window::RawWindowHandle) -> bool {
    use azul_core::window::RawWindowHandle;

    match handle {
        #[cfg(target_os = "macos")]
        RawWindowHandle::MacOS(h) => raise_macos(h.ns_window),
        #[cfg(target_os = "windows")]
        RawWindowHandle::Windows(h) => raise_windows(h.hwnd),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(h) => raise_x11(h.display, h.window),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Wayland(_) => {
            // NOT A GAP. `xdg_activation_v1` needs a token minted from a
            // recent INPUT SERIAL, and a request arriving on a D-Bus thread
            // has no serial - by construction, because the whole point of the
            // protocol is that only an app the user just interacted with may
            // take focus. A compositor that let this through would be broken.
            // The honest answer is "no", and an app that wants attention on
            // Wayland asks for it through the compositor's own affordances
            // (an urgency hint on the taskbar entry), which is a different
            // feature.
            static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
            if WARNED.set(()).is_ok() {
                crate::plog_info!(
                    "[window] raise ignored on Wayland: xdg_activation needs an input \\
                     serial, which a request from another process cannot have"
                );
            }
            false
        }
        _ => false,
    }
}

/// `[NSApp activateIgnoringOtherApps:YES]` then `[window makeKeyAndOrderFront:]`.
///
/// BOTH are needed and neither is enough: activating brings the APPLICATION
/// forward but leaves whichever window was key still key, and ordering a window
/// front inside a background app puts it above that app's own windows only.
#[cfg(target_os = "macos")]
fn raise_macos(ns_window: *mut core::ffi::c_void) -> bool {
    if ns_window.is_null() {
        return false;
    }
    unsafe {
        use objc2::{msg_send, runtime::AnyObject};

        let Some(app_cls) = std::ffi::CString::new("NSApplication")
            .ok()
            .as_deref()
            .and_then(objc2::runtime::AnyClass::get)
        else {
            return false;
        };
        let app: *mut AnyObject = msg_send![app_cls, sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
        }
        let window = ns_window.cast::<AnyObject>();
        let _: () = msg_send![window, makeKeyAndOrderFront: core::ptr::null::<AnyObject>()];
        true
    }
}

/// `SetForegroundWindow`, after un-minimising.
///
/// A MINIMISED window cannot be foreground, so `SetForegroundWindow` on one
/// succeeds and leaves it in the taskbar - which looks exactly like the raise
/// being ignored. `ShowWindow(SW_RESTORE)` first is what makes it visible.
///
/// Windows REFUSES the call outright unless this process already owns the
/// foreground (or the user just interacted with it), and it reports that by
/// returning false rather than by an error. Passing that back is what lets the
/// caller say so instead of claiming success.
#[cfg(target_os = "windows")]
fn raise_windows(hwnd: *mut core::ffi::c_void) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE},
    };

    if hwnd.is_null() {
        return false;
    }
    let hwnd = HWND(hwnd);
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// `_NET_ACTIVE_WINDOW`, the EWMH way to ask the window manager.
///
/// A CLIENT MESSAGE TO THE ROOT WINDOW, not `XRaiseWindow`: raising restacks
/// the window without giving it focus, so it comes to the front and then
/// ignores the keyboard. Every mainstream WM implements this message, and one
/// that does not simply drops it.
///
/// `data[0] = 2` is the source indication for "a pager or another application
/// asked", which is exactly what this is - claiming `1` ("the application
/// itself, from user action") would be a lie some window managers check.
#[cfg(target_os = "linux")]
fn raise_x11(display: *mut core::ffi::c_void, window: u64) -> bool {
    use crate::desktop::shell2::linux::x11::{
        defines::{
            ClientMessage, SubstructureNotifyMask, SubstructureRedirectMask, XClientMessageData,
            XClientMessageEvent, XEvent,
        },
        dlopen::Xlib,
    };

    if display.is_null() || window == 0 {
        return false;
    }
    let Ok(xlib) = Xlib::new() else {
        return false;
    };
    unsafe {
        let display = display.cast();
        let atom = (xlib.XInternAtom)(
            display,
            b"_NET_ACTIVE_WINDOW\0".as_ptr().cast(),
            // `only_if_exists = 0`: intern it either way, so a bare WM that has
            // never used the atom still receives a well-formed message.
            0,
        );
        if atom == 0 {
            return false;
        }
        let screen = (xlib.XDefaultScreen)(display);
        let root = (xlib.XRootWindow)(display, screen);

        let mut data = XClientMessageData { l: [0; 5] };
        // `data[0]` is the SOURCE INDICATION. `2` is "a pager or another
        // application asked", which is exactly what this is; claiming `1`
        // ("the application itself, from a user action") would be a lie, and
        // some window managers treat the two differently on purpose.
        data.l[0] = 2;
        let mut event = XEvent {
            client_message: XClientMessageEvent {
                type_: ClientMessage,
                serial: 0,
                send_event: 1,
                display,
                window,
                message_type: atom,
                // 32-BIT ITEMS. The EWMH message is five longs; declaring 8 or
                // 16 makes the WM read the payload as bytes and ignore it.
                format: 32,
                data,
            },
        };
        // The mask the WM listens on for root-window messages. A message sent
        // with any other mask is delivered nowhere and looks exactly like the
        // WM ignoring the request.
        (xlib.XSendEvent)(
            display,
            root,
            0,
            SubstructureNotifyMask | SubstructureRedirectMask,
            &raw mut event,
        );
        (xlib.XFlush)(display);
    }
    true
}
