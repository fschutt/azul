pub use kuchiki::NodeRef;
use markup5ever::{LocalName, QualName};
use app_state::AppState;
use traits::LayoutScreen;
use std::collections::BTreeMap;

use webrender::api::ItemTag;

/// This is only accessed from the main thread, so it's safe to use
pub(crate) static mut NODE_ID: u64 = 0;
pub(crate) static mut CALLBACK_ID: u64 = 0;

pub(crate) const HTML_CLASS: QualName = QualName { prefix: None, ns: ns!(html), local: local_name!("class") };
pub(crate) const HTML_ID: QualName = QualName { prefix: None, ns: ns!(html), local: local_name!("id") };
/// This an obscure ID to store the node ID which we need later on. `kuchiki` only allows to store LocalNames alongside
/// a node ID, so this is an (awful) hack to get this done.
pub(crate) const HTML_NODE_ID: QualName = QualName { prefix: None, ns: ns!(html), local: local_name!("actiontype") };

/// List of allowed DOM node types
///
/// The reason for this is because the markup5ever crate has
/// special macros for these node types, so either I need to expose the
/// whole markup5ever crate to the end user or I need to build a
/// wrapper type
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

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum On {
    MouseOver,
    MouseDown,
    MouseUp,
    MouseEnter,
    MouseLeave,
    DragDrop,
}

impl Into<LocalName> for On {
    fn into(self) -> LocalName {
        use self::On::*;
        match self {
            MouseOver => local_name!("onmouseover"),
            MouseDown => local_name!("onmousedown"),
            MouseUp => local_name!("onmouseup"),
            MouseEnter => local_name!("onmouseenter"),
            MouseLeave => local_name!("onmouseleave"),
            DragDrop => local_name!("ondragdrop"),
        }
    }
}

impl Into<LocalName> for NodeType {
    fn into(self) -> LocalName {
        use self::NodeType::*;
        match self {
            Div => local_name!("div"),
            Button => local_name!("button"),
            Ul => local_name!("ul"),
            Li => local_name!("li"),
            Ol =>local_name!("ol"),
            Label => local_name!("label"),
            Input => local_name!("input"),
            Form => local_name!("form"),
            Text { .. } => local_name!("p"),
        }
    }
}

pub struct DomNode<T: LayoutScreen> {
    /// `div`
    pub node_type: NodeType,
    /// `#main`
    pub id: Option<String>,
    /// `.myclass .otherclass`
    pub classes: Vec<String>,
    /// `onclick` -> `my_button_click_handler`
    pub events: CallbackList<T>,
    /// Immediate children of this node
    pub children: Vec<DomNode<T>>,
}

pub struct CallbackList<T: LayoutScreen> {
    pub(crate) callbacks: BTreeMap<On, fn(&mut AppState<T>) -> ()>
}

impl<T: LayoutScreen> CallbackList<T> {
    pub fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }
}

pub struct WebRenderIdList {
    /// Node tag -> List of callback IDs
    pub(crate) callbacks: Option<(ItemTag, BTreeMap<On, u64>)>,
}

impl WebRenderIdList {
    pub fn new() -> Self {
        Self {
            callbacks: None,
        }
    }
}

pub struct WrCallbackList<T: LayoutScreen> {
    /// callback ID -> function pointer
    pub(crate) callback_list: BTreeMap<u64, fn(&mut AppState<T>) -> ()>,
}

impl<T: LayoutScreen> WrCallbackList<T> {
    pub fn new() -> Self {
        Self {
            callback_list: BTreeMap::new(),
        }
    }
}

impl<T: LayoutScreen> DomNode<T> {

    /// Creates an empty node
    pub fn new(node_type: NodeType) -> Self {
        Self {
            node_type: node_type,
            id: None,
            classes: Vec::new(),
            events: CallbackList::new(),
            children: Vec::new(),
        }
    }

    #[inline]
    pub fn id<S: Into<String>>(mut self, id: S) -> Self {
        self.id = Some(id.into());
        self
    }

    #[inline]
    pub fn class<S: Into<String>>(mut self, class: S) -> Self {
        self.classes.push(class.into());
        self
    }

    #[inline]
    pub fn event(mut self, on: On, callback: fn(&mut AppState<T>) -> ()) -> Self {
        self.events.callbacks.insert(on, callback);
        self
    }

    #[inline]
    pub fn add_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    pub(crate) fn into_node_ref(self, callback_list: &mut WrCallbackList<T>, nodes_to_callback_id_list: &mut BTreeMap<ItemTag, BTreeMap<On, u64>>) -> NodeRef {

        use std::cell::RefCell;
        use std::collections::HashMap;
        use kuchiki::{NodeRef, Attributes, NodeData, ElementData};

        let mut event_list = BTreeMap::<On, u64>::new();
        let mut attributes = HashMap::new();

        if let Some(id) = self.id {
            attributes.insert(HTML_ID, id);
        }

        for class in self.classes {
            attributes.insert(HTML_CLASS, class);
        }

        for (key, value) in self.events.callbacks {
            unsafe {
                event_list.insert(key, CALLBACK_ID);
                callback_list.callback_list.insert(CALLBACK_ID, value);
                CALLBACK_ID += 1;
            }
        }

        if !event_list.is_empty() {
            use std::mem::transmute;
            nodes_to_callback_id_list.insert(unsafe { (NODE_ID, 0) }, event_list);
            unsafe { NODE_ID += 1; }
            let bytes: [u8; 8] = unsafe { transmute(NODE_ID.to_be()) };
            let bytes_string = unsafe { String::from_utf8_unchecked(bytes.to_vec()) };
            attributes.insert(HTML_NODE_ID, bytes_string);
        }

        let node = match self.node_type {
            NodeType::Text { content } => {
                NodeData::Text(RefCell::new(content))
            },
            _ => {
                NodeData::Element(ElementData {
                    name: QualName::new(None, ns!(html), self.node_type.into()),
                    attributes: RefCell::new(Attributes { map: attributes }),
                    template_contents: None,
                })
            }
        };

        let node = NodeRef::new(node);

        for child in self.children {
            let child_node = child.into_node_ref(callback_list, nodes_to_callback_id_list);
            node.append(child_node);
        }

        node
    }
}