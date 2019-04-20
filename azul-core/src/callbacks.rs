use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};
use azul_css::CssPath;
#[cfg(feature = "css_parser")]
use azul_css_parser::CssPathParseError;
use {
    app::{AppState, AppStateNoData},
    async::TerminateTimer,
    dom::{Dom, NodeType, NodeData},
    ui_state::UiState,
    id_tree::{NodeId, Node, NodeHierarchy},
    app_resources::AppResources,
    window::{FakeWindow, LogicalSize, WindowId, PhysicalSize, LogicalPosition},
    gl::Texture,
};
pub use stack_checked_pointer::StackCheckedPointer;
pub use gleam::gl::Gl;

pub type DefaultCallbackType<T, U> = fn(&mut U, &mut AppStateNoData<T>, &mut CallbackInfo<T>) -> UpdateScreen;
pub type DefaultCallbackTypeUnchecked<T> = fn(&StackCheckedPointer<T>, &mut AppStateNoData<T>, &mut CallbackInfo<T>) -> UpdateScreen;

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
    pub point_in_viewport: LogicalPosition,
    /// The coordinates of the original hit test point relative to the origin of this item.
    /// This is useful for calculating things like text offsets in the client.
    pub point_relative_to_item: LogicalPosition,
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

/// Callback that is invoked "by default", for example a text field that always
/// has a default "ontextinput" handler
pub struct DefaultCallback<T>(pub DefaultCallbackTypeUnchecked<T>);

impl_callback!(DefaultCallback<T>);

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

pub type CallbackType<T> = fn(&mut AppState<T>, &mut CallbackInfo<T>) -> UpdateScreen;
/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `UpdateScreen` that denotes if the screen should be redrawn.
/// The style is not affected by this, so if you make changes to the window's style
/// inside the function, the screen will not be automatically redrawn, unless you return
/// an `UpdateScreen::Redraw` from the function
pub struct Callback<T>(pub CallbackType<T>);
impl_callback!(Callback<T>);

pub type GlTextureCallbackType<T> = fn(&StackCheckedPointer<T>, LayoutInfo<T>, HidpiAdjustedBounds) -> Texture;
/// Callbacks that returns a rendered OpenGL texture
pub struct GlTextureCallback<T>(pub GlTextureCallbackType<T>);
impl_callback!(GlTextureCallback<T>);

pub type IFrameCallbackType<T> = fn(&StackCheckedPointer<T>, LayoutInfo<T>, HidpiAdjustedBounds) -> Dom<T>;
/// Callback that, given a rectangle area on the screen, returns the DOM appropriate for that bounds (useful for infinite lists)
pub struct IFrameCallback<T>(pub IFrameCallbackType<T>);
impl_callback!(IFrameCallback<T>);

pub type TimerCallbackType<T> = fn(&mut T, app_resources: &mut AppResources) -> (UpdateScreen, TerminateTimer);
/// Callback that can runs on every frame on the main thread - can modify the app data model
pub struct TimerCallback<T>(pub TimerCallbackType<T>);
impl_callback!(TimerCallback<T>);

/// Gives the `layout()` function access to the `AppResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
pub struct LayoutInfo<'a, 'b, T: 'b> {
    /// Gives _mutable_ access to the window
    pub window: &'b mut FakeWindow<T>,
    /// Allows the layout() function to reference app resources
    pub resources: &'a AppResources,
}

/// Information about the callback that is passed to the callback whenever a callback is invoked
pub struct CallbackInfo<'a, T: 'a> {
    /// The callback can change the focus - note that the focus is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus: Option<FocusTarget>,
    /// The ID of the window that the event was clicked on (for indexing into
    /// `app_state.windows`). `app_state.windows[event.window]` should never panic.
    pub window_id: &'a WindowId,
    /// The ID of the node that was hit. You can use this to query information about
    /// the node, but please don't hard-code any if / else statements based on the `NodeId`
    pub hit_dom_node: NodeId,
    /// UiState containing the necessary data for testing what
    pub(crate) ui_state: &'a UiState<T>,
    /// What items are currently being hit
    pub(crate) hit_test_items: &'a [HitTestItem],
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was hit**.
    pub cursor_relative_to_item: Option<(f32, f32)>,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**.
    pub cursor_in_viewport: Option<(f32, f32)>,
}

