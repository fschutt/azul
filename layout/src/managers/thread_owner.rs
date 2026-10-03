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
//!
//! TIMERS follow the same rule (THREADS8): a timer one of a node's lifecycle
//! callbacks started - the map's 250 ms tile sweep - belongs to the node.
//! When the node unmounts, `LayoutWindow::remap_node_ids` drops it from the
//! window's timers and puts it on [`ThreadOwnerManager::take_timers_to_stop`],
//! from which the shell stops its platform timer. A closing window stops ALL
//! its workers together ([`stop_all`], from `LayoutWindow`'s `Drop`).

use alloc::{collections::BTreeMap, vec::Vec};

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{ComponentEventFilter, EventFilter},
    task::{ThreadId, TimerId},
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

/// The owners of node-bound threads, and the orphans waiting to finish; the
/// owners of node-bound timers, and the timers of unmounted nodes waiting for
/// the shell to stop them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadOwnerManager {
    owners: BTreeMap<ThreadId, DomNodeId>,
    orphans: BTreeMap<ThreadId, Orphaned>,
    timer_owners: BTreeMap<TimerId, DomNodeId>,
    /// Timers whose node unmounted. The window drops them from its timer map
    /// at the unmount (they never fire again); the shell takes this list
    /// ([`Self::take_timers_to_stop`]) to stop their OS timers as well.
    timers_to_stop: Vec<TimerId>,
}

/// Where `owner` is after a rebuild of `dom`: the node it became, or `None`
/// when it unmounted - also when it lived in one of `dropped_doms`, the child
/// DOMs of `VirtualView`s that unmounted. A node of another DOM stays put.
fn follow(
    owner: DomNodeId,
    dom: DomId,
    map: &NodeIdMap,
    dropped_doms: &[DomId],
) -> Option<DomNodeId> {
    if dropped_doms.contains(&owner.dom) {
        return None;
    }
    map.resolve_dom_node_id(dom, owner)
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

    /// `timer_id` belongs to `owner` from now on: one of `owner`'s lifecycle
    /// callbacks started it, and it stops when `owner` unmounts.
    pub fn bind_timer(&mut self, timer_id: TimerId, owner: DomNodeId) {
        self.timer_owners.insert(timer_id, owner);
    }

    /// The node `timer_id` belongs to (`None`: the app's, or stopped).
    #[must_use]
    pub fn timer_owner(&self, timer_id: &TimerId) -> Option<DomNodeId> {
        self.timer_owners.get(timer_id).copied()
    }

    /// The timer left the window (`RemoveTimer`, or its id was started anew):
    /// its old node no longer speaks for it.
    pub fn forget_timer(&mut self, timer_id: &TimerId) {
        self.timer_owners.remove(timer_id);
    }

    /// The timers whose node unmounted since the last call: the shell stops
    /// each (`PlatformWindow::stop_timer`). Handed out once.
    #[must_use]
    pub fn take_timers_to_stop(&mut self) -> Vec<TimerId> {
        core::mem::take(&mut self.timers_to_stop)
    }

    /// Follow a rebuild of `dom`: owners move with their nodes; a thread
    /// whose node unmounted, or whose node lived in one of `dropped_doms` (the
    /// child DOMs of `VirtualView`s that unmounted), becomes an orphan.
    /// Returns the threads orphaned by THIS rebuild - the caller tells each to
    /// stop. A TIMER of such a node joins [`Self::take_timers_to_stop`] (the
    /// caller drops it from the window's timers too).
    pub fn remap_node_ids(
        &mut self,
        dom: DomId,
        map: &NodeIdMap,
        dropped_doms: &[DomId],
    ) -> Vec<ThreadId> {
        let mut orphaned = Vec::new();
        let owners = core::mem::take(&mut self.owners);
        for (thread_id, owner) in owners {
            match follow(owner, dom, map, dropped_doms) {
                Some(moved) => {
                    self.owners.insert(thread_id, moved);
                }
                None => orphaned.push(thread_id),
            }
        }
        for thread_id in &orphaned {
            self.orphans.insert(*thread_id, Orphaned::now());
        }
        let timer_owners = core::mem::take(&mut self.timer_owners);
        for (timer_id, owner) in timer_owners {
            match follow(owner, dom, map, dropped_doms) {
                Some(moved) => {
                    self.timer_owners.insert(timer_id, moved);
                }
                None => self.timers_to_stop.push(timer_id),
            }
        }
        orphaned
    }

    /// The timers of unmounted nodes the shell has not taken yet - for the
    /// window to drop from its own timer map at the unmount (idempotent).
    #[must_use]
    pub fn timers_to_stop(&self) -> &[TimerId] {
        &self.timers_to_stop
    }
}

