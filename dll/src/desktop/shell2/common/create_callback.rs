//! `WindowCreateOptions::create_callback`: fired once per window, when the window exists and before its first layout.
//!
//! The callback is where an app starts what lives as long as its window: a timer, a worker thread, a request for the
//! keyring, the language of its text (`CallbackInfo::set_locale`), the files it loads. The desktop shells fire it with
//! their own code (macOS, X11, Wayland and Windows take it out of the options before they consume them; headless keeps
//! it until `run()`); the phone shells took the options' window state and colours and DROPPED it, so an app that starts
//! a timer there was dead on a phone. This is the one function a shell calls: Android at its first `InitWindow`, iOS where
//! its window is ready (`dll/tests/shells_reach_the_create_callback.rs` fails when a shell does not reach it).
//!
//! What the callback asked for (timers, a locale, a new window, a changed window state) is applied the way every other
//! user callback's changes are (`PlatformWindow::apply_user_change`), and a result that needs more than a repaint raises a
//! regeneration. The shell runs its own event pass after this call (it applies what the changes put in the window state).

use azul_layout::callbacks::OptionCallback;

use super::event::PlatformWindow;

/// Fire `callback` (the options' `create_callback`) on `window` and apply what it asked for. A window without a
/// callback does nothing. The caller takes the callback out of its window first so that it fires exactly once.
pub fn invoke_create_callback<W: PlatformWindow>(window: &mut W, callback: OptionCallback) {
    let Some(mut callback) = callback.into_option() else {
        return;
    };

    let app_data = window.get_app_data().clone();
    let (changes, _update) = {
        let borrows = window.prepare_callback_invocation();
        let mut app_data_ref = app_data.borrow_mut();
        let result = borrows.layout_window.invoke_single_callback(
            &mut callback,
            &mut *app_data_ref,
            &borrows.window_handle,
            borrows.gl_context_ptr,
            borrows.system_style.clone(),
            &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
            borrows.previous_window_state,
            borrows.current_window_state,
            borrows.renderer_resources,
        );
        result
    };

    for change in &changes {
        let result = window.apply_user_change(change);
        if result != azul_core::events::ProcessEventResult::DoNothing {
            window
                .get_common_mut()
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        }
    }
}
