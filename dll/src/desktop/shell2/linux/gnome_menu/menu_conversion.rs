//! Menu Conversion - Azul Menu → DBus Format
//!
//! Converts Azul's Menu structure to the DBus format expected by GNOME Shell.

use std::sync::Arc;
use super::{GnomeMenuError, debug_log};
use super::menu_protocol::{DbusMenuItem, DbusMenuGroup};
use super::actions_protocol::DbusAction;

/// Menu conversion utilities
pub struct MenuConversion;

impl MenuConversion {
    /// Convert an Azul Menu to DBus format
    ///
    /// Transforms the Menu tree into flat groups that DBus can understand.
    pub fn convert_menu(menu: &azul_core::menu::Menu) -> Result<Vec<DbusMenuGroup>, GnomeMenuError> {
        debug_log("Converting Menu to DBus format");
        
        let mut groups = Vec::new();
        let mut next_group_id = 0u32;
        
        // Root menu (group 0, menu 0)
        let root_items = Self::convert_menu_items(&menu.items, &mut next_group_id)?;
        groups.push(DbusMenuGroup {
            group_id: 0,
            menu_id: 0,
            items: root_items,
        });
        
        // Recursively convert submenus
        Self::convert_submenus(&menu.items, &mut groups, &mut next_group_id)?;
        
        debug_log(&format!("Menu conversion complete: {} groups", groups.len()));
        Ok(groups)
    }

    /// Extract actions from Menu
    ///
    /// Collects all menu item callbacks and converts them to DBus actions.
    pub fn extract_actions(menu: &azul_core::menu::Menu) -> Result<Vec<DbusAction>, GnomeMenuError> {
        debug_log("Extracting actions from Menu");
        
        let mut actions = Vec::new();
        Self::extract_actions_recursive(&menu.items, "app", &mut actions)?;
        
        debug_log(&format!("Action extraction complete: {} actions", actions.len()));
        Ok(actions)
    }

    /// Convert menu items at a single level
    fn convert_menu_items(
        items: &azul_core::menu::MenuItemVec,
        next_group_id: &mut u32,
    ) -> Result<Vec<DbusMenuItem>, GnomeMenuError> {
        let mut dbus_items = Vec::new();
        
        for item in items.as_ref().iter() {
            match item {
                azul_core::menu::MenuItem::String(string_item) => {
                    // Generate action name from label
                    let action_name = if string_item.callback.is_some() {
                        Some(Self::generate_action_name(&string_item.label))
                    } else {
                        None
                    };
                    
                    // Check if item has children (submenu)
                    let has_children = !string_item.children.as_ref().is_empty();
                    let submenu = if has_children {
                        *next_group_id += 1;
                        Some((*next_group_id, 0))
                    } else {
                        None
                    };
                    
                    let enabled = match string_item.state {
                        azul_core::menu::MenuItemState::Normal => true,
                        azul_core::menu::MenuItemState::Greyed | 
                        azul_core::menu::MenuItemState::Disabled => false,
                    };
                    
                    dbus_items.push(DbusMenuItem {
                        label: string_item.label.as_str().to_string(),
                        action: action_name,
                        target: None,
                        submenu,
                        section: None,
                        enabled,
                    });
                }
                azul_core::menu::MenuItem::Separator => {
                    // Separators are represented as sections in DBus menus
                    dbus_items.push(DbusMenuItem {
                        label: String::new(),
                        action: None,
                        target: None,
                        submenu: None,
                        section: Some((0, 0)),
                        enabled: false,
                    });
                }
                azul_core::menu::MenuItem::BreakLine => {
                    // BreakLine is not supported in DBus menus, skip it
                    continue;
                }
            }
        }
        
        Ok(dbus_items)
    }

    /// Recursively convert submenus
    fn convert_submenus(
        items: &azul_core::menu::MenuItemVec,
        groups: &mut Vec<DbusMenuGroup>,
        next_group_id: &mut u32,
    ) -> Result<(), GnomeMenuError> {
        for item in items.as_ref().iter() {
            if let azul_core::menu::MenuItem::String(string_item) = item {
                if !string_item.children.as_ref().is_empty() {
                    let group_id = *next_group_id;
                    let submenu_items = Self::convert_menu_items(&string_item.children, next_group_id)?;
                    
                    groups.push(DbusMenuGroup {
                        group_id,
                        menu_id: 0,
                        items: submenu_items,
                    });
                    
                    // Recursively process submenus of this submenu
                    Self::convert_submenus(&string_item.children, groups, next_group_id)?;
                }
            }
        }
        
        Ok(())
    }