impl<'a, T: 'a> Clone for CallbackInfo<'a, T> {
    fn clone(&self) -> Self {
        Self {
            focus: self.focus.clone(),
            window_id: self.window_id,
            hit_dom_node: self.hit_dom_node,
            ui_state: self.ui_state,
            hit_test_items: self.hit_test_items,
            cursor_relative_to_item: self.cursor_relative_to_item,
            cursor_in_viewport: self.cursor_in_viewport,
        }
    }
}

impl<'a, T: 'a> fmt::Debug for CallbackInfo<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackInfo {{ \
            focus: {:?}, \
            window_id: {:?}, \
            hit_dom_node: {:?}, \
            ui_state: {:?}, \
            hit_test_items: {:?}, \
            cursor_relative_to_item: {:?}, \
            cursor_in_viewport: {:?}, \
        }}",
            self.focus,
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
/// Necessary when invoking `IFrameCallbacks` and `GlTextureCallbacks`, so
/// that they can change what their content is based on their size.
#[derive(Debug, Copy, Clone)]
pub struct HidpiAdjustedBounds {
    logical_size: LogicalSize,
    hidpi_factor: f32,
    winit_hidpi_factor: f32,
    // TODO: Scroll state / focus state of this div!
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

/// Defines the focused node ID for the next frame
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FocusTarget {
    Id(NodeId),
    Path(CssPath),
    NoFocus,
}

impl<'a, T: 'a> CallbackInfo<'a, T> {

