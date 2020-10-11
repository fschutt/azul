use std::{
    fmt,
    collections::{BTreeMap, HashSet},
    sync::atomic::{AtomicUsize, Ordering},
    marker::PhantomData,
    path::PathBuf,
};
#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(not(test))]
#[cfg(debug_assertions)]
use std::time::Duration;
use azul_css::{Css, CssPath};
#[cfg(debug_assertions)]
#[cfg(not(test))]
use azul_css::HotReloadHandler;
use crate::{
    dom::{DomId, EventFilter},
    id_tree::NodeId,
    callbacks::{Callback, DefaultCallback, HitTestItem, UpdateScreen, Redraw},
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

static LAST_ICON_KEY: AtomicUsize = AtomicUsize::new(0);

/// Key that is used for checking whether a window icon has changed -
/// this way azul doesn't need to diff the actual bytes, just the icon key.
/// Use `IconKey::new()` to generate a new, unique key
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct IconKey { id: usize }

impl IconKey {
    pub fn new() -> Self {
        Self { id: LAST_ICON_KEY.fetch_add(1, Ordering::SeqCst) }
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
    /// Tracks, if the `Shift` key is currently pressed - (READONLY)
    pub shift_down: bool,
    /// Tracks, if the `Ctrl` key is currently pressed - (READONLY)
    pub ctrl_down: bool,
    /// Tracks, if the `Alt` key is currently pressed - (READONLY)
    pub alt_down: bool,
    /// Tracks, if the `Super / Windows / Command` key is currently pressed - (READONLY)
    pub super_down: bool,
    /// Currently pressed key, already converted to a `char` - (READONLY)
    pub current_char: Option<char>,
    /// Same as `current_char`, but .
    ///
    /// **DO NOT USE THIS FOR TEXT INPUT, USE `current_char` and `On::TextInput` instead.**
    /// For example entering `à` will fire a `VirtualKeyCode::Grave`, then `VirtualKeyCode::A`,
    /// so to correctly combine characters, use the `current_char` field.
    pub current_virtual_keycode: Option<VirtualKeyCode>,
    /// Currently pressed virtual keycodes (READONLY) - it can happen that more t
    ///
    /// This is essentially an "extension" of `current_scancodes` - `current_keys` stores the characters, but what if the
    /// pressed key is not a character (such as `ArrowRight` or `PgUp`)?
    ///
    /// Note that this can have an overlap, so pressing "a" on the keyboard will insert
    /// both a `VirtualKeyCode::A` into `current_virtual_keycodes` and an `"a"` as a char into `current_keys`.
    pub pressed_virtual_keycodes: HashSet<VirtualKeyCode>,
    /// Same as `current_virtual_keycodes`, but the scancode identifies the physical key pressed,
    /// independent of the keyboard layout. The scancode does not change if the user adjusts the host's keyboard map.
    /// Use when the physical location of the key is more important than the key's host GUI semantics,
    /// such as for movement controls in a first-person game (German keyboard: Z key, UK keyboard: Y key, etc.)
    pub pressed_scancodes: HashSet<ScanCode>,
}

/// Mouse position, cursor type, user scroll input, etc.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MouseState {
    /// Current mouse cursor type, set to `None` if the cursor is hidden. (READWRITE)
    pub mouse_cursor_type: Option<MouseCursorType>,
    /// Where is the mouse cursor currently? Set to `None` if the window is not focused. (READWRITE)
    pub cursor_position: CursorPosition,
    /// Is the mouse cursor locked to the current window (important for applications like games)? (READWRITE)
    pub is_cursor_locked: bool,
    /// Is the left mouse button down? (READONLY)
    pub left_down: bool,
    /// Is the right mouse button down? (READONLY)
    pub right_down: bool,
    /// Is the middle mouse button down? (READONLY)
    pub middle_down: bool,
    /// Scroll amount in pixels in the horizontal direction. Gets reset to 0 after every frame (READONLY)
    pub scroll_x: Option<f32>,
    /// Scroll amount in pixels in the vertical direction. Gets reset to 0 after every frame (READONLY)
    pub scroll_y: Option<f32>,
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            mouse_cursor_type: Some(MouseCursorType::Default),
            cursor_position: CursorPosition::default(),
            is_cursor_locked: false,
            left_down: false,
            right_down: false,
            middle_down: false,
            scroll_x: None,
            scroll_y: None,
        }
    }
}