    /// Extract actions recursively
    fn extract_actions_recursive(
        items: &azul_core::menu::MenuItemVec,
        prefix: &str,
        actions: &mut Vec<DbusAction>,
    ) -> Result<(), GnomeMenuError> {
        for item in items.as_ref().iter() {
            if let azul_core::menu::MenuItem::String(string_item) = item {
                // Extract callback if present
                if let Some(ref callback) = string_item.callback.as_ref() {
                    let action_name = Self::generate_action_name(&string_item.label);
                    let enabled = match string_item.state {
                        azul_core::menu::MenuItemState::Normal => true,
                        azul_core::menu::MenuItemState::Greyed | 
                        azul_core::menu::MenuItemState::Disabled => false,
                    };
                    
                    // Clone the callback data
                    let callback_data = callback.data.clone();
                    let callback_fn = callback.callback.cb;
                    
                    actions.push(DbusAction {
                        name: action_name,
                        enabled,
                        parameter_type: None,
                        state: None,
                        callback: Arc::new(move |_param| {
                            unsafe {
                                type CallbackFn = unsafe extern "C" fn(
                                    *const std::ffi::c_void,
                                    *const std::ffi::c_void,
                                ) -> u8;
                                
                                let cb_fn: CallbackFn = std::mem::transmute(callback_fn);
                                (cb_fn)(std::ptr::null(), std::ptr::null());
                            }
                        }),
                    });
                }
                
                // Recursively extract from children
                if !string_item.children.as_ref().is_empty() {
                    Self::extract_actions_recursive(&string_item.children, prefix, actions)?;
                }
            }
        }
        
        Ok(())
    }

