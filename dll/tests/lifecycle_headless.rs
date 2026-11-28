//! The app's life reaches the app: a shell raises `Paused`, `Resumed`, `LowMemory` or `WillTerminate`
//! (`azul::desktop::lifecycle::raise`) and the handler the app installed with `CallbackInfo::set_lifecycle_handler`
//! runs against the window, reading the event with `CallbackInfo::get_lifecycle_event`.
//!
//! The headless window is a `PlatformWindow` like the phone shells' windows, so this is the path Android's `onPause`
//! and iOS's `applicationDidEnterBackground` take: no emulator is needed to know that the handler runs, with its data,
//! once per event, in order, and that a handler that was never set costs nothing.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::{
    lifecycle,
    shell2::{
        common::event::{PlatformWindow, SharedUndoManager},
        headless::HeadlessWindow,
    },
};
use azul_core::{
    callbacks::{CoreCallback, LayoutCallback, LayoutCallbackInfo, Update},
    dom::Dom,
    icon::{IconProviderHandle, SharedIconProvider},
    lifecycle::LifecycleEvent,
    menu::CoreMenuCallback,
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
};
use azul_layout::{callbacks::CallbackInfo, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

/// What the handler saw: the event of every delivery, and what `get_lifecycle_event` said.
#[derive(Default)]
struct Seen {
    events: Vec<Option<LifecycleEvent>>,
}

extern "C" fn on_lifecycle(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(mut seen) = data.downcast_mut::<Seen>() {
        seen.events.push(info.get_lifecycle_event());
    }
    Update::DoNothing
}

extern "C" fn an_app(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

fn window() -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = an_app;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut window = HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed");
    window.regenerate_layout().expect("the first layout");
    window
}

/// Install the handler the way the app does in its create callback: one handler for the app.
fn install_handler(window: &HeadlessWindow, data: &RefAny) {
    let handler: extern "C" fn(RefAny, CallbackInfo) -> Update = on_lifecycle;
    window
        .common
        .layout_window
        .as_ref()
        .expect("a layout window")
        .app_services
        .lifecycle
        .set_lifecycle_handler(CoreMenuCallback {
            refany: data.clone(),
            callback: CoreCallback { cb: handler as usize, ctx: OptionRefAny::None },
        });
}

fn seen_by(data: &mut RefAny) -> Vec<Option<LifecycleEvent>> {
    data.downcast_ref::<Seen>().expect("the handler's data").events.clone()
}

#[test]
fn a_paused_app_runs_its_handler_with_the_event() {
    let mut window = window();
    let mut data = RefAny::new(Seen::default());
    install_handler(&window, &data);

    lifecycle::raise(&mut window, LifecycleEvent::Paused);

    assert_eq!(seen_by(&mut data), vec![Some(LifecycleEvent::Paused)]);
}

#[test]
fn every_event_runs_the_handler_once_and_in_order() {
    let mut window = window();
    let mut data = RefAny::new(Seen::default());
    install_handler(&window, &data);

    for event in [
        LifecycleEvent::Paused,
        LifecycleEvent::Resumed,
        LifecycleEvent::LowMemory,
        LifecycleEvent::WillTerminate,
    ] {
        lifecycle::raise(&mut window, event);
    }

    assert_eq!(
        seen_by(&mut data),
        vec![
            Some(LifecycleEvent::Paused),
            Some(LifecycleEvent::Resumed),
            Some(LifecycleEvent::LowMemory),
            Some(LifecycleEvent::WillTerminate),
        ]
    );
}

#[test]
fn an_app_without_a_handler_hears_nothing_and_what_it_missed_is_not_replayed() {
    let mut window = window();
    // no handler yet: the event is dropped, nothing breaks
    lifecycle::raise(&mut window, LifecycleEvent::LowMemory);

    let mut data = RefAny::new(Seen::default());
    install_handler(&window, &data);
    lifecycle::raise(&mut window, LifecycleEvent::Resumed);

    assert_eq!(seen_by(&mut data), vec![Some(LifecycleEvent::Resumed)], "only what came after the handler");
}

#[test]
fn nothing_is_current_outside_a_delivery() {
    let window = window();
    let layout_window = window.common.layout_window.as_ref().expect("a layout window");
    assert_eq!(layout_window.app_services.lifecycle.current_lifecycle_event(), None);
}

#[test]
fn a_second_handler_replaces_the_first() {
    let mut window = window();
    let mut first = RefAny::new(Seen::default());
    let mut second = RefAny::new(Seen::default());
    install_handler(&window, &first);
    install_handler(&window, &second);

    lifecycle::raise(&mut window, LifecycleEvent::Paused);

    assert!(seen_by(&mut first).is_empty(), "the first was replaced");
    assert_eq!(seen_by(&mut second), vec![Some(LifecycleEvent::Paused)]);
}
