//! The thread whose answer asks for a new DOM is named (E76, 2026-11-28).
//!
//! The startup timeline prints, with every layout pass, whose callback asked for it. A write-back or a worker's message that
//! answers `Update::RefreshDom` is the thread's; `LayoutWindow::run_all_threads_naming_the_asker` says which thread it was
//! (the shell then names the node that started it, `managers::thread_owner`, as `via=a thread's write-back (...)`), and says
//! none when every thread kept quiet or answered `DoNothing`.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use azul_core::{
    callbacks::Update,
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::RendererResources,
    task::{ThreadId, ThreadReceiver},
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

/// A worker that asks for a new DOM once and ends.
extern "C" fn asks_for_the_dom(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let _sent = sender.send(ThreadReceiveMsg::Update(Update::RefreshDom));
}

/// A worker that answers `DoNothing` once and ends.
extern "C" fn answers_nothing(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let _sent = sender.send(ThreadReceiveMsg::Update(Update::DoNothing));
}

/// A worker that sends nothing at all.
extern "C" fn says_nothing(_init: RefAny, _sender: ThreadSender, _recv: ThreadReceiver) {}

fn finished(thread: &Thread) -> bool {
    thread.ptr.lock().is_ok_and(|inner| inner.is_finished())
}

/// A thread of `worker`, run to its end (it has said what it says).
fn thread_of(worker: ThreadCallbackType) -> Thread {
    let thread = Thread::create(RefAny::new(()), RefAny::new(()), worker);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !finished(&thread) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(finished(&thread), "harness: the worker ran");
    thread
}

fn poll(lw: &mut LayoutWindow) -> (Update, Option<ThreadId>) {
    let mut data = RefAny::new(());
    let state = FullWindowState::default();
    let (_changes, update, asker) = lw.run_all_threads_naming_the_asker(
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    (update, asker)
}

/// Three threads, one of which asks for the DOM: it is the one named, whatever order the threads are polled in.
///
/// Mutation: the named thread is the first thread polled, or the last: the quiet ones are named here.
#[test]
fn the_thread_that_asked_for_the_dom_is_the_one_named() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let (quiet, nothing, asking) = (ThreadId::unique(), ThreadId::unique(), ThreadId::unique());
    lw.add_thread(quiet, thread_of(says_nothing as ThreadCallbackType));
    lw.add_thread(nothing, thread_of(answers_nothing as ThreadCallbackType));
    lw.add_thread(asking, thread_of(asks_for_the_dom as ThreadCallbackType));

    let (update, named) = poll(&mut lw);

    assert_eq!(update, Update::RefreshDom, "the asking thread's answer is the window's");
    assert_eq!(named, Some(asking), "the thread that asked is named, not a thread that kept quiet");
}

/// No thread asked: nothing is named, and the answer is the old `run_all_threads`'s.
#[test]
fn no_thread_is_named_when_none_asked() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.add_thread(ThreadId::unique(), thread_of(says_nothing as ThreadCallbackType));
    lw.add_thread(ThreadId::unique(), thread_of(answers_nothing as ThreadCallbackType));

    let (update, named) = poll(&mut lw);

    assert_eq!(update, Update::DoNothing);
    assert_eq!(named, None, "nobody asked, nobody is named");
}
