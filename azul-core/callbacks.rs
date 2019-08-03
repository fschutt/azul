use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    collections::BTreeMap,
    rc::Rc,
};
use azul_css::{LayoutPoint, LayoutRect, CssPath};
#[cfg(feature = "css_parser")]
use azul_css_parser::CssPathParseError;
use {
    FastHashMap,
    app_resources::{Words, WordPositions, ScaledWords, LayoutedGlyphs},
    async::TerminateTimer,
    dom::{Dom, DomId, NodeType, NodeData},
    display_list::CachedDisplayList,
    ui_state::UiState,
    ui_solver::{PositionedRectangle, LayoutedRectangle, ScrolledNodes, LayoutResult},
    id_tree::{NodeId, Node, NodeHierarchy},
    app_resources::AppResources,
    window::{WindowSize, WindowState, FullWindowState, KeyboardState, MouseState, LogicalSize, PhysicalSize},
    async::{Timer, Task, TimerId},
    gl::Texture,
};

pub use stack_checked_pointer::StackCheckedPointer;
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

static LAST_DEFAULT_CALLBACK_ID: AtomicUsize = AtomicUsize::new(0);

/// Each default callback is identified by its ID (not by it's function pointer),
/// since multiple IDs could point to the same function.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DefaultCallbackId { id: usize }