impl MouseState {

    /// Returns whether any mouse button (left, right or center) is currently held down
    pub fn mouse_down(&self) -> bool {
        self.right_down || self.left_down || self.middle_down
    }

    pub fn get_scroll_x(&self) -> f32 {
        self.scroll_x.unwrap_or(0.0)
    }

    pub fn get_scroll_y(&self) -> f32 {
        self.scroll_y.unwrap_or(0.0)
    }

    pub fn get_scroll(&self) -> (f32, f32) {
        (self.get_scroll_x(), self.get_scroll_y())
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
    /// Toggles `webrender::DebugFlagsFGPU_CACHE_DBG`
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
    /// Flags such as whether the window is minimized / maximized, fullscreen, etc.
    pub flags: WindowFlags,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Current keyboard state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub keyboard_state: KeyboardState,
    /// Current mouse state
    pub mouse_state: MouseState,
    /// Sets location of IME candidate box in client area coordinates
    /// relative to the top left of the window.
    pub ime_position: Option<LogicalPosition>,
    /// Window options that can only be set on a certain platform
    /// (`WindowsWindowOptions` / `LinuxWindowOptions` / `MacWindowOptions`).
    pub platform_specific_options: PlatformSpecificOptions,
    /// The style of this window
    pub css: Css,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullWindowState {
    /// Current title of the window
    pub title: String,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: Option<LogicalPosition>,
    /// Flags such as whether the window is minimized / maximized, fullscreen, etc.
    pub flags: WindowFlags,
    /// Mostly used for debugging, shows WebRender-builtin graphs on the screen.
    /// Used for performance monitoring and displaying frame times (rendering-only).
    pub debug_state: DebugState,
    /// Current keyboard state - NOTE: mutating this field (currently) does nothing
    /// (doesn't get synchronized with OS-level window)!
    pub keyboard_state: KeyboardState,
    /// Current mouse state
    pub mouse_state: MouseState,
    /// Sets location of IME candidate box in client area coordinates
    /// relative to the top left of the window.
    pub ime_position: Option<LogicalPosition>,
    /// Window options that can only be set on a certain platform
    /// (`WindowsWindowOptions` / `LinuxWindowOptions` / `MacWindowOptions`).
    pub platform_specific_options: PlatformSpecificOptions,
    /// The style of this window
    pub css: Css,

    // --

    /// Previous window state, used for determining mouseout, etc. events
    pub previous_window_state: Option<Box<FullWindowState>>,
    /// Whether there is a file currently hovering over the window
    pub hovered_file: Option<PathBuf>,
    /// Whether there was a file currently dropped on the window
    pub dropped_file: Option<PathBuf>,
    /// What node is currently hovered over, default to None. Only necessary internal
    /// to the crate, for emitting `On::FocusReceived` and `On::FocusLost` events,
    /// as well as styling `:focus` elements
    pub focused_node: Option<(DomId, NodeId)>,
    /// Currently hovered nodes, default to an empty Vec. Important for
    /// styling `:hover` elements.
    pub hovered_nodes: BTreeMap<DomId, BTreeMap<NodeId, HitTestItem>>,
}

impl Default for FullWindowState {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.into(),
            size: WindowSize::default(),
            position: None,
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            ime_position: None,
            platform_specific_options: PlatformSpecificOptions::default(),
            css: Css::default(),

            // --

            previous_window_state: None,
            hovered_file: None,
            dropped_file: None,
            focused_node: None,
            hovered_nodes: BTreeMap::default(),
        }
    }
}

