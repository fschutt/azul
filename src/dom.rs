use traits::GetCssId;
use app_state::AppState;
use traits::LayoutScreen;
use std::collections::BTreeMap;
use id_tree::{NodeId, Arena};
use std::sync::{Arc, Mutex};
use std::fmt;
use std::rc::Rc;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use webrender::api::ColorU;
use glium::Texture2d;
use svg::Svg;

/// This is only accessed from the main thread, so it's safe to use
pub(crate) static mut NODE_ID: u64 = 0;
pub(crate) static mut CALLBACK_ID: u64 = 0;

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
pub struct Callback<T: LayoutScreen>(pub fn(&mut AppState<T>) -> UpdateScreen);

impl<T: LayoutScreen> fmt::Debug for Callback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Callback @ {:x?}", self.0 as usize)
    }
}

impl<T: LayoutScreen> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Callback(self.0.clone())
    }
}

/// As a hashing function, we use the function pointer casted to a usize
/// as a unique ID for the function. This way, we can hash and compare DOM nodes
/// (to create diffs between two states). Comparing usizes is more efficient
/// than re-creating the whole DOM and serves as a caching mechanism.
impl<T: LayoutScreen> Hash for Callback<T> {
  fn hash<H>(&self, state: &mut H) where H: Hasher {
    state.write_usize(self.0 as usize);
  }
}

/// Basically compares the function pointers and types for equality
impl<T: LayoutScreen> PartialEq for Callback<T> {
  fn eq(&self, rhs: &Self) -> bool {
    self.0 as usize == rhs.0 as usize
  }
}

impl<T: LayoutScreen> Eq for Callback<T> { }

impl<T: LayoutScreen> Copy for Callback<T> { }

use traits::Widget;

/// List of allowed DOM node types that are supported by `azul`.
///
/// All node types are purely convenience functions around `Div`,
/// `Image` and `Label`. For example a `Ul` is simply a convenience
/// wrapper around a repeated (`Div` + `Label`) clone where the first
/// `Div` is shaped like a circle (for `Ul`).
pub enum NodeType<T: LayoutScreen> {
    /// Regular div
    Div,
    /// A label that can be (optionally) be selectable with the mouse
    Label(String),
    Svg(Svg<T>),
    // GlTexture
}

impl<T: LayoutScreen> fmt::Debug for NodeType<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NodeType<{}>", self.get_css_id())
    }
}

impl<T: LayoutScreen> Clone for NodeType<T> {
    fn clone(&self) -> Self {
        use self::NodeType::*;
        match self {
            Div => Div,
            Label(text) => Label(text.clone()),
            Svg(svg) => Svg(svg.clone()),
        }
    }
}

impl<T: LayoutScreen> PartialEq for NodeType<T> {
    fn eq(&self, rhs: &Self) -> bool {
        use self::NodeType::*;
        match (self, rhs) {
            (Div, Div) => true,
            (Label(a), Label(b)) => a == b,
            (Svg(a), Svg(b)) => *a == *b,
            _ => false,
        }
    }
}

impl<T: LayoutScreen> Eq for NodeType<T> { }

impl<T: LayoutScreen> Hash for NodeType<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use self::NodeType::*;
        match self {
            Div => 0.hash(state),
            Label(l) => { 1.hash(state); l.hash(state) },
            Svg(s) => { 2.hash(state); s.hash(state) },
        }
    }
}

impl<T: LayoutScreen> GetCssId for NodeType<T> {
    fn get_css_id(&self) -> &'static str {
        use self::NodeType::*;
        match *self {
            Div => "div",
            Label(_) => "p",
            Svg(_) => "svg",
        }
    }
}

/// State of a checkbox (disabled, checked, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum CheckboxState {
    /// `[■]`
    Active,
    /// `[✔]`
    Checked,
    /// Greyed out checkbox
    Disabled {
        /// Should the checkbox fire on a mouseover / mouseup, etc. event
        ///
        /// This can be useful for showing warnings / tooltips / help messages
        /// as to why this checkbox is disabled
        fire_on_click: bool,
    },
    /// `[ ]`
    Unchecked
}

/// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum On {
    /// Mouse cursor is hovering over the element
    MouseOver,
    /// Mouse cursor has is over element and is pressed
    /// (not good for "click" events - use `MouseUp` instead)
    MouseDown,
    /// Mouse button has been released while cursor was over the element
    MouseUp,
    /// Mouse cursor has entered the element
    MouseEnter,
    /// Mouse cursor has left the element
    MouseLeave,
}

#[derive(PartialEq, Eq)]
pub(crate) struct NodeData<T: LayoutScreen> {
    /// `div`
    pub node_type: NodeType<T>,
    /// `#main`
    pub id: Option<String>,
    /// `.myclass .otherclass`
    pub classes: Vec<String>,
    /// `onclick` -> `my_button_click_handler`
    pub events: CallbackList<T>,
    /// Tag for hit-testing
    pub tag: Option<u64>,
}

impl<T: LayoutScreen> Hash for NodeData<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.id.hash(state);
        for class in &self.classes {
            class.hash(state);
        }
        self.events.hash(state);
    }
}

