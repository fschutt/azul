//! A window that closes while another stays open hands the app's background threads to it,
//! so a Send / Receive started from a main window goes on in the compose window left open. The
//! threads of the closing window's own nodes (a camera, a video decoder) stay with it and stop
//! with it.

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    refany::RefAny,
    styled_dom::NodeHierarchyItemId,
    task::{ThreadId, ThreadReceiver},
};
use azul_layout::{
    thread::{Thread, ThreadCallbackType, ThreadSender},
    window::LayoutWindow,
};
use rust_fontconfig::FcFontCache;

/// A worker that is done at once: the test is about who owns a thread, not about what it does.
extern "C" fn finished_worker(_init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {}

fn thread() -> Thread {
    Thread::create(RefAny::new(()), RefAny::new(()), finished_worker as ThreadCallbackType)
}

fn node(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

#[test]
fn a_closing_window_hands_its_app_threads_to_a_window_that_stays_open() {
    let mut closing = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut heir = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let app_thread = ThreadId::unique();
    let node_thread = ThreadId::unique();
    closing.add_thread(app_thread, thread());
    closing.add_thread_owned_by(node_thread, thread(), Some(node(1)));

    let taken = closing.take_app_threads();
    assert_eq!(taken.keys().copied().collect::<Vec<_>>(), vec![app_thread]);
    assert!(!closing.threads.contains_key(&app_thread));
    assert!(
        closing.threads.contains_key(&node_thread),
        "a worker of the closing window's node stays and stops with it"
    );

    assert!(heir.adopt_threads(taken), "the heir has threads to poll now");
    assert!(heir.threads.contains_key(&app_thread));
    assert_eq!(heir.thread_owners.owner(&app_thread), None, "it stays the app's");
    assert!(!heir.adopt_threads(BTreeMap::new()), "nothing came, nothing to poll");
}
