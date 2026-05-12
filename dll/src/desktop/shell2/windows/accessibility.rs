//! Windows accessibility integration using accesskit
//!
//! This module provides the bridge between Azul's accessibility tree
//! and Windows UI Automation (UIA) via the accesskit library.
//!
//! Gated behind `cfg(feature = "a11y")` — a no-op stub is provided
//! when the feature is disabled.

use std::sync::{Arc, Mutex};

#[cfg(feature = "a11y")]
use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
#[cfg(feature = "a11y")]
use accesskit_windows::SubclassingAdapter;

#[cfg(feature = "a11y")]
use azul_core::dom::{AccessibilityAction, DomId, NodeId};
#[cfg(feature = "a11y")]
use crate::desktop::shell2::windows::dlopen::HWND;

/// Windows accessibility adapter that bridges Azul and UI Automation
#[cfg(feature = "a11y")]
pub struct WindowsAccessibilityAdapter {
    /// accesskit adapter for Windows
    adapter: Arc<Mutex<Option<SubclassingAdapter>>>,
    /// Pending actions from assistive technology
    pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}

#[cfg(feature = "a11y")]
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

        // Create the accesskit adapter - wrap in catch_unwind for safety
        let adapter_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            SubclassingAdapter::new(
                accesskit_windows::HWND(hwnd as _),
                activation_handler,
                action_handler,
            )
        }));

        match adapter_result {
            Ok(adapter) => {
                if let Ok(mut guard) = self.adapter.try_lock() {
                    *guard = Some(adapter);
                }
                Ok(())
            }
            Err(_) => {
                Err("accessibility adapter panicked during initialization".into())
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
        // Focus state is automatically managed by the SubclassingAdapter
        // through Windows messages, so we don't need to manually update it
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

        let mut iter = actions.into_iter();
        let request = iter.next()?;

        // Re-queue remaining actions
        let remaining: Vec<_> = iter.collect();
        if !remaining.is_empty() {
            if let Ok(mut pending) = self.pending_actions.try_lock() {
                pending.extend(remaining);
            }
        }

        Self::decode_action_request(request)
    }

    fn decode_action_request(request: ActionRequest) -> Option<(DomId, NodeId, AccessibilityAction)> {
        use azul_core::geom::LogicalPosition;
        use azul_css::{props::basic::FloatValue, AzString};

        // Upper 32 bits = DomId, Lower 32 bits = NodeId + 1
        let a11y_node_id: u64 = request.target_node.0;
        let dom_id = DomId {
            inner: (a11y_node_id >> 32) as usize,
        };
        let node_id = NodeId::new(((a11y_node_id & 0xFFFF_FFFF).wrapping_sub(1)) as usize);

        let action = match request.action {
            accesskit::Action::Click => AccessibilityAction::Default,
            accesskit::Action::Focus => AccessibilityAction::Focus,
            accesskit::Action::Blur => AccessibilityAction::Blur,
            accesskit::Action::Collapse => AccessibilityAction::Collapse,
            accesskit::Action::Expand => AccessibilityAction::Expand,
            accesskit::Action::Increment => AccessibilityAction::Increment,
            accesskit::Action::Decrement => AccessibilityAction::Decrement,
            accesskit::Action::ShowContextMenu => AccessibilityAction::ShowContextMenu,
            accesskit::Action::HideTooltip => AccessibilityAction::HideTooltip,
            accesskit::Action::ShowTooltip => AccessibilityAction::ShowTooltip,
            accesskit::Action::ScrollUp => AccessibilityAction::ScrollUp,
            accesskit::Action::ScrollDown => AccessibilityAction::ScrollDown,
            accesskit::Action::ScrollLeft => AccessibilityAction::ScrollLeft,
            accesskit::Action::ScrollRight => AccessibilityAction::ScrollRight,
            accesskit::Action::ScrollIntoView => AccessibilityAction::ScrollIntoView,
            accesskit::Action::ReplaceSelectedText => {
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    AccessibilityAction::ReplaceSelectedText(AzString::from(value.as_ref()))
                } else {
                    return None;
                }
            }
            accesskit::Action::ScrollToPoint => {
                if let Some(accesskit::ActionData::ScrollToPoint(point)) = request.data {
                    AccessibilityAction::ScrollToPoint(LogicalPosition {
                        x: point.x as f32,
                        y: point.y as f32,
                    })
                } else {
                    return None;
                }
            }
            accesskit::Action::SetScrollOffset => {
                if let Some(accesskit::ActionData::SetScrollOffset(point)) = request.data {
                    AccessibilityAction::SetScrollOffset(LogicalPosition {
                        x: point.x as f32,
                        y: point.y as f32,
                    })
                } else {
                    return None;
                }
            }
            accesskit::Action::SetTextSelection => {
                if let Some(accesskit::ActionData::SetTextSelection(selection)) = request.data {
                    AccessibilityAction::SetTextSelection(
                        azul_core::dom::TextSelectionStartEnd {
                            selection_start: selection.anchor.character_index,
                            selection_end: selection.focus.character_index,
                        },
                    )
                } else {
                    return None;
                }
            }
            accesskit::Action::SetSequentialFocusNavigationStartingPoint => {
                AccessibilityAction::SetSequentialFocusNavigationStartingPoint
            }
            accesskit::Action::SetValue => match request.data {
                Some(accesskit::ActionData::Value(value)) => {
                    AccessibilityAction::SetValue(AzString::from(value.as_ref()))
                }
                Some(accesskit::ActionData::NumericValue(value)) => {
                    AccessibilityAction::SetNumericValue(FloatValue::new(value as f32))
                }
                _ => return None,
            },
            accesskit::Action::CustomAction => {
                if let Some(accesskit::ActionData::CustomAction(id)) = request.data {
                    AccessibilityAction::CustomAction(id)
                } else {
                    return None;
                }
            }
        };

        Some((dom_id, node_id, action))
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

/// Stub implementation when accessibility feature is disabled
#[cfg(not(feature = "a11y"))]
#[derive(Debug, Clone, Copy)]
pub struct WindowsAccessibilityAdapter;

#[cfg(not(feature = "a11y"))]
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

    pub fn poll_action(&self) -> Option<()> {
        None
    }
}
