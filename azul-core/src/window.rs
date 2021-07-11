use core::{
    ops,
    hash::{Hash, Hasher},
    cmp::Ordering,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
    ffi::c_void,
};
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use azul_css::{
    CssProperty, LayoutSize, U8Vec, ColorU, OptionF32,
    AzString, OptionAzString, LayoutPoint, LayoutRect,
    CssPath, OptionI32,
};
use crate::{
    FastHashMap, FastBTreeSet,
    callbacks::Callback,
    window_state::RelayoutFn,
    app_resources::{ImageRef, ImageCache, RendererResources, IdNamespace, ResourceUpdate, Epoch, ImageMask},
    styled_dom::{DomId, AzNodeId},
    id_tree::NodeId,
    callbacks::{OptionCallback, PipelineId, RefAny, DocumentId, DomNodeId, ScrollPosition, Update},
    ui_solver::{
        QuickResizeResult, OverflowingScrollNode,
        HitTest, LayoutResult, ExternalScrollId
    },
    display_list::{GlTextureCache, RenderCallbacks},
    callbacks::{LayoutCallback, LayoutCallbackType},
    task::{TimerId, ThreadId, Timer, Thread},
};
use rust_fontconfig::FcFontCache;
use crate::gl::OptionGlContextPtr;

pub const DEFAULT_TITLE: &str = "Azul App";

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

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub struct RendererOptions {
    pub vsync: Vsync,
    pub srgb: Srgb,
    pub hw_accel: HwAcceleration,
}

impl_option!(RendererOptions, OptionRendererOptions, [PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash]);

impl Default for RendererOptions {
    fn default() -> Self {
        Self {
            vsync: Vsync::Enabled,
            srgb: Srgb::Disabled,
            hw_accel: HwAcceleration::Enabled,
        }
    }
}

impl RendererOptions {
    pub const fn new(vsync: Vsync, srgb: Srgb, hw_accel: HwAcceleration) -> Self { Self { vsync, srgb, hw_accel } }
}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub enum Vsync { Enabled, Disabled, DontCare }
impl Vsync { pub const fn is_enabled(&self) -> bool { match self { Vsync::Enabled => true, _ => false } }}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub enum Srgb { Enabled, Disabled, DontCare }
impl Srgb { pub const fn is_enabled(&self) -> bool { match self { Srgb::Enabled => true, _ => false } }}

#[repr(C)]
#[derive(PartialEq, Copy, Clone, Debug, PartialOrd, Ord, Eq, Hash)]
pub enum HwAcceleration { Enabled, Disabled, DontCare }
impl HwAcceleration { pub const fn is_enabled(&self) -> bool { match self { HwAcceleration::Enabled => true, _ => false } }}


#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum RawWindowHandle {
    IOS(IOSHandle),
    MacOS(MacOSHandle),
    Xlib(XlibHandle),
    Xcb(XcbHandle),
    Wayland(WaylandHandle),
    Windows(WindowsHandle),
    Web(WebHandle),
    Android(AndroidHandle),
    Unsupported,
}

