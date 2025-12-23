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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use azul_core::menu::CoreMenuCallback;

use super::{debug_log, GnomeMenuError};

/// Pending menu callback to be processed in the main event loop.
///
/// When a DBus action is activated, we can't invoke the Azul callback directly
/// because we don't have access to the full window state (CallbackInfo).
/// Instead, we queue the callback data here and let the event loop process it.
#[derive(Clone)]
pub struct PendingMenuCallback {
    /// The action name that was activated
    pub action_name: String,
    /// The original menu callback data (RefAny + callback function pointer)
    pub menu_callback: CoreMenuCallback,
}

/// Global queue for pending menu callbacks.
///
/// DBus handlers add to this queue, and the X11/Wayland event loop drains it.
/// Using a lazy_static mutex for thread-safe access from DBus callback context.
static PENDING_MENU_CALLBACKS: std::sync::LazyLock<Mutex<Vec<PendingMenuCallback>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// Add a pending menu callback to the queue
pub fn queue_menu_callback(callback: PendingMenuCallback) {
    if let Ok(mut queue) = PENDING_MENU_CALLBACKS.lock() {
        debug_log(&format!(
            "Queuing menu callback for action: {}",
            callback.action_name
        ));
        queue.push(callback);
    }
}

/// Drain all pending menu callbacks from the queue
pub fn drain_pending_menu_callbacks() -> Vec<PendingMenuCallback> {
    if let Ok(mut queue) = PENDING_MENU_CALLBACKS.lock() {
        std::mem::take(&mut *queue)
    } else {
        Vec::new()
    }
}

/// Represents an action that can be invoked
#[derive(Clone)]
pub struct DbusAction {
    pub name: String,
    pub enabled: bool,
    pub parameter_type: Option<String>,
    pub state: Option<String>,
    /// The callback that queues the menu callback for processing in the event loop
    pub callback: Arc<dyn Fn(Option<String>) + Send + Sync>,
    /// The original menu callback data (stored for proper invocation)
    pub menu_callback: Option<CoreMenuCallback>,
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
            debug_log(&format!(
                "Registering action: {} (enabled: {})",
                action.name, action.enabled
            ));
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

        debug_log(&format!(
            "List method called, returning {} actions",
            names.len()
        ));
        Ok(names)
    }

    /// Handle Describe method call
    ///
    /// Returns (enabled, param_type, state) for the requested action.
    pub fn handle_describe(
        &self,
        action_name: &str,
    ) -> Result<(bool, String, Vec<String>), GnomeMenuError> {
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
            debug_log(&format!(
                "Warning: Describe called for unknown action: {}",
                action_name
            ));
            Err(GnomeMenuError::ActionRegistrationFailed(format!(
                "Action not found: {}",
                action_name
            )))
        }
    }

    /// Handle DescribeAll method call
    ///
    /// Returns all actions with their descriptions.
    pub fn handle_describe_all(
        &self,
    ) -> Result<HashMap<String, (bool, String, Vec<String>)>, GnomeMenuError> {
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

        debug_log(&format!(
            "DescribeAll method called, returning {} actions",
            result.len()
        ));
        Ok(result)
    }

    /// Handle Activate method call
    ///
    /// Invokes the callback for the requested action.
    pub fn handle_activate(
        &self,
        action_name: &str,
        parameter: Option<String>,
    ) -> Result<(), GnomeMenuError> {
        let actions = self.actions.lock().unwrap();

        if let Some(action) = actions.get(action_name) {
            debug_log(&format!(
                "Activate method called for: {} with parameter: {:?}",
                action_name, parameter
            ));

            if action.enabled {
                // Invoke the callback
                (action.callback)(parameter);
                Ok(())
            } else {
                debug_log(&format!(
                    "Warning: Attempt to activate disabled action: {}",
                    action_name
                ));
                Err(GnomeMenuError::ActionRegistrationFailed(format!(
                    "Action is disabled: {}",
                    action_name
                )))
            }
        } else {
            debug_log(&format!(
                "Warning: Activate called for unknown action: {}",
                action_name
            ));
            Err(GnomeMenuError::ActionRegistrationFailed(format!(
                "Action not found: {}",
                action_name
            )))
        }
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
            menu_callback: None,
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
            menu_callback: None,
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
            menu_callback: None,
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
            menu_callback: None,
        };

        protocol.register_actions(vec![action]).unwrap();
        protocol.handle_activate("app.test", None).unwrap();

        assert!(called.load(Ordering::Relaxed));
    }
}
