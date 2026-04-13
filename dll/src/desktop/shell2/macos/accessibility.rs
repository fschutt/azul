//! macOS accessibility integration via accesskit_macos
//!
//! This module handles the integration between Azul's accessibility tree
//! and macOS's NSAccessibility API through accesskit_macos.
//!
//! When the `a11y` feature is disabled, a no-op stub implementation is
//! provided so that call sites compile without conditional logic.

#[cfg(feature = "a11y")]
use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(feature = "a11y")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "a11y")]
use accesskit::{ActionRequest, TreeUpdate};
#[cfg(feature = "a11y")]
use accesskit_macos::SubclassingAdapter;
#[cfg(feature = "a11y")]
use azul_core::dom::{AccessibilityAction, DomId, NodeId};

#[cfg(feature = "a11y")]
/// Activation handler that provides the initial accessibility tree on demand
struct TreeActivationHandler {
    tree_provider: Arc<Mutex<Option<TreeUpdate>>>,
}

#[cfg(feature = "a11y")]
impl accesskit::ActivationHandler for TreeActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Return None to let the adapter go through the Placeholder state first.
        // When update_if_active() is later called with the real tree, the
        // Placeholder → Active transition generates proper focus events
        // (AXFocusedUIElementChanged) that VoiceOver needs to navigate correctly.
        // Returning Some here would skip Placeholder and go directly Inactive → Active,
        // which does NOT generate focus events.
        None
    }
}

#[cfg(feature = "a11y")]
/// Action handler that queues actions for later processing
struct ChannelActionHandler {
    sender: Sender<ActionRequest>,
}

#[cfg(feature = "a11y")]
impl accesskit::ActionHandler for ChannelActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.sender.send(request);
    }
}

#[cfg(feature = "a11y")]
/// macOS accessibility adapter that bridges Azul's accessibility tree
/// with NSAccessibility via accesskit
pub struct MacOSAccessibilityAdapter {
    /// The accesskit_macos adapter instance
    adapter: SubclassingAdapter,
    /// Channel for receiving action requests from assistive technologies
    action_receiver: Receiver<ActionRequest>,
    /// Shared tree provider for activation
    tree_provider: Arc<Mutex<Option<TreeUpdate>>>,
}

#[cfg(feature = "a11y")]
impl MacOSAccessibilityAdapter {
    /// Create a new accessibility adapter for a macOS NSView
    ///
    /// # Arguments
    /// - `view`: Raw pointer to the NSView object
    ///
    /// # Returns
    /// A new adapter instance that will handle bidirectional communication
    /// between the app and screen readers
    pub fn new(view: *mut std::ffi::c_void) -> Self {
        let (action_sender, action_receiver) = channel();
        let tree_provider = Arc::new(Mutex::new(None));

        // Create handlers
        let activation_handler = TreeActivationHandler {
            tree_provider: tree_provider.clone(),
        };
        let action_handler = ChannelActionHandler {
            sender: action_sender,
        };

        // SAFETY: view must be a valid NSView pointer
        let adapter = unsafe { SubclassingAdapter::new(view, activation_handler, action_handler) };

        Self {
            adapter,
            action_receiver,
            tree_provider,
        }
    }

    /// Update the accessibility tree with new state
    ///
    /// This should be called after each layout pass to synchronize
    /// the OS accessibility system with the application state.
    ///
    /// # Arguments
    /// - `tree_update`: The new tree state to submit
    ///
    /// # Note
    /// This function is designed to be non-blocking. If the a11y lock cannot
    /// be acquired immediately, the update is skipped to prevent UI hangs.
    pub fn update_tree(&mut self, tree_update: TreeUpdate) {
        crate::log_trace!(crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[a11y] update_tree: {} nodes, tree={}", tree_update.nodes.len(), tree_update.tree.is_some());

        // Store for next activation - use try_lock to avoid blocking
        if let Ok(mut guard) = self.tree_provider.try_lock() {
            *guard = Some(tree_update.clone());
        } else {
            crate::log_trace!(crate::desktop::shell2::common::debug_server::LogCategory::Platform, "[a11y] update_tree: lock contention, skipping");
            return;
        }

        // Update active tree and RAISE events.
        // QueuedEvents::raise() posts NSAccessibility notifications
        // (e.g. AXFocusedUIElementChanged) that VoiceOver listens for.
        // Without raising, VoiceOver never learns about tree changes.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.adapter.update_if_active(|| tree_update)
        }));
        match result {
            Ok(Some(events)) => {
                crate::log_trace!(crate::desktop::shell2::common::debug_server::LogCategory::Platform, "[a11y] update_tree: got QueuedEvents, raising");
                events.raise();
            }
            Ok(None) => {
                crate::log_trace!(crate::desktop::shell2::common::debug_server::LogCategory::Platform, "[a11y] update_tree: adapter inactive (no events)");
            }
            Err(e) => {
                let msg = e.downcast_ref::<String>().cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", e));
                crate::log_warn!(crate::desktop::shell2::common::debug_server::LogCategory::Platform, "[a11y] ERROR: update_if_active panicked: {}", msg);
            }
        }
    }

    /// Notify the adapter that the view's focus state changed.
    /// This must be called when the window gains or loses focus
    /// so VoiceOver knows which window is active.
    pub fn update_view_focus_state(&mut self, is_focused: bool) {
        crate::log_trace!(crate::desktop::shell2::common::debug_server::LogCategory::Platform, "[a11y] update_view_focus_state: is_focused={}", is_focused);
        if let Some(events) = self.adapter.update_view_focus_state(is_focused) {
            crate::log_trace!(crate::desktop::shell2::common::debug_server::LogCategory::Platform, "[a11y] update_view_focus_state: raising events");
            events.raise();
        }
    }

    /// Poll for action requests from assistive technologies
    ///
    /// This should be called regularly (e.g., in the event loop) to
    /// check if screen readers have requested any actions.
    ///
    /// # Returns
    /// An Option containing the decoded action request, or None if no actions pending
    pub fn poll_action(&self) -> Option<(DomId, NodeId, AccessibilityAction)> {
        let request = self.action_receiver.try_recv().ok()?;
        let (dom_id, node_id) = azul_layout::managers::a11y::decode_a11y_node_id(request.target_node);
        let action = azul_layout::managers::a11y::map_accesskit_action(request)?;
        Some((dom_id, node_id, action))
    }
}

/// No-op stub used when the `a11y` feature is disabled.
#[cfg(not(feature = "a11y"))]
#[derive(Clone, Copy)]
pub struct MacOSAccessibilityAdapter;

#[cfg(not(feature = "a11y"))]
impl MacOSAccessibilityAdapter {
    /// Create a no-op accessibility adapter (a11y feature disabled).
    pub fn new(_view: *mut std::ffi::c_void) -> Self {
        Self
    }

    /// No-op: accessibility tree updates are ignored without the `a11y` feature.
    pub fn update_tree(&mut self, _tree_update: ()) {}

    /// No-op: focus state updates are ignored without the `a11y` feature.
    pub fn update_view_focus_state(&mut self, _is_focused: bool) {}

    /// No-op: always returns `None` without the `a11y` feature.
    pub fn poll_action(&self) -> Option<()> {
        None
    }
}
