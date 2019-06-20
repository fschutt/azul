use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    collections::BTreeMap,
};
use azul_css::{LayoutPoint, CssPath};
#[cfg(feature = "css_parser")]
use azul_css_parser::CssPathParseError;
use {
    app::{AppState, AppStateNoData},
    async::TerminateTimer,
    dom::{Dom, DomId, NodeType, NodeData},
    ui_state::UiState,
    id_tree::{NodeId, Node, NodeHierarchy},
    app_resources::AppResources,
    window::{FakeWindow, WindowId, LogicalSize, PhysicalSize},
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

pub type LayoutCallback<T> = fn(&T, layout_info: LayoutInfo<T>) -> Dom<T>;

// -- default callback

pub struct DefaultCallbackInfoUnchecked<'a, T> {
    pub ptr: StackCheckedPointer<T>,
    pub app_state_no_data: AppStateNoData<'a, T>,
    /// UiState containing the necessary data for testing what
    pub ui_state: &'a BTreeMap<DomId, UiState<T>>,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus_target: &'a mut Option<FocusTarget>,
    /// The ID of the window that the event was clicked on (for indexing into
    /// `app_state.windows`). `app_state.windows[event.window]` should never panic.
    pub window_id: &'a WindowId,
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
    pub state: &'a mut U,
    pub app_state_no_data: AppStateNoData<'a, T>,
    /// UiState containing the necessary data for testing what
    pub ui_state: &'a BTreeMap<DomId, UiState<T>>,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus_target: &'a mut Option<FocusTarget>,
    /// The ID of the window that the event was clicked on (for indexing into
    /// `app_state.windows`). `app_state.windows[event.window]` should never panic.
    pub window_id: &'a WindowId,
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
            state: casted_value,
            app_state_no_data: self.app_state_no_data,
            ui_state: self.ui_state,
            focus_target: self.focus_target,
            window_id: self.window_id,
            hit_dom_node: self.hit_dom_node,
            hit_test_items: self.hit_test_items,
            cursor_relative_to_item: self.cursor_relative_to_item,
            cursor_in_viewport: self.cursor_in_viewport,
        };
        callback(casted_callback_info)
    }

    pub fn get_window(&self) -> &FakeWindow<T> {
        &self.app_state_no_data.windows[self.window_id]
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
pub struct CallbackInfo<'a, 'b, T: 'a> {
    /// Mutable access to the application state. Use this field to modify data in the `T` data model.
    pub state: &'a mut AppState<T>,
    /// UiState containing the necessary data for testing what
    pub ui_state: &'a BTreeMap<DomId, UiState<T>>,
    /// The callback can change the focus_target - note that the focus_target is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus_target: &'b mut Option<FocusTarget>,
    /// The ID of the window that the event was clicked on (for indexing into
    /// `app_state.windows`). `app_state.windows[event.window]` should never panic.
    pub window_id: &'b WindowId,
    /// The ID of the DOM + the node that was hit. You can use this to query
    /// information about the node, but please don't hard-code any if / else
    /// statements based on the `NodeId`
    pub hit_dom_node: (DomId, NodeId),
    /// What items are currently being hit
    pub hit_test_items: &'b [HitTestItem],
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<(f32, f32)>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<(f32, f32)>,
}
pub type CallbackReturn = UpdateScreen;
pub type CallbackType<T> = fn(CallbackInfo<T>) -> CallbackReturn;

impl<'a, 'b, T: 'a> CallbackInfo<'a, 'b, T> {
    pub fn get_window(&self) -> &FakeWindow<T> {
        &self.state.windows[self.window_id]
    }
}

// -- opengl callback

/// Callbacks that returns a rendered OpenGL texture
pub struct GlCallback<T>(pub GlCallbackTypeUnchecked<T>);
impl_callback!(GlCallback<T>);
pub struct GlCallbackInfoUnchecked<'a, 'b, T: 'b> {
    pub ptr: StackCheckedPointer<T>,
    pub layout_info: LayoutInfo<'a, 'b, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub struct GlCallbackInfo<'a, 'b, T: 'b, U: Sized> {
    pub state: &'a mut U,
    pub layout_info: LayoutInfo<'a, 'b, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub type GlCallbackReturn = Option<Texture>;
pub type GlCallbackTypeUnchecked<T> = fn(GlCallbackInfoUnchecked<T>) -> GlCallbackReturn;
pub type GlCallbackType<T, U> = fn(GlCallbackInfo<T, U>) -> GlCallbackReturn;

impl<'a, 'b, T: 'b> GlCallbackInfoUnchecked<'a, 'b, T> {
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
pub struct IFrameCallbackInfoUnchecked<'a, 'b, T: 'b> {
    pub ptr: StackCheckedPointer<T>,
    pub layout_info: LayoutInfo<'a, 'b, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub struct IFrameCallbackInfo<'a, 'b, T: 'b, U: Sized> {
    pub state: &'a mut U,
    pub layout_info: LayoutInfo<'a, 'b, T>,
    pub bounds: HidpiAdjustedBounds,
}
pub type IFrameCallbackReturn<T> = Option<Dom<T>>; // todo: return virtual scrolling frames!
pub type IFrameCallbackTypeUnchecked<T> = fn(IFrameCallbackInfoUnchecked<T>) -> IFrameCallbackReturn<T>;
pub type IFrameCallbackType<T, U> = fn(IFrameCallbackInfo<T, U>) -> IFrameCallbackReturn<T>;

