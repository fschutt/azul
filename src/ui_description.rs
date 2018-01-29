use kuchiki::NodeRef;
use FastHashMap;

#[derive(Debug, Default, Clone)]
pub struct UiDescription {
    pub styled_nodes: Vec<StyledNode>,
}

#[derive(Debug, Clone)]
pub struct StyledNode {
    /// The current node we are processing (the current HTML element)
    pub node: NodeRef,
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