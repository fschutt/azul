//! Stopping a background thread never waits for it on the UI thread.
//!
//! An app stops a worker with `CallbackInfo::remove_thread` (Cancel on a mail sync, a
//! picture download for the mail the user just left). The worker is told
//! `TerminateThread` and usually still has something to say: "stopped after 40 of 100
//! messages". So the window keeps listening to it: the click that stopped it returns at
//! once, the worker's last write-backs still run on the UI thread, and the thread leaves
//! the window when it has finished. A worker stuck in a blocking call must never freeze
//! the window while it is waited for.

use core::sync::atomic::{AtomicBool, Ordering};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use azul_core::task::ThreadId;

use super::*;

/// What the test reads back from the worker and its write-back.
#[derive(Debug, Default)]
struct StopProbe {
    /// The thread the click stops; set once it is added to the window.
    thread: Mutex<Option<ThreadId>>,
    /// The worker saw `TerminateThread`.
    told_to_stop: AtomicBool,
    /// The worker's last write-back ran on the UI thread.
    final_delivered: AtomicBool,
}

#[derive(Debug, Clone)]
struct StopNode {
    probe: Arc<StopProbe>,
}

/// How long the worker takes to wind down once it is told to stop: a mail sync that
/// finishes the message it is on before it reports.
const WIND_DOWN: Duration = Duration::from_secs(1);

/// The last write-back, run on the UI thread: it only marks that it arrived.
extern "C" fn record_final_write_back(
    mut writeback_data: RefAny,
    _sent: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(node) = writeback_data.downcast_ref::<StopNode>() {
        node.probe.final_delivered.store(true, Ordering::SeqCst);
    }
    azul_core::callbacks::Update::DoNothing
}

/// Waits for `TerminateThread`, then takes [`WIND_DOWN`] before it writes back what it
/// has and returns. Bounded, so an engine that never tells it to stop fails the test.
extern "C" fn slow_to_stop_worker(
    mut init: RefAny,
    mut sender: azul_layout::thread::ThreadSender,
    recv: azul_core::task::ThreadReceiver,
) {
    use azul_core::task::ThreadSendMsg;
    use azul_layout::thread::{ThreadReceiveMsg, ThreadWriteBackMsg, WriteBackCallbackType};

    let Some(probe) = init.downcast_ref::<StopNode>().map(|n| n.probe.clone()) else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(6);
    {
        let Ok(inner) = recv.ptr.lock() else {
            return;
        };
        loop {
            match inner
                .ptr
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(ThreadSendMsg::TerminateThread) => break,
                Ok(_) => {}
                Err(_) => return,
            }
        }
    }
    probe.told_to_stop.store(true, Ordering::SeqCst);
    std::thread::sleep(WIND_DOWN);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        record_final_write_back as WriteBackCallbackType,
        RefAny::new(()),
    )));
}

/// The click handler: stops the worker, as a Cancel button does.
extern "C" fn stop_on_click(
    mut data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    let id = data
        .downcast_ref::<StopNode>()
        .and_then(|n| n.probe.thread.lock().ok().and_then(|g| *g));
    if let Some(id) = id {
        info.remove_thread(id);
    }
    azul_core::callbacks::Update::DoNothing
}

/// One 400x300 button that stops the worker on a click.
extern "C" fn cancel_button_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        events::{EventFilter, HoverEventFilter},
    };

    let node = data
        .downcast_ref::<StopNode>()
        .map(|n| (*n).clone())
        .expect("stop node");
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 400px; height: 300px;")
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: stop_on_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(node),
                }]
                .into(),
            ),
    )
}

/// A window with the cancel button and a running app thread (one a click started, not a
/// node's own), as AzMail's Send / Receive.
fn window_with_a_running_worker(probe: &Arc<StopProbe>) -> (HeadlessWindow, ThreadId) {
    use azul_layout::thread::{Thread, ThreadCallback};

    use crate::desktop::shell2::common::event::PlatformWindow;

    let state = Arc::new(RefCell::new(RefAny::new(StopNode {
        probe: probe.clone(),
    })));
    let mut window = make_window_sized(&state, cancel_button_layout, 400.0, 300.0);
    window.regenerate_layout().expect("initial layout");

    let node = StopNode {
        probe: probe.clone(),
    };
    let thread = Thread::create(
        RefAny::new(node.clone()),
        RefAny::new(node),
        ThreadCallback::new(slow_to_stop_worker),
    );
    let id = ThreadId::unique();
    let _ = window.apply_user_change(&azul_layout::callbacks::CallbackChange::AddThread {
        thread_id: id,
        thread,
        owner: None,
    });
    if let Ok(mut slot) = probe.thread.lock() {
        *slot = Some(id);
    }
    assert!(
        window.thread_poll_timer_running,
        "a running thread is polled"
    );
    (window, id)
}

/// The longest any frame may take while the worker winds down: far below
/// [`WIND_DOWN`], so a frame that waits for the worker fails, and loose enough for a
/// debug build on a busy machine.
const FRAME_BUDGET: Duration = Duration::from_millis(400);

#[test]
fn removing_a_thread_returns_at_once_and_its_final_write_back_still_arrives() {
    use crate::desktop::shell2::common::event::PlatformWindow;

    let probe = Arc::new(StopProbe::default());
    let (mut window, id) = window_with_a_running_worker(&probe);

    let clicked = Instant::now();
    let _ = click_at(&mut window, 200.0, 150.0);
    let click_took = clicked.elapsed();
    assert!(
        click_took < FRAME_BUDGET,
        "the click that removes the thread must not wait for the worker on the UI thread \
         (took {click_took:?}, the worker winds down for {WIND_DOWN:?})"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut slowest_frame = Duration::ZERO;
    while !probe.final_delivered.load(Ordering::SeqCst) && Instant::now() < deadline {
        let frame = Instant::now();
        let _ = window.invoke_thread_callbacks();
        slowest_frame = slowest_frame.max(frame.elapsed());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        probe.told_to_stop.load(Ordering::SeqCst),
        "remove_thread tells the worker to stop"
    );
    assert!(
        probe.final_delivered.load(Ordering::SeqCst),
        "the write-back the worker sends after it was told to stop must still run"
    );
    assert!(
        slowest_frame < FRAME_BUDGET,
        "no frame may wait for the stopping worker (slowest {slowest_frame:?})"
    );

    // The worker returns right after its last write-back; then it leaves the window
    // and the thread poll stops.
    let gone = |w: &HeadlessWindow| {
        w.common
            .layout_window
            .as_ref()
            .is_none_or(|lw| !lw.threads.contains_key(&id))
    };
    while !gone(&window) && Instant::now() < deadline {
        let _ = window.invoke_thread_callbacks();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        gone(&window),
        "the stopped worker leaves the window once it has returned"
    );
    assert!(
        !window.thread_poll_timer_running,
        "no thread left, so the thread poll stops"
    );
}
