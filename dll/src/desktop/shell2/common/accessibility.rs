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
    tree: Mutex<Option<CompleteTree>>,
    phase: std::sync::atomic::AtomicU8,
}

/// The complete tree an adapter holds once it took every update: the nodes by id, the tree's
/// info and the focus.
///
/// Kept as a map, not as one big `TreeUpdate`: the layout side publishes a PATCH per frame (only
/// what changed, nothing when nothing did), and merging a patch into a list cost a scan of the
/// whole tree per node plus a reachability walk over all of it - per frame, even with no screen
/// reader running. A patch now costs what it carries and what it removes; the whole tree is
/// assembled only when an adapter needs one.
#[cfg(feature = "a11y")]
#[derive(Debug, Clone)]
struct CompleteTree {
    tree: accesskit::Tree,
    tree_id: accesskit::TreeId,
    focus: accesskit::NodeId,
    nodes: std::collections::HashMap<accesskit::NodeId, accesskit::Node>,
}

#[cfg(feature = "a11y")]
impl CompleteTree {
    /// The whole tree as one full update (node order does not matter to accesskit).
    fn to_update(&self) -> accesskit::TreeUpdate {
        accesskit::TreeUpdate {
            nodes: self
                .nodes
                .iter()
                .map(|(id, node)| (*id, node.clone()))
                .collect(),
            tree: Some(self.tree.clone()),
            focus: self.focus,
            tree_id: self.tree_id,
        }
    }
}

#[cfg(feature = "a11y")]
impl A11yTreeFeed {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// For `ActivationHandler::request_initial_tree`: the complete tree, once one was built.
    pub fn initial_tree(&self) -> Option<accesskit::TreeUpdate> {
        let tree = self
            .tree
            .try_lock()
            .ok()
            .and_then(|tree| tree.as_ref().map(CompleteTree::to_update));
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
                tree.as_ref().map(CompleteTree::to_update)
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

    /// The adapter did NOT take the update [`Self::next_update`] handed out (its lock was busy, or
    /// it panicked): it holds an older tree than the feed. With full trees every frame the next one
    /// healed that; with patches the next patch would be applied to a tree that misses this one -
    /// so the next update it gets is the complete tree.
    pub fn missed(&self) {
        if self.phase() == ACTIVE {
            self.set_phase(AWAITING_TREE);
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
/// nodes overwrite by id, and the children an updated node no longer lists - unless another updated
/// node lists them - leave with their subtrees. The cost is the update's and the removed nodes',
/// never the whole tree's.
#[cfg(feature = "a11y")]
fn merge_into(tree: &mut Option<CompleteTree>, update: &accesskit::TreeUpdate) {
    if let Some(info) = &update.tree {
        *tree = Some(CompleteTree {
            tree: info.clone(),
            tree_id: update.tree_id,
            focus: update.focus,
            nodes: update
                .nodes
                .iter()
                .map(|(id, node)| (*id, node.clone()))
                .collect(),
        });
        return;
    }
    let Some(full) = tree.as_mut() else {
        return;
    };
    let listed: std::collections::HashSet<accesskit::NodeId> = update
        .nodes
        .iter()
        .flat_map(|(_, node)| node.children().iter().copied())
        .collect();
    // The children the updated nodes drop, read from their OLD versions.
    let mut dropped: Vec<accesskit::NodeId> = update
        .nodes
        .iter()
        .filter_map(|(id, _)| full.nodes.get(id))
        .flat_map(|old| old.children().iter().copied())
        .filter(|child| !listed.contains(child))
        .collect();
    for (id, node) in &update.nodes {
        full.nodes.insert(*id, node.clone());
    }
    let root = full.tree.root;
    while let Some(id) = dropped.pop() {
        if id == root || listed.contains(&id) {
            continue;
        }
        if let Some(node) = full.nodes.remove(&id) {
            dropped.extend(node.children().iter().copied());
        }
    }
    full.focus = update.focus;
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

        /// A patch that drops a child drops its whole subtree from the complete tree, and keeps a
        /// child another updated node now lists (a move).
        #[test]
        fn a_patch_removes_a_dropped_subtree_and_keeps_a_moved_child() {
            let feed = A11yTreeFeed::new();
            // 0 > [1 > [3 > [4]], 2]
            feed.next_update(TreeUpdate {
                nodes: vec![
                    (NodeId(0), node(&[1, 2])),
                    (NodeId(1), node(&[3])),
                    (NodeId(2), node(&[])),
                    (NodeId(3), node(&[4])),
                    (NodeId(4), node(&[])),
                ],
                tree: Some(Tree::new(NodeId(0))),
                tree_id: TreeId::ROOT,
                focus: NodeId(0),
            });
            // 1 leaves the root (and 3 > 4 with it), but 4 moves under 2.
            feed.next_update(incremental(
                vec![(NodeId(0), node(&[2])), (NodeId(2), node(&[4]))],
                2,
            ));
            let tree = feed.initial_tree().expect("the complete tree");
            assert_eq!(ids(&tree), vec![0, 2, 4]);
            assert_eq!(tree.focus, NodeId(2));
        }

        /// An adapter that did not take an update (busy lock, a caught panic) holds an older tree:
        /// the next update it gets is the complete tree, not a patch against what it missed.
        #[test]
        fn an_adapter_that_missed_an_update_gets_the_complete_tree_next() {
            let feed = A11yTreeFeed::new();
            feed.next_update(full(&[1, 2]));
            assert!(feed.initial_tree().is_some(), "activation hands over the tree");
            let missed = feed
                .next_update(incremental(vec![(NodeId(0), node(&[1]))], 1))
                .expect("an active adapter is handed the patch");
            assert!(missed.tree.is_none());
            feed.missed();

            let next = feed
                .next_update(incremental(vec![(NodeId(1), node(&[]))], 1))
                .expect("the adapter still needs updates");
            assert!(
                next.tree.is_some(),
                "after a missed patch the adapter must get the complete tree"
            );
            assert_eq!(ids(&next), vec![0, 1]);
            feed.delivered(true);
            let after = feed
                .next_update(incremental(vec![(NodeId(1), node(&[]))], 1))
                .expect("active again");
            assert!(after.tree.is_none(), "back to patches once it holds the tree");
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
