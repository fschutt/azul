//! Windows accessibility integration using accesskit
//!
//! This module provides the bridge between Azul's accessibility tree
//! and Windows UI Automation (UIA) via the accesskit library.

use std::sync::{Arc, Mutex};

#[cfg(feature = "accessibility")]
use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, Node as AccesskitNode,
    NodeId as AccesskitNodeId, Role, Tree, TreeUpdate,
};
#[cfg(feature = "accessibility")]
use accesskit_windows::{Adapter, SubclassingAdapter};

#[cfg(feature = "accessibility")]
use crate::desktop::shell2::windows::dlopen::HWND;

/// Windows accessibility adapter that bridges Azul and UI Automation
#[cfg(feature = "accessibility")]
pub struct WindowsAccessibilityAdapter {
    /// accesskit adapter for Windows
    adapter: Arc<Mutex<Option<SubclassingAdapter>>>,
    /// Pending actions from assistive technology
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}

#[cfg(feature = "accessibility")]
impl WindowsAccessibilityAdapter {
    /// Create a new Windows accessibility adapter
    pub fn new() -> Self {
        Self {
            adapter: Arc::new(Mutex::new(None)),
            pending_actions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Initialize the adapter with a window handle
    ///
    /// This must be called after the window is created to attach
    /// the accessibility adapter to the window.
    pub fn initialize(&mut self, hwnd: HWND) -> Result<(), String> {
        let pending_actions = Arc::clone(&self.pending_actions);

        // Create handlers (same struct, but two instances for the API)
        let activation_handler = AccessibilityActionHandler {
            pending_actions: Arc::clone(&pending_actions),
        };
        let action_handler = AccessibilityActionHandler {
            pending_actions: Arc::clone(&pending_actions),
        };

        // Create the accesskit adapter
        let adapter = SubclassingAdapter::new(
            accesskit_windows::HWND(hwnd as _),
            activation_handler,
            action_handler,
        );

        *self.adapter.lock().unwrap() = Some(adapter);

        Ok(())
    }

    /// Update the accessibility tree
    ///
    /// This should be called after layout when the accessibility tree changes.
    pub fn update_tree(&self, tree_update: TreeUpdate) {
        if let Some(adapter) = self.adapter.lock().unwrap().as_mut() {
            adapter.update_if_active(|| tree_update);
        }
    }

    /// Notify that window focus changed
    pub fn set_focus(&self, _has_focus: bool) {
        // Focus state is automatically managed by the SubclassingAdapter
        // through Windows messages, so we don't need to manually update it
    }

    /// Get pending actions from assistive technology
    ///
    /// Returns actions that screen readers or other AT requested,
    /// which should be processed by the application.
    pub fn take_pending_actions(&self) -> Vec<ActionRequest> {
        let mut pending = self.pending_actions.lock().unwrap();
        std::mem::take(&mut *pending)
    }
}

#[cfg(feature = "accessibility")]
struct AccessibilityActionHandler {
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}

#[cfg(feature = "accessibility")]
impl ActivationHandler for AccessibilityActionHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Return None - initial tree will be set after first layout
        None
    }
}

#[cfg(feature = "accessibility")]
impl ActionHandler for AccessibilityActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        // Queue the action for processing by the main event loop
        self.pending_actions.lock().unwrap().push(request);
    }
}

/// Stub implementation when accessibility feature is disabled
#[cfg(not(feature = "accessibility"))]
pub struct WindowsAccessibilityAdapter;

#[cfg(not(feature = "accessibility"))]
impl WindowsAccessibilityAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn initialize(&mut self, _hwnd: *mut std::ffi::c_void) -> Result<(), String> {
        Ok(())
    }

    pub fn update_tree(&self, _tree_update: ()) {}

    pub fn set_focus(&self, _has_focus: bool) {}

    pub fn take_pending_actions(&self) -> Vec<()> {
        Vec::new()
    }
}