impl FullWindowState {

    pub fn get_mouse_state(&self) -> &MouseState {
        &self.mouse_state
    }

    pub fn get_keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub fn get_hovered_file(&self) -> Option<&PathBuf> {
        self.hovered_file.as_ref()
    }

    pub fn get_dropped_file(&self) -> Option<&PathBuf> {
        self.dropped_file.as_ref()
    }

    /// Returns the window state of the previous frame, useful for calculating
    /// metrics for dragging motions. Note that you can't call this function
    /// recursively - calling `get_previous_window_state()` on the returned
    /// `WindowState` will yield a `None` value.
    pub fn get_previous_window_state(&self) -> Option<&Box<FullWindowState>> {
        self.previous_window_state.as_ref()
    }
}

impl From<WindowState> for FullWindowState {
    /// Creates a FullWindowState from a regular WindowState, fills non-available
    /// fields with their default values
    fn from(window_state: WindowState) -> FullWindowState {
        FullWindowState {
            title: window_state.title,
            size: window_state.size,
            position: window_state.position,
            flags: window_state.flags,
            debug_state: window_state.debug_state,
            keyboard_state: window_state.keyboard_state,
            mouse_state: window_state.mouse_state,
            ime_position: window_state.ime_position,
            platform_specific_options: window_state.platform_specific_options,
            css: window_state.css,
            .. Default::default()
        }
    }
}

