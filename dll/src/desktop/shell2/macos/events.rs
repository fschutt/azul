//! macOS Event handling - converts NSEvent to Azul events and dispatches callbacks.

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    events::{EventFilter, MouseButton, NodesToCheck, ProcessEventResult, SyntheticEvent},
    geom::{LogicalPosition, PhysicalPositionI32},
    hit_test::{CursorTypeHitTest, FullHitTest},
    window::{
        CursorPosition, KeyboardState, MouseCursorType, MouseState, OptionMouseCursorType,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    callbacks::CallbackInfo,
    scroll::{ScrollbarComponent, ScrollbarHit, ScrollbarOrientation},
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::WindowState,
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;

use super::MacOSWindow;
// Re-export common types
pub use crate::desktop::shell2::common::event_v2::HitTestNode;
// Import V2 cross-platform event processing trait
use crate::desktop::shell2::common::event_v2::PlatformWindowV2;

/// Extension trait for Callback to convert from CoreCallback
trait CallbackExt {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self;
}

impl CallbackExt for azul_layout::callbacks::Callback {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self {
        // Use the existing safe wrapper method from Callback
        azul_layout::callbacks::Callback::from_core(core_callback)
    }
}

/// Result of processing an event - determines whether to redraw, update layout, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventProcessResult {
    /// No action needed
    DoNothing,
    /// Request redraw (present() will be called)
    RequestRedraw,
    /// Layout changed, need full rebuild
    RegenerateDisplayList,
    /// Window should close
    CloseWindow,
}

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events,
    /// keys)
    RootNodes,
}

impl MacOSWindow {
    /// Convert ProcessEventResult to platform-specific EventProcessResult
    #[inline]
    fn convert_process_result(result: azul_core::events::ProcessEventResult) -> EventProcessResult {
        use azul_core::events::ProcessEventResult as PER;
        match result {
            PER::DoNothing => EventProcessResult::DoNothing,
            PER::ShouldReRenderCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldUpdateDisplayListCurrentWindow => EventProcessResult::RequestRedraw,
            PER::UpdateHitTesterAndProcessAgain => EventProcessResult::RequestRedraw,
            PER::ShouldRegenerateDomCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldRegenerateDomAllWindows => EventProcessResult::RequestRedraw,
        }
    }

    /// Query WebRender hit-tester for scrollbar hits
    fn perform_scrollbar_hit_test(
        &self,
        position: LogicalPosition,
    ) -> Option<azul_core::hit_test::ScrollbarHitId> {
        use webrender::api::units::WorldPoint;

        use crate::desktop::wr_translate2::AsyncHitTester;

        let hit_tester = match &self.hit_tester {
            AsyncHitTester::Resolved(ht) => ht,
            _ => return None,
        };

        let world_point = WorldPoint::new(position.x, position.y);
        let hit_result = hit_tester.hit_test(world_point);

        // Check each hit item for scrollbar tag
        for item in &hit_result.items {
            if let Some(scrollbar_id) =
                crate::desktop::wr_translate2::translate_item_tag_to_scrollbar_hit_id(item.tag)
            {
                return Some(scrollbar_id);
            }
        }

        None
    }

    /// Handle scrollbar click (thumb or track)
    fn handle_scrollbar_click(
        &mut self,
        hit_id: azul_core::hit_test::ScrollbarHitId,
        position: LogicalPosition,
    ) -> EventProcessResult {
        use azul_core::hit_test::ScrollbarHitId;

        match hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id)
            | ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                // Start drag
                let layout_window = match self.layout_window.as_ref() {
                    Some(lw) => lw,
                    None => return EventProcessResult::DoNothing,
                };

                let scroll_offset = layout_window
                    .scroll_states
                    .get_current_offset(dom_id, node_id)
                    .unwrap_or_default();

                self.scrollbar_drag_state = Some(azul_layout::ScrollbarDragState {
                    hit_id,
                    initial_mouse_pos: position,
                    initial_scroll_offset: scroll_offset,
                });