unsafe impl Send for RawWindowHandle { }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IOSHandle {
    pub ui_window: *mut c_void,
    pub ui_view: *mut c_void,
    pub ui_view_controller: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct MacOSHandle {
    pub ns_window: *mut c_void,
    pub ns_view: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XlibHandle {
    /// An Xlib Window
    pub window: u64,
    pub display: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct XcbHandle {
    /// An X11 xcb_window_t.
    pub window: u32,
    /// A pointer to an X server xcb_connection_t.
    pub connection: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WaylandHandle {
    /// A pointer to a wl_surface
    pub surface: *mut c_void,
    /// A pointer to a wl_display.
    pub display: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WindowsHandle {
    /// A Win32 HWND handle.
    pub hwnd: *mut c_void,
    /// The HINSTANCE associated with this type's HWND.
    pub hinstance: *mut c_void,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WebHandle {
    /// An ID value inserted into the data attributes of the canvas element as 'raw-handle'
    ///
    /// When accessing from JS, the attribute will automatically be called rawHandle. Each canvas
    /// created by the windowing system should be assigned their own unique ID.
    /// 0 should be reserved for invalid / null IDs.
    pub id: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct AndroidHandle {
    /// A pointer to an ANativeWindow.
    pub a_native_window: *mut c_void,
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

impl_option!(KeyboardState, OptionKeyboardState, copy = false, [Debug, Clone, PartialEq]);

// char is not ABI-stable, use u32 instead
impl_option!(u32, OptionChar, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_option!(VirtualKeyCode, OptionVirtualKeyCode, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

impl_vec!(VirtualKeyCode, VirtualKeyCodeVec, VirtualKeyCodeVecDestructor);
impl_vec_debug!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_partialord!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_ord!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_clone!(VirtualKeyCode, VirtualKeyCodeVec, VirtualKeyCodeVecDestructor);
impl_vec_partialeq!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_eq!(VirtualKeyCode, VirtualKeyCodeVec);
impl_vec_hash!(VirtualKeyCode, VirtualKeyCodeVec);

impl_vec_as_hashmap!(VirtualKeyCode, VirtualKeyCodeVec);

impl_vec!(ScanCode, ScanCodeVec, ScanCodeVecDestructor);
impl_vec_debug!(ScanCode, ScanCodeVec);
impl_vec_partialord!(ScanCode, ScanCodeVec);
impl_vec_ord!(ScanCode, ScanCodeVec);
impl_vec_clone!(ScanCode, ScanCodeVec, ScanCodeVecDestructor);
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

impl_option!(MouseState, OptionMouseState, [Debug, Copy, Clone, PartialEq, PartialOrd]);

impl_option!(MouseCursorType, OptionMouseCursorType, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

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

    pub fn get_scroll_amount(&self) -> Option<(f32, f32)> {
        const SCROLL_THRESHOLD: f32 = 0.5; // px

        if self.scroll_x.is_none() && self.scroll_y.is_none() {
            return None;
        }

        let scroll_x = self.get_scroll_x();
        let scroll_y = self.get_scroll_y();

        if libm::fabsf(scroll_x) < SCROLL_THRESHOLD && libm::fabsf(scroll_y) < SCROLL_THRESHOLD {
            return None;
        }

        Some((scroll_x, scroll_y))
    }

    /// Function reset the `scroll_x` and `scroll_y` to `None` to clear the scroll amount
    pub fn reset_scroll_to_zero(&mut self) {
        self.scroll_x = OptionF32::None;
        self.scroll_y = OptionF32::None;
    }
}

// TODO: returned by process_system_scroll
#[derive(Debug)]
pub struct ScrollResult {

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
    pub profiler_dbg: bool,
    pub render_target_dbg: bool,
    pub texture_cache_dbg: bool,
    pub gpu_time_queries: bool,
    pub gpu_sample_queries: bool,
    pub disable_batching: bool,
    pub epochs: bool,
    pub echo_driver_messages: bool,
    pub show_overdraw: bool,
    pub gpu_cache_dbg: bool,
    pub texture_cache_dbg_clear_evicted: bool,
    pub picture_caching_dbg: bool,
    pub primitive_dbg: bool,
    pub zoom_dbg: bool,
    pub small_screen: bool,
    pub disable_opaque_pass: bool,
    pub disable_alpha_pass: bool,
    pub disable_clip_masks: bool,
    pub disable_text_prims: bool,
    pub disable_gradient_prims: bool,
    pub obscure_images: bool,
    pub glyph_flashing: bool,
    pub smart_profiler: bool,
    pub invalidation_dbg: bool,
    pub tile_cache_logging_dbg: bool,
    pub profiler_capture: bool,
    pub force_picture_invalidation: bool,
}


#[derive(Debug, Default)]
pub struct ScrollStates(pub FastHashMap<ExternalScrollId, ScrollState>);

impl ScrollStates {

    /// Special rendering function that skips building a layout and only does
    /// hit-testing and rendering - called on pure scroll events, since it's
    /// significantly less CPU-intensive to just render the last display list instead of
    /// re-layouting on every single scroll event.
    #[must_use]
    pub fn should_scroll_render(&mut self, (scroll_x, scroll_y): &(f32, f32), hit_test: &FullHitTest) -> bool {
        let mut should_scroll_render = false;

        for hit_test in hit_test.hovered_nodes.values() {
            for scroll_hit_test_item in hit_test.scroll_hit_test_nodes.values() {
                self.scroll_node(&scroll_hit_test_item.scroll_node, *scroll_x, *scroll_y);
                should_scroll_render = true;
                break; // only scroll first node that was hit
            }
        }

        should_scroll_render
    }

    pub fn new() -> ScrollStates {
        ScrollStates::default()
    }

    pub fn get_scroll_position(&self, scroll_id: &ExternalScrollId) -> Option<LogicalPosition> {
        self.0.get(&scroll_id).map(|entry| entry.get())
    }

    /// Set the scroll amount - does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub fn set_scroll_position(&mut self, node: &OverflowingScrollNode, scroll_position: LogicalPosition) {
        self.0.entry(node.parent_external_scroll_id)
        .or_insert_with(|| ScrollState::default())
        .set(scroll_position.x, scroll_position.y, &node.child_rect);
    }

    /// Updating (add to) the existing scroll amount does not update the `entry.used_this_frame`,
    /// since that is only relevant when we are actually querying the renderer.
    pub fn scroll_node(&mut self, node: &OverflowingScrollNode, scroll_by_x: f32, scroll_by_y: f32) {
        self.0.entry(node.parent_external_scroll_id)
        .or_insert_with(|| ScrollState::default())
        .add(scroll_by_x, scroll_by_y, &node.child_rect);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollState {
    /// Amount in pixel that the current node is scrolled
    pub scroll_position: LogicalPosition,
}

impl ScrollState {

    /// Return the current position of the scroll state
    pub fn get(&self) -> LogicalPosition {
        self.scroll_position
    }

    /// Add a scroll X / Y onto the existing scroll state
    pub fn add(&mut self, x: f32, y: f32, child_rect: &LogicalRect) {
        self.scroll_position.x = (self.scroll_position.x + x).max(0.0).min(child_rect.size.width);
        self.scroll_position.y = (self.scroll_position.y + y).max(0.0).min(child_rect.size.height);
    }

    /// Set the scroll state to a new position
    pub fn set(&mut self, x: f32, y: f32, child_rect: &LogicalRect) {
        self.scroll_position.x = x.max(0.0).min(child_rect.size.width);
        self.scroll_position.y = y.max(0.0).min(child_rect.size.height);
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        ScrollState {
            scroll_position: LogicalPosition::zero(),
        }
    }
}

/// Overwrites all fields of the `FullWindowState` with the fields of the `WindowState`,
/// but leaves the extra fields such as `.hover_nodes` untouched
pub fn update_full_window_state(
    full_window_state: &mut FullWindowState,
    window_state: &WindowState
) {
    full_window_state.title = window_state.title.clone();
    full_window_state.size = window_state.size.into();
    full_window_state.position = window_state.position.into();
    full_window_state.flags = window_state.flags;
    full_window_state.debug_state = window_state.debug_state;
    full_window_state.keyboard_state = window_state.keyboard_state.clone();
    full_window_state.mouse_state = window_state.mouse_state;
    full_window_state.ime_position = window_state.ime_position.into();
    full_window_state.platform_specific_options = window_state.platform_specific_options.clone();
}

#[derive(Debug)]
pub struct WindowInternal {
    /// Currently loaded fonts and images present in this renderer (window)
    pub renderer_resources: RendererResources,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: Option<RendererType>,
    /// Windows state of the window of (current frame - 1): initialized to None on startup
    pub previous_window_state: Option<FullWindowState>,
    /// Window state of this current window (current frame): initialized to the state of WindowCreateOptions
    pub current_window_state: FullWindowState,
    /// A "document" in WebRender usually corresponds to one tab (i.e. in Azuls case, the whole window).
    pub document_id: DocumentId,
    /// ID namespace under which every font / image for this window is registered
    pub id_namespace: IdNamespace,
    /// The "epoch" is a frame counter, to remove outdated images, fonts and OpenGL textures
    /// when they're not in use anymore.
    pub epoch: Epoch,
    /// Currently active, layouted rectangles and styled DOMs
    pub layout_results: Vec<LayoutResult>,
    /// Currently GL textures inside the active CachedDisplayList
    pub gl_texture_cache: GlTextureCache,
    /// States of scrolling animations, updated every frame
    pub scroll_states: ScrollStates,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullHitTest {
    pub hovered_nodes: BTreeMap<DomId, HitTest>,
    pub focused_node: Option<(DomId, NodeId)>,
}

impl FullHitTest {
    pub fn empty(focused_node: Option<DomNodeId>) -> Self {
        Self {
            hovered_nodes: BTreeMap::new(),
            focused_node: focused_node.and_then(|f| Some((f.dom, f.node.into_crate_internal()?))),
        }
    }
}


#[derive(Debug, Clone, Default, PartialEq)]
pub struct CursorTypeHitTest {
    /// closest-node is used for determining the cursor: property
    /// The node is guaranteed to have a non-default cursor: property,
    /// so that the cursor icon can be set accordingly
    pub cursor_node: Option<(DomId, NodeId)>,
    /// Mouse cursor type to set (if cursor_node is None, this is set to `MouseCursorType::Default`)
    pub cursor_icon: MouseCursorType,
}

impl CursorTypeHitTest {
    pub fn new(hit_test: &FullHitTest, layout_results: &[LayoutResult]) -> Self {

        use azul_css::StyleCursor;

        let mut cursor_node = None;
        let mut cursor_icon = MouseCursorType::Default;

        for (dom_id, hit_nodes) in hit_test.hovered_nodes.iter() {
            for (node_id, _) in hit_nodes.regular_hit_test_nodes.iter() {

                // if the node has a non-default cursor: property, insert it
                let styled_dom = &layout_results[dom_id.inner].styled_dom;
                let node_data_container = styled_dom.node_data.as_container();
                if let Some(cursor_prop) = styled_dom.get_css_property_cache().get_cursor(&node_data_container[*node_id], node_id, &styled_dom.styled_nodes.as_container()[*node_id].state) {
                    cursor_node = Some((*dom_id, *node_id));
                    cursor_icon = match cursor_prop.get_property().copied().unwrap_or_default() {
                        StyleCursor::Alias => MouseCursorType::Alias,
                        StyleCursor::AllScroll => MouseCursorType::AllScroll,
                        StyleCursor::Cell => MouseCursorType::Cell,
                        StyleCursor::ColResize => MouseCursorType::ColResize,
                        StyleCursor::ContextMenu => MouseCursorType::ContextMenu,
                        StyleCursor::Copy => MouseCursorType::Copy,
                        StyleCursor::Crosshair => MouseCursorType::Crosshair,
                        StyleCursor::Default => MouseCursorType::Default,
                        StyleCursor::EResize => MouseCursorType::EResize,
                        StyleCursor::EwResize => MouseCursorType::EwResize,
                        StyleCursor::Grab => MouseCursorType::Grab,
                        StyleCursor::Grabbing => MouseCursorType::Grabbing,
                        StyleCursor::Help => MouseCursorType::Help,
                        StyleCursor::Move => MouseCursorType::Move,
                        StyleCursor::NResize => MouseCursorType::NResize,
                        StyleCursor::NsResize => MouseCursorType::NsResize,
                        StyleCursor::NeswResize => MouseCursorType::NeswResize,
                        StyleCursor::NwseResize => MouseCursorType::NwseResize,
                        StyleCursor::Pointer => MouseCursorType::Hand,
                        StyleCursor::Progress => MouseCursorType::Progress,
                        StyleCursor::RowResize => MouseCursorType::RowResize,
                        StyleCursor::SResize => MouseCursorType::SResize,
                        StyleCursor::SeResize => MouseCursorType::SeResize,
                        StyleCursor::Text => MouseCursorType::Text,
                        StyleCursor::Unset => MouseCursorType::Default,
                        StyleCursor::VerticalText => MouseCursorType::VerticalText,
                        StyleCursor::WResize => MouseCursorType::WResize,
                        StyleCursor::Wait => MouseCursorType::Wait,
                        StyleCursor::ZoomIn => MouseCursorType::ZoomIn,
                        StyleCursor::ZoomOut => MouseCursorType::ZoomOut,
                    }
                }
            }
        }

        Self {
            cursor_node,
            cursor_icon,
        }
    }
}
pub struct WindowInternalInit {
    pub window_create_options: WindowCreateOptions,
    pub document_id: DocumentId,
    pub id_namespace: IdNamespace,
}

impl WindowInternal {

    /// Initializes the `WindowInternal` on window creation. Calls the layout() method once to initializes the layout
    #[cfg(all(feature = "multithreading", feature = "std"))]
    pub fn new<F>(
        mut init: WindowInternalInit,
        data: &mut RefAny,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        callbacks: &RenderCallbacks,
        fc_cache_real: &mut FcFontCache,
        relayout_fn: RelayoutFn,
        hit_test_func: F,
    ) -> Self
    where F: Fn(&FullWindowState, &ScrollStates, &[LayoutResult]) -> FullHitTest {

        use crate::callbacks::LayoutCallbackInfo;
        use crate::display_list::SolvedLayout;
        use crate::window_state::{NodesToCheck, StyleAndLayoutChanges};

        let mut inital_renderer_resources = RendererResources::default();

        let epoch = Epoch(0);

        let styled_dom = {

            let layout_callback = &mut init.window_create_options.state.layout_callback;
            let layout_info = LayoutCallbackInfo::new(
                init.window_create_options.state.size,
                init.window_create_options.state.theme,
                image_cache,
                gl_context,
                &fc_cache_real,
            );

            match layout_callback {
                LayoutCallback::Raw(r) => (r.cb)(data, layout_info),
                LayoutCallback::Marshaled(m) => {
                    let marshal_data = &mut m.marshal_data;
                    (m.cb.cb)(marshal_data, data, layout_info)
                }
            }

        };

        let mut current_window_state = FullWindowState::from_window_state(
            /*window_state: */ &init.window_create_options.state,
            /*dropped_file: */ None,
            /*hovered_file: */ None,
            /*focused_node: */ None,
            /*last_hit_test: */ FullHitTest::empty(/*current_focus*/ None),
        );

        let SolvedLayout { mut layout_results } = SolvedLayout::new(
            styled_dom,
            epoch,
            &init.document_id,
            &current_window_state,
            all_resource_updates,
            init.id_namespace,
            image_cache,
            &fc_cache_real,
            &callbacks,
            &mut inital_renderer_resources,
        );

        let scroll_states = ScrollStates::default();

        // apply the changes for the first frame:
        // simulate an event as if the cursor has moved over the hovered elements
        let ht = hit_test_func(&current_window_state, &scroll_states, &layout_results);
        current_window_state.last_hit_test = ht.clone();

        let nodes_to_check = NodesToCheck::simulated_mouse_move(
            &ht,
            None, // focused_node
            current_window_state.mouse_state.mouse_down()
        );

        let _ = StyleAndLayoutChanges::new(
            &nodes_to_check,
            &mut layout_results,
            &image_cache,
            &mut inital_renderer_resources,
            current_window_state.size.get_layout_size(),
            &init.document_id,
            Some(&BTreeMap::new()),
            Some(&BTreeMap::new()),
            &None,
            relayout_fn,
        );

        let gl_texture_cache = GlTextureCache::new(
            &mut layout_results,
            gl_context,
            init.id_namespace,
            &init.document_id,
            epoch,
            current_window_state.size.hidpi_factor,
            image_cache,
            &fc_cache_real,
            callbacks,
            all_resource_updates,
            &mut inital_renderer_resources,
        );

        WindowInternal {
            renderer_resources: inital_renderer_resources,
            renderer_type: gl_context.as_ref().map(|r| r.renderer_type),
            id_namespace: init.id_namespace,
            previous_window_state: None,
            current_window_state,
            document_id: init.document_id,
            epoch, // = 0
            layout_results,
            gl_texture_cache,
            scroll_states,
        }
    }

    /// Calls the layout function again and updates the self.internal.gl_texture_cache field
    #[cfg(all(feature = "multithreading"))]
    pub fn regenerate_styled_dom<F>(
        &mut self,
        data: &mut RefAny,
        image_cache: &ImageCache,
        gl_context: &OptionGlContextPtr,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        callbacks: &RenderCallbacks,
        fc_cache_real: &mut FcFontCache,
        relayout_fn: RelayoutFn,
        hit_test_func: F,
    ) where F: Fn(&FullWindowState, &ScrollStates, &[LayoutResult]) -> FullHitTest {

        use crate::callbacks::LayoutCallbackInfo;
        use crate::display_list::SolvedLayout;
        use crate::window_state::{NodesToCheck, StyleAndLayoutChanges};
        use crate::styled_dom::DefaultCallbacksCfg;

        let id_namespace = self.id_namespace;

        let mut styled_dom = {

            let layout_callback = &mut self.current_window_state.layout_callback;
            let layout_info = LayoutCallbackInfo::new(
                self.current_window_state.size,
                self.current_window_state.theme,
                image_cache,
                gl_context,
                &fc_cache_real,
            );

            match layout_callback {
                LayoutCallback::Raw(r) => (r.cb)(data, layout_info),
                LayoutCallback::Marshaled(m) => {
                    let marshal_data = &mut m.marshal_data;
                    (m.cb.cb)(marshal_data, data, layout_info)
                }
            }
        };

        styled_dom.insert_default_system_callbacks(DefaultCallbacksCfg {
            smooth_scroll: self.current_window_state.flags.smooth_scroll_enabled,
            enable_autotab: self.current_window_state.flags.autotab_enabled,
        });

        let SolvedLayout {
            mut layout_results,
        } = SolvedLayout::new(
            styled_dom,
            self.epoch,
            &self.document_id,
            &self.current_window_state,
            all_resource_updates,
            id_namespace,
            image_cache,
            &fc_cache_real,
            callbacks,
            &mut self.renderer_resources,
        );

        // apply the changes for the first frame
        let ht = hit_test_func(&self.current_window_state, &self.scroll_states, &layout_results);
        self.current_window_state.last_hit_test = ht.clone();

        // hit_test
        let nodes_to_check = NodesToCheck::simulated_mouse_move(
            &ht,
            self.current_window_state.focused_node,
            self.current_window_state.mouse_state.mouse_down()
        );

        let _ = StyleAndLayoutChanges::new(
            &nodes_to_check,
            &mut layout_results,
            &image_cache,
            &mut self.renderer_resources,
            self.current_window_state.size.get_layout_size(),
            &self.document_id,
            Some(&BTreeMap::new()),
            Some(&BTreeMap::new()),
            &None,
            relayout_fn,
        );

        let gl_texture_cache = GlTextureCache::new(
            &mut layout_results,
            gl_context,
            self.id_namespace,
            &self.document_id,
            self.epoch,
            self.current_window_state.size.hidpi_factor,
            image_cache,
            &fc_cache_real,
            callbacks,
            all_resource_updates,
            &mut self.renderer_resources,
        );

        // Delete unused font and image keys (that were not used in this frame)
        self.renderer_resources.do_gc(all_resource_updates);
        self.layout_results = layout_results;
        self.gl_texture_cache = gl_texture_cache;
        self.epoch.0 += 1;
    }

    /// Returns a copy of the current scroll states + scroll positions
    pub fn get_current_scroll_states(&self) -> BTreeMap<DomId, BTreeMap<AzNodeId, ScrollPosition>> {
        self.layout_results
        .iter()
        .enumerate()
        .filter_map(|(dom_id, layout_result)| {
            let scroll_positions = layout_result.scrollable_nodes.overflowing_nodes.iter().filter_map(|(node_id, overflowing_node)| {
                let scroll_position = ScrollPosition {
                    parent_rect: overflowing_node.parent_rect,
                    children_rect: overflowing_node.child_rect,
                };
                Some((*node_id, scroll_position))
            }).collect::<BTreeMap<_, _>>();

            if scroll_positions.is_empty() {
                None
            } else {
                Some((DomId { inner: dom_id }, scroll_positions))
            }
        }).collect()
    }

    /// Returns the overflowing size of the root body node. If WindowCreateOptions.size_to_content
    /// is set, the window size should be adjusted to this size before the window is shown.
    pub fn get_content_size(&self) -> LogicalSize {
        let layout_result = match self.layout_results.get(0) {
            Some(s) => s,
            None => return LogicalSize::zero(),
        };
        let root_width = layout_result.width_calculated_rects.as_ref()[NodeId::ZERO].overflow_width();
        let root_height = layout_result.height_calculated_rects.as_ref()[NodeId::ZERO].overflow_height();
        LogicalSize::new(root_width, root_height)
    }

    /// Does a full re-layout (without calling layout()) again:
    /// called in simple resize() scenarios
    pub fn do_quick_resize(
        &mut self,
        image_cache: &ImageCache,
        callbacks: &RenderCallbacks,
        relayout_fn: RelayoutFn,
        fc_cache: &FcFontCache,
        window_state: &FullWindowState,
    ) -> QuickResizeResult {
        LayoutResult::do_quick_resize(
            &self.document_id,
            DomId::ROOT_ID,
            LogicalRect::new(LogicalPosition::zero(), window_state.size.dimensions),
            image_cache,
            &mut self.layout_results,
            &mut self.gl_texture_cache,
            &mut self.renderer_resources,
            callbacks,
            relayout_fn,
            fc_cache,
            window_state,
        )
    }

    // Compares the previous and current window size and returns
    // true only if the rendering area increased, but not if it
    // decreased. This is useful to prevent unnecessary redraws
    // if the user resized the window - since the content of the
    // window is cached by the operating system, making the window
    // smaller should result in a no-op.
    pub fn resized_area_increased(&self) -> bool {
        let previous_state = match self.previous_window_state.as_ref() {
            None => return true,
            Some(s) => s.size.dimensions,
        };
        let current_state = &self.current_window_state.size.dimensions;

        current_state.width > previous_state.width ||
        current_state.height > previous_state.height
    }

    /// Returns whether the size or position of the window changed (if true,
    /// the caller needs to update the monitor field), since the window may have
    /// moved to a different monitor
    pub fn may_have_changed_monitor(&self) -> bool {
        let previous = match self.previous_window_state.as_ref() {
            None => return true,
            Some(s) => s,
        };
        let current = &self.current_window_state;

        previous.size.dimensions != current.size.dimensions &&
        previous.position != current.position
    }

    pub fn get_layout_size(&self) -> LayoutSize {
        LayoutSize::new(
            libm::roundf(self.current_window_state.size.dimensions.width) as isize,
            libm::roundf(self.current_window_state.size.dimensions.height) as isize
        )
    }

    /// Returns the menu bar set on the LayoutResults[0] node 0 or None
    pub fn get_menu_bar<'a>(&'a self) -> Option<&'a Box<Menu>> {
        let lr = self.layout_results.get(0)?;
        let ndc = lr.styled_dom.node_data.as_container();
        let nd = ndc.get_extended_lifetime(NodeId::ZERO)?;
        nd.get_menu_bar()
    }

    /// Returns the current context menu on the nearest hit node
    /// or None if no context menu was found
    pub fn get_context_menu<'a>(&'a self, hit_test: &FullHitTest) -> Option<&'a Box<Menu>> {
        let mut context_menu = None;

        for (dom_id, hit_test) in hit_test.hovered_nodes.iter() {
            let layout_result = self.layout_results.get(dom_id.inner)?;
            for node_id in hit_test.regular_hit_test_nodes.keys() {
                let ndc = layout_result.styled_dom.node_data.as_container();
                if let Some(cm) = ndc.get_extended_lifetime(*node_id).and_then(|node| node.get_context_menu()) {
                    context_menu = Some(cm);
                }
            }
        }
        context_menu
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
#[repr(C)]
pub struct TouchState {
    /// TODO: not yet implemented
    pub unimplemented: u8,
}

/// State, size, etc of the window, for comparing to the last frame
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
#[repr(C)]
pub enum WindowTheme {
    DarkMode,
    LightMode,
}

impl Default for WindowTheme {
    fn default() -> WindowTheme {
        WindowTheme::LightMode // sorry!
    }
}

impl_option!(WindowTheme, OptionWindowTheme, [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]);


#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Monitor {
    pub id: usize,
    pub name: OptionAzString,
    pub size: LayoutSize,
    pub position: LayoutPoint,
    pub scale_factor: f64,
    pub video_modes: VideoModeVec,
    pub is_primary_monitor: bool,
}

impl_vec!(Monitor, MonitorVec, MonitorVecDestructor);
impl_vec_debug!(Monitor, MonitorVec);
impl_vec_clone!(Monitor, MonitorVec, MonitorVecDestructor);
impl_vec_partialeq!(Monitor, MonitorVec);
impl_vec_partialord!(Monitor, MonitorVec);

impl core::hash::Hash for Monitor {
    fn hash<H>(&self, state: &mut H) where H: core::hash::Hasher {
        self.id.hash(state)
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor {
            id: 0,
            name: OptionAzString::None,
            size: LayoutSize::zero(),
            position: LayoutPoint::zero(),
            scale_factor: 1.0,
            video_modes: Vec::new().into(),
            is_primary_monitor: false,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct VideoMode {
    pub size: LayoutSize,
    pub bit_depth: u16,
    pub refresh_rate: u16,
}

impl_vec!(VideoMode, VideoModeVec, VideoModeVecDestructor);
impl_vec_clone!(VideoMode, VideoModeVec, VideoModeVecDestructor);
impl_vec_debug!(VideoMode, VideoModeVec);
impl_vec_partialeq!(VideoMode, VideoModeVec);
impl_vec_partialord!(VideoMode, VideoModeVec);

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowState {
    pub title: AzString,
    /// Theme of this window (dark or light) - can be set / overridden by the user
    ///
    /// Usually the operating system will set this field. On change, it will
    /// emit a `WindowEventFilter::ThemeChanged` event
    pub theme: WindowTheme,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: WindowPosition,
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
    /// Stores all states of currently connected touch input devices, pencils, tablets, etc.
    pub touch_state: TouchState,
    /// Sets location of IME candidate box in client area coordinates
    /// relative to the top left of the window.
    pub ime_position: ImePosition,
    /// Which monitor the window is currently residing on
    pub monitor: Monitor,
    /// Window options that can only be set on a certain platform
    /// (`WindowsWindowOptions` / `LinuxWindowOptions` / `MacWindowOptions`).
    pub platform_specific_options: PlatformSpecificOptions,
    /// Whether this window has SRGB / vsync / hardware acceleration
    pub renderer_options: RendererOptions,
    /// Color of the window background (can be transparent if necessary)
    pub background_color: ColorU,
    /// The `layout()` function for this window, stored as a callback function pointer,
    /// There are multiple reasons for doing this (instead of requiring `T: Layout` everywhere):
    ///
    /// - It seperates the `Dom` from the `Layout` trait, making it possible to split the
    ///   UI solving and styling into reusable crates
    /// - It's less typing work (prevents having to type `<T: Layout>` everywhere)
    /// - It's potentially more efficient to compile (less type-checking required)
    /// - It's a preparation for the C ABI, in which traits don't exist (for language bindings).
    ///   In the C ABI "traits" are simply structs with function pointers (and void* instead of T)
    pub layout_callback: LayoutCallback,
    /// Optional callback to run when the window closes
    pub close_callback: OptionCallback,
}

impl_option!(WindowState, OptionWindowState, copy = false, [Debug, Clone, PartialEq]);

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C, u8)]
pub enum WindowPosition {
    Uninitialized,
    Initialized(PhysicalPositionI32)
}

impl Default for WindowPosition {
    fn default() -> WindowPosition {
        WindowPosition::Uninitialized
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ImePosition {
    Uninitialized,
    Initialized(LogicalPosition)
}

impl Default for ImePosition {
    fn default() -> ImePosition {
        ImePosition::Uninitialized
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullWindowState {
    /// Theme of this window (dark or light) - can be set / overridden by the user
    ///
    /// Usually the operating system will set this field. On change, it will
    /// emit a `WindowEventFilter::ThemeChanged` event
    pub theme: WindowTheme,
    /// Current title of the window
    pub title: AzString,
    /// Size of the window + max width / max height: 800 x 600 by default
    pub size: WindowSize,
    /// The x and y position, or None to let the WM decide where to put the window (default)
    pub position: WindowPosition,
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
    /// Stores all states of currently connected touch input devices, pencils, tablets, etc.
    pub touch_state: TouchState,
    /// Sets location of IME candidate box in client area coordinates
    /// relative to the top left of the window.
    pub ime_position: ImePosition,
    /// Window options that can only be set on a certain platform
    /// (`WindowsWindowOptions` / `LinuxWindowOptions` / `MacWindowOptions`).
    pub platform_specific_options: PlatformSpecificOptions,
    /// Information about vsync and hardware acceleration
    pub renderer_options: RendererOptions,
    /// Background color of the window
    pub background_color: ColorU,
    /// The `layout()` function for this window, stored as a callback function pointer,
    /// There are multiple reasons for doing this (instead of requiring `T: Layout` everywhere):
    ///
    /// - It seperates the `Dom` from the `Layout` trait, making it possible to split the
    ///   UI solving and styling into reusable crates
    /// - It's less typing work (prevents having to type `<T: Layout>` everywhere)
    /// - It's potentially more efficient to compile (less type-checking required)
    /// - It's a preparation for the C ABI, in which traits don't exist (for language bindings).
    ///   In the C ABI "traits" are simply structs with function pointers (and void* instead of T)
    pub layout_callback: LayoutCallback,
    /// Callback to run before the window closes. If this callback returns `DoNothing`,
    /// the window won't close, otherwise it'll close regardless
    pub close_callback: OptionCallback,
    // --
    /// Current monitor
    pub monitor: Monitor,
    /// Whether there is a file currently hovering over the window
    pub hovered_file: Option<AzString>, // Option<PathBuf>
    /// Whether there was a file currently dropped on the window
    pub dropped_file: Option<AzString>, // Option<PathBuf>
    /// What node is currently hovered over, default to None. Only necessary internal
    /// to the crate, for emitting `On::FocusReceived` and `On::FocusLost` events,
    /// as well as styling `:focus` elements
    pub focused_node: Option<DomNodeId>,
    /// Last hit-test that was performed: necessary because the
    /// events are stored in a queue and only storing the hovered
    /// nodes is not sufficient to correctly determine events
    pub last_hit_test: FullHitTest,
}

impl Default for FullWindowState {
    fn default() -> Self {
        Self {
            theme: WindowTheme::default(),
            title: AzString::from_const_str(DEFAULT_TITLE),
            size: WindowSize::default(),
            position: WindowPosition::Uninitialized,
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            touch_state: TouchState::default(),
            ime_position: ImePosition::Uninitialized,
            platform_specific_options: PlatformSpecificOptions::default(),
            background_color: ColorU::WHITE,
            layout_callback: LayoutCallback::default(),
            close_callback: OptionCallback::None,
            renderer_options: RendererOptions::default(),
            monitor: Monitor::default(),
            // --

            hovered_file: None,
            dropped_file: None,
            focused_node: None,
            last_hit_test: FullHitTest::empty(None),
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

    pub fn get_hovered_file(&self) -> Option<&AzString> {
        self.hovered_file.as_ref()
    }

    pub fn get_dropped_file(&self) -> Option<&AzString> {
        self.dropped_file.as_ref()
    }

    pub fn get_scroll_amount(&self) -> Option<(f32, f32)> {
        self.mouse_state.get_scroll_amount()
    }

    pub fn layout_callback_changed(&self, other: &Option<Self>) -> bool {
        match other {
            Some(s) => self.layout_callback != s.layout_callback,
            None => false,
        }
    }

    /// Creates a FullWindowState from a regular WindowState,
    /// fills non-available fields with the given values
    ///
    /// You need to pass the extra fields explicitly in order
    /// to prevent state management bugs
    pub fn from_window_state(
        window_state: &WindowState,
        dropped_file: Option<AzString>,
        hovered_file: Option<AzString>,
        focused_node: Option<DomNodeId>,
        last_hit_test: FullHitTest,
    ) -> Self {
        Self {
            monitor: window_state.monitor.clone(),
            theme: window_state.theme,
            title: window_state.title.clone(),
            size: window_state.size,
            position: window_state.position.into(),
            flags: window_state.flags,
            debug_state: window_state.debug_state,
            keyboard_state: window_state.keyboard_state.clone(),
            mouse_state: window_state.mouse_state,
            touch_state: window_state.touch_state,
            ime_position: window_state.ime_position.into(),
            platform_specific_options: window_state.platform_specific_options.clone(),
            background_color: window_state.background_color,
            layout_callback: window_state.layout_callback.clone(),
            close_callback: window_state.close_callback,
            renderer_options: window_state.renderer_options,
            dropped_file,
            hovered_file,
            focused_node,
            last_hit_test,
        }
    }

    pub fn process_system_scroll(&mut self, scroll_states: &ScrollStates) -> Option<ScrollResult> {
        let (x, y) = self.mouse_state.get_scroll_amount()?;
        // TODO
        Some(ScrollResult { })
    }
}

impl From<FullWindowState> for WindowState {
    fn from(full_window_state: FullWindowState) -> WindowState {
        WindowState {
            monitor: full_window_state.monitor.clone(),
            theme: full_window_state.theme,
            title: full_window_state.title.into(),
            size: full_window_state.size,
            position: full_window_state.position.into(),
            flags: full_window_state.flags,
            debug_state: full_window_state.debug_state,
            keyboard_state: full_window_state.keyboard_state,
            mouse_state: full_window_state.mouse_state,
            touch_state: full_window_state.touch_state,
            ime_position: full_window_state.ime_position.into(),
            platform_specific_options: full_window_state.platform_specific_options,
            background_color: full_window_state.background_color,
            layout_callback: full_window_state.layout_callback,
            close_callback: full_window_state.close_callback,
            renderer_options: full_window_state.renderer_options,
        }
    }
}

#[derive(Debug)]
pub struct CallCallbacksResult {
    /// Whether the UI should be rendered anyways due to a (programmatic or user input) scroll event
    pub should_scroll_render: bool,
    /// Whether the callbacks say to rebuild the UI or not
    pub callbacks_update_screen: Update,
    /// WindowState that was (potentially) modified in the callbacks
    pub modified_window_state: Option<WindowState>,
    /// If a word changed (often times the case with text input), we don't need to relayout / rerender
    /// the whole screen. The result is passed to the `relayout()` function, which will only change the
    /// single node that was modified
    pub words_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, AzString>>>,
    /// A callback can "exchange" and image for a new one without requiring a new display list to be
    /// rebuilt. This is important for animated images, especially video.
    pub images_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, ImageRef>>>,
    /// Same as images, clip masks can be changed in callbacks, often the case with vector animations
    pub image_masks_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>>,
    /// If the focus target changes in the callbacks, the function will automatically
    /// restyle the DOM and set the new focus target
    pub css_properties_changed: Option<BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>>,
    /// If the callbacks have scrolled any nodes, the new scroll position will be stored here
    pub nodes_scrolled_in_callbacks: Option<BTreeMap<DomId, BTreeMap<AzNodeId, LogicalPosition>>>,
    /// Whether the focused node was changed from the callbacks
    pub update_focused_node: Option<Option<DomNodeId>>,
    /// Timers that were added in the callbacks
    pub timers: Option<FastHashMap<TimerId, Timer>>,
    /// Tasks that were added in the callbacks
    pub threads: Option<FastHashMap<ThreadId, Thread>>,
    /// Timers that were added in the callbacks
    pub timers_removed: Option<FastBTreeSet<TimerId>>,
    /// Tasks that were added in the callbacks
    pub threads_removed: Option<FastBTreeSet<ThreadId>>,
    /// Windows that were created in the callbacks
    pub windows_created: Vec<WindowCreateOptions>,
    /// Whether the cursor changed in the callbacks
    pub cursor_changed: bool,
}

impl CallCallbacksResult {
    pub fn cursor_changed(&self) -> bool {
        self.cursor_changed
    }
    pub fn focus_changed(&self) -> bool {
        self.update_focused_node.is_some()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct WindowFlags {
    /// Is the window currently maximized, minimized or fullscreen
    pub frame: WindowFrame,
    /// Is the window about to close on the next frame?
    pub is_about_to_close: bool,
    /// Does the window have decorations (close, minimize, maximize, title bar)?
    pub has_decorations: bool,
    /// Is the window currently visible?
    pub is_visible: bool,
    /// Is the window always on top?
    pub is_always_on_top: bool,
    /// Whether the window is resizable
    pub is_resizable: bool,
    /// Whether the window has focus or not (mutating this will request user attention)
    pub has_focus: bool,
    /// Whether the window has an "extended frame", i.e. the title bar is not rendered
    /// and the maximize / minimize / close buttons bleed into the window content
    pub has_extended_window_frame: bool,
    /// Whether or not the compositor should blur the application background
    pub has_blur_behind_window: bool,
    /// Is smooth scrolling enabled for this window?
    pub smooth_scroll_enabled: bool,
    /// Is automatic TAB switching supported?
    pub autotab_enabled: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub enum WindowFrame {
    Normal,
    Minimized,
    Maximized,
    Fullscreen,
}

impl Default for WindowFlags {
    fn default() -> Self {
        Self {
            frame: WindowFrame::Normal,
            is_about_to_close: false,
            has_decorations: true,
            is_visible: true,
            is_always_on_top: false,
            is_resizable: true,
            has_focus: true,
            has_extended_window_frame: false,
            has_blur_behind_window: false,
            smooth_scroll_enabled: true,
            autotab_enabled: true,
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

unsafe impl Sync for PlatformSpecificOptions { }
unsafe impl Send for PlatformSpecificOptions { }

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct WindowsWindowOptions {
    /// STARTUP ONLY: Whether the window should allow drag + drop operations (default: true)
    pub allow_drag_and_drop: bool,
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

impl Default for WindowsWindowOptions {
    fn default() -> WindowsWindowOptions {
        WindowsWindowOptions {
            allow_drag_and_drop: true,
            no_redirection_bitmap: false,
            window_icon: OptionWindowIcon::None,
            taskbar_icon: OptionTaskBarIcon::None,
            parent_window: OptionHwndHandle::None,
        }
    }
}

/// Note: this should be a *mut HWND
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

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub enum UserAttentionType {
    None,
    Critical,
    Informational,
}

impl Default for UserAttentionType {
    fn default() -> UserAttentionType {
        UserAttentionType::None
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
    pub request_user_attention: UserAttentionType,
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

impl_vec!(AzStringPair, StringPairVec, StringPairVecDestructor);
impl_vec_mut!(AzStringPair, StringPairVec);
impl_vec_debug!(AzStringPair, StringPairVec);
impl_vec_partialord!(AzStringPair, StringPairVec);
impl_vec_ord!(AzStringPair, StringPairVec);
impl_vec_clone!(AzStringPair, StringPairVec, StringPairVecDestructor);
impl_vec_partialeq!(AzStringPair, StringPairVec);
impl_vec_eq!(AzStringPair, StringPairVec);
impl_vec_hash!(AzStringPair, StringPairVec);

impl_option!(StringPairVec, OptionStringPairVec, copy = false, [Debug, Clone, PartialOrd, PartialEq, Ord, Eq, Hash]);

impl StringPairVec {
    pub fn get_key(&self, search_key: &str) -> Option<&AzString> {
        self.as_ref().iter().find_map(|v| if v.key.as_str() == search_key { Some(&v.value) } else { None })
    }
    pub fn get_key_mut(&mut self, search_key: &str) -> Option<&mut AzStringPair> {
        self.as_mut().iter_mut().find(|v| v.key.as_str() == search_key)
    }
    pub fn insert_kv<I: Into<AzString>>(&mut self, key: I, value: I) {
        let key = key.into();
        let value = value.into();
        match self.get_key_mut(key.as_str()) {
            None => { },
            Some(s) => { s.value = value; return; }
        }
        self.push(AzStringPair { key, value });
    }
}

impl_vec!(XWindowType, XWindowTypeVec, XWindowTypeVecDestructor);
impl_vec_debug!(XWindowType, XWindowTypeVec);
impl_vec_partialord!(XWindowType, XWindowTypeVec);
impl_vec_ord!(XWindowType, XWindowTypeVec);
impl_vec_clone!(XWindowType, XWindowTypeVec, XWindowTypeVecDestructor);
impl_vec_partialeq!(XWindowType, XWindowTypeVec);
impl_vec_eq!(XWindowType, XWindowTypeVec);
impl_vec_hash!(XWindowType, XWindowTypeVec);

impl_option!(WaylandTheme, OptionWaylandTheme, copy = false, [Debug, Clone, PartialEq, PartialOrd]);

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct MacWindowOptions {
    pub reserved: u8,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct WasmWindowOptions {
    // empty for now, single field must be present for ABI compat - always set to 0
    pub _reserved: u8,
}

impl WindowState {

    /// Creates a new, default `WindowState` with the given CSS style
    pub fn new(callback: LayoutCallbackType) -> Self {
        use crate::callbacks::LayoutCallbackInner;
        Self {
            layout_callback: LayoutCallback::Raw(
                LayoutCallbackInner { cb: callback }
            ),
            .. Default::default()
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
// Translation type because in winit 24.0 the WinitWaylandTheme is a trait instead
// of a struct, which makes things more complicated
pub struct WaylandTheme {
    pub title_bar_active_background_color: [u8;4],
    pub title_bar_active_separator_color: [u8;4],
    pub title_bar_active_text_color: [u8;4],
    pub title_bar_inactive_background_color: [u8;4],
    pub title_bar_inactive_separator_color: [u8;4],
    pub title_bar_inactive_text_color: [u8;4],
    pub maximize_idle_foreground_inactive_color: [u8;4],
    pub minimize_idle_foreground_inactive_color: [u8;4],
    pub close_idle_foreground_inactive_color: [u8;4],
    pub maximize_hovered_foreground_inactive_color: [u8;4],
    pub minimize_hovered_foreground_inactive_color: [u8;4],
    pub close_hovered_foreground_inactive_color: [u8;4],
    pub maximize_disabled_foreground_inactive_color: [u8;4],
    pub minimize_disabled_foreground_inactive_color: [u8;4],
    pub close_disabled_foreground_inactive_color: [u8;4],
    pub maximize_idle_background_inactive_color: [u8;4],
    pub minimize_idle_background_inactive_color: [u8;4],
    pub close_idle_background_inactive_color: [u8;4],
    pub maximize_hovered_background_inactive_color: [u8;4],
    pub minimize_hovered_background_inactive_color: [u8;4],
    pub close_hovered_background_inactive_color: [u8;4],
    pub maximize_disabled_background_inactive_color: [u8;4],
    pub minimize_disabled_background_inactive_color: [u8;4],
    pub close_disabled_background_inactive_color: [u8;4],
    pub maximize_idle_foreground_active_color: [u8;4],
    pub minimize_idle_foreground_active_color: [u8;4],
    pub close_idle_foreground_active_color: [u8;4],
    pub maximize_hovered_foreground_active_color: [u8;4],
    pub minimize_hovered_foreground_active_color: [u8;4],
    pub close_hovered_foreground_active_color: [u8;4],
    pub maximize_disabled_foreground_active_color: [u8;4],
    pub minimize_disabled_foreground_active_color: [u8;4],
    pub close_disabled_foreground_active_color: [u8;4],
    pub maximize_idle_background_active_color: [u8;4],
    pub minimize_idle_background_active_color: [u8;4],
    pub close_idle_background_active_color: [u8;4],
    pub maximize_hovered_background_active_color: [u8;4],
    pub minimize_hovered_background_active_color: [u8;4],
    pub close_hovered_background_active_color: [u8;4],
    pub maximize_disabled_background_active_color: [u8;4],
    pub minimize_disabled_background_active_color: [u8;4],
    pub close_disabled_background_active_color: [u8;4],
    pub title_bar_font: AzString,
    pub title_bar_font_size: f32,
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
    pub system_hidpi_factor: f32,
    /// Actual DPI value (default: 96)
    pub dpi: u32,
    /// Minimum dimensions of the window
    pub min_dimensions: OptionLogicalSize,
    /// Maximum dimensions of the window
    pub max_dimensions: OptionLogicalSize,
}

impl WindowSize {

    pub fn get_layout_size(&self) -> LayoutSize {
        LayoutSize::new(
            libm::roundf(self.dimensions.width) as isize,
            libm::roundf(self.dimensions.height) as isize,
        )
    }

    /// Get the actual logical size
    pub fn get_logical_size(&self) -> LogicalSize {
        self.dimensions
    }

    pub fn get_physical_size(&self) -> PhysicalSize<u32> {
        self.dimensions.to_physical(self.hidpi_factor)
    }

    /// Get a size that is usually smaller than the logical one,
    /// so that the winit DPI factor is compensated for.
    pub fn get_reverse_logical_size(&self) -> LogicalSize {
        LogicalSize::new(
            self.dimensions.width * self.hidpi_factor / self.system_hidpi_factor,
            self.dimensions.height * self.hidpi_factor / self.system_hidpi_factor,
        )
    }
}

impl Default for WindowSize {
    fn default() -> Self {
        Self {
            #[cfg(not(feature = "glow"))]
            dimensions: LogicalSize::new(640.0, 480.0),
            hidpi_factor: 1.0,
            system_hidpi_factor: 1.0,
            dpi: 96,
            min_dimensions: None.into(),
            max_dimensions: None.into(),
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        FullWindowState::default().into()
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct WindowCreateOptions {
    // Initial window state
    pub state: WindowState,
    /// If set, the first UI redraw will be called with a size of (0, 0) and the
    /// window size depends on the size of the overflowing UI. This is good for
    /// windows that do not want to take up unnecessary extra space
    pub size_to_content: bool,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer: OptionRendererOptions,
    /// Override the default window theme (set to `None` to use the OS-provided theme)
    pub theme: OptionWindowTheme,
    /// Optional callback to run when the window has been created (runs only once on startup)
    pub create_callback: OptionCallback,
    /// If set to true, will hot-reload the UI every 200ms, useful in combination with `StyledDom::from_file()`
    /// to hot-reload the UI from a file while developing.
    pub hot_reload: bool,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            size_to_content: false,
            renderer: OptionRendererOptions::None,
            theme: OptionWindowTheme::None,
            create_callback: OptionCallback::None,
            hot_reload: false,
        }
    }
}

impl WindowCreateOptions {
    pub fn new(callback: LayoutCallbackType) -> Self {
        Self {
            state: WindowState::new(callback),
            .. WindowCreateOptions::default()
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum RendererType {
    /// Force hardware rendering
    Hardware,
    /// Force software rendering
    Software,
}

impl_option!(RendererType, OptionRendererType, [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]);

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum UpdateFocusWarning {
    FocusInvalidDomId(DomId),
    FocusInvalidNodeId(AzNodeId),
    CouldNotFindFocusNode(CssPath),
}

impl ::core::fmt::Display for UpdateFocusWarning {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use self::UpdateFocusWarning::*;
        match self {
            FocusInvalidDomId(dom_id) => write!(f, "Focusing on DOM with invalid ID: {:?}", dom_id),
            FocusInvalidNodeId(node_id) => write!(f, "Focusing on node with invalid ID: {}", node_id),
            CouldNotFindFocusNode(css_path) => write!(f, "Could not find focus node for path: {}", css_path),
        }
    }
}

#[derive(Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct LogicalRect {
    pub origin: LogicalPosition,
    pub size: LogicalSize,
}

impl core::fmt::Debug for LogicalRect {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl core::fmt::Display for LogicalRect {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} @ {}", self.size, self.origin)
    }
}

impl LogicalRect {

    pub const fn zero() -> Self { Self::new(LogicalPosition::zero(), LogicalSize::zero()) }
    pub const fn new(origin: LogicalPosition, size: LogicalSize) -> Self {
        Self { origin, size }
    }

    #[inline(always)]
    pub fn max_x(&self) -> f32 { self.origin.x + self.size.width }
    #[inline(always)]
    pub fn min_x(&self) -> f32 { self.origin.x }
    #[inline(always)]
    pub fn max_y(&self) -> f32 { self.origin.y + self.size.height }
    #[inline(always)]
    pub fn min_y(&self) -> f32 { self.origin.y }

    /// Faster union for a Vec<LayoutRect>
    #[inline]
    pub fn union<I: Iterator<Item=Self>>(mut rects: I) -> Option<Self> {
        let first = rects.next()?;

        let mut max_width = first.size.width;
        let mut max_height = first.size.height;
        let mut min_x = first.origin.x;
        let mut min_y = first.origin.y;

        while let Some(Self { origin: LogicalPosition { x, y }, size: LogicalSize { width, height } }) = rects.next() {
            let cur_lower_right_x = x + width;
            let cur_lower_right_y = y + height;
            max_width = max_width.max(cur_lower_right_x - min_x);
            max_height = max_height.max(cur_lower_right_y - min_y);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }

        Some(Self {
            origin: LogicalPosition { x: min_x, y: min_y },
            size: LogicalSize { width: max_width, height: max_height },
        })
    }

    /// Same as `contains()`, but returns the (x, y) offset of the hit point
    ///
    /// On a regular computer this function takes ~3.2ns to run
    #[inline]
    pub fn hit_test(&self, other: &LogicalPosition) -> Option<LogicalPosition> {
        let dx_left_edge = other.x - self.min_x();
        let dx_right_edge = self.max_x() - other.x;
        let dy_top_edge = other.y - self.min_y();
        let dy_bottom_edge = self.max_y() - other.y;
        if dx_left_edge > 0.0 &&
           dx_right_edge > 0.0 &&
           dy_top_edge > 0.0 &&
           dy_bottom_edge > 0.0
        {
            Some(LogicalPosition::new(dx_left_edge, dy_top_edge))
        } else {
            None
        }
    }

    pub fn to_layout_rect(&self) -> LayoutRect {
        LayoutRect {
            origin: LayoutPoint::new(
                libm::roundf(self.origin.x) as isize,
                libm::roundf(self.origin.y) as isize,
            ),
            size: LayoutSize::new(
                libm::roundf(self.size.width) as isize,
                libm::roundf(self.size.height) as isize,
            )
        }
    }
}

use core::ops::SubAssign;
use core::ops::AddAssign;

#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

impl SubAssign<LogicalPosition> for LogicalPosition {
    fn sub_assign(&mut self, other: LogicalPosition) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

impl AddAssign<LogicalPosition> for LogicalPosition {
    fn add_assign(&mut self, other: LogicalPosition) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl core::fmt::Debug for LogicalPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl core::fmt::Display for LogicalPosition {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl ops::Add for LogicalPosition {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl ops::Sub for LogicalPosition {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
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

#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl core::fmt::Debug for LogicalSize {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl core::fmt::Display for LogicalSize {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
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

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct PhysicalPosition<T> {
    pub x: T,
    pub y: T,
}

impl<T: ::core::fmt::Display> ::core::fmt::Debug for PhysicalPosition<T> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

pub type PhysicalPositionI32 = PhysicalPosition<i32>;
impl_option!(PhysicalPositionI32, OptionPhysicalPositionI32, [Debug, Copy, Clone, PartialEq, PartialOrd]);

#[derive(Ord, Hash, Eq, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PhysicalSize<T> {
    pub width: T,
    pub height: T,
}

impl<T: ::core::fmt::Display> ::core::fmt::Debug for PhysicalSize<T> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
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
    NumpadAdd,
    NumpadDivide,
    NumpadDecimal,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    NumpadMultiply,
    NumpadSubtract,
    AbntC1,
    AbntC2,
    Apostrophe,
    Apps,
    Asterisk,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
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
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    OEM102,
    Period,
    PlayPause,
    Plus,
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

/// Menu struct (context menu, dropdown menu, context menu)
///
/// Modeled after the Windows API
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct Menu {
    pub items: MenuItemVec,
    pub position: MenuPopupPosition,
}

/// Position of where the menu should popup on the screen
///
/// Ignored for application-level menus
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuPopupPosition {
    // relative to cursor
    BottomLeftOfCursor,
    BottomRightOfCursor,
    TopLeftOfCursor,
    TopRightOfCursor,

    // relative to the rect that was clicked on
    BottomOfHitRect,
    LeftOfHitRect,
    TopOfHitRect,
    RightOfHitRect,

    // calculate the position based on how much space
    // is available for the context menu to either side
    // of the screen
    AutoCursor,
    AutoHitRect,
}

impl Menu {
    pub fn get_hash(&self) -> u64 {
        use ahash::AHasher as HashAlgorithm;
        use core::hash::{Hash, Hasher};

        let mut hasher = HashAlgorithm::default();
        self.hash(&mut hasher);

        hasher.finish()
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuItem {
    /// Regular menu item
    String(StringMenuItem),
    /// Separator line, only rendered when the direction is vertical
    Separator,
    /// Breaks the menu item into separate lines if laid out horizontally
    BreakLine,
}

impl_vec!(MenuItem, MenuItemVec, MenuItemVecDestructor);
impl_vec_clone!(MenuItem, MenuItemVec, MenuItemVecDestructor);
impl_vec_debug!(MenuItem, MenuItemVec);
impl_vec_partialeq!(MenuItem, MenuItemVec);
impl_vec_partialord!(MenuItem, MenuItemVec);
impl_vec_hash!(MenuItem, MenuItemVec);
impl_vec_eq!(MenuItem, MenuItemVec);
impl_vec_ord!(MenuItem, MenuItemVec);

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct StringMenuItem {
    /// Label of the menu
    pub label: AzString,
    /// Optional accelerator combination
    /// (ex. "CTRL + X" = [VirtualKeyCode::Ctrl, VirtualKeyCode::X]) for keyboard shortcut
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional callback to call
    pub callback: OptionMenuCallback,
    /// State (normal, greyed, disabled)
    pub state: MenuItemState,
    /// Optional icon for the menu entry
    pub icon: OptionMenuItemIcon,
    /// Sub-menus of this item (separators and line-breaks can't have sub-menus)
    pub children: MenuItemVec,
}

impl StringMenuItem {
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self {
            label: AzString::from_const_str(""),
            accelerator: None.into(),
            callback: None.into(),
            state: MenuItemState::Normal,
            icon: None.into(),
            children: Vec::new().into(),
        };
        core::mem::swap(&mut default, self);
        default
    }
}
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct VirtualKeyCodeCombo {
    pub keys: VirtualKeyCodeVec,
}

impl_option!(VirtualKeyCodeCombo, OptionVirtualKeyCodeCombo, copy = false, [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]);

/// Menu callback: What data / function pointer should
/// be called when the menu item is clicked?
#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct MenuCallback {
    pub callback: Callback,
    pub data: RefAny,
}

impl_option!(MenuCallback, OptionMenuCallback, copy = false, [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]);

#[derive(Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuItemIcon {
    /// Menu item shows a checkbox (either checked or not)
    Checkbox(bool),
    /// Menu item shows a custom image, usually in 16x16 format
    Image(ImageRef),
}

impl_option!(MenuItemIcon, OptionMenuItemIcon, copy = false, [Debug, Clone, PartialEq, PartialOrd, Hash, Eq, Ord]);

/// Describes the state of the menu
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub enum MenuItemState {
    /// Normal menu item (default)
    Normal,
    /// Menu item is greyed out and clicking it does nothing
    Greyed,
    /// Menu item is disabled, but NOT greyed out
    Disabled,
}