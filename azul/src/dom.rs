use azul_css::{CssProperty, NodeTypePath};
use glium::{framebuffer::SimpleFrameBuffer, Texture2d};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fmt,
    hash::{Hash, Hasher},
    iter::FromIterator,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
use {
    app_state::AppState,
    default_callbacks::{DefaultCallbackId, StackCheckedPointer},
    id_tree::{Arena, Node, NodeDataContainer, NodeHierarchy, NodeId},
    images::{ImageId, ImageState},
    text_cache::TextId,
    text_layout::{FontMetrics, TextSizePx, Words},
    traits::Layout,
    ui_state::UiState,
    window::HidpiAdjustedBounds,
    window::{WindowEvent, WindowInfo},
    FastHashMap,
};

static TAG_ID: AtomicUsize = AtomicUsize::new(1);

pub(crate) type TagId = u64;

/// Same as the `TagId`, but only for scrollable nodes
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) struct ScrollTagId(pub TagId);

pub(crate) fn new_tag_id() -> TagId {
    TAG_ID.fetch_add(1, Ordering::SeqCst) as TagId
}

pub(crate) fn new_scroll_tag_id() -> ScrollTagId {
    ScrollTagId(new_tag_id())
}

/// Calculated hash of a DOM node, used for querying attributes of the DOM node
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DomHash(pub u64);

/// A callback function has to return if the screen should
/// be updated after the function has run.PartialEq
///
/// This is necessary for updating the screen only if it is absolutely necessary.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum UpdateScreen {
    /// Redraw the screen
    Redraw,
    /// Don't redraw the screen
    DontRedraw,
}

/// This exist so you can conveniently use the `?` and `.into()` for your own code
///
/// - `Some`: `Redraw`
/// - `None`: `DontRedraw`
impl<T> From<Option<T>> for UpdateScreen {
    fn from(input: Option<T>) -> Self {
        match input {
            None => UpdateScreen::DontRedraw,
            Some(_) => UpdateScreen::Redraw,
        }
    }
}

/// This exist so you can conveniently use the `?` and `.into()` for your own code
///
/// - `Ok` -> `Redraw`
/// - `Err` -> `DontRedraw`
impl<T, E> From<Result<T, E>> for UpdateScreen {
    fn from(input: Result<T, E>) -> Self {
        match input {
            Ok(_) => UpdateScreen::DontRedraw,
            Err(_) => UpdateScreen::Redraw,
        }
    }
}

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `UpdateScreen` that denotes if the screen should be redrawn.
/// The style is not affected by this, so if you make changes to the window's style
/// inside the function, the screen will not be automatically redrawn, unless you return
/// an `UpdateScreen::Redraw` from the function
pub struct Callback<T: Layout>(pub fn(&mut AppState<T>, WindowEvent<T>) -> UpdateScreen);

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for Callback<T>

impl<T: Layout> fmt::Debug for Callback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Callback @ 0x{:x}", self.0 as usize)
    }
}

impl<T: Layout> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Callback(self.0.clone())
    }
}

/// As a hashing function, we use the function pointer casted to a usize
/// as a unique ID for the function. This way, we can hash and compare DOM nodes
/// (to create diffs between two states). Comparing usizes is more efficient
/// than re-creating the whole DOM and serves as a caching mechanism.
impl<T: Layout> Hash for Callback<T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        state.write_usize(self.0 as usize);
    }
}

/// Basically compares the function pointers and types for equality
impl<T: Layout> PartialEq for Callback<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 as usize == rhs.0 as usize
    }
}

impl<T: Layout> Eq for Callback<T> {}

impl<T: Layout> Copy for Callback<T> {}

pub struct GlTextureCallback<T: Layout>(
    pub fn(&StackCheckedPointer<T>, WindowInfo<T>, HidpiAdjustedBounds) -> Option<Texture>,
);

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for GlTextureCallback<T>

impl<T: Layout> fmt::Debug for GlTextureCallback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "GlTextureCallback @ 0x{:x}", self.0 as usize)
    }
}

impl<T: Layout> Clone for GlTextureCallback<T> {
    fn clone(&self) -> Self {
        GlTextureCallback(self.0.clone())
    }
}

impl<T: Layout> Hash for GlTextureCallback<T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        state.write_usize(self.0 as usize);
    }
}

impl<T: Layout> PartialEq for GlTextureCallback<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 as usize == rhs.0 as usize
    }
}

impl<T: Layout> Eq for GlTextureCallback<T> {}
impl<T: Layout> Copy for GlTextureCallback<T> {}

pub struct IFrameCallback<T: Layout>(
    pub fn(&StackCheckedPointer<T>, WindowInfo<T>, HidpiAdjustedBounds) -> Dom<T>,
);

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for IFrameCallback<T>

impl<T: Layout> fmt::Debug for IFrameCallback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IFrameCallback @ 0x{:x}", self.0 as usize)
    }
}

