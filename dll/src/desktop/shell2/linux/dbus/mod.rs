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
