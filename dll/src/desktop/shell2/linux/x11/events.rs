//! X11 Event handling - Cross-platform V2 event system with state-diffing
//!
//! This module implements the same event processing architecture as Windows and macOS:
//! 1. Save previous_window_state before modifying current_window_state
//! 2. Update current_window_state based on X11 events
//! 3. Use create_events_from_states() to detect changes via state diffing
//! 4. Use dispatch_events() to determine which callbacks to invoke
//! 5. Invoke callbacks recursively with depth limit
//! 6. Process callback results (DOM regeneration, window state changes, etc.)
//!
//! Includes full IME (XIM) support for international text input.

use std::{
    ffi::{CStr, CString},
    rc::Rc,
};

use azul_core::{
    callbacks::Update,
    dom::{DomId, NodeId},
    events::{EventFilter, MouseButton, ProcessEventResult},
    geom::{LogicalPosition, PhysicalPosition},
    hit_test::FullHitTest,
    window::{CursorPosition, VirtualKeyCode},
};
use azul_layout::{
    managers::hover::InputPointId,
};

use super::{defines::*, dlopen::Xlib, X11Window};
use crate::desktop::shell2::common::event::PlatformWindow;

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

// IME Support (X Input Method)

pub(super) struct ImeManager {
    xlib: Rc<Xlib>,
    xim: XIM,
    xic: XIC,
}

impl ImeManager {
    pub(super) fn new(xlib: &Rc<Xlib>, display: *mut Display, window: Window) -> Option<Self> {
        unsafe {
            // Set the locale. This is crucial for XIM to work correctly.
            let locale = CString::new("").unwrap();
            (xlib.XSetLocaleModifiers)(locale.as_ptr());

            let xim = (xlib.XOpenIM)(
                display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if xim.is_null() {
                log_warn!(
                    LogCategory::Input,
                    "[X11 IME] Could not open input method. IME will not be available."
                );
                return None;
            }

            let client_window_str = CString::new("clientWindow").unwrap();
            let input_style_str = CString::new("inputStyle").unwrap();

            let xic = (xlib.XCreateIC)(
                xim,
                input_style_str.as_ptr(),
                XIMPreeditNothing | XIMStatusNothing,
                client_window_str.as_ptr(),
                window,
                std::ptr::null_mut() as *const i8, // Sentinel
            );

            if xic.is_null() {
                log_warn!(
                    LogCategory::Input,
                    "[X11 IME] Could not create input context. IME will not be available."
                );
                (xlib.XCloseIM)(xim);
                return None;
            }

            (xlib.XSetICFocus)(xic);

            Some(Self {
                xlib: xlib.clone(),
                xim,
                xic,
            })
        }
    }

    /// Get the XIC (X Input Context) for setting IME properties
    pub(super) fn get_xic(&self) -> XIC {
        self.xic
    }

    /// Filters an event through the IME.
    /// Returns `true` if the event was consumed by the IME.
    pub(super) fn filter_event(&self, event: &mut XEvent) -> bool {
        unsafe { (self.xlib.XFilterEvent)(event, 0) != 0 }
    }

    /// Translates a key event into a character and a keysym, considering the IME.
    pub(super) fn lookup_string(&self, event: &mut XKeyEvent) -> (Option<String>, Option<KeySym>) {
        let mut keysym: KeySym = 0;
        let mut status: i32 = 0;
        let mut buffer: [i8; 32] = [0; 32];

        let count = unsafe {
            (self.xlib.XmbLookupString)(
                self.xic,
                event,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut keysym,
                &mut status,
            )
        };

        let chars = if count > 0 {
            Some(unsafe {
                CStr::from_ptr(buffer.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            })
        } else {
            None
        };

        let keysym = if keysym != 0 { Some(keysym) } else { None };

        (chars, keysym)
    }
}

impl Drop for ImeManager {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XDestroyIC)(self.xic);
            (self.xlib.XCloseIM)(self.xim);
        }
    }
}

// Event Handler - Main Implementation

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

impl X11Window {
    // V2 Cross-Platform Event Processing (from macOS/Windows)

