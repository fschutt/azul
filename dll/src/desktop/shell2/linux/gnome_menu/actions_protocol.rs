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

use std::sync::{Arc, Mutex};

use azul_core::menu::CoreMenuCallback;

use super::debug_log;

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
