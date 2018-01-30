use app_state::AppState;
use traits::LayoutScreen;
use std::collections::BTreeMap;
use id_tree::{NodeId, Children, Arena, FollowingSiblings};
use webrender::api::ItemTag;
use std::sync::{Arc, Mutex};
use std::fmt;

/// This is only accessed from the main thread, so it's safe to use
pub(crate) static mut NODE_ID: u64 = 0;
pub(crate) static mut CALLBACK_ID: u64 = 0;

pub enum Callback<T: LayoutScreen> {
    /// One-off function (for ex. exporting a file)
    ///
    /// This is best for actions that can run in the background
    /// and you don't need to get updates. It uses a background
    /// thread and therefore the data needs to be sendable.
    Async(fn(Arc<Mutex<AppState<T>>>) -> ()),
    /// Same as the `FnOnceNonBlocking`, but it blocks the current
    /// thread and does not require the type to be `Send`.
    Sync(fn(&mut AppState<T>) -> ()),
}

impl<T: LayoutScreen> fmt::Debug for Callback<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Callback::*;
        match *self {
            Async(_) => write!(f, "Callback::Async"),
            Sync(_) => write!(f, "Callback::Sync"),
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

impl<T: LayoutScreen> Copy for Callback<T> { }

/// List of allowed DOM node types
///
/// The reason for this is because the markup5ever crate has
/// special macros for these node types, so either I need to expose the
/// whole markup5ever crate to the end user or I need to build a
/// wrapper type
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeType {
    Div,
    Button,
    Ul,
    Li,
    Ol,
    Label,
    Input,
    Form,
    Text { content: String },
}

impl NodeType {
    pub fn get_css_id(&self) -> &'static str {
        use self::NodeType::*;
        match *self {
            Div => "div",
            Button => "button",
            Ul => "ul",
            Li => "li",
            Ol => "ol",
            Label => "label",
            Input => "input",
            Form => "form",
            Text { .. } => "p",
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum On {
    MouseOver,
    MouseDown,
    MouseUp,
    MouseEnter,
    MouseLeave,
    DragDrop,
}

#[derive(Clone)]
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

impl<T: LayoutScreen> fmt::Debug for NodeData<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NodeData - node_type: {:?}, id: {:?}, classes: {:?}, events: {:?}, tag: {:?} ",
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
    pub fn new(node_type: NodeType) -> Self {
        Self {
            node_type: node_type,
            id: None,
            classes: Vec::new(),
            events: CallbackList::<T>::new(),
            tag: None,
        }
    }

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

#[derive(Clone)]
pub struct Dom<T: LayoutScreen> {
    pub(crate) arena: Arena<NodeData<T>>,
    pub(crate) root: NodeId,
    pub(crate) current_root: NodeId,
    pub(crate) last: NodeId,
}

#[derive(Clone)]
pub struct CallbackList<T: LayoutScreen> {
    pub(crate) callbacks: BTreeMap<On, Callback<T>>
}
impl<T: LayoutScreen> fmt::Debug for CallbackList<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CallbackList")
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
    pub fn new(node_type: NodeType) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(NodeData::new(node_type));
        Self {
            arena: arena,
            root: root,
            current_root: root,
            last: root,
        }
    }

    #[inline]
    pub fn add_child(mut self, child: Self) -> Self {
        for ch in child.root.children(&child.arena) {
            let new_last = self.arena.new_node(child.arena[ch].data.special_clone());
            self.last.append(new_last, &mut self.arena);
            self.last = new_last;
        }
        self
    }

    #[inline]
    pub fn add_sibling(mut self, sibling: Self) -> Self {
        let new_sibling = self.arena.new_node(sibling.arena[sibling.root].data.special_clone());
        self.current_root.append(new_sibling, &mut self.arena);
        self.current_root = new_sibling;
        self
    }

    #[inline]
    pub fn id<S: Into<String>>(mut self, id: S) -> Self {
        self.arena[self.last].data.id = Some(id.into());
        self
    }

    #[inline]
    pub fn class<S: Into<String>>(mut self, class: S) -> Self {
        self.arena[self.last].data.classes.push(class.into());
        self
    }

    #[inline]
    pub fn event(mut self, on: On, callback: Callback<T>) -> Self {
        self.arena[self.last].data.events.callbacks.insert(on, callback);
        self.arena[self.last].data.tag = Some(unsafe { NODE_ID });
        unsafe { NODE_ID += 1; };
        self
    }
}

impl<T: LayoutScreen> Dom<T> {
    
    pub(crate) fn collect_callbacks(&self, callback_list: &mut BTreeMap<u64, Callback<T>>, nodes_to_callback_id_list: &mut  BTreeMap<u64, BTreeMap<On, u64>>) {
        for item in self.root.traverse(&self.arena) {
            let mut cb_id_list = BTreeMap::<On, u64>::new();
            let item = &self.arena[item.inner_value()];
            for (on, callback) in item.data.events.callbacks.iter() {
                let callback_id = unsafe { CALLBACK_ID };
                unsafe { CALLBACK_ID += 1; }
                callback_list.insert(callback_id, *callback);
                cb_id_list.insert(*on, callback_id);
            }
            nodes_to_callback_id_list.insert(item.data.tag.unwrap(), cb_id_list);
        }
    }
}