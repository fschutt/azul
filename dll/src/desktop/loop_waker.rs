//! The run loops' wake-up line for the APP-LEVEL event sources: the system
//! tray, native notifications and global hotkeys.
//!
//! None of the three belongs to a window, and none reports through a
//! window's own connection:
//!
//! | source | Linux | macOS | Windows |
//! |---|---|---|---|
//! | tray | SNI + dbusmenu calls on the session bus | `NSStatusItem` actions inside `sendEvent:` | (no backend yet) |
//! | notifications | `ActionInvoked` / `NotificationClosed` on the session bus | the UN delegate, on UN's own queue | `NIN_BALLOON*` to a hidden window |
//! | hotkeys | X11: `KeyPress` on a second display connection; Wayland: the portal's `Activated` on a listener thread | Carbon handler inside `sendEvent:` | `WM_HOTKEY` to a message-only window |
//!
//! The X11 and Wayland loops park in `poll(2)`, and none of those Linux
//! channels was in the poll set, so the loops used to cap their park at
//! 100 ms whenever a tray, an outstanding notification or a hotkey existed -
//! ten wake-ups a second for the life of a tray icon, and up to 100 ms of
//! latency on every click. This module replaces that with:
//!
//! * [`wait_fds`]: the descriptors that announce the sources - the shared D-Bus session connection
//!   ([`watch_dbus_connection`]), the hotkey grab connection, and one wake descriptor;
//! * [`wake`]: callable from ANY thread (the portal listener, a D-Bus burst that outgrew one
//!   iteration), it raises the wake descriptor on Linux and posts an app-defined `NSEvent` on
//!   macOS;
//! * [`must_not_park`]: work no descriptor can announce any more - messages libdbus already parsed
//!   during a blocking call, X events Xlib already queued, a mailbox a callback filled on the loop
//!   thread itself;
//! * [`service_sources`]: the per-iteration half, run by the app-event collector
//!   (`desktop::app_events`) before it takes the mailboxes: it acknowledges the wake, drains
//!   D-Bus completely and reads the hotkey connection.
//!
//! On macOS and Windows every source already wakes the loop by itself (an
//! NSEvent, a window message), so [`wake`] only adds the NSEvent post that
//! the notification delegate used to make privately; the loops there have to
//! DELIVER before they park, which is the collector's job.

use core::sync::atomic::{AtomicBool, Ordering};

/// Set by [`wake`], cleared by [`service_sources`]. The flag, not the
/// descriptor, is the source of truth: a descriptor can be drained by one
/// `poll(2)` round while the work it announced is taken by the next.
static PENDING: AtomicBool = AtomicBool::new(false);

/// Ask the event-loop thread to come around once more, from any thread.
///
/// Call it AFTER the work is in its mailbox (a fire parked, an event queued):
/// the loop takes the mailboxes after it saw the wake, never before.
pub fn wake() {
    PENDING.store(true, Ordering::Release);
    #[cfg(az_x11)]
    {
        let fd = wake_fd();
        if fd >= 0 {
            crate::desktop::shell2::linux::timer::signal_wake_fd(fd);
        }
    }
    #[cfg(target_os = "macos")]
    macos::post_wake_event();
}

/// Has a [`wake`] arrived that no iteration has served yet?
#[must_use]
pub fn is_pending() -> bool {
    PENDING.load(Ordering::Acquire)
}

/// Clear the flag and report whether it was set. The collector calls this
/// through [`service_sources`]; exposed for the tests.
#[must_use]
pub fn take_pending() -> bool {
    PENDING.swap(false, Ordering::AcqRel)
}

/// Per-iteration work on the event-loop thread, before the mailboxes are
/// taken: acknowledge the wake, dispatch everything the D-Bus connection
/// holds (tray property reads and clicks, notification signals, the GNOME
/// menu's calls - they all share the one session connection), and read the
/// hotkey grab connection.
pub fn service_sources() {
    #[cfg(az_x11)]
    {
        let fd = wake_fd();
        if fd >= 0 {
            crate::desktop::shell2::linux::timer::drain_fd(fd);
        }
    }
    let _ = take_pending();
    #[cfg(az_x11)]
    dbus_watch::drain_all();
    azul_layout::managers::global_hotkey::poll_backend();
}

