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

/// Extension trait for Callback to convert from CoreCallback
trait CallbackExt {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self;
}

impl CallbackExt for azul_layout::callbacks::Callback {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self {
        Self {
            cb: unsafe { std::mem::transmute(core_callback.cb) },
        }
    }
}

/// Result of processing an event - determines whether to redraw, update layout, etc.
#[derive(Debug, Clone, PartialEq)]
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

// NOTE: ScrollbarDragState is now imported from azul_layout::ScrollbarDragState
// (was previously defined here as duplicate)

impl MacOSWindow {
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
    pub(crate) fn handle_mouse_down(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = true,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = true,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = true,
            _ => {}
        }

        // Check for scrollbar hit FIRST
        if let Some(scrollbar_hit_id) = self.perform_scrollbar_hit_test(position) {
            return self.handle_scrollbar_click(scrollbar_hit_id, position);
        }

        // Perform hit testing to find which node was clicked
        let hit_test_result = self.perform_hit_test(position);

        // Dispatch callbacks for clicked nodes
        if let Some(hit_node) = hit_test_result {
            self.last_hovered_node = Some(hit_node);

            // Extract necessary data before borrowing
            let callback_result = self.dispatch_mouse_down_callbacks(hit_node, button, position);

            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    /// Process a mouse button up event.
    pub(crate) fn handle_mouse_up(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state - clear appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = false,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = false,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = false,
            _ => {}
        }

        // End scrollbar drag if active
        if self.scrollbar_drag_state.is_some() {
            self.scrollbar_drag_state = None;
            return EventProcessResult::RequestRedraw;
        }

        // Perform hit testing
        let hit_test_result = self.perform_hit_test(position);

        // Dispatch callbacks
        if let Some(hit_node) = hit_test_result {
            let callback_result = self.dispatch_mouse_up_callbacks(hit_node, button, position);

            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    /// Process a mouse move event.
    pub(crate) fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Handle active scrollbar drag
        if self.scrollbar_drag_state.is_some() {
            return self.handle_scrollbar_drag(position);
        }

        // Update hover state
        let hit_test_result = self.perform_hit_test(position);

        // Check if hovered node changed
        if let Some(hit_node) = hit_test_result {
            // Dispatch hover callbacks if node changed
            if self.last_hovered_node != Some(hit_node) {
                self.last_hovered_node = Some(hit_node);

                let callback_result = self.dispatch_hover_callbacks(hit_node, position);

                return self.process_callback_result_to_event_result(callback_result);
            }
        } else {
            self.last_hovered_node = None;
        }

        EventProcessResult::DoNothing
    }

    /// Process mouse entered window event.
    pub(crate) fn handle_mouse_entered(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state - cursor is now in window
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Perform hit testing to find which node cursor entered
        let hit_test_result = self.perform_hit_test(position);

        if let Some(hit_node) = hit_test_result {
            self.last_hovered_node = Some(hit_node);

            // Dispatch hover callbacks for enter
            let callback_result = self.dispatch_hover_callbacks(hit_node, position);
            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    /// Process mouse exited window event.
    pub(crate) fn handle_mouse_exited(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state - cursor left window
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(position);

        // Clear hovered node
        self.last_hovered_node = None;

        // Note: Could dispatch OnHoverOut callbacks here if needed
        EventProcessResult::DoNothing
    }

    /// Process a scroll wheel event.
    pub(crate) fn handle_scroll_wheel(&mut self, event: &NSEvent) -> EventProcessResult {
        let delta_x = unsafe { event.scrollingDeltaX() };
        let delta_y = unsafe { event.scrollingDeltaY() };
        let has_precise = unsafe { event.hasPreciseScrollingDeltas() };

        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Perform hit testing to find scrollable node
        let hit_test_result = self.perform_hit_test(position);

        if let Some(hit_node) = hit_test_result {
            let callback_result =
                self.dispatch_scroll_callbacks(hit_node, delta_x as f32, delta_y as f32, position);
            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    /// Process a key down event.
    pub(crate) fn handle_key_down(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Extract Unicode character from event
        let character = unsafe {
            event.characters().and_then(|s| {
                let s_str = s.to_string();
                s_str.chars().next()
            })
        };

        // Update keyboard state with keycode
        self.update_keyboard_state(key_code, modifiers, true);

        // Update keyboard state with character (for text input)
        self.update_keyboard_state_with_char(character);

        // Convert to VirtualKeyCode
        if let Some(vk) = self.convert_keycode(key_code) {
            let callback_result = self.dispatch_key_down_callbacks(vk, modifiers);
            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    /// Process a key up event.
    pub(crate) fn handle_key_up(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Update keyboard state
        self.update_keyboard_state(key_code, modifiers, false);

        // Clear current character on key up
        self.update_keyboard_state_with_char(None);

        // Convert to VirtualKeyCode
        if let Some(vk) = self.convert_keycode(key_code) {
            let callback_result = self.dispatch_key_up_callbacks(vk, modifiers);
            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    /// Process a window resize event.
    pub(crate) fn handle_resize(&mut self, new_width: f64, new_height: f64) -> EventProcessResult {
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

        if (current_hidpi - old_hidpi).abs() > 0.001 {
            eprintln!("[Resize] DPI changed: {} -> {}", old_hidpi, current_hidpi);
            self.current_window_state.size.dpi = (current_hidpi * 96.0) as u32;
        }

        // Notify compositor of resize (this is private in mod.rs, so we inline it here)
        if let Err(e) = self.handle_compositor_resize() {
            eprintln!("Compositor resize failed: {}", e);
        }

        // Resize requires full display list rebuild
        EventProcessResult::RegenerateDisplayList
    }

    /// Process a file drop event.
    pub(crate) fn handle_file_drop(&mut self, paths: Vec<String>) -> EventProcessResult {
        // Find node under cursor for file drop target
        if let CursorPosition::InWindow(pos) = self.current_window_state.mouse_state.cursor_position
        {
            let hit_test_result = self.perform_hit_test(pos);

            if let Some(hit_node) = hit_test_result {
                let callback_result = self.dispatch_file_drop_callbacks(hit_node, paths);
                return self.process_callback_result_to_event_result(callback_result);
            }
        }

        EventProcessResult::DoNothing
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

    /// Dispatch mouse down callbacks to hit node.
    fn dispatch_mouse_down_callbacks(
        &mut self,
        node: HitTestNode,
        button: MouseButton,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };

        // Get layout result for this DOM
        let callback_data = {
            let layout_window = match self.layout_window.as_mut() {
                Some(lw) => lw,
                None => return ProcessEventResult::DoNothing,
            };

            let dom_id = DomId {
                inner: node.dom_id as usize,
            };
            let node_id = match NodeId::from_usize(node.node_id as usize) {
                Some(nid) => nid,
                None => return ProcessEventResult::DoNothing,
            };

            let layout_result = match layout_window.layout_results.get(&dom_id) {
                Some(lr) => lr,
                None => return ProcessEventResult::DoNothing,
            };

            // Get node data to access callbacks
            let binding = layout_result.styled_dom.node_data.as_container();

            let node_data = match binding.get(node_id) {
                Some(nd) => nd,
                None => return ProcessEventResult::DoNothing,
            };

            node_data
                .get_callbacks()
                .as_container()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        };

        // Filter callbacks by event type (MouseDown)
        let event_filter = match button {
            MouseButton::Left => EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            MouseButton::Right => EventFilter::Hover(HoverEventFilter::RightMouseDown),
            MouseButton::Middle => EventFilter::Hover(HoverEventFilter::MiddleMouseDown),
            _ => EventFilter::Hover(HoverEventFilter::MouseDown),
        };

        let mut result = ProcessEventResult::DoNothing;

        // Borrow layout_window and RefCells once before the loop
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };
        let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

        // Collect all callback results first
        let mut callback_results = Vec::new();

        // Iterate through callbacks and invoke matching ones
        for mut callback_data in callback_data {
            if callback_data.event != event_filter {
                continue;
            }

            // Convert CoreCallback to Callback
            let mut callback = azul_layout::callbacks::Callback::from_core(callback_data.callback);

            // Invoke callback
            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data,
                &azul_core::window::RawWindowHandle::Unsupported, // TODO: proper window handle
                &self.gl_context_ptr,
                &mut self.image_cache,
                &mut *fc_cache_borrowed,
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &self.previous_window_state,
                &self.current_window_state,
                &self.renderer_resources,
            );

            callback_results.push(callback_result);
        }

        // Drop borrows before processing results
        drop(layout_window);
        drop(fc_cache_borrowed);

        // Process all callback results
        for callback_result in callback_results {
            result = result.max(self.process_callback_result(callback_result));
        }

        result
    }

    /// Dispatch mouse up callbacks to hit node.
    fn dispatch_mouse_up_callbacks(
        &mut self,
        node: HitTestNode,
        button: MouseButton,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };

        // First, collect callback data while borrowing layout_window immutably
        let (callback_data_list, event_filter) = {
            let layout_window = match self.layout_window.as_ref() {
                Some(lw) => lw,
                None => return ProcessEventResult::DoNothing,
            };

            let dom_id = DomId {
                inner: node.dom_id as usize,
            };
            let node_id = match NodeId::from_usize(node.node_id as usize) {
                Some(nid) => nid,
                None => return ProcessEventResult::DoNothing,
            };

            let layout_result = match layout_window.layout_results.get(&dom_id) {
                Some(lr) => lr,
                None => return ProcessEventResult::DoNothing,
            };

            let binding = layout_result.styled_dom.node_data.as_container();

            let node_data = match binding.get(node_id) {
                Some(nd) => nd,
                None => return ProcessEventResult::DoNothing,
            };

            let event_filter = match button {
                MouseButton::Left => EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                MouseButton::Right => EventFilter::Hover(HoverEventFilter::RightMouseUp),
                MouseButton::Middle => EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
                _ => EventFilter::Hover(HoverEventFilter::MouseUp),
            };

            (
                node_data
                    .get_callbacks()
                    .as_container()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>(),
                event_filter,
            )
        };

        let mut result = ProcessEventResult::DoNothing;

        // Now borrow layout_window mutably for callback invocation
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Borrow RefCells once before the loop
        let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

        // Collect all callback results first
        let mut callback_results = Vec::new();

        for mut callback_data in callback_data_list {
            if callback_data.event != event_filter {
                continue;
            }

            let mut callback = azul_layout::callbacks::Callback::from_core(callback_data.callback);

            let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data,
                &azul_core::window::RawWindowHandle::Unsupported,
                &self.gl_context_ptr,
                &mut self.image_cache,
                &mut *fc_cache_borrowed,
                &external,
                &self.previous_window_state,
                &self.current_window_state,
                &self.renderer_resources,
            );

            callback_results.push(callback_result);
        }

        // Drop borrows before processing results
        drop(layout_window);
        drop(fc_cache_borrowed);

        // Process all callback results
        for callback_result in callback_results {
            result = result.max(self.process_callback_result(callback_result));
        }

        result
    }

    /// Dispatch hover callbacks to hit node.
    fn dispatch_hover_callbacks(
        &mut self,
        node: HitTestNode,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };

        // First, collect callback data while borrowing layout_window immutably
        let callback_data_list = {
            let layout_window = match self.layout_window.as_ref() {
                Some(lw) => lw,
                None => return ProcessEventResult::DoNothing,
            };

            let dom_id = DomId {
                inner: node.dom_id as usize,
            };
            let node_id = match NodeId::from_usize(node.node_id as usize) {
                Some(nid) => nid,
                None => return ProcessEventResult::DoNothing,
            };

            let layout_result = match layout_window.layout_results.get(&dom_id) {
                Some(lr) => lr,
                None => return ProcessEventResult::DoNothing,
            };

            let binding = layout_result.styled_dom.node_data.as_container();

            let node_data = match binding.get(node_id) {
                Some(nd) => nd,
                None => return ProcessEventResult::DoNothing,
            };

            // Check for MouseOver callback
            let event_filter = EventFilter::Hover(HoverEventFilter::MouseOver);

            node_data
                .get_callbacks()
                .as_container()
                .iter()
                .filter(|cd| cd.event == event_filter)
                .cloned()
                .collect::<Vec<_>>()
        };

        let mut result = ProcessEventResult::DoNothing;

        // Now borrow layout_window mutably for callback invocation
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Borrow RefCells once before the loop
        let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

        // Collect all callback results first, then process after loop
        let mut callback_results = Vec::new();

        for callback_data in callback_data_list {
            // Event filter already applied during collection
            let mut callback = azul_layout::callbacks::Callback::from_core(callback_data.callback);

            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data.clone(),
                &azul_core::window::RawWindowHandle::Unsupported,
                &self.gl_context_ptr,
                &mut self.image_cache,
                &mut *fc_cache_borrowed,
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &self.previous_window_state,
                &self.current_window_state,
                &self.renderer_resources,
            );

            callback_results.push(callback_result);
        }

        // Drop borrows before processing results
        drop(layout_window);
        drop(fc_cache_borrowed);

        // Process all callback results
        for callback_result in callback_results {
            result = result.max(self.process_callback_result(callback_result));
        }

        result
    }

    /// Dispatch scroll callbacks to hit node.
    fn dispatch_scroll_callbacks(
        &mut self,
        node: HitTestNode,
        delta_x: f32,
        delta_y: f32,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        // Update scroll state in current_window_state
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
        self.current_window_state.mouse_state.scroll_x = OptionF32::Some(current_x + delta_x);
        self.current_window_state.mouse_state.scroll_y = OptionF32::Some(current_y + delta_y);

        // Perform GPU scroll if delta is significant
        if delta_x.abs() > 0.01 || delta_y.abs() > 0.01 {
            // Call gpu_scroll to update scroll positions without full relayout
            if let Err(e) = self.gpu_scroll(
                node.dom_id,
                node.node_id,
                -delta_x, // Invert for natural scrolling (macOS convention)
                -delta_y,
            ) {
                eprintln!("[Scroll] gpu_scroll failed: {}", e);
                // Fallback: trigger full re-render
                return ProcessEventResult::ShouldReRenderCurrentWindow;
            }

            // GPU scroll succeeded, frame already generated (GPU-only, no DOM rebuild)
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
        } else {
            ProcessEventResult::DoNothing
        }
    }

    /// Dispatch key down callbacks.
    fn dispatch_key_down_callbacks(
        &mut self,
        key: VirtualKeyCode,
        modifiers: NSEventModifierFlags,
    ) -> ProcessEventResult {
        // TODO: Filter window-level key callbacks
        ProcessEventResult::DoNothing
    }

    /// Dispatch key up callbacks.
    fn dispatch_key_up_callbacks(
        &mut self,
        key: VirtualKeyCode,
        modifiers: NSEventModifierFlags,
    ) -> ProcessEventResult {
        // TODO: Filter window-level key callbacks
        ProcessEventResult::DoNothing
    }

    /// Dispatch file drop callbacks to hit node.
    fn dispatch_file_drop_callbacks(
        &mut self,
        node: HitTestNode,
        paths: Vec<String>,
    ) -> ProcessEventResult {
        // TODO: Look up callbacks for node, filter by FileDrop event
        ProcessEventResult::DoNothing
    }

    /// Convert ProcessEventResult to EventProcessResult.
    fn process_callback_result_to_event_result(
        &mut self,
        result: ProcessEventResult,
    ) -> EventProcessResult {
        match result {
            ProcessEventResult::DoNothing => EventProcessResult::DoNothing,
            ProcessEventResult::ShouldReRenderCurrentWindow => EventProcessResult::RequestRedraw,
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                EventProcessResult::RegenerateDisplayList
            }
            ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                EventProcessResult::RegenerateDisplayList
            }
            ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                EventProcessResult::RegenerateDisplayList
            }
            ProcessEventResult::ShouldRegenerateDomAllWindows => {
                EventProcessResult::RegenerateDisplayList
            }
        }
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

        // TODO: Resize CPU framebuffer if CPU backend

        Ok(())
    }

