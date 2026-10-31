//! A worker wakes the UI thread when it has something to say, instead of the UI
//! thread asking every worker sixty times a second.
//!
//! Every window with a live background thread polled it each frame and sent each
//! worker a `Tick`, so an app with one permanent worker (AzMonitor's sampler,
//! AzMeet's call) woke sixty times a second for its whole life. A worker's send,
//! and its end, now call the wake the shell installed (`set_thread_wake`); the
//! frame `Tick` goes only to a worker that asked for it
//! (`Thread::with_frame_ticks`).

use core::sync::atomic::{AtomicUsize, Ordering};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use azul_core::{
    callbacks::Update,
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::RendererResources,
    task::{ThreadId, ThreadReceiver, ThreadSendMsg},
    window::RawWindowHandle,
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    thread::{Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

static WAKES: AtomicUsize = AtomicUsize::new(0);

extern "C" fn count_wake() {
    WAKES.fetch_add(1, Ordering::SeqCst);
}

/// Sends one update and ends.
extern "C" fn send_once(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let _sent = sender.send(ThreadReceiveMsg::Update(Update::RefreshDom));
}

fn finished(thread: &Thread) -> bool {
    thread.ptr.lock().is_ok_and(|inner| inner.is_finished())
}

#[test]
fn a_workers_send_wakes_the_ui_thread() {
    azul_layout::thread::set_thread_wake(Some(count_wake));
    // Whatever another test's worker left pending is taken, as a shell's poll
    // takes it: the next send wakes again.
    let _ = azul_layout::thread::take_thread_wake();
    let before = WAKES.load(Ordering::SeqCst);

    let thread = Thread::create(
        RefAny::new(()),
        RefAny::new(()),
        send_once as ThreadCallbackType,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !finished(&thread) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(finished(&thread), "harness: the worker ran");
    assert!(
        WAKES.load(Ordering::SeqCst) > before,
        "the worker's send (and its end) must wake the UI thread"
    );
}

/// How many `Tick`s each kind of worker saw.
#[derive(Debug, Default)]
struct Ticks {
    seen: AtomicUsize,
}

/// Counts the `Tick`s it gets until it is told to stop (bounded).
extern "C" fn count_ticks(mut init: RefAny, _sender: ThreadSender, recv: ThreadReceiver) {
    let Some(ticks) = init.downcast_ref::<Arc<Ticks>>().map(|t| (*t).clone()) else {
        return;
    };
    let Ok(inner) = recv.ptr.lock() else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match inner
            .ptr
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(ThreadSendMsg::Tick) => {
                ticks.seen.fetch_add(1, Ordering::SeqCst);
            }
            Ok(ThreadSendMsg::TerminateThread) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

fn poll(lw: &mut LayoutWindow) {
    let mut data = RefAny::new(());
    let state = FullWindowState::default();
    let _ = lw.run_all_threads(
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
}

#[test]
fn only_a_worker_that_asked_for_frame_ticks_gets_them() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let quiet = Arc::new(Ticks::default());
    let ticking = Arc::new(Ticks::default());
    lw.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(quiet.clone()),
            RefAny::new(()),
            count_ticks as ThreadCallbackType,
        ),
    );
    lw.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(ticking.clone()),
            RefAny::new(()),
            count_ticks as ThreadCallbackType,
        )
        .with_frame_ticks(true),
    );
    assert!(
        lw.thread_poll_needed(),
        "a worker that wants a tick each frame keeps the frame poll alive"
    );

    for _ in 0..3 {
        poll(&mut lw);
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while ticking.seen.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        ticking.seen.load(Ordering::SeqCst),
        3,
        "the worker that asked gets one tick per poll"
    );
    assert_eq!(
        quiet.seen.load(Ordering::SeqCst),
        0,
        "a worker that never asked for ticks gets none"
    );
    drop(lw);
}

#[test]
fn a_window_whose_workers_wake_it_needs_no_frame_poll() {
    azul_layout::thread::set_thread_wake(Some(count_wake));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let quiet = Arc::new(Ticks::default());
    lw.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(quiet),
            RefAny::new(()),
            count_ticks as ThreadCallbackType,
        ),
    );
    assert!(
        !lw.thread_poll_needed(),
        "the worker wakes the UI thread when it sends, so nothing polls it each frame"
    );
    drop(lw);
}
