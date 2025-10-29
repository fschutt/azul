//! Cross-platform V2 event processing system
//!
//! This module contains the unified event processing logic that is shared across all platforms
//! (macOS, Windows, X11, Wayland). The V2 system uses state-diffing between frames to detect
//! events, eliminating platform-specific event handling differences.
//!
//! Previously, this logic was duplicated ~4 times across the platform modules. It is now
//! centralized here and accessed via the `PlatformWindowV2` trait.

use alloc::sync::Arc;
use core::cell::RefCell;

use azul_core::{
    callbacks::{CallbackInfo, LayoutCallbackInfo},
    dom::DomId,
    events::{Event, EventFilter, ProcessEventResult},
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::ImageCache,
    window::{CursorPosition, WindowFlags},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    window::LayoutWindow,
    window_state::FullWindowState,
    RendererResources,
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2;

/// Trait that platform-specific window types must implement to use the unified V2 event system.
///
/// This trait provides access to the cross-platform state needed for event processing,
/// layout regeneration, and callback invocation. By implementing this trait, platform
/// windows can use the shared event processing logic in this module.
pub trait PlatformWindowV2 {
    /// Get mutable access to the layout window
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow>;
    
    /// Get immutable access to the layout window
    fn get_layout_window(&self) -> Option<&LayoutWindow>;
    
    /// Get the current window state
    fn get_current_window_state(&self) -> &FullWindowState;
    
    /// Get mutable access to the current window state
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState;
    
    /// Get the previous window state (if available)
    fn get_previous_window_state(&self) -> Option<&FullWindowState>;
    
    /// Set the previous window state
    fn set_previous_window_state(&mut self, state: FullWindowState);
    
    /// Get mutable access to shared resources
    fn get_resources_mut(&mut self) -> (&mut ImageCache, &mut RendererResources);
    
    /// Get the font cache
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;
    
    /// Get the OpenGL context pointer
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;
    
    /// Get the system style
    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle>;
    
    /// Get the shared application data
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;
    
    /// Get window flags
    fn get_flags(&self) -> &WindowFlags;
    
    /// Mark that the frame needs regeneration
    fn mark_frame_needs_regeneration(&mut self);
}

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events, keys)
    RootNodes,
}

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Process all window events using the V2 state-diffing system.
///
/// This is the main entry point for event processing. It compares the current and previous
/// window states to detect events, then dispatches callbacks for those events.
///
/// # Returns
/// * `ProcessEventResult` - Indicates whether the DOM needs regeneration, display list update, etc.
pub fn process_window_events_v2<W: PlatformWindowV2>(window: &mut W) -> ProcessEventResult {
    use azul_layout::window_state::create_events_from_states;

    // Compare current and previous states to generate events
    let previous_state = match window.get_previous_window_state() {
        Some(prev) => prev.clone(),
        None => {
            // First frame - no events yet
            window.set_previous_window_state(window.get_current_window_state().clone());
            return ProcessEventResult::DoNothing;
        }
    };

    let current_state = window.get_current_window_state().clone();
    let events = create_events_from_states(&current_state, &previous_state);

    // Process all detected events
    let result = process_window_events_recursive_v2(window, events);

    // Update previous state for next frame
    window.set_previous_window_state(current_state);

    result
}

/// Recursively process events by dispatching callbacks and handling their results.
///
/// This function implements the event processing loop:
/// 1. Invoke callbacks for all detected events
/// 2. Handle the result (regenerate DOM, update display list, etc.)
/// 3. If DOM was regenerated, recalculate hit-test and recurse
fn process_window_events_recursive_v2<W: PlatformWindowV2>(
    window: &mut W,
    events: azul_core::events::Events,
) -> ProcessEventResult {
    // Invoke callbacks for all events
    let callback_result = invoke_callbacks_v2(window, &events);

    // Handle the callback result
    let result = process_callback_result_v2(window, callback_result);

    // If DOM was regenerated, we need to recalculate hit-test and potentially process more events
    match result {
        ProcessEventResult::UpdateHitTesterAndProcessAgain => {
            // Recalculate hit-test at current mouse position
            if let Some(layout_window) = window.get_layout_window() {
                let mouse_state = &window.get_current_window_state().mouse_state;
                let cursor_position = mouse_state.cursor_position;
                
                let new_hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
                    &*crate::desktop::wr_translate2::AsyncHitTester::resolve_dummy(), // TODO: Pass real hit tester
                    None, // TODO: Pass real document_id
                    window.get_current_window_state().focused_node,
                    &layout_window.layout_results,
                    &cursor_position,
                    window.get_current_window_state().size.get_hidpi_factor(),
                );
                
                window.get_current_window_state_mut().last_hit_test = new_hit_test;
            }

            // Generate new events based on updated state
            let previous_state = window.get_previous_window_state()
                .cloned()
                .unwrap_or_else(|| window.get_current_window_state().clone());
            let new_events = azul_layout::window_state::create_events_from_states(
                window.get_current_window_state(),
                &previous_state,
            );

            // Recurse with new events
            process_window_events_recursive_v2(window, new_events)
        }
        other => other,
    }
}

