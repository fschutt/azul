use std::{
    fmt,
    rc::Rc,
    cell::RefCell,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicUsize, Ordering},
    collections::BTreeMap,
};
use webrender::api::ColorU;
use glium::{Texture2d, framebuffer::SimpleFrameBuffer};
use {
    window::WindowEvent,
    images::ImageId,
    cache::DomHash,
    text_cache::TextId,
    traits::Layout,
    app_state::AppState,
    id_tree::{NodeId, Arena},
};

/// This is only accessed from the main thread, so it's safe to use
pub(crate) static NODE_ID: AtomicUsize = AtomicUsize::new(0);
pub(crate) static CALLBACK_ID: AtomicUsize = AtomicUsize::new(0);

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

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `UpdateScreen` that denotes if the screen should be redrawn.
/// The CSS is not affected by this, so if you push to the windows' CSS inside the
/// function, the screen will not be automatically redrawn, unless you return an
/// `UpdateScreen::Redraw` from the function
pub struct Callback<T: Layout>(pub fn(&mut AppState<T>, WindowEvent) -> UpdateScreen);

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
  fn hash<H>(&self, state: &mut H) where H: Hasher {
    state.write_usize(self.0 as usize);
  }
}

/// Basically compares the function pointers and types for equality
impl<T: Layout> PartialEq for Callback<T> {
  fn eq(&self, rhs: &Self) -> bool {
    self.0 as usize == rhs.0 as usize
  }
}

impl<T: Layout> Eq for Callback<T> { }

impl<T: Layout> Copy for Callback<T> { }

/// List of core DOM node types built-into by `azul`.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum NodeType {
    /// Regular div with no particular type of data attached
    Div,
    /// A small label that can be (optionally) be selectable with the mouse
    Label(String),
    /// Larger amount of text, that has to be cached
    Text(TextId),
    /// An image that is rendered by webrender. The id is aquired by the
    /// `AppState::add_image()` function
    Image(ImageId),
    /// OpenGL texture. The `Svg` widget deserizalizes itself into a texture
    /// Equality and Hash values are only checked by the OpenGl texture ID,
    /// azul does not check that the contents of two textures are the same
    GlTexture(Texture),
}

impl NodeType {
    pub(crate) fn get_css_id(&self) -> &'static str {
        use self::NodeType::*;
        match self {
            Div => "div",
            Label(_) | Text(_) => "p",
            Image(_) => "image",
            GlTexture(_) => "texture",
        }
    }
}

/// OpenGL texture, use `ReadOnlyWindow::create_texture` to create a texture
///
/// **WARNING**: Don't forget to call `ReadOnlyWindow::unbind_framebuffer()`
/// when you are done with your OpenGL drawing, otherwise webrender will render
/// to the texture, not the window, so your texture will actually never show up.
/// If you use a `Texture` and you get a blank screen, this is probably why.
#[derive(Debug, Clone)]
pub struct Texture {
    pub(crate) inner: Rc<Texture2d>,
}

impl Texture {
    pub(crate) fn new(tex: Texture2d) -> Self {
        Self {
            inner: Rc::new(tex),
        }
    }

    /// Prepares the texture for drawing - you can only draw
    /// on a framebuffer, the texture itself is readonly from the
    /// OpenGL drivers point of view.
    ///
    /// **WARNING**: Don't forget to call `ReadOnlyWindow::unbind_framebuffer()`
    /// when you are done with your OpenGL drawing, otherwise webrender will render
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

impl Eq for Texture { }

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
}

#[derive(PartialEq, Eq)]
pub(crate) struct NodeData<T: Layout> {
    /// `div`
    pub node_type: NodeType,
    /// `#main`
    pub id: Option<String>,
    /// `.myclass .otherclass`
    pub classes: Vec<String>,
    /// `onclick` -> `my_button_click_handler`
    pub events: CallbackList<T>,
    /// Tag for hit-testing
    pub tag: Option<u64>,
}

impl<T: Layout> Hash for NodeData<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.id.hash(state);
        for class in &self.classes {
            class.hash(state);
        }
        self.events.hash(state);
    }
}

impl<T: Layout> NodeData<T> {
    pub fn calculate_node_data_hash(&self) -> DomHash {
        use std::hash::Hash;
        use twox_hash::XxHash;
        let mut hasher = XxHash::default();
        self.hash(&mut hasher);
        DomHash(hasher.finish())
    }
}

impl<T: Layout> Clone for NodeData<T> {
    fn clone(&self) -> Self {
        Self {
            node_type: self.node_type.clone(),
            id: self.id.clone(),
            classes: self.classes.clone(),
            events: self.events.special_clone(),
            tag: self.tag.clone(),
        }
    }
}

