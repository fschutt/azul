use std::{
    collections::BTreeMap,
    fmt,
    hash::Hasher,
    sync::atomic::{AtomicUsize, Ordering},
};
use azul_css::CssPath;
#[cfg(feature = "css-parser")]
use azul_css_parser::CssPathParseError;
use webrender::api::HitTestItem;
use glium::glutin::WindowId as GliumWindowId;
use {
    dom::{UpdateScreen, DontRedraw, NodeType, NodeData},
    traits::Layout,
    app::AppStateNoData,
    ui_state::UiState,
    id_tree::{NodeId, Node, NodeHierarchy},
    app_resources::AppResources,
    window::FakeWindow,
};
pub use stack_checked_pointer::StackCheckedPointer;

pub type DefaultCallbackType<T, U> = fn(&mut U, &mut AppStateNoData<T>, &mut CallbackInfo<T>) -> UpdateScreen;
pub type DefaultCallbackTypeUnchecked<T> = fn(&StackCheckedPointer<T>, &mut AppStateNoData<T>, &mut CallbackInfo<T>) -> UpdateScreen;

static LAST_DEFAULT_CALLBACK_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DefaultCallbackId(usize);

pub(crate) fn get_new_unique_default_callback_id() -> DefaultCallbackId {
    DefaultCallbackId(LAST_DEFAULT_CALLBACK_ID.fetch_add(1, Ordering::SeqCst))
}

pub struct DefaultCallback<T: Layout>(pub DefaultCallbackTypeUnchecked<T>);

impl_callback_bounded!(DefaultCallback<T: Layout>);

pub(crate) struct DefaultCallbackSystem<T: Layout> {
    callbacks: BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
}

impl<T: Layout> DefaultCallbackSystem<T> {

    /// Creates a new, empty list of callbacks
    pub(crate) fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }

    pub fn add_callback(
        &mut self,
        callback_id: DefaultCallbackId,
        ptr: StackCheckedPointer<T>,
        func: DefaultCallback<T>)
    {
        self.callbacks.insert(callback_id, (ptr, func));
    }

    /// NOTE: `app_data` is required so we know that we don't
    /// accidentally alias the data in `self.internal` (which could lead to UB).
    ///
    /// What we know is that the pointer (`self.internal`) points to somewhere
    /// in `T`, so we know that `self.internal` isn't aliased
    pub(crate) fn run_callback(
        &self,
        _app_data: &mut T,
        callback_id: &DefaultCallbackId,
        app_state_no_data: &mut AppStateNoData<T>,
        window_event: &mut CallbackInfo<T>)
    -> UpdateScreen
    {
        if let Some((callback_ptr, callback_fn)) = self.callbacks.get(callback_id) {
            (callback_fn.0)(callback_ptr, app_state_no_data, window_event)
        } else {
            #[cfg(feature = "logging")] {
                warn!("Calling default callback with invalid ID {:?}", callback_id);
            }
            DontRedraw
        }
    }

    /// Clears all callbacks
    pub(crate) fn clear(&mut self) {
        self.callbacks.clear();
    }
}

impl<T: Layout> Clone for DefaultCallbackSystem<T> {
    fn clone(&self) -> Self {
        Self {
            callbacks: self.callbacks.clone(),
        }
    }
}

/// Gives the `layout()` function access to the `AppResources` and the `Window`
/// (for querying images and fonts, as well as width / height)
pub struct LayoutInfo<'a, 'b, T: 'b + Layout> {
    pub window: &'b mut FakeWindow<T>,
    pub resources: &'a AppResources,
}

/// Information about the callback that is passed to the callback whenever a callback is invoked
pub struct CallbackInfo<'a, T: 'a + Layout> {
    /// The callback can change the focus - note that the focus is set before the
    /// next frames' layout() function is invoked, but the current frames callbacks are not affected.
    pub focus: Option<FocusTarget>,
    /// The ID of the window that the event was clicked on (for indexing into
    /// `app_state.windows`). `app_state.windows[event.window]` should never panic.
    pub window_id: &'a GliumWindowId,
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

impl<'a, T: 'a + Layout> Clone for CallbackInfo<'a, T> {
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

impl<'a, T: 'a + Layout> fmt::Debug for CallbackInfo<'a, T> {
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

impl<'a, T: 'a + Layout> CallbackInfo<'a, T> {

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
}

/// Defines the focused node ID for the next frame
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FocusTarget {
    Id(NodeId),
    Path(CssPath),
    NoFocus,
}

impl<'a, T: 'a + Layout> CallbackInfo<'a, T> {

    /// Set the focus to a certain div by parsing a string.
    /// Note that the parsing of the string can fail, therefore the Result
    #[cfg(feature = "css-parser")]
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