impl<T: Layout> Clone for IFrameCallback<T> {
    fn clone(&self) -> Self {
        IFrameCallback(self.0.clone())
    }
}

impl<T: Layout> Hash for IFrameCallback<T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        state.write_usize(self.0 as usize);
    }
}

impl<T: Layout> PartialEq for IFrameCallback<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 as usize == rhs.0 as usize
    }
}

impl<T: Layout> Eq for IFrameCallback<T> {}

impl<T: Layout> Copy for IFrameCallback<T> {}

/// List of core DOM node types built-into by `azul`.
pub enum NodeType<T: Layout> {
    /// Regular div with no particular type of data attached
    Div,
    /// A small label that can be (optionally) be selectable with the mouse
    Label(String),
    /// Larger amount of text, that has to be cached
    Text(TextId),
    /// An image that is rendered by WebRender. The id is acquired by the
    /// `AppState::add_image()` function
    Image(ImageId),
    /// OpenGL texture. The `Svg` widget deserizalizes itself into a texture
    /// Equality and Hash values are only checked by the OpenGl texture ID,
    /// Azul does not check that the contents of two textures are the same
    GlTexture((GlTextureCallback<T>, StackCheckedPointer<T>)),
    /// DOM that gets passed its width / height during the layout
    IFrame((IFrameCallback<T>, StackCheckedPointer<T>)),
}

// #[derive(Debug, Clone, PartialEq, Hash, Eq)] for NodeType<T>

impl<T: Layout> fmt::Debug for NodeType<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::NodeType::*;
        match self {
            Div => write!(f, "NodeType::Div"),
            Label(a) => write!(f, "NodeType::Label {{ {:?} }}", a),
            Text(a) => write!(f, "NodeType::Text {{ {:?} }}", a),
            Image(a) => write!(f, "NodeType::Image {{ {:?} }}", a),
            GlTexture((ptr, cb)) => write!(
                f,
                "NodeType::GlTexture {{ ptr: {:?}, callback: {:?} }}",
                ptr, cb
            ),
            IFrame((ptr, cb)) => write!(
                f,
                "NodeType::IFrame {{ ptr: {:?}, callback: {:?} }}",
                ptr, cb
            ),
        }
    }
}

impl<T: Layout> Clone for NodeType<T> {
    fn clone(&self) -> Self {
        use self::NodeType::*;
        match self {
            Div => Div,
            Label(a) => Label(a.clone()),
            Text(a) => Text(a.clone()),
            Image(a) => Image(a.clone()),
            GlTexture((ptr, a)) => GlTexture((ptr.clone(), a.clone())),
            IFrame((ptr, a)) => IFrame((ptr.clone(), a.clone())),
        }
    }
}

impl<T: Layout> Hash for NodeType<T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        use self::NodeType::*;
        use std::mem;
        mem::discriminant(&self).hash(state);
        match self {
            Div => {}
            Label(a) => a.hash(state),
            Text(a) => a.hash(state),
            Image(a) => a.hash(state),
            GlTexture((ptr, a)) => {
                ptr.hash(state);
                a.hash(state);
            }
            IFrame((ptr, a)) => {
                ptr.hash(state);
                a.hash(state);
            }
        }
    }
}

impl<T: Layout> PartialEq for NodeType<T> {
    fn eq(&self, rhs: &Self) -> bool {
        use self::NodeType::*;
        match (self, rhs) {
            (Div, Div) => true,
            (Label(a), Label(b)) => a == b,
            (Text(a), Text(b)) => a == b,
            (Image(a), Image(b)) => a == b,
            (GlTexture((ptr_a, a)), GlTexture((ptr_b, b))) => a == b && ptr_a == ptr_b,
            (IFrame((ptr_a, a)), IFrame((ptr_b, b))) => a == b && ptr_a == ptr_b,
            _ => false,
        }
    }
}

impl<T: Layout> Eq for NodeType<T> {}

impl<T: Layout> NodeType<T> {
    pub(crate) fn get_path(&self) -> NodeTypePath {
        use self::NodeType::*;
        match self {
            Div => NodeTypePath::Div,
            Label(_) | Text(_) => NodeTypePath::P,
            Image(_) => NodeTypePath::Img,
            GlTexture(_) => NodeTypePath::Texture,
            IFrame(_) => NodeTypePath::IFrame,
        }
    }

    /// Returns the preferred width, for example for an image, that would be the
    /// original width (an image always wants to take up the original space)
    pub(crate) fn get_preferred_width(
        &self,
        image_cache: &FastHashMap<ImageId, ImageState>,
    ) -> Option<f32> {
        use self::NodeType::*;
        match self {
            Image(i) => image_cache
                .get(i)
                .and_then(|image_state| Some(image_state.get_dimensions().0)),
            Label(_) | Text(_) =>
            /* TODO: Calculate the minimum width for the text? */
            {
                None
            }
            _ => None,
        }
    }

