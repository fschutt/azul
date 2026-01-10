//! macOS accessibility integration via accesskit_macos
//!
//! This module handles the integration between Azul's accessibility tree
//! and macOS's NSAccessibility API through accesskit_macos.

#[cfg(feature = "a11y")]
use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(feature = "a11y")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "a11y")]
use accesskit::{ActionRequest, NodeId as A11yNodeId, TreeUpdate};
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
        // Use try_lock to avoid blocking - return None if lock unavailable
        self.tree_provider
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take())
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
        // Store for next activation - use try_lock to avoid blocking
        if let Ok(mut guard) = self.tree_provider.try_lock() {
            *guard = Some(tree_update.clone());
        } else {
            // Lock contention - skip this update to avoid blocking the UI
            return;
        }

        // Update active tree - wrapped in catch_unwind to prevent panics
        // from crashing the application
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.adapter.update_if_active(|| tree_update);
        }));
    }

    /// Poll for action requests from assistive technologies
    ///
    /// This should be called regularly (e.g., in the event loop) to
    /// check if screen readers have requested any actions.
    ///
    /// # Returns
    /// An Option containing the decoded action request, or None if no actions pending
    pub fn poll_action(&self) -> Option<(DomId, NodeId, AccessibilityAction)> {
        // Try to receive action without blocking
        if let Ok(request) = self.action_receiver.try_recv() {
            // Decode the NodeId back to DomId + NodeId
            let a11y_node_id: u64 = request.target.0.into();
            let dom_id = DomId {
                inner: (a11y_node_id >> 32) as usize,
            };
            let node_id = NodeId::new((a11y_node_id & 0xFFFFFFFF) as usize);

            // Map accesskit Action to Azul AccessibilityAction
            use azul_core::geom::LogicalPosition;
            use azul_css::{props::basic::FloatValue, AzString};
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
        } else {
            None
        }
    }
}

// Stub for when accessibility feature is disabled
#[cfg(not(feature = "a11y"))]
#[derive(Clone, Copy)]
pub struct MacOSAccessibilityAdapter;

#[cfg(not(feature = "a11y"))]
impl MacOSAccessibilityAdapter {
    pub fn new(_view: *mut std::ffi::c_void, _initial_tree: ()) -> Self {
        Self
    }

    pub fn update_tree(&self, _tree_update: ()) {}

    pub fn poll_action(&self) -> Option<()> {
        None
    }

    pub fn view(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}
