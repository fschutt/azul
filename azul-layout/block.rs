
use std::collections::BTreeMap;
use crate::{
    RectContent,
    anon::AnonDom,
    style::Style,
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeDataContainer, NodeDepths, NodeId},
    ui_solver::{PositionedRectangle, ResolvedOffsets},
};
use azul_css::{LayoutSize, LayoutPoint, LayoutRect, Overflow};

pub(crate) fn compute<T: GetTextLayout>(
    root_size: LayoutSize,
    anon_dom: &AnonDom,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
) -> NodeDataContainer<PositionedRectangle> {

    let anon_dom_depths = anon_dom.anon_node_hierarchy.get_parents_sorted_by_depth();

    let mut positioned_rects = NodeDataContainer::new(vec![PositionedRectangle {
        bounds: LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(0.0, 0.0)),
        padding: ResolvedOffsets::zero(),
        margin: ResolvedOffsets::zero(),
        border_widths: ResolvedOffsets::zero(),
        resolved_text_layout_options: None,
        overflow: Overflow::Scroll,
    }; anon_dom.anon_node_hierarchy.len()]);

    solve_widths(
        root_size.width,
        anon_dom,
        &anon_dom_depths,
        &mut positioned_rects,
    );

    position_items_horizontal(
        anon_dom,
        &anon_dom_depths,
        &mut positioned_rects,
    );

    solve_content_heights(
        &anon_dom_depths,
        rect_contents,
        &mut positioned_rects,
    );

    solve_heights(
        &anon_dom_depths,
        rect_contents,
        &mut positioned_rects,
    );

    position_items_vertical(
        &anon_dom_depths,
        &mut positioned_rects,
    );

    NodeDataContainer::new(
        anon_dom.original_node_id_mapping.iter().map(|(original_node_id, anon_node_id)| {
            positioned_rects[*anon_node_id].clone()
        }).collect()
    )
}

fn solve_widths(
    root_width: f32,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {

    use crate::anon::AnonNode::{InlineNode, BlockNode, AnonStyle};

    debug_assert!(anon_dom.anon_node_hierarchy.len() == positioned_rects.len());

    macro_rules! calc_block_width {($id:expr, $parent_content_size:expr) => ({
        match &anon_dom.anon_node_data[$id] {
            BlockNode(ref style) | InlineNode(ref style) => {
                let block_width = calculate_block_width(style, $parent_content_size);
                apply_block_width(block_width, &mut positioned_rects[$id]);
            },
            AnonStyle => {
                // set padding, margin, border to zero
                apply_block_width(BlockWidth {
                    width: $parent_content_size,
                    .. Default::default()
                }, &mut positioned_rects[$id])
            }
        }
    })}

    calc_block_width!(NodeId::ZERO, root_width);

    for (_depth, parent_node_id) in anon_dom_depths {
        let parent_content_size = positioned_rects[*parent_node_id].bounds.size.width;
        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {
            calc_block_width!(child_id, parent_content_size);
        }
    }
}

fn apply_block_width(block: BlockWidth, node: &mut PositionedRectangle) {
    node.bounds.size.width = block.width;
    node.padding.right = block.padding.0;
    node.padding.left = block.padding.1;
    node.margin.right = block.margin.0;
    node.margin.left = block.margin.1;
    node.border_widths.right = block.border_width.0;
    node.border_widths.left = block.border_width.1;
}

fn position_items_horizontal(
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {

}

fn solve_content_heights<T: GetTextLayout>(
    anon_dom_depths: &NodeDepths,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {

}

fn solve_heights<T: GetTextLayout>(
    anon_dom_depths: &NodeDepths,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {

}

fn position_items_vertical(
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {

}

// ------

#[derive(Debug, Copy, Clone, Default)]
struct BlockWidth {
    width: f32,
    border_width: (f32, f32),
    margin: (f32, f32),
    padding: (f32, f32),
}

impl BlockWidth {
    #[inline]
    pub fn content_box_width(&self) -> f32 { self.width }
    #[inline]
    pub fn padding_box_width(&self) -> f32 { self.content_box_width() + self.padding.0 + self.padding.1 }
    #[inline]
    pub fn border_box_width(&self) -> f32 { self.padding_box_width() + self.border_width.0 + self.border_width.1 }
    #[inline]
    pub fn margin_box_width(&self) -> f32 { self.border_box_width() + self.margin.0 + self.margin.1 }
}

// see: https://limpet.net/mbrubeck/2014/09/17/toy-layout-engine-6-block.html
fn calculate_block_width(
    style: &Style,
    parent_content_width: f32,
) -> BlockWidth {

    use crate::{
        number::{Number, MinMax, OrElse},
        style::{Dimension, BoxSizing::{BorderBox, ContentBox}},
    };

    let pw = Number::Defined(parent_content_width);

    // The default for block width is 100% of the parent width
    let width = style.size.width.resolve(pw).or_else(parent_content_width);

    let mut margin_left = style.margin.left.resolve(pw).or_else(0.0);
    let mut margin_right = style.margin.right.resolve(pw).or_else(0.0);

    let padding_left = style.padding.left.resolve(pw).or_else(0.0);
    let padding_right = style.padding.right.resolve(pw).or_else(0.0);

    let border_width_left = style.border.left.resolve(pw).or_else(0.0);
    let border_width_right = style.border.left.resolve(pw).or_else(0.0);

    // Adjust for min / max width properties
    let mut width = Number::Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        ContentBox => width + padding_left + padding_right + border_width_left + border_width_right,
        // The width and height properties (and min/max properties) includes content, padding and border
        BorderBox => width,
    })
    .maybe_min(style.min_size.width.resolve(pw))
    .maybe_max(style.max_size.width.resolve(pw))
    .or_else(width)
    - padding_left - padding_right - border_width_left - border_width_right;

    let total_width = width
        + margin_left
        + margin_right
        + padding_left
        + padding_right
        + border_width_left
        + border_width_right;

    if style.size.width != Dimension::Auto && total_width > parent_content_width {
        if style.margin.left == Dimension::Auto {
            margin_left = 0.0;
        }
        if style.margin.right == Dimension::Auto {
            margin_right = 0.0;
        }
    }

    let underflow = parent_content_width - total_width;

    match (
        style.size.width == Dimension::Auto,
        style.margin.left == Dimension::Auto,
        style.margin.right == Dimension::Auto
    ) {
        // If the values are overconstrained, calculate margin_right.
        (false, false, false) => {
            margin_right += underflow;
        }

        // If exactly one size is auto, its used value follows from the equality.
        (false, false, true) => { margin_right = underflow; }
        (false, true, false) => { margin_left  = underflow; }

        // If width is set to auto, any other auto values become 0.
        (true, _, _) => {
            if style.margin.left == Dimension::Auto { margin_left = 0.0; }
            if style.margin.right == Dimension::Auto { margin_right = 0.0; }

            if underflow >= 0.0 {
                // Expand width to fill the underflow.
                width = underflow;
            } else {
                // Width can't be negative. Adjust the right margin instead.
                width = 0.0;
                margin_right += underflow;
            }
        }

        // If margin-left and margin-right are both auto, their used values are equal.
        (false, true, true) => {
            margin_left = underflow / 2.0;
            margin_right = underflow / 2.0;
        }
    }

    BlockWidth {
        width,
        border_width: (border_width_left, border_width_right),
        margin: (margin_left, margin_right),
        padding: (padding_left, padding_right),
    }
}