impl DefaultCallbackId {
    pub fn new() -> Self {
        DefaultCallbackId { id: LAST_DEFAULT_CALLBACK_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

/// A tag that can be used to identify items during hit testing. If the tag
/// is missing then the item doesn't take part in hit testing at all. This
/// is composed of two numbers. In Servo, the first is an identifier while the
/// second is used to select the cursor that should be used during mouse
/// movement. In Gecko, the first is a scrollframe identifier, while the second
/// is used to store various flags that APZ needs to properly process input
/// events.
pub type ItemTag = (u64, u16);

/// This type carries no valuable semantics for WR. However, it reflects the fact that
/// clients (Servo) may generate pipelines by different semi-independent sources.
/// These pipelines still belong to the same `IdNamespace` and the same `DocumentId`.
/// Having this extra Id field enables them to generate `PipelineId` without collision.
pub type PipelineSourceId = u32;

/// Callback function pointer (has to be a function pointer in
/// order to be compatible with C APIs later on).
pub type LayoutCallback<T> = fn(&T, layout_info: LayoutInfo<T>) -> Dom<T>;

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
    pub tag: ItemTag,
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

macro_rules! impl_get_gl_context {() => {
    /// Returns a reference-counted pointer to the OpenGL context
    pub fn get_gl_context(&self) -> Rc<Gl> {
        self.gl_context.clone()
    }

    /// Adds a default callback to the window. The default callbacks are
    /// cleared after every frame, so two-way data binding widgets have to call this
    /// on every frame they want to insert a default callback.
    ///
    /// Returns an ID by which the callback can be uniquely identified (used for hit-testing)
    #[must_use]
    pub fn add_default_callback(&mut self, callback_fn: DefaultCallbackTypeUnchecked<T>, callback_ptr: StackCheckedPointer<T>) -> DefaultCallbackId {
        let default_callback_id = DefaultCallbackId::new();
        self.default_callbacks.insert(default_callback_id, (callback_ptr, DefaultCallback(callback_fn)));
        default_callback_id
    }
};}

/// Implements functions for `CallbackInfo`, `DefaultCallbackInfoUnchecked` and `DefaultCallbackInfo`,
/// to prevent duplicating the functions
#[macro_export]
macro_rules! impl_task_api {() => (
    /// Insert a timer into the list of active timers.
    /// Replaces the existing timer if called with the same TimerId.
    pub fn add_timer(&mut self, id: TimerId, timer: Timer<T>) {
        self.timers.insert(id, timer);
    }

    /// Returns if a timer with the given ID is currently running
    pub fn has_timer(&self, timer_id: &TimerId) -> bool {
        self.get_timer(timer_id).is_some()
    }

    /// Returns a reference to an existing timer (if the `TimerId` is valid)
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer<T>> {
        self.timers.get(&timer_id)
    }

    /// Deletes a timer and returns it (if the `TimerId` is valid)
    pub fn delete_timer(&mut self, timer_id: &TimerId) -> Option<Timer<T>> {
        self.timers.remove(timer_id)
    }

    /// Adds a (thread-safe) `Task` to the app that runs on a different thread
    pub fn add_task(&mut self, task: Task<T>) {
        self.tasks.push(task);
    }
)}

/// Implements functions for `CallbackInfo`, `DefaultCallbackInfoUnchecked` and `DefaultCallbackInfo`,
/// to prevent duplicating the functions
macro_rules! impl_callback_info_api {() => (

    pub fn window_state(&self) -> &FullWindowState {
        self.current_window_state
    }

    pub fn window_state_mut(&mut self) -> &mut WindowState {
        self.modifiable_window_state
    }

    pub fn get_keyboard_state(&self) -> &KeyboardState {
        self.window_state().get_keyboard_state()
    }

    pub fn get_mouse_state(&self) -> &MouseState {
        self.window_state().get_mouse_state()
    }

    /// Returns the bounds (width / height / position / margins / border) for any given NodeId,
    /// useful for calculating scroll positions / offsets
    pub fn get_bounds(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&PositionedRectangle> {
        self.layout_result.get(&dom_id)?.rects.get(*node_id)
    }

    /// If the node is a text node, return the text of the node
    pub fn get_words(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&Words> {
        self.layout_result.get(&dom_id)?.word_cache.get(&node_id)
    }

    /// If the node is a text node, return the shaped glyphs (on a per-word basis, unpositioned)
    pub fn get_scaled_words(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&ScaledWords> {
        self.layout_result.get(&dom_id).as_ref().and_then(|lr| lr.scaled_words.get(&node_id).as_ref().map(|sw| &sw.0))
    }

    /// If the node is a text node, return the shaped glyphs (on a per-word basis, unpositioned)
    pub fn get_word_positions(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&WordPositions> {
        self.layout_result.get(&dom_id).as_ref().and_then(|lr| lr.positioned_word_cache.get(&node_id).as_ref().map(|sw| &sw.0))
    }

    pub fn get_layouted_glyphs(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&LayoutedGlyphs> {
        self.layout_result.get(&dom_id)?.layouted_glyph_cache.get(&node_id)
    }

    /// Returns information about the current scroll position of a node, such as the
    /// size of the scroll frame, the position of the scroll in the parent (how far the node has been scrolled),
    /// as well as the size of the parent node (so that things like "scroll to left edge", etc. are easy to calculate).
    pub fn get_current_scroll_position(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<ScrollPosition> {
        self.current_scroll_states.get(&dom_id)?.get(node_id).cloned()
    }

    /// For any node ID, returns what the position in its parent it is, plus the parent itself.
    /// Returns `None` on the root ID (because the root has no parent, therefore it's the 1st item)
    ///
    /// Note: Index is 0-based (first item has the index of 0)
    pub fn get_index_in_parent(&self, node_id: &(DomId, NodeId)) -> Option<(usize, (DomId, NodeId))> {
        let node_layout = &self.ui_state[&node_id.0].dom.arena.node_layout;

        if node_id.1.index() > node_layout.len() {
            return None; // node_id out of range
        }

        let parent_node = self.get_parent_node_id(node_id)?;
        Some((node_layout.get_index_in_parent(node_id.1), parent_node))
    }

    // Functions that are may be called from the user callback
    // - the `CallbackInfo` contains a `&mut UiState`, which can be
    // used to query DOM information when the callbacks are run

    /// Returns the hierarchy of the given node ID
    pub fn get_node(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&Node> {
        self.ui_state[dom_id].dom.arena.node_layout.internal.get(node_id.index())
    }

    /// Returns the parent of the given `NodeId` or None if the target is the root node.
    pub fn get_parent_node_id(&self, node_id: &(DomId, NodeId)) -> Option<(DomId, NodeId)> {
        let new_node_id = self.get_node(node_id)?.parent?;
        Some((node_id.0.clone(), new_node_id))
    }

    /// Returns the node hierarchy (DOM tree order)
    pub fn get_node_hierarchy(&self) -> &NodeHierarchy {
        &self.ui_state[&self.hit_dom_node.0].dom.arena.node_layout
    }

    /// Returns the node content of a specific node
    pub fn get_node_content(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&NodeData<T>> {
        self.ui_state[dom_id].dom.arena.node_data.internal.get(node_id.index())
    }

    /// Returns the index of the target NodeId (the target that received the event)
    /// in the targets parent or None if the target is the root node
    pub fn target_index_in_parent(&self) -> Option<usize> {
        let (index, _) = self.get_index_in_parent(&self.hit_dom_node)?;
        Some(index)
    }

    /// Returns the parent of the current target or None if the target is the root node.
    pub fn target_parent_node_id(&self) -> Option<(DomId, NodeId)> {
        self.get_parent_node_id(&self.hit_dom_node)
    }

    /// Checks whether the target of the CallbackInfo has a certain node type
    pub fn target_is_node_type(&self, node_type: NodeType<T>) -> bool {
        if let Some(self_node) = self.get_node_content(&self.hit_dom_node) {
            self_node.is_node_type(node_type)
        } else {
            false
        }
    }

    /// Checks whether the target of the CallbackInfo has a certain ID
    pub fn target_has_id(&self, id: &str) -> bool {
        if let Some(self_node) = self.get_node_content(&self.hit_dom_node) {
            self_node.has_id(id)
        } else {
            false
        }
    }

    /// Checks whether the target of the CallbackInfo has a certain class
    pub fn target_has_class(&self, class: &str) -> bool {
        if let Some(self_node) = self.get_node_content(&self.hit_dom_node) {
            self_node.has_class(class)
        } else {
            false
        }
    }

    /// Traverses up the hierarchy, checks whether any parent has a certain ID,
    /// the returns that parent
    pub fn any_parent_has_id(&self, id: &str) -> Option<(DomId, NodeId)> {
        self.parent_nodes().find(|parent_id| {
            if let Some(self_node) = self.get_node_content(parent_id) {
                self_node.has_id(id)
            } else {
                false
            }
        })
    }

    /// Traverses up the hierarchy, checks whether any parent has a certain class
    pub fn any_parent_has_class(&self, class: &str) -> Option<(DomId, NodeId)> {
        self.parent_nodes().find(|parent_id| {
            if let Some(self_node) = self.get_node_content(parent_id) {
                self_node.has_class(class)
            } else {
                false
            }
        })
    }

    /// Scrolls a node to a certain position
    pub fn scroll_node(&mut self, (dom_id, node_id): &(DomId, NodeId), scroll_location: LayoutPoint) {
        self.nodes_scrolled_in_callback
            .entry(dom_id.clone())
            .or_insert_with(|| BTreeMap::default())
            .insert(*node_id, scroll_location);
    }

    /// Scrolls a node to a certain position
    pub fn scroll_target(&mut self, scroll_location: LayoutPoint) {
        let target = self.hit_dom_node.clone(); // borrowing issue
        self.scroll_node(&target, scroll_location);
    }

    /// Set the focus_target to a certain div by parsing a string.
    /// Note that the parsing of the string can fail, therefore the Result
    #[cfg(feature = "css_parser")]
    pub fn set_focus_from_css<'c>(&mut self, input: &'c str) -> Result<(), CssPathParseError<'c>> {
        use azul_css_parser::parse_css_path;
        let path = parse_css_path(input)?;
        *self.focus_target = Some(FocusTarget::Path(path));
        Ok(())
    }

    /// Creates an iterator that starts at the current DOM node and continouusly
    /// returns the parent `(DomId, NodeId)`, until the iterator gets to the root DOM node.
    pub fn parent_nodes<'c>(&'c self) -> ParentNodesIterator<'c, T> {
        ParentNodesIterator {
            ui_state: &self.ui_state,
            current_item: self.hit_dom_node.clone(),
        }
    }

    /// Sets the focus_target by using an already-parsed `CssPath`.
    pub fn set_focus_from_path(&mut self, path: CssPath) {
        *self.focus_target = Some(FocusTarget::Path(path))
    }

    /// Set the focus_target of the window to a specific div using a `NodeId`.
    ///
    /// Note that this ID will be dependent on the position in the DOM and therefore
    /// the next frames UI must be the exact same as the current one, otherwise
    /// the focus_target will be cleared or shifted (depending on apps setting).
    pub fn set_focus_from_node_id(&mut self, id: (DomId, NodeId)) {
        *self.focus_target = Some(FocusTarget::Id(id));
    }

    /// Clears the focus_target for the next frame.
    pub fn clear_focus(&mut self) {
        *self.focus_target = Some(FocusTarget::NoFocus);
    }
)}

// -- default callback

pub struct DefaultCallbackInfoUnchecked<'a, T> {
    /// Type-erased pointer to a unknown type on the stack (inside of `T`),
    /// pointer has to be casted to a `U` type first (via `.invoke_callback()`)
    pub ptr: StackCheckedPointer<T>,
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
    /// The user can push default callbacks in this `DefaultCallbackSystem`,
    /// which get called later in the hit-testing logic
    pub default_callbacks: &'a mut BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub gl_context: Rc<Gl>,
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
    /// What items are currently being hit
    pub hit_test_items: &'a [HitTestItem],
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<(f32, f32)>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<(f32, f32)>,
}

pub struct DefaultCallbackInfo<'a, T, U> {
    pub data: &'a mut U,
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
    /// The user can push default callbacks in this `DefaultCallbackSystem`,
    /// which get called later in the hit-testing logic
    pub default_callbacks: &'a mut BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub gl_context: Rc<Gl>,
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
    /// What items are currently being hit
    pub hit_test_items: &'a [HitTestItem],
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<(f32, f32)>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<(f32, f32)>,
}

/// Callback that is invoked "by default", for example a text field that always
/// has a default "ontextinput" handler
pub struct DefaultCallback<T>(pub DefaultCallbackTypeUnchecked<T>);
impl_callback!(DefaultCallback<T>);
pub type DefaultCallbackTypeUnchecked<T> = fn(DefaultCallbackInfoUnchecked<T>) -> CallbackReturn;
pub type DefaultCallbackType<T, U> = fn(DefaultCallbackInfo<T, U>) -> CallbackReturn;

impl<'a, T> DefaultCallbackInfoUnchecked<'a, T> {
    pub unsafe fn invoke_callback<U: Sized + 'static>(self, callback: DefaultCallbackType<T, U>) -> CallbackReturn {
        let casted_value: &mut U = self.ptr.cast();
        let casted_callback_info = DefaultCallbackInfo {
            data: casted_value,
            current_window_state: self.current_window_state,
            modifiable_window_state: self.modifiable_window_state,
            layout_result: self.layout_result,
            scrolled_nodes: self.scrolled_nodes,
            cached_display_list: self.cached_display_list,
            default_callbacks: self.default_callbacks,
            gl_context: self.gl_context,
            resources: self.resources,
            timers: self.timers,
            tasks: self.tasks,
            ui_state: self.ui_state,
            focus_target: self.focus_target,
            current_scroll_states: self.current_scroll_states,
            nodes_scrolled_in_callback: self.nodes_scrolled_in_callback,
            hit_dom_node: self.hit_dom_node,
            hit_test_items: self.hit_test_items,
            cursor_relative_to_item: self.cursor_relative_to_item,
            cursor_in_viewport: self.cursor_in_viewport,
        };
        callback(casted_callback_info)
    }
}



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
    pub data: &'a mut T,
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
    /// The user can push default callbacks in this `DefaultCallbackSystem`,
    /// which get called later in the hit-testing logic
    pub default_callbacks: &'a mut BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    /// but not change any window properties from underneath - this would
    /// lead to mismatch between the
    pub gl_context: Rc<Gl>,
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
    /// What items are currently being hit
    pub hit_test_items: &'a [HitTestItem],
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
            default_callbacks: {:?}, \
            gl_context: {{ .. }}, \
            resources: {{ .. }}, \
            timers: {{ .. }}, \
            tasks: {{ .. }}, \
            ui_state: {:?}, \
            focus_target: {:?}, \
            current_scroll_states: {:?}, \
            nodes_scrolled_in_callback: {:?}, \
            hit_dom_node: {:?}, \
            hit_test_items: {:?}, \
            cursor_relative_to_item: {:?}, \
            cursor_in_viewport: {:?}, \
        }}",
            self.current_window_state,
            self.modifiable_window_state,
            self.layout_result,
            self.scrolled_nodes,
            self.cached_display_list,
            self.default_callbacks,
            self.ui_state,
            self.focus_target,
            self.current_scroll_states,
            self.nodes_scrolled_in_callback,
            self.hit_dom_node,
            self.hit_test_items,
            self.cursor_relative_to_item,
            self.cursor_in_viewport,
        )
    }
}

