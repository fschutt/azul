//! Contains methods related to event filtering (i.e. detecting whether a
//! click was a mouseover, mouseout, and so on and calling the correct callbacks)

use std::{
    collections::{HashSet, BTreeMap},
    path::PathBuf,
};
use glium::glutin::{
    Window, Event, WindowEvent, KeyboardInput, ScanCode, ElementState,
    MouseCursor, VirtualKeyCode, MouseScrollDelta,
    ModifiersState, dpi::{LogicalPosition, LogicalSize},
};
use webrender::api::{HitTestResult, HitTestItem};
use {
    dom::{On, Callback, TabIndex},
    default_callbacks::DefaultCallbackId,
    id_tree::NodeId,
    ui_state::UiState,
    traits::Layout,
};

const DEFAULT_TITLE: &str = "Azul App";
const DEFAULT_WIDTH: f64 = 800.0;
const DEFAULT_HEIGHT: f64 = 600.0;

/// Determines which keys are pressed currently (modifiers, etc.)
#[derive(Default, Debug, Clone)]
pub struct KeyboardState
{
    // Modifier keys that are currently actively pressed during this frame
    //
    // Note: These are tracked separately by glium to prevent missing state changes
    // when the window isn't focused

    /// Shift key
    pub shift_down: bool,
    /// Ctrl key
    pub ctrl_down: bool,
    /// Alt key
    pub alt_down: bool,
    /// `Super / Windows / Command` key
    pub super_down: bool,
    /// Currently pressed key, already converted to characters
    pub current_char: Option<char>,
    /// Holds the key that was pressed last if there is Some. Holds None otherwise.
    pub latest_virtual_keycode: Option<VirtualKeyCode>,
    /// Currently pressed virtual keycodes - this is essentially an "extension"
    /// of `current_keys` - `current_keys` stores the characters, but what if the
    /// pressed key is not a character (such as `ArrowRight` or `PgUp`)?
    ///
    /// Note that this can have an overlap, so pressing "a" on the keyboard will insert
    /// both a `VirtualKeyCode::A` into `current_virtual_keycodes` and an `"a"` as a char into `current_keys`.
    pub current_virtual_keycodes: HashSet<VirtualKeyCode>,
    /// Same as `current_virtual_keycodes`, but the scancode identifies the physical key pressed.
    ///
    /// This should not change if the user adjusts the host's keyboard map.
    /// Use when the physical location of the key is more important than the key's host GUI semantics,
    /// such as for movement controls in a first-person game (German keyboard: Z key, UK keyboard: Y key, etc.)
    pub current_scancodes: HashSet<ScanCode>,
}

impl KeyboardState {

    fn update_from_modifier_state(&mut self, state: ModifiersState) {
        self.shift_down = state.shift;
        self.ctrl_down = state.ctrl;
        self.alt_down = state.alt;
        self.super_down = state.logo;
    }
}

/// Mouse position on the screen
#[derive(Debug, Copy, Clone)]
pub struct MouseState
{
    /// Current mouse cursor type
    pub mouse_cursor_type: MouseCursor,
    //// Where is the mouse cursor currently? Set to `None` if the window is not focused
    pub cursor_pos: Option<LogicalPosition>,
    //// Is the left mouse button down?
    pub left_down: bool,
    //// Is the right mouse button down?
    pub right_down: bool,
    //// Is the middle mouse button down?
    pub middle_down: bool,
    /// Scroll amount in pixels in the horizontal direction. Gets reset to 0 after every frame
    pub scroll_x: f64,
    /// Scroll amount in pixels in the vertical direction. Gets reset to 0 after every frame
    pub scroll_y: f64,
}