/// Invoke all callbacks for the detected events.
///
/// This function walks through all detected events and invokes the appropriate callbacks
/// on the appropriate nodes (either specific hovered nodes or root nodes).
fn invoke_callbacks_v2<W: PlatformWindowV2>(
    window: &mut W,
    events: &azul_core::events::Events,
) -> ProcessEventResult {
    use azul_core::events::EventFilter;

    let mut overall_result = ProcessEventResult::DoNothing;

    // Process window-level events (on root nodes)
    for event_filter in &events.window_events {
        let callback_result = invoke_callbacks_for_target_v2(
            window,
            *event_filter,
            CallbackTarget::RootNodes,
        );
        overall_result = overall_result.max(callback_result);
    }

    // Process hover events (on hovered nodes)
    if let Some(layout_window) = window.get_layout_window() {
        let hovered_nodes = &window.get_current_window_state().last_hit_test.hovered_nodes;
        
        for (dom_id, node_hit_test) in hovered_nodes {
            for (node_id, _hit_item) in &node_hit_test.regular_hit_test_nodes {
                let target = CallbackTarget::Node(HitTestNode {
                    dom_id: dom_id.inner as u64,
                    node_id: node_id.index() as u64,
                });

                for event_filter in &events.hover_events {
                    let callback_result = invoke_callbacks_for_target_v2(
                        window,
                        *event_filter,
                        target,
                    );
                    overall_result = overall_result.max(callback_result);
                }
            }
        }
    }

    // Process focus events (on focused node)
    if let Some(focused_node) = window.get_current_window_state().focused_node {
        if let Some(layout_window) = window.get_layout_window() {
            // Find which DOM owns this focused node
            for (dom_id, layout_result) in &layout_window.layout_results {
                let target = CallbackTarget::Node(HitTestNode {
                    dom_id: dom_id.inner as u64,
                    node_id: focused_node.index() as u64,
                });

                for event_filter in &events.focus_events {
                    let callback_result = invoke_callbacks_for_target_v2(
                        window,
                        *event_filter,
                        target,
                    );
                    overall_result = overall_result.max(callback_result);
                }
            }
        }
    }

    overall_result
}

/// Invoke callbacks for a specific target (node or root nodes).
fn invoke_callbacks_for_target_v2<W: PlatformWindowV2>(
    window: &mut W,
    event_filter: EventFilter,
    target: CallbackTarget,
) -> ProcessEventResult {
    let layout_window = match window.get_layout_window_mut() {
        Some(lw) => lw,
        None => return ProcessEventResult::DoNothing,
    };

    let mut overall_result = ProcessEventResult::DoNothing;

    // Build the event to pass to callbacks
    let event = Event {
        filter: event_filter,
    };

    match target {
        CallbackTarget::Node(hit_node) => {
            // Invoke callbacks on specific node
            let dom_id = DomId { inner: hit_node.dom_id as usize };
            
            if let Some(layout_result) = layout_window.layout_results.get_mut(&dom_id) {
                let node_id = azul_core::id::NodeId::from_usize(hit_node.node_id as usize);
                
                if let Some(node_id) = node_id {
                    if let Some(node_data) = layout_result.styled_dom.node_data.as_container_mut().get_mut(node_id) {
                        // Invoke all callbacks on this node
                        for callback in node_data.callbacks.as_slice() {
                            if callback.event == event_filter {
                                let result = invoke_single_callback(
                                    window,
                                    &event,
                                    callback.callback,
                                    dom_id,
                                    node_id,
                                );
                                overall_result = overall_result.max(result);
                            }
                        }
                    }
                }
            }
        }
        CallbackTarget::RootNodes => {
            // Invoke callbacks on root nodes (NodeId::ZERO) across all DOMs
            for (dom_id, layout_result) in layout_window.layout_results.iter_mut() {
                let root_node_id = azul_core::id::NodeId::ZERO;
                
                if let Some(node_data) = layout_result.styled_dom.node_data.as_container_mut().get_mut(root_node_id) {
                    for callback in node_data.callbacks.as_slice() {
                        if callback.event == event_filter {
                            let result = invoke_single_callback(
                                window,
                                &event,
                                callback.callback,
                                *dom_id,
                                root_node_id,
                            );
                            overall_result = overall_result.max(result);
                        }
                    }
                }
            }
        }
    }

    overall_result
}