// -- opengl callback

/// Callbacks that returns a rendered OpenGL texture
pub struct GlCallback<T>(pub GlCallbackTypeUnchecked<T>);
impl_callback!(GlCallback<T>);
pub struct GlCallbackInfoUnchecked<'a, T: 'a> {
    pub ptr: StackCheckedPointer<T>,
    pub layout_info: LayoutInfo<'a, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub struct GlCallbackInfo<'a, T: 'a, U: Sized> {
    pub state: &'a mut U,
    pub layout_info: LayoutInfo<'a, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub type GlCallbackReturn = Option<Texture>;
pub type GlCallbackTypeUnchecked<T> = fn(GlCallbackInfoUnchecked<T>) -> GlCallbackReturn;
pub type GlCallbackType<T, U> = fn(GlCallbackInfo<T, U>) -> GlCallbackReturn;

impl<'a, T: 'a> GlCallbackInfoUnchecked<'a, T> {
    pub unsafe fn invoke_callback<U: Sized + 'static>(self, callback: GlCallbackType<T, U>) -> GlCallbackReturn {
        let casted_value: &mut U = self.ptr.cast();
        let casted_callback_info = GlCallbackInfo {
            state: casted_value,
            layout_info: self.layout_info,
            bounds: self.bounds,
        };
        callback(casted_callback_info)
    }
}

