
use std::collections::BTreeMap;
use crate::{
    RectContent,
    RectContent::{Image, Text},
    anon::{AnonDom, AnonNode::{InlineNode, BlockNode, AnonStyle}},
    style::{Style, Overflow as StyleOverflow, Display, Dimension, BoxSizing},
    number::{Number::{Defined, Undefined}, MinMax, OrElse},
};
use azul_core::{
    traits::GetTextLayout,
    id_tree::{NodeDataContainer, NodeDepths, NodeId},
    ui_solver::{
        PositionedRectangle, InlineTextLayout,
        ResolvedTextLayoutOptions, ResolvedOffsets
    },
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

    let mut resolved_text_layout_options = BTreeMap::new();

    solve_widths(
        root_size.width,
        anon_dom,
        &anon_dom_depths,
        &mut positioned_rects,
        rect_contents,
        &mut resolved_text_layout_options,
    );

    solve_heights(
        root_size,
        anon_dom,
        &anon_dom_depths,
        rect_contents,
        &mut positioned_rects,
        &resolved_text_layout_options,
    );

    position_items(
        root_size.width,
        anon_dom,
        &anon_dom_depths,
        &mut positioned_rects,
    );

    NodeDataContainer::new(
        anon_dom.original_node_id_mapping
        .iter()
        .map(|(_, anon_node_id)| positioned_rects[*anon_node_id].clone())
        .collect()
    )
}

