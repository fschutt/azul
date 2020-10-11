use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    collections::BTreeMap,
    rc::Rc,
    any::Any,
    hash::Hash,
    cell::{Ref as StdRef, RefMut as StdRefMut, RefCell},
};
use azul_css::{LayoutPoint, LayoutRect, CssPath};
#[cfg(feature = "css_parser")]
use azul_css_parser::CssPathParseError;
use crate::{
    FastHashMap,
    app_resources::{Words, WordPositions, ScaledWords, LayoutedGlyphs},
    dom::{Dom, DomId, TagId, NodeType, NodeData},
    display_list::CachedDisplayList,
    ui_state::UiState,
    ui_description::UiDescription,
    ui_solver::{PositionedRectangle, LayoutedRectangle, ScrolledNodes, LayoutResult},
    id_tree::{NodeId, Node, NodeHierarchy},
    app_resources::AppResources,
    window::{
        WindowSize, WindowState, FullWindowState,
        KeyboardState, MouseState, LogicalSize, PhysicalSize,
        UpdateFocusWarning,
    },
    task::{Timer, TerminateTimer, Task, TimerId},
    gl::Texture,
};

pub use gleam::gl::Gl;

/// A callback function has to return if the screen should be updated after the
/// function has run.
///
/// NOTE: This is currently a typedef for `Option<()>`, so that you can use
/// the `?` operator in callbacks (to simply not redraw if there is an error).
/// This was an enum previously, but since Rust doesn't have a "custom try" operator,
/// this led to a lot of usability problems. In the future, this might change back
/// to an enum therefore the constants "Redraw" and "DontRedraw" are not capitalized,
/// to minimize breakage.
pub type UpdateScreen = Option<()>;
/// After the callback is called, the screen needs to redraw
/// (layout() function being called again).
#[allow(non_upper_case_globals)]
pub const Redraw: Option<()> = Some(());
/// The screen does not need to redraw after the callback has been called.
#[allow(non_upper_case_globals)]
pub const DontRedraw: Option<()> = None;

#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ref<T: 'static>(Rc<RefCell<T>>);

impl<T: 'static> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Ref(self.0.clone())
    }
}

impl<T: 'static + Hash> Hash for Ref<T> {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        let self_ptr = Rc::into_raw(self.0.clone()) as *const c_void as usize;
        state.write_usize(self_ptr);
        self.0.borrow().hash(state)
    }
}

impl<T: 'static> Ref<T> {

    pub fn new(data: T) -> Self {
        Ref(Rc::new(RefCell::new(data)))
    }

    pub fn borrow(&self) -> StdRef<T> {
        self.0.borrow()
    }

    pub fn borrow_mut(&mut self) -> StdRefMut<T> {
        self.0.borrow_mut()
    }

    pub fn upcast(self) -> RefAny {
        RefAny(self.0 as Rc<dyn Any>)
    }
}

impl<T: 'static> From<Ref<T>> for RefAny {
    fn from(r: Ref<T>) -> Self {
        r.upcast()
    }
}

#[derive(Debug)]
pub struct RefAny(Rc<dyn Any>);

impl Clone for RefAny {
    fn clone(&self) -> Self {
        RefAny(self.0.clone())
    }
}

use std::ffi::c_void;

impl ::std::hash::Hash for RefAny {
    fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
        let self_ptr = Rc::into_raw(self.0.clone()) as *const c_void as usize;
        state.write_usize(self_ptr);
    }
}

impl PartialEq for RefAny {
    fn eq(&self, rhs: &Self) -> bool {
        Rc::ptr_eq(&self.0, &rhs.0)
    }
}

impl PartialOrd for RefAny {
    fn partial_cmp(&self, rhs: &Self) -> Option<::std::cmp::Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for RefAny {
    fn cmp(&self, rhs: &Self) -> ::std::cmp::Ordering {
        let self_ptr = Rc::into_raw(self.0.clone()) as *const c_void as usize;
        let rhs_ptr = Rc::into_raw(rhs.0.clone()) as *const c_void as usize;
        self_ptr.cmp(&rhs_ptr)
    }
}

impl Eq for RefAny { }

impl RefAny {
    pub fn downcast<T: 'static>(&self) -> Option<&RefCell<T>> {
        self.0.downcast_ref::<RefCell<T>>()
    }
}