// -- iframe callback

/// Callback that, given a rectangle area on the screen, returns the DOM appropriate for that bounds (useful for infinite lists)
pub struct IFrameCallback<T>(pub IFrameCallbackTypeUnchecked<T>);
impl_callback!(IFrameCallback<T>);
pub struct IFrameCallbackInfoUnchecked<'a, T: 'a> {
    pub ptr: StackCheckedPointer<T>,
    pub layout_info: LayoutInfo<'a, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub struct IFrameCallbackInfo<'a, T: 'a, U: Sized> {
    pub state: &'a mut U,
    pub layout_info: LayoutInfo<'a, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub type IFrameCallbackReturn<T> = Option<Dom<T>>; // todo: return virtual scrolling frames!
pub type IFrameCallbackTypeUnchecked<T> = fn(IFrameCallbackInfoUnchecked<T>) -> IFrameCallbackReturn<T>;
pub type IFrameCallbackType<T, U> = fn(IFrameCallbackInfo<T, U>) -> IFrameCallbackReturn<T>;

impl<'a, T: 'a> IFrameCallbackInfoUnchecked<'a, T> {
    pub unsafe fn invoke_callback<U: Sized + 'static>(self, callback: IFrameCallbackType<T, U>) -> IFrameCallbackReturn<T> {
        let casted_value: &mut U = self.ptr.cast();
        let casted_callback_info = IFrameCallbackInfo {
            state: casted_value,
            layout_info: self.layout_info,
            bounds: self.bounds,
        };
        callback(casted_callback_info)
    }
}

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
pub struct LayoutInfo<'a, T> {
    /// Window size (so that apps can return a different UI depending on
    /// the window size - mobile / desktop view). Should be later removed
    /// in favor of "resize" handlers and @media queries.
    window_size: &'a WindowSize,
    /// Optimization for resizing: If a DOM has no Iframes and the window size
    /// does not change the state of the UI, then resizing the window will not
    /// result in calls to the .layout() function (since the resulting UI would
    /// stay the same).
    ///
    /// Stores "stops" in logical pixels where the UI needs to be re-generated
    /// should the width of the window change.
    window_size_width_stops: &'a mut Vec<f32>,
    /// Same as `window_size_width_stops` but for the height of the window.
    window_size_height_stops: &'a mut Vec<f32>,
    /// The user can push default callbacks in this `DefaultCallbackSystem`,
    /// which get called later in the hit-testing logic
    pub default_callbacks: &'a mut BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
    /// An Rc to the original WindowContext - this is only so that
    /// the user can create textures and other OpenGL content in the window
    pub gl_context: Rc<Gl>,
    /// Allows the layout() function to reference app resources such as FontIDs or ImageIDs
    pub resources: &'a AppResources,
}

impl<'a, T> LayoutInfo<'a, T> {
    impl_get_gl_context!();
}

impl<'a, T> LayoutInfo<'a, T> {

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
    Path(CssPath),
    NoFocus,
}

impl<'a, T: 'a> CallbackInfo<'a, T> {
    impl_callback_info_api!();
    impl_task_api!();
    impl_get_gl_context!();
}

impl<'a, T> DefaultCallbackInfoUnchecked<'a, T> {
    impl_callback_info_api!();
    impl_task_api!();
    impl_get_gl_context!();
}

impl<'a, T, U> DefaultCallbackInfo<'a, T, U> {
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