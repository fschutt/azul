//! Multi-column block containers (CSS Multicol 1).

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric,
        },
        layout::{
            LayoutBorderSpacing, LayoutClear, LayoutDisplay, LayoutFloat, LayoutHeight,
            LayoutJustifyContent, LayoutOverflow, LayoutPosition, LayoutTableLayout,
            LayoutTextJustify, LayoutWidth, LayoutWritingMode, ShapeInside, ShapeOutside,
            StyleBorderCollapse, StyleCaptionSide, StyleEmptyCells,
        },
        property::CssProperty,
        style::{
            BorderStyle, StyleDirection, StyleHyphens, StyleLineBreak, StyleListStylePosition,
            StyleListStyleType, StyleOverflowWrap, StyleTextAlign, StyleTextAlignLast,
            StyleTextBoxTrim, StyleTextCombineUpright, StyleTextOrientation, StyleUnicodeBidi,
            StyleVerticalAlign, StyleVisibility, StyleWhiteSpace, StyleWordBreak,
        },
    },
};
use rust_fontconfig::FcWeight;
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};
#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_ifc_layout, debug_info, debug_log, debug_table_layout, debug_warning,
    font_traits::{
        ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
        LayoutFragment, ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, TextLayoutCache,
        UnifiedConstraints,
    },
    solver3::{
        geometry::{BoxProps, ContainingBlock as CBTY, EdgeSizes, IntrinsicSizes},
        getters::{
            get_clear, get_css_border_bottom_width, get_css_border_top_width, get_css_box_sizing,
            get_css_height, get_css_padding_bottom, get_css_padding_top, get_css_width,
            get_direction_property, get_display_property, get_element_font_size, get_float,
            get_list_style_position, get_list_style_type, get_overflow_x, get_overflow_y,
            get_parent_font_size, get_root_font_size, get_style_properties, get_text_align,
            get_text_box_edge_property, get_text_box_trim_property, get_text_orientation_property,
            get_unicode_bidi_property, get_vertical_align_property, get_visibility,
            get_white_space_property, get_writing_mode, MultiValue,
        },
        layout_tree::{
            AnonymousBoxType, CachedInlineLayout, LayoutNode, LayoutNodeCold, LayoutNodeHot,
            LayoutNodeId, LayoutNodeWarm, LayoutTree, PseudoElement,
        },
        positioning::get_position_type,
        scrollbar::{ScrollbarKind, ScrollbarRequirements},
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutDebugMessage, LayoutError, Result,
    },
    text3::cache::{
        AvailableSpace as Text3AvailableSpace, BreakType, ClearType, InlineBreak,
        TextAlign as Text3TextAlign,
    },
};
#[allow(clippy::wildcard_imports)]
// the formatting contexts' items, re-exported from the sibling modules by mod.rs
use super::*;

// Multi-column block containers (CSS Multicol 1)

/// The columns of a multi-column block container, as `layout_bfc` lays it
/// out.
pub(super) struct BlockColumns {
    style: crate::solver3::multicol::ColumnStyle,
    pub(super) geometry: crate::solver3::multicol::ColumnGeometry,
    /// The container's content-box width, the columns are placed across.
    content_width: f32,
    /// The container's own content height, when it has a definite one.
    definite_height: Option<f32>,
    /// `direction: rtl`: the columns run right to left.
    rtl: bool,
    /// The container, whose flow the next layout pass learns from.
    dom_id: NodeId,
    /// The page height when the container is laid out on paged media with
    /// columns of its own height - its columns then wrap from page to page
    /// (`multicol::plan_paged_columns`); `None` in continuous media.
    page: Option<f32>,
    /// The room the first page leaves its columns, when it is too little for
    /// the whole container (`multicol::ColumnPages::rooms`).
    first_room: Option<f32>,
}

/// The content extent of a multi-column container's columns.
pub(super) struct ColumnsExtent {
    /// The tallest column.
    pub(super) height: f32,
    /// How far the columns reach on the inline axis.
    pub(super) width: f32,
}