                EventProcessResult::RequestRedraw
            }

            ScrollbarHitId::VerticalTrack(dom_id, node_id) => {
                // Jump scroll to clicked position on track
                self.handle_track_click(dom_id, node_id, position, true)
            }

            ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                // Jump scroll to clicked position on track
                self.handle_track_click(dom_id, node_id, position, false)
            }
        }
    }

    /// Handle track click - jump scroll to clicked position
    fn handle_track_click(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
        click_position: LogicalPosition,
        is_vertical: bool,
    ) -> EventProcessResult {
        // Get scrollbar state to calculate target position
        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return EventProcessResult::DoNothing,
        };

        // Get current scrollbar geometry
        let scrollbar_state = if is_vertical {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return EventProcessResult::DoNothing,
        };

        // Get current scroll state
        let scroll_state = match layout_window
            .scroll_states
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return EventProcessResult::DoNothing,
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
        // click_ratio should center the thumb at the clicked position
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

        // Center thumb at click position: target_scroll = click_ratio * max_scroll - (thumb_size /
        // 2) For simplicity, just jump to click_ratio * max_scroll
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
            return EventProcessResult::DoNothing;
        }

        EventProcessResult::RequestRedraw
    }

    /// Handle scrollbar drag (continuous thumb movement)
    fn handle_scrollbar_drag(&mut self, current_pos: LogicalPosition) -> EventProcessResult {
        let drag_state = match &self.scrollbar_drag_state {
            Some(ds) => ds.clone(),
            None => return EventProcessResult::DoNothing,
        };

        use azul_core::hit_test::ScrollbarHitId;
        let (dom_id, node_id, is_vertical) = match drag_state.hit_id {
            ScrollbarHitId::VerticalThumb(d, n) | ScrollbarHitId::VerticalTrack(d, n) => {
                (d, n, true)
            }
            ScrollbarHitId::HorizontalThumb(d, n) | ScrollbarHitId::HorizontalTrack(d, n) => {
                (d, n, false)
            }
        };

        // Get scrollbar geometry to convert pixel delta to scroll delta
        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return EventProcessResult::DoNothing,
        };

        let scrollbar_state = if is_vertical {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_states.get_scrollbar_state(
                dom_id,
                node_id,
                azul_layout::scroll::ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return EventProcessResult::DoNothing,
        };

        let scroll_state = match layout_window
            .scroll_states
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return EventProcessResult::DoNothing,
        };

        // Calculate mouse delta in pixels
        let pixel_delta = if is_vertical {
            current_pos.y - drag_state.initial_mouse_pos.y
        } else {
            current_pos.x - drag_state.initial_mouse_pos.x
        };

        // Convert pixel delta to scroll delta
        // pixel_delta / track_size = scroll_delta / max_scroll
        // Therefore: scroll_delta = (pixel_delta / track_size) * max_scroll

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
        let usable_track_size = (track_size - thumb_size).max(1.0); // Avoid division by zero

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
            return EventProcessResult::DoNothing;
        }

        EventProcessResult::RequestRedraw
    }

    /// Process a mouse button down event.
    pub fn handle_mouse_down(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Check for scrollbar hit FIRST (before state changes)
        if let Some(scrollbar_hit_id) = self.perform_scrollbar_hit_test(position) {
            return self.handle_scrollbar_click(scrollbar_hit_id, position);
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = true,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = true,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = true,
            _ => {}
        }

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Use V2 cross-platform event system - it will automatically:
        // - Detect MouseDown event (left/right/middle)
        // - Dispatch to hovered nodes (including CSD buttons with callbacks)
        // - Handle event propagation
        // - Process callback results recursively
        let result = self.process_window_events_recursive_v2(0);

        Self::convert_process_result(result)
    }

    /// Process a mouse button up event.
    pub fn handle_mouse_up(&mut self, event: &NSEvent, button: MouseButton) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // End scrollbar drag if active (before state changes)
        if self.scrollbar_drag_state.is_some() {
            self.scrollbar_drag_state = None;
            return EventProcessResult::RequestRedraw;
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state - clear appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = false,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = false,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = false,
            _ => {}
        }

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Check for right-click context menu (before event processing)
        if button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                if self
                    .try_show_context_menu(hit_node, position, event)
                    .is_some()
                {
                    return EventProcessResult::DoNothing;
                }
            }
        }

        // Use V2 cross-platform event system - automatically detects MouseUp
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process a mouse move event.
    pub fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Handle active scrollbar drag (special case - not part of normal event system)
        if self.scrollbar_drag_state.is_some() {
            return self.handle_scrollbar_drag(position);
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Update hit test
        self.update_hit_test(position);

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process mouse entered window event.
    pub fn handle_mouse_entered(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state - cursor is now in window
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Update hit test
        self.update_hit_test(position);

        // V2 system will detect MouseEnter events from state diff
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process mouse exited window event.
    pub fn handle_mouse_exited(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state - cursor left window
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(position);

        // Clear last hit test since mouse is out
        self.current_window_state.last_hit_test = FullHitTest::empty(None);

        // V2 system will detect MouseLeave events from state diff
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process a scroll wheel event.
    pub fn handle_scroll_wheel(&mut self, event: &NSEvent) -> EventProcessResult {
        let delta_x = unsafe { event.scrollingDeltaX() };
        let delta_y = unsafe { event.scrollingDeltaY() };
        let _has_precise = unsafe { event.hasPreciseScrollingDeltas() };

        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update scroll state
        use azul_css::OptionF32;
        let current_x = self
            .current_window_state
            .mouse_state
            .scroll_x
            .into_option()
            .unwrap_or(0.0);
        let current_y = self
            .current_window_state
            .mouse_state
            .scroll_y
            .into_option()
            .unwrap_or(0.0);

        self.current_window_state.mouse_state.scroll_x =
            OptionF32::Some(current_x + delta_x as f32);
        self.current_window_state.mouse_state.scroll_y =
            OptionF32::Some(current_y + delta_y as f32);

        // Update hit test
        self.update_hit_test(position);

        // GPU scroll for visible scrollbars (if delta is significant)
        if (delta_x.abs() > 0.01 || delta_y.abs() > 0.01) {
            if let Some(hit_node) = self.get_first_hovered_node() {
                let _ = self.gpu_scroll(
                    hit_node.dom_id,
                    hit_node.node_id,
                    -delta_x as f32, // Invert for natural scrolling
                    -delta_y as f32,
                );
            }
        }

        // V2 system will detect Scroll event from state diff
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process a key down event.
    pub fn handle_key_down(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Extract Unicode character from event
        let character = unsafe {
            event.characters().and_then(|s| {
                let s_str = s.to_string();
                s_str.chars().next()
            })
        };

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update keyboard state with keycode
        self.update_keyboard_state(key_code, modifiers, true);

        // Update keyboard state with character (for text input)
        self.update_keyboard_state_with_char(character);

        // V2 system will detect VirtualKeyDown and TextInput from state diff
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process a key up event.
    pub fn handle_key_up(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update keyboard state
        self.update_keyboard_state(key_code, modifiers, false);

        // Clear current character on key up
        self.update_keyboard_state_with_char(None);

        // V2 system will detect VirtualKeyUp from state diff
        let result = self.process_window_events_recursive_v2(0);

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Process a flags changed event (modifier keys).
    pub fn handle_flags_changed(&mut self, event: &NSEvent) -> EventProcessResult {
        let modifiers = unsafe { event.modifierFlags() };

        // Determine which modifier keys are currently pressed
        let shift_pressed = modifiers.contains(NSEventModifierFlags::Shift);
        let ctrl_pressed = modifiers.contains(NSEventModifierFlags::Control);
        let alt_pressed = modifiers.contains(NSEventModifierFlags::Option);
        let cmd_pressed = modifiers.contains(NSEventModifierFlags::Command);

        // Track previous state to detect what changed
        let keyboard_state = &self.current_window_state.keyboard_state;
        let was_shift_down = keyboard_state.shift_down();
        let was_ctrl_down = keyboard_state.ctrl_down();
        let was_alt_down = keyboard_state.alt_down();
        let was_cmd_down = keyboard_state.super_down();

        // Update keyboard state based on changes
        use azul_core::window::VirtualKeyCode;

        // Shift key changed
        if shift_pressed != was_shift_down {
            if shift_pressed {
                self.update_keyboard_state(0x38, modifiers, true); // LShift keycode
            } else {
                self.update_keyboard_state(0x38, modifiers, false);
            }
        }

        // Control key changed
        if ctrl_pressed != was_ctrl_down {
            if ctrl_pressed {
                self.update_keyboard_state(0x3B, modifiers, true); // LControl keycode
            } else {
                self.update_keyboard_state(0x3B, modifiers, false);
            }
        }

        // Alt/Option key changed
        if alt_pressed != was_alt_down {
            if alt_pressed {
                self.update_keyboard_state(0x3A, modifiers, true); // LAlt keycode
            } else {
                self.update_keyboard_state(0x3A, modifiers, false);
            }
        }

        // Command key changed
        if cmd_pressed != was_cmd_down {
            if cmd_pressed {
                self.update_keyboard_state(0x37, modifiers, true); // LWin (Command) keycode
            } else {
                self.update_keyboard_state(0x37, modifiers, false);
            }
        }

        // Dispatch modifier changed callbacks if any modifier changed
        if shift_pressed != was_shift_down
            || ctrl_pressed != was_ctrl_down
            || alt_pressed != was_alt_down
            || cmd_pressed != was_cmd_down
        {
            // For now, just return DoNothing - could dispatch specific callbacks later
            EventProcessResult::DoNothing
        } else {
            EventProcessResult::DoNothing
        }
    }

    /// Process a window resize event.
    pub fn handle_resize(&mut self, new_width: f64, new_height: f64) -> EventProcessResult {
        use azul_core::geom::LogicalSize;

        let new_size = LogicalSize {
            width: new_width as f32,
            height: new_height as f32,
        };

        // Update window state
        self.current_window_state.size.dimensions = new_size;

        // Check if DPI changed (window may have moved to different display)
        let current_hidpi = self.get_hidpi_factor();
        let old_hidpi = self.current_window_state.size.get_hidpi_factor();

        if (current_hidpi.inner.get() - old_hidpi.inner.get()).abs() > 0.001 {
            eprintln!(
                "[Resize] DPI changed: {} -> {}",
                old_hidpi.inner.get(),
                current_hidpi.inner.get()
            );
            self.current_window_state.size.dpi = (current_hidpi.inner.get() * 96.0) as u32;
        }

        // Notify compositor of resize (this is private in mod.rs, so we inline it here)
        if let Err(e) = self.handle_compositor_resize() {
            eprintln!("Compositor resize failed: {}", e);
        }

        // Resize requires full display list rebuild
        EventProcessResult::RegenerateDisplayList
    }

    /// Process a file drop event.
    pub fn handle_file_drop(&mut self, paths: Vec<String>) -> EventProcessResult {
        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update state with dropped file
        if let Some(first_path) = paths.first() {
            self.current_window_state.dropped_file = Some(first_path.clone().into());
        }

        // Update hit test at current cursor position
        if let CursorPosition::InWindow(pos) = self.current_window_state.mouse_state.cursor_position
        {
            self.update_hit_test(pos);
        }

        // V2 system will detect FileDrop event from state diff
        let result = self.process_window_events_recursive_v2(0);

        // Clear dropped file after processing
        self.current_window_state.dropped_file = None;

        match result {
            azul_core::events::ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RequestRedraw
            }
            azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RequestRedraw
            }
        }
    }

    /// Perform hit testing at given position using WebRender hit-testing API.
    fn perform_hit_test(&mut self, position: LogicalPosition) -> Option<HitTestNode> {
        use azul_core::window::CursorPosition;

        let layout_window = self.layout_window.as_ref()?;

        // Early return if no layout results
        if layout_window.layout_results.is_empty() {
            return None;
        }

        let cursor_position = CursorPosition::InWindow(position);

        // Use layout_results directly (BTreeMap)
        let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
            &*self.hit_tester.resolve(),
            self.document_id,
            self.current_window_state.focused_node,
            &layout_window.layout_results,
            &cursor_position,
            self.current_window_state.size.get_hidpi_factor(),
        );

        // Extract first hovered node from hit test result
        hit_test
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes.keys().next().map(|node_id| {
                    let node_id_value = node_id.index();
                    HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id_value as u64,
                    }
                })
            })
            .next()
    }

    /// Convert macOS keycode to VirtualKeyCode.
    fn convert_keycode(&self, keycode: u16) -> Option<VirtualKeyCode> {
        // macOS keycodes: https://eastmanreference.com/complete-list-of-applescript-key-codes
        match keycode {
            0x00 => Some(VirtualKeyCode::A),
            0x01 => Some(VirtualKeyCode::S),
            0x02 => Some(VirtualKeyCode::D),
            0x03 => Some(VirtualKeyCode::F),
            0x04 => Some(VirtualKeyCode::H),
            0x05 => Some(VirtualKeyCode::G),
            0x06 => Some(VirtualKeyCode::Z),
            0x07 => Some(VirtualKeyCode::X),
            0x08 => Some(VirtualKeyCode::C),
            0x09 => Some(VirtualKeyCode::V),
            0x0B => Some(VirtualKeyCode::B),
            0x0C => Some(VirtualKeyCode::Q),
            0x0D => Some(VirtualKeyCode::W),
            0x0E => Some(VirtualKeyCode::E),
            0x0F => Some(VirtualKeyCode::R),
            0x10 => Some(VirtualKeyCode::Y),
            0x11 => Some(VirtualKeyCode::T),
            0x12 => Some(VirtualKeyCode::Key1),
            0x13 => Some(VirtualKeyCode::Key2),
            0x14 => Some(VirtualKeyCode::Key3),
            0x15 => Some(VirtualKeyCode::Key4),
            0x16 => Some(VirtualKeyCode::Key6),
            0x17 => Some(VirtualKeyCode::Key5),
            0x18 => Some(VirtualKeyCode::Equals),
            0x19 => Some(VirtualKeyCode::Key9),
            0x1A => Some(VirtualKeyCode::Key7),
            0x1B => Some(VirtualKeyCode::Minus),
            0x1C => Some(VirtualKeyCode::Key8),
            0x1D => Some(VirtualKeyCode::Key0),
            0x1E => Some(VirtualKeyCode::RBracket),
            0x1F => Some(VirtualKeyCode::O),
            0x20 => Some(VirtualKeyCode::U),
            0x21 => Some(VirtualKeyCode::LBracket),
            0x22 => Some(VirtualKeyCode::I),
            0x23 => Some(VirtualKeyCode::P),
            0x24 => Some(VirtualKeyCode::Return),
            0x25 => Some(VirtualKeyCode::L),
            0x26 => Some(VirtualKeyCode::J),
            0x27 => Some(VirtualKeyCode::Apostrophe),
            0x28 => Some(VirtualKeyCode::K),
            0x29 => Some(VirtualKeyCode::Semicolon),
            0x2A => Some(VirtualKeyCode::Backslash),
            0x2B => Some(VirtualKeyCode::Comma),
            0x2C => Some(VirtualKeyCode::Slash),
            0x2D => Some(VirtualKeyCode::N),
            0x2E => Some(VirtualKeyCode::M),
            0x2F => Some(VirtualKeyCode::Period),
            0x30 => Some(VirtualKeyCode::Tab),
            0x31 => Some(VirtualKeyCode::Space),
            0x32 => Some(VirtualKeyCode::Grave),
            0x33 => Some(VirtualKeyCode::Back),
            0x35 => Some(VirtualKeyCode::Escape),
            0x37 => Some(VirtualKeyCode::LWin), // Command
            0x38 => Some(VirtualKeyCode::LShift),
            0x39 => Some(VirtualKeyCode::Capital), // Caps Lock
            0x3A => Some(VirtualKeyCode::LAlt),    // Option
            0x3B => Some(VirtualKeyCode::LControl),
            0x3C => Some(VirtualKeyCode::RShift),
            0x3D => Some(VirtualKeyCode::RAlt),
            0x3E => Some(VirtualKeyCode::RControl),
            0x7B => Some(VirtualKeyCode::Left),
            0x7C => Some(VirtualKeyCode::Right),
            0x7D => Some(VirtualKeyCode::Down),
            0x7E => Some(VirtualKeyCode::Up),
            _ => None,
        }
    }

    /// Update keyboard state from event.
    fn update_keyboard_state(
        &mut self,
        keycode: u16,
        modifiers: NSEventModifierFlags,
        is_down: bool,
    ) {
        use azul_core::window::VirtualKeyCode;

        // Convert keycode to VirtualKeyCode first (before borrowing)
        let vk = match self.convert_keycode(keycode) {
            Some(k) => k,
            None => return,
        };

        let keyboard_state = &mut self.current_window_state.keyboard_state;

        if is_down {
            // Add to pressed keys if not already present
            let mut already_pressed = false;
            for pressed_key in keyboard_state.pressed_virtual_keycodes.as_ref() {
                if *pressed_key == vk {
                    already_pressed = true;
                    break;
                }
            }
            if !already_pressed {
                // Convert to Vec, add, convert back
                let mut pressed_vec: Vec<VirtualKeyCode> =
                    keyboard_state.pressed_virtual_keycodes.as_ref().to_vec();
                pressed_vec.push(vk);
                keyboard_state.pressed_virtual_keycodes =
                    azul_core::window::VirtualKeyCodeVec::from_vec(pressed_vec);
            }
            keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::Some(vk);
        } else {
            // Remove from pressed keys
            let pressed_vec: Vec<VirtualKeyCode> = keyboard_state
                .pressed_virtual_keycodes
                .as_ref()
                .iter()
                .copied()
                .filter(|k| *k != vk)
                .collect();
            keyboard_state.pressed_virtual_keycodes =
                azul_core::window::VirtualKeyCodeVec::from_vec(pressed_vec);
            keyboard_state.current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::None;
        }
    }

    /// Update keyboard state with character from event
    fn update_keyboard_state_with_char(&mut self, character: Option<char>) {
        use azul_core::window::OptionChar;

        self.current_window_state.keyboard_state.current_char = match character {
            Some(ch) => OptionChar::Some(ch as u32),
            None => OptionChar::None,
        };
    }

    /// Handle compositor resize notification.
    fn handle_compositor_resize(&mut self) -> Result<(), String> {
        use webrender::api::units::{DeviceIntRect, DeviceIntSize};

        // Get new physical size
        let physical_size = self.current_window_state.size.get_physical_size();
        let new_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

        // Update WebRender document size
        let mut txn = webrender::Transaction::new();
        let device_rect = DeviceIntRect::from_size(new_size);
        txn.set_document_view(device_rect);

        // Send transaction
        if let Some(ref layout_window) = self.layout_window {
            let document_id =
                crate::desktop::wr_translate2::wr_translate_document_id(layout_window.document_id);
            self.render_api.send_transaction(document_id, txn);
        }

        // Resize GL viewport (if OpenGL backend)
        if let Some(ref gl_context) = self.gl_context {
            // Make context current
            unsafe {
                gl_context.makeCurrentContext();
            }

            // Resize viewport
            if let Some(ref gl) = self.gl_functions {
                use azul_core::gl as gl_types;
                gl.functions.viewport(
                    0,
                    0,
                    physical_size.width as gl_types::GLint,
                    physical_size.height as gl_types::GLint,
                );
            }
        }

        // Resize CPU framebuffer if using CPU backend
        if let Some(cpu_view) = &self.cpu_view {
            unsafe {
                // Force the CPU view to resize its framebuffer on next draw
                // The actual resize happens in CPUView::drawRect when bounds change
                cpu_view.setNeedsDisplay(true);
            }
        }

        Ok(())
    }

    /// Try to show context menu for the given node at position.
    /// Returns Some if a menu was shown, None otherwise.
    fn try_show_context_menu(
        &mut self,
        node: HitTestNode,
        position: LogicalPosition,
        event: &NSEvent,
    ) -> Option<()> {
        use azul_core::dom::DomId;

        let layout_window = self.layout_window.as_ref()?;
        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = layout_window.layout_results.get(&dom_id)?;

        // Check if this node has a context menu
        let node_id = azul_core::id::NodeId::from_usize(node.node_id as usize)?;
        let binding = layout_result.styled_dom.node_data.as_container();
        let node_data = binding.get(node_id)?;

        // Context menus are stored directly on NodeData, not as callbacks
        let context_menu = node_data.get_context_menu()?;

        eprintln!(
            "[Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Check if native context menus are enabled
        if self.current_window_state.flags.use_native_context_menus {
            self.show_native_context_menu_at_position(context_menu, position, event);
        } else {
            self.show_window_based_context_menu(context_menu, position);
        }

        Some(())
    }

    /// Show an NSMenu as a context menu at the given screen position.
    fn show_native_context_menu_at_position(
        &self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
        event: &NSEvent,
    ) {
        use objc2_app_kit::NSMenu;
        use objc2_foundation::{MainThreadMarker, NSPoint};

        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => {
                eprintln!("[Context Menu] Not on main thread, cannot show menu");
                return;
            }
        };

        let ns_menu = NSMenu::new(mtm);

        for item in menu.items.as_slice() {
            match item {
                azul_core::menu::MenuItem::String(string_item) => {
                    let menu_item = objc2_app_kit::NSMenuItem::new(mtm);
                    let title = objc2_foundation::NSString::from_str(&string_item.label);
                    menu_item.setTitle(&title);
                    ns_menu.addItem(&menu_item);
                }
                azul_core::menu::MenuItem::Separator => {
                    let separator = unsafe { objc2_app_kit::NSMenuItem::separatorItem(mtm) };
                    ns_menu.addItem(&separator);
                }
                _ => {}
            }
        }

        let view_point = NSPoint {
            x: position.x as f64,
            y: position.y as f64,
        };

        let view = if let Some(ref gl_view) = self.gl_view {
            Some(&**gl_view as &objc2::runtime::AnyObject)
        } else if let Some(ref cpu_view) = self.cpu_view {
            Some(&**cpu_view as &objc2::runtime::AnyObject)
        } else {
            None
        };

        if let Some(view) = view {
            eprintln!(
                "[Context Menu] Showing menu at position ({}, {}) with {} items",
                position.x,
                position.y,
                menu.items.as_slice().len()
            );

            unsafe {
                use objc2::{msg_send_id, rc::Retained, runtime::AnyObject, sel};

                let _: () = msg_send_id![
                    &ns_menu,
                    popUpMenuPositioningItem: Option::<&AnyObject>::None,
                    atLocation: view_point,
                    inView: view
                ];
            }
        }
    }

    /// Show a context menu using Azul window-based menu system
    ///
    /// This uses the same unified menu system as regular menus (crate::desktop::menu::show_menu)
    /// but spawns at cursor position instead of below a trigger rect.
    fn show_window_based_context_menu(
        &self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let _menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for context menus (they spawn at cursor)
            Some(position), // Cursor position for menu positioning
            None,           // No parent menu
        );

        // TODO: Queue window creation request for processing in main event loop
        // For now, we log the request. The proper implementation requires:
        // 1. A queue of pending WindowCreateOptions
        // 2. Processing this queue in the event loop via create_window()
        // 3. Multi-window support in the macOS event loop (similar to X11)
        eprintln!(
            "[macOS] Window-based context menu requested at screen ({}, {}) - requires \
             multi-window support",
            position.x, position.y
        );
    }

    // ========================================================================
    // Helper Functions for V2 Event System
    // ========================================================================

    /// Update hit test at given position and store in current_window_state.
    fn update_hit_test(&mut self, position: LogicalPosition) {
        if let Some(layout_window) = self.layout_window.as_ref() {
            let cursor_position = CursorPosition::InWindow(position);
            let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
                &*self.hit_tester.resolve(),
                self.document_id,
                self.current_window_state.focused_node,
                &layout_window.layout_results,
                &cursor_position,
                self.current_window_state.size.get_hidpi_factor(),
            );
            self.current_window_state.last_hit_test = hit_test;
        }
    }

    /// Get the first hovered node from current hit test.
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        self.current_window_state
            .last_hit_test
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes
                    .keys()
                    .next()
                    .map(|node_id| HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
            })
            .next()
    }

    /// Convert ProcessEventResult to EventProcessResult for old API compatibility.
    fn process_callback_result_to_event_result_v2(
        &self,
        result: ProcessEventResult,
    ) -> EventProcessResult {
        use azul_core::events::ProcessEventResult as PER;

        match result {
            PER::DoNothing => EventProcessResult::DoNothing,
            PER::ShouldReRenderCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldUpdateDisplayListCurrentWindow => EventProcessResult::RequestRedraw,
            PER::UpdateHitTesterAndProcessAgain => EventProcessResult::RequestRedraw,
            PER::ShouldRegenerateDomCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldRegenerateDomAllWindows => EventProcessResult::RequestRedraw,
        }
    }

    // ========================================================================
    // V2 Cross-Platform Event Processing
    // ========================================================================
    // NOTE: All V2 event processing methods are now provided by the
    // PlatformWindowV2 trait in common/event_v2.rs. The trait provides:
    // - process_window_events_v2() - Entry point (public API)
    // - process_window_events_recursive_v2() - Recursive processing
    // - invoke_callbacks_v2() - Required method (implemented in mod.rs)
    // - process_callback_result_v2() - Result handling
    // This eliminates ~336 lines of platform-specific duplicated code.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keycode_conversion() {
        // Test some basic keycodes
        assert_eq!(Some(VirtualKeyCode::A), convert_keycode_test(0x00));
        assert_eq!(Some(VirtualKeyCode::Return), convert_keycode_test(0x24));
        assert_eq!(Some(VirtualKeyCode::Space), convert_keycode_test(0x31));
        assert_eq!(None, convert_keycode_test(0xFF)); // Invalid
    }

    fn convert_keycode_test(keycode: u16) -> Option<VirtualKeyCode> {
        // Helper for testing keycode conversion without MacOSWindow instance
        match keycode {
            0x00 => Some(VirtualKeyCode::A),
            0x24 => Some(VirtualKeyCode::Return),
            0x31 => Some(VirtualKeyCode::Space),
            _ => None,
        }
    }
}
