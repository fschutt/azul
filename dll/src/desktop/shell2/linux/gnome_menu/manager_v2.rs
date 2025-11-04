//! GNOME Menu Manager V2 - Uses dlopen DBus implementation
//!
//! This is the new implementation that loads DBus dynamically and uses
//! low-level protocol handlers from protocol_impl.rs

use std::{
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
};

use super::{
    debug_log, register_actions_interface, register_menus_interface, DbusAction, DbusMenuGroup,
    GnomeMenuError,
};
use crate::desktop::shell2::linux::dbus::DBusLib;

/// GNOME menu manager V2 with dlopen DBus
///
/// This version loads libdbus-1.so at runtime and uses the low-level
/// protocol implementation for full cross-compilation support.
pub struct GnomeMenuManagerV2 {
    app_name: String,
    bus_name: String,
    object_path: String,

    // Shared DBus library instance (can be shared across windows)
    dbus_lib: Rc<DBusLib>,

    // Raw DBus connection (NOT wrapped in Arc to keep it simple)
    connection: *mut crate::desktop::shell2::linux::dbus::DBusConnection,

    // Menu and action state (shared with protocol handlers)
    menu_groups: Arc<Mutex<HashMap<u32, DbusMenuGroup>>>,
    actions: Arc<Mutex<HashMap<String, DbusAction>>>,
}

impl GnomeMenuManagerV2 {
    /// Create a new GNOME menu manager with dlopen DBus
    ///
    /// # Arguments
    ///
    /// * `app_name` - Application name (will be sanitized for DBus)
    /// * `dbus_lib` - Shared DBus library instance (load once, share across windows)
    ///
    /// # Returns
    ///
    /// Returns `None` if DBus connection fails or GNOME menus not available.
    pub fn new(app_name: &str, dbus_lib: Rc<DBusLib>) -> Result<Self, GnomeMenuError> {
        debug_log(&format!(
            "Creating GNOME menu manager V2 for app: {}",
            app_name
        ));

        // Sanitize app name for DBus
        let sanitized_name = app_name
            .replace('.', "_")
            .replace(' ', "_")
            .replace('-', "_");

        let bus_name = format!("org.gtk.{}", sanitized_name);
        let object_path = format!("/org/gtk/{}", sanitized_name.replace('_', "/"));

        debug_log(&format!("Bus name: {}", bus_name));
        debug_log(&format!("Object path: {}", object_path));

        // Initialize error structure
        let mut error = crate::desktop::shell2::linux::dbus::DBusError {
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
        let connection = unsafe {
            (dbus_lib.dbus_bus_get)(
                crate::desktop::shell2::linux::dbus::DBUS_BUS_SESSION,
                &mut error,
            )
        };

        if connection.is_null() {
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
        let bus_name_cstr = std::ffi::CString::new(bus_name.clone())
            .map_err(|e| GnomeMenuError::ServiceRegistrationFailed(e.to_string()))?;

        let result = unsafe {
            (dbus_lib.dbus_bus_request_name)(
                connection,
                bus_name_cstr.as_ptr(),
                crate::desktop::shell2::linux::dbus::DBUS_NAME_FLAG_DO_NOT_QUEUE,
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
                (dbus_lib.dbus_connection_unref)(connection);
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

        // Create shared state for protocol handlers
        let menu_groups = Arc::new(Mutex::new(HashMap::new()));
        let actions = Arc::new(Mutex::new(HashMap::new()));

        // Register org.gtk.Menus interface
        let menubar_path = format!("{}/menus/MenuBar", object_path);
        register_menus_interface(&dbus_lib, connection, &menubar_path, menu_groups.clone())?;

        // Register org.gtk.Actions interface
        register_actions_interface(&dbus_lib, connection, &object_path, actions.clone())?;

        debug_log("All DBus interfaces registered successfully");

        Ok(Self {
            app_name: app_name.to_string(),
            bus_name,
            object_path,
            dbus_lib,
            connection,
            menu_groups,
            actions,
        })
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

    /// Update menu structure
    ///
    /// Stores the menu groups for GNOME Shell to query via Start() method.
    pub fn update_menu(&self, groups: Vec<DbusMenuGroup>) -> Result<(), GnomeMenuError> {
        let mut menu_groups = self.menu_groups.lock().unwrap();

        menu_groups.clear();
        for group in groups {
            debug_log(&format!(
                "Updating menu group {} with {} items",
                group.group_id,
                group.items.len()
            ));
            menu_groups.insert(group.group_id, group);
        }

        debug_log("Menu structure updated");
        Ok(())
    }

    /// Register actions
    ///
    /// Stores actions for GNOME Shell to invoke via Activate() method.
    pub fn register_actions(&self, actions: Vec<DbusAction>) -> Result<(), GnomeMenuError> {
        let mut action_map = self.actions.lock().unwrap();

        action_map.clear();
        for action in actions {
            debug_log(&format!(
                "Registering action: {} (enabled: {})",
                action.name, action.enabled
            ));
            action_map.insert(action.name.clone(), action);
        }

        debug_log(&format!("Registered {} actions", action_map.len()));
        Ok(())
    }

    /// Set X11 window properties to advertise DBus menu services
    ///
    /// This tells GNOME Shell where to find our menu via DBus.
    pub fn set_window_properties(
        &self,
        window_id: u64,
        display: *mut std::ffi::c_void,
    ) -> Result<(), GnomeMenuError> {
        debug_log("Setting X11 window properties for GNOME menu");

        super::X11Properties::set_properties(
            window_id,
            display,
            &self.app_name,
            &self.bus_name,
            &self.object_path,
        )?;

        debug_log("X11 window properties set successfully");
        Ok(())
    }

    /// Set Wayland window properties (best-effort)
    ///
    /// Unlike X11, Wayland doesn't have window properties. GNOME Shell on Wayland
    /// uses the app_id from the xdg_toplevel protocol to match windows to DBus services.
    /// This function exists for API consistency but doesn't set any properties.
    pub fn set_window_properties_wayland(
        &self,
        _surface_id: u32,
        app_id: &Option<String>,
    ) -> Result<(), GnomeMenuError> {
        debug_log("Wayland menu integration (using app_id matching)");

        if let Some(id) = app_id {
            debug_log(&format!(
                "App ID: {}, DBus name: {}, path: {}",
                id, self.bus_name, self.object_path
            ));
        } else {
            debug_log(&format!(
                "No app_id set. GNOME Shell may not find menus. DBus: {}",
                self.bus_name
            ));
        }

        // Note: GNOME Shell on Wayland automatically discovers DBus menu services
        // by matching the app_id from xdg_toplevel with the DBus bus name.
        // No explicit property setting required.
        Ok(())
    }

    /// Process pending DBus messages
    ///
    /// Should be called regularly (e.g., in event loop) to handle incoming
    /// method calls from GNOME Shell.
    pub fn process_messages(&self) {
        unsafe {
            // Non-blocking message processing
            (self.dbus_lib.dbus_connection_read_write_dispatch)(self.connection, 0);
        }
    }
}

impl Drop for GnomeMenuManagerV2 {
    fn drop(&mut self) {
        debug_log(&format!(
            "Cleaning up GNOME menu manager V2 for: {}",
            self.app_name
        ));

        if !self.connection.is_null() {
            unsafe {
                (self.dbus_lib.dbus_connection_flush)(self.connection);
                (self.dbus_lib.dbus_connection_unref)(self.connection);
            }
        }
    }
}

// Safety: DBus connections are thread-safe according to libdbus docs
// (as long as we don't share the connection pointer directly)
unsafe impl Send for GnomeMenuManagerV2 {}
