//! A second window of the app on a shell with ONE surface: a view pushed over the first.
//!
//! An activity has one surface, so `queue_window_create` cannot open an OS window. The window the app
//! asks for (AzMail's compose window) is made like any window of the app - its own layout window, its own
//! window state, layout callback, timers and callbacks, so `close_window()`, `get_current_window_state()`
//! and the timers it starts inside mean THAT window - but it has no surface of its own: it takes this
//! one over, full screen. `common` is always the window on top; the windows under it wait in
//! `views_below` with their state, their timers and their threads going on (nothing is drawn for them).
//!
//! BACK, a close the app asks for and the app's own `CloseRequested` callbacks (which may veto, as
//! for a window of its own) pop the top window; what it left running (a send that is still on its
//! thread) goes on in the window below.

use azul_core::callbacks::RelayoutReason;
use azul_layout::window_state::WindowCreateOptions;

use super::{make_common, AndroidWindow, RenderBackend};
use crate::{
    desktop::shell2::{
        android_core::{a11y_refresh::ScrollRefresh, slop::TapSlop},
        common::{debug_server, event::PlatformWindow},
    },
    log_debug, log_error, log_info,
};

impl AndroidWindow {
    /// Is the window the user sees a view pushed over another window of the app?
    #[must_use]
    pub fn is_pushed_view(&self) -> bool {
        !self.views_below.is_empty()
    }

    /// The windows the app asked for are pushed, oldest first, and the close the app asked for of the
    /// view on top runs - at the turn of the loop, never inside the pass that asked.
    pub(super) fn apply_pending_views(&mut self) {
        for options in core::mem::take(&mut self.pending_views) {
            self.push_view(options);
        }
        // A close the app raised (`close_window()`), or a window state it wrote with the flag up.
        let owed = core::mem::take(&mut self.view_close_owed);
        if self.is_pushed_view()
            && (owed || self.common.current_window_state().flags.close_requested)
        {
            // The app's `CloseRequested` callbacks hear it first and may keep the view
            // (the "Save the draft?" question): the flag stands only if nobody vetoed.
            let _ = self.confirm_app_close("android.view.close");
            if self.common.current_window_state().flags.close_requested {
                self.pop_view_now();
            }
        }
    }

    /// The renderer of the GPU path is made for the window it draws (its resource keys, its document): a window
    /// pushed over it draws on the CPU, and a surface has one producer. So the window leaves the GPU path when its
    /// first view comes - the surface, the renderer and the context in the order the lifecycle says - and takes the
    /// CPU path for good, as a window whose GPU path ended does, with one diagnostic line.
    fn leave_the_gpu_path(&mut self) {
        if self.backend != RenderBackend::Gpu {
            return;
        }
        log_info!(
            super::LogCategory::Rendering,
            "[render] a second window of the app is shown over this one: the window draws on the CPU from here on"
        );
        self.gpu_shutdown();
        self.common.deinit_renderer();
        self.backend = RenderBackend::Cpu;
        self.gpu_present_owed = false;
    }

