//! The app's life in the shells: where a shell hands the system's lifecycle callbacks to the app.
//!
//! A shell calls [`raise`] from the handler of the platform's callback - Android's `onPause` / `onResume` /
//! `onTrimMemory`, iOS's `applicationDidEnterBackground` / `applicationWillEnterForeground` /
//! `applicationDidReceiveMemoryWarning` / `applicationWillTerminate` - on the thread that owns its window. The event
//! is queued in the app's mailbox (`AppServices::lifecycle`) and the handler the app installed
//! (`CallbackInfo::set_lifecycle_handler`) runs against that window at once, with the event installed as the one
//! `CallbackInfo::get_lifecycle_event` reports; what the handler asked for (a timer, a changed window state, a rebuild)
//! is applied the way every other callback's changes are.
//!
//! A callback of the system on a thread without a window (a JNI call on Java's UI thread) uses [`queue`]: it parks the
//! event and wakes the loop, and the loop thread calls [`deliver`] with its window. A handler is the app's own code:
//! it runs on the loop thread like a menu click, never on the thread the system called.
//!
//! The same call serves a test: `HeadlessWindow` is a `PlatformWindow`, so a host test raises `Paused` and sees the
//! handler run (`dll/tests/lifecycle_headless.rs`), and the e2e ops of the debug server reach it the same way.

use azul_core::{events::ProcessEventResult, lifecycle::LifecycleEvent};
use azul_layout::managers::lifecycle::{route_lifecycle_events, LifecycleDelivery};

use super::{
    process_app,
    shell2::common::event::{MenuInvocation, PlatformWindow},
};

/// Tell the app of `event`: queue it and run the app's handler against `window` now. Returns the strongest result the
/// handler asked for (`DoNothing` when the app has no handler or `window` has no layout yet).
pub fn raise<W: PlatformWindow + ?Sized>(window: &mut W, event: LifecycleEvent) -> ProcessEventResult {
    let Some(services) = window.get_layout_window().map(|lw| lw.app_services.clone()) else {
        return ProcessEventResult::DoNothing;
    };
    if !services.lifecycle.queue_lifecycle_event(event) {
        crate::plog_warn!("[lifecycle] {event:?} dropped: the mailbox of waiting events is full");
    }
    deliver(window)
}

/// Park `event` for the loop, from a thread that has no window: the loop's next [`deliver`] runs it. Nothing is
/// queued before an App registered its services.
pub fn queue(event: LifecycleEvent) {
    if let Some(services) = process_app::services() {
        if services.lifecycle.queue_lifecycle_event(event) {
            super::loop_waker::wake();
        }
    }
}

/// Run every queued lifecycle event against `window`, in the order they arrived. An app without a handler hears
/// nothing: the events are dropped.
pub fn deliver<W: PlatformWindow + ?Sized>(window: &mut W) -> ProcessEventResult {
    let Some(events) = window
        .get_layout_window()
        .map(|lw| lw.app_services.lifecycle.clone())
    else {
        return ProcessEventResult::DoNothing;
    };
    let queued = events.drain_lifecycle_events();
    if queued.is_empty() {
        return ProcessEventResult::DoNothing;
    }
    let handler = events.lifecycle_handler();
    if handler.as_ref().is_none() {
        crate::plog_debug!(
            "[lifecycle] {} event(s) dropped: the app set no handler (CallbackInfo::set_lifecycle_handler)",
            queued.len()
        );
    }
    let mut result = ProcessEventResult::DoNothing;
    for LifecycleDelivery { callback, event } in route_lifecycle_events(queued, &handler) {
        // Installed for the call, so the handler can ask `CallbackInfo::get_lifecycle_event` which event it runs for.
        let run = || window.invoke_menu_callback(callback, MenuInvocation::Native { site: "lifecycle" });
        result = result.max(events.with_current_lifecycle_event(&event, run));
    }
    if result != ProcessEventResult::DoNothing {
        // What the handler asked for (a rebuild) reaches the window the way a create callback's does.
        window
            .get_common_mut()
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    }
    result
}
