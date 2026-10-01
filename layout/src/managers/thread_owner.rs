//! Which DOM node a background [`Thread`](crate::thread::Thread) belongs to,
//! and the workers of unmounted nodes on their way out.
//!
//! A worker that one of a node's LIFECYCLE callbacks started (`AfterMount`,
//! `NodeResized`, `Updated`: a video decoder, a camera / screen-capture /
//! microphone session, a map's tile fetcher) exists for that node - nothing
//! else reads what it writes back. The window used to keep every worker until
//! it finished on its own, and a capture or decode loop never does: a
//! `<video>` removed from the DOM kept decoding into state no node showed,
//! and only `run_all_threads` reaping FINISHED threads ever removed anything
//! (PR #476 ledger, engine backlog 4).
//!
//! The rule (one place, [`binds_threads_to_node`]): the dispatcher binds a
//! thread to the node whose lifecycle callback added it. Every rebuild remaps
//! the binding with the node (a node that moved keeps its worker) and
//! ORPHANS the thread when the node unmounted - also when the node lived in
//! the child DOM of a `VirtualView` whose host unmounted. An orphan is told to
//! stop (`ThreadSendMsg::TerminateThread`) at once, from the same unmount path
//! that drops the node's `VirtualView` / scroll / focus state
//! (`LayoutWindow::remap_node_ids`); `LayoutWindow::run_all_threads` then
//! delivers none of its write-backs (no node reads them, and they would only
//! request frames) and retires it with the ordinary `RemoveThread` as soon as
//! it has finished - which joins it without waiting - or, past
//! [`ORPHAN_GRACE`], after detaching it, so a worker stuck in a device read
//! cannot stall the UI thread in the thread destructor's join.
//!
//! Threads started anywhere else - a click handler, a timer, a `BeforeUnmount`
//! callback, a write-back - are the APP's and are never bound: a download
//! must not die because the button that started it re-rendered as a progress
//! bar.

use alloc::{collections::BTreeMap, vec::Vec};

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{ComponentEventFilter, EventFilter},
    task::ThreadId,
};

use super::NodeIdMap;

/// How long an orphaned worker may take to acknowledge `TerminateThread`
/// before it is detached instead of joined.
pub const ORPHAN_GRACE: core::time::Duration = core::time::Duration::from_secs(2);

/// Does a thread added by a callback answering `filter` belong to the
/// callback's node? Only for the callbacks a node's own life drives - mount,
/// resize, update - never for user input, timers or `BeforeUnmount` (a thread
/// started while a node goes away is meant to outlive it).
#[must_use]
pub const fn binds_threads_to_node(filter: &EventFilter) -> bool {
    matches!(
        filter,
        EventFilter::Component(
            ComponentEventFilter::AfterMount
                | ComponentEventFilter::NodeResized
                | ComponentEventFilter::Updated
        )
    )
}

/// When an orphan was told to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orphaned {
    /// `None` on targets without a clock (they run no threads either).
    #[cfg(feature = "std")]
    pub since: Option<std::time::Instant>,
}

impl Orphaned {
    fn now() -> Self {
        Self {
            #[cfg(feature = "std")]
            since: Some(std::time::Instant::now()),
        }
    }

    /// Has it had [`ORPHAN_GRACE`] to stop?
    #[cfg(feature = "std")]
    #[must_use]
    pub fn is_overdue(&self, now: std::time::Instant) -> bool {
        self.since
            .is_some_and(|t| now.saturating_duration_since(t) >= ORPHAN_GRACE)
    }
}

/// The owners of node-bound threads, and the orphans waiting to finish.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadOwnerManager {
    owners: BTreeMap<ThreadId, DomNodeId>,
    orphans: BTreeMap<ThreadId, Orphaned>,
}

impl ThreadOwnerManager {
    /// `thread_id` belongs to `owner` from now on.
    pub fn bind(&mut self, thread_id: ThreadId, owner: DomNodeId) {
        self.owners.insert(thread_id, owner);
    }

    /// The node `thread_id` belongs to (`None`: the app's, or orphaned).
    #[must_use]
    pub fn owner(&self, thread_id: &ThreadId) -> Option<DomNodeId> {
        self.owners.get(thread_id).copied()
    }

    /// Was `thread_id` told to stop because its node unmounted?
    #[must_use]
    pub fn orphaned(&self, thread_id: &ThreadId) -> Option<Orphaned> {
        self.orphans.get(thread_id).copied()
    }

    /// How many orphans have not been retired yet.
    #[must_use]
    pub fn orphan_count(&self) -> usize {
        self.orphans.len()
    }

    /// The thread left the window (`RemoveThread`): forget it.
    pub fn forget(&mut self, thread_id: &ThreadId) {
        self.owners.remove(thread_id);
        self.orphans.remove(thread_id);
    }

    /// Follow a rebuild of `dom`: owners move with their nodes; a thread
    /// whose node unmounted, or whose node lived in one of `dropped_doms` (the
    /// child DOMs of `VirtualView`s that unmounted), becomes an orphan.
    /// Returns the threads orphaned by THIS rebuild - the caller tells each to
    /// stop.
    pub fn remap_node_ids(
        &mut self,
        dom: DomId,
        map: &NodeIdMap,
        dropped_doms: &[DomId],
    ) -> Vec<ThreadId> {
        let mut orphaned = Vec::new();
        let owners = core::mem::take(&mut self.owners);
        for (thread_id, owner) in owners {
            if dropped_doms.contains(&owner.dom) {
                orphaned.push(thread_id);
                continue;
            }
            match map.resolve_dom_node_id(dom, owner) {
                Some(moved) => {
                    self.owners.insert(thread_id, moved);
                }
                None => orphaned.push(thread_id),
            }
        }
        for thread_id in &orphaned {
            self.orphans.insert(*thread_id, Orphaned::now());
        }
        orphaned
    }
}