/// The columns of the block container at `node_index`, when it is a
/// multi-column container the column layout handles: continuous media (not
/// a K30b fragment pass), a definite width, a horizontal writing mode. In
/// every other case it lays out as one column, as it always did.
/// `content_box` is the size its children's containing block has.
pub(super) fn block_columns<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    node: &LayoutNodeHot,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
    content_box: LogicalSize,
) -> Option<BlockColumns> {
    if constraints.fragmentainer.is_some()
        || constraints.writing_mode != LayoutWritingMode::HorizontalTb
        || !matches!(
            constraints.available_width_type,
            Text3AvailableSpace::Definite(_)
        )
    {
        return None;
    }
    let dom_id = node.dom_node_id?;
    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let style = crate::solver3::multicol::column_style(
        ctx.styled_dom,
        dom_id,
        node_state,
        ctx.viewport_size,
    )?;
    let has_definite_height = node.used_size.is_some()
        && tree.warm(LayoutNodeId::new(node_index)).is_some_and(|w| {
            matches!(
                w.computed_style.height,
                Some(LayoutHeight::Px(_) | LayoutHeight::Calc(_))
            )
        });
    let definite_height = has_definite_height
        .then_some(content_box.height)
        .filter(|h| h.is_finite() && *h > 0.0);
    let rtl = matches!(
        get_direction_property(ctx.styled_dom, dom_id, node_state),
        MultiValue::Exact(StyleDirection::Rtl)
    );
    let page = ctx
        .cache_map
        .column_pages
        .pages
        .map(|pages| pages.normal_page_content_height)
        .filter(|_| definite_height.is_none());
    let first_room = page.and_then(|_| ctx.cache_map.column_pages.rooms.get(&dom_id).copied());
    Some(BlockColumns {
        geometry: style.geometry(content_box.width),
        style,
        content_width: content_box.width,
        definite_height,
        rtl,
        dom_id,
        page,
        first_room,
    })
}