impl Default for MouseState {
    /// Creates a new mouse state
    fn default() -> Self {
        Self {
            mouse_cursor_type: MouseCursor::Default,
            cursor_pos: None,
            left_down: false,
            right_down: false,
            middle_down: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

/// Toggles webrender debug flags (will make stuff appear on
/// the screen that you might not want to - used for debugging purposes)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DebugState {
    /// Toggles `webrender::DebugFlags::PROFILER_DBG`
    pub profiler_dbg: bool,
    /// Toggles `webrender::DebugFlags::RENDER_TARGET_DBG`
    pub render_target_dbg: bool,
    /// Toggles `webrender::DebugFlags::TEXTURE_CACHE_DBG`
    pub texture_cache_dbg: bool,
    /// Toggles `webrender::DebugFlags::GPU_TIME_QUERIES`
    pub gpu_time_queries: bool,
    /// Toggles `webrender::DebugFlags::GPU_SAMPLE_QUERIES`
    pub gpu_sample_queries: bool,
    /// Toggles `webrender::DebugFlags::DISABLE_BATCHING`
    pub disable_batching: bool,
    /// Toggles `webrender::DebugFlags::EPOCHS`
    pub epochs: bool,
    /// Toggles `webrender::DebugFlags::COMPACT_PROFILER`
    pub compact_profiler: bool,
    /// Toggles `webrender::DebugFlags::ECHO_DRIVER_MESSAGES`
    pub echo_driver_messages: bool,
    /// Toggles `webrender::DebugFlags::NEW_FRAME_INDICATOR`
    pub new_frame_indicator: bool,
    /// Toggles `webrender::DebugFlags::NEW_SCENE_INDICATOR`
    pub new_scene_indicator: bool,
    /// Toggles `webrender::DebugFlags::SHOW_OVERDRAW`
    pub show_overdraw: bool,
    /// Toggles `webrender::DebugFlags::GPU_CACHE_DBG`
    pub gpu_cache_dbg: bool,
}

impl Default for DebugState {
    fn default() -> Self {
        Self {
            profiler_dbg: false,
            render_target_dbg: false,
            texture_cache_dbg: false,
            gpu_time_queries: false,
            gpu_sample_queries: false,
            disable_batching: false,
            epochs: false,
            compact_profiler: false,
            echo_driver_messages: false,
            new_frame_indicator: false,
            new_scene_indicator: false,
            show_overdraw: false,
            gpu_cache_dbg: false,
        }
    }
}

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Clone)]
pub struct WindowState {
    /// The state of the keyboard for this frame
    pub(crate) keyboard_state: KeyboardState,
    /// The state of the mouse, read-only
    pub(crate) mouse_state: MouseState,
    /// Whether there is a file currently hovering over the window
    pub(crate) hovered_file: Option<PathBuf>,
    /// What node is currently hovered over, default to None. Only necessary internal
    /// to the crate, for emitting `On::FocusReceived` and `On::FocusLost` events,
    /// as well as styling `:focus` elements
    pub(crate) focused_node: Option<NodeId>,
    /// Currently hovered nodes, default to an empty Vec. Important for
    /// styling `:hover` elements.
    pub(crate) hovered_nodes: Vec<(NodeId, HitTestItem)>,
    /// Previous window state, used for determining mouseout, etc. events
    pub(crate) previous_window_state: Option<Box<WindowState>>,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// Current title of the window
    pub title: String,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: Option<LogicalPosition>,
    /// Is the window currently maximized
    pub is_maximized: bool,
    /// Is the window currently fullscreened?
    pub is_fullscreen: bool,
    /// Does the window have decorations (close, minimize, maximize, title bar)?
    pub has_decorations: bool,
    /// Is the window currently visible?
    pub is_visible: bool,
    /// Is the window background transparent?
    pub is_transparent: bool,
    /// Is the window always on top?
    pub is_always_on_top: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct WindowSize {
    /// Width and height of the window, in logical
    /// units (may not correspond to the physical on-screen size)
    pub dimensions: LogicalSize,
    /// DPI factor of the window
    pub hidpi_factor: f64,
    /// Minimum dimensions of the window
    pub min_dimensions: Option<LogicalSize>,
    /// Maximum dimensions of the window
    pub max_dimensions: Option<LogicalSize>,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            dimensions: LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
            hidpi_factor: 1.0,
            min_dimensions: None,
            max_dimensions: None,
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            focused_node: None,
            hovered_nodes: Vec::new(),
            hovered_file: None,
            previous_window_state: None,
            title: DEFAULT_TITLE.into(),
            position: None,
            size: WindowSize::default(),
            is_maximized: false,
            is_fullscreen: false,
            has_decorations: true,
            is_visible: true,
            is_transparent: false,
            is_always_on_top: false,
            debug_state: DebugState::default(),
        }
    }
}

pub(crate) struct DetermineCallbackResult<T: Layout> {
    pub(crate) hit_test_item: HitTestItem,
    pub(crate) default_callbacks: BTreeMap<On, DefaultCallbackId>,
    pub(crate) normal_callbacks: BTreeMap<On, Callback<T>>,
}