/// This type carries no valuable semantics for WR. However, it reflects the fact that
/// clients (Servo) may generate pipelines by different semi-independent sources.
/// These pipelines still belong to the same `IdNamespace` and the same `DocumentId`.
/// Having this extra Id field enables them to generate `PipelineId` without collision.
pub type PipelineSourceId = u32;

/// Callback function pointer (has to be a function pointer in
/// order to be compatible with C APIs later on).
pub type LayoutCallback<T> = fn(&T, layout_info: LayoutInfo) -> Dom<T>;

/// Information about a scroll frame, given to the user by the framework
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ScrollPosition {
    /// How big is the scroll rect (i.e. the union of all children)?
    pub scroll_frame_rect: LayoutRect,
    /// How big is the parent container (so that things like "scroll to left edge" can be implemented)?
    pub parent_rect: LayoutedRectangle,
    /// Where (measured from the top left corner) is the frame currently scrolled to?
    pub scroll_location: LayoutPoint,
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PipelineId(pub PipelineSourceId, pub u32);

static LAST_PIPELINE_ID: AtomicUsize = AtomicUsize::new(0);

impl PipelineId {
    pub const DUMMY: PipelineId = PipelineId(0, 0);

    pub fn new() -> Self {
        PipelineId(LAST_PIPELINE_ID.fetch_add(1, Ordering::SeqCst) as u32, 0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct HitTestItem {
    /// The pipeline that the display item that was hit belongs to.
    pub pipeline: PipelineId,
    /// The tag of the hit display item.
    pub tag: TagId,
    /// The hit point in the coordinate space of the "viewport" of the display item.
    /// The viewport is the scroll node formed by the root reference frame of the display item's pipeline.
    pub point_in_viewport: LayoutPoint,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LayoutPoint,
}

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.0` field:
///
/// ```
/// struct MyCallback<T>(fn (&T));
///
/// // impl <T> Display, Debug, etc. for MyCallback<T>
/// impl_callback!(MyCallback<T>);
/// ```
///
/// This is necessary to work around for https://github.com/rust-lang/rust/issues/54508
macro_rules! impl_callback {($callback_value:ident<$t:ident>) => (

    impl<$t> ::std::fmt::Display for $callback_value<$t> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    impl<$t> ::std::fmt::Debug for $callback_value<$t> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let callback = stringify!($callback_value);
            write!(f, "{} @ 0x{:x}", callback, self.0 as usize)
        }
    }

    impl<$t> Clone for $callback_value<$t> {
        fn clone(&self) -> Self {
            $callback_value(self.0.clone())
        }
    }

    impl<$t> ::std::hash::Hash for $callback_value<$t> {
        fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
            state.write_usize(self.0 as usize);
        }
    }

    impl<$t> PartialEq for $callback_value<$t> {
        fn eq(&self, rhs: &Self) -> bool {
            self.0 as usize == rhs.0 as usize
        }
    }

    impl<$t> PartialOrd for $callback_value<$t> {
        fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
            Some((self.0 as usize).cmp(&(other.0 as usize)))
        }
    }

    impl<$t> Ord for $callback_value<$t> {
        fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
            (self.0 as usize).cmp(&(other.0 as usize))
        }
    }

    impl<$t> Eq for $callback_value<$t> { }

    impl<$t> Copy for $callback_value<$t> { }
)}

