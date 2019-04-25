use std::{
    collections::{BTreeMap, HashSet},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
use gleam::gl::Gl;
use {
    callbacks::{DefaultCallbackId, DefaultCallback},
    stack_checked_pointer::StackCheckedPointer,
};

pub const DEFAULT_TITLE: &str = "Azul App";
pub const DEFAULT_WIDTH: f32 = 800.0;
pub const DEFAULT_HEIGHT: f32 = 600.0;

static LAST_WINDOW_ID: AtomicUsize = AtomicUsize::new(0);

/// Each default callback is identified by its ID (not by it's function pointer),
/// since multiple IDs could point to the same function.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct WindowId { id: usize }

impl WindowId {
    pub fn new() -> Self {
        WindowId { id: LAST_WINDOW_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

/// User-modifiable fake window: Actions performed on this "fake" window don't
/// have a direct impact on the actual OS-level window, changes are deferred and
/// syncronized with the OS window at the end of the frame.
pub struct FakeWindow<T> {
    /// The window state for the next frame
    pub state: WindowState,
    /// The user can push default callbacks in this `DefaultCallbackSystem`,
    /// which get called later in the hit-testing logic
    pub default_callbacks: BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub gl_context: Rc<Gl>,
}

impl<T> FakeWindow<T> {

    /// Returns a reference-counted pointer to the OpenGL context
    pub fn get_gl_context(&self) -> Rc<Gl> {
        self.gl_context.clone()
    }

    /// Returns the physical (width, height) in pixel of this window
    pub fn get_physical_size(&self) -> (usize, usize) {
        let hidpi = self.get_hidpi_factor();
        let physical = self.state.size.dimensions.to_physical(hidpi);
        (physical.width as usize, physical.height as usize)
    }

    /// Returns the current HiDPI factor for this window.
    pub fn get_hidpi_factor(&self) -> f32 {
        self.state.size.hidpi_factor
    }

    /// Returns the current keyboard keyboard state. We don't want the library
    /// user to be able to modify this state, only to read it.
    pub fn get_keyboard_state<'a>(&'a self) -> &'a KeyboardState {
        &self.state.keyboard_state
    }

    /// Returns the current windows mouse state. We don't want the library
    /// user to be able to modify this state, only to read it
    pub fn get_mouse_state<'a>(&'a self) -> &'a MouseState {
        &self.state.mouse_state
    }

    /// Adds a default callback to the window. The default callbacks are
    /// cleared after every frame, so two-way data binding widgets have to call this
    /// on every frame they want to insert a default callback.
    ///
    /// Returns an ID by which the callback can be uniquely identified (used for hit-testing)
    #[must_use]
    pub fn add_callback(&mut self, callback_ptr: StackCheckedPointer<T>, callback_fn: DefaultCallback<T>) -> DefaultCallbackId {
        let default_callback_id = DefaultCallbackId::new();
        self.default_callbacks.insert(default_callback_id, (callback_ptr, callback_fn));
        default_callback_id
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MouseCursorType {
    Default,
    Crosshair,
    Hand,
    Arrow,
    Move,
    Text,
    Wait,
    Help,
    Progress,
    NotAllowed,
    ContextMenu,
    Cell,
    VerticalText,
    Alias,
    Copy,
    NoDrop,
    Grab,
    Grabbing,
    AllScroll,
    ZoomIn,
    ZoomOut,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
}

impl Default for MouseCursorType {
    fn default() -> Self {
        MouseCursorType::Default
    }
}

/// Hardware-dependent keyboard scan code.
pub type ScanCode = u32;

/// Determines which keys are pressed currently (modifiers, etc.)
#[derive(Default, Debug, Clone, PartialEq)]
pub struct KeyboardState {
    /// Tracks, if the `Shift` key is currently pressed
    pub shift_down: bool,
    /// Tracks, if the `Ctrl` key is currently pressed
    pub ctrl_down: bool,
    /// Tracks, if the `Alt` key is currently pressed
    pub alt_down: bool,
    /// Tracks, if the `Super / Windows / Command` key is currently pressed
    pub super_down: bool,
    /// Currently pressed key, already converted to a `char`
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

/// Mouse position on the screen
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MouseState {
    /// Current mouse cursor type, set to `None` if the cursor is hidden - READWRITE
    pub mouse_cursor_type: Option<MouseCursorType>,
    /// Where is the mouse cursor currently? Set to `None` if the window is not focused - READWRITE
    pub cursor_pos: CursorPosition,
    /// Is the mouse cursor locked to the current window (important for applications like games) - READWRITE
    pub is_cursor_locked: bool,
    /// Is the left mouse button down (READONLY)?
    pub left_down: bool,
    /// Is the right mouse button down?
    pub right_down: bool,
    /// Is the middle mouse button down?
    pub middle_down: bool,
    /// Scroll amount in pixels in the horizontal direction. Gets reset to 0 after every frame
    pub scroll_x: f32,
    /// Scroll amount in pixels in the vertical direction. Gets reset to 0 after every frame
    pub scroll_y: f32,
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            mouse_cursor_type: Some(MouseCursorType::Default),
            cursor_pos: CursorPosition::default(),
            is_cursor_locked: false,
            left_down: false,
            right_down: false,
            middle_down: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

impl MouseState {
    /// Returns whether any mouse button (left, right or center) is currently held down
    pub fn mouse_down(&self) -> bool {
        self.right_down || self.left_down || self.middle_down
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum CursorPosition {
    OutOfWindow,
    Uninitialized,
    InWindow(LogicalPosition),
}

impl Default for CursorPosition {
    fn default() -> CursorPosition {
        CursorPosition::Uninitialized
    }
}

impl CursorPosition {
    pub fn get_position(&self) -> Option<LogicalPosition> {
        match self {
            CursorPosition::InWindow(logical_pos) => Some(*logical_pos),
            CursorPosition::OutOfWindow | CursorPosition::Uninitialized => None,
        }
    }
}

/// Toggles webrender debug flags (will make stuff appear on
/// the screen that you might not want to - used for debugging purposes)
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Clone, PartialEq)]
pub struct WindowState {
    /// Current title of the window
    pub title: String,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
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
    /// Is the window always on top?
    pub is_always_on_top: bool,
    /// Whether the window is resizable
    pub is_resizable: bool,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Current keyboard state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub keyboard_state: KeyboardState,
    /// Current mouse state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub mouse_state: MouseState,
    /// Sets location of IME candidate box in client area coordinates relative to the top left
    /// Supported on all platforms.
    pub ime_position: Option<LogicalPosition>,
    /// On X11, sets window urgency hint (`XUrgencyHint`). On macOs, requests user attention via
    pub request_user_attention: bool,
    /// Set the windows Wayland theme. Irrelevant on other platforms, set to `None`
    pub wayland_theme: Option<WaylandTheme>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FullScreenMode {
    /// - macOS: If the window is in windowed mode, transitions it slowly to fullscreen mode
    /// - other: Does the same as `FastFullScreen`.
    SlowFullScreen,
    /// Window should immediately go into fullscreen mode (on macOS this is not the default behaviour).
    FastFullScreen,
    /// - macOS: If the window is in fullscreen mode, transitions slowly back to windowed state.
    /// - other: Does the same as `FastWindowed`.
    SlowWindowed,
    /// If the window is in fullscreen mode, will immediately go back to windowed mode (on macOS this is not the default behaviour).
    FastWindowed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WaylandTheme {
    /// Primary color when the window is focused
    pub primary_active: [u8; 4],
    /// Primary color when the window is unfocused
    pub primary_inactive: [u8; 4],
    /// Secondary color when the window is focused
    pub secondary_active: [u8; 4],
    /// Secondary color when the window is unfocused
    pub secondary_inactive: [u8; 4],
    /// Close button color when hovered over
    pub close_button_hovered: [u8; 4],
    /// Close button color
    pub close_button: [u8; 4],
    /// Close button color when hovered over
    pub maximize_button_hovered: [u8; 4],
    /// Maximize button color
    pub maximize_button: [u8; 4],
    /// Minimize button color when hovered over
    pub minimize_button_hovered: [u8; 4],
    /// Minimize button color
    pub minimize_button: [u8; 4],
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct WindowSize {
    /// Width and height of the window, in logical
    /// units (may not correspond to the physical on-screen size)
    pub dimensions: LogicalSize,
    /// DPI factor of the window
    pub hidpi_factor: f32,
    /// (Internal only, unused): winit HiDPI factor
    pub winit_hidpi_factor: f32,
    /// Minimum dimensions of the window
    pub min_dimensions: Option<LogicalSize>,
    /// Maximum dimensions of the window
    pub max_dimensions: Option<LogicalSize>,
}

impl WindowSize {
    pub fn get_inner_logical_size(&self) -> LogicalSize {
        LogicalSize::new(
            self.dimensions.width / self.winit_hidpi_factor * self.hidpi_factor,
            self.dimensions.height / self.winit_hidpi_factor * self.hidpi_factor
        )
    }

    pub fn get_reverse_logical_size(&self) -> LogicalSize {
        LogicalSize::new(
            self.dimensions.width / self.hidpi_factor * self.winit_hidpi_factor,
            self.dimensions.height / self.hidpi_factor * self.winit_hidpi_factor,
        )
    }
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            dimensions: LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
            hidpi_factor: 1.0,
            winit_hidpi_factor: 1.0,
            min_dimensions: None,
            max_dimensions: None,
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.into(),
            size: WindowSize::default(),
            position: None,
            is_maximized: false,
            is_fullscreen: false,
            has_decorations: true,
            is_visible: true,
            is_always_on_top: false,
            is_resizable: true,
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            ime_position: None,
            request_user_attention: false,
            wayland_theme: None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct PhysicalPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct PhysicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalPosition {
    pub fn new(x: f32, y: f32) -> Self { Self { x, y } }

    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalPosition {
        PhysicalPosition {
            x: self.x * hidpi_factor,
            y: self.y * hidpi_factor,
        }
    }
}

impl PhysicalPosition {
    pub fn new(x: f32, y: f32) -> Self { Self { x, y } }

    pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x / hidpi_factor,
            y: self.y / hidpi_factor,
        }
    }
}

impl LogicalSize {
    pub fn new(width: f32, height: f32) -> Self { Self { width, height } }

    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalSize {
        PhysicalSize {
            width: self.width * hidpi_factor,
            height: self.height * hidpi_factor,
        }
    }
}

impl PhysicalSize {
    pub fn new(width: f32, height: f32) -> Self { Self { width, height } }

    pub fn to_logical(self, hidpi_factor: f32) -> LogicalSize {
        LogicalSize {
            width: self.width / hidpi_factor,
            height: self.height / hidpi_factor,
        }
    }
}

/// Utility function for easier creation of a keymap - i.e. `[vec![Ctrl, S], my_function]`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AcceleratorKey {
    Ctrl,
    Alt,
    Shift,
    Key(VirtualKeyCode),
}

impl AcceleratorKey {

    /// Checks if the current keyboard state contains the given char or modifier,
    /// i.e. if the keyboard state currently has the shift key pressed and the
    /// accelerator key is `Shift`, evaluates to true, otherwise to false.
    pub fn matches(&self, keyboard_state: &KeyboardState) -> bool {
        use self::AcceleratorKey::*;
        match self {
            Ctrl => keyboard_state.ctrl_down,
            Alt => keyboard_state.alt_down,
            Shift => keyboard_state.shift_down,
            Key(k) => keyboard_state.current_virtual_keycodes.contains(k),
        }
    }
}

/// Symbolic name for a keyboard key, does NOT take the keyboard locale into account
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VirtualKeyCode {
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Caret,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    AbntC1,
    AbntC2,
    Add,
    Apostrophe,
    Apps,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Decimal,
    Divide,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Multiply,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    OEM102,
    Period,
    PlayPause,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Subtract,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Copy,
    Paste,
    Cut,
}
