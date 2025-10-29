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

use azul_core::{refany::RefAny, resources::ImageCache};
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

/// Hit test node structure for event routing
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Target for callback dispatch
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs
    RootNodes,
}

/// Result of processing an event - tells the system what to do next
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ProcessEventResult {
    /// Do nothing, continue normal event loop
    DoNothing,
    /// Regenerate the DOM for the current window
    ShouldRegenerateDomCurrentWindow,
    /// Regenerate the DOM for all windows
    ShouldRegenerateDomAllWindows,
    /// Update the display list for the current window
    ShouldUpdateDisplayListCurrentWindow,
    /// Update hit-tester and process the event again
    UpdateHitTesterAndProcessAgain,
    /// Re-render the current window (GPU scroll, etc.)
    ShouldReRenderCurrentWindow,
}

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

/// Invoke callbacks for a given target and event filter
pub fn invoke_callbacks(
    window: &mut Win32Window,
    target: CallbackTarget,
    event_filter: azul_core::events::EventFilter,
) -> Vec<CallCallbacksResult> {
    use azul_core::{
        dom::{DomId, NodeId},
        id::NodeId as CoreNodeId,
    };

    // Collect callbacks based on target
    let callback_data_list = match target {
        CallbackTarget::Node(node) => {
            let layout_window = match window.layout_window.as_ref() {
                Some(lw) => lw,
                None => return Vec::new(),
            };

            let dom_id = DomId {
                inner: node.dom_id as usize,
            };
            let node_id = match NodeId::from_usize(node.node_id as usize) {
                Some(nid) => nid,
                None => return Vec::new(),
            };

            let layout_result = match layout_window.layout_results.get(&dom_id) {
                Some(lr) => lr,
                None => return Vec::new(),
            };

            let binding = layout_result.styled_dom.node_data.as_container();
            let node_data = match binding.get(node_id) {
                Some(nd) => nd,
                None => return Vec::new(),
            };

            node_data
                .get_callbacks()
                .as_container()
                .iter()
                .filter(|cd| cd.event == event_filter)
                .cloned()
                .collect::<Vec<_>>()
        }
        CallbackTarget::RootNodes => {
            let layout_window = match window.layout_window.as_ref() {
                Some(lw) => lw,
                None => return Vec::new(),
            };

            let mut callbacks = Vec::new();
            for (_dom_id, layout_result) in &layout_window.layout_results {
                if let Some(root_node) = layout_result
                    .styled_dom
                    .node_data
                    .as_container()
                    .get(CoreNodeId::ZERO)
                {
                    for callback in root_node.get_callbacks().iter() {
                        if callback.event == event_filter {
                            callbacks.push(callback.clone());
                        }
                    }
                }
            }
            callbacks
        }
    };

    if callback_data_list.is_empty() {
        return Vec::new();
    }

    // Invoke all collected callbacks
    let window_handle = window.get_raw_window_handle();
    let layout_window = match window.layout_window.as_mut() {
        Some(lw) => lw,
        None => return Vec::new(),
    };

    let mut results = Vec::new();
    let mut fc_cache_clone = (*window.fc_cache).clone();

    for callback_data in callback_data_list {
        let mut callback =
            azul_layout::callbacks::Callback::from_core(callback_data.callback.clone());

        let callback_result = layout_window.invoke_single_callback(
            &mut callback,
            &mut callback_data.data.clone(),
            &window_handle,
            &window.gl_context_ptr,
            &mut window.image_cache,
            &mut fc_cache_clone,
            &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
            &window.previous_window_state,
            &window.current_window_state,
            &window.renderer_resources,
        );

        results.push(callback_result);
    }

    results
}

/// Process a single callback result and update window state
pub fn process_callback_result(
    window: &mut Win32Window,
    result: &CallCallbacksResult,
) -> ProcessEventResult {
    use azul_core::callbacks::Update;

    let mut event_result = ProcessEventResult::DoNothing;

    // Handle window state modifications
    if let Some(ref modified_state) = result.modified_window_state {
        window.current_window_state.title = modified_state.title.clone();
        window.current_window_state.size = modified_state.size;
        window.current_window_state.position = modified_state.position;
        window.current_window_state.flags = modified_state.flags;
        window.current_window_state.background_color = modified_state.background_color;

        // Check if window should close
        if modified_state.flags.close_requested {
            window.is_open = false;
            return ProcessEventResult::DoNothing;
        }

        event_result = ProcessEventResult::ShouldReRenderCurrentWindow;
    }

    // Handle focus changes
    if let Some(new_focus) = result.update_focused_node {
        window.current_window_state.focused_node = new_focus;
        event_result = ProcessEventResult::ShouldReRenderCurrentWindow;
    }

    // Handle image updates
    if result.images_changed.is_some() || result.image_masks_changed.is_some() {
        event_result = event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
    }

    // Handle timers, threads, etc.
    if result.timers.is_some()
        || result.timers_removed.is_some()
        || result.threads.is_some()
        || result.threads_removed.is_some()
    {
        event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
    }

    // Process Update screen command
    match result.callbacks_update_screen {
        Update::RefreshDom => {
            if let Err(e) = window.regenerate_layout() {
                eprintln!("Layout regeneration error: {}", e);
            }
            event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        Update::RefreshDomAllWindows => {
            if let Err(e) = window.regenerate_layout() {
                eprintln!("Layout regeneration error: {}", e);
            }
            event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
        }
        Update::DoNothing => {}
    }

    event_result
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

impl PartialOrd for ProcessEventResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ProcessEventResult {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use ProcessEventResult::*;
        let self_priority = match self {
            DoNothing => 0,
            ShouldReRenderCurrentWindow => 1,
            ShouldUpdateDisplayListCurrentWindow => 2,
            UpdateHitTesterAndProcessAgain => 3,
            ShouldRegenerateDomCurrentWindow => 4,
            ShouldRegenerateDomAllWindows => 5,
        };
        let other_priority = match other {
            DoNothing => 0,
            ShouldReRenderCurrentWindow => 1,
            ShouldUpdateDisplayListCurrentWindow => 2,
            UpdateHitTesterAndProcessAgain => 3,
            ShouldRegenerateDomCurrentWindow => 4,
            ShouldRegenerateDomAllWindows => 5,
        };
        self_priority.cmp(&other_priority)
    }
}
