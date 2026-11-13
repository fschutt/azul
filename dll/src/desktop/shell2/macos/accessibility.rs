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
use std::sync::Arc;

#[cfg(feature = "a11y")]
use accesskit::{ActionRequest, TreeUpdate};
#[cfg(feature = "a11y")]
use accesskit_macos::SubclassingAdapter;
#[cfg(feature = "a11y")]
use azul_core::dom::{AccessibilityAction, DomId, NodeId};

#[cfg(feature = "a11y")]
use crate::desktop::shell2::common::accessibility::A11yTreeFeed;

#[cfg(feature = "a11y")]
/// Activation handler that provides the initial accessibility tree on demand
struct TreeActivationHandler {
    feed: Arc<A11yTreeFeed>,
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
        self.feed.placeholder_requested();
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
    feed: Arc<A11yTreeFeed>,
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
        let feed = A11yTreeFeed::new();

        // Create handlers
        let activation_handler = TreeActivationHandler { feed: feed.clone() };
        let action_handler = ChannelActionHandler {
            sender: action_sender,
        };

        // SAFETY: view must be a valid NSView pointer
        let adapter = unsafe { SubclassingAdapter::new(view, activation_handler, action_handler) };

        Self {
            adapter,
            action_receiver,
            feed,
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
        crate::log_trace!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[a11y] update_tree: {} nodes, tree={}",
            tree_update.nodes.len(),
            tree_update.tree.is_some()
        );

        // The window root names the app's language (every element inherits
        // it); VoiceOver asks each element for it as `AXLanguage`.
        if let Some(language) = tree_update
            .nodes
            .iter()
            .find(|(id, _)| id.0 == 0)
            .map(|(_, root)| root.language().unwrap_or_default().to_string())
        {
            ax_language::set(&language);
        }
        ax_language::install();

        let Some(tree_update) = self.feed.next_update(tree_update) else {
            return;
        };
        let had_tree = tree_update.tree.is_some();

        // Update active tree and RAISE events.
        // QueuedEvents::raise() posts NSAccessibility notifications
        // (e.g. AXFocusedUIElementChanged) that VoiceOver listens for.
        // Without raising, VoiceOver never learns about tree changes.
        let result = crate::desktop::recoverable_panic::catch(|| {
            self.adapter.update_if_active(|| tree_update)
        });
        match result {
            Ok(Some(events)) => {
                self.feed.delivered(had_tree);
                crate::log_trace!(
                    crate::desktop::shell2::common::debug_server::LogCategory::Platform,
                    "[a11y] update_tree: got QueuedEvents, raising"
                );
                events.raise();
            }
            Ok(None) => {
                self.feed.delivered(had_tree);
                crate::log_trace!(
                    crate::desktop::shell2::common::debug_server::LogCategory::Platform,
                    "[a11y] update_tree: adapter inactive (no events)"
                );
            }
            Err(e) => {
                // The adapter did not take it: the next update must be the
                // complete tree, not a patch against what it missed.
                self.feed.missed();
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", e));
                crate::log_warn!(
                    crate::desktop::shell2::common::debug_server::LogCategory::Platform,
                    "[a11y] ERROR: update_if_active panicked: {}",
                    msg
                );
            }
        }
    }

    /// Notify the adapter that the view's focus state changed.
    /// This must be called when the window gains or loses focus
    /// so VoiceOver knows which window is active.
    pub fn update_view_focus_state(&mut self, is_focused: bool) {
        crate::log_trace!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[a11y] update_view_focus_state: is_focused={}",
            is_focused
        );
        if let Some(events) = self.adapter.update_view_focus_state(is_focused) {
            crate::log_trace!(
                crate::desktop::shell2::common::debug_server::LogCategory::Platform,
                "[a11y] update_view_focus_state: raising events"
            );
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
        let (dom_id, node_id) =
            azul_layout::managers::a11y::decode_a11y_node_id(request.target_node);
        let action = azul_layout::managers::a11y::map_accesskit_action(request)?;
        Some((dom_id, node_id, action))
    }
}

