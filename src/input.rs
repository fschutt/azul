use webrender::api::{HitTestResult, PipelineId, DocumentId, HitTestFlags, RenderApi, WorldPoint};

pub fn hit_test_ui(api: &RenderApi, document_id: DocumentId, pipeline_id: Option<PipelineId>, point: WorldPoint) -> HitTestResult {
    api.hit_test(document_id, pipeline_id, point, HitTestFlags::FIND_ALL)
}

use std::time::{Instant, Duration};
use glium::glutin::{MouseCursor, VirtualKeyCode};

/// Determines which keys are pressed currently (modifiers, etc.)
#[derive(Debug, Clone)]
pub struct KeyboardState
{
    /// Modifier keys that are currently actively pressed during this cycle
    pub modifiers: Vec<VirtualKeyCode>,
    /// Hidden keys, such as the "n" in CTRL + n. Always lowercase
    pub hidden_keys: Vec<char>,
    /// Actual keys pressed during this cycle (i.e. regular text input)
    pub keys: Vec<char>,
}

impl KeyboardState
{
    pub fn new() -> Self
    {
        Self {
            modifiers: Vec::new(),
            hidden_keys: Vec::new(),
            keys: Vec::new(),
        }
    }
}

/// Mouse position on the screen
#[derive(Debug, Copy, Clone)]
pub struct MouseState
{
    /// Current mouse cursor type
    pub mouse_cursor_type: MouseCursor,
    //// Where the mouse cursor is. None if the window is not focused
    pub mouse_cursor: Option<(i32, i32)>,
    //// Is the left MB down?
    pub left_down: bool,
    //// Is the right MB down?
    pub right_down: bool,
    //// Is the middle MB down?
    pub middle_down: bool,
    /// How far has the mouse scrolled in x direction?
    pub mouse_scroll_x: f32,
    /// How far has the mouse scrolled in y direction?
    pub mouse_scroll_y: f32,
}

impl MouseState
{
    /// Creates a new mouse state
    /// Input: How fast the scroll (mouse) should be converted into pixels
    /// Usually around 10.0 (10 pixels per mouse wheel line)
    pub fn new() -> Self
    {
        MouseState {
            mouse_cursor_type: MouseCursor::Default,
            mouse_cursor: Some((0, 0)),
            left_down: false,
            right_down: false,
            middle_down: false,
            mouse_scroll_x: 0.0,
            mouse_scroll_y: 0.0,
        }
    }
}

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Clone)]
pub struct WindowState
{
    /// The state of the keyboard
    pub(crate) keyboard_state: KeyboardState,
    /// The state of the mouse
    pub(crate) mouse_state: MouseState,
    /// Width of the window
    pub width: u32,
    /// Height of the window
    pub height: u32,
    /// Time of the last rendering update, set after the `redraw()` method
    pub time_of_last_update: Instant,
    /// Minimum frame time
    pub min_frame_time: Duration,
}

impl WindowState
{
    /// Creates a new window state
    pub fn new(
        width: u32,
        height: u32,
    ) -> Self
    {
        Self {
            keyboard_state: KeyboardState::new(),
            mouse_state: MouseState::new(),
            width,
            height,
            time_of_last_update: Instant::now(),
            min_frame_time: Duration::from_millis(16),
        }
    }
}
