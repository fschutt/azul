//! Common callback result processing for all platforms.
//!
//! This module provides a centralized implementation for processing CallCallbacksResult
//! across all windowing systems (Windows, macOS, X11, Wayland).
//!
//! Key responsibilities:
//! - Extract and apply window state changes
//! - Handle timer add/remove operations
//! - Handle thread add/remove operations
//! - Process Update screen commands
//! - Return appropriate ProcessEventResult

use std::collections::{BTreeMap, BTreeSet, HashMap};

use azul_core::{callbacks::Update, events::ProcessEventResult};
use azul_layout::{
    callbacks::CallCallbacksResult, thread::Thread, timer::Timer, window_state::FullWindowState,
};

/// Result of processing callback results
pub struct CallbackProcessingResult {
    pub event_result: ProcessEventResult,
    pub timers_added: HashMap<usize, Timer>,
    pub timers_removed: BTreeSet<usize>,
    pub threads_added: BTreeMap<azul_core::task::ThreadId, Thread>,
    pub threads_removed: BTreeSet<azul_core::task::ThreadId>,
    pub should_close: bool,
}

impl Default for CallbackProcessingResult {
    fn default() -> Self {
        Self {
            event_result: ProcessEventResult::DoNothing,
            timers_added: HashMap::new(),
            timers_removed: BTreeSet::new(),
            threads_added: BTreeMap::new(),
            threads_removed: BTreeSet::new(),
            should_close: false,
        }
    }
}

/// Process callback results and extract all state changes.
///
/// This function does NOT directly modify window state - it returns
/// structured data that the caller should apply to their window.
///
/// # Returns
///
/// - `CallbackProcessingResult` containing all extracted changes
pub fn process_callback_results(
    callback_result: &CallCallbacksResult,
    current_window_state: &mut FullWindowState,
) -> CallbackProcessingResult {
    let mut result = CallbackProcessingResult::default();

    // Handle window state modifications
    if let Some(ref modified_state) = callback_result.modified_window_state {
        current_window_state.title = modified_state.title.clone();
        current_window_state.theme = modified_state.theme.clone();
        current_window_state.size = modified_state.size;
        current_window_state.position = modified_state.position;
        current_window_state.flags = modified_state.flags;
        current_window_state.debug_state = modified_state.debug_state.clone();
        current_window_state.keyboard_state = modified_state.keyboard_state.clone();
        current_window_state.mouse_state = modified_state.mouse_state.clone();
        current_window_state.touch_state = modified_state.touch_state.clone();
        current_window_state.ime_position = modified_state.ime_position;
        current_window_state.monitor_id = modified_state.monitor_id;
        current_window_state.platform_specific_options =
            modified_state.platform_specific_options.clone();
        current_window_state.renderer_options = modified_state.renderer_options;
        current_window_state.background_color = modified_state.background_color;
        current_window_state.layout_callback = modified_state.layout_callback.clone();
        current_window_state.close_callback = modified_state.close_callback.clone();

        // Check if window should close
        if modified_state.flags.close_requested {
            result.should_close = true;
            return result;
        }

        result.event_result = result
            .event_result
            .max(ProcessEventResult::ShouldReRenderCurrentWindow);
    }

    // Handle focus changes
    if callback_result.update_focused_node.is_change() {
        result.event_result = result
            .event_result
            .max(ProcessEventResult::ShouldReRenderCurrentWindow);
    }

    // Handle image updates
    if callback_result.images_changed.is_some() || callback_result.image_masks_changed.is_some() {
        result.event_result = result
            .event_result
            .max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
    }

    // Extract timers added/removed
    if let Some(ref timers) = callback_result.timers {
        result.timers_added = timers
            .iter()
            .map(|(id, timer)| (id.id, timer.clone()))
            .collect();
    }

    if let Some(ref timers_removed) = callback_result.timers_removed {
        result.timers_removed = timers_removed.iter().map(|id| id.id).collect();
    }

    // Extract threads added/removed
    if let Some(ref threads) = callback_result.threads {
        result.threads_added = threads.clone();
    }

    if let Some(ref threads_removed) = callback_result.threads_removed {
        result.threads_removed = threads_removed.clone();
    }

    // Process Update screen command
    match callback_result.callbacks_update_screen {
        Update::RefreshDom => {
            result.event_result = result
                .event_result
                .max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        Update::RefreshDomAllWindows => {
            result.event_result = result
                .event_result
                .max(ProcessEventResult::ShouldRegenerateDomAllWindows);
        }
        Update::DoNothing => {}
    }

    result
}
