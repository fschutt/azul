use std::{
    collections::BTreeMap,
};
use azul_css::{Css, CssDeclaration, CssProperty, CssPropertyType};
use crate::{
    FastHashMap,
    id_tree::{NodeId, NodeDataContainer},
    dom::DomId,
    ui_state::{UiState, HoverGroup},
    callbacks::HitTestItem,
    style::HtmlCascadeInfo,
};

#[derive(Debug)]
pub struct UiDescription {
    /// DOM ID of this arena (so that multiple DOMs / IFrames can be displayed in one window)
    pub dom_id: DomId,
    /// Data necessary for matching nodes properly (necessary to resolve CSS paths in callbacks)
    pub html_tree: NodeDataContainer<HtmlCascadeInfo>,
    /// ID of the root node of the arena (usually NodeId(0))
    pub ui_descr_root: NodeId,
    /// This field is created from the Css
    pub styled_nodes: NodeDataContainer<StyledNode>,
    /// The style properties that should be overridden for this frame, cloned from the `Css`
    pub dynamic_css_overrides: BTreeMap<NodeId, FastHashMap<String, CssProperty>>,
    /// In order to hit-test :hover and :active selectors, need to insert tags for all rectangles
    /// that have a non-:hover path, for example if we have `#thing:hover`, then all nodes selected by `#thing`
    /// need to get a TagId, otherwise, they can't be hit-tested.
    pub selected_hover_nodes: BTreeMap<NodeId, HoverGroup>,
}

impl UiDescription {
    /// Applies the styles to the nodes calculated from the `layout_screen`
    /// function and calculates the final display list that is submitted to the
    /// renderer.
    pub fn new(
        ui_state: &mut UiState,
        style: &Css,
        focused_node: &Option<(DomId, NodeId)>,
        hovered_nodes: &BTreeMap<NodeId, HitTestItem>,
        is_mouse_down: bool,
    ) -> Self {

        let ui_description = crate::style::match_dom_selectors(
            ui_state,
            &style,
            focused_node,
            hovered_nodes,
            is_mouse_down,
        );

        // Important: Create all the tags for the :hover and :active selectors
        ui_state.create_tags_for_hover_nodes(&ui_description.selected_hover_nodes);

        ui_description
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, PartialOrd, Eq, Ord)]
pub struct StyledNode {
    /// The CSS constraints, after the cascading step
    pub css_constraints: BTreeMap<CssPropertyType, CssDeclaration>,
}