macro_rules! impl_callback_no_generics {($callback_value:ident) => (

    impl ::std::fmt::Display for $callback_value {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:?}", self)
        }
    }

    impl ::std::fmt::Debug for $callback_value {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let callback = stringify!($callback_value);
            write!(f, "{} @ 0x{:x}", callback, self.0 as usize)
        }
    }

    impl Clone for $callback_value {
        fn clone(&self) -> Self {
            $callback_value(self.0.clone())
        }
    }

    impl ::std::hash::Hash for $callback_value {
        fn hash<H>(&self, state: &mut H) where H: ::std::hash::Hasher {
            state.write_usize(self.0 as usize);
        }
    }

    impl PartialEq for $callback_value {
        fn eq(&self, rhs: &Self) -> bool {
            self.0 as usize == rhs.0 as usize
        }
    }

    impl PartialOrd for $callback_value {
        fn partial_cmp(&self, other: &Self) -> Option<::std::cmp::Ordering> {
            Some((self.0 as usize).cmp(&(other.0 as usize)))
        }
    }

    impl Ord for $callback_value {
        fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
            (self.0 as usize).cmp(&(other.0 as usize))
        }
    }

    impl Eq for $callback_value { }

    impl Copy for $callback_value { }
)}

macro_rules! impl_get_gl_context {() => {
    /// Returns a reference-counted pointer to the OpenGL context
    pub fn get_gl_context(&self) -> Rc<dyn Gl> {
        self.gl_context.clone()
    }
};}

// -- default callback

pub struct DefaultCallbackInfo<'a, T> {
    /// Type-erased pointer to a unknown type on the stack (inside of `T`),
    /// pointer has to be casted to a `U` type first (via `.invoke_callback()`)
    pub state: &'a RefAny,
    /// State of the current window that the callback was called on (read only!)
    pub current_window_state: &'a FullWindowState,
    /// User-modifiable state of the window that the callback was called on
    pub modifiable_window_state: &'a mut WindowState,
    /// Currently active, layouted rectangles
    pub layout_result: &'a BTreeMap<DomId, LayoutResult>,
    /// Nodes that overflow their parents and are able to scroll
    pub scrolled_nodes: &'a BTreeMap<DomId, ScrolledNodes>,
    /// Current display list active in this window (useful for debugging)
    pub cached_display_list: &'a CachedDisplayList,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub gl_context: Rc<dyn Gl>,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    pub resources : &'a mut AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    pub timers: &'a mut FastHashMap<TimerId, Timer<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub tasks: &'a mut Vec<Task<T>>,
    /// UiState containing the necessary data for testing what
    pub ui_state: &'a BTreeMap<DomId, UiState<T>>,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus_target: &'a mut Option<FocusTarget>,
    /// Immutable (!) reference to where the nodes are currently scrolled (current position)
    pub current_scroll_states: &'a BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>,
    /// Mutable map where a user can set where he wants the nodes to be scrolled to (for the next frame)
    pub nodes_scrolled_in_callback: &'a mut BTreeMap<DomId, BTreeMap<NodeId, LayoutPoint>>,
    /// The ID of the node that was hit. You can use this to query information about
    /// the node, but please don't hard-code any if / else statements based on the `NodeId`
    pub hit_dom_node: (DomId, NodeId),
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<(f32, f32)>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<(f32, f32)>,
}

/// Callback that is invoked "by default", for example a text field that always
/// has a default "ontextinput" handler
pub struct DefaultCallback<T>(pub DefaultCallbackType<T>);
impl_callback!(DefaultCallback<T>);

pub type DefaultCallbackType<T> = fn(DefaultCallbackInfo<T>) -> CallbackReturn;

// -- normal callback

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `UpdateScreen` that denotes if the screen should be redrawn.
/// The style is not affected by this, so if you make changes to the window's style
/// inside the function, the screen will not be automatically redrawn, unless you return
/// an `UpdateScreen::Redraw` from the function
pub struct Callback<T>(pub CallbackType<T>);
impl_callback!(Callback<T>);

