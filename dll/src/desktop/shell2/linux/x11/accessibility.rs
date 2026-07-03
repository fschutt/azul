//! Linux accessibility integration using accesskit
//!
//! This module provides the bridge between Azul's accessibility tree
//! and Linux AT-SPI (Assistive Technology Service Provider Interface)
//! via the accesskit_unix library.

use std::sync::{Arc, Mutex};

#[cfg(feature = "a11y")]
use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate,
};
#[cfg(feature = "a11y")]
use accesskit_unix::Adapter;

#[cfg(feature = "a11y")]
use azul_core::dom::{AccessibilityAction, DomId, NodeId};

/// Linux accessibility adapter that bridges Azul and AT-SPI
#[cfg(feature = "a11y")]
pub struct LinuxAccessibilityAdapter {
    /// accesskit adapter for Linux
    adapter: Arc<Mutex<Option<Adapter>>>,
    /// Pending actions from assistive technology
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
    /// The most recent tree we built. Shared with the ActivationHandler so
    /// `request_initial_tree()` can hand the adapter a non-empty tree the
    /// moment an AT (screen reader) connects. Without it the handler returned
    /// None → the adapter never activated → `update_if_active()` stayed a
    /// no-op, i.e. a11y silently did nothing.
    last_tree: Arc<Mutex<Option<TreeUpdate>>>,
}

#[cfg(feature = "a11y")]
impl LinuxAccessibilityAdapter {
    /// Create a new Linux accessibility adapter
    pub fn new() -> Self {
        Self {
            adapter: Arc::new(Mutex::new(None)),
            pending_actions: Arc::new(Mutex::new(Vec::new())),
            last_tree: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize the adapter
    ///
    /// This must be called after the window is created to start
    /// the AT-SPI connection.
    pub fn initialize(&mut self, _window_name: &str) -> Result<(), String> {
        let pending_actions = Arc::clone(&self.pending_actions);
        let last_tree = Arc::clone(&self.last_tree);

        // Create handlers
        let activation_handler = AccessibilityActionHandler {
            pending_actions: Arc::clone(&pending_actions),
            last_tree: Arc::clone(&last_tree),
        };
        let action_handler = AccessibilityActionHandler {
            pending_actions: Arc::clone(&pending_actions),
            last_tree: Arc::clone(&last_tree),
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
        // Remember the latest tree so request_initial_tree() (called when an AT
        // activates the adapter) can return a non-empty tree — otherwise the
        // adapter never activates and update_if_active() below is a no-op.
        if let Ok(mut last) = self.last_tree.try_lock() {
            *last = Some(tree_update.clone());
        }
        // Use try_lock to avoid blocking the UI thread
        let Ok(mut guard) = self.adapter.try_lock() else {
            return; // Skip update if lock not available
        };

        if let Some(adapter) = guard.as_mut() {
            // Wrap in catch_unwind to prevent panics from crashing the app.
            // accesskit's unix adapter raises AT-SPI events internally and
            // returns `()` (unlike the macOS adapter which hands back a
            // `QueuedEvents`), so there's nothing to dispatch on our side.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                adapter.update_if_active(|| tree_update);
            }));
        }
    }

    /// Notify that window focus changed.
    ///
    /// MWA-A3b: the previous "automatically managed" comment was WRONG —
    /// accesskit_unix does NOT observe window focus itself. Its adapter
    /// starts with `is_window_focused: false` forever unless told, which
    /// suppressed AT-SPI focus events entirely: Orca never announced focus
    /// moves inside azul windows. The platform shells call this from X11
    /// FocusIn/FocusOut and wl_keyboard enter/leave (this adapter is shared
    /// by both Linux backends).
    pub fn set_focus(&self, has_focus: bool) {
        let Ok(mut guard) = self.adapter.try_lock() else {
            return;
        };
        if let Some(adapter) = guard.as_mut() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                adapter.update_window_focus_state(has_focus);
            }));
        }
    }

    /// MWA-C-a11y: tell AT-SPI where the window sits on screen (physical
    /// pixels, root coordinates). accesskit_unix composes node bounds with
    /// these root bounds for screen-coordinate queries (magnifier tracking,
    /// "where am I") — with no caller everything reported origin (0,0).
    /// Called from X11 ConfigureNotify; Wayland has no global window
    /// position by design, so this stays X11-only. Outer/inner are the same
    /// rect (azul draws CSD inside the client area; there is no OS frame we
    /// could measure portably).
    pub fn set_root_window_bounds(&self, x: f64, y: f64, width: f64, height: f64) {
        let rect = accesskit::Rect {
            x0: x,
            y0: y,
            x1: x + width,
            y1: y + height,
        };
        let Ok(mut guard) = self.adapter.try_lock() else {
            return;
        };
        if let Some(adapter) = guard.as_mut() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                adapter.set_root_window_bounds(rect, rect);
            }));
        }
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

    /// Poll for a single accessibility action, decoded into Azul types
    pub fn poll_action(&self) -> Option<(DomId, NodeId, AccessibilityAction)> {
        let actions = self.take_pending_actions();
        if actions.is_empty() {
            return None;
        }

        // Process first action, re-queue the rest
        let mut iter = actions.into_iter();
        let request = iter.next()?;

        // Re-queue remaining actions
        let remaining: Vec<_> = iter.collect();
        if !remaining.is_empty() {
            if let Ok(mut pending) = self.pending_actions.try_lock() {
                pending.extend(remaining);
            }
        }

        let (dom_id, node_id) =
            azul_layout::managers::a11y::decode_a11y_node_id(request.target_node);
        let action = azul_layout::managers::a11y::map_accesskit_action(request)?;
        Some((dom_id, node_id, action))
    }
}

#[cfg(feature = "a11y")]
struct AccessibilityActionHandler {
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
    /// Shared with LinuxAccessibilityAdapter::update_tree — the latest tree,
    /// returned from request_initial_tree() so the adapter activates with a
    /// populated tree.
    last_tree: Arc<Mutex<Option<TreeUpdate>>>,
}

#[cfg(feature = "a11y")]
impl ActivationHandler for AccessibilityActionHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Hand the freshly-activated adapter the latest tree we built. Returning
        // None left the adapter inactive (update_if_active is a no-op until the
        // adapter is active), so a screen reader connecting after the first
        // layout saw an empty accessibility tree.
        self.last_tree.lock().ok().and_then(|g| (*g).clone())
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
#[derive(Debug, Clone, Copy)]
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

    pub fn set_root_window_bounds(&self, _x: f64, _y: f64, _width: f64, _height: f64) {}

    pub fn take_pending_actions(&self) -> Vec<()> {
        Vec::new()
    }
}
