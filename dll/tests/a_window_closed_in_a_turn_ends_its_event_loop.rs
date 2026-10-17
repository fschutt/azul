//! A headless window closed during a turn of its event loop ends the loop at
//! once, also with nothing left to wake it (no timer, no thread, no debug
//! server). The turn went on to the loop's wait, which then had no wake
//! source, and the process stayed up with no window (AzPaint after its
//! "Save changes?" exported the drawing and closed).

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use azul::desktop::shell2::headless::{HeadlessEvent, HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::Dom,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(Dom::create_p_with_text("a drawing"))
}

#[test]
fn a_window_closed_in_a_turn_ends_its_event_loop() {
    // The loop must return long before this; a loop that waits for a wake
    // that never comes would hang the test run instead of failing it.
    let returned = Arc::new(AtomicBool::new(false));
    let watched = returned.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        if !watched.load(Ordering::SeqCst) {
            eprintln!("the event loop of a closed window did not return within 30 s");
            std::process::exit(101);
        }
    });

    let mut options = WindowCreateOptions::default();
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut window = HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("a headless window");
    window.inject_event(HeadlessEvent::Close);
    window.run().expect("the event loop ends");
    returned.store(true, Ordering::SeqCst);
}