/// Information about the callback that is passed to the callback whenever a callback is invoked
pub struct CallbackInfo<'a, T: 'a> {
    /// Your data (the global struct which all callbacks will have access to)
    pub state: &'a mut T,
    /// State of the current window that the callback was called on (read only!)
    pub current_window_state: &'a FullWindowState,
    /// User-modifiable state of the window that the callback was called on
    pub modifiable_window_state: &'a mut WindowState,
    /// Currently active, layouted rectangles
    pub layout_result: &'a BTreeMap<DomId, LayoutResult>,
    /// Nodes that overflow their parents and are able to scroll
    pub scrolled_nodes: &'a BTreeMap<DomId, ScrolledNodes>,
    /// Current display list active in this window (useful for debugging)
    pub cached_display_list: &'a CachedDisplayList,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub gl_context: Rc<dyn Gl>,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    pub resources : &'a mut AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    pub timers: &'a mut FastHashMap<TimerId, Timer<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub tasks: &'a mut Vec<Task<T>>,
    /// UiState containing the necessary data for testing what
    pub ui_state: &'a BTreeMap<DomId, UiState<T>>,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus_target: &'a mut Option<FocusTarget>,
    /// Immutable (!) reference to where the nodes are currently scrolled (current position)
    pub current_scroll_states: &'a BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>,
    /// Mutable map where a user can set where he wants the nodes to be scrolled to (for the next frame)
    pub nodes_scrolled_in_callback: &'a mut BTreeMap<DomId, BTreeMap<NodeId, LayoutPoint>>,
    /// The ID of the DOM + the node that was hit. You can use this to query
    /// information about the node, but please don't hard-code any if / else
    /// statements based on the `NodeId`
    pub hit_dom_node: (DomId, NodeId),
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<(f32, f32)>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<(f32, f32)>,
}
pub type CallbackReturn = UpdateScreen;
pub type CallbackType<T> = fn(CallbackInfo<T>) -> CallbackReturn;

impl<'a, T: 'a> fmt::Debug for CallbackInfo<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackInfo {{
            data: {{ .. }}, \
            current_window_state: {:?}, \
            modifiable_window_state: {:?}, \
            layout_result: {:?}, \
            scrolled_nodes: {:?}, \
            cached_display_list: {:?}, \
            gl_context: {{ .. }}, \
            resources: {{ .. }}, \
            timers: {{ .. }}, \
            tasks: {{ .. }}, \
            ui_state: {:?}, \
            focus_target: {:?}, \
            current_scroll_states: {:?}, \
            nodes_scrolled_in_callback: {:?}, \
            hit_dom_node: {:?}, \
            cursor_relative_to_item: {:?}, \
            cursor_in_viewport: {:?}, \
        }}",
            self.current_window_state,
            self.modifiable_window_state,
            self.layout_result,
            self.scrolled_nodes,
            self.cached_display_list,
            self.ui_state,
            self.focus_target,
            self.current_scroll_states,
            self.nodes_scrolled_in_callback,
            self.hit_dom_node,
            self.cursor_relative_to_item,
            self.cursor_in_viewport,
        )
    }
}

// -- opengl callback

/// Callbacks that returns a rendered OpenGL texture
pub struct GlCallback(pub GlCallbackType);
impl_callback_no_generics!(GlCallback);

pub struct GlCallbackInfo<'a> {
    pub state: &'a RefAny,
    pub layout_info: LayoutInfo<'a>,
    pub bounds: HidpiAdjustedBounds,
}

pub type GlCallbackReturn = Option<Texture>;
pub type GlCallbackType = fn(GlCallbackInfo) -> GlCallbackReturn;

// -- iframe callback

/// Callback that, given a rectangle area on the screen, returns the DOM
/// appropriate for that bounds (useful for infinite lists)
pub struct IFrameCallback<T>(pub IFrameCallbackType<T>);
impl_callback!(IFrameCallback<T>);

pub struct IFrameCallbackInfo<'a> {
    pub state: &'a RefAny,
    pub layout_info: LayoutInfo<'a>,
    pub bounds: HidpiAdjustedBounds,
}

pub type IFrameCallbackReturn<T> = Option<Dom<T>>; // todo: return virtual scrolling frames!
pub type IFrameCallbackType<T> = fn(IFrameCallbackInfo) -> IFrameCallbackReturn<T>;

// -- timer callback