/// An orphan's turn in `LayoutWindow::run_all_threads`: drops whatever it
/// wrote back (no node reads it, and delivering it would request frames for
/// nothing) and says whether to retire it NOW - it has finished, so the join
/// in the thread's destructor returns at once; or it is overdue, and its join
/// handle is detached first, so the destructor does not wait on a worker that
/// never answers `TerminateThread`.
#[cfg(all(feature = "std", feature = "text_layout"))]
#[must_use]
pub fn poll_orphan(
    orphaned: Orphaned,
    thread: &crate::thread::Thread,
    now: std::time::Instant,
) -> bool {
    use crate::thread::OptionThreadReceiveMsg;

    let Ok(mut inner) = thread.ptr.lock() else {
        return true;
    };
    while let OptionThreadReceiveMsg::Some(_) = inner.receiver_try_recv() {}
    if inner.is_finished() {
        return true;
    }
    if orphaned.is_overdue(now) {
        drop(inner.thread_handle.take());
        eprintln!(
            "[azul][thread] the worker of an unmounted node did not stop within {} ms of \
             TerminateThread; it was DETACHED (it ends with the process)",
            ORPHAN_GRACE.as_millis()
        );
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use azul_core::{dom::NodeId, styled_dom::NodeHierarchyItemId};

    use super::*;

    const ROOT: DomId = DomId::ROOT_ID;

    fn node(dom: DomId, n: usize) -> DomNodeId {
        DomNodeId {
            dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
        }
    }

    #[test]
    fn only_a_nodes_own_lifecycle_callbacks_bind_its_threads() {
        use azul_core::events::{HoverEventFilter, WindowEventFilter};
        assert!(binds_threads_to_node(&EventFilter::Component(
            ComponentEventFilter::AfterMount
        )));
        assert!(binds_threads_to_node(&EventFilter::Component(
            ComponentEventFilter::NodeResized
        )));
        assert!(binds_threads_to_node(&EventFilter::Component(
            ComponentEventFilter::Updated
        )));
        // A thread started while the node goes away is meant to outlive it.
        assert!(!binds_threads_to_node(&EventFilter::Component(
            ComponentEventFilter::BeforeUnmount
        )));
        // User input starts the APP's threads (a download outlives its button).
        assert!(!binds_threads_to_node(&EventFilter::Hover(
            HoverEventFilter::MouseUp
        )));
        assert!(!binds_threads_to_node(&EventFilter::Window(
            WindowEventFilter::MouseUp
        )));
    }

    #[test]
    fn a_thread_whose_node_unmounted_is_orphaned_once() {
        let mut m = ThreadOwnerManager::default();
        let t = ThreadId::unique();
        m.bind(t, node(ROOT, 3));
        // Node 3 is gone, node 1 stayed.
        let map = NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(1))]);
        assert_eq!(m.remap_node_ids(ROOT, &map, &[]), vec![t]);
        assert_eq!(m.owner(&t), None);
        assert!(m.orphaned(&t).is_some());
        // A second rebuild does not report it again.
        assert!(m.remap_node_ids(ROOT, &map, &[]).is_empty());
        m.forget(&t);
        assert_eq!(m.orphan_count(), 0);
    }

    #[test]
    fn a_thread_follows_its_node_when_the_node_moves() {
        let mut m = ThreadOwnerManager::default();
        let t = ThreadId::unique();
        m.bind(t, node(ROOT, 3));
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(5))]);
        assert!(m.remap_node_ids(ROOT, &map, &[]).is_empty());
        assert_eq!(m.owner(&t), Some(node(ROOT, 5)));
        assert!(m.orphaned(&t).is_none());
    }

    #[test]
    fn a_rebuild_of_another_dom_leaves_the_thread_alone() {
        let mut m = ThreadOwnerManager::default();
        let t = ThreadId::unique();
        let child = DomId { inner: 7 };
        m.bind(t, node(child, 2));
        assert!(m
            .remap_node_ids(ROOT, &NodeIdMap::default(), &[])
            .is_empty());
        assert_eq!(m.owner(&t), Some(node(child, 2)));
    }

    #[test]
    fn a_thread_in_the_child_dom_of_an_unmounted_virtual_view_is_orphaned() {
        let mut m = ThreadOwnerManager::default();
        let t = ThreadId::unique();
        let child = DomId { inner: 7 };
        m.bind(t, node(child, 2));
        let map = NodeIdMap::from_pairs([(NodeId::new(0), NodeId::new(0))]);
        assert_eq!(m.remap_node_ids(ROOT, &map, &[child]), vec![t]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn an_orphan_is_overdue_after_the_grace_period() {
        let start = std::time::Instant::now();
        let o = Orphaned { since: Some(start) };
        assert!(!o.is_overdue(start));
        assert!(o.is_overdue(start + ORPHAN_GRACE));
    }
}
