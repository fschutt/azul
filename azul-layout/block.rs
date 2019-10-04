
use std::collections::BTreeMap;
use crate::{
    RectContent, Style,
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeHierarchy, NodeDataContainer, NodeDepths, NodeId},
    ui_solver::{PositionedRectangle, ResolvedOffsets},
};
use azul_css::{LayoutSize, LayoutPoint, LayoutRect, Overflow};

pub(crate) fn compute<T: GetTextLayout>(
    _root_id: NodeId,
    node_hierarchy: &NodeHierarchy,
    node_styles: &NodeDataContainer<Style>,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    _root_size: LayoutSize,
    node_depths: &NodeDepths,
) -> NodeDataContainer<PositionedRectangle> {

    use crate::anon::AnonDom;

    let anon_dom = AnonDom::new(
        node_hierarchy,
        node_styles,
        node_depths,
        rect_contents,
    );

    println!("{:#?}", anon_dom);

    NodeDataContainer::new(vec![PositionedRectangle {
        bounds: LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(100.0, 100.0)),
        padding: ResolvedOffsets::zero(),
        margin: ResolvedOffsets::zero(),
        border_widths: ResolvedOffsets::zero(),
        content_size: None,
        resolved_text_layout_options: None,
        overflow: Overflow::Scroll,
    }; node_hierarchy.len()])

/*
    /// Outer bounds of the rectangle
    pub bounds: LayoutRect,
    /// Padding of the rectangle
    pub padding: ResolvedOffsets,
    /// Margin of the rectangle
    pub margin: ResolvedOffsets,
    /// Border widths of the rectangle
    pub border_widths: ResolvedOffsets,
    /// Size of the content, for example if a div contains an image or text,
    /// that image or the text block can be bigger than the actual rect
    pub content_size: Option<LayoutSize>,
    /// If this is an inline rectangle, resolve the %-based font sizes
    /// and store them here.
    pub resolved_text_layout_options: Option<(ResolvedTextLayoutOptions, InlineTextLayout, LayoutRect)>,
    /// Determines if the rect should be clipped or not (TODO: x / y as separate fields!)
    pub overflow: Overflow,
*/
}