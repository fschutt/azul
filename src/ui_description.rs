use FastHashMap;
use id_tree::{Arena, NodeId};
use traits::LayoutScreen;
use ui_state::UiState;
use css::Css;
use dom::NodeData;
use std::cell::RefCell;
use std::rc::Rc;

pub struct UiDescription<T: LayoutScreen> {
    pub(crate) ui_descr_arena: Rc<RefCell<Arena<NodeData<T>>>>,
    pub(crate) ui_descr_root: Option<NodeId>,
    pub(crate) styled_nodes: Vec<StyledNode>,
}

impl<T: LayoutScreen> Clone for UiDescription<T> {
    fn clone(&self) -> Self {
        Self {
            ui_descr_arena: self.ui_descr_arena.clone(),
            ui_descr_root: self.ui_descr_root.clone(),
            styled_nodes: self.styled_nodes.clone(),
        }
    }
}

impl<T: LayoutScreen> Default for UiDescription<T> {
    fn default() -> Self {
        Self {
            ui_descr_arena: Rc::new(RefCell::new(Arena::new())),
            ui_descr_root: None,
            styled_nodes: Vec::new(),
        }
    }
}

impl<T: LayoutScreen> UiDescription<T> {
    pub fn from_ui_state(ui_state: &UiState<T>, style: &mut Css) -> Self
    {
        T::style_dom(&ui_state.dom, style)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledNode {
    /// The current node we are processing (the current HTML element)
    pub id: NodeId,
    /// The z-index level that we are currently on
    pub z_level: u32,
    /// The CSS constraints, after the cascading step
    pub css_constraints: CssConstraintList
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssConstraintList {
    pub list: FastHashMap<String, String>
}

impl CssConstraintList {
    pub fn empty() -> Self {
        Self {
            list: FastHashMap::default(),
        }
    }
}