/// Invoke a single callback and return its result.
fn invoke_single_callback<W: PlatformWindowV2>(
    window: &mut W,
    event: &Event,
    callback: azul_core::callbacks::CoreCallback,
    dom_id: DomId,
    node_id: azul_core::id::NodeId,
) -> ProcessEventResult {
    // Build CallbackInfo
    let (image_cache, renderer_resources) = window.get_resources_mut();
    
    let mut callback_info = CallbackInfo {
        state: window.get_current_window_state().clone().into(),
        current_window_state: window.get_current_window_state().clone(),
        previous_window_state: window.get_previous_window_state().cloned(),
        modifiable_window_state: azul_core::window::WindowStateArgs::default(), // TODO: Proper implementation
        gl_context: window.get_gl_context_ptr().clone(),
        image_cache,
        system_fonts: window.get_fc_cache().clone(),
        renderer_resources,
        timers: alloc::vec::Vec::new(), // TODO
        threads: alloc::vec::Vec::new(), // TODO
        new_windows: alloc::vec::Vec::new(),
        system_callbacks: ExternalSystemCallbacks::rust_internal(),
        stop_propagation: false,
        focus_target: None,
        words_changed_in_callbacks: alloc::collections::BTreeMap::new(),
        images_changed_in_callbacks: alloc::collections::BTreeMap::new(),
        image_masks_changed_in_callbacks: alloc::collections::BTreeMap::new(),
        css_properties_changed_in_callbacks: alloc::collections::BTreeMap::new(),
        current_scroll_states: alloc::collections::BTreeMap::new(), // TODO
        nodes_scrolled_in_callback: alloc::collections::BTreeMap::new(),
        hit_dom_node: (dom_id, node_id),
        cursor_relative_to_item: None, // TODO
        cursor_in_viewport: None, // TODO
    };

    // Invoke callback
    let update = match callback {
        azul_core::callbacks::CoreCallback::Core(core_cb) => {
            (core_cb.cb)(window.get_app_data(), &mut callback_info)
        }
        azul_core::callbacks::CoreCallback::Marshaled(marshaled_cb) => {
            (marshaled_cb.cb.cb)(
                &mut marshaled_cb.marshal_data.clone(),
                window.get_app_data(),
                &mut callback_info,
            )
        }
    };

    // Convert Update to ProcessEventResult
    match update {
        azul_core::callbacks::Update::DoNothing => ProcessEventResult::DoNothing,
        azul_core::callbacks::Update::RefreshDom => ProcessEventResult::ShouldRegenerateDomCurrentWindow,
        azul_core::callbacks::Update::RefreshDomAllWindows => ProcessEventResult::ShouldRegenerateDomAllWindows,
    }
}

/// Handle the result of callback processing.
///
/// This function decides what action to take based on the callback result:
/// - Regenerate DOM and layout
/// - Update display list only
/// - Do nothing
fn process_callback_result_v2<W: PlatformWindowV2>(
    window: &mut W,
    result: ProcessEventResult,
) -> ProcessEventResult {
    match result {
        ProcessEventResult::DoNothing => ProcessEventResult::DoNothing,
        
        ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
            // Regenerate layout (this will be implemented in layout_v2.rs)
            window.mark_frame_needs_regeneration();
            ProcessEventResult::UpdateHitTesterAndProcessAgain
        }
        
        ProcessEventResult::ShouldRegenerateDomAllWindows => {
            // TODO: Signal to event loop to regenerate all windows
            window.mark_frame_needs_regeneration();
            ProcessEventResult::UpdateHitTesterAndProcessAgain
        }
        
        ProcessEventResult::ShouldReRenderCurrentWindow => {
            window.mark_frame_needs_regeneration();
            ProcessEventResult::ShouldReRenderCurrentWindow
        }
        
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
            window.mark_frame_needs_regeneration();
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
        }
        
        ProcessEventResult::UpdateHitTesterAndProcessAgain => {
            ProcessEventResult::UpdateHitTesterAndProcessAgain
        }
    }
}
