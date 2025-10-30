//! Cross-platform V2 event processing system
//!
//! This module contains the **complete unified event processing logic** that is shared across all platforms
//! (macOS, Windows, X11, Wayland). The V2 system uses state-diffing between frames to detect
//! events, eliminating platform-specific event handling differences.
//!
//! ## Architecture
//!
//! The `PlatformWindowV2` trait provides **default implementations** for all complex logic:
//! - Event processing (state diffing via `process_window_events()`)
//! - Callback invocation (`invoke_callbacks_v2()`)
//! - Callback result handling (`process_callback_result_v2()`)
//! - Hit testing (`perform_scrollbar_hit_test()`)
//! - Scrollbar interaction (`handle_scrollbar_click()`, `handle_scrollbar_drag()`)
//!
//! Platform implementations only need to:
//! 1. Implement simple getter methods to access their window state
//! 2. Call `process_window_events()` after updating platform state
//! 3. Update the screen based on the returned `ProcessEventResult`
//!
//! Previously, this logic was duplicated ~4 times (~3000 lines) across:
//! - `macos/events.rs` (~2000 lines)
//! - `windows/process.rs` (~1800 lines)
//! - `linux/x11/events.rs` (~1900 lines)
//! - `linux/wayland/mod.rs` (~1500 lines)

use alloc::sync::Arc;
use core::cell::RefCell;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::DomId,
    events::{dispatch_events, CallbackTarget as CoreCallbackTarget, EventFilter, ProcessEventResult},
    geom::LogicalPosition,
    gl::*,
    hit_test::{DocumentId, PipelineId},
    id::NodeId as CoreNodeId,
    refany::RefAny,
    resources::{ImageCache, IdNamespace, RendererResources},
    window::RawWindowHandle,
};

use azul_layout::{
    callbacks::{CallCallbacksResult, CallbackInfo, Callback as LayoutCallback, ExternalSystemCallbacks},
    hit_test::FullHitTest,
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{create_events_from_states, FullWindowState},
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2::{self, AsyncHitTester, WrRenderApi};

/// Maximum depth for recursive event processing (prevents infinite loops from callbacks)
const MAX_EVENT_RECURSION_DEPTH: usize = 5;

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events, keys)
    RootNodes,
}

/// Trait that platform-specific window types must implement to use the unified V2 event system.
///
/// This trait provides **default implementations** for all complex cross-platform logic.
/// Platform implementations only need to implement the simple getter methods.
///
/// ## Required Methods (Simple Getters)
///
/// Platforms must implement these methods to expose their internal state:
/// - Layout window access (`get_layout_window`, `get_layout_window_mut`)
/// - Window state access (`get_current_window_state`, `get_previous_window_state`, etc.)
/// - Resource access (`get_image_cache_mut`, `get_renderer_resources_mut`, etc.)
/// - Hit testing state (`get_hit_tester`, `get_scrollbar_drag_state`, etc.)
///
/// ## Provided Methods (Complete Logic)
///
/// These methods have default implementations with the full cross-platform logic:
/// - `process_window_events()` - Main event processing entry point
/// - `invoke_callbacks_v2()` - Callback dispatch
/// - `process_callback_result_v2()` - Handle callback results
/// - `perform_scrollbar_hit_test()` - Scrollbar interaction
/// - `handle_scrollbar_click()` - Scrollbar click handling
/// - `handle_scrollbar_drag()` - Scrollbar drag handling
///
pub trait PlatformWindowV2 {
    // =========================================================================
    // REQUIRED: Simple Getter Methods (Platform Must Implement)
    // =========================================================================
    
    // === Layout Window Access ===
    
    /// Get mutable access to the layout window
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow>;
    
    /// Get immutable access to the layout window
    fn get_layout_window(&self) -> Option<&LayoutWindow>;
    
    // === Window State Access ===
    
    /// Get the current window state
    fn get_current_window_state(&self) -> &FullWindowState;
    
    /// Get mutable access to the current window state
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState;
    
    /// Get the previous window state (if available)
    fn get_previous_window_state(&self) -> &Option<FullWindowState>;
    
    /// Set the previous window state
    fn set_previous_window_state(&mut self, state: FullWindowState);
    
    // === Resource Access ===
    
    /// Get mutable access to image cache
    fn get_image_cache_mut(&mut self) -> &mut ImageCache;
    
    /// Get mutable access to renderer resources
    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources;
    
    /// Get the font cache
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;
    
    /// Get the OpenGL context pointer
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;
    
    /// Get the system style
    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle>;
    
    /// Get the shared application data
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;
    
