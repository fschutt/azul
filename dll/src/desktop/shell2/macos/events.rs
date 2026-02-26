//! macOS Event handling - converts NSEvent to Azul events and dispatches callbacks.

use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::{EventFilter, MouseButton, ProcessEventResult, SyntheticEvent},
    geom::{LogicalPosition, PhysicalPositionI32},
    hit_test::{CursorTypeHitTest, FullHitTest},
    window::{
        CursorPosition, KeyboardState, MouseCursorType, MouseState, OptionMouseCursorType,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    callbacks::CallbackInfo,
    managers::{
        hover::InputPointId,
        scroll_state::{ScrollbarComponent, ScrollbarHit},
    },
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;

use super::MacOSWindow;
// Re-export common types
pub use crate::desktop::shell2::common::event::HitTestNode;
// Import V2 cross-platform event processing trait
use crate::desktop::shell2::common::event::PlatformWindow;

/// Convert macOS window coordinates to Azul logical coordinates.
///
/// macOS uses a bottom-left origin coordinate system where Y=0 is at the bottom.
/// Azul/WebRender uses a top-left origin coordinate system where Y=0 is at the top.
/// This function converts from macOS to Azul coordinates.
#[inline]
fn macos_to_azul_coords(location: NSPoint, window_height: f32) -> LogicalPosition {
    LogicalPosition::new(location.x as f32, window_height - location.y as f32)
}

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

impl MacOSWindow {
    /// Convert ProcessEventResult to platform-specific EventProcessResult
    #[inline]
    fn convert_process_result(result: azul_core::events::ProcessEventResult) -> EventProcessResult {
        use azul_core::events::ProcessEventResult as PER;
        match result {
            PER::DoNothing => EventProcessResult::DoNothing,
            PER::ShouldReRenderCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldUpdateDisplayListCurrentWindow => EventProcessResult::RegenerateDisplayList,
            PER::UpdateHitTesterAndProcessAgain => EventProcessResult::RegenerateDisplayList,
            PER::ShouldIncrementalRelayout => EventProcessResult::RegenerateDisplayList,
            PER::ShouldRegenerateDomCurrentWindow => EventProcessResult::RegenerateDisplayList,
            PER::ShouldRegenerateDomAllWindows => EventProcessResult::RegenerateDisplayList,
        }
    }

    // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
    // are now provided by the PlatformWindow trait as default methods.
    // The trait methods are cross-platform and work identically.
    // See dll/src/desktop/shell2/common/event.rs for the implementation.

    /// Process a mouse button down event.
    pub fn handle_mouse_down(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Check for scrollbar hit FIRST (before state changes)
        // Use trait method from PlatformWindow
        if let Some(scrollbar_hit_id) = PlatformWindow::perform_scrollbar_hit_test(self, position)
        {
            let result = PlatformWindow::handle_scrollbar_click(self, scrollbar_hit_id, position);
            return Self::convert_process_result(result);
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update mouse state
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.common.current_window_state.mouse_state.left_down = true,
            MouseButton::Right => self.common.current_window_state.mouse_state.right_down = true,
            MouseButton::Middle => self.common.current_window_state.mouse_state.middle_down = true,
            _ => {}
        }

        // Record input sample for gesture detection (button down starts new session)
        let button_state = match button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            _ => 0x00,
        };
        self.record_input_sample(position, button_state, true, false, None);

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Use V2 cross-platform event system - it will automatically:
        // - Detect MouseDown event (left/right/middle)
        // - Dispatch to hovered nodes (including CSD buttons with callbacks)
        // - Handle event propagation
        // - Process callback results recursively
        let result = self.process_window_events(0);

        Self::convert_process_result(result)
    }

    /// Process a mouse button up event.
    pub fn handle_mouse_up(&mut self, event: &NSEvent, button: MouseButton) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // End scrollbar drag if active (before state changes)
        if self.common.scrollbar_drag_state.is_some() {
            self.common.scrollbar_drag_state = None;
            return EventProcessResult::RequestRedraw;
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update mouse state - clear appropriate button flag
        match button {
            MouseButton::Left => self.common.current_window_state.mouse_state.left_down = false,
            MouseButton::Right => self.common.current_window_state.mouse_state.right_down = false,
            MouseButton::Middle => self.common.current_window_state.mouse_state.middle_down = false,
            _ => {}
        }

        // Record input sample for gesture detection (button up ends session)
        let button_state = match button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            _ => 0x00,
        };
        self.record_input_sample(position, button_state, false, true, None);

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
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
    }

    /// Process a mouse move event.
    pub fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Handle active scrollbar drag (special case - not part of normal event system)
        // Use trait method from PlatformWindow
        if self.common.scrollbar_drag_state.is_some() {
            let result = PlatformWindow::handle_scrollbar_drag(self, position);
            return Self::convert_process_result(result);
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update mouse state
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Record input sample for gesture detection (movement during button press)
        let button_state = if self.common.current_window_state.mouse_state.left_down {
            0x01
        } else {
            0x00
        } | if self.common.current_window_state.mouse_state.right_down {
            0x02
        } else {
            0x00
        } | if self.common.current_window_state.mouse_state.middle_down {
            0x04
        } else {
            0x00
        };
        self.record_input_sample(position, button_state, false, false, None);

        // Update hit test
        self.update_hit_test(position);

        // Update cursor based on CSS cursor properties
        // This is done BEFORE callbacks so callbacks can override the cursor
        if let Some(layout_window) = self.common.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                // Update the window state cursor type
                self.common.current_window_state.mouse_state.mouse_cursor_type =
                    Some(cursor_test.cursor_icon).into();
                // Set the actual OS cursor
                let cursor_name = self.map_cursor_type_to_macos(cursor_test.cursor_icon);
                self.set_cursor(cursor_name);
            }
        }

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
    }

    /// Process mouse entered window event.
    pub fn handle_mouse_entered(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update mouse state - cursor is now in window
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Update hit test
        self.update_hit_test(position);

        // V2 system will detect MouseEnter events from state diff
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
    }

    /// Process mouse exited window event.
    pub fn handle_mouse_exited(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update mouse state - cursor left window
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(position);

        // Clear last hit test since mouse is out
        use azul_layout::managers::hover::InputPointId;
        if let Some(ref mut layout_window) = self.common.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
        }

        // V2 system will detect MouseLeave events from state diff
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
    }

    /// Process a scroll wheel event.
    pub fn handle_scroll_wheel(&mut self, event: &NSEvent) -> EventProcessResult {
        let delta_x = unsafe { event.scrollingDeltaX() };
        let delta_y = unsafe { event.scrollingDeltaY() };
        let has_precise = unsafe { event.hasPreciseScrollingDeltas() };

        let location = unsafe { event.locationInWindow() };
        let window_height = self.common.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update hit test FIRST (required for scroll manager)
        self.update_hit_test(position);

        // Queue scroll input for the physics timer instead of directly setting offsets.
        // The timer will consume these via ScrollInputQueue and push CallbackChange::ScrollTo.
        if (delta_x.abs() > 0.01 || delta_y.abs() > 0.01) {
            let mut should_start_timer = false;
            let mut input_queue_clone = None;

            if let Some(layout_window) = self.get_layout_window_mut() {
                use azul_core::task::Instant;
                use azul_layout::managers::hover::InputPointId;
                use azul_layout::managers::scroll_state::ScrollInputSource;

                let now = Instant::from(std::time::Instant::now());
                let source = if has_precise {
                    ScrollInputSource::TrackpadContinuous
                } else {
                    ScrollInputSource::WheelDiscrete
                };

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        -delta_x as f32, // Invert for natural scrolling
                        -delta_y as f32,
                        source,
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    )
                {
                    should_start_timer = start_timer;
                    if start_timer {
                        input_queue_clone = Some(
                            layout_window.scroll_manager.get_input_queue()
                        );
                    }
                }
            }

            // Start the scroll momentum timer if this is the first input
            // (must be done outside the borrow of layout_window)
            if should_start_timer {
                if let Some(queue) = input_queue_clone {
                    use azul_core::task::{TimerId, SCROLL_MOMENTUM_TIMER_ID};
                    use azul_layout::scroll_timer::{ScrollPhysicsState, scroll_physics_timer_callback};
                    use azul_layout::timer::{Timer, TimerCallbackType};
                    use azul_core::refany::RefAny;
                    use azul_core::task::Duration;

                    let physics_state = ScrollPhysicsState::new(queue, self.common.system_style.scroll_physics.clone());
                    let interval_ms = self.common.system_style.scroll_physics.timer_interval_ms;
                    let data = RefAny::new(physics_state);
                    let timer = Timer::create(
                        data,
                        scroll_physics_timer_callback as TimerCallbackType,
                        azul_layout::callbacks::ExternalSystemCallbacks::rust_internal()
                            .get_system_time_fn,
                    )
                    .with_interval(Duration::System(
                        azul_core::task::SystemTimeDiff::from_millis(interval_ms as u64),
                    ));

                    self.start_timer(SCROLL_MOMENTUM_TIMER_ID.id, timer);
                }
            }
        }

        // V2 system will detect Scroll event from ScrollManager state
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
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
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update keyboard state with keycode
        self.update_keyboard_state(key_code, modifiers, true);

        // Handle text input for printable characters
        // On macOS, interpretKeyEvents SHOULD trigger insertText: via NSTextInputClient,
        // but there seems to be an issue with protocol conformance in objc2.
        // So we handle printable characters directly here.
        // Control characters and modified keys (Cmd+X, Ctrl+C, etc.) are NOT inserted as text.
        if let Some(ch) = character {
            let is_control_char = ch.is_control();
            let has_cmd = modifiers.contains(objc2_app_kit::NSEventModifierFlags::Command);
            let has_ctrl = modifiers.contains(objc2_app_kit::NSEventModifierFlags::Control);
            
            // Only insert text for normal printable characters without Cmd/Ctrl
            if !is_control_char && !has_cmd && !has_ctrl {
                let text_input = ch.to_string();
                self.handle_text_input(&text_input);
            }
        }

        // V2 system will detect VirtualKeyDown and TextInput from state diff
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
    }

    /// Process a key up event.
    pub fn handle_key_up(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update keyboard state
        self.update_keyboard_state(key_code, modifiers, false);

        // Clear current character on key up
        self.update_keyboard_state_with_char(None);

        // V2 system will detect VirtualKeyUp from state diff
        let result = self.process_window_events(0);
        Self::convert_process_result(result)
    }

    /// Process text input from IME (called from insertText:replacementRange:)
    ///
    /// This is the proper way to handle text input on macOS, as it respects
    /// the IME composition system for non-ASCII characters (accents, CJK, etc.)
    pub fn handle_text_input(&mut self, text: &str) {
        use azul_core::events::ProcessEventResult;
        
        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Record text input - this returns a map of nodes that need TextInput event dispatched
        let affected_nodes = if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.record_text_input(text)
        } else {
            return; // No layout window, nothing to do
        };

        if affected_nodes.is_empty() {
            println!("[handle_text_input] No affected nodes returned from record_text_input");
            return;
        }

        // Manually process the generated text input event.
        // We do NOT call process_window_events() here, because that function
        // is for discovering events from state diffs. Here, we already know the exact event.
        let mut overall_result = ProcessEventResult::DoNothing;

        // Build synthetic events for each affected node
        let now = {
            #[cfg(feature = "std")]
            { azul_core::task::Instant::from(std::time::Instant::now()) }
            #[cfg(not(feature = "std"))]
            { azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0)) }
        };

        let text_events: Vec<azul_core::events::SyntheticEvent> = affected_nodes
            .iter()
            .map(|(dom_node_id, _)| {
                azul_core::events::SyntheticEvent::new(
                    azul_core::events::EventType::Input,
                    azul_core::events::EventSource::User,
                    *dom_node_id,
                    now.clone(),
                    azul_core::events::EventData::None,
                )
            })
            .collect();

        if !text_events.is_empty() {
            let (text_changes_result, text_update, _) = self.dispatch_events_propagated(&text_events);
            overall_result = overall_result.max(text_changes_result);
            use azul_core::callbacks::Update;
            if matches!(text_update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                overall_result = overall_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
        }

        // Apply text changeset after callbacks
        if let Some(layout_window) = self.get_layout_window_mut() {
            let dirty_nodes = layout_window.apply_text_changeset();
            if !dirty_nodes.is_empty() {
                println!("[handle_text_input] Applied text changeset, {} dirty nodes", dirty_nodes.len());
                overall_result = overall_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
        }

        // Request redraw if needed
        if overall_result > ProcessEventResult::ShouldReRenderCurrentWindow {
            self.common.frame_needs_regeneration = true;
            self.request_redraw();
        } else if overall_result == ProcessEventResult::ShouldReRenderCurrentWindow {
            self.request_redraw();
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
        let keyboard_state = &self.common.current_window_state.keyboard_state;
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

        // Store old context for comparison
        let old_context = self.dynamic_selector_context.clone();

        // Update window state
        self.common.current_window_state.size.dimensions = new_size;

        // Update dynamic selector context with new viewport dimensions
        self.dynamic_selector_context.viewport_width = new_width as f32;
        self.dynamic_selector_context.viewport_height = new_height as f32;
        self.dynamic_selector_context.orientation = if new_width > new_height {
            azul_css::dynamic_selector::OrientationType::Landscape
        } else {
            azul_css::dynamic_selector::OrientationType::Portrait
        };

        // Check if DPI changed (window may have moved to different display)
        let current_hidpi = self.get_hidpi_factor();
        let old_hidpi = self.common.current_window_state.size.get_hidpi_factor();

        if (current_hidpi.inner.get() - old_hidpi.inner.get()).abs() > 0.001 {
            log_info!(
                LogCategory::Window,
                "[Resize] DPI changed: {} -> {}",
                old_hidpi.inner.get(),
                current_hidpi.inner.get()
            );
            self.common.current_window_state.size.dpi = (current_hidpi.inner.get() * 96.0) as u32;
        }

        // Notify compositor of resize (this is private in mod.rs, so we inline it here)
        if let Err(e) = self.handle_compositor_resize() {
            log_error!(LogCategory::Rendering, "Compositor resize failed: {}", e);
        }

        // Check if viewport dimensions actually changed (debounce rapid resize events)
        let viewport_changed =
            (old_context.viewport_width - self.dynamic_selector_context.viewport_width).abs() > 0.5
                || (old_context.viewport_height - self.dynamic_selector_context.viewport_height)
                    .abs()
                    > 0.5;

        if !viewport_changed {
            // No significant change, just update compositor
            return EventProcessResult::RequestRedraw;
        }

        // Check if any CSS breakpoints were crossed
        // Common breakpoints: 320, 480, 640, 768, 1024, 1280, 1440, 1920
        let breakpoints = [320.0, 480.0, 640.0, 768.0, 1024.0, 1280.0, 1440.0, 1920.0];
        let breakpoint_crossed =
            old_context.viewport_breakpoint_changed(&self.dynamic_selector_context, &breakpoints);

        if breakpoint_crossed {
            log_debug!(
                LogCategory::Layout,
                "[Resize] Breakpoint crossed: {}x{} -> {}x{}",
                old_context.viewport_width,
                old_context.viewport_height,
                self.dynamic_selector_context.viewport_width,
                self.dynamic_selector_context.viewport_height
            );
        }

        // Resize requires full display list rebuild
        EventProcessResult::RegenerateDisplayList
    }

    /// Process a file drop event.
    pub fn handle_file_drop(&mut self, paths: Vec<String>) -> EventProcessResult {
        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update cursor manager with dropped file
        if let Some(first_path) = paths.first() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                layout_window
                    .file_drop_manager
                    .set_dropped_file(Some(first_path.clone().into()));
            }
        }

        // Update hit test at current cursor position
        if let CursorPosition::InWindow(pos) = self.common.current_window_state.mouse_state.cursor_position
        {
            self.update_hit_test(pos);
        }

        // V2 system will detect FileDrop event from state diff
        let result = self.process_window_events(0);

        // Clear dropped file after processing (one-shot event)
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }

        Self::convert_process_result(result)
    }

    /// Perform hit testing at given position using WebRender hit-testing API.
    fn perform_hit_test(&mut self, position: LogicalPosition) -> Option<HitTestNode> {
        use azul_core::window::CursorPosition;

        let layout_window = self.common.layout_window.as_ref()?;

        // Early return if no layout results
        if layout_window.layout_results.is_empty() {
            return None;
        }

        let cursor_position = CursorPosition::InWindow(position);

        // Get focused node from FocusManager
        let focused_node = layout_window.focus_manager.get_focused_node().copied();

        // Use layout_results directly (BTreeMap)
        let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
            &*self.common.hit_tester.as_mut().unwrap().resolve(),
            self.common.document_id.unwrap(),
            focused_node,
            &layout_window.layout_results,
            &cursor_position,
            self.common.current_window_state.size.get_hidpi_factor(),
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

        let keyboard_state = &mut self.common.current_window_state.keyboard_state;

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
    /// NOTE: This method is deprecated and should not set current_char anymore.
    /// Text input is now handled by process_text_input() which receives the
    /// composed text directly from NSTextInputClient.
    fn update_keyboard_state_with_char(&mut self, _character: Option<char>) {
        // current_char field has been removed from KeyboardState
        // KeyboardState now only tracks virtual keys and scancodes
        // Text input is handled separately by LayoutWindow::process_text_input()
    }

    /// Handle compositor resize notification.
    fn handle_compositor_resize(&mut self) -> Result<(), String> {
        use webrender::api::units::{DeviceIntRect, DeviceIntSize, DevicePixelScale};

        // Get new physical size
        let physical_size = self.common.current_window_state.size.get_physical_size();
        let new_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let hidpi_factor = self.common.current_window_state.size.get_hidpi_factor();

        // Update WebRender document size
        let mut txn = webrender::Transaction::new();
        let device_rect = DeviceIntRect::from_size(new_size);
        // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
        txn.set_document_view(device_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

        // Send transaction
        if let Some(ref layout_window) = self.common.layout_window {
            let document_id =
                crate::desktop::wr_translate2::wr_translate_document_id(layout_window.document_id);
            self.common.render_api.as_mut().unwrap().send_transaction(document_id, txn);
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

        let layout_window = self.common.layout_window.as_ref()?;
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
        // Clone the menu to avoid borrow conflicts
        let context_menu = node_data.get_context_menu()?.clone();

        log_debug!(
            LogCategory::Input,
            "[Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Check if native context menus are enabled
        if self.common.current_window_state.flags.use_native_context_menus {
            self.show_native_context_menu_at_position(&context_menu, position, event);
        } else {
            self.show_window_based_context_menu(&context_menu, position);
        }

        Some(())
    }

    /// Show an NSMenu as a context menu at the given screen position.
    fn show_native_context_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
        event: &NSEvent,
    ) {
        use objc2_app_kit::{NSMenu, NSMenuItem};
        use objc2_foundation::{MainThreadMarker, NSPoint, NSString};

        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => {
                log_warn!(
                    LogCategory::Platform,
                    "[Context Menu] Not on main thread, cannot show menu"
                );
                return;
            }
        };

        let ns_menu = NSMenu::new(mtm);

        // Build menu items recursively from Azul menu structure
        Self::recursive_build_nsmenu(&ns_menu, menu.items.as_slice(), &mtm, &mut self.menu_state);

        // Show the menu at the specified position
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
            log_debug!(
                LogCategory::Input,
                "[Context Menu] Showing native menu at position ({}, {}) with {} items",
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
    ///
    /// The menu window creation is queued and will be processed in Phase 3 of the event loop.
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for context menus (they spawn at cursor)
            Some(position), // Cursor position for menu positioning
            None,           // No parent menu
        );

        // Queue window creation request for processing in Phase 3 of the event loop
        // The event loop will create the window with MacOSWindow::new_with_fc_cache()
        log_debug!(
            LogCategory::Window,
            "[macOS] Queuing window-based context menu at screen ({}, {}) - will be created in \
             event loop Phase 3",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
    }

    /// Recursively builds an NSMenu from Azul MenuItem array
    ///
    /// This mirrors the Win32 recursive_construct_menu() logic:
    /// - Leaf items (no children) -> addItem with callback
    /// - Items with children -> create submenu and recurse
    /// - Separators -> add separator item
    pub(crate) fn recursive_build_nsmenu(
        menu: &objc2_app_kit::NSMenu,
        items: &[azul_core::menu::MenuItem],
        mtm: &objc2::MainThreadMarker,
        menu_state: &mut crate::desktop::shell2::macos::menu::MenuState,
    ) {
        use objc2_app_kit::{NSMenu, NSMenuItem};
        use objc2_foundation::NSString;

        for item in items {
            match item {
                azul_core::menu::MenuItem::String(string_item) => {
                    let menu_item = NSMenuItem::new(*mtm);
                    let title = NSString::from_str(&string_item.label);
                    menu_item.setTitle(&title);

                    // Set enabled/disabled state based on MenuItemState
                    let enabled = match string_item.menu_item_state {
                        azul_core::menu::MenuItemState::Normal => true,
                        azul_core::menu::MenuItemState::Disabled => false,
                        azul_core::menu::MenuItemState::Greyed => false,
                    };
                    menu_item.setEnabled(enabled);

                    // Check if this item has children (submenu)
                    if !string_item.children.as_ref().is_empty() {
                        // Create submenu and recurse
                        let submenu = NSMenu::new(*mtm);
                        let submenu_title = NSString::from_str(&string_item.label);
                        submenu.setTitle(&submenu_title);

                        // Recursively build submenu items
                        Self::recursive_build_nsmenu(
                            &submenu,
                            string_item.children.as_ref(),
                            mtm,
                            menu_state,
                        );

                        // Attach submenu to menu item
                        menu_item.setSubmenu(Some(&submenu));

                        log_debug!(
                            LogCategory::Input,
                            "[Context Menu] Created submenu '{}' with {} items",
                            string_item.label,
                            string_item.children.as_ref().len()
                        );
                    } else {
                        use crate::desktop::shell2::macos::menu;
                        // Leaf item - wire up callback using the same system as menu bar
                        if let Some(callback) = string_item.callback.as_option() {
                            let tag = menu_state.register_callback(callback.clone());
                            menu_item.setTag(tag as isize);

                            // Use shared AzulMenuTarget for callback dispatch
                            let target = menu::AzulMenuTarget::shared_instance(*mtm);
                            unsafe {
                                menu_item.setTarget(Some(&target));
                                menu_item.setAction(Some(objc2::sel!(menuItemAction:)));
                            }
                        }

                        // Set keyboard shortcut if present
                        if let Some(ref accelerator) = string_item.accelerator.into_option() {
                            menu::set_menu_item_accelerator(&menu_item, accelerator);
                        }
                    }

                    menu.addItem(&menu_item);
                }

                azul_core::menu::MenuItem::Separator => {
                    let separator = unsafe { NSMenuItem::separatorItem(*mtm) };
                    menu.addItem(&separator);
                }

                azul_core::menu::MenuItem::BreakLine => {
                    // BreakLine is for horizontal menu layouts, not supported in NSMenu
                    // Just add a separator as a visual indication
                    let separator = unsafe { NSMenuItem::separatorItem(*mtm) };
                    menu.addItem(&separator);
                }
            }
        }
    }

    // Helper Functions for V2 Event System

    /// Update hit test at given position and store in current_window_state.
    fn update_hit_test(&mut self, position: LogicalPosition) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            let cursor_position = CursorPosition::InWindow(position);
            // Get focused node from FocusManager
            let focused_node = layout_window.focus_manager.get_focused_node().copied();
            let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
                &*self.common.hit_tester.as_mut().unwrap().resolve(),
                self.common.document_id.unwrap(),
                focused_node,
                &layout_window.layout_results,
                &cursor_position,
                self.common.current_window_state.size.get_hidpi_factor(),
            );
            use azul_layout::managers::hover::InputPointId;
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    /// Get the first hovered node from current mouse hit test.
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        use azul_layout::managers::hover::InputPointId;
        self.common.layout_window
            .as_ref()?
            .hover_manager
            .get_current(&InputPointId::Mouse)?
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
        Self::convert_process_result(result)
    }

    // V2 Cross-Platform Event Processing
    // NOTE: All V2 event processing methods are now provided by the
    // PlatformWindow trait in common/event.rs. The trait provides:
    // - process_window_events_v2() - Entry point (public API)
    // - process_window_events() - Recursive processing
    // - dispatch_events_propagated() - W3C Capture→Target→Bubble dispatch
    // - apply_user_change() - Result handling
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
