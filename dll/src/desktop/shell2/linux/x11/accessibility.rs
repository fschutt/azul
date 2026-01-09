//! Linux accessibility integration using accesskit
//!
//! This module provides the bridge between Azul's accessibility tree
//! and Linux AT-SPI (Assistive Technology Service Provider Interface)
//! via the accesskit_unix library.

use std::sync::{Arc, Mutex};

#[cfg(feature = "a11y")]
use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler,
    Node as AccesskitNode, NodeId as AccesskitNodeId, Role, Tree, TreeUpdate,
};
#[cfg(feature = "a11y")]
use accesskit_unix::Adapter;

/// Linux accessibility adapter that bridges Azul and AT-SPI
#[cfg(feature = "a11y")]
pub struct LinuxAccessibilityAdapter {
    /// accesskit adapter for Linux
    adapter: Arc<Mutex<Option<Adapter>>>,
    /// Pending actions from assistive technology
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}

#[cfg(feature = "a11y")]
impl LinuxAccessibilityAdapter {
    /// Create a new Linux accessibility adapter
    pub fn new() -> Self {
        Self {
            adapter: Arc::new(Mutex::new(None)),
            pending_actions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Initialize the adapter
    ///
    /// This must be called after the window is created to start
    /// the AT-SPI connection.
    pub fn initialize(&mut self, window_name: &str) -> Result<(), String> {
        let pending_actions = Arc::clone(&self.pending_actions);

        // Create handlers
        let activation_handler = AccessibilityActionHandler {
            pending_actions: Arc::clone(&pending_actions),
        };
        let action_handler = AccessibilityActionHandler {
            pending_actions: Arc::clone(&pending_actions),
        };
        let deactivation_handler = AccessibilityDeactivationHandler;

        // Create the accesskit adapter - wrap in catch_unwind for safety
        // DBus connection can fail in various ways
        let adapter_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Adapter::new(activation_handler, action_handler, deactivation_handler)
        }));

        match adapter_result {
            Ok(adapter) => {
                if let Ok(mut guard) = self.adapter.try_lock() {
                    *guard = Some(adapter);
                }
                Ok(())
            }
            Err(_) => {
                // Accessibility initialization failed - not critical, continue without it
                Ok(())
            }
        }
    }

    /// Update the accessibility tree
    ///
    /// This should be called after layout when the accessibility tree changes.
    ///
    /// # Note
    /// This function is designed to be non-blocking. If the a11y lock cannot
    /// be acquired immediately, the update is skipped to prevent UI hangs.
    pub fn update_tree(&self, tree_update: TreeUpdate) {
        // Use try_lock to avoid blocking the UI thread
        let Ok(mut guard) = self.adapter.try_lock() else {
            return; // Skip update if lock not available
        };
        
        if let Some(adapter) = guard.as_mut() {
            // Wrap in catch_unwind to prevent panics from crashing the app
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                adapter.update_if_active(|| tree_update);
            }));
        }
    }

    /// Notify that window focus changed
    pub fn set_focus(&self, _has_focus: bool) {
        // Focus state is automatically managed by the Adapter
        // through AT-SPI protocol, so we don't need to manually update it
    }

    /// Get pending actions from assistive technology
    ///
    /// Returns actions that screen readers or other AT requested,
    /// which should be processed by the application.
    pub fn take_pending_actions(&self) -> Vec<ActionRequest> {
        // Use try_lock to avoid blocking
        if let Ok(mut pending) = self.pending_actions.try_lock() {
            std::mem::take(&mut *pending)
        } else {
            Vec::new()
        }
    }
}

#[cfg(feature = "a11y")]
struct AccessibilityActionHandler {
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}

#[cfg(feature = "a11y")]
impl ActivationHandler for AccessibilityActionHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Return None - initial tree will be set after first layout
        None
    }
}

#[cfg(feature = "a11y")]
impl ActionHandler for AccessibilityActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        // Queue the action for processing by the main event loop
        // Use try_lock to avoid blocking - drop action if lock unavailable
        if let Ok(mut pending) = self.pending_actions.try_lock() {
            pending.push(request);
        }
    }
}

#[cfg(feature = "a11y")]
struct AccessibilityDeactivationHandler;

#[cfg(feature = "a11y")]
impl DeactivationHandler for AccessibilityDeactivationHandler {
    fn deactivate_accessibility(&mut self) {
        // Called when accessibility is deactivated
        // No cleanup needed for now
    }
}

/// Stub implementation when accessibility feature is disabled
#[cfg(not(feature = "a11y"))]
pub struct LinuxAccessibilityAdapter;

#[cfg(not(feature = "a11y"))]
impl LinuxAccessibilityAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn initialize(&mut self, _window_name: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn update_tree(&self, _tree_update: ()) {}

    pub fn set_focus(&self, _has_focus: bool) {}

    pub fn take_pending_actions(&self) -> Vec<()> {
        Vec::new()
    }
}
