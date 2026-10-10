//! DBus dynamic loading module
//!
//! Provides runtime loading of libdbus-1.so for GNOME menu integration
//! without requiring compile-time linking.

pub mod dlopen;

pub use dlopen::{
    DBusConnection, DBusError, DBusLib, DBusMessage, DBusMessageIter, DBusObjectPathVTable,
    DBusPendingCall, DBUS_MESSAGE_TYPE_ERROR, DBUS_TIMEOUT_USE_DEFAULT,
    DBUS_BUS_SESSION, DBUS_DISPATCH_COMPLETE, DBUS_DISPATCH_DATA_REMAINS,
    DBUS_DISPATCH_NEED_MEMORY, DBUS_HANDLER_RESULT_HANDLED, DBUS_HANDLER_RESULT_NEED_MEMORY,
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED, DBUS_NAME_FLAG_DO_NOT_QUEUE, DBUS_TYPE_ARRAY,
    DBUS_TYPE_BOOLEAN, DBUS_TYPE_BYTE, DBUS_TYPE_DICT_ENTRY, DBUS_TYPE_INT32,
    DBUS_TYPE_OBJECT_PATH, DBUS_TYPE_STRING, DBUS_TYPE_STRUCT, DBUS_TYPE_UINT32, DBUS_TYPE_VARIANT,
};

/// How many messages one [`drain_connection`] dispatches before it yields to
/// the rest of the loop iteration. A panel that floods the connection must
/// not starve the windows; what is left is announced through the loop waker,
/// so the next iteration continues without parking.
pub const MAX_DISPATCH_PER_DRAIN: usize = 256;

/// Read what the socket holds, write what is queued, and dispatch every
/// message that parsed - without blocking.
///
/// `dbus_connection_read_write_dispatch` is the wrong tool for a loop that
/// parks in `poll(2)`: per call it either dispatches ONE queued message or
/// reads, never both. A message it read therefore sat in libdbus's incoming
/// queue until the next wake-up, and no poll on the socket can announce it
/// (the bytes are no longer in the socket). This reads once, then dispatches
/// until the queue reports [`DBUS_DISPATCH_COMPLETE`].
///
/// Returns `false` when the connection is closed.
///
/// # Safety
/// `conn` must be a live connection obtained from `lib`, used on the thread
/// that runs the event loop (the thread every caller in this crate uses).
pub unsafe fn drain_connection(lib: &DBusLib, conn: *mut DBusConnection) -> bool {
    if conn.is_null() {
        return false;
    }
    // Non-blocking (timeout 0): reads at most what the socket holds right
    // now and flushes the outgoing queue as far as the socket takes it.
    let connected = unsafe { (lib.dbus_connection_read_write)(conn, 0) } != 0;
    for _ in 0..MAX_DISPATCH_PER_DRAIN {
        if unsafe { (lib.dbus_connection_dispatch)(conn) } != DBUS_DISPATCH_DATA_REMAINS {
            return connected;
        }
    }
    // Budget spent with messages left: come back without parking.
    crate::desktop::loop_waker::wake();
    connected
}

/// Are messages parsed and waiting in `conn`'s incoming queue? They arrived
/// during a blocking call (`send_with_reply_and_block` queues whatever else
/// comes in while it waits for its reply), so the socket no longer announces
/// them and the loop must not park on top of them.
///
/// # Safety
/// As [`drain_connection`].
pub unsafe fn has_undispatched_messages(lib: &DBusLib, conn: *mut DBusConnection) -> bool {
    !conn.is_null()
        && unsafe { (lib.dbus_connection_get_dispatch_status)(conn) } == DBUS_DISPATCH_DATA_REMAINS
}

/// The connection's socket, for a poll set. `None` before authentication has
/// finished or on a non-socket transport.
///
/// # Safety
/// As [`drain_connection`].
pub unsafe fn connection_fd(lib: &DBusLib, conn: *mut DBusConnection) -> Option<i32> {
    if conn.is_null() {
        return None;
    }
    let mut fd: core::ffi::c_int = -1;
    let ok = unsafe { (lib.dbus_connection_get_unix_fd)(conn, &mut fd) } != 0;
    (ok && fd >= 0).then_some(fd)
}

/// Returns `true` if a global-menu registrar — `com.canonical.AppMenu.Registrar`
/// (KDE Global Menu applet, Unity, appmenu-gtk-module) — currently owns a name on
/// the DBus session bus, i.e. the desktop will render an *exported* application
/// menu bar.
///
/// Decides menu-bar strategy on Linux/X11: registrar present → export the menu
/// natively over DBus; registrar absent → inject a software menu bar into the
/// window (the common case — XFCE, bare WMs, and default KDE/GNOME, which keep the
/// menu in-window). Returns `false` if libdbus is unavailable or the session bus
/// can't be reached (→ inject), so it can never wrongly suppress the menu.
pub fn native_global_menu_available() -> bool {
    use core::ffi::c_char;

    let lib = match dlopen::DBusLib::new() {
        Ok(l) => l,
        Err(_) => return false,
    };
    unsafe {
        let mut err: DBusError = core::mem::zeroed();
        (lib.dbus_error_init)(&mut err);
        let conn = (lib.dbus_bus_get)(DBUS_BUS_SESSION, &mut err);
        if conn.is_null() {
            (lib.dbus_error_free)(&mut err);
            return false;
        }
        let name = b"com.canonical.AppMenu.Registrar\0";
        let has = (lib.dbus_bus_name_has_owner)(conn, name.as_ptr() as *const c_char, &mut err);
        // `dbus_bus_get` hands back a shared, ref-counted connection — balance our ref.
        (lib.dbus_connection_unref)(conn);
        let errored = (lib.dbus_error_is_set)(&err) != 0;
        (lib.dbus_error_free)(&mut err);
        !errored && has != 0
    }
}
