//! Contains methods related to event filtering (i.e. detecting whether a
//! click was a mouseover, mouseout, and so on and calling the correct callbacks)

use glium::glutin::{
    Window, Event, WindowEvent, KeyboardInput, ElementState,
    MouseCursor, VirtualKeyCode, MouseButton, MouseScrollDelta, TouchPhase,
};
use {
    dom::On,
    menu::{ApplicationMenu, ContextMenu},
};

const DEFAULT_TITLE: &str = "Azul App";
const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;

/// Determines which keys are pressed currently (modifiers, etc.)
#[derive(Debug, Default, Clone)]
pub struct KeyboardState
{
    /// Modifier keys that are currently actively pressed during this frame
    pub modifiers: Vec<VirtualKeyCode>,
    /// Hidden keys, such as the "n" in CTRL + n. Always lowercase
    pub hidden_keys: Vec<char>,
    /// Actual keys pressed during this frame (i.e. regular text input)
    pub keys: Vec<char>,
}

/// Mouse position on the screen
#[derive(Debug, Copy, Clone)]
pub struct MouseState
{
    /// Current mouse cursor type
    pub mouse_cursor_type: MouseCursor,
    //// Where is the mouse cursor? Set to `None` if the window is not focused
    pub mouse_cursor: Option<(i32, i32)>,
    //// Is the left MB down?
    pub left_down: bool,
    //// Is the right MB down?
    pub right_down: bool,
    //// Is the middle MB down?
    pub middle_down: bool,
}

impl Default for MouseState {
    /// Creates a new mouse state
    fn default() -> Self {
        Self {
            mouse_cursor_type: MouseCursor::Default,
            mouse_cursor: Some((0, 0)),
            left_down: false,
            right_down: false,
            middle_down: false,
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
    /// The "global" application menu of this window (one window usually only has one menu)
    pub application_menu: Option<ApplicationMenu>,
    /// The current context menu for this window
    pub context_menu: Option<ContextMenu>,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: Option<WindowPosition>,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct WindowPosition {
    /// X position from the left side of the screen
    pub x: u32,
    /// Y position from the top of the screen
    pub y: u32,
}


#[derive(Debug, Copy, Clone)]
pub struct WindowSize {
    /// Width of the window
    pub width: u32,
    /// Height of the window
    pub height: u32,
    /// DPI factor of the window
    pub hidpi_factor: f32,
    /// Minimum dimensions of the window
    pub min_dimensions: Option<(u32, u32)>,
    /// Maximum dimensions of the window
    pub max_dimensions: Option<(u32, u32)>,
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
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
            application_menu: None,
            context_menu: None,
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
    pub(crate) fn determine_callbacks(&mut self, event: &Event) -> Option<Vec<On>> {

        use glium::glutin::Event::WindowEvent;
        use glium::glutin::WindowEvent::*;
        use glium::glutin::{ElementState, MouseButton };
        use glium::glutin::MouseButton::*;

        let event = if let WindowEvent { event, .. } = event { Some(event) } else { None };
        let event = event?;

        // store the current window state so we can set it in this.previous_window_state later on
        let mut previous_state = Box::new(self.clone());
        previous_state.previous_window_state = None;

        let mut events_vec = Vec::<On>::new();

        // TODO: right mouse down / middle mouse down?
        match event {
            MouseInput { state: ElementState::Pressed, button, .. } => {
                match button {
                    Left => {
                        if !self.mouse_state.left_down {
                            events_vec.push(On::MouseDown);
                        }
                        self.mouse_state.left_down = true;
                    },
                    Right => {
                        if !self.mouse_state.right_down {
                            events_vec.push(On::MouseDown);
                        }
                        self.mouse_state.right_down = true;
                    },
                    Middle => {
                        if !self.mouse_state.middle_down {
                            events_vec.push(On::MouseDown);
                        }
                        self.mouse_state.middle_down = true;
                    },
                    _ => { }
                }
            },
            MouseInput { state: ElementState::Released, button, .. } => {
                match button {
                    Left => {
                        if self.mouse_state.left_down {
                            events_vec.push(On::MouseUp);
                        }
                        self.mouse_state.left_down = false;
                    },
                    Right => {
                        if self.mouse_state.right_down {
                            events_vec.push(On::MouseUp);
                        }
                        self.mouse_state.right_down = false;
                    },
                    Middle => {
                        if self.mouse_state.middle_down {
                            events_vec.push(On::MouseUp);
                        }
                        self.mouse_state.middle_down = false;
                    },
                    _ => { }
                }
            },
            _ => {
                // TODO
            }
        }

        self.previous_window_state = Some(previous_state);

        if events_vec.is_empty() {
            None
        } else {
            Some(events_vec)
        }
    }
}

fn update_mouse_cursor(window: &Window, old: &MouseCursor, new: &MouseCursor) {
    if *old != *new {
        window.set_cursor(*new);
    }
}

fn virtual_key_code_to_char(code: VirtualKeyCode) -> Option<char> {
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
        O => Some('a'),
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