use cache::DomHash;

impl<T: LayoutScreen> NodeData<T> {
    pub fn calculate_node_data_hash(&self) -> DomHash {
        use std::hash::Hash;
        use twox_hash::XxHash;
        let mut hasher = XxHash::default();
        self.hash(&mut hasher);
        DomHash(hasher.finish())
    }
}

impl<T: LayoutScreen> Clone for NodeData<T> {
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

impl<T: LayoutScreen> fmt::Debug for NodeData<T> {
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

impl<T: LayoutScreen> CallbackList<T> {
    fn special_clone(&self) -> Self {
        Self {
            callbacks: self.callbacks.clone(),
        }
    }
}

impl<T: LayoutScreen> NodeData<T> {
    /// Creates a new NodeData
    pub fn new(node_type: NodeType<T>) -> Self {
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
pub struct Dom<T: LayoutScreen> {
    pub(crate) arena: Rc<RefCell<Arena<NodeData<T>>>>,
    pub(crate) root: NodeId,
    pub(crate) current_root: NodeId,
    pub(crate) last: NodeId,
}

impl<T: LayoutScreen> fmt::Debug for Dom<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
        "Dom {{ \
            \tarena: {:?}, \
            \troot: {:?}, \
            \tcurrent_root: {:?}, \
            \tlast: {:?}, \
        }}",
        self.arena,
        self.root,
        self.current_root,
        self.last)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CallbackList<T: LayoutScreen> {
    pub(crate) callbacks: BTreeMap<On, Callback<T>>
}

impl<T: LayoutScreen> Hash for CallbackList<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for callback in &self.callbacks {
            callback.hash(state);
        }
    }
}

impl<T: LayoutScreen> fmt::Debug for CallbackList<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackList (length: {:?})", self.callbacks.len())
    }
}

impl<T: LayoutScreen> CallbackList<T> {
    pub fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }
}

impl<T: LayoutScreen> Dom<T> {

    /// Creates an empty DOM
    #[inline]
    pub fn new(node_type: NodeType<T>) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(NodeData::new(node_type));
        Self {
            arena: Rc::new(RefCell::new(arena)),
            root: root,
            current_root: root,
            last: root,
        }
    }

    /// Adds a child DOM to the current DOM
    #[inline]
    pub fn add_child(&mut self, child: Self) {
        for ch in child.root.children(&*child.arena.borrow()) {
            let new_last = (*self.arena.borrow_mut()).new_node((*child.arena.borrow())[ch].data.special_clone());
            self.last.append(new_last, &mut self.arena.borrow_mut());
            self.last = new_last;
        }
    }

    /// Adds a sibling to the current DOM
    #[inline]
    pub fn add_sibling(&mut self, sibling: Self) {
        for sib in sibling.root.following_siblings(&*sibling.arena.borrow()) {
            let sibling_clone = (*sibling.arena.borrow())[sib].data.special_clone();
            let new_sibling = (*self.arena.borrow_mut()).new_node(sibling_clone);
            self.current_root.insert_after(new_sibling, &mut self.arena.borrow_mut());
            self.current_root = new_sibling;
        }
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_id<S: Into<String>>(mut self, id: S) -> Self {
        self.id(id);
        self
    }

    /// Same as `id`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_class<S: Into<String>>(mut self, class: S) -> Self {
        self.class(class);
        self
    }

    /// Same as `event`, but easier to use for method chaining in a builder-style pattern
    #[inline]
    pub fn with_event(mut self, on: On, callback: Callback<T>) -> Self {
        self.event(on, callback);
        self
    }

    #[inline]
    pub fn id<S: Into<String>>(&mut self, id: S) {
        self.arena.borrow_mut()[self.last].data.id = Some(id.into());
    }

    #[inline]
    pub fn class<S: Into<String>>(&mut self, class: S) {
        self.arena.borrow_mut()[self.last].data.classes.push(class.into());
    }

    #[inline]
    pub fn event(&mut self, on: On, callback: Callback<T>) {
        self.arena.borrow_mut()[self.last].data.events.callbacks.insert(on, callback);
        self.arena.borrow_mut()[self.last].data.tag = Some(unsafe { NODE_ID });
        unsafe { NODE_ID += 1; };
    }
}

impl<T: LayoutScreen> Dom<T> {

    pub(crate) fn collect_callbacks(
        &self,
        callback_list: &mut BTreeMap<u64, Callback<T>>,
        nodes_to_callback_id_list: &mut  BTreeMap<u64, BTreeMap<On, u64>>)
    {
        for item in self.root.traverse(&*self.arena.borrow()) {
            let mut cb_id_list = BTreeMap::<On, u64>::new();
            let item = &self.arena.borrow()[item.inner_value()];
            for (on, callback) in item.data.events.callbacks.iter() {
                let callback_id = unsafe { CALLBACK_ID };
                unsafe { CALLBACK_ID += 1; }
                callback_list.insert(callback_id, *callback);
                cb_id_list.insert(*on, callback_id);
            }
            if let Some(tag) = item.data.tag {
                nodes_to_callback_id_list.insert(tag, cb_id_list);
            }
        }
    }
}