    /// Given a certain width, returns the
    pub(crate) fn get_preferred_height_based_on_width(
        &self,
        div_width: TextSizePx,
        image_cache: &FastHashMap<ImageId, ImageState>,
        words: Option<&Words>,
        font_metrics: Option<FontMetrics>,
    ) -> Option<TextSizePx> {
        use self::NodeType::*;
        use azul_css::{LayoutOverflow, TextOverflowBehaviour, TextOverflowBehaviourInner};

        match self {
            Image(i) => image_cache.get(i).and_then(|image_state| {
                let (image_original_height, image_original_width) = image_state.get_dimensions();
                Some(div_width * (image_original_width / image_original_height))
            }),
            Label(_) | Text(_) => {
                let (words, font) = (words?, font_metrics?);
                let vertical_info = words.get_vertical_height(
                    &LayoutOverflow {
                        horizontal: TextOverflowBehaviour::Modified(
                            TextOverflowBehaviourInner::Scroll,
                        ),
                        ..Default::default()
                    },
                    &font,
                    div_width,
                );
                Some(vertical_info.vertical_height)
            }
            _ => None,
        }
    }
}

/// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum On {
    /// Mouse cursor is hovering over the element
    MouseOver,
    /// Mouse cursor has is over element and is pressed
    /// (not good for "click" events - use `MouseUp` instead)
    MouseDown,
    /// (Specialization of `MouseDown`). Fires only if the left mouse button
    /// has been pressed while cursor was over the element
    LeftMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the middle mouse button
    /// has been pressed while cursor was over the element
    MiddleMouseDown,
    /// (Specialization of `MouseDown`). Fires only if the right mouse button
    /// has been pressed while cursor was over the element
    RightMouseDown,
    /// Mouse button has been released while cursor was over the element
    MouseUp,
    /// (Specialization of `MouseUp`). Fires only if the left mouse button has
    /// been released while cursor was over the element
    LeftMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the middle mouse button has
    /// been released while cursor was over the element
    MiddleMouseUp,
    /// (Specialization of `MouseUp`). Fires only if the right mouse button has
    /// been released while cursor was over the element
    RightMouseUp,
    /// Mouse cursor has entered the element
    MouseEnter,
    /// Mouse cursor has left the element
    MouseLeave,
    /// Mousewheel / touchpad scrolling
    Scroll,
    /// The window received a unicode character (also respects the system locale).
    /// Check `keyboard_state.current_char` to get the current pressed character.
    TextInput,
    /// A **virtual keycode** was pressed. Note: This is only the virtual keycode,
    /// not the actual char. If you want to get the character, use `TextInput` instead.
    /// A virtual key does not have to map to a printable character.
    ///
    /// You can get all currently pressed virtual keycodes in the `keyboard_state.current_virtual_keycodes`
    /// and / or just the last keycode in the `keyboard_state.latest_virtual_keycode`.
    VirtualKeyDown,
    /// A **virtual keycode** was release. See `VirtualKeyDown` for more info.
    VirtualKeyUp,
    /// A file has been dropped on the element
    HoveredFile,
    /// A file is being hovered on the element
    DroppedFile,
    /// A file was hovered, but has exited the window
    HoveredFileCancelled,
    /// Equivalent to `onfocus`
    FocusReceived,
    /// Equivalent to `onblur`
    FocusLost,
}