pub(crate) struct CallbacksOfHitTest<T: Layout> {
    /// A BTreeMap where each item is already filtered by the proper hit-testing type,
    /// meaning in order to get the proper callbacks, you simply have to iterate through
    /// all node IDs
    pub nodes_with_callbacks: BTreeMap<NodeId, DetermineCallbackResult<T>>,
    /// Whether the screen should be redrawn even if no Callback returns an `UpdateScreen::Redraw`.
    /// This is necessary for `:hover` and `:active` mouseovers - otherwise the screen would
    /// only update on the next resize.
    pub needs_redraw_anyways: bool,
    /// Same as `needs_redraw_anyways`, but for reusing the layout from the previous frame.
    /// Each `:hover` and `:active` group stores whether it modifies the layout, as
    /// a performance optimization.
    pub needs_relayout_anyways: bool,
}

impl<T: Layout> Default for CallbacksOfHitTest<T> {
    fn default() -> Self {
        Self {
            nodes_with_callbacks: BTreeMap::new(),
            needs_redraw_anyways: false,
            needs_relayout_anyways: false,
        }
    }
}

impl WindowState
{
    pub fn get_mouse_state(&self) -> &MouseState {
        &self.mouse_state
    }

    pub fn get_keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub fn get_hovered_file(&self) -> Option<&PathBuf> {
        self.hovered_file.as_ref()
    }