    // Event Handlers (State-Diffing Pattern)

    /// Handle mouse button press/release events
    pub fn handle_mouse_button(&mut self, event: &XButtonEvent) -> ProcessEventResult {
        let is_down = event.type_ == ButtonPress;
        let position = LogicalPosition::new(event.x as f32, event.y as f32);

        // Map X11 button to MouseButton
        let button = match event.button {
            1 => MouseButton::Left,
            2 => MouseButton::Middle,
            3 => MouseButton::Right,
            4 if is_down => {
                // Scroll up - handle separately
                return self.handle_scroll(0.0, 1.0, position);
            }
            5 if is_down => {
                // Scroll down - handle separately
                return self.handle_scroll(0.0, -1.0, position);
            }
            _ => MouseButton::Other(event.button as u8),
        };

        // Check for scrollbar hit FIRST (before state changes)
        if is_down {
            if let Some(scrollbar_hit_id) =
                PlatformWindow::perform_scrollbar_hit_test(self, position)
            {
                return PlatformWindow::handle_scrollbar_click(self, scrollbar_hit_id, position);
            }
        } else {
            // End scrollbar drag if active
            if self.common.scrollbar_drag_state.is_some() {
                self.common.scrollbar_drag_state = None;
                return ProcessEventResult::ShouldReRenderCurrentWindow;
            }
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.common.current_window_state.mouse_state.left_down = is_down,
            MouseButton::Right => self.common.current_window_state.mouse_state.right_down = is_down,
            MouseButton::Middle => self.common.current_window_state.mouse_state.middle_down = is_down,
            _ => {}
        }

        // Record input sample for gesture detection
        // X11 provides x_root/y_root as native screen-absolute coordinates
        let button_state = match button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            _ => 0x00,
        };
        let screen_pos = LogicalPosition::new(event.x_root as f32, event.y_root as f32);
        self.record_input_sample(position, button_state, is_down, !is_down, Some(screen_pos));

        // Update hit test
        self.update_hit_test(position);