/// Cuts a multi-column block container's single column into its columns
/// (`multicol::plan_columns`) and moves its children there: an in-flow
/// child to its column, a paragraph that continues in the next columns laid
/// out again split between its lines ([`split_into_columns`]), a float to
/// the column its top falls in. `positions` are the children's positions in
/// the single column, relative to the container's content box.
///
/// A paragraph beside a float of the context (`floats`) does not split: its
/// lines wrapped around the float, and a split must break its lines exactly
/// as they are. Every other paragraph splits between its lines.
#[allow(clippy::too_many_arguments)] // one layout step, the layout state it needs
pub(super) fn distribute_into_columns<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    float_cache: &mut HashMap<usize, FloatingContext>,
    columns: &BlockColumns,
    children: &[usize],
    positions: &mut BTreeMap<usize, LogicalPosition>,
    floats: &FloatingContext,
    constraints: &LayoutConstraints<'_>,
) -> Result<ColumnsExtent> {
    use crate::solver3::multicol::{
        plan_columns, plan_paged_columns, ColumnFlowRecord, FlowBox, PageRoom,
    };

    // The heights the floats' margin boxes span, in the single column.
    let float_spans: Vec<(f32, f32)> = floats
        .floats
        .iter()
        .map(|float| {
            let margin_box = float.margin_box();
            (margin_box.origin.y, margin_box.origin.y + margin_box.size.height)
        })
        .collect();
    let mut flow: Vec<(usize, FlowBox)> = Vec::new();
    let mut floats: Vec<usize> = Vec::new();
    for &child in children {
        let Some(&pos) = positions.get(&child) else {
            continue;
        };
        let Some(node) = tree.get(LayoutNodeId::new(child)) else {
            continue;
        };
        if get_float_property(ctx.styled_dom, node.dom_node_id) != LayoutFloat::None {
            floats.push(child);
            continue;
        }
        let height = node.used_size.unwrap_or_default().height;
        let beside_a_float = float_spans
            .iter()
            .any(|&(top, bottom)| top < pos.y + height && bottom > pos.y);
        let lines = if beside_a_float {
            Vec::new()
        } else {
            splittable_lines(ctx, tree, child, pos.y)
        };
        flow.push((
            child,
            FlowBox {
                top: pos.y,
                bottom: pos.y + height,
                lines,
            },
        ));
    }

    let boxes: Vec<FlowBox> = flow.iter().map(|(_, b)| b.clone()).collect();
    let plan = match (columns.page, columns.first_room) {
        (Some(rest), Some(first)) => {
            plan_paged_columns(&boxes, columns.geometry.count, PageRoom { first, rest })
        }
        _ => plan_columns(
            &boxes,
            columns.geometry.count,
            columns.definite_height,
            columns.style.fill,
        ),
    };
    if columns.page.is_some() {
        ctx.cache_map.column_pages.flows.insert(
            columns.dom_id,
            ColumnFlowRecord {
                boxes: boxes.clone(),
                count: columns.geometry.count,
                height: plan.height,
                row_tops: plan.row_tops.clone(),
            },
        );
    }
    let column_x = |k: usize| {
        columns
            .geometry
            .column_x(k, columns.content_width, columns.rtl)
    };

    for ((child, flow_box), placement) in flow.iter().zip(&plan.boxes) {
        let k = placement.column;
        let start = plan.starts.get(k).copied().unwrap_or(0.0);
        if let Some(pos) = positions.get_mut(child) {
            *pos = LogicalPosition::new(
                pos.x + column_x(plan.lane(k)),
                pos.y - start + plan.row_top(k),
            );
        }
        if !placement.line_breaks.is_empty() {
            let next_start = plan.starts.get(k + 1).copied().unwrap_or(flow_box.bottom);
            let content_top = flow_box.top
                + tree
                    .get(LayoutNodeId::new(*child))
                    .map_or(0.0, |n| {
                        let bp = n.box_props.unpack();
                        bp.border.top + bp.padding.top
                    });
            // Where each further column of the paragraph starts, seen from
            // the paragraph's content-box top and first column.
            let places = (1..=placement.line_breaks.len())
                .map(|n| {
                    let j = k + n;
                    crate::text3::cache::ColumnPlace {
                        dx: column_x(plan.lane(j)) - column_x(plan.lane(k)),
                        top: plan.row_top(j) - plan.row_top(k) + start - content_top,
                    }
                })
                .collect();
            split_into_columns(
                ctx,
                tree,
                text_cache,
                float_cache,
                *child,
                flow_box,
                next_start,
                crate::text3::cache::ColumnFlow {
                    breaks: placement.line_breaks.clone(),
                    places,
                },
                constraints,
            )?;
        }
    }
    for child in floats {
        if let Some(pos) = positions.get_mut(&child) {
            let k = plan.column_at(pos.y);
            let start = plan.starts.get(k).copied().unwrap_or(0.0);
            *pos = LogicalPosition::new(
                pos.x + column_x(plan.lane(k)),
                pos.y - start + plan.row_top(k),
            );
        }
    }

    let width = (0..plan.starts.len())
        .map(|k| column_x(plan.lane(k)) + columns.geometry.width)
        .fold(columns.content_width, f32::max);
    Ok(ColumnsExtent {
        height: plan.height,
        width,
    })
}