/// An orphan's turn in `LayoutWindow::run_all_threads` (and in [`stop_all`]):
/// drops whatever it wrote back (no node reads it, and delivering it would
/// request frames for nothing) and says whether to retire it NOW - it has
/// finished, so the join in the thread's destructor returns at once; or it is
/// overdue, and its join handle is detached first, so the destructor does not
/// wait on a worker that never answers `TerminateThread`.
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
            "[azul][thread] a worker whose node unmounted (or whose window closed) did not stop \
             within {} ms of TerminateThread; it was DETACHED (it ends with the process)",
            ORPHAN_GRACE.as_millis()
        );
        return true;
    }
    false
}

/// Stop EVERY worker in `threads` - the window they belong to is going away.
///
/// All of them are told `TerminateThread` first and then polled TOGETHER
/// against one shared [`ORPHAN_GRACE`] (a worker still running after it is
/// detached, [`poll_orphan`]); then the map is emptied, and each `Thread`
/// destructor finds its worker finished (or its handle gone) and returns at
/// once. A window closes in the time its SLOWEST worker takes to stop, at
/// most one grace period - dropping the map entry by entry made each worker
/// wait for the previous one, the sum of them all, N x 2 s for workers stuck
/// in a device read.
#[cfg(all(feature = "std", feature = "text_layout"))]
pub fn stop_all(threads: &mut BTreeMap<ThreadId, crate::thread::Thread>) {
    if threads.is_empty() {
        return;
    }
    for thread in threads.values() {
        let _ = thread.send_message(azul_core::task::ThreadSendMsg::TerminateThread);
    }
    let told = Orphaned::now();
    let mut running: Vec<&crate::thread::Thread> = threads.values().collect();
    loop {
        let now = std::time::Instant::now();
        running.retain(|thread| !poll_orphan(told, thread, now));
        if running.is_empty() {
            break;
        }
        std::thread::sleep(core::time::Duration::from_millis(2));
    }
    threads.clear();
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

    // --- Timers: the same rule as threads (THREADS8) ---------------------
    //
    // The map's tile sweep is a 250 ms timer its `AfterMount` starts and
    // nothing ever stops: a map that left the DOM woke the app four times a
    // second for ever, holding its tile cache, and a remount added a second
    // sweep. A timer one of a node's lifecycle callbacks started belongs to
    // the node like its threads do.

    #[test]
    fn a_timer_whose_node_unmounted_is_stopped_once() {
        use azul_core::task::TimerId;
        let mut m = ThreadOwnerManager::default();
        let t = TimerId { id: 41 };
        m.bind_timer(t, node(ROOT, 3));
        // Node 3 is gone, node 1 stayed.
        let map = NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(1))]);
        assert!(
            m.remap_node_ids(ROOT, &map, &[]).is_empty(),
            "no thread was orphaned"
        );
        assert_eq!(m.timer_owner(&t), None);
        assert_eq!(m.take_timers_to_stop(), vec![t]);
        assert!(m.take_timers_to_stop().is_empty(), "handed out once");
        let _ = m.remap_node_ids(ROOT, &map, &[]);
        assert!(
            m.take_timers_to_stop().is_empty(),
            "a second rebuild does not stop it again"
        );
    }

    #[test]
    fn a_timer_follows_its_node_when_the_node_moves() {
        use azul_core::task::TimerId;
        let mut m = ThreadOwnerManager::default();
        let t = TimerId { id: 41 };
        m.bind_timer(t, node(ROOT, 3));
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(5))]);
        let _ = m.remap_node_ids(ROOT, &map, &[]);
        assert_eq!(m.timer_owner(&t), Some(node(ROOT, 5)));
        assert!(m.take_timers_to_stop().is_empty());
    }

    #[test]
    fn a_timer_in_the_child_dom_of_an_unmounted_virtual_view_is_stopped() {
        use azul_core::task::TimerId;
        let mut m = ThreadOwnerManager::default();
        let t = TimerId { id: 41 };
        let child = DomId { inner: 7 };
        m.bind_timer(t, node(child, 2));
        let map = NodeIdMap::from_pairs([(NodeId::new(0), NodeId::new(0))]);
        let _ = m.remap_node_ids(ROOT, &map, &[child]);
        assert_eq!(m.take_timers_to_stop(), vec![t]);
    }

    #[test]
    fn a_timer_that_was_removed_is_never_stopped_for_its_old_node() {
        use azul_core::task::TimerId;
        let mut m = ThreadOwnerManager::default();
        let t = TimerId { id: 41 };
        m.bind_timer(t, node(ROOT, 3));
        // The timer ended (or the app removed it, or re-added the id from a
        // click): its old node no longer speaks for it.
        m.forget_timer(&t);
        let _ = m.remap_node_ids(ROOT, &NodeIdMap::default(), &[]);
        assert!(m.take_timers_to_stop().is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn an_orphan_is_overdue_after_the_grace_period() {
        let start = std::time::Instant::now();
        let o = Orphaned { since: Some(start) };
        assert!(!o.is_overdue(start));
        assert!(o.is_overdue(start + ORPHAN_GRACE));
    }

    // --- A window that closes stops its workers together -----------------
    //
    // THREADS8. Dropping a window dropped its `threads` map one entry at a
    // time, and each `Thread`'s destructor told ITS worker to stop and then
    // waited for it (up to 2 s) before the next worker heard anything: the
    // window took the SUM of its workers' stop times to close, on the UI
    // thread - and N x 2 s when the workers were stuck in a device read.
    #[cfg(all(feature = "std", feature = "text_layout"))]
    mod a_window_that_closes {
        use std::time::{Duration, Instant};

        use azul_core::{
            refany::RefAny,
            task::{OptionThreadSendMsg, ThreadId, ThreadReceiver, ThreadSendMsg},
        };
        use rust_fontconfig::FcFontCache;

        use crate::{
            managers::thread_owner::ORPHAN_GRACE,
            thread::{Thread, ThreadCallback, ThreadCallbackType, ThreadSender},
            window::LayoutWindow,
        };

        /// How long a worker takes to stop once it is told (a capture device
        /// closing).
        const STOP_TAKES: Duration = Duration::from_millis(300);

        extern "C" fn slow_to_stop(_init: RefAny, _sender: ThreadSender, mut recv: ThreadReceiver) {
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(10) {
                if let OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) = recv.recv() {
                    std::thread::sleep(STOP_TAKES);
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        /// Never answers `TerminateThread` (stuck in a device read). Bounded,
        /// so the test process does not keep it for ever.
        extern "C" fn never_stops(_init: RefAny, _sender: ThreadSender, _recv: ThreadReceiver) {
            std::thread::sleep(Duration::from_secs(8));
        }

        fn window_with(workers: usize, worker: ThreadCallbackType) -> LayoutWindow {
            let mut window = LayoutWindow::new(FcFontCache::default()).expect("a window");
            for _ in 0..workers {
                window.threads.insert(
                    ThreadId::unique(),
                    Thread::create(
                        RefAny::new(()),
                        RefAny::new(()),
                        ThreadCallback::new(worker),
                    ),
                );
            }
            window
        }

        #[test]
        fn tells_every_worker_to_stop_before_it_waits_for_any_of_them() {
            let window = window_with(3, slow_to_stop);
            let start = Instant::now();
            drop(window);
            let took = start.elapsed();
            assert!(
                took < STOP_TAKES * 2,
                "three workers that each take {} ms to stop must stop TOGETHER: closing took {} \
                 ms (one after the other is ~{} ms)",
                STOP_TAKES.as_millis(),
                took.as_millis(),
                (STOP_TAKES * 3).as_millis()
            );
        }

        #[test]
        fn waits_one_grace_period_for_workers_that_never_stop_not_one_each() {
            let window = window_with(3, never_stops);
            let start = Instant::now();
            drop(window);
            let took = start.elapsed();
            assert!(
                took < ORPHAN_GRACE + ORPHAN_GRACE / 2,
                "three stuck workers must cost the closing window ONE grace period ({} ms), then \
                 be detached: closing took {} ms",
                ORPHAN_GRACE.as_millis(),
                took.as_millis()
            );
        }
    }
}