impl<T: Layout> fmt::Debug for NodeData<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "NodeData {{ \
                \tnode_type: {:?}, \
                \tid: {:?}, \
                \tclasses: {:?}, \
                \tevents: {:?}, \
                \ttag: {:?} \
            }}",
        self.node_type,
        self.id,
        self.classes,
        self.events,
        self.tag)
    }
}

impl<T: Layout> CallbackList<T> {
    fn special_clone(&self) -> Self {
        Self {
            callbacks: self.callbacks.clone(),
        }
    }
}

impl<T: Layout> NodeData<T> {
    /// Creates a new NodeData
    pub fn new(node_type: NodeType) -> Self {
        Self {
            node_type: node_type,
            id: None,
            classes: Vec::new(),
            events: CallbackList::<T>::new(),
            tag: None,
        }
    }

    /// Since `#[derive(Clone)]` requires `T: Clone`, we currently
    /// have to make our own version
    fn special_clone(&self) -> Self {
        Self {
            node_type: self.node_type.clone(),
            id: self.id.clone(),
            classes: self.classes.clone(),
            events: self.events.special_clone(),
            tag: self.tag.clone(),
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
        write!(f,
        "Dom {{ \
            \tarena: {:?}, \
            \troot: {:?}, \
            \thead: {:?}, \
        }}",
        self.arena,
        self.root,
        self.head)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CallbackList<T: Layout> {
    pub(crate) callbacks: BTreeMap<On, Callback<T>>
}

impl<T: Layout> Hash for CallbackList<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for callback in &self.callbacks {
            callback.hash(state);
        }
    }
}

impl<T: Layout> fmt::Debug for CallbackList<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackList (length: {:?})", self.callbacks.len())
    }
}

impl<T: Layout> CallbackList<T> {
    pub fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }
}

impl<T: Layout> Dom<T> {

    /// Creates an empty DOM
    #[inline]
    pub fn new(node_type: NodeType) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(NodeData::new(node_type));
        Self {
            arena: Rc::new(RefCell::new(arena)),
            root: root,
            head: root,
        }
    }

    /// Adds a sibling to the current DOM
    pub fn add_sibling(&mut self, sibling: Self) {
        use id_tree::Node;

        let self_len = self.arena.borrow().nodes_len();
        let sibling_len = sibling.arena.borrow().nodes_len();

        let mut self_arena = self.arena.borrow_mut();
        let mut sibling_arena = sibling.arena.borrow_mut();

        for node_id in 0..sibling_len {

            let node: &mut Node<NodeData<T>> = &mut sibling_arena[NodeId::new(node_id)];

            {
                let mut b_node_parent_is_some = false;
                if let Some(parent) = node.parent_mut() {
                    *parent = *parent + self_len;
                    b_node_parent_is_some = true;
                }
                if !b_node_parent_is_some {
                    node.parent = self_arena[self.head].parent;
                }
            }

            {
                let mut b_node_previous_sibling_is_some = false;
                if let Some(previous_sibling) = node.previous_sibling_mut() {
                    *previous_sibling = *previous_sibling + self_len;
                    b_node_previous_sibling_is_some = true;
                }
                if !b_node_previous_sibling_is_some {
                    node.previous_sibling = Some(self.head);
                }
            }

            if let Some(next_sibling) = node.next_sibling_mut() {
                *next_sibling = *next_sibling + self_len;
            }

            if let Some(first_child) = node.first_child_mut() {
                *first_child = *first_child + self_len;
            }

            if let Some(last_child) = node.last_child_mut() {
                *last_child = *last_child + self_len;
            }
        }

        let head_node_id = NodeId::new(self_len);
        self_arena[self.head].next_sibling = Some(head_node_id);
        self.head = head_node_id;
        (&mut *self_arena).append(&mut sibling_arena);
    }

    /// Adds a child DOM to the current DOM
    pub fn add_child(&mut self, child: Self) {

        use id_tree::Node;

        let self_len = self.arena.borrow().nodes_len();
        let child_len = child.arena.borrow().nodes_len();

        let mut self_arena = self.arena.borrow_mut();
        let mut child_arena = child.arena.borrow_mut();

        let mut last_sibling = None;

        for node_id in 0..child_len {
            let node_id = NodeId::new(node_id);
            let node: &mut Node<NodeData<T>> = &mut child_arena[node_id];

            // WARNING: Order of these blocks is important!
            {
                let mut b_node_previous_sibling_is_some = false;
                if let Some(previous_sibling) = node.previous_sibling_mut() {
                    *previous_sibling = *previous_sibling + self_len;
                    b_node_previous_sibling_is_some = true;
                }
                if !b_node_previous_sibling_is_some {
                    let last_child = self_arena[self.head].last_child;
                    if last_child.is_some() && node.parent.is_none() {
                        node.previous_sibling = last_child;
                        self_arena[last_child.unwrap()].next_sibling = Some(node_id + self_len);
                    }
                }
            }

            {
                let mut b_node_parent_is_some = false;
                if let Some(parent) = node.parent_mut() {
                    *parent = *parent + self_len;
                    b_node_parent_is_some = true;
                }
                if !b_node_parent_is_some {
                    if node.next_sibling.is_none() {
                        // We have encountered the last root item
                        last_sibling = Some(node_id);
                    }
                    node.parent = Some(self.head);
                }
            }

            if let Some(next_sibling) = node.next_sibling_mut() {
                *next_sibling = *next_sibling + self_len;
            }

            if let Some(first_child) = node.first_child_mut() {
                *first_child = *first_child + self_len;
            }

            if let Some(last_child) = node.last_child_mut() {
                *last_child = *last_child + self_len;
            }
        }

        self_arena[self.head].first_child.get_or_insert(NodeId::new(self_len));
        self_arena[self.head].last_child = Some(last_sibling.unwrap() + self_len);
        (&mut *self_arena).append(&mut child_arena);
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_id<S: Into<String>>(mut self, id: S) -> Self {
        self.set_id(id);
        self
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_class<S: Into<String>>(mut self, class: S) -> Self {
        self.set_class(class);
        self
    }

    /// Same as `event`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_callback(mut self, on: On, callback: Callback<T>) -> Self {
        self.set_callback(on, callback);
        self
    }

    #[inline]
    pub fn with_child(mut self, child: Self) -> Self {
        self.add_child(child);
        self
    }

    #[inline]
    pub fn with_sibling(mut self, sibling: Self) -> Self {
        self.add_sibling(sibling);
        self
    }

    #[inline]
    pub fn set_id<S: Into<String>>(&mut self, id: S) {
        self.arena.borrow_mut()[self.head].data.id = Some(id.into());
    }

    #[inline]
    pub fn set_class<S: Into<String>>(&mut self, class: S) {
        self.arena.borrow_mut()[self.head].data.classes.push(class.into());
    }

    #[inline]
    pub fn set_callback(&mut self, on: On, callback: Callback<T>) {
        self.arena.borrow_mut()[self.head].data.events.callbacks.insert(on, callback);
        self.arena.borrow_mut()[self.head].data.tag = Some(NODE_ID.fetch_add(1, Ordering::SeqCst) as u64);
    }
}