fn solve_widths<T: GetTextLayout>(
    root_width: f32,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    resolved_text_layout_options: &mut BTreeMap<NodeId, (ResolvedTextLayoutOptions, InlineTextLayout)>,
) {

    debug_assert!(anon_dom.anon_node_hierarchy.len() == positioned_rects.len());

    let mut last_trailing = None;

    macro_rules! calc_block_width {($id:expr, $parent_content_size:expr) => ({
        let id: NodeId = $id;
        if $parent_content_size > 0.0 {
            let block_width = match &anon_dom.anon_node_data[id] {
                BlockNode(ref style) => calculate_block_width(style, $parent_content_size),
                InlineNode(ref style) => {

                    let original_node_id = &anon_dom.reverse_node_id_mapping.get(&id).unwrap();
                    let parent_overflow_x = anon_dom.anon_node_hierarchy[id].parent.as_ref().map(|parent_id| {
                        anon_dom.anon_node_data[*parent_id].get_overflow_x()
                    }).unwrap_or(StyleOverflow::Auto);

                    let w = calculate_inline_width(
                        id,
                        last_trailing,
                        rect_contents.get_mut(original_node_id),
                        style,
                        $parent_content_size,
                        parent_overflow_x,
                        resolved_text_layout_options,
                    );

                    last_trailing = w.trailing;

                    BlockWidth {
                        width: w.width,
                        border_width: w.border_width,
                        margin: w.margin,
                        padding: w.padding,
                    }
                },
                AnonStyle => {
                    // set padding, margin, border to zero
                    BlockWidth { width: $parent_content_size, .. Default::default() }
                }
            };

            apply_block_width(block_width, &mut positioned_rects[$id]);
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

fn solve_heights<T: GetTextLayout>(
    root_size: LayoutSize,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    rect_contents: &mut BTreeMap<NodeId, RectContent<T>>,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
    resolved_text_layout_options: &BTreeMap<NodeId, (ResolvedTextLayoutOptions, InlineTextLayout)>,
) {
    debug_assert!(anon_dom.anon_node_hierarchy.len() == positioned_rects.len());

    // First, distribute the known heights in a top-down fashion
    for (depth, parent_node_id) in anon_dom_depths.iter() {

        let h = match &anon_dom.anon_node_data[*parent_node_id] {
            BlockNode(ref style) | InlineNode(ref style) => {
                match style.size.height {
                    Dimension::Undefined | Dimension::Auto => 0.0,
                    Dimension::Pixels(p) => p,
                    Dimension::Percent(pct) => {
                        let parent = anon_dom.anon_node_hierarchy[*parent_node_id].parent;
                        let parent_height = match parent {
                            None => root_size.height,
                            Some(s) => positioned_rects[s].bounds.size.height,
                        };
                        parent_height / 100.0 * pct
                    }
                }
            },
            AnonStyle => 0.0,
        };

        positioned_rects[*parent_node_id].bounds.size.height = h;
    }

    // Then, bubble the inline items up and increase the height if the
    // height isn't fixed.

    let mut content_heights = BTreeMap::new();
    let mut children_height_sum = 0.0;
    let mut current_depth_level = anon_dom_depths.last().map(|(s, _)| *s).unwrap_or(0);
    let mut last_parent = None;

    for (depth, parent_node_id) in anon_dom_depths.iter().rev() {

        last_parent = anon_dom.anon_node_hierarchy[*parent_node_id].parent;

        if current_depth_level != *depth {
            if let Some(last_parent) = last_parent {
                content_heights.insert(last_parent, children_height_sum);
            }
            last_parent = None;
            children_height_sum = 0.0;
            current_depth_level = *depth;
        }

        let parent_size = positioned_rects[*parent_node_id].bounds.size;

        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {

            let self_width = positioned_rects[child_id].bounds.size.width;
            let children_content_height = child_id
                .children(&anon_dom.anon_node_hierarchy)
                .map(|c| content_heights.get(&c).copied().unwrap_or(0.0))
                .sum();

            let block_height = match &anon_dom.anon_node_data[child_id] {
                BlockNode(ref style) => {
                    calculate_block_height(
                        style,
                        parent_size.width,
                        parent_size.height,
                        self_width,
                        children_content_height,
                        rect_contents.get_mut(&child_id),
                        resolved_text_layout_options.get(&child_id),
                    )
                },
                InlineNode(ref style) => {
                    calculate_block_height(
                        style,
                        parent_size.width,
                        parent_size.height,
                        self_width,
                        children_content_height,
                        rect_contents.get_mut(&child_id),
                        resolved_text_layout_options.get(&child_id),
                    )
                },
                AnonStyle => {
                    // set padding, margin, border to zero
                    BlockHeight { height: children_content_height, .. Default::default() }
                }
            };

            content_heights.insert(child_id, block_height.margin_box_height());

            apply_block_height(block_height, &mut positioned_rects[child_id]);

            children_height_sum += block_height.margin_box_height();
        }
    }

    // If there is no defined height on the body node, set the height on the root node to be the
    // combined height of all child nodes.

    match &anon_dom.anon_node_data[NodeId::ZERO] {
        BlockNode(ref style) | InlineNode(ref style) => {
            let mut root_block_height = calculate_block_height(
                style,
                root_size.width,
                root_size.height,
                positioned_rects[NodeId::ZERO].bounds.size.width,
                children_height_sum,
                rect_contents.get_mut(&NodeId::ZERO),
                resolved_text_layout_options.get(&NodeId::ZERO),
            );
            root_block_height.height = root_block_height.height.min(root_size.height);
            apply_block_height(root_block_height, &mut positioned_rects[NodeId::ZERO]);
        },
        AnonStyle => {
            positioned_rects[NodeId::ZERO].bounds.size.height = children_height_sum.min(root_size.height);
        },
    }

    // bubble inline sizes up
    // the first leaf node will never be a inline node, since inline nodes are wrapped by AnonNodes
}

fn position_items(
    _root_width: f32,
    anon_dom: &AnonDom,
    anon_dom_depths: &NodeDepths,
    positioned_rects: &mut NodeDataContainer<PositionedRectangle>,
) {
/*
    // calculate max horizontal size (use overflow!)
    let mut cur_x = 0.0;
    let mut cur_y = 0.0;
    let mut previous_horizontal_margin = 0.0;
    let mut cur_depth = 0;

    for (depth, parent_node_id) in anon_dom_depths.iter() {

        if cur_depth != depth {
            cur_x = 0;
            cur_y = 0;
            previous_margin = 0.0;
        }

        let parent_width = match anon_dom.anon_node_hierarchy[*parent_node_id].parent {
            None => (root_width, Overflow::Scroll),
            Some(s) => (positioned_rects[*s].bounds.size.width, ,
        };
        let parent_content_size = positioned_rects[*parent_node_id].bounds.size.width;
        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {
            calc_block_width!(child_id, parent_content_size);
        }
    }
*/
    for (_, parent_node_id) in anon_dom_depths.iter() {
        let parent = positioned_rects[*parent_node_id].bounds;
        for child_id in parent_node_id.children(&anon_dom.anon_node_hierarchy) {
            let child = &mut positioned_rects[child_id];
            child.bounds.origin.x += parent.origin.x;
            child.bounds.origin.y += parent.origin.y;
        }
    }

    /*
    d.content.x = containing_block.content.x +
                  d.margin.left + d.border.left + d.padding.left;

    // Position the box below all the previous boxes in the container.
    d.content.y = containing_block.content.height + containing_block.content.y +
                  d.margin.top + d.border.top + d.padding.top;
    */
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

    if style.display == Display::None {
        return BlockWidth::default();
    }

    let pw = Defined(parent_content_width);

    // The default for block width is 100% of the parent width
    let width = style.size.width.resolve(pw).or_else(parent_content_width);

    let mut margin_left = style.margin.left.resolve(pw).or_else(0.0);
    let mut margin_right = style.margin.right.resolve(pw).or_else(0.0);

    let padding_left = style.padding.left.resolve(pw).or_else(0.0);
    let padding_right = style.padding.right.resolve(pw).or_else(0.0);

    let border_width_left = style.border.left.resolve(pw).or_else(0.0);
    let border_width_right = style.border.left.resolve(pw).or_else(0.0);

    // Adjust for min / max width properties
    let mut width = Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        BoxSizing::ContentBox => width + padding_left + padding_right + border_width_left + border_width_right,
        // The width and height properties (and min/max properties) includes content, padding and border
        BoxSizing::BorderBox => width,
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

#[derive(Debug, Copy, Clone, Default)]
struct InlineWidth {
    width: f32,
    border_width: (f32, f32),
    margin: (f32, f32),
    padding: (f32, f32),
    trailing: Option<f32>,
}

impl InlineWidth {
    #[inline]
    pub fn content_box_width(&self) -> f32 { self.width }
    #[inline]
    pub fn padding_box_width(&self) -> f32 { self.content_box_width() + self.padding.0 + self.padding.1 }
    #[inline]
    pub fn border_box_width(&self) -> f32 { self.padding_box_width() + self.border_width.0 + self.border_width.1 }
    #[inline]
    pub fn margin_box_width(&self) -> f32 { self.border_box_width() + self.margin.0 + self.margin.1 }
}

fn calculate_inline_width<T: GetTextLayout>(
    node_id: NodeId,
    last_leading: Option<f32>,
    rect_content: Option<&mut RectContent<T>>,
    style: &Style,
    parent_content_width: f32,
    parent_overflow_x: StyleOverflow,
    resolved_text_layout_options: &mut BTreeMap<NodeId, (ResolvedTextLayoutOptions, InlineTextLayout)>,
) -> InlineWidth {

    if style.display == Display::None {
        return InlineWidth::default();
    }

    let pw = Defined(parent_content_width);

    let mut leading = None;

    // The inline block has a different width calculation
    let width = match (style.size.width.resolve(pw), rect_content) {
        (Defined(f), _) => f,
        (Undefined, None) => 0.0,
        (Undefined, Some(Image(image_width, image_height))) => {
            *image_width as f32 / *image_height as f32 * style.size.width.resolve(pw)
            .maybe_min(style.min_size.width.resolve(pw))
            .maybe_max(style.max_size.width.resolve(pw))
            .or_else(parent_content_width)
        },
        (Undefined, Some(Text(t))) => {
            use azul_core::ui_solver::{
                DEFAULT_FONT_SIZE_PX, DEFAULT_LETTER_SPACING,
                DEFAULT_WORD_SPACING,
            };

            let text_layout_options = ResolvedTextLayoutOptions {
                max_horizontal_width: if parent_overflow_x.allows_horizontal_overflow() { None } else { Some(parent_content_width) },
                leading: last_leading,
                holes: Vec::new(),
                font_size_px: style.font_size_px.to_pixels(DEFAULT_FONT_SIZE_PX as f32),
                letter_spacing: style.letter_spacing.map(|ls| ls.to_pixels(DEFAULT_LETTER_SPACING)),
                word_spacing: style.word_spacing.map(|ls| ls.to_pixels(DEFAULT_WORD_SPACING)),
                line_height: style.line_height,
                tab_width: style.tab_width,
            };

            let layouted_inline_text = t.get_text_layout(&text_layout_options);

            leading = Some(layouted_inline_text.get_trailing());

            let inline_text_bounds = layouted_inline_text.get_bounds();

            resolved_text_layout_options.insert(node_id, (text_layout_options.clone(), layouted_inline_text));

            inline_text_bounds.size.width
        },
    };

    let margin_left = style.margin.left.resolve(pw).or_else(0.0);
    let margin_right = style.margin.right.resolve(pw).or_else(0.0);

    let padding_left = style.padding.left.resolve(pw).or_else(0.0);
    let padding_right = style.padding.right.resolve(pw).or_else(0.0);

    let border_width_left = style.border.left.resolve(pw).or_else(0.0);
    let border_width_right = style.border.right.resolve(pw).or_else(0.0);

    // Adjust for min / max width properties
    let width = Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        BoxSizing::ContentBox => width + padding_left + padding_right + border_width_left + border_width_right,
        // The width and height properties (and min/max properties) includes content, padding and border
        BoxSizing::BorderBox => width,
    })
    .maybe_min(style.min_size.width.resolve(pw))
    .maybe_max(style.max_size.width.resolve(pw))
    .or_else(width)
    - padding_left - padding_right - border_width_left - border_width_right;

    InlineWidth {
        width,
        border_width: (border_width_left, border_width_right),
        margin: (margin_left, margin_right),
        padding: (padding_left, padding_right),
        trailing: if style.display == Display::Inline { leading } else { None }
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

// ----

#[derive(Debug, Copy, Clone, Default)]
struct BlockHeight {
    height: f32,
    content_height: Option<f32>,
    border_height: (f32, f32),
    margin: (f32, f32),
    padding: (f32, f32),
}

impl BlockHeight {
    #[inline]
    pub fn content_box_height(&self) -> f32 { self.height }
    #[inline]
    pub fn padding_box_height(&self) -> f32 { self.content_box_height() + self.padding.0 + self.padding.1 }
    #[inline]
    pub fn border_box_height(&self) -> f32 { self.padding_box_height() + self.border_height.0 + self.border_height.1 }
    #[inline]
    pub fn margin_box_height(&self) -> f32 { self.border_box_height() + self.margin.0 + self.margin.1 }
}

fn calculate_block_height<T: GetTextLayout>(
    style: &Style,
    parent_width: f32,
    parent_height: f32,
    block_width: f32,
    children_content_height: f32,
    rect_content: Option<&mut RectContent<T>>,
    resolved_text_layout_options: Option<&(ResolvedTextLayoutOptions, InlineTextLayout)>,
) -> BlockHeight {

    if style.display == Display::None {
        return BlockHeight::default();
    }

    let ph = Defined(parent_height);

    let mut content_height = None;

    let height = match style.size.height {
        Dimension::Undefined | Dimension::Auto => {
            let self_content_height = match rect_content {
                None => None,
                Some(Image(image_width, image_height)) => {
                    Some(*image_width as f32 / *image_height as f32 * block_width)
                },
                Some(Text(t)) => {
                    match resolved_text_layout_options {
                        None => None,
                        Some((tlo, layouted_inline_text)) => {
                            let inline_text_bounds = layouted_inline_text.get_bounds();
                            Some(inline_text_bounds.size.height)
                        }
                    }
                },
            };

            content_height = self_content_height;
            children_content_height.max(self_content_height.unwrap_or(0.0))
        },
        Dimension::Pixels(p) => p,
        Dimension::Percent(pct) => {
            parent_height / 100.0 * pct
        }
    };

    let pw = Defined(parent_width);

    // CSS spec:
    //
    // The percentage is calculated with respect to the *width* of the generated box’s
    // containing block. Note that this is true for margin-top and margin-bottom as well.

    let margin_top = style.margin.top.resolve(pw).or_else(0.0);
    let margin_bottom = style.margin.bottom.resolve(pw).or_else(0.0);

    let padding_top = style.padding.top.resolve(pw).or_else(0.0);
    let padding_bottom = style.padding.bottom.resolve(pw).or_else(0.0);

    let border_height_top = style.border.top.resolve(pw).or_else(0.0);
    let border_height_bottom = style.border.bottom.resolve(pw).or_else(0.0);

    // Adjust for min / max height properties
    let height = Defined(match style.box_sizing {
        // The width and height properties (and min/max properties) includes only the content.
        BoxSizing::ContentBox => height + padding_top + padding_bottom + border_height_top + border_height_bottom,
        // The width and height properties (and min/max properties) includes content, padding and border
        BoxSizing::BorderBox => height,
    })
    .maybe_min(style.min_size.height.resolve(ph))
    .maybe_max(style.max_size.height.resolve(ph))
    .or_else(height)
    - padding_top - padding_bottom - border_height_top - border_height_bottom;

    BlockHeight {
        height,
        content_height,
        border_height: (border_height_top, border_height_bottom),
        margin: (margin_top, margin_bottom),
        padding: (padding_top, padding_bottom),
    }
}

fn apply_block_height(block: BlockHeight, node: &mut PositionedRectangle) {
    node.bounds.size.height = block.height;
    node.padding.top = block.padding.0;
    node.padding.bottom = block.padding.1;
    node.margin.top = block.margin.0;
    node.margin.bottom = block.margin.1;
    node.border_widths.top = block.border_height.0;
    node.border_widths.bottom = block.border_height.1;
}