pub struct NodeData<T: Layout> {
    /// `div`
    pub node_type: NodeType<T>,
    /// `#main #something`
    pub ids: Vec<String>,
    /// `.myclass .otherclass`
    pub classes: Vec<String>,
    /// `On::MouseUp` -> `Callback(my_button_click_handler)`
    pub callbacks: Vec<(On, Callback<T>)>,
    /// Usually not set by the user directly - `FakeWindow::add_default_callback`
    /// returns a callback ID, so that we know which default callback(s) are attached
    /// to this node.
    ///
    /// This is only important if this node has any default callbacks.
    pub default_callback_ids: Vec<(On, DefaultCallbackId)>,
    /// Override certain dynamic styling properties in this frame. For this,
    /// these properties have to have a name (the ID).
    ///
    /// For example, in the CSS stylesheet:
    ///
    /// ```css,ignore
    /// #my_item { width: [[ my_custom_width | 200px ]] }
    /// ```
    ///
    /// ```rust,ignore
    /// let node = NodeData {
    ///     id: Some("my_item".into()),
    ///     dynamic_css_overrides: vec![("my_custom_width".into(), CssProperty::Width(LayoutWidth::px(500.0)))]
    /// }
    /// ```
    pub dynamic_css_overrides: Vec<(String, CssProperty)>,
    /// Whether this div can be dragged or not, similar to `draggable = "true"` in HTML, .
    ///
    /// **TODO**: Currently doesn't do anything, since the drag & drop implementation is missing, API stub.
    pub draggable: bool,
    /// Whether this div can be focused, and if yes, in what default to `None` (not focusable).
    /// Note that without this, there can be no `On::FocusReceived` (equivalent to onfocus),
    /// `On::FocusLost` (equivalent to onblur), etc. events.
    pub tab_index: Option<TabIndex>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum TabIndex {
    /// Automatic tab index, similar to simply setting `focusable = "true"` or `tabindex = 0`
    /// (both have the effect of making the element focusable).
    ///
    /// Sidenote: See https://www.w3.org/TR/html5/editing.html#sequential-focus-navigation-and-the-tabindex-attribute
    /// for interesting notes on tabindex and accessibility
    Auto,
    /// Set the tab index in relation to its parent element. I.e. if you have a list of elements,
    /// the focusing order is restricted to the current parent.
    ///
    /// Ex. a div might have:
    ///
    /// ```no_run,ignore
    /// div (Auto)
    /// |- element1 (OverrideInParent 0) <- current focus
    /// |- element2 (OverrideInParent 5)
    /// |- element3 (OverrideInParent 2)
    /// |- element4 (Global 5)
    /// ```
    ///
    /// When pressing tab repeatedly, the focusing order will be
    /// "element3, element2, element4, div", since OverrideInParent elements
    /// take precedence among global order.
    OverrideInParent(usize),
    /// Set the global tabindex order, independe
    Global(usize),
}

impl Default for TabIndex {
    fn default() -> Self {
        TabIndex::Auto
    }
}

impl<T: Layout> PartialEq for NodeData<T> {
    fn eq(&self, other: &Self) -> bool {
        self.node_type == other.node_type
            && self.ids == other.ids
            && self.classes == other.classes
            && self.callbacks == other.callbacks
            && self.default_callback_ids == other.default_callback_ids
            && self.dynamic_css_overrides == other.dynamic_css_overrides
            && self.draggable == other.draggable
            && self.tab_index == other.tab_index
    }
}

impl<T: Layout> Eq for NodeData<T> {}

impl<T: Layout> Default for NodeData<T> {
    fn default() -> Self {
        NodeData {
            node_type: NodeType::Div,
            ids: Vec::new(),
            classes: Vec::new(),
            callbacks: Vec::new(),
            default_callback_ids: Vec::new(),
            dynamic_css_overrides: Vec::new(),
            draggable: false,
            tab_index: None,
        }
    }
}

impl<T: Layout> Hash for NodeData<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        for id in &self.ids {
            id.hash(state);
        }
        for class in &self.classes {
            class.hash(state);
        }
        for callback in &self.callbacks {
            callback.hash(state);
        }
        for default_callback_id in &self.default_callback_ids {
            default_callback_id.hash(state);
        }
        for dynamic_css_override in &self.dynamic_css_overrides {
            dynamic_css_override.hash(state);
        }
        self.draggable.hash(state);
        self.tab_index.hash(state);
    }
}

impl<T: Layout> Clone for NodeData<T> {
    fn clone(&self) -> Self {
        Self {
            node_type: self.node_type.clone(),
            ids: self.ids.clone(),
            classes: self.classes.clone(),
            callbacks: self.callbacks.clone(),
            default_callback_ids: self.default_callback_ids.clone(),
            dynamic_css_overrides: self.dynamic_css_overrides.clone(),
            draggable: self.draggable.clone(),
            tab_index: self.tab_index.clone(),
        }
    }
}

impl<T: Layout> fmt::Display for NodeData<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let html_type = self.node_type.get_path();

        let id_string = if self.ids.is_empty() {
            String::new()
        } else {
            self.ids
                .iter()
                .map(|x| format!("#{}", x))
                .collect::<Vec<String>>()
                .join(" ")
        };

        let class_string = if self.classes.is_empty() {
            String::new()
        } else {
            self.classes
                .iter()
                .map(|x| format!(".{}", x))
                .collect::<Vec<String>>()
                .join(" ")
        };

        write!(f, "[{} {} {}]", html_type, id_string, class_string)
    }
}

impl<T: Layout> fmt::Debug for NodeData<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "NodeData {{ \
             \tnode_type: {:?}, \
             \tids: {:?}, \
             \tclasses: {:?}, \
             \tcallbacks: {:?}, \
             \tdefault_callback_ids: {:?}, \
             \tdynamic_css_overrides: {:?}, \
             \tdraggable: {:?}, \
             \ttab_index: {:?}, \
             }}",
            self.node_type,
            self.ids,
            self.classes,
            self.callbacks,
            self.default_callback_ids,
            self.dynamic_css_overrides,
            self.draggable,
            self.tab_index
        )
    }
}

impl<T: Layout> NodeData<T> {
    pub(crate) fn calculate_node_data_hash(&self) -> DomHash {
        use std::hash::Hash;

        // Pick hash algorithm based on features
        #[cfg(not(feature = "faster-hashing"))]
        use std::collections::hash_map::DefaultHasher as HashAlgorithm;
        #[cfg(feature = "faster-hashing")]
        use twox_hash::XxHash as HashAlgorithm;

        let mut hasher = HashAlgorithm::default();
        self.hash(&mut hasher);
        DomHash(hasher.finish())
    }

