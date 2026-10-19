//! The LAST window to close does not wait for its workers either: a tray app
//! goes on running after its last window, and a worker stuck in a blocking
//! call froze it for up to the 2 s grace in `stop_all`. The workers are told
//! to stop and the window goes at once; the process waits for them at its
//! end (`join_stopping_workers`, which the shells call when `App::run`
//! returns), each up to its grace, so a closing app still lets them finish.

use std::time::{Duration, Instant};

use azul_core::{
    refany::RefAny,
    task::{OptionThreadSendMsg, ThreadId, ThreadReceiver, ThreadSendMsg},
};
use azul_layout::{
    managers::thread_owner::{join_stopping_workers, ORPHAN_GRACE},
    thread::{Thread, ThreadCallbackType, ThreadSender},
    window::LayoutWindow,
};
use rust_fontconfig::FcFontCache;

/// How long the worker takes to stop once it is told.
const STOP_TAKES: Duration = Duration::from_millis(600);

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

#[test]
fn closing_the_last_window_never_waits_for_its_workers() {
    let mut last = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    last.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(()), RefAny::new(()), slow_to_stop as ThreadCallbackType),
    );

    let start = Instant::now();
    drop(last);
    let took = start.elapsed();
    assert!(
        took < STOP_TAKES / 2,
        "closing the last window waited {took:?} for a worker that takes {STOP_TAKES:?} to stop"
    );

    // At the end of the process the worker gets its time to finish, bounded.
    let start = Instant::now();
    join_stopping_workers();
    let took = start.elapsed();
    assert!(
        took >= STOP_TAKES / 2,
        "the process ended without letting the worker stop ({took:?})"
    );
    assert!(
        took < ORPHAN_GRACE + Duration::from_millis(500),
        "the wait at the end is bounded by the grace ({took:?})"
    );
}
