use css_parser::ParsedCssProperty;
use FastHashMap;
use id_tree::{Arena, NodeId};
use traits::Layout;
use ui_state::UiState;
use css::Css;
use dom::NodeData;
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::BTreeMap;
use css::CssDeclaration;

pub struct UiDescription<T: Layout> {
    pub(crate) ui_descr_arena: Rc<RefCell<Arena<NodeData<T>>>>,
    /// ID of the root node of the arena (usually NodeId(0))
    pub(crate) ui_descr_root: Option<NodeId>,
    /// This field is created from the Css parser
    pub(crate) styled_nodes: BTreeMap<NodeId, StyledNode>,
    /// In the display list, we take references to the `UiDescription.styled_nodes`
    ///
    /// However, if there is no style, we want to have a default style applied
    /// and the reference to that style has to live as least as long as the `self.styled_nodes`
    /// This is why we need this field here
    pub(crate) default_style_of_node: StyledNode,
    /// The CSS properties that should be overridden for this frame, cloned from the `Css`
    pub(crate) dynamic_css_overrides: FastHashMap<String, ParsedCssProperty>,
}

impl<T: Layout> Clone for UiDescription<T> {
    fn clone(&self) -> Self {
        Self {
            ui_descr_arena: self.ui_descr_arena.clone(),
            ui_descr_root: self.ui_descr_root.clone(),
            styled_nodes: self.styled_nodes.clone(),
            default_style_of_node: self.default_style_of_node.clone(),
            dynamic_css_overrides: self.dynamic_css_overrides.clone(),
        }
    }
}

impl<T: Layout> Default for UiDescription<T> {
    fn default() -> Self {
        Self {
            ui_descr_arena: Rc::new(RefCell::new(Arena::new())),
            ui_descr_root: None,
            styled_nodes: BTreeMap::new(),
            default_style_of_node: StyledNode::default(),
            dynamic_css_overrides: FastHashMap::default(),
        }
    }
}

impl<T: Layout> UiDescription<T> {
    pub fn from_ui_state(ui_state: &UiState<T>, style: &Css) -> Self
    {
        T::style_dom(&ui_state.dom, style)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct StyledNode {
    /// The z-index level that we are currently on, 0 by default
    pub(crate) z_level: u32,
    /// The CSS constraints, after the cascading step
    pub(crate) css_constraints: CssConstraintList
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct CssConstraintList {
    pub(crate) list: Vec<CssDeclaration>
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_ui_description_file() {

}