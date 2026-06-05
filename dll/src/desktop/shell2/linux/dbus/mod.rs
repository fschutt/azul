//! DBus dynamic loading module
//!
//! Provides runtime loading of libdbus-1.so for GNOME menu integration
//! without requiring compile-time linking.

pub mod dlopen;

pub use dlopen::{
    DBusConnection, DBusError, DBusLib, DBusMessage, DBusMessageIter, DBusObjectPathVTable,
    DBUS_BUS_SESSION, DBUS_HANDLER_RESULT_HANDLED, DBUS_HANDLER_RESULT_NEED_MEMORY,
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED, DBUS_NAME_FLAG_DO_NOT_QUEUE, DBUS_TYPE_ARRAY,
    DBUS_TYPE_STRING, DBUS_TYPE_UINT32, DBUS_TYPE_VARIANT,
};

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
