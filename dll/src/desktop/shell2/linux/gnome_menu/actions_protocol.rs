//! org.gtk.Actions Protocol Implementation
//!
//! Implements the DBus interface for action dispatch.
//!
//! ## Interface Methods
//!
//! - `List() → as`
//!   - Return array of action names
//!
//! - `Describe(action: s) → (bsav)`
//!   - Return (enabled, param_type, state) for an action
//!
//! - `DescribeAll() → a{s(bsav)}`
//!   - Return all actions with descriptions
//!
//! - `Activate(action: s, parameter: av, platform_data: a{sv})`
//!   - Invoke action callback

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use super::{GnomeMenuError, debug_log};

/// Represents an action that can be invoked
#[derive(Clone)]
pub struct DbusAction {
    pub name: String,
    pub enabled: bool,
    pub parameter_type: Option<String>,
    pub state: Option<String>,
    pub callback: Arc<dyn Fn(Option<String>) + Send + Sync>,
}

impl std::fmt::Debug for DbusAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbusAction")
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("parameter_type", &self.parameter_type)
            .field("state", &self.state)
            .finish()
    }
}

/// org.gtk.Actions protocol handler
pub struct ActionsProtocol {
    actions: Arc<Mutex<HashMap<String, DbusAction>>>,
}

impl ActionsProtocol {
    /// Create a new actions protocol handler
    pub fn new() -> Self {
        debug_log("Initializing org.gtk.Actions protocol");
        
        Self {
            actions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register actions
    ///
    /// Stores actions for later invocation by GNOME Shell.
    pub fn register_actions(&self, actions: Vec<DbusAction>) -> Result<(), GnomeMenuError> {
        let mut action_map = self.actions.lock().unwrap();
        
        action_map.clear();
        for action in actions {
            debug_log(&format!("Registering action: {} (enabled: {})", 
                action.name, action.enabled));
            action_map.insert(action.name.clone(), action);
        }

        debug_log(&format!("Registered {} actions", action_map.len()));
        Ok(())
    }

    /// Handle List method call
    ///
    /// Returns all action names.
    pub fn handle_list(&self) -> Result<Vec<String>, GnomeMenuError> {
        let actions = self.actions.lock().unwrap();
        let names: Vec<String> = actions.keys().cloned().collect();
        
        debug_log(&format!("List method called, returning {} actions", names.len()));
        Ok(names)
    }

    /// Handle Describe method call
    ///
    /// Returns (enabled, param_type, state) for the requested action.
    pub fn handle_describe(&self, action_name: &str) -> Result<(bool, String, Vec<String>), GnomeMenuError> {
        let actions = self.actions.lock().unwrap();
        
        if let Some(action) = actions.get(action_name) {
            let param_type = action.parameter_type.clone().unwrap_or_default();
            let state = if let Some(s) = &action.state {
                vec![s.clone()]
            } else {
                vec![]
            };
            
            debug_log(&format!("Describe method called for: {}", action_name));
            Ok((action.enabled, param_type, state))
        } else {
            debug_log(&format!("Warning: Describe called for unknown action: {}", action_name));
            Err(GnomeMenuError::ActionRegistrationFailed(
                format!("Action not found: {}", action_name)
            ))
        }
    }

    /// Handle DescribeAll method call
    ///
    /// Returns all actions with their descriptions.
    pub fn handle_describe_all(&self) -> Result<HashMap<String, (bool, String, Vec<String>)>, GnomeMenuError> {
        let actions = self.actions.lock().unwrap();
        let mut result = HashMap::new();

        for (name, action) in actions.iter() {
            let param_type = action.parameter_type.clone().unwrap_or_default();
            let state = if let Some(s) = &action.state {
                vec![s.clone()]
            } else {
                vec![]
            };
            
            result.insert(name.clone(), (action.enabled, param_type, state));
        }

        debug_log(&format!("DescribeAll method called, returning {} actions", result.len()));
        Ok(result)
    }

    /// Handle Activate method call
    ///
    /// Invokes the callback for the requested action.
    pub fn handle_activate(
        &self, 
        action_name: &str, 
        parameter: Option<String>
    ) -> Result<(), GnomeMenuError> {
        let actions = self.actions.lock().unwrap();
        
        if let Some(action) = actions.get(action_name) {
            debug_log(&format!("Activate method called for: {} with parameter: {:?}", 
                action_name, parameter));
            
            if action.enabled {
                // Invoke the callback
                (action.callback)(parameter);
                Ok(())
            } else {
                debug_log(&format!("Warning: Attempt to activate disabled action: {}", action_name));
                Err(GnomeMenuError::ActionRegistrationFailed(
                    format!("Action is disabled: {}", action_name)
                ))
            }
        } else {
            debug_log(&format!("Warning: Activate called for unknown action: {}", action_name));
            Err(GnomeMenuError::ActionRegistrationFailed(
                format!("Action not found: {}", action_name)
            ))
        }
    }

    /// Register with DBus
    ///
    /// Sets up the DBus method handlers for org.gtk.Actions interface.
    pub fn register_with_dbus(&self, connection: &super::DbusConnection) -> Result<(), GnomeMenuError> {
        debug_log("Registering org.gtk.Actions interface with DBus");
        
        #[cfg(all(target_os = "linux", feature = "gnome-menus"))]
        {
            use dbus::blocking::Connection;
            use dbus::tree::{Factory, MethodErr};
            use dbus::arg::{Variant, RefArg};
            use std::collections::HashMap as StdHashMap;
            
            let conn = connection.get_connection();
            let conn_lock = conn.lock().unwrap();
            
            let factory = Factory::new_fn::<()>();
            let actions_data1 = self.actions.clone();
            let actions_data2 = self.actions.clone();
            let actions_data3 = self.actions.clone();
            let actions_data4 = self.actions.clone();
            
            let interface = factory
                .interface("org.gtk.Actions", ())
                .add_m(factory.method("List", (), move |m| {
                    debug_log("DBus List() called");
                    
                    let actions = actions_data1.lock().unwrap();
                    let names: Vec<String> = actions.keys().cloned().collect();
                    
                    Ok(vec![m.msg.method_return().append1(names)])
                }).outarg::<Vec<String>, _>("actions"))
                .add_m(factory.method("Describe", (), move |m| {
                    let action_name: String = m.msg.read1()
                        .map_err(|e| MethodErr::failed(&e))?;
                    
                    debug_log(&format!("DBus Describe() called for: {}", action_name));
                    
                    let actions = actions_data2.lock().unwrap();
                    
                    if let Some(action) = actions.get(&action_name) {
                        let param_type = action.parameter_type.clone().unwrap_or_default();
                        let state: Vec<Variant<Box<dyn RefArg>>> = vec![];
                        
                        Ok(vec![m.msg.method_return().append3(action.enabled, param_type, state)])
                    } else {
                        Err(MethodErr::failed(&format!("Action not found: {}", action_name)))
                    }
                }).inarg::<String, _>("action")
                  .outarg::<(bool, String, Vec<Variant<Box<dyn RefArg>>>), _>("description"))
                .add_m(factory.method("DescribeAll", (), move |m| {
                    debug_log("DBus DescribeAll() called");
                    
                    let actions = actions_data3.lock().unwrap();
                    let mut result = StdHashMap::new();
                    
                    for (name, action) in actions.iter() {
                        let param_type = action.parameter_type.clone().unwrap_or_default();
                        let state: Vec<Variant<Box<dyn RefArg>>> = vec![];
                        result.insert(name.clone(), (action.enabled, param_type, state));
                    }
                    
                    Ok(vec![m.msg.method_return().append1(result)])
                }).outarg::<StdHashMap<String, (bool, String, Vec<Variant<Box<dyn RefArg>>>)>, _>("actions"))
                .add_m(factory.method("Activate", (), move |m| {
                    let (action_name, parameter, _platform_data): (String, Vec<Variant<Box<dyn RefArg>>>, StdHashMap<String, Variant<Box<dyn RefArg>>>) = 
                        m.msg.read3().map_err(|e| MethodErr::failed(&e))?;
                    
                    debug_log(&format!("DBus Activate() called for: {}", action_name));
                    
                    let actions = actions_data4.lock().unwrap();
                    
                    if let Some(action) = actions.get(&action_name) {
                        if action.enabled {
                            // Extract parameter if present
                            let param_str = parameter.get(0).and_then(|v| {
                                v.as_str().map(|s| s.to_string())
                            });
                            
                            // Invoke callback (drop lock first to avoid deadlock)
                            let callback = action.callback.clone();
                            drop(actions);
                            
                            callback(param_str);
                            
                            Ok(vec![m.msg.method_return()])
                        } else {
                            Err(MethodErr::failed(&format!("Action is disabled: {}", action_name)))
                        }
                    } else {
                        Err(MethodErr::failed(&format!("Action not found: {}", action_name)))
                    }
                }).inarg::<String, _>("action")
                  .inarg::<Vec<Variant<Box<dyn RefArg>>>, _>("parameter")
                  .inarg::<StdHashMap<String, Variant<Box<dyn RefArg>>>, _>("platform_data"));
            
            let object_path = connection.get_object_path();
            let tree = factory.tree(()).add(
                factory.object_path(object_path, ())
                    .introspectable()
                    .add(interface)
            );
            
            tree.start_receive(&*conn_lock);
            
            debug_log("org.gtk.Actions interface registered successfully");
            Ok(())
        }
        
        #[cfg(not(all(target_os = "linux", feature = "gnome-menus")))]
        Err(GnomeMenuError::NotImplemented)
    }
}

impl Default for ActionsProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actions_protocol_creation() {
        let protocol = ActionsProtocol::new();
        assert!(protocol.actions.lock().unwrap().is_empty());
    }

    #[test]
    fn test_action_registration() {
        let protocol = ActionsProtocol::new();
        
        let action = DbusAction {
            name: "app.quit".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
            callback: Arc::new(|_| {}),
        };

        assert!(protocol.register_actions(vec![action]).is_ok());
        assert_eq!(protocol.actions.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_list_method() {
        let protocol = ActionsProtocol::new();
        
        let action = DbusAction {
            name: "app.quit".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
            callback: Arc::new(|_| {}),
        };
        
        protocol.register_actions(vec![action]).unwrap();
        
        let names = protocol.handle_list().unwrap();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"app.quit".to_string()));
    }

    #[test]
    fn test_describe_method() {
        let protocol = ActionsProtocol::new();
        
        let action = DbusAction {
            name: "app.quit".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
            callback: Arc::new(|_| {}),
        };
        
        protocol.register_actions(vec![action]).unwrap();
        
        let (enabled, param_type, state) = protocol.handle_describe("app.quit").unwrap();
        assert!(enabled);
        assert_eq!(param_type, "");
        assert!(state.is_empty());
    }

    #[test]
    fn test_activate_method() {
        use std::sync::atomic::{AtomicBool, Ordering};
        
        let protocol = ActionsProtocol::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        
        let action = DbusAction {
            name: "app.test".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
            callback: Arc::new(move |_| {
                called_clone.store(true, Ordering::Relaxed);
            }),
        };
        
        protocol.register_actions(vec![action]).unwrap();
        protocol.handle_activate("app.test", None).unwrap();
        
        assert!(called.load(Ordering::Relaxed));
    }
}
