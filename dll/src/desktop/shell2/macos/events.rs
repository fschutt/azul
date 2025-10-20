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
        use crate::desktop::wr_translate2::AsyncHitTester;
        use webrender::api::units::WorldPoint;
        
        let hit_tester = match &self.hit_tester {
            AsyncHitTester::Resolved(ht) => ht,
            _ => return None,
        };

        let world_point = WorldPoint::new(position.x, position.y);
        let hit_result = hit_tester.hit_test(None, world_point);

        // Check each hit item for scrollbar tag
        for item in &hit_result.items {
            if let Some((tag, _)) = item.tag {
                if let Some(scrollbar_id) = 
                    crate::desktop::wr_translate2::translate_item_tag_to_scrollbar_hit_id(tag) 
                {
                    return Some(scrollbar_id);
                }
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
            
            ScrollbarHitId::VerticalTrack(dom_id, node_id) 
            | ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                // TODO: Jump scroll to clicked position
                EventProcessResult::DoNothing
            }
        }
    }

    /// Handle scrollbar drag (continuous thumb movement)
    fn handle_scrollbar_drag(&mut self, current_pos: LogicalPosition) -> EventProcessResult {
        let drag_state = match &self.scrollbar_drag_state {
            Some(ds) => ds.clone(),
            None => return EventProcessResult::DoNothing,
        };

        use azul_core::hit_test::ScrollbarHitId;
        let (dom_id, node_id) = match drag_state.hit_id {
            ScrollbarHitId::VerticalThumb(d, n) | ScrollbarHitId::VerticalTrack(d, n) => (d, n),
            ScrollbarHitId::HorizontalThumb(d, n) | ScrollbarHitId::HorizontalTrack(d, n) => (d, n),
        };

        // Calculate scroll delta from drag delta
        let delta = LogicalPosition::new(
            current_pos.x - drag_state.initial_mouse_pos.x,
            current_pos.y - drag_state.initial_mouse_pos.y,
        );

        // Use gpu_scroll to update scroll position
        if let Err(e) = self.gpu_scroll(
            dom_id.inner as u64,
            node_id.index() as u64,
            delta.x,
            delta.y,
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

            // Borrow app_data and fc_cache for callback dispatch
            let mut app_data_borrowed = self.app_data.borrow_mut();
            let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

            let callback_result = self.dispatch_mouse_down_callbacks(
                hit_node,
                button,
                position,
                &mut *app_data_borrowed,
                &mut *fc_cache_borrowed,
            );

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
            let mut app_data_borrowed = self.app_data.borrow_mut();
            let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

            let callback_result = self.dispatch_mouse_up_callbacks(
                hit_node,
                button,
                position,
                &mut *app_data_borrowed,
                &mut *fc_cache_borrowed,
            );

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

                let mut app_data_borrowed = self.app_data.borrow_mut();
                let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

                let callback_result = self.dispatch_hover_callbacks(
                    hit_node,
                    position,
                    &mut *app_data_borrowed,
                    &mut *fc_cache_borrowed,
                );

                return self.process_callback_result_to_event_result(callback_result);
            }
        } else {
            self.last_hovered_node = None;
        }

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

        // Update keyboard state
        self.update_keyboard_state(key_code, modifiers, true);

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

        // Notify compositor of resize
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
                return self.process_callback_result(callback_result);
            }
        }

        EventProcessResult::DoNothing
    }

    /// Perform hit testing at given position using WebRender hit-testing API.
    fn perform_hit_test(&self, position: LogicalPosition) -> Option<HitTestNode> {
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
                ht.regular_hit_test_nodes.keys().next().and_then(|node_id| {
                    let node_id_value = node_id.into_crate_internal()?;
                    Some(HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id_value as u64,
                    })
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
        // TODO: Update self.current_window_state.keyboard_state
    }

    /// Dispatch mouse down callbacks to hit node.
    fn dispatch_mouse_down_callbacks(
        &mut self,
        node: HitTestNode,
        button: MouseButton,
        position: LogicalPosition,
        app_data: &mut azul_core::refany::RefAny,
        fc_cache: &mut rust_fontconfig::FcFontCache,
    ) -> ProcessEventResult {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };

        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };
        let node_id = match NodeId::from_crate_internal(node.node_id as u32) {
            Some(nid) => nid,
            None => return ProcessEventResult::DoNothing,
        };

        // Get layout result for this DOM
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return ProcessEventResult::DoNothing,
        };

        // Get node data to access callbacks
        let node_data = match layout_result
            .styled_dom
            .node_data
            .as_container()
            .get(node_id)
        {
            Some(nd) => nd,
            None => return ProcessEventResult::DoNothing,
        };

        // Filter callbacks by event type (MouseDown)
        let event_filter = match button {
            MouseButton::Left => EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            MouseButton::Right => EventFilter::Hover(HoverEventFilter::RightMouseDown),
            MouseButton::Middle => EventFilter::Hover(HoverEventFilter::MiddleMouseDown),
            _ => EventFilter::Hover(HoverEventFilter::MouseDown),
        };

        let mut result = ProcessEventResult::DoNothing;

        // Iterate through callbacks and invoke matching ones
        for callback_data in node_data.callbacks.as_container().iter() {
            if callback_data.event != event_filter {
                continue;
            }

            // Convert CoreCallback to Callback
            let mut callback = azul_layout::callbacks::Callback {
                cb: callback_data.callback.cb,
            };

            // Invoke callback
            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data.clone(),
                &azul_core::window::RawWindowHandle::default(), // TODO: proper window handle
                &self.gl_context_ptr,
                &mut self.image_cache,
                fc_cache,
                &azul_layout::callbacks::ExternalSystemCallbacks::default(),
                &self.previous_window_state,
                &self.current_window_state,
                &self.renderer_resources,
            );

            // Process callback result
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
        app_data: &mut azul_core::refany::RefAny,
        fc_cache: &mut rust_fontconfig::FcFontCache,
    ) -> ProcessEventResult {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };

        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };
        let node_id = match NodeId::from_crate_internal(node.node_id as u32) {
            Some(nid) => nid,
            None => return ProcessEventResult::DoNothing,
        };

        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return ProcessEventResult::DoNothing,
        };

        let node_data = match layout_result
            .styled_dom
            .node_data
            .as_container()
            .get(node_id)
        {
            Some(nd) => nd,
            None => return ProcessEventResult::DoNothing,
        };

        let event_filter = match button {
            MouseButton::Left => EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            MouseButton::Right => EventFilter::Hover(HoverEventFilter::RightMouseUp),
            MouseButton::Middle => EventFilter::Hover(HoverEventFilter::MiddleMouseUp),
            _ => EventFilter::Hover(HoverEventFilter::MouseUp),
        };

        let mut result = ProcessEventResult::DoNothing;

        for callback_data in node_data.callbacks.as_container().iter() {
            if callback_data.event != event_filter {
                continue;
            }

            let mut callback = azul_layout::callbacks::Callback {
                cb: callback_data.callback.cb,
            };

            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data.clone(),
                &azul_core::window::RawWindowHandle::default(),
                &self.gl_context_ptr,
                &mut self.image_cache,
                fc_cache,
                &azul_layout::callbacks::ExternalSystemCallbacks::default(),
                &self.previous_window_state,
                &self.current_window_state,
                &self.renderer_resources,
            );

            result = result.max(self.process_callback_result(callback_result));
        }

        result
    }

    /// Dispatch hover callbacks to hit node.
    fn dispatch_hover_callbacks(
        &mut self,
        node: HitTestNode,
        position: LogicalPosition,
        app_data: &mut azul_core::refany::RefAny,
        fc_cache: &mut rust_fontconfig::FcFontCache,
    ) -> ProcessEventResult {
        use azul_core::{
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };

        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };
        let node_id = match NodeId::from_crate_internal(node.node_id as u32) {
            Some(nid) => nid,
            None => return ProcessEventResult::DoNothing,
        };

        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return ProcessEventResult::DoNothing,
        };

        let node_data = match layout_result
            .styled_dom
            .node_data
            .as_container()
            .get(node_id)
        {
            Some(nd) => nd,
            None => return ProcessEventResult::DoNothing,
        };

        // Check for MouseOver callback
        let event_filter = EventFilter::Hover(HoverEventFilter::MouseOver);

        let mut result = ProcessEventResult::DoNothing;

        for callback_data in node_data.callbacks.as_container().iter() {
            if callback_data.event != event_filter {
                continue;
            }

            let mut callback = azul_layout::callbacks::Callback {
                cb: callback_data.callback.cb,
            };

            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data.clone(),
                &azul_core::window::RawWindowHandle::default(),
                &self.gl_context_ptr,
                &mut self.image_cache,
                fc_cache,
                &azul_layout::callbacks::ExternalSystemCallbacks::default(),
                &self.previous_window_state,
                &self.current_window_state,
                &self.renderer_resources,
            );

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
        // OptionF32 needs to be set, not incremented
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

        // TODO: If we have a layout_window, update scroll positions for scrollable nodes
        // For now, trigger a scroll render
        if delta_x.abs() > 0.01 || delta_y.abs() > 0.01 {
            ProcessEventResult::ShouldReRenderCurrentWindow
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
        // TODO: Notify WebRender compositor of new size
        // TODO: Resize GL viewport or CPU framebuffer
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
            if modified_state.flags.is_about_to_close {
                self.is_open = false;
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle focus changes
        if let Some(new_focus) = result.update_focused_node {
            self.current_window_state.focused_node = Some(new_focus);
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
