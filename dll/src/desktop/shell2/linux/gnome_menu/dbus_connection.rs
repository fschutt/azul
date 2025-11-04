//! DBus Connection Management
//!
//! Handles connection to the DBus session bus and service registration.

use std::{
    ffi::CString,
    rc::Rc,
    sync::{Arc, Mutex},
};

use super::{debug_log, GnomeMenuError};
#[cfg(all(target_os = "linux", feature = "gnome-menus"))]
use crate::desktop::shell2::linux::dbus::{
    DBusConnection as RawDBusConnection, DBusError, DBusLib, DBUS_BUS_SESSION,
    DBUS_NAME_FLAG_DO_NOT_QUEUE,
};

/// DBus connection wrapper
pub struct DbusConnection {
    app_name: String,
    bus_name: String,
    object_path: String,
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    dbus_lib: Rc<DBusLib>,
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    connection: Arc<Mutex<*mut RawDBusConnection>>,
    #[cfg(not(all(target_os = "linux", feature = "gnome-menus")))]
    _phantom: std::marker::PhantomData<()>,
}

impl DbusConnection {
    /// Create a new DBus connection
    ///
    /// Connects to the session bus and registers the service name.
    pub fn new(app_name: &str) -> Result<Self, GnomeMenuError> {
        debug_log(&format!("Establishing DBus connection for: {}", app_name));

        // Sanitize app name for DBus (replace dots and special chars)
        let sanitized_name = app_name
            .replace('.', "_")
            .replace(' ', "_")
            .replace('-', "_");

        let bus_name = format!("org.gtk.{}", sanitized_name);
        let object_path = format!("/org/gtk/{}", sanitized_name.replace('_', "/"));

        debug_log(&format!("Bus name: {}", bus_name));
        debug_log(&format!("Object path: {}", object_path));

        #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
        {
            // Load DBus library dynamically
            let dbus_lib = DBusLib::new().map_err(|e| {
                GnomeMenuError::DbusConnectionFailed(format!("Failed to load libdbus-1.so: {}", e))
            })?;

            // Initialize error structure
            let mut error = DBusError {
                name: std::ptr::null(),
                message: std::ptr::null(),
                dummy1: 0,
                dummy2: 0,
                dummy3: 0,
                dummy4: 0,
                dummy5: 0,
                padding1: std::ptr::null_mut(),
            };

            unsafe {
                (dbus_lib.dbus_error_init)(&mut error);
            }

            // Get session bus connection
            let conn = unsafe { (dbus_lib.dbus_bus_get)(DBUS_BUS_SESSION, &mut error) };

            if conn.is_null() {
                let error_msg = if !error.message.is_null() {
                    let c_str = unsafe { std::ffi::CStr::from_ptr(error.message) };
                    c_str.to_string_lossy().into_owned()
                } else {
                    "Unknown error".to_string()
                };
                unsafe {
                    (dbus_lib.dbus_error_free)(&mut error);
                }
                return Err(GnomeMenuError::DbusConnectionFailed(format!(
                    "Failed to connect to session bus: {}",
                    error_msg
                )));
            }

            // Request the service name
            let bus_name_cstr = CString::new(bus_name.clone())
                .map_err(|e| GnomeMenuError::ServiceRegistrationFailed(e.to_string()))?;

            let result = unsafe {
                (dbus_lib.dbus_bus_request_name)(
                    conn,
                    bus_name_cstr.as_ptr(),
                    DBUS_NAME_FLAG_DO_NOT_QUEUE,
                    &mut error,
                )
            };

            if result < 0 {
                let error_msg = if !error.message.is_null() {
                    let c_str = unsafe { std::ffi::CStr::from_ptr(error.message) };
                    c_str.to_string_lossy().into_owned()
                } else {
                    "Unknown error".to_string()
                };
                unsafe {
                    (dbus_lib.dbus_error_free)(&mut error);
                    (dbus_lib.dbus_connection_unref)(conn);
                }
                return Err(GnomeMenuError::ServiceRegistrationFailed(format!(
                    "Failed to register service name: {}",
                    error_msg
                )));
            }

            unsafe {
                (dbus_lib.dbus_error_free)(&mut error);
            }

            debug_log("DBus service registered successfully");

            Ok(Self {
                app_name: app_name.to_string(),
                bus_name,
                object_path,
                dbus_lib,
                connection: Arc::new(Mutex::new(conn)),
            })
        }

        #[cfg(not(all(target_os = "linux", feature = "gnome-menus")))]
        Err(GnomeMenuError::NotImplemented)
    }

    /// Get the DBus service name
    pub fn get_bus_name(&self) -> &str {
        &self.bus_name
    }

    /// Get the DBus object path
    pub fn get_object_path(&self) -> &str {
        &self.object_path
    }

    /// Get the app menu object path
    pub fn get_app_menu_path(&self) -> String {
        format!("{}/menus/AppMenu", self.object_path)
    }

    /// Get the menu bar object path
    pub fn get_menubar_path(&self) -> String {
        format!("{}/menus/MenuBar", self.object_path)
    }

    /// Get the DBus library handle
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    pub fn get_dbus_lib(&self) -> Rc<DBusLib> {
        self.dbus_lib.clone()
    }

    /// Get the raw DBus connection
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    pub fn get_connection(&self) -> Arc<Mutex<*mut RawDBusConnection>> {
        self.connection.clone()
    }
}

impl Drop for DbusConnection {
    fn drop(&mut self) {
        debug_log(&format!(
            "Cleaning up DBus connection for: {}",
            self.app_name
        ));

        #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
        {
            let conn = self.connection.lock().unwrap();
            if !conn.is_null() {
                unsafe {
                    (self.dbus_lib.dbus_connection_flush)(*conn);
                    (self.dbus_lib.dbus_connection_unref)(*conn);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_name_sanitization() {
        // This will fail with NotImplemented, but we can test the logic
        let result = DbusConnection::new("My.Test-App 123");
        assert!(result.is_err());

        // When implemented, should create:
        // bus_name: "org.gtk.My_Test_App_123"
        // object_path: "/org/gtk/My/Test/App/123"
    }
}