    /// Creates a new NodeData
    pub fn new(node_type: NodeType<T>) -> Self {
        Self {
            node_type,
            ..Default::default()
        }
    }
}

/// The document model, similar to HTML. This is a create-only structure, you don't actually read anything back
#[derive(Clone, PartialEq, Eq)]
pub struct Dom<T: Layout> {
    pub(crate) arena: Rc<RefCell<Arena<NodeData<T>>>>,
    pub(crate) root: NodeId,
    pub(crate) head: NodeId,
}

impl<T: Layout> fmt::Debug for Dom<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Dom {{ arena: {:?}, root: {:?}, head: {:?} }}",
            self.arena, self.root, self.head
        )
    }
}

impl<T: Layout> FromIterator<Dom<T>> for Dom<T> {
    fn from_iter<I: IntoIterator<Item = Dom<T>>>(iter: I) -> Self {
        let mut c = Dom::new(NodeType::Div);
        for i in iter {
            c.add_child(i);
        }
        c
    }
}

impl<T: Layout> FromIterator<NodeData<T>> for Dom<T> {
    fn from_iter<I: IntoIterator<Item = NodeData<T>>>(iter: I) -> Self {
        use id_tree::Node;

        // We have to use a "root" node, otherwise we run into problems if
        // the iterator executes 0 times (and therefore pushes 0 nodes)

        // "Root" node of this DOM
        let mut node_data = vec![NodeData::new(NodeType::Div)];
        let mut node_layout = vec![Node {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            last_child: None,
            first_child: None,
        }];

        let mut idx = 0;

        for item in iter {
            let node = Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: if idx == 0 {
                    None
                } else {
                    Some(NodeId::new(idx))
                },
                next_sibling: Some(NodeId::new(idx + 2)),
                last_child: None,
                first_child: None,
            };
            node_layout.push(node);
            node_data.push(item);

            idx += 1;
        }

        let nodes_len = node_layout.len();

        // nodes_len is always at least 1, since we pushed the original root node
        // Check if there is a child DOM
        if nodes_len > 1 {
            if let Some(last) = node_layout.get_mut(nodes_len - 1) {
                last.next_sibling = None;
            }
            node_layout[0].last_child = Some(NodeId::new(nodes_len - 1));
            node_layout[0].first_child = Some(NodeId::new(1));
        }

        Dom {
            head: NodeId::new(0),
            root: NodeId::new(0),
            arena: Rc::new(RefCell::new(Arena {
                node_data: NodeDataContainer::new(node_data),
                node_layout: NodeHierarchy::new(node_layout),
            })),
        }
    }
}

impl<T: Layout> FromIterator<NodeType<T>> for Dom<T> {
    fn from_iter<I: IntoIterator<Item = NodeType<T>>>(iter: I) -> Self {
        iter.into_iter()
            .map(|i| NodeData {
                node_type: i,
                ..Default::default()
            })
            .collect()
    }
}

impl<T: Layout> Dom<T> {
    /// Creates an empty DOM
    #[inline]
    pub fn new(node_type: NodeType<T>) -> Self {
        Self::with_capacity(node_type, 0)
    }

    /// Shorthand for `Dom::new(NodeType::Div)`.
    #[inline]
    pub fn div() -> Self {
        Self::new(NodeType::Div)
    }

    /// Shorthand for `Dom::new(NodeType::Label(value.into()))`
    pub fn label<S: Into<String>>(value: S) -> Self {
        Self::new(NodeType::Label(value.into()))
    }

    /// Shorthand for `Dom::new(NodeType::Text(text_id))`
    #[inline]
    pub fn text_id(text_id: TextId) -> Self {
        Self::new(NodeType::Text(text_id))
    }

    /// Shorthand for `Dom::new(NodeType::Image(image_id))`
    #[inline]
    pub fn image(image: ImageId) -> Self {
        Self::new(NodeType::Image(image))
    }

    /// Shorthand for `Dom::new(NodeType::GlTexture((callback, ptr)))`
    #[inline]
    pub fn gl_texture(callback: GlTextureCallback<T>, ptr: StackCheckedPointer<T>) -> Self {
        Self::new(NodeType::GlTexture((callback, ptr)))
    }

    /// Shorthand for `Dom::new(NodeType::IFrame((callback, ptr)))`
    #[inline]
    pub fn iframe(callback: IFrameCallback<T>, ptr: StackCheckedPointer<T>) -> Self {
        Self::new(NodeType::IFrame((callback, ptr)))
    }

    /// Returns the number of nodes in this DOM
    #[inline]
    pub fn len(&self) -> usize {
        self.arena.borrow().len()
    }