/// The lines of the in-flow child at `child` (its border box at flow
/// offset `top`) when it may continue in the next column between two of
/// them: a plain inline formatting context - no block formatting context of
/// its own, not replaced, no columns of its own - whose stored layout is
/// the unsplit one. The line boxes are not kept, so a line's extent is its
/// items' (those with a height), offset into the flow.
pub(super) fn splittable_lines<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    child: usize,
    top: f32,
) -> Vec<crate::solver3::multicol::FlowLine> {
    let id = LayoutNodeId::new(child);
    let Some(node) = tree.get(id) else {
        return Vec::new();
    };
    if node.formatting_context != FormattingContext::Inline
        || establishes_new_bfc(ctx, node, tree.cold(id))
        || is_block_level_replaced(ctx, node)
    {
        return Vec::new();
    }
    let unsplit = tree
        .warm(id)
        .and_then(|w| w.inline_layout_result.as_ref())
        .and_then(|cached| cached.constraints.as_ref())
        .is_some_and(|c| c.columns <= 1 && c.column_flow.is_none());
    if !unsplit {
        return Vec::new();
    }
    let Some(layout) = tree.materialized_inline_layout_for_node(child) else {
        return Vec::new();
    };
    let bp = node.box_props.unpack();
    let content_top = top + bp.border.top + bp.padding.top;
    let mut extents: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    for item in &layout.items {
        let height = item.item.bounds().height;
        if height.is_nan() || height <= 0.0 {
            continue;
        }
        let item_top = content_top + item.position.y;
        let item_bottom = item_top + height;
        extents
            .entry(item.line_index)
            .and_modify(|(t, b)| {
                *t = t.min(item_top);
                *b = b.max(item_bottom);
            })
            .or_insert((item_top, item_bottom));
    }
    extents
        .into_iter()
        .map(|(index, (top, bottom))| crate::solver3::multicol::FlowLine { index, top, bottom })
        .collect()
}

/// Lays the paragraph at `child` out again as one piece of its
/// multi-column container's flow: from each line index in `column_flow`
/// on its lines continue at the top of the next column
/// (`text3::cache::ColumnFlow`), and its box keeps the part in its first
/// column (down to `next_column_start` on the flow). The lines break
/// exactly as in the single column - same width, same content - so the
/// plan made on that layout holds.
#[allow(clippy::too_many_arguments)] // one layout step, the layout state it needs
pub(super) fn split_into_columns<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    float_cache: &mut HashMap<usize, FloatingContext>,
    child: usize,
    flow_box: &crate::solver3::multicol::FlowBox,
    next_column_start: f32,
    column_flow: crate::text3::cache::ColumnFlow,
    constraints: &LayoutConstraints<'_>,
) -> Result<()> {
    let node = tree
        .get(LayoutNodeId::new(child))
        .ok_or(LayoutError::InvalidTree)?;
    let size = node.used_size.unwrap_or_default();
    let bp = node.box_props.unpack();
    let content_size = bp.inner_size(size, LayoutWritingMode::HorizontalTb);

    let mut bfc_state = BfcState::new();
    let split_constraints = LayoutConstraints {
        available_size: content_size,
        bfc_state: Some(&mut bfc_state),
        writing_mode: constraints.writing_mode,
        writing_mode_ctx: constraints.writing_mode_ctx,
        text_align: constraints.text_align,
        containing_block_size: constraints.containing_block_size,
        available_width_type: Text3AvailableSpace::Definite(content_size.width),
        fragmentainer: None,
        column_flow: Some(column_flow),
    };
    let split = layout_formatting_context(
        ctx,
        tree,
        text_cache,
        child,
        &split_constraints,
        float_cache,
    )?;
    // Its atomic inlines moved with their lines.
    for (inner, pos) in split.output.positions {
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(inner)) {
            warm.relative_position = Some(pos);
        }
    }
    // Its box is what stays in its first column - and so is its content
    // extent. The overflow size its single-column pass stored still reached
    // the whole unsplit height below the box, and the painter's content rect
    // (`get_scroll_content_size` takes the larger of the two) carried it
    // into the paged extent: 80px columns made a 163px document.
    let first_column = LogicalSize::new(size.width, (next_column_start - flow_box.top).max(0.0));
    if let Some(node) = tree.get_mut(LayoutNodeId::new(child)) {
        node.used_size = Some(first_column);
    }
    if let Some(warm) = tree.warm_mut(LayoutNodeId::new(child)) {
        warm.overflow_content_size = Some(first_column);
    }
    Ok(())
}