    /// Creates an iterator that starts at the current DOM node and continouusly
    /// returns the parent NodeId, until it gets to the root component.
    pub fn parent_nodes<'b>(&'b self) -> ParentNodesIterator<'b> {
        ParentNodesIterator {
            current_item: self.hit_dom_node,
            node_hierarchy: &self.ui_state.dom.arena.node_layout,
        }
    }

    /// For any node ID, returns what the position in its parent it is, plus the parent itself.
    /// Returns `None` on the root ID (because the root has no parent, therefore it's the 1st item)
    ///
    /// Note: Index is 0-based (first item has the index of 0)
    pub fn get_index_in_parent(&self, node_id: NodeId) -> Option<(usize, NodeId)> {
        let node_layout = &self.ui_state.dom.arena.node_layout;

        if node_id.index() > node_layout.len() {
            return None; // node_id out of range
        }

        let parent = node_layout[node_id].parent?;
        Some((node_layout.get_index_in_parent(node_id), parent))
    }

    // Functions that are may be called from the user callback
    // - the `CallbackInfo` contains a `&mut UiState`, which can be
    // used to query DOM information when the callbacks are run

    /// Returns the hierarchy of the given node ID
    pub fn get_node<'b>(&'b self, node_id: NodeId) -> Option<&'b Node> {
        self.ui_state.dom.arena.node_layout.internal.get(node_id.index())
    }

    /// Returns the node hierarchy (DOM tree order)
    pub fn get_node_hierarchy<'b>(&'b self) -> &'b NodeHierarchy {
        &self.ui_state.dom.arena.node_layout
    }

    /// Returns the node content of a specific node
    pub fn get_node_content<'b>(&'b self, node_id: NodeId) -> Option<&'b NodeData<T>> {
        self.ui_state.dom.arena.node_data.internal.get(node_id.index())
    }

    /// Returns the index of the target NodeId (the target that received the event)
    /// in the targets parent or None if the target is the root node
    pub fn target_index_in_parent(&self) -> Option<usize> {
        if self.get_node(self.hit_dom_node)?.parent.is_some() {
            Some(self.ui_state.dom.arena.node_layout.get_index_in_parent(self.hit_dom_node))
        } else {
            None
        }
    }

    /// Returns the parent of the given `NodeId` or None if the target is the root node.
    pub fn parent(&self, node_id: NodeId) -> Option<NodeId> {
        self.get_node(node_id)?.parent
    }

    /// Returns the parent of the current target or None if the target is the root node.
    pub fn target_parent(&self) -> Option<NodeId> {
        self.parent(self.hit_dom_node)
    }

    /// Checks whether the target of the CallbackInfo has a certain node type
    pub fn target_is_node_type(&self, node_type: NodeType<T>) -> bool {
        if let Some(self_node) = self.get_node_content(self.hit_dom_node) {
            self_node.is_node_type(node_type)
        } else {
            false
        }
    }

    /// Checks whether the target of the CallbackInfo has a certain ID
    pub fn target_has_id(&self, id: &str) -> bool {
        if let Some(self_node) = self.get_node_content(self.hit_dom_node) {
            self_node.has_id(id)
        } else {
            false
        }
    }

    /// Checks whether the target of the CallbackInfo has a certain class
    pub fn target_has_class(&self, class: &str) -> bool {
        if let Some(self_node) = self.get_node_content(self.hit_dom_node) {
            self_node.has_class(class)
        } else {
            false
        }
    }

    /// Traverses up the hierarchy, checks whether any parent has a certain ID,
    /// the returns that parent
    pub fn any_parent_has_id(&self, id: &str) -> Option<NodeId> {
        self.parent_nodes().find(|parent_id| {
            if let Some(self_node) = self.get_node_content(*parent_id) {
                self_node.has_id(id)
            } else {
                false
            }
        })
    }

    /// Traverses up the hierarchy, checks whether any parent has a certain class
    pub fn any_parent_has_class(&self, class: &str) -> Option<NodeId> {
        self.parent_nodes().find(|parent_id| {
            if let Some(self_node) = self.get_node_content(*parent_id) {
                self_node.has_class(class)
            } else {
                false
            }
        })
    }

    /// Set the focus to a certain div by parsing a string.
    /// Note that the parsing of the string can fail, therefore the Result
    #[cfg(feature = "css_parser")]
    pub fn set_focus<'b>(&mut self, input: &'b str) -> Result<(), CssPathParseError<'b>> {
        use azul_css_parser::parse_css_path;
        let path = parse_css_path(input)?;
        self.focus = Some(FocusTarget::Path(path));
        Ok(())
    }

    /// Sets the focus by using an already-parsed `CssPath`.
    pub fn set_focus_by_path(&mut self, path: CssPath) {
        self.focus = Some(FocusTarget::Path(path))
    }

    /// Set the focus of the window to a specific div using a `NodeId`.
    ///
    /// Note that this ID will be dependent on the position in the DOM and therefore
    /// the next frames UI must be the exact same as the current one, otherwise
    /// the focus will be cleared or shifted (depending on apps setting).
    pub fn set_focus_by_node_id(&mut self, id: NodeId) {
        self.focus = Some(FocusTarget::Id(id));
    }

    /// Clears the focus for the next frame.
    pub fn clear_focus(&mut self) {
        self.focus = Some(FocusTarget::NoFocus);
    }
}

/// Iterator that, starting from a certain starting point, returns the
/// parent node until it gets to the root node.
pub struct ParentNodesIterator<'a> {
    current_item: NodeId,
    node_hierarchy: &'a NodeHierarchy,
}

impl<'a> ParentNodesIterator<'a> {

    /// Returns what node ID the iterator is currently processing
    pub fn current_node(&self) -> NodeId {
        self.current_item
    }

    /// Returns the offset into the parent of the current node or None if the item has no parent
    pub fn current_index_in_parent(&self) -> Option<usize> {
        if self.node_hierarchy[self.current_item].has_parent() {
            Some(self.node_hierarchy.get_index_in_parent(self.current_item))
        } else {
            None
        }
    }
}

impl<'a> Iterator for ParentNodesIterator<'a> {
    type Item = NodeId;

    /// For each item in the current item path, returns the index of the item in the parent
    fn next(&mut self) -> Option<NodeId> {
        let new_parent = self.node_hierarchy[self.current_item].parent?;
        self.current_item = new_parent;
        Some(new_parent)
    }
}