    /// Creates an empty DOM with space reserved for `cap` nodes
    #[inline]
    pub fn with_capacity(node_type: NodeType<T>, cap: usize) -> Self {
        let mut arena = Arena::with_capacity(cap.saturating_add(1));
        let root = arena.new_node(NodeData::new(node_type));
        Self {
            arena: Rc::new(RefCell::new(arena)),
            root: root,
            head: root,
        }
    }

    /// Adds a child DOM to the current DOM
    pub fn add_child(&mut self, child: Self) {
        // Note: for a more readable Python version of this algorithm,
        // see: https://gist.github.com/fschutt/4b3bd9a2654b548a6eb0b6a8623bdc8a#file-dow_new_2-py-L65-L107

        let self_len = self.arena.borrow().len();
        let child_len = child.arena.borrow().len();

        if child_len == 0 {
            // No nodes to append, nothing to do
            return;
        }

        if self_len == 0 {
            // Self has no nodes, therefore all child nodes will
            // replace the self nodes, so
            *self = child;
            return;
        }

        let mut self_arena = self.arena.borrow_mut();
        let mut child_arena = child.arena.borrow_mut();

        let mut last_sibling = None;

        for node_id in 0..child_len {
            let node_id = NodeId::new(node_id);
            let node_id_child: &mut Node = &mut child_arena.node_layout[node_id];

            // WARNING: Order of these blocks is important!

            if node_id_child
                .previous_sibling
                .as_mut()
                .and_then(|previous_sibling| {
                    // Some(previous_sibling) - increase the parent ID by the current arena length
                    *previous_sibling += self_len;
                    Some(previous_sibling)
                })
                .is_none()
            {
                // None - set the current heads' last child as the new previous sibling
                let last_child = self_arena.node_layout[self.head].last_child;
                if last_child.is_some() && node_id_child.parent.is_none() {
                    node_id_child.previous_sibling = last_child;
                    self_arena.node_layout[last_child.unwrap()].next_sibling =
                        Some(node_id + self_len);
                }
            }

            if node_id_child
                .parent
                .as_mut()
                .and_then(|parent| {
                    *parent += self_len;
                    Some(parent)
                })
                .is_none()
            {
                // Have we encountered the last root item?
                if node_id_child.next_sibling.is_none() {
                    last_sibling = Some(node_id);
                }
                node_id_child.parent = Some(self.head);
            }

            if let Some(next_sibling) = node_id_child.next_sibling.as_mut() {
                *next_sibling += self_len;
            }

            if let Some(first_child) = node_id_child.first_child.as_mut() {
                *first_child += self_len;
            }

            if let Some(last_child) = node_id_child.last_child.as_mut() {
                *last_child += self_len;
            }
        }

        self_arena.node_layout[self.head]
            .first_child
            .get_or_insert(NodeId::new(self_len));
        self_arena.node_layout[self.head].last_child = Some(last_sibling.unwrap() + self_len);

        (&mut *self_arena).append_arena(&mut child_arena);
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_id<S: Into<String>>(mut self, id: S) -> Self {
        self.add_id(id);
        self
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_class<S: Into<String>>(mut self, class: S) -> Self {
        self.add_class(class);
        self
    }

    /// Same as `event`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_callback(mut self, on: On, callback: Callback<T>) -> Self {
        self.add_callback(on, callback);
        self
    }

    #[inline]
    pub fn with_child(mut self, child: Self) -> Self {
        self.add_child(child);
        self
    }

    #[inline]
    pub fn with_css_override<S: Into<String>>(mut self, id: S, property: CssProperty) -> Self {
        self.add_css_override(id, property);
        self
    }

    #[inline]
    pub fn with_tab_index(mut self, tab_index: TabIndex) -> Self {
        self.add_tab_index(tab_index);
        self
    }

    #[inline]
    pub fn add_id<S: Into<String>>(&mut self, id: S) {
        self.arena.borrow_mut().node_data[self.head]
            .ids
            .push(id.into());
    }

    #[inline]
    pub fn add_class<S: Into<String>>(&mut self, class: S) {
        self.arena.borrow_mut().node_data[self.head]
            .classes
            .push(class.into());
    }

    #[inline]
    pub fn add_callback(&mut self, on: On, callback: Callback<T>) {
        self.arena.borrow_mut().node_data[self.head]
            .callbacks
            .push((on, callback));
    }

    #[inline]
    pub fn add_default_callback_id(&mut self, on: On, id: DefaultCallbackId) {
        self.arena.borrow_mut().node_data[self.head]
            .default_callback_ids
            .push((on, id));
    }

    #[inline]
    pub fn add_tab_index(&mut self, tab_index: TabIndex) {
        self.arena.borrow_mut().node_data[self.head].tab_index = Some(tab_index);
    }

    #[inline]
    pub fn add_css_override<S: Into<String>>(&mut self, override_id: S, property: CssProperty) {
        self.arena.borrow_mut().node_data[self.head]
            .dynamic_css_overrides
            .push((override_id.into(), property));
    }

    /// Prints a debug formatted version of the DOM for easier debugging
    pub fn debug_dump(&self) {
        println!("{}", self.arena.borrow().print_tree(|t| format!("{}", t)));
    }

    pub(crate) fn into_ui_state(self) -> UiState<T> {
        // NOTE: Originally it was allowed to create a DOM with
        // multiple root elements using `add_sibling()` and `with_sibling()`.
        //
        // However, it was decided to remove these functions (in commit #586933),
        // as they aren't practical (you can achieve the same thing with one
        // wrapper div and multiple add_child() calls) and they create problems
        // when layouting elements since add_sibling() essentially modifies the
        // space that the parent can distribute, which in code, simply looks weird
        // and led to bugs.
        //
        // It is assumed that the DOM returned by the user has exactly one root node
        // with no further siblings and that the root node is the Node with the ID 0.

        // All nodes that have regular (user-defined) callbacks
        let mut tag_ids_to_callbacks = BTreeMap::new();
        // All nodes that have a default callback
        let mut tag_ids_to_default_callbacks = BTreeMap::new();
        // All tags that have can be focused (necessary for hit-testing)
        let mut tab_index_tags = BTreeMap::new();
        // All tags that have can be dragged & dropped (necessary for hit-testing)
        let mut draggable_tags = BTreeMap::new();

        // Mapping from tags to nodes (necessary so that the hit-testing can resolve the NodeId from any given tag)
        let mut tag_ids_to_node_ids = BTreeMap::new();
        // Mapping from nodes to tags, reverse mapping (not used right now, may be useful in the future)
        let mut node_ids_to_tag_ids = BTreeMap::new();
        // Which nodes have extra dynamic CSS overrides?
        let mut dynamic_css_overrides = BTreeMap::new();

        // Reset the tag
        TAG_ID.swap(1, Ordering::SeqCst);

        {
            let arena = &self.arena.borrow();

            debug_assert!(arena.node_layout[NodeId::new(0)].next_sibling.is_none());

            for node_id in arena.linear_iter() {
                let data = &arena.node_data[node_id];

                let mut node_tag_id = None;

                if !data.callbacks.is_empty() {
                    let tag_id = new_tag_id();
                    tag_ids_to_callbacks.insert(tag_id, data.callbacks.iter().cloned().collect());
                    node_tag_id = Some(tag_id);
                }

                if !data.default_callback_ids.is_empty() {
                    let tag_id = node_tag_id.unwrap_or_else(|| new_tag_id());
                    tag_ids_to_default_callbacks
                        .insert(tag_id, data.default_callback_ids.iter().cloned().collect());
                    node_tag_id = Some(tag_id);
                }

                if data.draggable {
                    let tag_id = node_tag_id.unwrap_or_else(|| new_tag_id());
                    draggable_tags.insert(tag_id, node_id);
                    node_tag_id = Some(tag_id);
                }

                if let Some(tab_index) = data.tab_index {
                    let tag_id = node_tag_id.unwrap_or_else(|| new_tag_id());
                    tab_index_tags.insert(tag_id, (node_id, tab_index));
                    node_tag_id = Some(tag_id);
                }

                if let Some(tag_id) = node_tag_id {
                    tag_ids_to_node_ids.insert(tag_id, node_id);
                    node_ids_to_tag_ids.insert(node_id, tag_id);
                }

                // Collect all the styling overrides into one hash map
                if !data.dynamic_css_overrides.is_empty() {
                    dynamic_css_overrides.insert(
                        node_id,
                        data.dynamic_css_overrides.iter().cloned().collect(),
                    );
                }
            }
        }

        UiState {
            dom: self,
            tag_ids_to_callbacks,
            tag_ids_to_default_callbacks,
            tab_index_tags,
            draggable_tags,
            node_ids_to_tag_ids,
            tag_ids_to_node_ids,
            dynamic_css_overrides,
            tag_ids_to_hover_active_states: BTreeMap::new(),
        }
    }
}

/// OpenGL texture, use `ReadOnlyWindow::create_texture` to create a texture
///
/// **WARNING**: Don't forget to call `ReadOnlyWindow::unbind_framebuffer()`
/// when you are done with your OpenGL drawing, otherwise WebRender will render
/// to the texture, not the window, so your texture will actually never show up.
/// If you use a `Texture` and you get a blank screen, this is probably why.
#[derive(Debug, Clone)]
pub struct Texture {
    pub(crate) inner: Rc<Texture2d>,
}

impl Texture {
    /// Note: You can initialize this texture from an existing (external texture).
    pub fn new(tex: Texture2d) -> Self {
        Self {
            inner: Rc::new(tex),
        }
    }

