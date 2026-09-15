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
//! * Android — `AccessibilityNodeProvider::performAction` runs on the Java UI thread while
//!   `android_main` runs on its own native thread.
//! * iOS — UIKit calls `accessibilityActivate` on the main thread, which is also the
//!   `CADisplayLink` thread, so it would be safe unsynchronised; it shares the type anyway rather
//!   than growing a second one.
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

#[cfg(feature = "a11y")]
const INACTIVE: u8 = 0;
#[cfg(feature = "a11y")]
const AWAITING_TREE: u8 = 1;
#[cfg(feature = "a11y")]
const ACTIVE: u8 = 2;

/// Decides what an `accesskit` adapter may be handed.
///
/// An adapter that activated without a tree (its `request_initial_tree` returned `None`) sits in a
/// placeholder state, and the next update it gets must carry the whole tree: an incremental one
/// panics inside `accesskit_consumer`, on Windows from within the window procedure, where the
/// panic aborts the process. The feed keeps the complete current tree, merging incremental updates
/// the way the consumer does, and hands that tree over whenever the adapter still needs one.
///
/// Shared between the shell and the adapter's activation handler, which can run on another thread.
#[cfg(feature = "a11y")]
#[derive(Debug, Default)]
pub struct A11yTreeFeed {
    tree: Mutex<Option<accesskit::TreeUpdate>>,
    phase: std::sync::atomic::AtomicU8,
}

#[cfg(feature = "a11y")]
impl A11yTreeFeed {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// For `ActivationHandler::request_initial_tree`: the complete tree, once one was built.
    pub fn initial_tree(&self) -> Option<accesskit::TreeUpdate> {
        let tree = self.tree.try_lock().ok().and_then(|tree| tree.clone());
        self.set_phase(if tree.is_some() { ACTIVE } else { AWAITING_TREE });
        tree
    }

    /// For an activation handler that answers `None` on purpose (macOS wants the placeholder
    /// transition for its focus notifications): the adapter now waits for a complete tree.
    pub fn placeholder_requested(&self) {
        self.set_phase(AWAITING_TREE);
    }

    /// Records `update` and returns what to pass to `update_if_active`, or `None` when the adapter
    /// cannot use anything yet. Call [`Self::delivered`] once the adapter accepted it.
    pub fn next_update(&self, update: accesskit::TreeUpdate) -> Option<accesskit::TreeUpdate> {
        let full = {
            let Ok(mut tree) = self.tree.lock() else {
                return None;
            };
            merge_into(&mut tree, &update);
            if update.tree.is_none() && self.phase() == AWAITING_TREE {
                tree.clone()
            } else {
                None
            }
        };
        match self.phase() {
            ACTIVE => Some(update),
            AWAITING_TREE if update.tree.is_some() => Some(update),
            AWAITING_TREE => full,
            _ => None,
        }
    }

    /// The adapter accepted an update from [`Self::next_update`].
    pub fn delivered(&self, update_had_tree: bool) {
        if update_had_tree && self.phase() == AWAITING_TREE {
            self.set_phase(ACTIVE);
        }
    }

    fn phase(&self) -> u8 {
        self.phase.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn set_phase(&self, phase: u8) {
        self.phase.store(phase, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Applies `update` to the complete tree the way `accesskit_consumer` does: a tree replaces it,
/// nodes overwrite by id, and nodes no longer reachable from the root are dropped.
#[cfg(feature = "a11y")]
fn merge_into(tree: &mut Option<accesskit::TreeUpdate>, update: &accesskit::TreeUpdate) {
    if update.tree.is_some() {
        *tree = Some(update.clone());
        return;
    }
    let Some(full) = tree.as_mut() else {
        return;
    };
    for (id, node) in &update.nodes {
        match full.nodes.iter_mut().find(|(existing, _)| existing == id) {
            Some(slot) => slot.1 = node.clone(),
            None => full.nodes.push((*id, node.clone())),
        }
    }
    full.focus = update.focus;
    let Some(root) = full.tree.as_ref().map(|t| t.root) else {
        return;
    };
    let children: std::collections::HashMap<accesskit::NodeId, Vec<accesskit::NodeId>> = full
        .nodes
        .iter()
        .map(|(id, node)| (*id, node.children().to_vec()))
        .collect();
    let mut reachable = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if reachable.insert(id) {
            stack.extend(children.get(&id).into_iter().flatten().copied());
        }
    }
    full.nodes.retain(|(id, _)| reachable.contains(id));
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

    #[cfg(feature = "a11y")]
    mod feed {
        use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};

        use super::super::A11yTreeFeed;

        fn node(children: &[u64]) -> Node {
            let mut node = Node::new(Role::GenericContainer);
            node.set_children(children.iter().map(|c| NodeId(*c)).collect::<Vec<_>>());
            node
        }

        fn full(children: &[u64]) -> TreeUpdate {
            let mut nodes = vec![(NodeId(0), node(children))];
            nodes.extend(children.iter().map(|c| (NodeId(*c), node(&[]))));
            TreeUpdate {
                nodes,
                tree: Some(Tree::new(NodeId(0))),
                tree_id: TreeId::ROOT,
                focus: NodeId(0),
            }
        }

        fn incremental(nodes: Vec<(NodeId, Node)>, focus: u64) -> TreeUpdate {
            TreeUpdate {
                nodes,
                tree: None,
                tree_id: TreeId::ROOT,
                focus: NodeId(focus),
            }
        }

        fn ids(update: &TreeUpdate) -> Vec<u64> {
            let mut ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
            ids.sort_unstable();
            ids
        }

        #[test]
        fn an_inactive_adapter_gets_nothing() {
            let feed = A11yTreeFeed::new();
            assert!(feed.next_update(full(&[1])).is_none());
            assert!(feed
                .next_update(incremental(vec![(NodeId(1), node(&[]))], 1))
                .is_none());
        }

        #[test]
        fn activation_hands_over_the_merged_tree() {
            let feed = A11yTreeFeed::new();
            assert!(feed.initial_tree().is_none());
            feed.next_update(full(&[1, 2]));
            feed.delivered(true);
            let tree = feed.initial_tree().expect("a complete tree after the first full update");
            assert_eq!(ids(&tree), vec![0, 1, 2]);
            assert!(tree.tree.is_some());
        }

        #[test]
        fn a_waiting_adapter_gets_the_complete_tree_instead_of_an_increment() {
            let feed = A11yTreeFeed::new();
            feed.next_update(full(&[1, 2]));
            feed.placeholder_requested();
            let removal = incremental(
                vec![
                    (NodeId(0), node(&[2])),
                    (NodeId(2), node(&[3])),
                    (NodeId(3), node(&[])),
                ],
                3,
            );
            let handed = feed.next_update(removal).expect("the waiting adapter needs a tree");
            assert!(handed.tree.is_some());
            assert_eq!(ids(&handed), vec![0, 2, 3]);
            assert_eq!(handed.focus, NodeId(3));
            feed.delivered(true);
            let next = feed
                .next_update(incremental(vec![(NodeId(3), node(&[]))], 3))
                .expect("an active adapter takes increments");
            assert!(next.tree.is_none());
        }
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