/// Is work owed that no descriptor in [`wait_fds`] will announce? The Linux
/// loops return from their wait instead of parking when it is.
///
/// * a [`wake`] nobody has served yet;
/// * D-Bus messages libdbus parsed during a blocking call (`send_with_reply_and_block` queues
///   everything else that arrives while it waits), which have left the socket;
/// * X events the hotkey connection's Xlib already read into its queue;
/// * a mailbox filled on the loop thread itself - a callback posted a notification, a Carbon /
///   D-Bus handler queued a click - after this iteration's collector ran.
#[must_use]
pub fn must_not_park() -> bool {
    if is_pending() {
        return true;
    }
    #[cfg(az_x11)]
    if dbus_watch::any_undispatched() {
        return true;
    }
    if crate::desktop::global_hotkey::has_buffered_input() {
        return true;
    }
    azul_layout::managers::notification::has_queued_requests()
        || azul_layout::managers::notification::has_queued_events()
        || azul_layout::managers::global_hotkey::has_pending_fires()
}

/// The descriptors the Linux loops add to their `poll(2)` set: the wake
/// descriptor, the watched D-Bus connection(s) and the hotkey grab
/// connection. Readability is all they mean - the loop does not read them;
/// the collector's [`service_sources`] does, at the top of the next
/// iteration.
#[cfg(az_x11)]
#[must_use]
pub fn wait_fds() -> Vec<i32> {
    let mut fds: Vec<i32> = Vec::with_capacity(3);
    let fd = wake_fd();
    if fd >= 0 {
        fds.push(fd);
    }
    for fd in dbus_watch::fds() {
        if !fds.contains(&fd) {
            fds.push(fd);
        }
    }
    if let Some(fd) = crate::desktop::global_hotkey::loop_wait_fd() {
        if !fds.contains(&fd) {
            fds.push(fd);
        }
    }
    fds
}

/// Put `conn` - the shared session connection a tray, the notification
/// backend or the GNOME menu exporter just opened - into the loops' wait set.
/// Idempotent: every caller gets the same connection from `dbus_bus_get`.
///
/// Takes a reference of its own, held for the process: the GNOME menu
/// exporter drops its reference with its window, and the loop must not be
/// left polling a finalized connection.
#[cfg(az_x11)]
pub fn watch_dbus_connection(
    lib: &std::sync::Arc<crate::desktop::shell2::linux::dbus::DBusLib>,
    conn: *mut crate::desktop::shell2::linux::dbus::DBusConnection,
) {
    dbus_watch::watch(lib, conn);
}

/// The wake descriptor: an eventfd on Linux, a kqueue `EVFILT_USER` for the
/// X11 backend on a macOS host - `linux::timer`'s wake descriptors, the ones
/// the theme watcher and the WebRender frame-ready wake use. Created on first
/// use; `-1` if it could not be.
#[cfg(az_x11)]
fn wake_fd() -> i32 {
    static WAKE_FD: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *WAKE_FD.get_or_init(crate::desktop::shell2::linux::timer::new_wake_fd)
}

/// The watched D-Bus connections. In practice exactly one: the process's
/// shared session connection.
#[cfg(az_x11)]
mod dbus_watch {
    use std::sync::{Arc, Mutex, PoisonError};

    use crate::desktop::shell2::linux::dbus::{self, DBusConnection, DBusLib};

    struct Watched {
        lib: Arc<DBusLib>,
        /// A `*mut DBusConnection` we hold a reference on, as `usize` so the
        /// static is `Send`. Only the event-loop thread uses it.
        conn: usize,
    }

    static WATCHED: Mutex<Vec<Watched>> = Mutex::new(Vec::new());

    fn with<R>(f: impl FnOnce(&mut Vec<Watched>) -> R) -> R {
        let mut guard = WATCHED.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut guard)
    }

    pub(super) fn watch(lib: &Arc<DBusLib>, conn: *mut DBusConnection) {
        if conn.is_null() {
            return;
        }
        with(|watched| {
            if watched.iter().any(|w| w.conn == conn as usize) {
                return;
            }
            // Our own reference, never released: see `watch_dbus_connection`.
            let held = unsafe { (lib.dbus_connection_ref)(conn) };
            if held.is_null() {
                return;
            }
            watched.push(Watched {
                lib: lib.clone(),
                conn: held as usize,
            });
        });
    }

    pub(super) fn fds() -> Vec<i32> {
        with(|watched| {
            watched
                .iter()
                .filter_map(|w| unsafe { dbus::connection_fd(&w.lib, w.conn as *mut DBusConnection) })
                .collect()
        })
    }

    /// Drain every watched connection. A connection that reports itself
    /// closed is dropped from the set: its socket would report POLLHUP to
    /// every `poll(2)` from now on, and the loop would spin on it.
    pub(super) fn drain_all() {
        // Handlers run inside the dispatch and may reach `watch` (a tray
        // created from a callback), so the lock is not held across it.
        let snapshot: Vec<(Arc<DBusLib>, usize)> =
            with(|watched| watched.iter().map(|w| (w.lib.clone(), w.conn)).collect());
        let mut closed: Vec<usize> = Vec::new();
        for (lib, conn) in snapshot {
            if !unsafe { dbus::drain_connection(&lib, conn as *mut DBusConnection) } {
                closed.push(conn);
            }
        }
        if !closed.is_empty() {
            crate::plog_warn!(
                "[loop-waker] the D-Bus session connection closed; the tray and notifications \
                 no longer hear from the desktop"
            );
            with(|watched| watched.retain(|w| !closed.contains(&w.conn)));
        }
    }

    pub(super) fn any_undispatched() -> bool {
        with(|watched| {
            watched.iter().any(|w| unsafe {
                dbus::has_undispatched_messages(&w.lib, w.conn as *mut DBusConnection)
            })
        })
    }
}