    // === Scrollbar State ===
    
    /// Get the current scrollbar drag state
    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState>;
    
    /// Get mutable access to scrollbar drag state
    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState>;
    
    /// Set scrollbar drag state
    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>);
    
    // === Hit Testing ===
    
    /// Get the async hit tester
    fn get_hit_tester(&self) -> &AsyncHitTester;
    
    /// Get mutable access to hit tester
    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester;
    
    /// Get the last hovered node
    fn get_last_hovered_node(&self) -> Option<&HitTestNode>;
    
    /// Set the last hovered node
    fn set_last_hovered_node(&mut self, node: Option<HitTestNode>);
    
    // === WebRender Infrastructure ===
    
    /// Get the document ID
    fn get_document_id(&self) -> DocumentId;
    
    /// Get the ID namespace
    fn get_id_namespace(&self) -> IdNamespace;
    
    /// Get the render API
    fn get_render_api(&self) -> &WrRenderApi;
    
    /// Get mutable access to render API
    fn get_render_api_mut(&mut self) -> &mut WrRenderApi;
    
    /// Get the renderer (if available)
    fn get_renderer(&self) -> Option<&webrender::Renderer>;
    
    /// Get mutable access to renderer
    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer>;
    
    // === Timers and Threads ===
    
    /// Get raw window handle for spawning child windows
    fn get_raw_window_handle(&self) -> RawWindowHandle;
    
    // === Frame Regeneration ===
    
    /// Check if frame needs regeneration
    fn needs_frame_regeneration(&self) -> bool;
    
    /// Mark that the frame needs regeneration
    fn mark_frame_needs_regeneration(&mut self);
    
    /// Clear frame regeneration flag
    fn clear_frame_regeneration_flag(&mut self);
    
    // === Callback Invocation ===
    
    /// Invoke callbacks for a given target and event filter.
    ///
    /// **Platform Implementation Required**: This method MUST be implemented by each platform
    /// because it requires direct field access to avoid borrow checker issues with trait methods.
    ///
    /// ## Implementation Pattern
    ///
    /// ```rust,ignore
    /// fn invoke_callbacks_v2(&mut self, target: CallbackTarget, event_filter: EventFilter) -> Vec<CallCallbacksResult> {
    ///     // 1. Collect callbacks from NodeData
    ///     let callback_data_list = match target {
    ///         CallbackTarget::Node(node) => { /* collect from node */ }
    ///         CallbackTarget::RootNodes => { /* collect from all roots */ }
    ///     };
    ///     
    ///     // 2. Prepare for callback invocation
    ///     let window_handle = self.get_raw_window_handle();
    ///     let mut fc_cache_clone = (*self.fc_cache).clone();
    ///     
    ///     // 3. Invoke each callback using layout_window.invoke_single_callback()
    ///     let mut results = Vec::new();
    ///     for callback_data in callback_data_list {
    ///         let mut callback = Callback::from_core(callback_data.callback);
    ///         let result = self.layout_window.as_mut().unwrap().invoke_single_callback(
    ///             &mut callback,
    ///             &mut callback_data.data.clone(),
    ///             &window_handle,
    ///             &self.gl_context_ptr,
    ///             &mut self.image_cache,
    ///             &mut fc_cache_clone,
    ///             self.system_style.clone(),
    ///             &ExternalSystemCallbacks::rust_internal(),
    ///             &self.previous_window_state,
    ///             &self.current_window_state,
    ///             &self.renderer_resources,
    ///         );
    ///         results.push(result);
    ///     }
    ///     results
    /// }
    /// ```
    fn invoke_callbacks_v2(
        &mut self,
        target: CallbackTarget,
        event_filter: EventFilter,
    ) -> Vec<CallCallbacksResult>;
    
    // =========================================================================
    // PROVIDED: Complete Logic (Default Implementations)
    // =========================================================================
    
    /// GPU-accelerated smooth scrolling.
    ///
    /// This applies a scroll delta to a node and updates WebRender's display list
    /// for smooth GPU-based scrolling.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM ID containing the scrollable node
    /// * `node_id` - The scrollable node ID
    /// * `delta_x` - Horizontal scroll delta (pixels)
    /// * `delta_y` - Vertical scroll delta (pixels)
    ///
    /// ## Returns
    /// * `Ok(())` - Scroll applied successfully
    /// * `Err(msg)` - Error message if scroll failed
    fn gpu_scroll(
        &mut self,
        dom_id: u64,
        node_id: u64,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EasingFunction, EventSource},
            geom::LogicalPosition,
        };
        use azul_layout::scroll::ScrollEvent;

        let layout_window = self.get_layout_window_mut().ok_or("No layout window")?;

        let dom_id_typed = DomId {
            inner: dom_id as usize,
        };
        let node_id_typed = node_id as u32;

        // Create scroll event
        let scroll_event = ScrollEvent {
            dom_id: dom_id_typed,
            node_id: NodeId::new(node_id_typed as usize),
            delta: LogicalPosition::new(delta_x, delta_y),
            source: EventSource::User,
            duration: None, // Instant scroll
            easing: EasingFunction::Linear,
        };

        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

        // Apply scroll
        layout_window.scroll_states.scroll_by(
            scroll_event.dom_id,
            scroll_event.node_id,
            scroll_event.delta,
            scroll_event
                .duration
                .unwrap_or(azul_core::task::Duration::System(
                    azul_core::task::SystemTimeDiff { secs: 0, nanos: 0 },
                )),
            scroll_event.easing,
            (external.get_system_time_fn.cb)(),
        );

        self.mark_frame_needs_regeneration();
        Ok(())
    }
    
    /// Process all window events using the V2 state-diffing system.
    ///
    /// This is the **main entry point** for event processing. Call this after updating
    /// the current window state with platform events.
    ///
    /// ## Workflow
    /// 1. Compare current vs previous window state to detect events
    /// 2. Use `dispatch_events()` to determine which callbacks to invoke
    /// 3. Invoke callbacks and collect results
    /// 4. Handle callback results (regenerate DOM, update display list, etc.)
    /// 5. Recurse if needed (DOM was regenerated)
    ///
    /// ## Returns
    /// * `ProcessEventResult` - Tells the platform what action to take (redraw, close, etc.)
    fn process_window_events(&mut self) -> ProcessEventResult {
        self.process_window_events_recursive_v2(0)
    }
    
    /// V2: Recursive event processing with depth limit.
    ///
    /// This implements the complete event processing workflow with recursion
    /// for cases where callbacks regenerate the DOM.
    fn process_window_events_recursive_v2(&mut self, depth: usize) -> ProcessEventResult {
        if depth >= MAX_EVENT_RECURSION_DEPTH {
            eprintln!(
                "[PlatformWindowV2] Max event recursion depth {} reached",
                MAX_EVENT_RECURSION_DEPTH
            );
            return ProcessEventResult::DoNothing;
        }

        // Get previous state (or use current as fallback for first frame)
        let previous_state = self
            .get_previous_window_state()
            .as_ref()
            .unwrap_or(self.get_current_window_state());

        // Detect all events that occurred by comparing states
        let events = create_events_from_states(self.get_current_window_state(), previous_state);

        if events.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        // Get hit test if available
        let hit_test = if !self.get_current_window_state().last_hit_test.is_empty() {
            Some(&self.get_current_window_state().last_hit_test)
        } else {
            None
        };

        // Use cross-platform dispatch logic to determine which callbacks to invoke
        let dispatch_result = dispatch_events(&events, hit_test);

        if dispatch_result.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        // Invoke all callbacks and collect results
        let mut result = ProcessEventResult::DoNothing;
        let mut should_stop_propagation = false;
        let mut should_recurse = false;

        for callback_to_invoke in &dispatch_result.callbacks {
            if should_stop_propagation {
                break;
            }

            // Convert core CallbackTarget to shell CallbackTarget
            let target = match &callback_to_invoke.target {
                CoreCallbackTarget::Node { dom_id, node_id } => CallbackTarget::Node(HitTestNode {
                    dom_id: dom_id.inner as u64,
                    node_id: node_id.index() as u64,
                }),
                CoreCallbackTarget::RootNodes => CallbackTarget::RootNodes,
            };

            // Invoke callbacks and collect results
            let callback_results = self.invoke_callbacks_v2(target, callback_to_invoke.event_filter);

            for callback_result in callback_results {
                let event_result = self.process_callback_result_v2(&callback_result);
                result = result.max(event_result);

                // Check if we should stop propagation
                if callback_result.stop_propagation {
                    should_stop_propagation = true;
                    break;
                }

                // Check if we need to recurse (DOM was regenerated)
                use azul_core::callbacks::Update;
                if matches!(
                    callback_result.callbacks_update_screen,
                    Update::RefreshDom | Update::RefreshDomAllWindows
                ) {
                    should_recurse = true;
                }
            }
        }

        // Recurse if needed
        if should_recurse && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            let recursive_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(recursive_result);
        }

        result
    }
    
    /// V2: Process callback result and determine what action to take.
    ///
    /// This converts the callback result into a `ProcessEventResult` that tells
    /// the platform what to do next (redraw, regenerate layout, etc.).
    ///
    /// This method handles:
    /// - Window state modifications (title, size, position, flags)
    /// - Focus changes
    /// - Image/image mask updates
    /// - Timer/thread management
    /// - New window creation
    /// - DOM regeneration triggering
    fn process_callback_result_v2(&mut self, result: &CallCallbacksResult) -> ProcessEventResult {
        use azul_core::callbacks::Update;

        let mut event_result = ProcessEventResult::DoNothing;

        // Handle window state modifications
        if let Some(ref modified_state) = result.modified_window_state {
            let current_state = self.get_current_window_state_mut();
            current_state.title = modified_state.title.clone();
            current_state.size = modified_state.size;
            current_state.position = modified_state.position;
            current_state.flags = modified_state.flags;
            current_state.background_color = modified_state.background_color;

            // Check if window should close
            if modified_state.flags.close_requested {
                // Platform should handle window destruction
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle focus changes
        if let Some(new_focus) = result.update_focused_node {
            self.get_current_window_state_mut().focused_node = new_focus;
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle image updates
        if result.images_changed.is_some() || result.image_masks_changed.is_some() {
            event_result =
                event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
        }

        // Handle timers and threads
        if result.timers.is_some()
            || result.timers_removed.is_some()
            || result.threads.is_some()
            || result.threads_removed.is_some()
        {
            // Process timers and threads through the layout_window
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Add new timers
                if let Some(timers) = &result.timers {
                    for (timer_id, timer) in timers {
                        layout_window.timers.insert(*timer_id, timer.clone());
                    }
                }

                // Remove old timers
                if let Some(timers_removed) = &result.timers_removed {
                    for timer_id in timers_removed {
                        layout_window.timers.remove(timer_id);
                    }
                }

                // Add new threads
                if let Some(threads) = &result.threads {
                    for (thread_id, thread) in threads {
                        layout_window.threads.insert(*thread_id, thread.clone());
                    }
                }

                // Remove old threads
                if let Some(threads_removed) = &result.threads_removed {
                    for thread_id in threads_removed {
                        layout_window.threads.remove(thread_id);
                    }
                }
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle new windows spawned in callbacks
        if !result.windows_created.is_empty() {
            // TODO: Signal to event loop to create new windows
            // For now, just log
            eprintln!(
                "[PlatformWindowV2] {} new windows requested (not yet implemented)",
                result.windows_created.len()
            );
        }

        // Process Update screen command
        match result.callbacks_update_screen {
            Update::DoNothing => {}
            Update::RefreshDom => {
                self.mark_frame_needs_regeneration();
                event_result =
                    event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
            Update::RefreshDomAllWindows => {
                self.mark_frame_needs_regeneration();
                event_result =
                    event_result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            }
        }

        event_result
    }
    
    /// Perform scrollbar hit-test at the given position.
    ///
    /// Returns `Some(ScrollbarHitId)` if a scrollbar was hit, `None` otherwise.
    ///
    /// This uses WebRender's hit-tester to check for scrollbar tags.
    fn perform_scrollbar_hit_test(
        &self,
        position: azul_core::geom::LogicalPosition,
    ) -> Option<azul_core::hit_test::ScrollbarHitId> {
        use webrender::api::units::WorldPoint;

        let hit_tester = match self.get_hit_tester() {
            AsyncHitTester::Resolved(ht) => ht,
            _ => return None,
        };

        let world_point = WorldPoint::new(position.x, position.y);
        let hit_result = hit_tester.hit_test(world_point);

        // Check each hit item for scrollbar tag
        for item in hit_result.items.iter() {
            if let Some(scrollbar_id) =
                wr_translate2::translate_item_tag_to_scrollbar_hit_id(item.tag)
            {
                return Some(scrollbar_id);
            }
        }

        None
    }
    
    /// Handle scrollbar click (thumb or track).
    ///
    /// Returns `ProcessEventResult` indicating whether to redraw.
    fn handle_scrollbar_click(
        &mut self,
        hit_id: azul_core::hit_test::ScrollbarHitId,
        position: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::hit_test::ScrollbarHitId;

        match hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id)
            | ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                // Start drag
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return ProcessEventResult::DoNothing,
                };

                let scroll_offset = layout_window
                    .scroll_states
                    .get_current_offset(dom_id, node_id)
                    .unwrap_or_default();

                self.set_scrollbar_drag_state(Some(ScrollbarDragState {
                    hit_id,
                    initial_mouse_pos: position,
                    initial_scroll_offset: scroll_offset,
                }));

                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            ScrollbarHitId::VerticalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, true)
            }

            ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, false)
            }
        }
    }
    
    /// Handle track click - jump scroll to clicked position.
    fn handle_track_click(
        &mut self,
        dom_id: DomId,
        node_id: CoreNodeId,
        click_position: azul_core::geom::LogicalPosition,
        is_vertical: bool,
    ) -> ProcessEventResult {
        use azul_layout::scroll::ScrollbarOrientation;

        // Get scrollbar state to calculate target position
        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Get current scrollbar geometry
        let scrollbar_state = if is_vertical {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return ProcessEventResult::DoNothing,
        };

        // Get current scroll state
        let scroll_state = match layout_window
            .scroll_states
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate which position on the track was clicked (0.0 = top/left, 1.0 = bottom/right)
        let click_ratio = if is_vertical {
            let track_top = scrollbar_state.track_rect.origin.y;
            let track_height = scrollbar_state.track_rect.size.height;
            ((click_position.y - track_top) / track_height).clamp(0.0, 1.0)
        } else {
            let track_left = scrollbar_state.track_rect.origin.x;
            let track_width = scrollbar_state.track_rect.size.width;
            ((click_position.x - track_left) / track_width).clamp(0.0, 1.0)
        };

        // Calculate target scroll position
        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);
        let target_scroll = click_ratio * max_scroll;

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let scroll_delta = target_scroll - current_scroll;

        // Apply scroll using gpu_scroll
        if let Err(e) = self.gpu_scroll(
            dom_id.inner as u64,
            node_id.index() as u64,
            if is_vertical { 0.0 } else { scroll_delta },
            if is_vertical { scroll_delta } else { 0.0 },
        ) {
            eprintln!("Track click scroll failed: {}", e);
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }
    
    /// Handle scrollbar drag - update scroll position based on mouse delta.
    fn handle_scrollbar_drag(
        &mut self,
        current_pos: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::hit_test::ScrollbarHitId;
        use azul_layout::scroll::ScrollbarOrientation;

        let drag_state = match self.get_scrollbar_drag_state() {
            Some(ds) => ds.clone(),
            None => return ProcessEventResult::DoNothing,
        };

        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate delta
        let (dom_id, node_id, is_vertical) = match drag_state.hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id) => (dom_id, node_id, true),
            ScrollbarHitId::HorizontalThumb(dom_id, node_id) => (dom_id, node_id, false),
            _ => return ProcessEventResult::DoNothing,
        };

        let pixel_delta = if is_vertical {
            current_pos.y - drag_state.initial_mouse_pos.y
        } else {
            current_pos.x - drag_state.initial_mouse_pos.x
        };

        // Get scrollbar geometry
        let orientation = if is_vertical {
            ScrollbarOrientation::Vertical
        } else {
            ScrollbarOrientation::Horizontal
        };

        let scrollbar_state = match layout_window
            .scroll_states
            .get_scrollbar_state(dom_id, node_id, orientation)
        {
            Some(s) if s.visible => s,
            _ => return ProcessEventResult::DoNothing,
        };

        let scroll_state = match layout_window
            .scroll_states
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Convert pixel delta to scroll delta
        // pixel_delta / track_size = scroll_delta / max_scroll
        let track_size = if is_vertical {
            scrollbar_state.track_rect.size.height
        } else {
            scrollbar_state.track_rect.size.width
        };

        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);

        // Account for thumb size: usable track size is track_size - thumb_size
        let thumb_size = scrollbar_state.thumb_size_ratio * track_size;
        let usable_track_size = (track_size - thumb_size).max(1.0);

        // Calculate scroll delta
        let scroll_delta = if usable_track_size > 0.0 {
            (pixel_delta / usable_track_size) * max_scroll
        } else {
            0.0
        };

        // Calculate target scroll position (initial + delta from drag start)
        let target_scroll = if is_vertical {
            drag_state.initial_scroll_offset.y + scroll_delta
        } else {
            drag_state.initial_scroll_offset.x + scroll_delta
        };

        // Clamp to valid range
        let target_scroll = target_scroll.clamp(0.0, max_scroll);

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let delta_from_current = target_scroll - current_scroll;

        // Use gpu_scroll to update scroll position
        if let Err(e) = self.gpu_scroll(
            dom_id.inner as u64,
            node_id.index() as u64,
            if is_vertical { 0.0 } else { delta_from_current },
            if is_vertical { delta_from_current } else { 0.0 },
        ) {
            eprintln!("Scrollbar drag failed: {}", e);
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}
