//! Cross-platform accessibility action ingress.
//!
//! The four desktop backends each own an `accesskit` adapter that decodes an
//! assistive-technology request into `(DomId, NodeId, AccessibilityAction)` and
//! parks it until the frame loop polls it (see `windows/accessibility.rs`,
//! `macos/accessibility.rs`, `linux/x11/accessibility.rs`). Headless, iOS and
//! Android have no `accesskit` adapter — accesskit ships no UIKit or Android
//! backend — but they still need the SAME thing: a place for an out-of-band
//! action to land, drained by the frame loop on the thread that owns the
//! `LayoutWindow`.
//!
//! [`A11yActionQueue`] is that place. It is deliberately the same shape as the
//! desktop adapters' `poll_action()` so [`super::event::PlatformWindow::
//! dispatch_accessibility_actions`] can drive all seven backends identically.
//!
//! The queue is `Arc<Mutex<..>>` because the producer is NOT always the loop
//! thread:
//!
//! * Android — `AccessibilityNodeProvider::performAction` runs on the Java UI
//!   thread while `android_main` runs on its own native thread.
//! * iOS — UIKit calls `accessibilityActivate` on the main thread, which is
//!   also the `CADisplayLink` thread, so it would be safe unsynchronised; it
//!   shares the type anyway rather than growing a second one.
//! * headless — a test/host may inject from any thread.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use azul_core::dom::{AccessibilityAction, DomId, NodeId};

/// A decoded accessibility action targeting one DOM node.
pub type PendingA11yAction = (DomId, NodeId, AccessibilityAction);

/// Thread-safe FIFO of accessibility actions waiting to be applied.
///
/// Cloning shares the queue (it is an `Arc` inside), so a platform callback can
/// hold a clone and push into the same queue the window drains.
#[derive(Debug, Clone, Default)]
pub struct A11yActionQueue {
    inner: Arc<Mutex<VecDeque<PendingA11yAction>>>,
}

impl A11yActionQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Queue one action. Never blocks the producer for long: the only other
    /// holder of the lock is the frame loop's `drain()`, which is O(n) memcpy.
    ///
    /// A poisoned mutex is ignored rather than panicking — an assistive
    /// technology's request must never take the app down.
    pub fn push(&self, dom_id: DomId, node_id: NodeId, action: AccessibilityAction) {
        if let Ok(mut q) = self.inner.lock() {
            q.push_back((dom_id, node_id, action));
        }
    }

    /// Pop the oldest queued action, mirroring the desktop adapters'
    /// `poll_action()` so the shared dispatch loop reads the same on every
    /// backend.
    #[must_use]
    pub fn poll_action(&self) -> Option<PendingA11yAction> {
        self.inner.lock().ok().and_then(|mut q| q.pop_front())
    }

    /// Take everything queued so far.
    #[must_use]
    pub fn drain(&self) -> Vec<PendingA11yAction> {
        self.inner
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().map_or(true, |q| q.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use azul_core::dom::{AccessibilityAction, DomId, NodeId};

    use super::A11yActionQueue;

    #[test]
    fn queue_is_fifo_and_shared_between_clones() {
        let a = A11yActionQueue::new();
        let b = a.clone();
        assert!(a.is_empty());

        b.push(DomId::ROOT_ID, NodeId::new(3), AccessibilityAction::Focus);
        b.push(DomId::ROOT_ID, NodeId::new(4), AccessibilityAction::Default);

        assert!(!a.is_empty());
        assert_eq!(
            a.poll_action(),
            Some((DomId::ROOT_ID, NodeId::new(3), AccessibilityAction::Focus))
        );
        assert_eq!(
            a.poll_action(),
            Some((DomId::ROOT_ID, NodeId::new(4), AccessibilityAction::Default))
        );
        assert_eq!(a.poll_action(), None);
    }

    #[test]
    fn drain_takes_everything_at_once() {
        let q = A11yActionQueue::new();
        q.push(DomId::ROOT_ID, NodeId::new(1), AccessibilityAction::Expand);
        q.push(
            DomId::ROOT_ID,
            NodeId::new(2),
            AccessibilityAction::Collapse,
        );
        assert_eq!(q.drain().len(), 2);
        assert!(q.drain().is_empty());
    }
}