    /// Determine which event / which callback(s) should be called and in which order
    ///
    /// This function also updates / mutates the current window state, so that
    /// the window state is updated for the next frame
    pub(crate) fn determine_callbacks<T: Layout>(&mut self, hit_test_result: &HitTestResult, event: &Event, ui_state: &UiState<T>)
    -> CallbacksOfHitTest<T>
    {
        use std::collections::HashSet;
        use glium::glutin::{
            Event, WindowEvent, KeyboardInput,
            MouseButton::*,
        };

        let event = if let Event::WindowEvent { event, .. } = event {
            event
        } else {
            return CallbacksOfHitTest::default();
        };

        // store the current window state so we can set it in this.previous_window_state later on
        let mut previous_state = Box::new(self.clone());
        previous_state.previous_window_state = None;

        let mut events_vec = HashSet::<On>::new();
        events_vec.insert(On::MouseOver);

        match event {
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
                events_vec.insert(On::MouseDown);
                match button {
                    Left => {
                        if !self.mouse_state.left_down {
                            events_vec.insert(On::LeftMouseDown);
                        }
                        self.mouse_state.left_down = true;
                    },
                    Right => {
                        if !self.mouse_state.right_down {
                            events_vec.insert(On::RightMouseDown);
                        }
                        self.mouse_state.right_down = true;
                    },
                    Middle => {
                        if !self.mouse_state.middle_down {
                            events_vec.insert(On::MiddleMouseDown);
                        }
                        self.mouse_state.middle_down = true;
                    },
                    _ => { }
                }
            },
            WindowEvent::MouseInput { state: ElementState::Released, button, .. } => {
                match button {
                    Left => {
                        if self.mouse_state.left_down {
                            events_vec.insert(On::MouseUp);
                            events_vec.insert(On::LeftMouseUp);
                        }
                        self.mouse_state.left_down = false;
                    },
                    Right => {
                        if self.mouse_state.right_down {
                            events_vec.insert(On::MouseUp);
                            events_vec.insert(On::RightMouseUp);
                        }
                        self.mouse_state.right_down = false;
                    },
                    Middle => {
                        if self.mouse_state.middle_down {
                            events_vec.insert(On::MouseUp);
                            events_vec.insert(On::MiddleMouseUp);
                        }
                        self.mouse_state.middle_down = false;
                    },
                    _ => { }
                }
            },
            WindowEvent::MouseWheel { .. } => {
                events_vec.insert(On::Scroll);
            },
            WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, virtual_keycode: Some(_), .. }, .. } => {
                events_vec.insert(On::VirtualKeyDown);
            },
            WindowEvent::ReceivedCharacter(c) => {
                if !c.is_control() {
                    events_vec.insert(On::TextInput);
                }
            },
            WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Released, virtual_keycode: Some(_), .. }, .. } => {
                events_vec.insert(On::VirtualKeyUp);
            },
            WindowEvent::HoveredFile(_) => {
                events_vec.insert(On::HoveredFile);
            },
            WindowEvent::DroppedFile(_) => {
                events_vec.insert(On::DroppedFile);
            },
            WindowEvent::HoveredFileCancelled => {
                events_vec.insert(On::HoveredFileCancelled);
            },
            _ => { }
        }

        let event_was_mouse_down = if let WindowEvent::MouseInput { state: ElementState::Pressed, .. } = event { true } else { false };
        let event_was_mouse_release = if let WindowEvent::MouseInput { state: ElementState::Released, .. } = event { true } else { false };

        // TODO: If the current mouse is down, but the event
        // wasn't a click, that means it was a drag

        // Figure out if an item has received the onfocus or onfocusleave event
        let closest_item_with_focus_tab: Option<(NodeId, TabIndex)> = if event_was_mouse_down || event_was_mouse_release {
            // Find the first (closest to cursor in hierarchy) item that has a tabindex
            hit_test_result.items.iter().rev().find_map(|item| ui_state.tab_index_tags.get(&item.tag.0)).cloned()
        } else {
            None
        };

        if let Some((new_focused_element_node_id, _)) = closest_item_with_focus_tab {
            // Update the current window states focus element, regardless of
            // whether an On::FocusReceived or a On::FocusLost
            self.focused_node = Some(new_focused_element_node_id);
            if previous_state.focused_node != Some(new_focused_element_node_id) {
                if previous_state.focused_node.is_none() {
                    events_vec.insert(On::FocusReceived);
                } else {
                    events_vec.insert(On::FocusLost);
                }
                // else, if the last element = current element,
                // then the focus is still on the same field
            }
        } else if event_was_mouse_release || event_was_mouse_down {
            self.focused_node = None;
            events_vec.insert(On::FocusLost);
        }

        // Update all hovered nodes for creating new :hover tags
        self.hovered_nodes = hit_test_result.items.iter().filter_map(|hit_test_item| {
            ui_state.tag_ids_to_node_ids
            .get(&hit_test_item.tag.0)
            .map(|node_id| (*node_id, hit_test_item.clone()))
        }).collect();

        fn hit_test_item_to_callback_result<T: Layout>(
            item: &HitTestItem,
            ui_state: &UiState<T>,
            events_vec: &HashSet<On>)
         -> Option<(NodeId, DetermineCallbackResult<T>)>
         {
            let item_node_id = ui_state.tag_ids_to_node_ids.get(&item.tag.0)?;
            let default_callbacks = ui_state.tag_ids_to_default_callbacks
                .get(&item.tag.0)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|(on, _)| events_vec.contains(&on))
                .collect();

            let normal_callbacks = ui_state.tag_ids_to_callbacks
                .get(&item.tag.0)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|(on, _)| events_vec.contains(&on))
                .collect();

            let hit_test_item = item.clone();
            Some((*item_node_id, DetermineCallbackResult { default_callbacks, normal_callbacks, hit_test_item }))
        };

        let mut nodes_with_callbacks = hit_test_result.items
            .iter()
            .filter_map(|item| hit_test_item_to_callback_result(item, ui_state, &events_vec))
            .collect::<BTreeMap<_, _>>();

        let mut needs_hover_redraw = false;
        let mut needs_hover_relayout = false;

        // Insert all On::MouseEnter events
        for (mouse_enter_node_id, hit_test_item) in self.hovered_nodes.iter()
            .cloned()
            .filter(|current| previous_state.hovered_nodes.iter().find(|x| x.0 == current.0).is_none())
        {
            let tag_for_this_node = ui_state.node_ids_to_tag_ids.get(&mouse_enter_node_id).unwrap();

            let default_callbacks = ui_state.tag_ids_to_default_callbacks
                .get(&tag_for_this_node)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|(on, _)| *on == On::MouseEnter)
                .collect();

            let normal_callbacks = ui_state.tag_ids_to_callbacks
                .get(&tag_for_this_node)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|(on, _)| *on == On::MouseEnter)
                .collect();

            let hit_test_item = hit_test_item.clone();
            let callback_result = DetermineCallbackResult { default_callbacks, normal_callbacks, hit_test_item };
            nodes_with_callbacks.insert(mouse_enter_node_id, callback_result);

            if let Some((_, hover_group)) = ui_state.tag_ids_to_hover_active_states.get(&tag_for_this_node) {
                // We definitely need to redraw (on any :hover) change
                needs_hover_redraw = true;
                // Only set this to true if the :hover group actually affects the layout
                if hover_group.affects_layout {
                    needs_hover_relayout = true;
                }
            }
        }
/*

        // Insert all On::MouseLeave events
        for mouse_leave_node_id in previous_state.hovered_nodes.iter().filter(|prev| self.hovered_nodes.iter().find(|x| x == prev).is_none()).map(|x| *x) {
            nodes_with_callbacks.entry(mouse_leave_node_id)
            .or_insert_with(||
                DetermineCallbackResult {
                    hit_test_item: HitTestItem,
                    default_callbacks: BTreeMap<On, DefaultCallbackId>,
                    normal_callbacks: BTreeMap<On, Callback<T>>,
                }
            )
        }
*/

        self.previous_window_state = Some(previous_state);

        CallbacksOfHitTest {
            needs_redraw_anyways: needs_hover_redraw,
            needs_relayout_anyways: needs_hover_relayout,
            nodes_with_callbacks,
        }
