use FastHashMap;
use id_tree::{Arena, NodeId};
use traits::LayoutScreen;
use ui_state::UiState;
use css::Css;
use dom::NodeData;

#[derive(Clone)]
pub struct UiDescription<'a, T: LayoutScreen + 'a> {
    pub arena: Option<&'a Arena<NodeData<T>>>,
    pub styled_nodes: Vec<StyledNode>,
}

impl<'a, T: LayoutScreen> Default for UiDescription<'a, T> {
    fn default() -> Self {
        Self {
            arena: None,
            styled_nodes: Vec::new(),
        }
    }
}

impl<'a, T: LayoutScreen> UiDescription<'a, T> {
    pub fn from_ui_state(ui_state: &'a UiState<T>, style: &mut Css) -> Self
    {
        T::style_dom(&ui_state.dom, style)
    }
}

#[derive(Debug, Clone)]
pub struct StyledNode {
    /// The current node we are processing (the current HTML element)
    pub id: NodeId,
    /// The z-index level that we are currently on
    pub z_level: u32,
    /// The CSS constraints, after the cascading step
    pub css_constraints: CssConstraintList
}

#[derive(Debug, Clone)]
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