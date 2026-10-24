//! A window that closes while another stays open hands every thread to that window and
//! waits for none of them. The app's thread goes on there, a thread the app stopped
//! still has its last write-back delivered there, and a worker of one of the closing
//! window's nodes is told to stop and is retired there as an orphan. So a worker stuck
//! in a blocking call never freezes the windows that stay open.

use std::time::{Duration, Instant};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    refany::RefAny,
    styled_dom::NodeHierarchyItemId,
    task::{OptionThreadSendMsg, ThreadId, ThreadReceiver, ThreadSendMsg},
};
use azul_layout::{
    thread::{Thread, ThreadCallbackType, ThreadSender},
    window::LayoutWindow,
};
use rust_fontconfig::FcFontCache;

/// How long a worker takes to stop once it is told: a capture device closing.
const STOP_TAKES: Duration = Duration::from_millis(600);

/// Runs until it is told to stop, then takes [`STOP_TAKES`]. Bounded, so a test that
/// never tells it does not keep it for ever.
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

fn worker() -> Thread {
    Thread::create(
        RefAny::new(()),
        RefAny::new(()),
        slow_to_stop as ThreadCallbackType,
    )
}

fn node(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

#[test]
fn a_closing_window_never_waits_for_its_workers() {
    let mut closing = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut heir = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let app = ThreadId::unique();
    let stopped = ThreadId::unique();
    let of_a_node = ThreadId::unique();
    closing.add_thread(app, worker());
    closing.add_thread(stopped, worker());
    closing.add_thread_owned_by(of_a_node, worker(), Some(node(1)));
    assert!(closing.stop_thread(&stopped));

    let start = Instant::now();
    let handed = closing.take_threads_for_heir();
    drop(closing);
    let took = start.elapsed();
    assert!(
        took < STOP_TAKES / 2,
        "closing the window must not wait for its workers ({took:?}; one takes {STOP_TAKES:?} \
         to stop)"
    );
    assert_eq!(
        handed.len(),
        3,
        "every thread goes to the window that stays open"
    );

    assert!(heir.adopt_handed_over_threads(handed));
    assert!(heir.threads.contains_key(&app));
    assert!(heir.thread_owners.orphaned(&app).is_none());
    assert!(
        heir.thread_owners.stopping(&app).is_none(),
        "the app's thread goes on"
    );
    assert!(
        heir.thread_owners.stopping(&stopped).is_some(),
        "the thread the app stopped still has its last write-backs delivered"
    );
    assert!(
        heir.thread_owners.orphaned(&of_a_node).is_some(),
        "the worker of the closed window's node is an orphan in the heir"
    );

    // The node's worker was told to stop when its window closed: it returns after
    // STOP_TAKES, not after the 10 s it would run untold.
    let deadline = Instant::now() + Duration::from_secs(5);
    let finished = |w: &LayoutWindow, id: &ThreadId| {
        w.threads
            .get(id)
            .is_some_and(|t| t.ptr.lock().is_ok_and(|inner| inner.is_finished()))
    };
    while !finished(&heir, &of_a_node) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        finished(&heir, &of_a_node),
        "the closed window's node worker was told to stop"
    );
}