impl From<FullWindowState> for WindowState {
    fn from(full_window_state: FullWindowState) -> WindowState {
        WindowState {
            title: full_window_state.title,
            size: full_window_state.size,
            position: full_window_state.position,
            flags: full_window_state.flags,
            debug_state: full_window_state.debug_state,
            keyboard_state: full_window_state.keyboard_state,
            mouse_state: full_window_state.mouse_state,
            ime_position: full_window_state.ime_position,
            platform_specific_options: full_window_state.platform_specific_options,
            css: full_window_state.css,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CallCallbacksResult {
    pub needs_restyle_hover_active: bool,
    pub needs_relayout_hover_active: bool,
    pub needs_restyle_focus_changed: bool,
    pub should_scroll_render: bool,
    pub callbacks_update_screen: UpdateScreen,
}

impl CallCallbacksResult {

    pub fn should_relayout(&self) -> bool {
        self.needs_relayout_hover_active ||
        self.callbacks_update_screen == Redraw
    }

    pub fn should_restyle(&self) -> bool {
        self.should_relayout() ||
        self.needs_restyle_focus_changed ||
        self.needs_restyle_hover_active
    }

    pub fn should_rerender(&self) -> bool {
        self.should_restyle() ||
        self.should_scroll_render
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct WindowFlags {
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
}

impl Default for WindowFlags {
    fn default() -> Self {
        Self {
            is_maximized: false,
            is_fullscreen: false,
            has_decorations: true,
            is_visible: true,
            is_always_on_top: false,
            is_resizable: true,
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub type PlatformSpecificOptions = WasmWindowOptions;
#[cfg(target_os = "windows")]
pub type PlatformSpecificOptions = WindowsWindowOptions;
#[cfg(target_os = "linux")]
pub type PlatformSpecificOptions = LinuxWindowOptions;
#[cfg(target_os = "macos")]
pub type PlatformSpecificOptions = MacWindowOptions;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct WasmWindowOptions {
    // empty for now
}

#[cfg(target_os = "windows")]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct WindowsWindowOptions {
    /// STARTUP ONLY: Sets `WS_EX_NOREDIRECTIONBITMAP`
    pub no_redirection_bitmap: bool,
    /// STARTUP ONLY: Window icon (decoded bytes), appears at the top right corner of the window
    pub window_icon: Option<WindowIcon>,
    /// READWRITE: Taskbar icon (decoded bytes), usually 256x256x4 bytes large (`ICON_BIG`).
    ///
    /// Can be changed in callbacks / at runtime.
    pub taskbar_icon: Option<TaskBarIcon>,
    /// STARTUP ONLY: Pointer (casted to void pointer) to a HWND handle
    pub parent_window: Option<*mut c_void>,
}

#[derive(Debug)]
pub enum StateOperation {
    Remove = 0, // _NET_WM_STATE_REMOVE
    Add = 1,    // _NET_WM_STATE_ADD
    Toggle = 2, // _NET_WM_STATE_TOGGLE
}

impl From<bool> for StateOperation {
    fn from(op: bool) -> Self {
        if op {
            StateOperation::Add
        } else {
            StateOperation::Remove
        }
    }
}

/// X window type. Maps directly to
/// [`_NET_WM_WINDOW_TYPE`](https://specifications.freedesktop.org/wm-spec/wm-spec-1.5.html).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum XWindowType {
    /// A desktop feature. This can include a single window containing desktop icons with the same dimensions as the
    /// screen, allowing the desktop environment to have full control of the desktop, without the need for proxying
    /// root window clicks.
    Desktop,
    /// A dock or panel feature. Typically a Window Manager would keep such windows on top of all other windows.
    Dock,
    /// Toolbar windows. "Torn off" from the main application.
    Toolbar,
    /// Pinnable menu windows. "Torn off" from the main application.
    Menu,
    /// A small persistent utility window, such as a palette or toolbox.
    Utility,
    /// The window is a splash screen displayed as an application is starting up.
    Splash,
    /// This is a dialog window.
    Dialog,
    /// A dropdown menu that usually appears when the user clicks on an item in a menu bar.
    /// This property is typically used on override-redirect windows.
    DropdownMenu,
    /// A popup menu that usually appears when the user right clicks on an object.
    /// This property is typically used on override-redirect windows.
    PopupMenu,
    /// A tooltip window. Usually used to show additional information when hovering over an object with the cursor.
    /// This property is typically used on override-redirect windows.
    Tooltip,
    /// The window is a notification.
    /// This property is typically used on override-redirect windows.
    Notification,
    /// This should be used on the windows that are popped up by combo boxes.
    /// This property is typically used on override-redirect windows.
    Combo,
    /// This indicates the the window is being dragged.
    /// This property is typically used on override-redirect windows.
    Dnd,
    /// This is a normal, top-level window.
    Normal,
}

impl Default for XWindowType {
    fn default() -> Self {
        XWindowType::Normal
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct LinuxWindowOptions {
    /// (Unimplemented) - Can only be set at window creation, can't be changed in callbacks.
    pub x11_visual: Option<*const ()>,
    /// (Unimplemented) - Can only be set at window creation, can't be changed in callbacks.
    pub x11_screen: Option<i32>,
    /// Build window with `WM_CLASS` hint; defaults to the name of the binary. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_wm_classes: Option<Vec<(String, String)>>,
    /// Build window with override-redirect flag; defaults to false. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_override_redirect: bool,
    /// Build window with `_NET_WM_WINDOW_TYPE` hint; defaults to `Normal`. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_window_type: Option<XWindowType>,
    /// Build window with `_GTK_THEME_VARIANT` hint set to the specified value. Currently only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_gtk_theme_variant: Option<String>,
    /// Build window with resize increment hint. Only implemented on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_resize_increments: Option<LogicalSize>,
    /// Build window with base size hint. Only implemented on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_base_size: Option<LogicalSize>,
    /// Build window with a given application ID. It should match the `.desktop` file distributed with
    /// your program. Only relevant on Wayland.
    /// Can only be set at window creation, can't be changed in callbacks.
    ///
    /// For details about application ID conventions, see the
    /// [Desktop Entry Spec](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html#desktop-file-id)
    pub wayland_app_id: Option<String>,
    pub request_user_attention: bool,
    pub wayland_theme: Option<WaylandTheme>,
    pub window_icon: Option<WindowIcon>,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct MacWindowOptions {
    pub request_user_attention: bool,
}

impl WindowState {

    pub fn new(css: Css) -> Self {
        Self {
            css,
            .. Default::default()
        }
    }

    pub fn with_css(self, css: Css) -> Self {
        Self {
            css,
            .. self
        }
    }

    /// Returns the current keyboard keyboard state. We don't want the library
    /// user to be able to modify this state, only to read it.
    pub fn get_mouse_state(&self) -> &MouseState {
        &self.mouse_state
    }

    /// Returns the current windows mouse state. We don't want the library
    /// user to be able to modify this state, only to read it.
    pub fn get_keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    /// Returns the physical (width, height) in pixel of this window
    pub fn get_physical_size(&self) -> (usize, usize) {
        let hidpi = self.get_hidpi_factor();
        let physical = self.size.dimensions.to_physical(hidpi);
        (physical.width as usize, physical.height as usize)
    }

    /// Returns the current HiDPI factor for this window.
    pub fn get_hidpi_factor(&self) -> f32 {
        self.size.hidpi_factor
    }
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

    /// Get the actual logical size
    pub fn get_logical_size(&self) -> LogicalSize {
        LogicalSize::new(self.dimensions.width, self.dimensions.height)
    }

    pub fn get_physical_size(&self) -> PhysicalSize {
        PhysicalSize::new(
            self.dimensions.width * self.hidpi_factor,
            self.dimensions.height * self.hidpi_factor,
        )
    }

    /// Get a size that is usually smaller than the logical one, so that the winit DPI factor is compensated for.
    pub fn get_reverse_logical_size(&self) -> LogicalSize {
        LogicalSize::new(
            self.dimensions.width * self.hidpi_factor / self.winit_hidpi_factor,
            self.dimensions.height * self.hidpi_factor / self.winit_hidpi_factor,
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
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            ime_position: None,
            platform_specific_options: PlatformSpecificOptions::default(),
            css: Css::default(),
        }
    }
}

/// Options on how to initially create the window
pub struct WindowCreateOptions<T> {
    /// State of the window, set the initial title / width / height here.
    pub state: WindowState,
    // /// Which monitor should the window be created on?
    // pub monitor: Monitor,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: RendererType,
    #[cfg(debug_assertions)]
    #[cfg(not(test))]
    /// An optional style hot-reloader for the current window, only available with debug_assertions
    /// enabled
    pub hot_reload_handler: Option<Box<dyn HotReloadHandler>>,
    // Marker, necessary to create a Window<T> out of the create options
    pub marker: PhantomData<T>,
}

impl<T> fmt::Debug for WindowCreateOptions<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[cfg(debug_assertions)]
        #[cfg(not(test))] {
            write!(f, "WindowCreateOptions {{ state: {:?}, renderer_type: {:?}, hot_reload_handler: {:?} }}",
                   self.state, self.renderer_type, self.hot_reload_handler.is_some())
        }
        #[cfg(any(not(debug_assertions), test))] {
            write!(f, "WindowCreateOptions {{ state: {:?}, renderer_type: {:?} }}", self.state, self.renderer_type)
        }
    }
}

impl<T> Default for WindowCreateOptions<T> {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            renderer_type: RendererType::default(),
            #[cfg(debug_assertions)]
            #[cfg(not(test))]
            hot_reload_handler: None,
            marker: PhantomData,
        }
    }
}

impl<T> WindowCreateOptions<T> {

    pub fn new(css: Css) -> Self {
        Self {
            state: WindowState::new(css),
            .. Default::default()
        }
    }

    pub fn with_css(mut self, css: Css) -> Self {
        self.state.css = css;
        self
    }

    #[cfg(not(test))]
    #[cfg(debug_assertions)]
    pub fn new_hot_reload(hot_reload_handler: Box<dyn HotReloadHandler>) -> Self {
        Self {
            hot_reload_handler: Some(hot_reload_handler),
            .. Default::default()
        }
    }
}

#[cfg(not(test))]
#[cfg(debug_assertions)]
pub struct HotReloader(pub Box<dyn HotReloadHandler>);

#[cfg(not(test))]
#[cfg(debug_assertions)]
impl HotReloader {

    pub fn new(hot_reload_handler: Box<dyn HotReloadHandler>) -> Self {
        Self(hot_reload_handler)
    }

    pub fn get_reload_interval(&self) -> Duration {
        self.0.get_reload_interval()
    }

    /// Reloads the CSS (if possible).
    ///
    /// Returns:
    ///
    /// - Ok(css) if the CSS has been successfully reloaded
    /// - Err(why) if the CSS failed to hot-reload.
    pub fn reload_style(&self) -> Result<Css, String> {
        match self.0.reload_style() {
            Ok(mut new_css) => {
                new_css.sort_by_specificity();
                Ok(new_css)
            },
            Err(why) => {
                Err(format!("{}", why))
            },
        }
    }
}

/// Force a specific renderer.
/// By default, Azul will try to use the hardware renderer and fall
/// back to the software renderer if it can't create an OpenGL 3.2 context.
/// However, in some cases a hardware renderer might create problems
/// or you want to force either a software or hardware renderer.
///
/// If the field `renderer_type` on the `WindowCreateOptions` is not
/// `RendererType::Default`, the `create_window` method will try to create
/// a window with the specific renderer type and **crash** if the renderer is
/// not available for whatever reason.
///
/// If you don't know what any of this means, leave it at `Default`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RendererType {
    Default,
    Hardware,
    Software,
}

impl Default for RendererType {
    fn default() -> Self {
        RendererType::Default
    }
}

/// Custom event type, to construct an `EventLoop<AzulWindowUpdateEvent>`.
/// This is dispatched into the `EventLoop` (to send a "custom" event)
pub enum AzulUpdateEvent<T> {
    CreateWindow {
        window_create_options: WindowCreateOptions<T>,
    },
    CloseWindow {
        window_id: WindowId,
    },
    DoHitTest {
        window_id: WindowId,
    },
    RebuildUi {
        window_id: WindowId,
    },
    RestyleUi {
        window_id: WindowId,
        skip_layout: bool,
    },
    RelayoutUi {
        window_id: WindowId,
    },
    RebuildDisplayList {
        window_id: WindowId,
    },
    SendDisplayListToWebRender {
        window_id: WindowId,
    },
    UpdateScrollStates {
        window_id: WindowId,
    },
    UpdateAnimations {
        window_id: WindowId,
    },
    UpdateImages {
        window_id: WindowId,
    },
    // ... etc.
}

impl<T> fmt::Debug for AzulUpdateEvent<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::AzulUpdateEvent::*;
        match self {
            CreateWindow { window_create_options } => write!(f, "CreateWindow {{ window_create_options: {:?} }}", window_create_options),
            CloseWindow { window_id } => write!(f, "CloseWindow {{ window_id: {:?} }}", window_id),
            DoHitTest { window_id } => write!(f, "DoHitTest {{ window_id: {:?} }}", window_id),
            RebuildUi { window_id } => write!(f, "RebuildUi {{ window_id: {:?} }}", window_id),
            RestyleUi { window_id, skip_layout } => write!(f, "RestyleUi {{ window_id: {:?}, skip_layout: {:?} }}", window_id, skip_layout),
            RelayoutUi { window_id } => write!(f, "RelayoutUi {{ window_id: {:?} }}", window_id),
            RebuildDisplayList { window_id } => write!(f, "RebuildDisplayList {{ window_id: {:?} }}", window_id),
            SendDisplayListToWebRender { window_id } => write!(f, "SendDisplayListToWebRender {{ window_id: {:?} }}", window_id),
            UpdateScrollStates { window_id } => write!(f, "UpdateScrollStates {{ window_id: {:?} }}", window_id),
            UpdateAnimations { window_id } => write!(f, "UpdateAnimations {{ window_id: {:?} }}", window_id),
            UpdateImages { window_id } => write!(f, "UpdateImages {{ window_id: {:?} }}", window_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum UpdateFocusWarning {
    FocusInvalidDomId(DomId),
    FocusInvalidNodeId(NodeId),
    CouldNotFindFocusNode(CssPath),
}

impl ::std::fmt::Display for UpdateFocusWarning {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        use self::UpdateFocusWarning::*;
        match self {
            FocusInvalidDomId(dom_id) => write!(f, "Focusing on DOM with invalid ID: {:?}", dom_id),
            FocusInvalidNodeId(node_id) => write!(f, "Focusing on node with invalid ID: {}", node_id),
            CouldNotFindFocusNode(css_path) => write!(f, "Could not find focus node for path: {}", css_path),
        }
    }
}

pub struct DetermineCallbackResult<T> {
    pub hit_test_item: Option<HitTestItem>,
    pub default_callbacks: BTreeMap<EventFilter, DefaultCallback<T>>,
    pub normal_callbacks: BTreeMap<EventFilter, Callback<T>>,
}

impl<T> DetermineCallbackResult<T> {

    pub fn has_default_callbacks(&self) -> bool {
        !self.default_callbacks.is_empty()
    }

    pub fn has_normal_callbacks(&self) -> bool {
        !self.normal_callbacks.is_empty()
    }

    pub fn has_any_callbacks(&self) -> bool {
        self.has_default_callbacks() ||
        self.has_normal_callbacks()
    }
}

impl<T> Default for DetermineCallbackResult<T> {
    fn default() -> Self {
        DetermineCallbackResult {
            hit_test_item: None,
            default_callbacks: BTreeMap::new(),
            normal_callbacks: BTreeMap::new(),
        }
    }
}

impl<T> Clone for DetermineCallbackResult<T> {
    fn clone(&self) -> Self {
        DetermineCallbackResult {
            hit_test_item: self.hit_test_item.clone(),
            default_callbacks: self.default_callbacks.clone(),
            normal_callbacks: self.normal_callbacks.clone(),
        }
    }
}

impl<T> fmt::Debug for DetermineCallbackResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
        "DetermineCallbackResult {{ \
            hit_test_item: {:?},\
            default_callbacks: {:#?},\
            normal_callbacks: {:#?},\
        }}",
            self.hit_test_item,
            self.default_callbacks,
            self.normal_callbacks.keys().collect::<Vec<_>>(),
        )
    }
}