impl<T: Layout> Dom<T> {

    pub(crate) fn collect_callbacks(
        &self,
        callback_list: &mut BTreeMap<u64, Callback<T>>,
        nodes_to_callback_id_list: &mut  BTreeMap<u64, BTreeMap<On, u64>>)
    {
        for item in self.root.traverse(&*self.arena.borrow()) {
            let mut cb_id_list = BTreeMap::<On, u64>::new();
            let item = &self.arena.borrow()[item.inner_value()];
            for (on, callback) in item.data.events.callbacks.iter() {
                let callback_id = CALLBACK_ID.fetch_add(1, Ordering::SeqCst) as u64;
                callback_list.insert(callback_id, *callback);
                cb_id_list.insert(*on, callback_id);
            }
            if let Some(tag) = item.data.tag {
                nodes_to_callback_id_list.insert(tag, cb_id_list);
            }
        }
    }
}

#[test]
fn test_dom_sibling_1() {

    use window::WindowInfo;

    struct TestLayout { }

    impl Layout for TestLayout {
        fn layout(&self) -> Dom<Self> {
            Dom::new(NodeType::Div)
                .with_child(
                    Dom::new(NodeType::Div)
                    .with_id("sibling-1")
                    .with_child(Dom::new(NodeType::Div)
                        .with_id("sibling-1-child-1")))
                .with_child(Dom::new(NodeType::Div)
                    .with_id("sibling-2")
                    .with_child(Dom::new(NodeType::Div)
                        .with_id("sibling-2-child-1")))
        }
    }

    let dom = TestLayout{ }.layout();
    let arena = dom.arena.borrow();

    assert_eq!(NodeId::new(0), dom.root);

    assert_eq!(Some(String::from("sibling-1")),
        arena[
            arena[dom.root]
            .first_child().expect("root has no first child")
        ].data.id);

    assert_eq!(Some(String::from("sibling-2")),
        arena[
            arena[
                arena[dom.root]
                .first_child().expect("root has no first child")
            ].next_sibling().expect("root has no second sibling")
        ].data.id);

    assert_eq!(Some(String::from("sibling-1-child-1")),
        arena[
            arena[
                arena[dom.root]
                .first_child().expect("root has no first child")
            ].first_child().expect("first child has no first child")
        ].data.id);

    assert_eq!(Some(String::from("sibling-2-child-1")),
        arena[
            arena[
                arena[
                    arena[dom.root]
                    .first_child().expect("root has no first child")
                ].next_sibling().expect("first child has no second sibling")
            ].first_child().expect("second sibling has no first child")
        ].data.id);
}