/// `AXLanguage` on every accessibility element: the language VoiceOver picks
/// its voice by.
///
/// accesskit sets the language of a node only inside the attributed strings
/// of text ranges, which labels and buttons do not have; an element itself
/// answers `AXLanguage` with nothing, and VoiceOver reads German labels with
/// the default voice. accesskit's element class (`AccessKitNode`) answers
/// unknown attributes in `accessibilityAttributeValue:`; this wraps that
/// method once the class exists and answers `AXLanguage` with the app's
/// language (one per process: every window speaks the app's language), and
/// every other attribute with accesskit's own answer.
#[cfg(feature = "a11y")]
mod ax_language {
    use std::{
        ffi::c_void,
        sync::{
            atomic::{AtomicBool, AtomicPtr, Ordering},
            Mutex,
        },
    };

    use objc2::runtime::{AnyClass, AnyObject, Sel};
    use objc2_foundation::NSString;

    /// The app's language as a BCP 47 tag, from the window root of the last
    /// tree update; empty when unknown (`AXLanguage` then falls through).
    static LANGUAGE: Mutex<String> = Mutex::new(String::new());
    /// accesskit's own `accessibilityAttributeValue:`, which answers every
    /// attribute but `AXLanguage`. Null until [`install`] succeeded.
    static ORIGINAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
    /// Whether the wrapper is installed: the class only exists once accesskit
    /// made its first element, so [`install`] retries until then.
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    extern "C" {
        fn class_getInstanceMethod(cls: *const c_void, name: *const c_void) -> *mut c_void;
        fn method_getImplementation(method: *mut c_void) -> *mut c_void;
        fn method_setImplementation(method: *mut c_void, imp: *const c_void) -> *mut c_void;
    }

    pub(super) fn set(language: &str) {
        if let Ok(mut current) = LANGUAGE.lock() {
            if current.as_str() != language {
                *current = language.to_string();
            }
        }
    }

    pub(super) fn install() {
        if INSTALLED.load(Ordering::Acquire) {
            return;
        }
        let Some(class) = AnyClass::get(c"AccessKitNode") else {
            return;
        };
        let selector = objc2::sel!(accessibilityAttributeValue:);
        // SAFETY: the runtime functions take a live class and a selector;
        // the method is accesskit's own, defined on this very class, so
        // replacing its implementation changes no other class.
        unsafe {
            let method = class_getInstanceMethod(
                (class as *const AnyClass).cast(),
                std::mem::transmute::<Sel, *const c_void>(selector),
            );
            if method.is_null() {
                return;
            }
            ORIGINAL.store(method_getImplementation(method), Ordering::Release);
            method_setImplementation(method, attribute_value as *const c_void);
        }
        INSTALLED.store(true, Ordering::Release);
    }

    extern "C" fn attribute_value(
        this: *mut AnyObject,
        cmd: Sel,
        attribute: *mut AnyObject,
    ) -> *mut AnyObject {
        if !attribute.is_null() {
            let language = LANGUAGE.lock().map(|l| l.clone()).unwrap_or_default();
            if !language.is_empty() {
                let key = NSString::from_str("AXLanguage");
                // SAFETY: `attribute` is the NSString AppKit asks about.
                let asks_language: bool =
                    unsafe { objc2::msg_send![&*attribute, isEqualToString: &*key] };
                if asks_language {
                    return objc2::rc::Retained::autorelease_return(NSString::from_str(&language))
                        .cast();
                }
            }
        }
        let original = ORIGINAL.load(Ordering::Acquire);
        if original.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: `original` is the implementation this method had, with
        // this signature (`- (id)accessibilityAttributeValue:(NSString *)`).
        let original: extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut AnyObject =
            unsafe { std::mem::transmute(original) };
        original(this, cmd, attribute)
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
