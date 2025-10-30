//! Process event callbacks and manage window lifecycle for Win32.
//!
//! This module handles:
//!
//! - Processing UI events and invoking callbacks
//! - Timer event handling
//! - Thread message processing
//! - Window creation/destruction
//! - Callback result processing

use std::{collections::HashMap, sync::Arc};

use azul_core::{
    refany::RefAny, 
    resources::ImageCache,
    events::ProcessEventResult,
};
use azul_layout::{
    callbacks::CallCallbacksResult,
    thread::Thread,
    timer::Timer,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::FcFontCache;
use webrender::Transaction as WrTransaction;

use super::Win32Window;

/// Process a timer event for a window
#[must_use]
pub fn process_timer(
    timer_id: usize,
    window: &mut Win32Window,
    image_cache: &mut ImageCache,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<super::dlopen::HWND>,
) -> ProcessEventResult {
    // Get current time from system
    let frame_start = std::time::Instant::now();

    // Run the timer callback via LayoutWindow
    let callback_result = if let Some(ref mut layout_window) = window.layout_window {
        #[cfg(feature = "std")]
        {
            use azul_core::window::RawWindowHandle;

            // Create Win32 window handle
            let window_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                hwnd: window.hwnd as *mut std::ffi::c_void,
                hinstance: window.hinstance as *mut std::ffi::c_void,
            });

            // Get the GL context from window
            let gl_context = window.gl_context_ptr.clone();

            // Clone fc_cache for use in callback
            let mut fc_cache_clone = (*window.fc_cache).clone();

            // Get current time using system callback
            let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
            let azul_instant = (system_callbacks.get_system_time_fn.cb)();

            layout_window.run_single_timer(
                timer_id,
                azul_instant,
                &window_handle,
                &gl_context,
                image_cache,
                &mut fc_cache_clone,
                window.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &window.previous_window_state,
                &window.current_window_state,
                &window.renderer_resources,
            )
        }
        #[cfg(not(feature = "std"))]
        {
            CallCallbacksResult::default()
        }
    } else {
        CallCallbacksResult::default()
    };

    process_callback_results(
        callback_result,
        window,
        image_cache,
        new_windows,
        destroyed_windows,
    )
}

/// Process thread messages for a window
#[must_use]
pub fn process_threads(
    window: &mut Win32Window,
    image_cache: &mut ImageCache,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<super::dlopen::HWND>,
) -> ProcessEventResult {
    #[cfg(feature = "std")]
    {
        use azul_core::window::RawWindowHandle;

        // Run all pending thread messages via LayoutWindow
        let callback_result = if let Some(ref mut layout_window) = window.layout_window {
            // We need RefAny data here - but we don't have it in this context
            // This needs to be passed from a higher level
            // For now, create an empty RefAny as placeholder
            let mut placeholder_data = RefAny::new(());

            // Create Win32 window handle
            let window_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                hwnd: window.hwnd as *mut std::ffi::c_void,
                hinstance: window.hinstance as *mut std::ffi::c_void,
            });

            // Get the GL context from window
            let gl_context = window.gl_context_ptr.clone();

            // Clone fc_cache for use in callback
            let mut fc_cache_clone = (*window.fc_cache).clone();

            // TODO: Process all pending threads, not just one
            // For now, return empty result
            CallCallbacksResult::default()
        } else {
            CallCallbacksResult::default()
        };

        process_callback_results(
            callback_result,
            window,
            image_cache,
            new_windows,
            destroyed_windows,
        )
    }
    #[cfg(not(feature = "std"))]
    {
        ProcessEventResult::DoNothing
    }
}

/// Process callback results and determine what action to take next
#[must_use]
pub fn process_callback_results(
    mut callback_results: CallCallbacksResult,
    window: &mut Win32Window,
    image_cache: &mut ImageCache,
    new_windows: &mut Vec<WindowCreateOptions>,
    destroyed_windows: &mut Vec<super::dlopen::HWND>,
) -> ProcessEventResult {
    use azul_core::callbacks::Update;

    let mut result = ProcessEventResult::DoNothing;

    // Handle window creation/destruction
    new_windows.extend(callback_results.windows_created.into_iter());

    // Check if window should be closed (close_requested flag set by callback)
    if let Some(ref modified_state) = callback_results.modified_window_state {
        if modified_state.flags.close_requested {
            destroyed_windows.push(window.hwnd);
            result = ProcessEventResult::DoNothing; // Window will be destroyed
        }
    }

    // Handle image updates
    if callback_results.images_changed.is_some() || callback_results.image_masks_changed.is_some() {
        if let Some(ref mut layout_window) = window.layout_window {
            // TODO: Update image resources via LayoutWindow
            // let updated_images = layout_window.update_image_resources(...);

            // For now, just mark that we need to update display list
            result = ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
        }
    }

    // Handle font updates
    if callback_results.words_changed.is_some() {
        // Font/text updates
        result = ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
    }

    // Handle timers - convert from Option<HashMap<TimerId, Timer>> to HashMap<usize, Timer>
    if let Some(timers) = callback_results.timers {
        let timers_added: HashMap<usize, Timer> = timers
            .into_iter()
            .map(|(id, timer)| (id.id, timer))
            .collect();
        let timers_removed = callback_results
            .timers_removed
            .map(|set| set.into_iter().map(|id| id.id).collect())
            .unwrap_or_default();

        window.start_stop_timers(timers_added, timers_removed);
    }

    // Handle threads - add/remove directly from layout_window.threads
    if let Some(threads) = callback_results.threads {
        if let Some(layout_window) = window.layout_window.as_mut() {
            for (thread_id, thread) in threads {
                layout_window.threads.insert(thread_id, thread);
            }
            // Start thread tick timer when threads are added
            if !layout_window.threads.is_empty() {
                window.start_thread_tick_timer();
            }
        }
    }
    if let Some(threads_removed) = callback_results.threads_removed {
        if let Some(layout_window) = window.layout_window.as_mut() {
            for thread_id in threads_removed {
                layout_window.threads.remove(&thread_id);
            }

            // Stop the thread tick timer if no more threads are active
            if layout_window.threads.is_empty() {
                window.stop_thread_tick_timer();
            }
        }
    }

    // Determine final result based on callbacks
    match callback_results.callbacks_update_screen {
        Update::DoNothing => {}
        Update::RefreshDom => {
            result = ProcessEventResult::ShouldRegenerateDomCurrentWindow;
        }
        Update::RefreshDomAllWindows => {
            result = ProcessEventResult::ShouldRegenerateDomAllWindows;
        }
    }

    result
}

/// Extension trait for Callback to convert from CoreCallbackData
trait CallbackExt {
    fn from_core(
        core_callback: azul_core::callbacks::CoreCallbackData,
    ) -> azul_layout::callbacks::Callback;
}

impl CallbackExt for azul_layout::callbacks::Callback {
    fn from_core(core_callback: azul_core::callbacks::CoreCallbackData) -> Self {
        Self {
            cb: unsafe { std::mem::transmute(core_callback.callback.cb) },
        }
    }
}