/// Callback that can runs on every frame on the main thread - can modify the app data model
pub struct TimerCallback<T>(pub TimerCallbackType<T>);
impl_callback!(TimerCallback<T>);
pub struct TimerCallbackInfo<'a, T> {
    pub state: &'a mut T,
    pub app_resources: &'a mut AppResources,
}
pub type TimerCallbackReturn = (UpdateScreen, TerminateTimer);
pub type TimerCallbackType<T> = fn(TimerCallbackInfo<T>) -> TimerCallbackReturn;

/// Gives the `layout()` function access to the `AppResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
pub struct LayoutInfo<'a> {
    /// Window size (so that apps can return a different UI depending on
    /// the window size - mobile / desktop view). Should be later removed
    /// in favor of "resize" handlers and @media queries.
    pub window_size: &'a WindowSize,
    /// Optimization for resizing: If a DOM has no Iframes and the window size
    /// does not change the state of the UI, then resizing the window will not
    /// result in calls to the .layout() function (since the resulting UI would
    /// stay the same).
    ///
    /// Stores "stops" in logical pixels where the UI needs to be re-generated
    /// should the width of the window change.
    pub window_size_width_stops: &'a mut Vec<f32>,
    /// Same as `window_size_width_stops` but for the height of the window.
    pub window_size_height_stops: &'a mut Vec<f32>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    pub gl_context: Rc<dyn Gl>,
    /// Allows the layout() function to reference app resources such as FontIDs or ImageIDs
    pub resources: &'a AppResources,
}

impl<'a> LayoutInfo<'a> {
    impl_get_gl_context!();
}

impl<'a> LayoutInfo<'a> {

    /// Returns whether the window width is larger than `width`,
    /// but sets an internal "dirty" flag - so that the UI is re-generated when
    /// the window is resized above or below `width`.
    ///
    /// For example:
    ///
    /// ```rust,no_run,ignore
    /// fn layout(info: LayoutInfo<T>) -> Dom<T> {
    ///     if info.window_width_larger_than(720.0) {
    ///         render_desktop_ui()
    ///     } else {
    ///         render_mobile_ui()
    ///     }
    /// }
    /// ```
    ///
    /// Here, the UI is dependent on the width of the window, so if the window
    /// resizes above or below 720px, the `layout()` function needs to be called again.
    /// Internally Azul stores the `720.0` and only calls the `.layout()` function
    /// again if the window resizes above or below the value.
    ///
    /// NOTE: This should be later depreceated into `On::Resize` handlers and
    /// `@media` queries.
    pub fn window_width_larger_than(&mut self, width: f32) -> bool {
        self.window_size_width_stops.push(width);
        self.window_size.get_logical_size().width > width
    }

    pub fn window_width_smaller_than(&mut self, width: f32) -> bool {
        self.window_size_width_stops.push(width);
        self.window_size.get_logical_size().width < width
    }

    pub fn window_height_larger_than(&mut self, height: f32) -> bool {
        self.window_size_height_stops.push(height);
        self.window_size.get_logical_size().height > height
    }

    pub fn window_height_smaller_than(&mut self, height: f32) -> bool {
        self.window_size_height_stops.push(height);
        self.window_size.get_logical_size().height < height
    }
}

/// Information about the bounds of a laid-out div rectangle.
///
/// Necessary when invoking `IFrameCallbacks` and `GlCallbacks`, so
/// that they can change what their content is based on their size.
#[derive(Debug, Copy, Clone)]
pub struct HidpiAdjustedBounds {
    pub logical_size: LogicalSize,
    pub hidpi_factor: f32,
    pub winit_hidpi_factor: f32,
    // TODO: Scroll state / focus_target state of this div!
}

impl HidpiAdjustedBounds {

    #[inline(always)]
    pub fn from_bounds(bounds: LayoutRect, hidpi_factor: f32, winit_hidpi_factor: f32) -> Self {
        let logical_size = LogicalSize::new(bounds.size.width, bounds.size.height);
        Self {
            logical_size,
            hidpi_factor,
            winit_hidpi_factor,
        }
    }

    pub fn get_physical_size(&self) -> PhysicalSize {
        self.get_logical_size().to_physical(self.winit_hidpi_factor)
    }