    /// Prepares the texture for drawing - you can only draw
    /// on a framebuffer, the texture itself is readonly from the
    /// OpenGL drivers point of view.
    ///
    /// **WARNING**: Don't forget to call `ReadOnlyWindow::unbind_framebuffer()`
    /// when you are done with your OpenGL drawing, otherwise WebRender will render
    /// to the texture instead of the window, so your texture will actually
    /// never show up on the screen, since it is never rendered.
    /// If you use a `Texture` and you get a blank screen, this is probably why.
    pub fn as_surface<'a>(&'a self) -> SimpleFrameBuffer<'a> {
        self.inner.as_surface()
    }
}

impl Hash for Texture {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use glium::GlObject;
        self.inner.get_id().hash(state);
    }
}

impl PartialEq for Texture {
    /// Note: Comparison uses only the OpenGL ID, it doesn't compare the
    /// actual contents of the texture.
    fn eq(&self, other: &Texture) -> bool {
        use glium::GlObject;
        self.inner.get_id() == other.inner.get_id()
    }
}

impl Eq for Texture {}

#[test]
fn test_dom_sibling_1() {
    struct TestLayout {}

    impl Layout for TestLayout {
        fn layout(&self) -> Dom<Self> {
            Dom::new(NodeType::Div)
                .with_child(
                    Dom::new(NodeType::Div)
                        .with_id("sibling-1")
                        .with_child(Dom::new(NodeType::Div).with_id("sibling-1-child-1")),
                )
                .with_child(
                    Dom::new(NodeType::Div)
                        .with_id("sibling-2")
                        .with_child(Dom::new(NodeType::Div).with_id("sibling-2-child-1")),
                )
        }
    }

    let dom = TestLayout {}.layout();
    let arena = dom.arena.borrow();

    assert_eq!(NodeId::new(0), dom.root);

    assert_eq!(
        vec![String::from("sibling-1")],
        arena.node_data[arena.node_layout[dom.root]
            .first_child
            .expect("root has no first child")]
        .ids
    );

    assert_eq!(
        vec![String::from("sibling-2")],
        arena.node_data[arena.node_layout[arena.node_layout[dom.root]
            .first_child
            .expect("root has no first child")]
        .next_sibling
        .expect("root has no second sibling")]
        .ids
    );

    assert_eq!(
        vec![String::from("sibling-1-child-1")],
        arena.node_data[arena.node_layout[arena.node_layout[dom.root]
            .first_child
            .expect("root has no first child")]
        .first_child
        .expect("first child has no first child")]
        .ids
    );

    assert_eq!(
        vec![String::from("sibling-2-child-1")],
        arena.node_data[arena.node_layout[arena.node_layout[arena.node_layout[dom.root]
            .first_child
            .expect("root has no first child")]
        .next_sibling
        .expect("first child has no second sibling")]
        .first_child
        .expect("second sibling has no first child")]
        .ids
    );
}