        // Check for right-click context menu (before event processing)
        if !is_down && button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                if self.try_show_context_menu(hit_node, position) {
                    return ProcessEventResult::DoNothing;
                }
            }
        }

        // V2 system will automatically detect MouseDown/MouseUp and dispatch callbacks
        self.process_window_events(0)
    }

    /// Handle mouse motion events
    pub fn handle_mouse_move(&mut self, event: &XMotionEvent) -> ProcessEventResult {
        let position = LogicalPosition::new(event.x as f32, event.y as f32);

        // Handle active scrollbar drag (special case - not part of normal event system)
        if self.common.scrollbar_drag_state.is_some() {
            return PlatformWindow::handle_scrollbar_drag(self, position);
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state
        self.common.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Record input sample for gesture detection (movement during button press)
        // X11 provides x_root/y_root as native screen-absolute coordinates
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
        let screen_pos = LogicalPosition::new(event.x_root as f32, event.y_root as f32);
        self.record_input_sample(position, button_state, false, false, Some(screen_pos));

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
                self.set_cursor(cursor_test.cursor_icon);
            }
        }

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        self.process_window_events(0)
    }

    /// Handle mouse entering/leaving window
    pub fn handle_mouse_crossing(&mut self, event: &XCrossingEvent) -> ProcessEventResult {
        let position = LogicalPosition::new(event.x as f32, event.y as f32);

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update modifier state from X11 event state field
        self.update_modifiers_from_x11_state(event.state);

        // Update mouse state based on enter/leave
        if event.type_ == EnterNotify {
            self.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(position);
            self.update_hit_test(position);
        } else if event.type_ == LeaveNotify {
            self.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::OutOfWindow(position);
            // Clear hit test since mouse is out
            if let Some(ref mut layout_window) = self.common.layout_window {
                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
            }
        }

        // V2 system will detect MouseEnter/MouseLeave from state diff
        self.process_window_events(0)
    }

    /// Handle scroll wheel events (X11 button 4/5)
    fn handle_scroll(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Update hit test
        self.update_hit_test(position);

        // Queue scroll input for the physics timer instead of directly setting offsets.
        {
            let mut should_start_timer = false;
            let mut input_queue_clone = None;

            if let Some(ref mut layout_window) = self.common.layout_window {
                use azul_core::task::Instant;
                use azul_layout::managers::scroll_state::ScrollInputSource;

                let now = Instant::from(std::time::Instant::now());

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        -delta_x * 20.0,
                        -delta_y * 20.0,
                        ScrollInputSource::WheelDiscrete,
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
            if should_start_timer {
                if let Some(queue) = input_queue_clone {
                    use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                    use azul_layout::scroll_timer::{ScrollPhysicsState, scroll_physics_timer_callback};
                    use azul_layout::timer::{Timer, TimerCallbackType};
                    use azul_core::refany::RefAny;
                    use azul_core::task::Duration;

                    let physics_state = ScrollPhysicsState::new(queue, self.resources.system_style.scroll_physics.clone());
                    let interval_ms = self.resources.system_style.scroll_physics.timer_interval_ms;
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

        // V2 system will detect Scroll event from recorded state
        self.process_window_events(0)
    }

    /// Handle keyboard events (key press/release)
    pub fn handle_keyboard(&mut self, event: &mut XKeyEvent) -> ProcessEventResult {
        let is_down = event.type_ == KeyPress;

        // Use IME for character translation
        let (char_str, keysym) = if let Some(ime) = &self.ime_manager {
            ime.lookup_string(event)
        } else {
            // Fallback for when IME is not available
            let mut keysym: KeySym = 0;
            let mut buffer = [0; 32];
            let count = unsafe {
                (self.xlib.XLookupString)(
                    event,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                    &mut keysym,
                    std::ptr::null_mut(),
                )
            };
            let chars = if count > 0 {
                unsafe {
                    CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                }
            } else {
                String::new()
            };
            (Some(chars), Some(keysym))
        };

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Record text input if we have a character and it's a key press
        if is_down {
            if let Some(ref text) = char_str {
                if !text.is_empty() {
                    if let Some(ref mut layout_window) = self.common.layout_window {
                        layout_window.record_text_input(text);
                    }
                }
            }
        }

        // Update keyboard state with virtual key and scancode
        if let Some(vk) = keysym.and_then(keysym_to_virtual_keycode) {
            if is_down {
                self.common.current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .insert_hm_item(vk);
                self.common.current_window_state
                    .keyboard_state
                    .current_virtual_keycode = Some(vk).into();

                // Track scancode (X11 keycode is the scancode)
                self.common.current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .insert_hm_item(event.keycode as u32);
            } else {
                self.common.current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .remove_hm_item(&vk);
                self.common.current_window_state
                    .keyboard_state
                    .current_virtual_keycode = None.into();

                // Remove scancode
                self.common.current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .remove_hm_item(&(event.keycode as u32));
            }
        }

        // Character input is now handled by V2 event system
        // current_char field has been removed from KeyboardState

        // V2 system will detect VirtualKeyDown/VirtualKeyUp/TextInput from state diff
        self.process_window_events(0)
    }

    // Helper Functions for V2 Event System

    /// Update keyboard state based on X11 event state field.
    ///
    /// X11 events (XButtonEvent, XMotionEvent, XCrossingEvent, XKeyEvent) contain a `state`
    /// field that indicates which modifier keys were held when the event occurred.
    /// This function synchronizes the KeyboardState with that information.
    fn update_modifiers_from_x11_state(&mut self, state: std::ffi::c_uint) {
        use azul_core::window::VirtualKeyCode;

        // Check each modifier mask and update the keyboard state accordingly
        let keyboard_state = &mut self.common.current_window_state.keyboard_state;

        // Shift
        let shift_down = (state & SHIFT_MASK) != 0;
        if shift_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LShift);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LShift);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RShift);
        }

        // Control
        let ctrl_down = (state & CONTROL_MASK) != 0;
        if ctrl_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LControl);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LControl);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RControl);
        }

        // Alt (Mod1)
        let alt_down = (state & MOD1_MASK) != 0;
        if alt_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LAlt);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LAlt);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RAlt);
        }

        // Super/Windows (Mod4)
        let super_down = (state & MOD4_MASK) != 0;
        if super_down {
            keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(VirtualKeyCode::LWin);
        } else {
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::LWin);
            keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&VirtualKeyCode::RWin);
        }
    }

    /// Update hit test at given position and store in current_window_state
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
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    /// Get the first hovered node from current hit test
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
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

    /// Get raw window handle for callbacks
    fn get_raw_window_handle(&self) -> azul_core::window::RawWindowHandle {
        azul_core::window::RawWindowHandle::Xlib(azul_core::window::XlibHandle {
            window: self.window as u64,
            display: self.display as *mut std::ffi::c_void,
        })
    }

    // Scrollbar Handling (from Windows/macOS)

    /// Query WebRender hit-tester for scrollbar hits at given position
    // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
    // are now provided by the PlatformWindow trait as default methods.
    // The trait methods are cross-platform and work identically.
    // See dll/src/desktop/shell2/common/event.rs for the implementation.
    //
    // X11-specific note: X11 doesn't have native mouse capture like Windows/macOS,
    // so we rely on the event loop to deliver motion events during drag.

    // Context Menu Support

    /// Try to show context menu for the given node at position
    ///
    /// Uses the unified menu system (crate::desktop::menu::show_menu) which is identical
    /// to how menu bar menus work, but spawns at cursor position instead of below a trigger rect.
    /// Returns true if a menu was shown
    fn try_show_context_menu(&mut self, node: HitTestNode, position: LogicalPosition) -> bool {
        let layout_window = match self.common.layout_window.as_ref() {
            Some(lw) => lw,
            None => return false,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return false,
        };

        // Check if this node has a context menu
        let node_id = match azul_core::id::NodeId::from_usize(node.node_id as usize) {
            Some(nid) => nid,
            None => return false,
        };

        let binding = layout_result.styled_dom.node_data.as_container();
        let node_data = match binding.get(node_id) {
            Some(nd) => nd,
            None => return false,
        };

        // Context menus are stored directly on NodeData
        // Clone to avoid borrow conflict (same pattern as macOS)
        let context_menu = match node_data.get_context_menu() {
            Some(menu) => (**menu).clone(),
            None => return false,
        };

        log_debug!(
            LogCategory::Input,
            "[X11 Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Queue the window creation instead of creating immediately
        self.show_window_based_context_menu(&context_menu, position);
        true
    }

    /// Queue a window-based context menu for creation in the event loop
    /// This is part of the unified multi-window menu system (Shell2 V2)
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using unified menu system
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.resources.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for context menus
            Some(position), // Cursor position
            None,           // No parent menu
        );

        log_debug!(
            LogCategory::Window,
            "[X11] Queuing window-based context menu at screen ({}, {})",
            position.x,
            position.y
        );
        self.pending_window_creates.push(menu_options);
    }
}