    pub fn get_logical_size(&self) -> LogicalSize {
        // NOTE: hidpi factor, not winit_hidpi_factor!
        LogicalSize::new(
            self.logical_size.width * self.hidpi_factor,
            self.logical_size.height * self.hidpi_factor
        )
    }

    pub fn get_hidpi_factor(&self) -> f32 {
        self.hidpi_factor
    }
}

/// Defines the focus_targeted node ID for the next frame
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FocusTarget {
    Id((DomId, NodeId)),
    Path((DomId, CssPath)),
    NoFocus,
}

impl FocusTarget {
    pub fn resolve<T>(
        &self,
        ui_descriptions: &BTreeMap<DomId, UiDescription>,
        ui_states: &BTreeMap<DomId, UiState<T>>,
    ) -> Result<Option<(DomId, NodeId)>, UpdateFocusWarning> {

        use crate::callbacks::FocusTarget::*;
        use crate::style::matches_html_element;

        match self {
            Id((dom_id, node_id)) => {
                let ui_state = ui_states.get(&dom_id).ok_or(UpdateFocusWarning::FocusInvalidDomId(dom_id.clone()))?;
                let _ = ui_state.dom.arena.node_data.get(*node_id).ok_or(UpdateFocusWarning::FocusInvalidNodeId(*node_id))?;
                Ok(Some((dom_id.clone(), *node_id)))
            },
            NoFocus => Ok(None),
            Path((dom_id, css_path)) => {
                let ui_state = ui_states.get(&dom_id).ok_or(UpdateFocusWarning::FocusInvalidDomId(dom_id.clone()))?;
                let ui_description = ui_descriptions.get(&dom_id).ok_or(UpdateFocusWarning::FocusInvalidDomId(dom_id.clone()))?;
                let html_node_tree = &ui_description.html_tree;
                let node_hierarchy = &ui_state.dom.arena.node_layout;
                let node_data = &ui_state.dom.arena.node_data;
                let resolved_node_id = html_node_tree
                    .linear_iter()
                    .find(|node_id| matches_html_element(css_path, *node_id, &node_hierarchy, &node_data, &html_node_tree))
                    .ok_or(UpdateFocusWarning::CouldNotFindFocusNode(css_path.clone()))?;
                Ok(Some((dom_id.clone(), resolved_node_id)))
            },
        }
    }
}

impl<'a, T: 'a> CallbackInfo<'a, T> {
    impl_callback_info_api!();
    impl_task_api!();
    impl_get_gl_context!();
}

impl<'a, T> DefaultCallbackInfo<'a, T> {
    impl_callback_info_api!();
    impl_task_api!();
    impl_get_gl_context!();
}

/// Iterator that, starting from a certain starting point, returns the
/// parent node until it gets to the root node.
pub struct ParentNodesIterator<'a, T: 'a> {
    ui_state: &'a BTreeMap<DomId, UiState<T>>,
    current_item: (DomId, NodeId),
}

impl<'a, T: 'a> ParentNodesIterator<'a, T> {

    /// Returns what node ID the iterator is currently processing
    pub fn current_node(&self) -> (DomId, NodeId) {
        self.current_item.clone()
    }

    /// Returns the offset into the parent of the current node or None if the item has no parent
    pub fn current_index_in_parent(&self) -> Option<usize> {
        let node_layout = &self.ui_state[&self.current_item.0].dom.arena.node_layout;
        if node_layout[self.current_item.1].parent.is_some() {
            Some(node_layout.get_index_in_parent(self.current_item.1))
        } else {
            None
        }
    }
}

impl<'a, T: 'a> Iterator for ParentNodesIterator<'a, T> {
    type Item = (DomId, NodeId);

    /// For each item in the current item path, returns the index of the item in the parent
    fn next(&mut self) -> Option<(DomId, NodeId)> {
        let parent_node_id = self.ui_state[&self.current_item.0].dom.arena.node_layout[self.current_item.1].parent?;
        self.current_item.1 = parent_node_id;
        Some((self.current_item.0.clone(), parent_node_id))
    }
}