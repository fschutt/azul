//! org.gtk.Menus Protocol Implementation
//!
//! Implements the DBus interface for menu structure export.
//!
//! ## Interface Methods
//!
//! - `Start(subscriptions: au) → a(uuaa{sv})`
//!   - Subscribe to menu groups
//!   - Returns menu structure in DBus format
//!
//! - `End(subscriptions: au)`
//!   - Unsubscribe from menu groups
//!
//! ## Menu Format
//!
//! ```text
//! array of (group_id, menu_id, items)
//! items = array of dict {
//!     "label": variant<string>,
//!     "action": variant<string>,
//!     "target": variant<...>,
//!     "submenu": variant<(uint, uint)>,
//!     "section": variant<(uint, uint)>,
//! }
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::{debug_log, GnomeMenuError};

/// Represents a menu item in DBus format
#[derive(Debug, Clone)]
pub struct DbusMenuItem {
    pub label: String,
    pub action: Option<String>,
    pub target: Option<String>,
    pub submenu: Option<(u32, u32)>, // (group_id, menu_id)
    pub section: Option<(u32, u32)>, // For separators
    pub enabled: bool,
}

/// Represents a menu group (subscription group)
#[derive(Debug, Clone)]
pub struct DbusMenuGroup {
    pub group_id: u32,
    pub menu_id: u32,
    pub items: Vec<DbusMenuItem>,
}

/// org.gtk.Menus protocol handler
pub struct MenuProtocol {
    menu_groups: Arc<Mutex<HashMap<u32, DbusMenuGroup>>>,
}

impl MenuProtocol {
    /// Create a new menu protocol handler
    pub fn new() -> Self {
        debug_log("Initializing org.gtk.Menus protocol");

        Self {
            menu_groups: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Update menu structure
    ///
    /// Stores the menu groups for later retrieval by GNOME Shell.
    pub fn update_menu(&self, groups: Vec<DbusMenuGroup>) -> Result<(), GnomeMenuError> {
        let mut menu_groups = self.menu_groups.lock().unwrap();

        menu_groups.clear();
        for group in groups {
            debug_log(&format!(
                "Registering menu group {} with {} items",
                group.group_id,
                group.items.len()
            ));
            menu_groups.insert(group.group_id, group);
        }

        debug_log("Menu structure updated");
        Ok(())
    }

    /// Handle Start method call
    ///
    /// Called by GNOME Shell to subscribe to menu groups.
    /// Returns the menu structure for the requested groups.
    pub fn handle_start(
        &self,
        subscriptions: Vec<u32>,
    ) -> Result<Vec<DbusMenuGroup>, GnomeMenuError> {
        debug_log(&format!(
            "Start method called with subscriptions: {:?}",
            subscriptions
        ));

        let menu_groups = self.menu_groups.lock().unwrap();
        let mut result = Vec::new();

        for group_id in subscriptions {
            if let Some(group) = menu_groups.get(&group_id) {
                result.push(group.clone());
            } else {
                debug_log(&format!(
                    "Warning: Subscription for unknown group {}",
                    group_id
                ));
            }
        }

        Ok(result)
    }

    /// Handle End method call
    ///
    /// Called by GNOME Shell to unsubscribe from menu groups.
    pub fn handle_end(&self, subscriptions: Vec<u32>) -> Result<(), GnomeMenuError> {
        debug_log(&format!(
            "End method called with subscriptions: {:?}",
            subscriptions
        ));
        // In a simple implementation, we don't need to do anything here
        Ok(())
    }

    /// Register with DBus
    ///
    /// Sets up the DBus method handlers for org.gtk.Menus interface.
    pub fn register_with_dbus(
        &self,
        connection: &super::DbusConnection,
    ) -> Result<(), GnomeMenuError> {
        debug_log("Registering org.gtk.Menus interface with DBus");

        #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
        {
            use std::sync::Arc;

            use dbus::{
                arg::{Dict, RefArg, Variant},
                blocking::Connection,
                tree::{Factory, MethodErr},
            };

            let conn = connection.get_connection();
            let conn_lock = conn.lock().unwrap();

            let factory = Factory::new_fn::<()>();
            let menu_groups_data = self.menu_groups.clone();
            let menu_groups_data2 = self.menu_groups.clone();

            let interface = factory
                .interface("org.gtk.Menus", ())
                .add_m(
                    factory
                        .method("Start", (), move |m| {
                            let subscriptions: Vec<u32> =
                                m.msg.read1().map_err(|e| MethodErr::failed(&e))?;

                            debug_log(&format!("DBus Start() called with {:?}", subscriptions));

                            let groups = menu_groups_data.lock().unwrap();
                            let mut result = Vec::new();

                            for group_id in subscriptions {
                                if let Some(group) = groups.get(&group_id) {
                                    // Format: (group_id, menu_id, items_array)
                                    result.push((
                                        group.group_id,
                                        group.menu_id,
                                        Vec::<(String, Variant<Box<dyn RefArg>>)>::new(),
                                    ));
                                }
                            }

                            Ok(vec![m.msg.method_return().append1(result)])
                        })
                        .outarg::<Vec<(u32, u32, Vec<(String, Variant<Box<dyn RefArg>>)>)>, _>(
                            "menus",
                        ),
                )
                .add_m(
                    factory
                        .method("End", (), move |m| {
                            let subscriptions: Vec<u32> =
                                m.msg.read1().map_err(|e| MethodErr::failed(&e))?;

                            debug_log(&format!("DBus End() called with {:?}", subscriptions));

                            Ok(vec![m.msg.method_return()])
                        })
                        .inarg::<Vec<u32>, _>("subscriptions"),
                );

            let menubar_path = connection.get_menubar_path();
            let tree = factory.tree(()).add(
                factory
                    .object_path(menubar_path, ())
                    .introspectable()
                    .add(interface),
            );

            tree.start_receive(&*conn_lock);

            debug_log("org.gtk.Menus interface registered successfully");
            Ok(())
        }

        #[cfg(not(all(target_os = "linux", feature = "gnome-menus")))]
        Err(GnomeMenuError::NotImplemented)
    }
}

impl Default for MenuProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_protocol_creation() {
        let protocol = MenuProtocol::new();
        assert!(protocol.menu_groups.lock().unwrap().is_empty());
    }

    #[test]
    fn test_menu_update() {
        let protocol = MenuProtocol::new();

        let group = DbusMenuGroup {
            group_id: 0,
            menu_id: 0,
            items: vec![DbusMenuItem {
                label: "File".to_string(),
                action: None,
                target: None,
                submenu: Some((1, 0)),
                section: None,
                enabled: true,
            }],
        };

        assert!(protocol.update_menu(vec![group]).is_ok());
        assert_eq!(protocol.menu_groups.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_start_method() {
        let protocol = MenuProtocol::new();

        let group = DbusMenuGroup {
            group_id: 0,
            menu_id: 0,
            items: vec![],
        };

        protocol.update_menu(vec![group]).unwrap();

        let result = protocol.handle_start(vec![0]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].group_id, 0);
    }
}