    /// Generate a valid DBus action name from a label
    fn generate_action_name(label: &azul_css::AzString) -> String {
        let label_str = label.as_str();
        let sanitized = label_str
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else if c.is_whitespace() || c == '-' {
                    '.'
                } else {
                    '_'
                }
            })
            .collect::<String>();
        
        format!("app.{}", sanitized)
    }

    /// Convert a single menu item to DBus format
    #[allow(dead_code)]
    fn convert_menu_item(
        item_label: &str,
        has_submenu: bool,
        submenu_group: Option<(u32, u32)>,
        action_name: Option<String>,
    ) -> DbusMenuItem {
        DbusMenuItem {
            label: item_label.to_string(),
            action: action_name,
            target: None,
            submenu: if has_submenu { submenu_group } else { None },
            section: None,
            enabled: true,
        }
    }

    /// Create a DBus action from a menu item callback
    #[allow(dead_code)]
    fn create_action(
        action_name: &str,
        enabled: bool,
        callback: impl Fn(Option<String>) + Send + Sync + 'static,
    ) -> DbusAction {
        DbusAction {
            name: action_name.to_string(),
            enabled,
            parameter_type: None,
            state: None,
            callback: Arc::new(callback),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_item_conversion() {
        let item = MenuConversion::convert_menu_item(
            "File",
            true,
            Some((1, 0)),
            None,
        );
        
        assert_eq!(item.label, "File");
        assert!(item.submenu.is_some());
        assert_eq!(item.submenu.unwrap(), (1, 0));
        assert!(item.enabled);
    }

    #[test]
    fn test_action_creation() {
        use std::sync::atomic::{AtomicBool, Ordering};
        
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        
        let action = MenuConversion::create_action(
            "app.test",
            true,
            move |_| {
                called_clone.store(true, Ordering::Relaxed);
            },
        );
        
        assert_eq!(action.name, "app.test");
        assert!(action.enabled);
        
        (action.callback)(None);
        assert!(called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_action_name_generation() {
        let label = azul_css::AzString::from_const_str("File > New");
        let action_name = MenuConversion::generate_action_name(&label);
        assert_eq!(action_name, "app.file...new");
    }

    #[test]
    fn test_convert_empty_menu() {
        let menu = azul_core::menu::Menu::new(azul_core::menu::MenuItemVec::from_vec(vec![]));
        let result = MenuConversion::convert_menu(&menu).unwrap();
        
        // Should have at least the root group
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].group_id, 0);
        assert_eq!(result[0].items.len(), 0);
    }
}

```

use std::sync::Arc;
use super::{GnomeMenuError, debug_log};
use super::menu_protocol::{DbusMenuItem, DbusMenuGroup};
use super::actions_protocol::DbusAction;

/// Menu conversion utilities
pub struct MenuConversion;

impl MenuConversion {
    /// Convert an Azul Menu to DBus format
    ///
    /// Transforms the Menu tree into flat groups that DBus can understand.
    pub fn convert_menu(_menu: &crate::menu::Menu) -> Result<Vec<DbusMenuGroup>, GnomeMenuError> {
        debug_log("Converting Menu to DBus format");
        
        // TODO: Implement actual conversion when Menu structure is accessible
        // For now, return empty to allow compilation
        
        // Future implementation will:
        // 1. Traverse Menu tree
        // 2. Create menu groups for each submenu level
        // 3. Generate unique group IDs
        // 4. Convert menu items to DbusMenuItem format
        // 5. Handle separators as sections
        
        debug_log("Menu conversion complete (stub)");
        Ok(vec![])
        
        /*
        let mut groups = Vec::new();
        let mut next_group_id = 0u32;
        
        // Root menu (group 0)
        let root_items = Self::convert_menu_items(&menu.items, &mut next_group_id)?;
        groups.push(DbusMenuGroup {
            group_id: 0,
            menu_id: 0,
            items: root_items,
        });
        
        // Recursively convert submenus
        Self::convert_submenus(&menu.items, &mut groups, &mut next_group_id)?;
        
        Ok(groups)
        */
    }

    /// Extract actions from Menu
    ///
    /// Collects all menu item callbacks and converts them to DBus actions.
    pub fn extract_actions(_menu: &crate::menu::Menu) -> Result<Vec<DbusAction>, GnomeMenuError> {
        debug_log("Extracting actions from Menu");
        
        // TODO: Implement actual action extraction when Menu structure is accessible
        
        // Future implementation will:
        // 1. Traverse all menu items
        // 2. Extract callback for each item
        // 3. Generate unique action name (e.g., "app.file.new")
        // 4. Wrap callback in Arc for thread-safety
        // 5. Determine enabled state
        
        debug_log("Action extraction complete (stub)");
        Ok(vec![])
        
        /*
        let mut actions = Vec::new();
        
        Self::extract_actions_recursive(&menu.items, "app", &mut actions)?;
        
        Ok(actions)
        */
    }

    /// Convert a single menu item to DBus format
    #[allow(dead_code)]
    fn convert_menu_item(
        item_label: &str,
        has_submenu: bool,
        submenu_group: Option<(u32, u32)>,
        action_name: Option<String>,
    ) -> DbusMenuItem {
        DbusMenuItem {
            label: item_label.to_string(),
            action: action_name,
            target: None,
            submenu: if has_submenu { submenu_group } else { None },
            section: None,
            enabled: true,
        }
    }

    /// Create a DBus action from a menu item callback
    #[allow(dead_code)]
    fn create_action(
        action_name: &str,
        enabled: bool,
        callback: impl Fn(Option<String>) + Send + Sync + 'static,
    ) -> DbusAction {
        DbusAction {
            name: action_name.to_string(),
            enabled,
            parameter_type: None,
            state: None,
            callback: Arc::new(callback),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_item_conversion() {
        let item = MenuConversion::convert_menu_item(
            "File",
            true,
            Some((1, 0)),
            None,
        );
        
        assert_eq!(item.label, "File");
        assert!(item.submenu.is_some());
        assert_eq!(item.submenu.unwrap(), (1, 0));
        assert!(item.enabled);
    }

    #[test]
    fn test_action_creation() {
        use std::sync::atomic::{AtomicBool, Ordering};
        
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        
        let action = MenuConversion::create_action(
            "app.test",
            true,
            move |_| {
                called_clone.store(true, Ordering::Relaxed);
            },
        );
        
        assert_eq!(action.name, "app.test");
        assert!(action.enabled);
        
        (action.callback)(None);
        assert!(called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_convert_menu_returns_empty() {
        // This is a placeholder test until Menu conversion is implemented
        // For now, just verify it doesn't panic
        
        // Can't test with real Menu without access to the struct
        // Will be updated in Week 2 when Menu structure is integrated
    }
}
