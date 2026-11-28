//! What the system asks the app to open: the platform half.
//!
//! The data model is `azul_core::open_files`; the queue, the handler and the limits are `azul_layout::managers::open_files`,
//! `azul_layout::open_intent` (what an Android intent asks) and `azul_layout::open_inbox` (the working copies). This module is
//! the OS plumbing and the pump that runs the app's handler:
//!
//! | | Android | iOS |
//! |---|---|---|
//! | where it arrives | `AzulActivity.onCreate` (the intent that STARTED the app) and `onNewIntent` (the app runs): `AzulOpenIntent.nativeOnIntent` | `application:openURL:options:` of the app delegate |
//! | what is read | the action, type, data, `EXTRA_STREAM`, `ClipData`, `EXTRA_TEXT`, flags - on the UI thread, nothing slow | the URL; a file URL is security-scoped when it was opened in place |
//! | the copy | a worker thread: `ContentResolver.openFileDescriptor` -> a `File` -> [`azul_layout::open_inbox`] | a worker thread: the file at the URL -> the inbox (a file already inside the app's `Documents/Inbox` is used where it is) |
//! | the handler | the loop thread's pump, [`pump`] | the display tick's pump, [`pump`] |
//!
//! Requests are kept until the app installs its handler (`CallbackInfo::set_open_handler`): the intent that starts an app arrives
//! before the app's first callback.

use azul_core::events::ProcessEventResult;
use azul_layout::managers::open_files::OpenDelivery;

use crate::desktop::{
    process_app,
    shell2::common::event::{MenuInvocation, PlatformWindow},
};

#[cfg(all(target_os = "android", feature = "jni"))]
pub mod android;
#[cfg(target_os = "ios")]
pub mod apple;

/// Take the requests that wait for a handler and now have one. Event-loop thread.
#[must_use]
pub fn take_deliveries() -> Vec<OpenDelivery> {
    process_app::services().map_or_else(Vec::new, |services| services.open.take_deliveries())
}

/// Give back deliveries that had no window to run against (the request that started the app arrives before its first window).
pub fn defer_deliveries(deliveries: Vec<OpenDelivery>) {
    if deliveries.is_empty() {
        return;
    }
    if let Some(services) = process_app::services() {
        crate::plog_debug!("[open] {} request(s) wait for a window to run against", deliveries.len());
        services.open.defer(deliveries);
    }
}

/// Run the app's handler for each delivery against `window`, with the request installed for `CallbackInfo::get_open_request`.
pub fn invoke_deliveries<W: PlatformWindow>(window: &mut W, deliveries: Vec<OpenDelivery>) -> ProcessEventResult {
    let slot = window.get_layout_window().map(|lw| lw.app_services.open.clone());
    let mut result = ProcessEventResult::DoNothing;
    for delivery in deliveries {
        let OpenDelivery { callback, request } = delivery;
        let run = || window.invoke_menu_callback(callback, MenuInvocation::Native { site: "open" });
        result = result.max(match &slot {
            Some(slot) => slot.with_current(&request, run),
            None => run(),
        });
    }
    result
}

/// The loop's slot: take what waits and run it against `window`.
pub fn pump<W: PlatformWindow>(window: &mut W) -> ProcessEventResult {
    let deliveries = take_deliveries();
    if deliveries.is_empty() {
        return ProcessEventResult::DoNothing;
    }
    invoke_deliveries(window, deliveries)
}

/// Whether a request waits for a handler (an app that never installs one keeps them, bounded).
#[must_use]
pub fn has_held() -> bool {
    process_app::services().is_some_and(|services| services.open.held() > 0)
}
