//! Contains methods related to event filtering (i.e. detecting whether a
//! click was a mouseover, mouseout, and so on and calling the correct callbacks)

use glium::glutin::{
    Window, Event, WindowEvent, KeyboardInput, ScanCode, ElementState,
    MouseCursor, VirtualKeyCode, MouseScrollDelta,
    ModifiersState, dpi::{LogicalPosition, LogicalSize},
};
use std::collections::HashSet;
use {
    dom::On,
};

const DEFAULT_TITLE: &str = "Azul App";
const DEFAULT_WIDTH: f64 = 800.0;
const DEFAULT_HEIGHT: f64 = 600.0;

/// Determines which keys are pressed currently (modifiers, etc.)
#[derive(Debug, Default, Clone)]
pub struct KeyboardState
{
    // Modifier keys that are currently actively pressed during this frame
    //
    // Note: These are tracked seperately by glium to prevent missing state changes
    // when the window isn't focused

    /// Shift key
    pub shift_down: bool,
    /// Ctrl key
    pub ctrl_down: bool,
    /// Alt key
    pub alt_down: bool,
    /// `Super / Windows / Command` key
    pub super_down: bool,
    /// Currently pressed keys, already converted to characters
    pub current_keys: HashSet<char>,
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
            cursor_pos: Some(LogicalPosition::new(0.0, 0.0)),
            left_down: false,
            right_down: false,
            middle_down: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Clone)]
pub struct WindowState
{
    /// Previous window state, used for determining mouseout, etc. events
    pub(crate) previous_window_state: Option<Box<WindowState>>,
    /// Current title of the window
    pub title: String,
    /// The state of the keyboard for this frame
    pub(crate) keyboard_state: KeyboardState,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: Option<LogicalPosition>,
    /// The state of the mouse
    pub(crate) mouse_state: MouseState,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
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
            previous_window_state: None,
            title: DEFAULT_TITLE.into(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            position: None,
            size: WindowSize::default(),
            is_maximized: false,
            is_fullscreen: false,
            has_decorations: true,
            is_visible: true,
            is_transparent: false,
            is_always_on_top: false,
        }
    }
}

impl WindowState
{
    // Determine which event / which callback(s) should be called and in which order
    //
    // This function also updates / mutates the current window state,
    // so that we are ready for the next frame
    pub(crate) fn determine_callbacks(&mut self, event: &Event) -> Vec<On> {

        use std::collections::HashSet;
        use glium::glutin::{
            Event, WindowEvent, KeyboardInput,
            MouseButton::*,
            dpi::LogicalPosition,
        };

        let event = if let Event::WindowEvent { event, .. } = event { event } else { return Vec::new(); };

        // store the current window state so we can set it in this.previous_window_state later on
        let mut previous_state = Box::new(self.clone());
        previous_state.previous_window_state = None;

        let mut events_vec = HashSet::<On>::new();

        match event {
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
                match button {
                    Left => {
                        if !self.mouse_state.left_down {
                            events_vec.insert(On::MouseDown);
                            events_vec.insert(On::LeftMouseDown);
                        }
                        self.mouse_state.left_down = true;
                    },
                    Right => {
                        if !self.mouse_state.right_down {
                            events_vec.insert(On::MouseDown);
                            events_vec.insert(On::RightMouseDown);
                        }
                        self.mouse_state.right_down = true;
                    },
                    Middle => {
                        if !self.mouse_state.middle_down {
                            events_vec.insert(On::MouseDown);
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
            WindowEvent::MouseWheel { delta, .. } => {
                let (scroll_x_px, scroll_y_px) = match delta {
                    MouseScrollDelta::PixelDelta(LogicalPosition { x, y }) => (*x, *y),
                    MouseScrollDelta::LineDelta(x, y) => (*x as f64 * 100.0, *y as f64 * 100.0),
                };
                self.mouse_state.scroll_x = -scroll_x_px;
                self.mouse_state.scroll_y = -scroll_y_px; // TODO: "natural scrolling"?
                events_vec.insert(On::Scroll);
            },
            WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, virtual_keycode: Some(_), .. }, .. } => {
                events_vec.insert(On::KeyDown);
            },
            WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Released, virtual_keycode: Some(_), .. }, .. } => {
                events_vec.insert(On::KeyUp);
            },
            _ => { }
        }

        self.previous_window_state = Some(previous_state);

        events_vec.into_iter().collect()
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

    /// Updates self.keyboard_state to reflect what characters are currently held down
    pub(crate) fn update_keyboard_pressed_chars(&mut self, event: &Event) {
        use glium::glutin::KeyboardInput;

        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, virtual_keycode, scancode, .. }, .. } => {
                        if let Some(vk) = virtual_keycode {
                            if let Some(ch) = virtual_key_code_to_char(*vk) {
                                self.keyboard_state.current_keys.insert(ch);
                            }
                            self.keyboard_state.current_virtual_keycodes.insert(*vk);
                            self.keyboard_state.latest_virtual_keycode = Some(*vk);
                        }
                        self.keyboard_state.current_scancodes.insert(*scancode);
                    },
                    WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Released, virtual_keycode, scancode, .. }, .. } => {
                        if let Some(vk) = virtual_keycode {
                            if let Some(ch) = virtual_key_code_to_char(*vk) {
                                self.keyboard_state.current_keys.remove(&ch);
                            }
                            self.keyboard_state.current_virtual_keycodes.remove(vk);
                            self.keyboard_state.latest_virtual_keycode = None;
                        }
                        self.keyboard_state.current_scancodes.remove(scancode);
                    },
                    WindowEvent::Focused(false) => {
                        self.keyboard_state.current_keys.clear();
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
}

fn update_mouse_cursor(window: &Window, old: &MouseCursor, new: &MouseCursor) {
    if *old != *new {
        window.set_cursor(*new);
    }
}

pub(crate) fn virtual_key_code_to_char(code: VirtualKeyCode) -> Option<char> {
    use glium::glutin::VirtualKeyCode::*;
    match code {
        Key1 => Some('1'),
        Key2 => Some('2'),
        Key3 => Some('3'),
        Key4 => Some('4'),
        Key5 => Some('5'),
        Key6 => Some('6'),
        Key7 => Some('7'),
        Key8 => Some('8'),
        Key9 => Some('9'),
        Key0 => Some('0'),
        A => Some('a'),
        B => Some('b'),
        C => Some('c'),
        D => Some('d'),
        E => Some('e'),
        F => Some('f'),
        G => Some('g'),
        H => Some('h'),
        I => Some('i'),
        J => Some('j'),
        K => Some('k'),
        L => Some('l'),
        M => Some('m'),
        N => Some('n'),
        O => Some('o'),
        P => Some('p'),
        Q => Some('q'),
        R => Some('r'),
        S => Some('s'),
        T => Some('t'),
        U => Some('u'),
        V => Some('v'),
        W => Some('w'),
        X => Some('x'),
        Y => Some('y'),
        Z => Some('z'),
        Return | NumpadEnter => Some('\n'),
        Space => Some(' '),
        Caret => Some('^'),
        Apostrophe => Some('\''),
        Backslash => Some('\\'),
        Colon | Period=> Some('.'),
        Comma | NumpadComma => Some(','),
        Divide | Slash => Some('/'),
        Equals | NumpadEquals => Some('='),
        Grave => Some('´'),
        Minus | Subtract => Some('-'),
        Multiply => Some('*'),
        Semicolon => Some(':'),
        Tab => Some('\t'),
        _ => None
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_window_state_file() {

}