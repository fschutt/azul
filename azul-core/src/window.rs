use std::{
    fmt,
    hash::{Hash, Hasher},
    cmp::Ordering,
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
    path::PathBuf,
    ffi::c_void,
};
use azul_css::{U8Vec, AzString, Css, LayoutPoint, LayoutRect, CssPath};
use crate::{
    FastHashMap,
    app_resources::Epoch,
    dom::{DomId, EventFilter},
    id_tree::NodeId,
    task::AzDuration,
    callbacks::{PipelineId, DocumentId, Callback, ScrollPosition, HitTestItem, UpdateScreen},
    ui_solver::{OverflowingScrollNode, ExternalScrollId, ScrolledNodes},
    ui_state::UiState,
    display_list::{SolvedLayoutCache, GlTextureCache, CachedDisplayList},
    gl::GlContextPtr,
};

pub const DEFAULT_TITLE: &str = "Azul App";
pub const DEFAULT_WIDTH: f32 = 800.0;
pub const DEFAULT_HEIGHT: f32 = 600.0;

static LAST_WINDOW_ID: AtomicUsize = AtomicUsize::new(0);

/// Each default callback is identified by its ID (not by it's function pointer),
/// since multiple IDs could point to the same function.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct WindowId { id: usize }

impl WindowId {
    pub fn new() -> Self {
        WindowId { id: LAST_WINDOW_ID.fetch_add(1, AtomicOrdering::SeqCst) }
    }
}

static LAST_ICON_KEY: AtomicUsize = AtomicUsize::new(0);

/// Key that is used for checking whether a window icon has changed -
/// this way azul doesn't need to diff the actual bytes, just the icon key.
/// Use `IconKey::new()` to generate a new, unique key
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct IconKey { id: usize }

