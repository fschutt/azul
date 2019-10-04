
use std::collections::BTreeMap;
use crate::{
    RectContent, Style,
    anon::AnonDom,
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeHierarchy, NodeDataContainer, NodeDepths, NodeId},
    ui_solver::{PositionedRectangle, ResolvedOffsets},
};
use azul_css::{LayoutSize, LayoutPoint, LayoutRect, Overflow};

pub(crate) fn compute<T: GetTextLayout>(
    _root_id: NodeId,
    _root_size: LayoutSize,
    node_hierarchy: &NodeHierarchy,
    node_styles: &NodeDataContainer<Style>,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    anon_dom: &AnonDom,
) -> NodeDataContainer<PositionedRectangle> {

    let solved_nodes = NodeDataContainer::new(vec![PositionedRectangle {
        bounds: LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(100.0, 100.0)),
        padding: ResolvedOffsets::zero(),
        margin: ResolvedOffsets::zero(),
        border_widths: ResolvedOffsets::zero(),
        content_size: None,
        resolved_text_layout_options: None,
        overflow: Overflow::Scroll,
    }; anon_dom.anon_node_hierarchy.len()]);

    NodeDataContainer::new(
        anon_dom.original_node_id_mapping.iter().map(|(original_node_id, anon_node_id)| {
            solved_nodes[*anon_node_id].clone()
        }).collect()
    )
}