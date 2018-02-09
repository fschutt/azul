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
/// function, the screen will be redrawn (if necessary).
pub enum Callback<T: LayoutScreen> {
    /// One-off function (for ex. exporting a file)
    ///
    /// This is best for actions that can run in the background
    /// and you don't need to get updates. It uses a background
    /// thread and therefore the data needs to be sendable.
    Async(fn(Arc<Mutex<AppState<T>>>) -> UpdateScreen),
    /// Same as the `FnOnceNonBlocking`, but it blocks the current
    /// thread and does not require the type to be `Send`.
    Sync(fn(&mut AppState<T>) -> UpdateScreen),
}

impl<T: LayoutScreen> fmt::Debug for Callback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Callback::*;
        match *self {
            Async(func) => write!(f, "Callback::Async @ {:?}", func as usize),
            Sync(func) => write!(f, "Callback::Sync @ {:?}", func as usize),
        }
    }
}

impl<T: LayoutScreen> Clone for Callback<T> 
{
    fn clone(&self) -> Self {
        match *self {
            Callback::Async(ref f) => Callback::Async(f.clone()),
            Callback::Sync(ref f) => Callback::Sync(f.clone()),
        }
    }
}

// as a hashing function, we use the function pointer casted to a usize
// as a unique ID to the function. i.e. if a function 
//
// This way, we can hash and compare DOM nodes (to create diffs between two states)
// Comparing usizes is more efficient than re-creating the whole DOM and serves as a 
// caching mechanism.
impl<T: LayoutScreen> Hash for Callback<T> {
  fn hash<H>(&self, state: &mut H) where H: Hasher {
    use self::Callback::*;
    match *self {
        Async(f) => { state.write_usize(f as usize); }
        Sync(f) => { state.write_usize(f as usize); }
    }
  }
}

impl<T: LayoutScreen> PartialEq for Callback<T> {
  fn eq(&self, rhs: &Self) -> bool {
    use self::Callback::*;
    if let (Async(self_f), Async(other_f)) = (*self, *rhs) {
        if self_f as usize == other_f as usize { return true; }
    } else if let (Sync(self_f), Sync(other_f)) = (*self, *rhs) {
        if self_f as usize == other_f as usize { return true; }
    }
    false
  }
}
impl<T: LayoutScreen> Eq for Callback<T> { }
impl<T: LayoutScreen> Copy for Callback<T> { }

/// List of allowed DOM node types that are supported by `azul`.
///
/// The dom type 
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeType {
    /// Regular div
    Div,
    /// Button
    Button {
        /// The text on the button
        label: String,
    },
    /// Unordered list
    Ul,
    /// Ordered list
    Ol,
    /// List item. Only valid if the parent is `NodeType::Ol` or `NodeType::Ul`.
    Li,
    /// A label that can be (optionally) be selectable with the mouse
    Label { 
        /// Text of the label
        text: String,
    },
    /// This is more or less like a `GroupBox` in Visual Basic, draws a border 
    Form {
        /// The text of the label
        text: Option<String>,
    },
    /// Single-line text input
    TextInput { 
        content: String, 
        placeholder: Option<String> 
    },
    /// Multi line text input 
    TextEdit { 
        content: String, 
        placeholder: Option<String>,
    },
    /// A register-like tab
    Tab {
        label: String,
    },
    /// Checkbox
    Checkbox {
        /// active
        state: CheckboxState,
    },
    /// Dropdown item
    Dropdown {
        items: Vec<String>,
    },
    /// Small (default yellow) tooltip for help
    ToolTip {
        title: String,
        content: String,
    },
    /// Password input, like the TextInput, but the items are rendered as dots 
    /// (if `use_dots` is active)
    Password {
        content: String,
        placeholder: Option<String>,
        use_dots: bool,
    },
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

impl NodeType {

    /// Get the CSS / HTML identifier "p", "ul", "li", etc.
    /// 
    /// Full list of the types you can use in CSS:
    /// 
    /// ```ignore
    /// Div         => "div"
    /// Button      => "button"
    /// Ul          => "ul"
    /// Ol          => "ol"
    /// Li          => "li"
    /// Label       => "label"
    /// Form        => "form"
    /// TextInput   => "text-input"
    /// TextEdit    => "text-edit"
    /// Tab         => "tab"
    /// Checkbox    => "checkbox"
    /// Color       => "color"
    /// Drowdown    => "dropdown"
    /// ToolTip     => "tooltip"
    /// Password    => "password"
    /// ```
    pub fn get_css_identifier(&self) -> &'static str {
        use self::NodeType::*;
        match *self {
            Div => "div",
            Button { .. } => "button",
            Ul => "ul",
            Ol => "ol",
            Li => "li",
            Label { .. } => "label",
            Form { .. } => "form",
            TextInput { .. } => "text-input",
            TextEdit { .. } => "text-edit",
            Tab { .. } => "tab",
            Checkbox { .. } => "checkbox",
            Dropdown { .. } => "dropdown",
            ToolTip { .. } => "tooltip",
            Password { .. } => "password",
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
        write!(f, "NodeData {{
    node_type: {:?}, 
    id: {:?}, 
    classes: {:?}, 
    events: {:?}, 
    tag: {:?} 
}}",
        self.node_type, self.id, self.classes, self.events, self.tag)
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
pub struct Dom<T: LayoutScreen> {
    pub(crate) arena: Rc<RefCell<Arena<NodeData<T>>>>,
    pub(crate) root: NodeId,
    pub(crate) current_root: NodeId,
    pub(crate) last: NodeId,
}

impl<T: LayoutScreen> fmt::Debug for Dom<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Dom {{ 
   arena: {:?}, 
   root: {:?}, 
   current_root: {:?},
   last: {:?}
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
    pub fn new(node_type: NodeType) -> Self {
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
    
    pub(crate) fn collect_callbacks(&self, callback_list: &mut BTreeMap<u64, Callback<T>>, nodes_to_callback_id_list: &mut  BTreeMap<u64, BTreeMap<On, u64>>) {
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