    /// Makes the window `options` describe and puts it on top of the one the user sees.
    fn push_view(&mut self, mut options: WindowCreateOptions) {
        crate::desktop::shell2::common::resolve_initial_background_color(
            &mut options,
            &self.view_config.system_style,
            self.common.app.mode(),
        );
        // The view takes the surface over: its size, its density, the display it is on and the mode
        // it shows are the surface's, whatever the app asked for a window of its own.
        {
            let surface = self.common.current_window_state();
            options.window_state.size = surface.size;
            options.window_state.mode = surface.mode;
            options.window_state.monitor_id = surface.monitor_id;
            options.window_state.flags.display_preset = surface.flags.display_preset;
        }
        let made = make_common(
            options,
            self.common.fc_cache.clone(),
            &self.view_config,
            self.common.system_style.clone(),
            self.common.app_data.clone(),
            self.common.undo_manager.clone(),
            self.common.app.clone(),
        );
        let (view, create_callback) = match made {
            Ok(made) => made,
            Err(e) => {
                log_error!(
                    super::LogCategory::Window,
                    "[Android] the window the app asked for could not be made: {:?}",
                    e
                );
                return;
            }
        };
        self.leave_the_gpu_path();
        let below = core::mem::replace(&mut self.common, view);
        self.views_below.push(below);
        // What the system told the window it is on - the bars, the cutout, the keyboard - is the
        // surface's: the view has it from its first layout.
        if let (Some(view_lw), Some(below_lw)) = (
            self.common.layout_window.as_mut(),
            self.views_below
                .last()
                .and_then(|below| below.layout_window.as_ref()),
        ) {
            view_lw.safe_area_insets = below_lw.safe_area_insets.clone();
            view_lw.display_features = below_lw.display_features.clone();
        }
        log_debug!(
            super::LogCategory::Window,
            "[Android] a window of the app is pushed over the one on screen ({} below)",
            self.views_below.len()
        );
        self.view_changed();
        // The debug server reaches the view by its window id, like any window of the app.
        debug_server::register_debug_timer_on_new_window(self);
        // As on the first window's `InitWindow`: the window exists, nothing is laid out yet, and what
        // the callback asks for is applied by the pass that follows.
        crate::desktop::shell2::common::create_callback::invoke_create_callback(self, create_callback);
        self.apply_bars_of_the_window_on_top();
        let _ = self.process_window_events(0);
    }

    /// The top view closes: the window under it is the one on screen again, with whatever its timers
    /// and threads did meanwhile. Whatever the view left running on a thread goes on in that window.
    pub(super) fn pop_view_now(&mut self) {
        let Some(parent) = self.views_below.pop() else {
            return;
        };
        self.view_close_owed = false;
        let mut closed = core::mem::replace(&mut self.common, parent);
        let poll_was_needed = PlatformWindow::thread_poll_needed(self);
        let (inherited, keyring_ops) = closed.layout_window.as_mut().map_or_else(
            || (Vec::new(), 0),
            |lw| {
                (
                    lw.take_threads_for_heir(),
                    core::mem::take(&mut lw.keyring_manager.in_flight),
                )
            },
        );
        let adopted = self.common.layout_window.as_mut().is_some_and(|lw| {
            lw.keyring_manager.mark_requests_dispatched(keyring_ops);
            lw.adopt_handed_over_threads(inherited)
        });
        if adopted {
            PlatformWindow::sync_thread_poll_timer(self, poll_was_needed);
        }
        if keyring_ops > 0 {
            PlatformWindow::sync_capability_pump_timer(self);
        }
        closed.deinit_renderer();
        drop(closed);
        log_debug!(
            super::LogCategory::Window,
            "[Android] the window on top closed ({} left below)",
            self.views_below.len()
        );
        self.view_changed();
        self.apply_bars_of_the_window_on_top();
    }

    /// What the window on top asks of the system bars (`WindowFlags::system_bars`).
    fn apply_bars_of_the_window_on_top(&mut self) {
        #[cfg(feature = "android-activity")]
        super::apply_system_bars_if_changed(self);
    }

    /// The window on screen is another one now: nothing the shell kept of the last one's frame, its
    /// touches or its text is true any more.
    fn view_changed(&mut self) {
        // The next frame is painted in full and posted whole.
        self.cpu_backend.force_full_repaint = true;
        self.present.surface_changed();
        self.needs_rerender = true;
        // A finger that was down belonged to the other window; the IME and TalkBack read the new one.
        self.tap = TapSlop::default();
        self.touch_pan_last = None;
        self.primary_pointer_id = None;
        self.pan_pointer_id = None;
        self.ime_dirty = true;
        self.a11y_scroll = ScrollRefresh::new();
        self.common.a11y_dirty = true;
        // The window is built again from the app's state: a pushed one for its first picture, the one
        // that comes back for what changed while it waited.
        self.common.request_regeneration(RelayoutReason::RefreshDom);
    }

    /// The timers and the threads of the windows under the one on screen go on (a mail check is
    /// not paused because a message is being written); nothing is drawn for them.
    pub(super) fn pump_suspended_views(&mut self) {
        for i in 0..self.views_below.len() {
            core::mem::swap(&mut self.common, &mut self.views_below[i]);
            let _ = self.process_timers_and_threads();
            core::mem::swap(&mut self.common, &mut self.views_below[i]);
        }
    }
}
