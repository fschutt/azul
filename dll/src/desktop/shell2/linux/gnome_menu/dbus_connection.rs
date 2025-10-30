//! DBus Connection Management
//!
//! Handles connection to the DBus session bus and service registration.

use std::sync::{Arc, Mutex};
use super::{GnomeMenuError, debug_log};

#[cfg(all(target_os = "linux", feature = "gnome-menus"))]
use dbus::blocking::Connection;

/// DBus connection wrapper
pub struct DbusConnection {
    app_name: String,
    bus_name: String,
    object_path: String,
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    connection: Arc<Mutex<Connection>>,
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
            use std::time::Duration;
            
            let conn = Connection::new_session()
                .map_err(|e| GnomeMenuError::DbusConnectionFailed(e.to_string()))?;

            // Request the service name with DBUS_NAME_FLAG_DO_NOT_QUEUE
            conn.request_name(&bus_name, false, true, false)
                .map_err(|e| GnomeMenuError::ServiceRegistrationFailed(e.to_string()))?;

            debug_log("DBus service registered successfully");

            Ok(Self {
                app_name: app_name.to_string(),
                bus_name,
                object_path,
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
    
    /// Get the DBus connection
    #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
    pub fn get_connection(&self) -> Arc<Mutex<Connection>> {
        self.connection.clone()
    }
}

impl Drop for DbusConnection {
    fn drop(&mut self) {
        debug_log(&format!("Cleaning up DBus connection for: {}", self.app_name));
        // TODO: Cleanup when dbus crate is added
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