/// macOS: wake `runMode:beforeDate:` / `nextEventMatchingMask:` with an
/// app-defined `NSEvent`. `process_event` routes unknown event types to a
/// no-op arm, and under `NSApplication.run()` the event is discarded (the
/// 33 ms drain timer serves that mode), so posting is always safe.
#[cfg(target_os = "macos")]
mod macos {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSPoint;

    /// `NSEventTypeApplicationDefined`.
    const NS_EVENT_TYPE_APPLICATION_DEFINED: usize = 15;

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        /// The global `NSApp`: nil until `+[NSApplication sharedApplication]`
        /// has run on the main thread. Read (not `sharedApplication`) because
        /// a wake may come from any thread, and calling that off the main
        /// thread would CREATE the application object there when none exists
        /// yet.
        #[allow(non_upper_case_globals)]
        static NSApp: *mut AnyObject;
    }

    /// `postEvent:atStart:` may be sent from any thread.
    pub(super) fn post_wake_event() {
        let app: *mut AnyObject = unsafe { NSApp };
        if app.is_null() {
            return;
        }
        let Some(event_cls) = AnyClass::get(c"NSEvent") else {
            return;
        };
        let event: *mut AnyObject = unsafe {
            msg_send![
                event_cls,
                otherEventWithType: NS_EVENT_TYPE_APPLICATION_DEFINED,
                location: NSPoint::new(0.0, 0.0),
                modifierFlags: 0usize,
                timestamp: 0.0f64,
                windowNumber: 0isize,
                context: core::ptr::null_mut::<AnyObject>(),
                subtype: 0i16,
                data1: 0isize,
                data2: 0isize
            ]
        };
        if event.is_null() {
            return;
        }
        let _: () = unsafe { msg_send![app, postEvent: event, atStart: false] };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag is process-wide; the tests that touch it take turns.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
        SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn a_wake_from_another_thread_is_seen_once_by_the_loop() {
        let _serial = serial();
        let _ = take_pending();
        std::thread::spawn(wake).join().expect("the waking thread ran");
        assert!(is_pending(), "the wake is visible to the loop thread");
        assert!(must_not_park(), "a pending wake forbids parking");
        assert!(take_pending(), "the loop takes it");
        assert!(!take_pending(), "exactly once");
    }

    #[test]
    fn service_sources_acknowledges_the_wake() {
        let _serial = serial();
        wake();
        service_sources();
        assert!(!is_pending(), "one iteration served the wake");
    }

    /// The descriptor half, where it exists: a wake makes the wake
    /// descriptor readable for `poll(2)`, and serving it clears it again - a
    /// descriptor left readable would spin every idle loop.
    #[cfg(az_x11)]
    #[test]
    fn a_wake_makes_the_wake_descriptor_readable_until_served() {
        let _serial = serial();
        service_sources();
        let fd = wake_fd();
        assert!(fd >= 0, "the wake descriptor exists");
        assert!(wait_fds().contains(&fd), "it is in the loops' wait set");
        let readable = |fd: i32| {
            let mut p = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            unsafe { libc::poll(&mut p, 1, 0) > 0 && p.revents & libc::POLLIN != 0 }
        };
        assert!(!readable(fd), "an idle loop parks");
        std::thread::spawn(wake).join().expect("the waking thread ran");
        assert!(readable(fd), "a wake from another thread ends the park");
        service_sources();
        assert!(!readable(fd), "serving the wake re-arms the park");
    }
}