#[test]
fn test_dom_from_iter_1() {
    use id_tree::Node;

    struct TestLayout {}

    impl Layout for TestLayout {
        fn layout(&self) -> Dom<Self> {
            (0..5)
                .map(|e| NodeData::new(NodeType::Label(format!("{}", e + 1))))
                .collect()
        }
    }

    let dom = TestLayout {}.layout();
    let arena = dom.arena.borrow();

    // We need to have 6 nodes:
    //
    // root                 NodeId(0)
    //   |-> 1              NodeId(1)
    //   |-> 2              NodeId(2)
    //   |-> 3              NodeId(3)
    //   |-> 4              NodeId(4)
    //   '-> 5              NodeId(5)

    assert_eq!(arena.len(), 6);

    // Check root node
    assert_eq!(
        arena.node_layout.get(NodeId::new(0)),
        Some(&Node {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            first_child: Some(NodeId::new(1)),
            last_child: Some(NodeId::new(5)),
        })
    );
    assert_eq!(
        arena.node_data.get(NodeId::new(0)),
        Some(&NodeData::new(NodeType::Div))
    );

    assert_eq!(
        arena
            .node_layout
            .get(NodeId::new(arena.node_layout.len() - 1)),
        Some(&Node {
            parent: Some(NodeId::new(0)),
            previous_sibling: Some(NodeId::new(4)),
            next_sibling: None,
            first_child: None,
            last_child: None,
        })
    );
    assert_eq!(
        arena.node_data.get(NodeId::new(arena.node_data.len() - 1)),
        Some(&NodeData {
            node_type: NodeType::Label(String::from("5")),
            ..Default::default()
        })
    );
}

/// Test that there shouldn't be a DOM that has 0 nodes
#[test]
fn test_zero_size_dom() {
    struct TestLayout {}

    impl Layout for TestLayout {
        fn layout(&self) -> Dom<Self> {
            Dom::new(NodeType::Div)
        }
    }

    let mut null_dom = (0..0)
        .map(|_| NodeData {
            node_type: NodeType::Div,
            ..Default::default()
        })
        .collect::<Dom<TestLayout>>();

    assert!(null_dom.arena.borrow().len() == 1);

    null_dom.add_class("hello"); // should not panic
    null_dom.add_id("id-hello"); // should not panic
}
