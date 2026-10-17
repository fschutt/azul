//! A headless window closed during a turn of its event loop ends the loop at
//! once, also with nothing left to wake it (no timer, no thread, no debug
//! server). The turn went on to the loop's wait, which then had no wake
//! source, and the process stayed up with no window (AzPaint after its
//! "Save changes?" exported the drawing and closed from a callback).

use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use azul::desktop::shell2::{common::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, TimerCallbackReturn, Update},
    dom::Dom,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::{AppConfig, AppTerminationBehavior},
    task::TerminateTimer,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    timer::{Timer, TimerCallbackInfo, TimerCallbackType},
    window_state::WindowCreateOptions,
};
use rust_fontconfig::FcFontCache;

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(Dom::create_p_with_text("a drawing"))
}

/// Closes the window and ends itself: afterwards nothing is left to wake the loop.
extern "C" fn close_now(_data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    info.callback_info.close_window();
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Terminate,
    }
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

    // The loop returns to the test instead of ending the process.
    let mut config = AppConfig::default();
    config.termination_behavior = AppTerminationBehavior::ReturnToMain;
    let mut options = WindowCreateOptions::default();
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut window = HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        config,
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("a headless window");
    // Due a few turns in, so the close does not ride on the wake the timer's
    // start left pending.
    let timer = Timer::create(
        RefAny::new(()),
        close_now as TimerCallbackType,
        ExternalSystemCallbacks::rust_internal().get_system_time_fn,
    )
    .with_delay(azul_core::task::Duration::from_millis(80));
    window.start_timer(7, timer);
    window.run().expect("the event loop ends");
    returned.store(true, Ordering::SeqCst);
}