impl IconKey {
    pub fn new() -> Self {
        Self { id: LAST_ICON_KEY.fetch_add(1, AtomicOrdering::SeqCst) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
#[repr(C)]
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
    pub current_char: OptionChar,
    /// Same as `current_char`, but .
    ///
    /// **DO NOT USE THIS FOR TEXT INPUT, USE `current_char` and `On::TextInput` instead.**
    /// For example entering `à` will fire a `VirtualKeyCode::Grave`, then `VirtualKeyCode::A`,
    /// so to correctly combine characters, use the `current_char` field.
    pub current_virtual_keycode: OptionVirtualKeyCode,
    /// Currently pressed virtual keycodes (READONLY) - it can happen that more t
    ///
    /// This is essentially an "extension" of `current_scancodes` - `current_keys` stores the characters, but what if the
    /// pressed key is not a character (such as `ArrowRight` or `PgUp`)?
    ///
    /// Note that this can have an overlap, so pressing "a" on the keyboard will insert
    /// both a `VirtualKeyCode::A` into `current_virtual_keycodes` and an `"a"` as a char into `current_keys`.
    pub pressed_virtual_keycodes: VirtualKeyCodeVec,
    /// Same as `current_virtual_keycodes`, but the scancode identifies the physical key pressed,
    /// independent of the keyboard layout. The scancode does not change if the user adjusts the host's keyboard map.
    /// Use when the physical location of the key is more important than the key's host GUI semantics,
    /// such as for movement controls in a first-person game (German keyboard: Z key, UK keyboard: Y key, etc.)
    pub pressed_scancodes: ScanCodeVec,
}

// char is not ABI-stable, use u32 instead
impl_option!(u32, OptionChar, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(VirtualKeyCode, OptionVirtualKeyCode, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl_vec!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_debug!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_partialord!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_ord!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_clone!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_partialeq!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_eq!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_hash!(VirtualKeyCode, VirtualKeyCodeVec);

impl_vec_as_hashmap!(VirtualKeyCode, VirtualKeyCodeVec);

impl_vec!(ScanCode, ScanCodeVec);
impl_vec_debug!(ScanCode, ScanCodeVec);
impl_vec_partialord!(ScanCode, ScanCodeVec);
impl_vec_ord!(ScanCode, ScanCodeVec);
impl_vec_clone!(ScanCode, ScanCodeVec);
impl_vec_partialeq!(ScanCode, ScanCodeVec);
impl_vec_eq!(ScanCode, ScanCodeVec);
impl_vec_hash!(ScanCode, ScanCodeVec);

impl_vec_as_hashmap!(ScanCode, ScanCodeVec);

/// Mouse position, cursor type, user scroll input, etc.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct MouseState {
    /// Current mouse cursor type, set to `None` if the cursor is hidden. (READWRITE)
    pub mouse_cursor_type: OptionMouseCursorType,
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
    pub scroll_x: OptionF32,
    /// Scroll amount in pixels in the vertical direction. Gets reset to 0 after every frame (READONLY)
    pub scroll_y: OptionF32,
}

impl_option!(i32, OptionI32, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(f32, OptionF32, [Debug, Copy, Clone, PartialEq, PartialOrd]);
impl_option!(MouseCursorType, OptionMouseCursorType, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(AzString, OptionAzString, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl Default for MouseState {
    fn default() -> Self {
        Self {
            mouse_cursor_type: Some(MouseCursorType::Default).into(),
            cursor_position: CursorPosition::default(),
            is_cursor_locked: false,
            left_down: false,
            right_down: false,
            middle_down: false,
            scroll_x: None.into(),
            scroll_y: None.into(),
        }
    }
}

impl MouseState {

    /// Returns whether any mouse button (left, right or center) is currently held down
    pub fn mouse_down(&self) -> bool {
        self.right_down || self.left_down || self.middle_down
    }

    pub fn get_scroll_x(&self) -> f32 {
        self.scroll_x.as_option().copied().unwrap_or(0.0)
    }

    pub fn get_scroll_y(&self) -> f32 {
        self.scroll_y.as_option().copied().unwrap_or(0.0)
    }

    pub fn get_scroll(&self) -> (f32, f32) {
        (self.get_scroll_x(), self.get_scroll_y())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
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
#[repr(C)]
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


#[derive(Debug, Default)]
pub struct ScrollStates(pub FastHashMap<ExternalScrollId, ScrollState>);

impl ScrollStates {

    pub fn new() -> ScrollStates {
        ScrollStates::default()
    }

    #[must_use = "function marks the scroll node as dirty, therefore marked as must_use"]
    pub fn get_scroll_position(&self, scroll_id: &ExternalScrollId) -> Option<LayoutPoint> {
        self.0.get(&scroll_id).map(|entry| entry.get())
    }

    /// Set the scroll amount - does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub fn set_scroll_position(&mut self, node: &OverflowingScrollNode, scroll_position: LayoutPoint) {
        self.0.entry(node.parent_external_scroll_id)
        .or_insert_with(|| ScrollState::default())
        .set(scroll_position.x, scroll_position.y, &node.child_rect);
    }

    /// NOTE: This has to be a getter, because we need to update
    #[must_use = "function marks the scroll ID as dirty, therefore the function is must_use"]
    pub fn get_scroll_position_and_mark_as_used(&mut self, scroll_id: &ExternalScrollId) -> Option<LayoutPoint> {
        let entry = self.0.get_mut(&scroll_id)?;
        Some(entry.get_and_mark_as_used())
    }

    /// Updating (add to) the existing scroll amount does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub fn scroll_node(&mut self, node: &OverflowingScrollNode, scroll_by_x: f32, scroll_by_y: f32) {
        self.0.entry(node.parent_external_scroll_id)
        .or_insert_with(|| ScrollState::default())
        .add(scroll_by_x, scroll_by_y, &node.child_rect);
    }

    /// Removes all scroll states that weren't used in the last frame
    pub fn remove_unused_scroll_states(&mut self) {
        self.0.retain(|_, state| state.used_this_frame);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollState {
    /// Amount in pixel that the current node is scrolled
    pub scroll_position: LayoutPoint,
    /// Was the scroll amount used in this frame?
    pub used_this_frame: bool,
}

impl ScrollState {

    /// Return the current position of the scroll state
    pub fn get(&self) -> LayoutPoint {
        self.scroll_position
    }

    /// Add a scroll X / Y onto the existing scroll state
    pub fn add(&mut self, x: f32, y: f32, child_rect: &LayoutRect) {
        self.scroll_position.x = (self.scroll_position.x + x).max(0.0).min(child_rect.size.width);
        self.scroll_position.y = (self.scroll_position.y + y).max(0.0).min(child_rect.size.height);
    }

    /// Set the scroll state to a new position
    pub fn set(&mut self, x: f32, y: f32, child_rect: &LayoutRect) {
        self.scroll_position.x = x.max(0.0).min(child_rect.size.width);
        self.scroll_position.y = y.max(0.0).min(child_rect.size.height);
    }

    /// Returns the scroll position and also set the "used_this_frame" flag
    pub fn get_and_mark_as_used(&mut self) -> LayoutPoint {
        self.used_this_frame = true;
        self.scroll_position
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState {
            scroll_position: LayoutPoint::zero(),
            used_this_frame: true,
        }
    }
}

/// Overwrites all fields of the `FullWindowState` with the fields of the `WindowState`,
/// but leaves the extra fields such as `.hover_nodes` untouched
pub fn update_full_window_state(
    full_window_state: &mut FullWindowState,
    window_state: &WindowState
) {
    full_window_state.title = window_state.title.clone().into();
    full_window_state.size = window_state.size.into();
    full_window_state.position = window_state.position.into();
    full_window_state.flags = window_state.flags;
    full_window_state.debug_state = window_state.debug_state;
    full_window_state.keyboard_state = window_state.keyboard_state.clone();
    full_window_state.mouse_state = window_state.mouse_state;
    full_window_state.ime_position = window_state.ime_position.into();
    full_window_state.platform_specific_options = window_state.platform_specific_options.clone();
}

/// Resets the mouse states `scroll_x` and `scroll_y` to 0
pub fn clear_scroll_state(window_state: &mut FullWindowState) {
    window_state.mouse_state.scroll_x = OptionF32::None;
    window_state.mouse_state.scroll_y = OptionF32::None;
}

pub struct WindowInternal {
    /// A "document" in WebRender usually corresponds to one tab (i.e. in Azuls case, the whole window).
    pub document_id: DocumentId,
    /// One "document" (tab) can have multiple "pipelines" (important for hit-testing).
    ///
    /// A document can have multiple pipelines, for example in Firefox the tab / navigation bar,
    /// the actual browser window and the inspector are seperate pipelines, but contained in one document.
    /// In Azul, one pipeline = one document (this could be improved later on).
    pub pipeline_id: PipelineId,
    /// The "epoch" is a frame counter, to remove outdated images, fonts and OpenGL textures
    /// when they're not in use anymore.
    pub epoch: Epoch,
    /// Current display list active in this window (useful for debugging)
    pub cached_display_list: CachedDisplayList,
    /// Currently active, layouted rectangles
    pub layout_result: SolvedLayoutCache,
    /// Currently GL textures inside the active CachedDisplayList
    pub gl_texture_cache: GlTextureCache,
    /// Current scroll states of nodes (x and y position of where they are scrolled)
    pub scrolled_nodes: BTreeMap<DomId, ScrolledNodes>,
    /// States of scrolling animations, updated every frame
    pub scroll_states: ScrollStates,
}

impl WindowInternal {

    /// Returns a copy of the current scroll states + scroll positions
    pub fn get_current_scroll_states(&self, ui_states: &BTreeMap<DomId, UiState>)
    -> BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>
    {
        self.scrolled_nodes.iter().filter_map(|(dom_id, scrolled_nodes)| {

            let layout_result = self.layout_result.solved_layouts.get(dom_id)?;
            let ui_state = &ui_states.get(dom_id)?;

            let scroll_positions = scrolled_nodes.overflowing_nodes.iter().filter_map(|(node_id, overflowing_node)| {
                let scroll_location = self.scroll_states.get_scroll_position(&overflowing_node.parent_external_scroll_id)?;
                let parent_node = ui_state.get_dom().arena.node_hierarchy[*node_id].parent.unwrap_or(NodeId::ZERO);
                let scroll_position = ScrollPosition {
                    scroll_frame_rect: overflowing_node.child_rect,
                    parent_rect: layout_result.rects[parent_node].to_layouted_rectangle(),
                    scroll_location,
                };
                Some((*node_id, scroll_position))
            }).collect();

            Some((dom_id.clone(), scroll_positions))
        }).collect()
    }
}

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowState {
    /// Current title of the window
    pub title: AzString,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: OptionPhysicalPositionI32,
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
    pub ime_position: OptionLogicalPosition,
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
    pub position: Option<PhysicalPosition<i32>>,
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
            title: window_state.title.into(),
            size: window_state.size,
            position: window_state.position.into(),
            flags: window_state.flags,
            debug_state: window_state.debug_state,
            keyboard_state: window_state.keyboard_state,
            mouse_state: window_state.mouse_state,
            ime_position: window_state.ime_position.into(),
            platform_specific_options: window_state.platform_specific_options,
            css: window_state.css,
            .. Default::default()
        }
    }
}

impl From<FullWindowState> for WindowState {
    fn from(full_window_state: FullWindowState) -> WindowState {
        WindowState {
            title: full_window_state.title.into(),
            size: full_window_state.size,
            position: full_window_state.position.into(),
            flags: full_window_state.flags,
            debug_state: full_window_state.debug_state,
            keyboard_state: full_window_state.keyboard_state,
            mouse_state: full_window_state.mouse_state,
            ime_position: full_window_state.ime_position.into(),
            platform_specific_options: full_window_state.platform_specific_options,
            css: full_window_state.css,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallCallbacksResult {
    pub needs_restyle_hover_active: bool,
    pub needs_relayout_hover_active: bool,
    pub needs_restyle_focus_changed: bool,
    /// Whether the UI should be rendered anyways due to a (programmatic or user input) scroll event
    pub should_scroll_render: bool,
    /// Whether the callbacks say to rebuild the UI or not
    pub callbacks_update_screen: UpdateScreen,
    /// WindowState that was (potentially) modified in the callbacks
    pub modified_window_state: WindowState,
}

impl CallCallbacksResult {

    pub fn should_relayout(&self) -> bool {
        self.needs_relayout_hover_active ||
        self.callbacks_update_screen == UpdateScreen::Redraw
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
#[repr(C)]
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

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PlatformSpecificOptions {
    pub windows_options: WindowsWindowOptions,
    pub linux_options: LinuxWindowOptions,
    pub mac_options: MacWindowOptions,
    pub wasm_options: WasmWindowOptions,
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct WindowsWindowOptions {
    /// STARTUP ONLY: Sets `WS_EX_NOREDIRECTIONBITMAP`
    pub no_redirection_bitmap: bool,
    /// STARTUP ONLY: Window icon (decoded bytes), appears at the top right corner of the window
    pub window_icon: OptionWindowIcon,
    /// READWRITE: Taskbar icon (decoded bytes), usually 256x256x4 bytes large (`ICON_BIG`).
    ///
    /// Can be changed in callbacks / at runtime.
    pub taskbar_icon: OptionTaskBarIcon,
    /// STARTUP ONLY: Pointer (casted to void pointer) to a HWND handle
    pub parent_window: OptionHwndHandle,
}

type HwndHandle = *mut c_void;

impl_option!(HwndHandle, OptionHwndHandle, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

/// X window type. Maps directly to
/// [`_NET_WM_WINDOW_TYPE`](https://specifications.freedesktop.org/wm-spec/wm-spec-1.5.html).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LinuxWindowOptions {
    /// (Unimplemented) - Can only be set at window creation, can't be changed in callbacks.
    pub x11_visual: OptionX11Visual,
    /// (Unimplemented) - Can only be set at window creation, can't be changed in callbacks.
    pub x11_screen: OptionI32,
    /// Build window with `WM_CLASS` hint; defaults to the name of the binary. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_wm_classes: StringPairVec,
    /// Build window with override-redirect flag; defaults to false. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_override_redirect: bool,
    /// Build window with `_NET_WM_WINDOW_TYPE` hint; defaults to `Normal`. Only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_window_types: XWindowTypeVec,
    /// Build window with `_GTK_THEME_VARIANT` hint set to the specified value. Currently only relevant on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_gtk_theme_variant: OptionAzString,
    /// Build window with resize increment hint. Only implemented on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_resize_increments: OptionLogicalSize,
    /// Build window with base size hint. Only implemented on X11.
    /// Can only be set at window creation, can't be changed in callbacks.
    pub x11_base_size: OptionLogicalSize,
    /// Build window with a given application ID. It should match the `.desktop` file distributed with
    /// your program. Only relevant on Wayland.
    /// Can only be set at window creation, can't be changed in callbacks.
    ///
    /// For details about application ID conventions, see the
    /// [Desktop Entry Spec](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html#desktop-file-id)
    pub wayland_app_id: OptionAzString,
    pub wayland_theme: OptionWaylandTheme,
    pub request_user_attention: bool,
    pub window_icon: OptionWindowIcon,
}

type X11Visual = *const c_void;
impl_option!(X11Visual, OptionX11Visual, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct AzStringPair {
    pub key: AzString,
    pub value: AzString,
}

impl_vec!(AzStringPair, StringPairVec);
impl_vec_debug!(AzStringPair, StringPairVec);
impl_vec_partialord!(AzStringPair, StringPairVec);
impl_vec_ord!(AzStringPair, StringPairVec);
impl_vec_clone!(AzStringPair, StringPairVec);
impl_vec_partialeq!(AzStringPair, StringPairVec);
impl_vec_eq!(AzStringPair, StringPairVec);
impl_vec_hash!(AzStringPair, StringPairVec);

impl_vec!(XWindowType, XWindowTypeVec);
impl_vec_debug!(XWindowType, XWindowTypeVec);
impl_vec_partialord!(XWindowType, XWindowTypeVec);
impl_vec_ord!(XWindowType, XWindowTypeVec);
impl_vec_clone!(XWindowType, XWindowTypeVec);
impl_vec_partialeq!(XWindowType, XWindowTypeVec);
impl_vec_eq!(XWindowType, XWindowTypeVec);
impl_vec_hash!(XWindowType, XWindowTypeVec);

impl_option!(WaylandTheme, OptionWaylandTheme, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct MacWindowOptions {
    pub request_user_attention: bool,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct WasmWindowOptions {
    // empty for now, single field must be present for ABI compat - always set to 0
    pub _reserved: u8,
}

impl WindowState {

    /// Creates a new, default `WindowState` with the given CSS style
    pub fn new(css: Css) -> Self { Self { css, .. Default::default() } }

    /// Same as `WindowState::new` but to be used as a builder method
    pub fn with_css(self, css: Css) -> Self { Self { css, .. self } }

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
        (self.size.dimensions.width as usize, self.size.dimensions.height as usize)
    }

    /// Returns the current HiDPI factor for this window.
    pub fn get_hidpi_factor(&self) -> f32 {
        self.size.hidpi_factor
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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
#[repr(C)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WaylandTheme {
    /// Primary color when the window is focused
    pub primary_color_active: [u8;4],
    /// Primary color when the window is unfocused
    pub primary_color_inactive: [u8;4],
    /// Secondary color when the window is focused
    pub secondary_color_active: [u8;4],
    /// Secondary color when the window is unfocused
    pub secondary_color_inactive: [u8;4],
    /// Close button color (idle state)
    pub close_button_color_idle: [u8;4],
    /// Close button color (hovered state)
    pub close_button_color_hovered: [u8;4],
    /// Close button color (disabled state)
    pub close_button_color_disabled: [u8;4],
    /// Maximize button color (idle state)
    pub maximize_button_color_idle: [u8;4],
    /// Maximize button color (hovered state)
    pub maximize_button_color_hovered: [u8;4],
    /// Maximize button color (disabled state)
    pub maximize_button_color_disabled: [u8;4],
    /// Minimize button color (idle state)
    pub minimize_button_color_idle: [u8;4],
    /// Minimize button color (hovered state)
    pub minimize_button_color_hovered: [u8;4],
    /// Minimize button color (disabled state)
    pub minimize_button_color_disabled: [u8;4],
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct WindowSize {
    /// Width and height of the window, in logical
    /// units (may not correspond to the physical on-screen size)
    pub dimensions: LogicalSize,
    /// DPI factor of the window
    pub hidpi_factor: f32,
    /// (Internal only, unused): winit HiDPI factor
    pub winit_hidpi_factor: f32,
    /// Minimum dimensions of the window
    pub min_dimensions: OptionLogicalSize,
    /// Maximum dimensions of the window
    pub max_dimensions: OptionLogicalSize,
}

impl WindowSize {

    /// Get the actual logical size
    pub fn get_logical_size(&self) -> LogicalSize {
        self.dimensions
    }

    pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.dimensions.to_physical(self.hidpi_factor)
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
            min_dimensions: None.into(),
            max_dimensions: None.into(),
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.to_string().into(),
            size: WindowSize::default(),
            position: None.into(),
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            ime_position: None.into(),
            platform_specific_options: PlatformSpecificOptions::default(),
            css: Css::default(),
        }
    }
}

/// Options on how to initially create the window
#[derive(Debug, Clone)]
#[repr(C)]
pub struct WindowCreateOptions {
    /// State of the window, set the initial title / width / height here.
    pub state: WindowState,
    // /// Which monitor should the window be created on?
    // pub monitor: Monitor,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: RendererType,
    /// An optional style hot-reloader for the current window, only available with debug_assertions
    /// enabled
    pub hot_reload: OptionHotReloadOptions,
}

impl_option!(HotReloadOptions, OptionHotReloadOptions, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct HotReloadOptions {
    pub path: AzString,
    pub reload_interval: AzDuration,
    pub apply_native_css: bool,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            renderer_type: RendererType::default(),
            hot_reload: None.into(),
        }
    }
}

impl WindowCreateOptions {

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

    pub fn new_hot_reload(hot_reload_options: HotReloadOptions) -> Self {
        Self {
            hot_reload: Some(hot_reload_options).into(),
            .. Default::default()
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
#[repr(C, u8)]
pub enum RendererType {
    /// Use the hardware renderer first, then fall back to OSMesa
    Default,
    /// Force hardware rendering
    ForceHardware,
    /// Force software rendering
    ForceSoftware,
    /// Render using a custom OpenGL implementation
    #[cfg(feature = "opengl")]
    Custom(GlContextPtr),
}

impl RendererType {
    #[inline(always)]
    fn get_type(&self) -> RendererTypeNoData {
        match self {
            RendererType::Default => RendererTypeNoData::Default,
            RendererType::ForceHardware => RendererTypeNoData::ForceHardware,
            RendererType::ForceSoftware => RendererTypeNoData::ForceSoftware,
            #[cfg(feature = "opengl")]
            RendererType::Custom(_) => RendererTypeNoData::Custom,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RendererTypeNoData {
    Default,
    ForceHardware,
    ForceSoftware,
    Custom,
}

impl Clone for RendererType {
    fn clone(&self) -> Self {
        use self::RendererType::*;
        match self {
            Default => Default,
            ForceHardware => ForceHardware,
            ForceSoftware => ForceSoftware,
            #[cfg(feature = "opengl")]
            Custom(gl) => Custom(gl.clone()),
        }
    }
}

impl PartialOrd for RendererType {
    fn partial_cmp(&self, other: &RendererType) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RendererType {
    fn cmp(&self, other: &RendererType) -> Ordering {
        self.get_type().cmp(&other.get_type())
    }
}

impl PartialEq for RendererType {
    fn eq(&self, other: &RendererType) -> bool {
        self.get_type().eq(&other.get_type())
    }
}

impl Eq for RendererType { }

impl Hash for RendererType {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.get_type().hash(state);
    }
}

impl fmt::Debug for RendererType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RendererType::Default => write!(f, "Default"),
            RendererType::ForceHardware => write!(f, "ForceHardware"),
            RendererType::ForceSoftware => write!(f, "ForceSoftware"),
            #[cfg(feature = "opengl")]
            RendererType::Custom(_) => write!(f, "Custom"),
        }
    }
}

impl Default for RendererType {
    fn default() -> Self {
        RendererType::Default
    }
}

/// Custom event type, to construct an `EventLoop<AzulWindowUpdateEvent>`.
/// This is dispatched into the `EventLoop` (to send a "custom" event)
pub enum AzulUpdateEvent {
    CreateWindow {
        window_create_options: WindowCreateOptions,
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
    RedrawRequested {
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

impl fmt::Debug for AzulUpdateEvent {
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
            RedrawRequested { window_id } => write!(f, "RedrawRequested {{ window_id: {:?} }}", window_id),
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

pub struct DetermineCallbackResult {
    pub hit_test_item: Option<HitTestItem>,
    pub normal_callbacks: BTreeMap<EventFilter, Callback>,
}

impl DetermineCallbackResult {

    pub fn has_normal_callbacks(&self) -> bool {
        !self.normal_callbacks.is_empty()
    }

    pub fn has_any_callbacks(&self) -> bool {
        self.has_normal_callbacks()
    }
}

impl Default for DetermineCallbackResult {
    fn default() -> Self {
        DetermineCallbackResult {
            hit_test_item: None,
            normal_callbacks: BTreeMap::new(),
        }
    }
}

impl Clone for DetermineCallbackResult {
    fn clone(&self) -> Self {
        DetermineCallbackResult {
            hit_test_item: self.hit_test_item.clone(),
            normal_callbacks: self.normal_callbacks.clone(),
        }
    }
}

impl fmt::Debug for DetermineCallbackResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
        "DetermineCallbackResult {{ \
            hit_test_item: {:?},\
            normal_callbacks: {:#?},\
        }}",
            self.hit_test_item,
            self.normal_callbacks.keys().collect::<Vec<_>>(),
        )
    }
}

pub struct CallbacksOfHitTest {
    /// A BTreeMap where each item is already filtered by the proper hit-testing type,
    /// meaning in order to get the proper callbacks, you simply have to iterate through
    /// all node IDs
    pub nodes_with_callbacks: BTreeMap<NodeId, DetermineCallbackResult>,
    /// Same as `needs_redraw_anyways`, but for reusing the layout from the previous frame.
    /// Each `:hover` and `:active` group stores whether it modifies the layout, as
    /// a performance optimization.
    pub needs_relayout_anyways: bool,
    /// Whether the screen should be redrawn even if no Callback returns an `UpdateScreen::Redraw`.
    /// This is necessary for `:hover` and `:active` mouseovers - otherwise the screen would
    /// only update on the next resize.
    pub needs_redraw_anyways: bool,
}

impl Default for CallbacksOfHitTest {
    fn default() -> Self {
        Self {
            nodes_with_callbacks: BTreeMap::new(),
            needs_redraw_anyways: false,
            needs_relayout_anyways: false,
        }
    }
}

impl fmt::Debug for CallbacksOfHitTest {
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

impl CallbacksOfHitTest {
    /// Returns whether there is any
    pub fn should_call_callbacks(&self) -> bool {
        !self.nodes_with_callbacks.is_empty() &&
        self.nodes_with_callbacks.values().any(|n| n.has_any_callbacks())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct LogicalRect {
    pub origin: LogicalPosition,
    pub size: LogicalSize,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

const DECIMAL_MULTIPLIER: f32 = 1000.0;

impl_option!(LogicalPosition, OptionLogicalPosition, [Debug, Copy, Clone, PartialEq, PartialOrd]);

impl Ord for LogicalPosition {
    fn cmp(&self, other: &LogicalPosition) -> Ordering {
        let self_x = (self.x * DECIMAL_MULTIPLIER) as usize;
        let self_y = (self.y * DECIMAL_MULTIPLIER) as usize;
        let other_x = (other.x * DECIMAL_MULTIPLIER) as usize;
        let other_y = (other.y * DECIMAL_MULTIPLIER) as usize;
        self_x.cmp(&other_x).then(self_y.cmp(&other_y))
    }
}

impl Eq for LogicalPosition { }

impl Hash for LogicalPosition {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        let self_x = (self.x * DECIMAL_MULTIPLIER) as usize;
        let self_y = (self.y * DECIMAL_MULTIPLIER) as usize;
        self_x.hash(state);
        self_y.hash(state);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl_option!(LogicalSize, OptionLogicalSize, [Debug, Copy, Clone, PartialEq, PartialOrd]);

impl Ord for LogicalSize {
    fn cmp(&self, other: &LogicalSize) -> Ordering {
        let self_width = (self.width * DECIMAL_MULTIPLIER) as usize;
        let self_height = (self.height * DECIMAL_MULTIPLIER) as usize;
        let other_width = (other.width * DECIMAL_MULTIPLIER) as usize;
        let other_height = (other.height * DECIMAL_MULTIPLIER) as usize;
        self_width.cmp(&other_width).then(self_height.cmp(&other_height))
    }
}

impl Eq for LogicalSize { }

impl Hash for LogicalSize {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        let self_width = (self.width * DECIMAL_MULTIPLIER) as usize;
        let self_height = (self.height * DECIMAL_MULTIPLIER) as usize;
        self_width.hash(state);
        self_height.hash(state);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PhysicalPosition<T> {
    pub x: T,
    pub y: T,
}

type PhysicalPositionI32 = PhysicalPosition<i32>;
impl_option!(PhysicalPositionI32, OptionPhysicalPositionI32, [Debug, Copy, Clone, PartialEq, PartialOrd]);

#[derive(Debug, Ord, Hash, Eq, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PhysicalSize<T> {
    pub width: T,
    pub height: T,
}

pub type PhysicalSizeU32 = PhysicalSize<u32>;
impl_option!(PhysicalSizeU32, OptionPhysicalSizeU32, [Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]);
pub type PhysicalSizeF32 = PhysicalSize<f32>;
impl_option!(PhysicalSizeF32, OptionPhysicalSizeF32, [Debug, Copy, Clone, PartialEq, PartialOrd]);

impl LogicalPosition {
    #[inline(always)]
    pub const fn new(x: f32, y: f32) -> Self { Self { x, y } }
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalPosition<u32> {
        PhysicalPosition {
            x: (self.x * hidpi_factor) as u32,
            y: (self.y * hidpi_factor) as u32,
        }
    }
}

impl<T> PhysicalPosition<T> {
    #[inline(always)]
    pub const fn new(x: T, y: T) -> Self { Self { x, y } }
}

impl PhysicalPosition<i32> {
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0, 0) }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x as f32 / hidpi_factor,
            y: self.y as f32 / hidpi_factor,
        }
    }
}

impl PhysicalPosition<f64> {
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalPosition {
        LogicalPosition {
            x: self.x as f32 / hidpi_factor,
            y: self.y as f32 / hidpi_factor,
        }
    }
}

impl LogicalSize {
    #[inline(always)]
    pub const fn new(width: f32, height: f32) -> Self { Self { width, height } }
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0.0, 0.0) }
    #[inline(always)]
    pub fn to_physical(self, hidpi_factor: f32) -> PhysicalSize<u32> {
        PhysicalSize {
            width: (self.width * hidpi_factor) as u32,
            height: (self.height * hidpi_factor) as u32,
        }
    }
}

impl<T> PhysicalSize<T> {
    #[inline(always)]
    pub const fn new(width: T, height: T) -> Self { Self { width, height } }
}

impl PhysicalSize<u32> {
    #[inline(always)]
    pub const fn zero() -> Self { Self::new(0, 0) }
    #[inline(always)]
    pub fn to_logical(self, hidpi_factor: f32) -> LogicalSize {
        LogicalSize {
            width: self.width as f32 / hidpi_factor,
            height: self.height as f32 / hidpi_factor,
        }
    }
}

/// Utility function for easier creation of a keymap - i.e. `[vec![Ctrl, S], my_function]`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
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
            Key(k) => keyboard_state.pressed_virtual_keycodes.iter().any(|key| key == k),
        }
    }
}

/// Symbolic name for a keyboard key, does NOT take the keyboard locale into account
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
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

/// 16x16x4 bytes icon
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SmallWindowIconBytes {
    pub key: IconKey,
    pub rgba_bytes: U8Vec,
}

/// 16x16x4 bytes icon
#[derive(Debug, Clone)]
#[repr(C)]
pub struct LargeWindowIconBytes {
    pub key: IconKey,
    pub rgba_bytes: U8Vec,
}

// Window icon that usually appears in the top-left corner of the window
#[derive(Debug, Clone)]
#[repr(C)]
pub enum WindowIcon {
    Small(SmallWindowIconBytes),
    /// 32x32x4 bytes icon
    Large(LargeWindowIconBytes),
}

impl_option!(WindowIcon, OptionWindowIcon, copy = false, [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]);

impl WindowIcon {
    pub fn get_key(&self) -> IconKey {
        match &self {
            WindowIcon::Small(SmallWindowIconBytes { key, .. }) => *key,
            WindowIcon::Large(LargeWindowIconBytes { key, .. }) => *key,
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
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some((self.get_key()).cmp(&rhs.get_key()))
    }
}

impl Eq for WindowIcon { }

impl Ord for WindowIcon {
    fn cmp(&self, rhs: &Self) -> Ordering {
        (self.get_key()).cmp(&rhs.get_key())
    }
}

impl Hash for WindowIcon {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.get_key().hash(state);
    }
}

/// 256x256x4 bytes window icon
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TaskBarIcon {
    pub key: IconKey,
    pub rgba_bytes: U8Vec,
}

impl_option!(TaskBarIcon, OptionTaskBarIcon, copy = false, [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]);

impl PartialEq for TaskBarIcon {
    fn eq(&self, rhs: &Self) -> bool {
        self.key == rhs.key
    }
}

impl PartialOrd for TaskBarIcon {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some((self.key).cmp(&rhs.key))
    }
}

impl Eq for TaskBarIcon { }

impl Ord for TaskBarIcon {
    fn cmp(&self, rhs: &Self) -> Ordering {
        (self.key).cmp(&rhs.key)
    }
}

impl Hash for TaskBarIcon {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        self.key.hash(state);
    }
}