// Extension Trait for Callback Conversion

trait CallbackExt {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self;
}

impl CallbackExt for azul_layout::callbacks::Callback {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self {
        // Use the existing safe wrapper method from Callback
        azul_layout::callbacks::Callback::from_core(core_callback)
    }
}

// Keycode Conversion

pub fn keysym_to_virtual_keycode(keysym: KeySym) -> Option<VirtualKeyCode> {
    // This is a partial mapping based on X11/keysymdef.h
    match keysym as u32 {
        XK_BackSpace => Some(VirtualKeyCode::Back),
        XK_Tab => Some(VirtualKeyCode::Tab),
        XK_Return => Some(VirtualKeyCode::Return),
        XK_Pause => Some(VirtualKeyCode::Pause),
        XK_Scroll_Lock => Some(VirtualKeyCode::Scroll),
        XK_Escape => Some(VirtualKeyCode::Escape),
        XK_Home => Some(VirtualKeyCode::Home),
        XK_Left => Some(VirtualKeyCode::Left),
        XK_Up => Some(VirtualKeyCode::Up),
        XK_Right => Some(VirtualKeyCode::Right),
        XK_Down => Some(VirtualKeyCode::Down),
        XK_Page_Up => Some(VirtualKeyCode::PageUp),
        XK_Page_Down => Some(VirtualKeyCode::PageDown),
        XK_End => Some(VirtualKeyCode::End),
        XK_Insert => Some(VirtualKeyCode::Insert),
        XK_Delete => Some(VirtualKeyCode::Delete),
        XK_space => Some(VirtualKeyCode::Space),
        XK_0 => Some(VirtualKeyCode::Key0),
        XK_1 => Some(VirtualKeyCode::Key1),
        XK_2 => Some(VirtualKeyCode::Key2),
        XK_3 => Some(VirtualKeyCode::Key3),
        XK_4 => Some(VirtualKeyCode::Key4),
        XK_5 => Some(VirtualKeyCode::Key5),
        XK_6 => Some(VirtualKeyCode::Key6),
        XK_7 => Some(VirtualKeyCode::Key7),
        XK_8 => Some(VirtualKeyCode::Key8),
        XK_9 => Some(VirtualKeyCode::Key9),
        XK_a | XK_A => Some(VirtualKeyCode::A),
        XK_b | XK_B => Some(VirtualKeyCode::B),
        XK_c | XK_C => Some(VirtualKeyCode::C),
        XK_d | XK_D => Some(VirtualKeyCode::D),
        XK_e | XK_E => Some(VirtualKeyCode::E),
        XK_f | XK_F => Some(VirtualKeyCode::F),
        XK_g | XK_G => Some(VirtualKeyCode::G),
        XK_h | XK_H => Some(VirtualKeyCode::H),
        XK_i | XK_I => Some(VirtualKeyCode::I),
        XK_j | XK_J => Some(VirtualKeyCode::J),
        XK_k | XK_K => Some(VirtualKeyCode::K),
        XK_l | XK_L => Some(VirtualKeyCode::L),
        XK_m | XK_M => Some(VirtualKeyCode::M),
        XK_n | XK_N => Some(VirtualKeyCode::N),
        XK_o | XK_O => Some(VirtualKeyCode::O),
        XK_p | XK_P => Some(VirtualKeyCode::P),
        XK_q | XK_Q => Some(VirtualKeyCode::Q),
        XK_r | XK_R => Some(VirtualKeyCode::R),
        XK_s | XK_S => Some(VirtualKeyCode::S),
        XK_t | XK_T => Some(VirtualKeyCode::T),
        XK_u | XK_U => Some(VirtualKeyCode::U),
        XK_v | XK_V => Some(VirtualKeyCode::V),
        XK_w | XK_W => Some(VirtualKeyCode::W),
        XK_x | XK_X => Some(VirtualKeyCode::X),
        XK_y | XK_Y => Some(VirtualKeyCode::Y),
        XK_z | XK_Z => Some(VirtualKeyCode::Z),
        XK_F1 => Some(VirtualKeyCode::F1),
        XK_F2 => Some(VirtualKeyCode::F2),
        XK_F3 => Some(VirtualKeyCode::F3),
        XK_F4 => Some(VirtualKeyCode::F4),
        XK_F5 => Some(VirtualKeyCode::F5),
        XK_F6 => Some(VirtualKeyCode::F6),
        XK_F7 => Some(VirtualKeyCode::F7),
        XK_F8 => Some(VirtualKeyCode::F8),
        XK_F9 => Some(VirtualKeyCode::F9),
        XK_F10 => Some(VirtualKeyCode::F10),
        XK_F11 => Some(VirtualKeyCode::F11),
        XK_F12 => Some(VirtualKeyCode::F12),
        XK_Shift_L => Some(VirtualKeyCode::LShift),
        XK_Shift_R => Some(VirtualKeyCode::RShift),
        XK_Control_L => Some(VirtualKeyCode::LControl),
        XK_Control_R => Some(VirtualKeyCode::RControl),
        XK_Alt_L => Some(VirtualKeyCode::LAlt),
        XK_Alt_R => Some(VirtualKeyCode::RAlt),
        XK_Super_L => Some(VirtualKeyCode::LWin),
        XK_Super_R => Some(VirtualKeyCode::RWin),
        _ => None,
    }
}