/*
DetermineCallbackResult<T: Layout> {
    pub(crate) hit_test_item: HitTestItem,
    pub(crate) default_callbacks: BTreeMap<On, DefaultCallbackId>,
    pub(crate) normal_callbacks: BTreeMap<On, Callback<T>>,
}
*/
    }

    pub(crate) fn update_keyboard_modifiers(&mut self, event: &Event) {
        let modifiers = match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::KeyboardInput { input: KeyboardInput { modifiers, .. }, .. } |
                    WindowEvent::CursorMoved { modifiers, .. } |
                    WindowEvent::MouseWheel { modifiers, .. } |
                    WindowEvent::MouseInput { modifiers, .. } => {
                        Some(modifiers)
                    },
                    _ => None,
                }
            },
            _ => None,
        };

        if let Some(modifiers) = modifiers {
            self.keyboard_state.update_from_modifier_state(*modifiers);
        }
    }

    /// After the initial events are filtered, this will update the mouse
    /// cursor position, if the event is a `CursorMoved` and set it to `None`
    /// if the cursor has left the window
    pub(crate) fn update_mouse_cursor_position(&mut self, event: &Event) {
        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        self.mouse_state.cursor_pos = Some(*position);
                    },
                    WindowEvent::CursorLeft { .. } => {
                        self.mouse_state.cursor_pos = None;
                    },
                    WindowEvent::CursorEntered { .. } => {
                        self.mouse_state.cursor_pos = Some(LogicalPosition::new(0.0, 0.0))
                    },
                    _ => { }
                }
            },
            _ => { },
        }
    }

    pub(crate) fn update_scroll_state(&mut self, event: &Event) {
        match event {
            Event::WindowEvent { event: WindowEvent::MouseWheel { delta, .. }, .. } => {
                const LINE_DELTA: f64 = 38.0;

                let (scroll_x_px, scroll_y_px) = match delta {
                    MouseScrollDelta::PixelDelta(LogicalPosition { x, y }) => (*x, *y),
                    MouseScrollDelta::LineDelta(x, y) => (*x as f64 * LINE_DELTA, *y as f64 * LINE_DELTA),
                };
                self.mouse_state.scroll_x = -scroll_x_px;
                self.mouse_state.scroll_y = -scroll_y_px; // TODO: "natural scrolling"?
            },
            _ => { },
        }
    }

    /// Updates self.keyboard_state to reflect what characters are currently held down
    pub(crate) fn update_keyboard_pressed_chars(&mut self, event: &Event) {
        use glium::glutin::KeyboardInput;

        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, virtual_keycode, scancode, .. }, .. } => {
                        if let Some(vk) = virtual_keycode {
                            self.keyboard_state.current_virtual_keycodes.insert(*vk);
                            self.keyboard_state.latest_virtual_keycode = Some(*vk);
                        }
                        self.keyboard_state.current_scancodes.insert(*scancode);
                    },
                    // The char event is sliced inbetween a keydown and a keyup event
                    // so the keyup has to clear the character again
                    WindowEvent::ReceivedCharacter(c) => {
                        self.keyboard_state.current_char = Some(*c);
                    },
                    WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Released, virtual_keycode, scancode, .. }, .. } => {
                        if let Some(vk) = virtual_keycode {
                            self.keyboard_state.current_virtual_keycodes.remove(vk);
                            self.keyboard_state.latest_virtual_keycode = None;
                        }
                        self.keyboard_state.current_scancodes.remove(scancode);
                    },
                    WindowEvent::Focused(false) => {
                        self.keyboard_state.current_char = None;
                        self.keyboard_state.current_virtual_keycodes.clear();
                        self.keyboard_state.latest_virtual_keycode = None;
                        self.keyboard_state.current_scancodes.clear();
                    },
                    _ => { },
                }
            },
            _ => { }
        }

    }

    pub(crate) fn update_misc_events(&mut self, event: &Event) {
        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::HoveredFile(path) => {
                        self.hovered_file = Some(path.clone());
                    },
                    WindowEvent::DroppedFile(path) => {
                        self.hovered_file = Some(path.clone());
                    },
                    WindowEvent::HoveredFileCancelled => {
                        self.hovered_file = None;
                    },
                    _ => { },
                }
            },
            _ => { },
        }
    }
}

fn update_mouse_cursor(window: &Window, old: &MouseCursor, new: &MouseCursor) {
    if *old != *new {
        window.set_cursor(*new);
    }
}