pub struct CallbacksOfHitTest<T> {
    /// A BTreeMap where each item is already filtered by the proper hit-testing type,
    /// meaning in order to get the proper callbacks, you simply have to iterate through
    /// all node IDs
    pub nodes_with_callbacks: BTreeMap<NodeId, DetermineCallbackResult<T>>,
    /// Same as `needs_redraw_anyways`, but for reusing the layout from the previous frame.
    /// Each `:hover` and `:active` group stores whether it modifies the layout, as
    /// a performance optimization.
    pub needs_relayout_anyways: bool,
    /// Whether the screen should be redrawn even if no Callback returns an `UpdateScreen::Redraw`.
    /// This is necessary for `:hover` and `:active` mouseovers - otherwise the screen would
    /// only update on the next resize.
    pub needs_redraw_anyways: bool,
}

impl<T> Default for CallbacksOfHitTest<T> {
    fn default() -> Self {
        Self {
            nodes_with_callbacks: BTreeMap::new(),
            needs_redraw_anyways: false,
            needs_relayout_anyways: false,
        }
    }
}

impl<T> fmt::Debug for CallbacksOfHitTest<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
        "CallbacksOfHitTest {{ \
            nodes_with_callbacks: {:#?},
            needs_relayout_anyways: {:?},
            needs_redraw_anyways: {:?},
        }}",
            self.nodes_with_callbacks,
            self.needs_relayout_anyways,
            self.needs_redraw_anyways,
        )
    }
}