impl<'a, 'b, T: 'b> IFrameCallbackInfoUnchecked<'a, 'b, T> {
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
pub struct LayoutInfo<'a, 'b, T: 'b> {
    /// Gives _mutable_ access to the window
    pub window: &'b mut FakeWindow<T>,
    /// Allows the layout() function to reference app resources
    pub resources: &'a AppResources,
}

impl<'a, 'b, T: 'a> fmt::Debug for CallbackInfo<'a, 'b, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackInfo {{ \
            focus_target: {:?}, \
            window_id: {:?}, \
            hit_dom_node: {:?}, \
            ui_state: {:?}, \
            hit_test_items: {:?}, \
            cursor_relative_to_item: {:?}, \
            cursor_in_viewport: {:?}, \
        }}",
            self.focus_target,
            self.window_id,
            self.hit_dom_node,
            self.ui_state,
            self.hit_test_items,
            self.cursor_relative_to_item,
            self.cursor_in_viewport,
        )
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

impl<'a, 'b, T: 'a> CallbackInfo<'a, 'b, T> {

    /// Creates an iterator that starts at the current DOM node and continouusly
    /// returns the parent NodeId, until it gets to the root component.
    pub fn parent_nodes<'c>(&'c self) -> ParentNodesIterator<'c, 'a, 'b, T> {
        ParentNodesIterator {
            callback_info: &self,
            current_item: self.hit_dom_node.clone(),
        }
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

        let parent_node = self.parent(node_id)?;
        Some((node_layout.get_index_in_parent(node_id.1), parent_node))
    }

    // Functions that are may be called from the user callback
    // - the `CallbackInfo` contains a `&mut UiState`, which can be
    // used to query DOM information when the callbacks are run

    /// Returns the hierarchy of the given node ID
    pub fn get_node(&self, (dom_id, node_id): &(DomId, NodeId)) -> Option<&Node> {
        self.ui_state[dom_id].dom.arena.node_layout.internal.get(node_id.index())
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

    /// Returns the parent of the given `NodeId` or None if the target is the root node.
    pub fn parent(&self, node_id: &(DomId, NodeId)) -> Option<(DomId, NodeId)> {
        let new_node_id = self.get_node(node_id)?.parent?;
        Some((node_id.0.clone(), new_node_id))
    }

    /// Returns the parent of the current target or None if the target is the root node.
    pub fn target_parent(&self) -> Option<(DomId, NodeId)> {
        self.parent(&self.hit_dom_node)
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

    /// Set the focus_target to a certain div by parsing a string.
    /// Note that the parsing of the string can fail, therefore the Result
    #[cfg(feature = "css_parser")]
    pub fn set_focus_target_from_css<'c>(&mut self, input: &'c str) -> Result<(), CssPathParseError<'c>> {
        use azul_css_parser::parse_css_path;
        let path = parse_css_path(input)?;
        *self.focus_target = Some(FocusTarget::Path(path));
        Ok(())
    }

    /// Sets the focus_target by using an already-parsed `CssPath`.
    pub fn set_focus_target_from_path(&mut self, path: CssPath) {
        *self.focus_target = Some(FocusTarget::Path(path))
    }

    /// Set the focus_target of the window to a specific div using a `NodeId`.
    ///
    /// Note that this ID will be dependent on the position in the DOM and therefore
    /// the next frames UI must be the exact same as the current one, otherwise
    /// the focus_target will be cleared or shifted (depending on apps setting).
    pub fn set_focus_target_from_node_id(&mut self, id: (DomId, NodeId)) {
        *self.focus_target = Some(FocusTarget::Id(id));
    }

    /// Clears the focus_target for the next frame.
    pub fn clear_focus_target(&mut self) {
        *self.focus_target = Some(FocusTarget::NoFocus);
    }
}

/// Iterator that, starting from a certain starting point, returns the
/// parent node until it gets to the root node.
pub struct ParentNodesIterator<'a, 'b, 'c, T: 'c> {
    callback_info: &'a CallbackInfo<'b, 'c, T>,
    current_item: (DomId, NodeId),
}

impl<'a, 'b, 'c, T: 'b> ParentNodesIterator<'a, 'b, 'c, T> {

    /// Returns what node ID the iterator is currently processing
    pub fn current_node(&self) -> (DomId, NodeId) {
        self.current_item.clone()
    }

    /// Returns the offset into the parent of the current node or None if the item has no parent
    pub fn current_index_in_parent(&self) -> Option<usize> {
        self.callback_info.get_index_in_parent(&self.current_node()).map(|(index, _)| index)
    }
}

impl<'a, 'b, 'c, T: 'b> Iterator for ParentNodesIterator<'a, 'b, 'c, T> {
    type Item = (DomId, NodeId);

    /// For each item in the current item path, returns the index of the item in the parent
    fn next(&mut self) -> Option<(DomId, NodeId)> {
        let new_parent = self.callback_info.parent(&self.current_item)?;
        self.current_item = new_parent.clone();
        Some(new_parent)
    }
}