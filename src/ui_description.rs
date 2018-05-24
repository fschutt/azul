use FastHashMap;
use id_tree::{Arena, NodeId};
use traits::LayoutScreen;
use ui_state::UiState;
use css::Css;
use dom::NodeData;
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::BTreeMap;
use css::CssDeclaration;

pub struct UiDescription<'a, T: LayoutScreen> {
    pub(crate) ui_descr_arena: Rc<RefCell<Arena<NodeData<T>>>>,
    /// ID of the root node of the arena (usually NodeId(0))
    pub(crate) ui_descr_root: Option<NodeId>,
    /// This field is created from the Css parser
    pub(crate) styled_nodes: BTreeMap<NodeId, StyledNode<'a>>,
    /// In the display list, we take references to the `UiDescription.styled_nodes`
    ///
    /// However, if there is no style, we want to have a default style applied
    /// and the reference to that style has to live as least as long as the `self.styled_nodes`
    /// This is why we need this field here
    pub(crate) default_style_of_node: StyledNode<'a>,
}

impl<'a, T: LayoutScreen> Clone for UiDescription<'a, T> {
    fn clone(&self) -> Self {
        Self {
            ui_descr_arena: self.ui_descr_arena.clone(),
            ui_descr_root: self.ui_descr_root.clone(),
            styled_nodes: self.styled_nodes.clone(),
            default_style_of_node: self.default_style_of_node.clone(),
        }
    }
}

impl<'a, T: LayoutScreen> Default for UiDescription<'a, T> {
    fn default() -> Self {
        Self {
            ui_descr_arena: Rc::new(RefCell::new(Arena::new())),
            ui_descr_root: None,
            styled_nodes: BTreeMap::new(),
            default_style_of_node: StyledNode::default(),
        }
    }
}

impl<'a, T: LayoutScreen> UiDescription<'a, T> {
    pub fn from_ui_state(ui_state: &UiState<T>, style: &'a mut Css<'a>) -> Self
    {
        T::style_dom(&ui_state.dom, style)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct StyledNode<'a> {
    /// The z-index level that we are currently on, 0 by default
    pub(crate) z_level: u32,
    /// The CSS constraints, after the cascading step
    pub(crate) css_constraints: CssConstraintList<'a>
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct CssConstraintList<'a> {
    pub(crate) list: Vec<CssDeclaration<'a>>
}