impl<T> CallbacksOfHitTest<T> {
    /// Returns whether there is any
    pub fn should_call_callbacks(&self) -> bool {
        !self.nodes_with_callbacks.is_empty() &&
        self.nodes_with_callbacks.values().any(|n| n.has_any_callbacks())
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
    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self { Self { x, y } }
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalPosition {
        PhysicalPosition {
            x: self.x * hidpi_factor,
            y: self.y * hidpi_factor,
        }
    }
}

impl PhysicalPosition {
    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self { Self { x, y } }
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x / hidpi_factor,
            y: self.y / hidpi_factor,
        }
    }
}

impl LogicalSize {
    #[inline(always)]
    pub const fn new(width: f32, height: f32) -> Self { Self { width, height } }
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalSize {
        PhysicalSize {
            width: self.width * hidpi_factor,
            height: self.height * hidpi_factor,
        }
    }
}

impl PhysicalSize {
    #[inline(always)]
    pub const fn new(width: f32, height: f32) -> Self { Self { width, height } }
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
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
            Key(k) => keyboard_state.pressed_virtual_keycodes.contains(k),
        }
    }
}

/// Symbolic name for a keyboard key, does NOT take the keyboard locale into account
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VirtualKeyCode {
    Asterisk,
    Plus,
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

// Window icon that usually appears in the top-left corner of the window
#[derive(Debug, Clone)]
pub enum WindowIcon {
    /// 16x16x4 bytes icon
    Small { key: IconKey, rgba_bytes: Vec<u8> },
    /// 32x32x4 bytes icon
    Large { key: IconKey, rgba_bytes: Vec<u8> },
}

impl WindowIcon {
    pub fn get_key(&self) -> IconKey {
        match &self {
            WindowIcon::Small { key, .. } => *key,
            WindowIcon::Large { key, .. } => *key,
        }
    }
}
// -- Only compare the IconKey (for WindowIcon and TaskBarIcon)

impl PartialEq for WindowIcon {
    fn eq(&self, rhs: &Self) -> bool {
        self.get_key() == rhs.get_key()
    }
}

impl PartialOrd for WindowIcon {
    fn partial_cmp(&self, rhs: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.get_key()).cmp(&rhs.get_key()))
    }
}

impl Eq for WindowIcon { }

impl Ord for WindowIcon {
    fn cmp(&self, rhs: &Self) -> ::std::cmp::Ordering {
        (self.get_key()).cmp(&rhs.get_key())
    }
}

impl ::std::hash::Hash for WindowIcon {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        self.get_key().hash(state);
    }
}

/// 256x256x4 bytes window icon
#[derive(Debug, Clone)]
pub struct TaskBarIcon {
    pub key: IconKey,
    pub rgba_bytes: Vec<u8>,
}

impl PartialEq for TaskBarIcon {
    fn eq(&self, rhs: &Self) -> bool {
        self.key == rhs.key
    }
}

impl PartialOrd for TaskBarIcon {
    fn partial_cmp(&self, rhs: &Self) -> Option<::std::cmp::Ordering> {
        Some((self.key).cmp(&rhs.key))
    }
}

impl Eq for TaskBarIcon { }

impl Ord for TaskBarIcon {
    fn cmp(&self, rhs: &Self) -> ::std::cmp::Ordering {
        (self.key).cmp(&rhs.key)
    }
}

impl ::std::hash::Hash for TaskBarIcon {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        self.key.hash(state);
    }
}