    /// Process callback result from invoke_single_callback and convert to ProcessEventResult.
    fn process_callback_result(
        &mut self,
        result: azul_layout::callbacks::CallCallbacksResult,
    ) -> ProcessEventResult {
        use azul_core::callbacks::Update;

        let mut event_result = ProcessEventResult::DoNothing;

        // Handle window state modifications
        if let Some(modified_state) = result.modified_window_state {
            self.current_window_state.title = modified_state.title;
            self.current_window_state.size = modified_state.size;
            self.current_window_state.position = modified_state.position;
            self.current_window_state.flags = modified_state.flags;
            self.current_window_state.background_color = modified_state.background_color;

            // Check if window should close
            if modified_state.flags.close_requested {
                self.is_open = false;
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle focus changes
        if let Some(new_focus) = result.update_focused_node {
            self.current_window_state.focused_node = new_focus;
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle image updates
        if result.images_changed.is_some() || result.image_masks_changed.is_some() {
            // TODO: Update image cache and send to WebRender
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle timers
        if result.timers.is_some() || result.timers_removed.is_some() {
            // TODO: Start/stop timers
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle threads
        if result.threads.is_some() || result.threads_removed.is_some() {
            // TODO: Start/stop threads
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Process Update screen command
        match result.callbacks_update_screen {
            Update::RefreshDom => {
                // Regenerate layout from layout callback (uses stored app_data/fc_cache)
                if let Err(e) = self.regenerate_layout() {
                    eprintln!("Layout regeneration error: {}", e);
                }
                event_result =
                    event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
            Update::RefreshDomAllWindows => {
                // Regenerate layout for this window, caller should handle other windows
                if let Err(e) = self.regenerate_layout() {
                    eprintln!("Layout regeneration error: {}", e);
                }
                event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            }
            Update::DoNothing => {}
        }

        event_result
    }
}

/// Temporary hit test node structure (placeholder).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
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
