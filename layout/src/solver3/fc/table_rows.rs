//! Table rows: cell heights, baselines and vertical alignment, and placing the cells and grid boxes.

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

/// Does this cell establish an INLINE formatting context: only inline-level
/// children (CSS 2.2 9.4.2), with or without loose text?
///
/// A cell of only inline BOXES (`<td><span>$10.00</span></td>`, Postmark's
/// `<td align="center"><a style="display: inline-block">`) is an IFC like any
/// block container of inline content: its `text-align` places them. It
/// needed a loose text child, so such a cell took the block branch and its
/// box sat at the cell's left edge (MAILENG6 item 5). One exception keeps the
/// block branch: an inline child that holds a block-level box
/// (`<a><img style="display: block"></a>`, block-in-inline, CSS 2.2
/// 9.2.1.1) with no loose text beside it - `layout_ifc` does not split an
/// inline around a block yet, and the block branch is what lays such a
/// linked picture out today.
///
/// Then it is laid out as ONE IFC, on two explicit paths that never meet:
/// the table's min/max-content MEASUREMENT ([`measure_cell_content_width`])
/// lays its IFC out under the measurement constraint and reads the extent
/// (min-content = its longest word), and the FINAL pass
/// ([`layout_cell_for_height`]) lays it out at its column width. The generic
/// route (`layout_formatting_context` -> `layout_bfc`) made the cell's text
/// child a block-level box sized at its MAX-content width, so a cell's
/// min-content came out equal to its max and a 220px table of prose ran its
/// cells 360px wide; routing EVERY such cell through `layout_ifc` inside
/// `layout_formatting_context` (7534be8c7, reverted) also changed the final
/// passes and dropped whole tables from the layout.
pub(super) fn cell_is_inline_formatting_context(styled_dom: &StyledDom, cell_dom_id: NodeId) -> bool {
    // A cell whose inline box holds a block (`<td><a><img style="display:
    // block"></a></td>`) is not one: that inline is split around the block
    // (CSS 2.2 s9.2.1.1), which `has_only_inline_children` answers - the
    // twin walk `inline_children_hold_a_block` that kept such cells on the
    // block branch is gone (`layout_tree::inline_holds_a_block`).
    crate::solver3::layout_tree::has_only_inline_children(styled_dom, cell_dom_id)
}

/// Layout a cell with its computed column width to determine its content height
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub(super) fn layout_cell_for_height<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    cell_index: usize,
    cell_width: f32,
    constraints: &LayoutConstraints<'_>,
) -> Result<f32> {
    let cell_node = tree
        .get(LayoutNodeId::new(cell_index))
        .ok_or(LayoutError::InvalidTree)?;
    // An ANONYMOUS cell (CSS 2.2 17.2.1: the reconciler wraps a row's stray
    // children in one) has no DOM node: its inline runs sit in anonymous
    // inline wrappers, so it is laid out by the block branch below. It used
    // to fail the whole table with `InvalidTree`.
    let cell_dom_id = cell_node.dom_node_id;

    // Check if cell has text content directly in DOM (not in LayoutTree)
    // Text nodes are intentionally not included in LayoutTree per CSS spec,
    // but we need to measure them for table cell height calculation.
    //
    // The text branch below lays the CELL out as one inline formatting
    // context, then clears its children's own inline layouts. That is right
    // for loose text and for a cell whose children are all inline-level (CSS
    // 2.2 section 9.4.2) - but mail HTML is indented, so a cell holding an
    // `<h1>` and a `<p>` also has whitespace text children, and taking the
    // text branch for it lost both: `layout_ifc` does not lay the blocks out
    // and the clearing wiped their text (AzMail sample 01, an empty newsletter
    // body). Collapsible whitespace between blocks is no text at all (CSS 2.2
    // section 9.2.2.1), so such a cell takes the block branch.
    //
    // And so does a cell with loose text AND a block child
    // (`<td>Label<div>..</div></td>`): it is a block container with mixed
    // content, its loose text in an anonymous block box beside the block
    // (`LayoutTreeBuilder` builds a cell's children like any block
    // container's). Laid out as one IFC, the block was not laid out and the
    // clearing below wiped its text. Only a cell whose children are ALL
    // inline-level establishes an inline formatting context (9.4.2).
    let has_text_children =
        cell_dom_id.is_some_and(|dom_id| cell_is_inline_formatting_context(ctx.styled_dom, dom_id));

    debug_table_layout!(
        ctx,
        "layout_cell_for_height: cell_index={}, has_text_children={}",
        cell_index,
        has_text_children
    );

    // Get padding and border to calculate content width
    let cell_node = tree
        .get(LayoutNodeId::new(cell_index))
        .ok_or(LayoutError::InvalidTree)?;
    let cell_bp = cell_node.box_props.unpack();
    let padding = &cell_bp.padding;
    let border = &cell_bp.border;
    let writing_mode = constraints.writing_mode;

    // cell_width is the border-box width (includes padding/border from column
    // width calculation) but layout functions need content-box width
    // A fixed column narrower than the cell's padding leaves no room, never
    // less than none.
    let content_width = (cell_width
        - padding.cross_start(writing_mode)
        - padding.cross_end(writing_mode)
        - border.cross_start(writing_mode)
        - border.cross_end(writing_mode))
    .max(0.0);

    debug_table_layout!(
        ctx,
        "Cell width: border_box={:.2}, content_box={:.2}",
        cell_width,
        content_width
    );

    let content_height = if has_text_children {
        // Cell contains text - use IFC to measure it
        debug_table_layout!(ctx, "Using IFC to measure text content");

        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width, // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            writing_mode_ctx: constraints.writing_mode_ctx,
            bfc_state: None,
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            // Use definite width for final cell layout!
            // This replaces any previous MinContent/MaxContent measurement.
            available_width_type: Text3AvailableSpace::Definite(content_width),
            fragmentainer: None,
            column_flow: None,
        };

        let output = layout_ifc(ctx, text_cache, tree, cell_index, &cell_constraints)?;
        // Where the line put each atomic inline (an inline-block, an image):
        // its relative position, which hit-testing, the positioning pass and
        // the painting of its own content read. Dropped here, the box stayed
        // at the cell's content origin while its line painted it in place -
        // a centered button's label at the cell's left edge.
        publish_interior_positions(tree, &output);

        // The cell now owns the authoritative IFC result. Clear any duplicate
        // inline_layout_result from text children that was set during the cell's
        // prior BFC Pass 1 (which ran before layout_cell_for_height).
        let cell_children: Vec<usize> = tree.children(cell_index).to_vec();
        for child_idx in cell_children {
            if let Some(warm) = tree.warm_mut(LayoutNodeId::new(child_idx)) {
                warm.inline_layout_result = None;
            }
        }

        debug_table_layout!(
            ctx,
            "IFC returned height={:.2}",
            output.overflow_size.height
        );

        output.overflow_size.height
    } else {
        // Cell contains block-level children or is empty - use regular layout
        debug_table_layout!(ctx, "Using regular layout for block children");

        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width, // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            writing_mode_ctx: constraints.writing_mode_ctx,
            bfc_state: None,
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            // Use Definite width for final cell layout!
            available_width_type: Text3AvailableSpace::Definite(content_width),
            fragmentainer: None,
            column_flow: None,
        };

        let mut temp_positions: super::super::PositionVec = Vec::new();
        let mut temp_scrollbar_reflow = false;
        let mut temp_float_cache = HashMap::new();

        // The table decides a cell's width. `layout_bfc` lays a cell's
        // children out inside the cell's `used_size`, and a table cell's
        // own layout never overwrites that once set - so it still held the
        // min/max-content MEASUREMENT's width, and a `width: 100%` rule or
        // an auto-width block came out as wide as the measurement instead
        // of the column(s) (a spanning `<hr>` 0 px or 1.7e38 px wide). This
        // is the cell's final layout: give it its column width first. The
        // block size stays what the measurement left (it carries an explicit
        // `height`, see the read below).
        if let Some(cell) = tree.get_mut(LayoutNodeId::new(cell_index)) {
            if let Some(size) = cell.used_size {
                cell.used_size = Some(size.with_cross(writing_mode, cell_width));
            }
        }

        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            cell_index,
            LogicalPosition::zero(),
            &CBTY::from_flattened_with_width_type(
                cell_constraints.available_size,
                cell_constraints.available_width_type,
            ),
            &mut temp_positions,
            &mut temp_scrollbar_reflow,
            &mut temp_float_cache,
            // PerformLayout: final table cell layout with definite width
            crate::solver3::cache::ComputeMode::PerformLayout,
        )?;

        // The CONTENT box's height, like the text branch's: the sum below
        // adds the padding and border. It is the extent of the layout just
        // done at the column width (wrapped at the column, CSS 2.2 17.5.3),
        // and only that: `used_size` still holds the min/max-content
        // MEASUREMENT's box (a cell's own layout never overwrites it), laid
        // out at another width - a nested `width: 80%` table at its
        // min-content, one word per line - and taking the larger of the two
        // made Mailgun's invoice row 89px too tall, its content centred in it
        // (MAILREF8 group C). The cell's own `height` is read below
        // (`cell_specified_border_box_height`), which is what the measured
        // term once stood in for.
        tree.warm(LayoutNodeId::new(cell_index))
            .and_then(|w| w.overflow_content_size)
            .map_or(0.0, |s| s.height)
            .max(0.0)
    };

    // Add padding and border to get the total height
    let cell_node = tree
        .get(LayoutNodeId::new(cell_index))
        .ok_or(LayoutError::InvalidTree)?;
    let cell_bp = cell_node.box_props.unpack();
    let padding = &cell_bp.padding;
    let border = &cell_bp.border;
    let writing_mode = constraints.writing_mode;

    let total_height = content_height
        + padding.main_start(writing_mode)
        + padding.main_end(writing_mode)
        + border.main_start(writing_mode)
        + border.main_end(writing_mode);

    debug_table_layout!(
        ctx,
        "Cell total height: cell_index={}, content={:.2}, padding/border={:.2}, total={:.2}",
        cell_index,
        content_height,
        padding.main_start(writing_mode)
            + padding.main_end(writing_mode)
            + border.main_start(writing_mode)
            + border.main_end(writing_mode),
        total_height
    );

    // The cell's own `height` is a minimum for its box (CSS 2.2 17.5.3) - for
    // a cell of text too, which the inline branch above never asked.
    let specified = cell_node.dom_node_id.and_then(|dom_id| {
        cell_specified_border_box_height(ctx.styled_dom, dom_id, &cell_bp, ctx.viewport_size)
    });
    Ok(total_height.max(specified.unwrap_or(0.0)))
}

// or bottom of content edge if no such line box exists
// +spec:box-model:b64fa0 - Cell baseline is first in-flow line box or bottom of content edge
// +spec:overflow:3fa86f - Table cell baseline: first in-flow line box or bottom of content edge;
// scrolling boxes treated as at origin +spec:inline-formatting-context:c4a20d - cell baseline:
// first in-flow line box or bottom of content edge +spec:inline-formatting-context:17a9c1 -
// vertical-align baseline/top/bottom/middle for table cells
pub(super) fn compute_cell_baseline(cell_index: usize, tree: &LayoutTree) -> f32 {
    let Some(cell_node) = tree.get(LayoutNodeId::new(cell_index)) else {
        return 0.0;
    };
    if let Some(baseline) = first_line_baseline(cell_index, tree, 0) {
        return baseline;
    }

    // No line box found: baseline is the bottom of the content edge
    let cell_bp = cell_node.box_props.unpack();
    let used_size = cell_node.used_size.unwrap_or_default();
    let padding_bottom = cell_bp.padding.bottom;
    let border_bottom = cell_bp.border.bottom;
    used_size.height - padding_bottom - border_bottom
}

/// The baseline of the first in-flow line box inside a box, measured from
/// the top of its border box: its own first line when it holds lines, else
/// the first of its in-flow children's - at any depth, offset by where each
/// child sits (`<td><div style="padding-top: 40px">data</div></td>` has its
/// baseline 40px further down than the div's own line).
pub(super) fn first_line_baseline(index: usize, tree: &LayoutTree, depth: usize) -> Option<f32> {
    // +spec:inline-formatting-context:27be38 - cell baseline is first in-flow line box or bottom of
    // content edge
    const MAX_DEPTH: usize = 64;
    let node = tree.get(LayoutNodeId::new(index))?;
    let bp = node.box_props.unpack();
    let content_top = bp.padding.top + bp.border.top;
    if let Some(cached_layout) = tree
        .warm(LayoutNodeId::new(index))
        .and_then(|w| w.inline_layout_result.as_ref())
    {
        // (d6h) Materialized: sentinel-safe first-line baseline.
        let inline_result = cached_layout.materialized();
        if let Some(first_item) = inline_result.items.first() {
            let (item_ascent, _) = text3::cache::get_item_vertical_metrics_approx(&first_item.item);
            return Some(content_top + first_item.position.y + item_ascent);
        }
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    for &child in tree.children(index) {
        let child_top = tree
            .warm(LayoutNodeId::new(child))
            .and_then(|w| w.relative_position)
            .map_or(0.0, |p| p.y);
        if let Some(baseline) = first_line_baseline(child, tree, depth + 1) {
            return Some(content_top + child_top + baseline);
        }
    }
    None
}

/// The baseline an inline-block takes from its content, measured from the
/// top of the border box of `index` - CSS 2.2 10.8.1, "the baseline of its
/// last line box in the normal flow" - searched the way Chrome searches it:
/// - a box holding lines answers with its last line's baseline;
/// - otherwise its in-flow children are asked from the LAST one up, each
///   offset by where it sits; out-of-flow boxes (absolute, fixed, floats)
///   have no line box in the normal flow;
/// - a TABLE answers nothing (Blink's `LayoutTable::InlineBlockBaseline` is
///   -1; `LayoutNG` skips tables for the inline-block baseline): the search
///   goes on above it;
/// - a child whose `overflow` is not `visible` answers with its bottom margin
///   edge, not its own lines (10.8.1's overflow rule, applied by Blink to
///   every block on the way down);
/// - a flex or grid child answers with its FIRST baseline (`LayoutNG`: "some
///   fragments use their first baseline"), `first_line_baseline`.
///
/// `None`: no line box at all - the caller's baseline is then the inline-
/// block's bottom margin edge. Mail templates (Cerberus) open with a clipped
/// preheader and go on in tables, so `AzMail`'s inline-block paper sits on the
/// preheader's bottom edge, one strut ascent below the line's top.
pub(super) fn inline_block_baseline(index: usize, tree: &LayoutTree, depth: usize) -> Option<f32> {
    const MAX_DEPTH: usize = 64;
    let node = tree.get(LayoutNodeId::new(index))?;
    let bp = node.box_props.unpack();
    let content_top = bp.padding.top + bp.border.top;
    if let Some(cached_layout) = tree
        .warm(LayoutNodeId::new(index))
        .and_then(|w| w.inline_layout_result.as_ref())
    {
        // (d6h) Materialized: sentinel-safe.
        return cached_layout
            .materialized()
            .last_line_baseline()
            .map(|baseline| content_top + baseline);
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    for &child in tree.children(index).iter().rev() {
        let Some(child_node) = tree.get(LayoutNodeId::new(child)) else {
            continue;
        };
        let child_warm = tree.warm(LayoutNodeId::new(child));
        let out_of_flow = child_warm.is_some_and(|w| {
            matches!(
                w.computed_style.position,
                LayoutPosition::Absolute | LayoutPosition::Fixed
            ) || w.computed_style.float != LayoutFloat::None
        });
        if out_of_flow || matches!(child_node.formatting_context, FormattingContext::Table) {
            continue;
        }
        let child_top = child_warm
            .and_then(|w| w.relative_position)
            .map_or(0.0, |p| p.y);
        let clips = child_warm.is_some_and(|w| {
            w.computed_style.overflow_x != LayoutOverflow::Visible
                || w.computed_style.overflow_y != LayoutOverflow::Visible
        });
        if clips {
            let height = child_node.used_size.map_or(0.0, |s| s.height);
            let margin_bottom = child_node.box_props.unpack().margin.bottom;
            return Some(content_top + child_top + height + margin_bottom);
        }
        let baseline = if matches!(
            child_node.formatting_context,
            FormattingContext::Flex | FormattingContext::Grid
        ) {
            first_line_baseline(child, tree, depth + 1)
        } else {
            inline_block_baseline(child, tree, depth + 1)
        };
        if let Some(baseline) = baseline {
            return Some(content_top + child_top + baseline);
        }
    }
    None
}

/// A table cell's `vertical-align` (CSS 2.2 17.5.3), `baseline` when unset
/// or for an anonymous cell.
pub(super) fn cell_vertical_align(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> StyleVerticalAlign {
    dom_id.map_or(StyleVerticalAlign::Baseline, |dom_id| {
        let node_state = styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        match get_vertical_align_property(styled_dom, dom_id, &node_state) {
            MultiValue::Exact(v) => v,
            _ => StyleVerticalAlign::Baseline,
        }
    })
}

/// Does this alignment put the cell on the row's baseline? (`sub`, `super`,
/// `text-top`, `text-bottom`, lengths and percentages fall back to baseline
/// in a table cell, CSS 2.2 17.5.3.)
pub(super) fn is_baseline_aligned(va: StyleVerticalAlign) -> bool {
    !matches!(
        va,
        StyleVerticalAlign::Top | StyleVerticalAlign::Middle | StyleVerticalAlign::Bottom
    )
}

/// +spec:box-model:72b495 - Table row height = max of computed height and MIN required by cells;
/// baseline alignment
// +spec:display-property:728144 - Table height algorithm: row heights from cell content, rowspan
// distribution, vertical-align in cells (top/middle/bottom/baseline,
// sub/super/text-top/text-bottom/length/percentage fall back to baseline), cell baseline
// computation, and horizontal alignment via text-align +spec:positioning:3eaadd - Table height
// algorithms (§17.5.3): row height = max of cell heights/MIN,   rowspan distribution,
// vertical-align in table cells, cell baseline definition
/// Calculate row heights based on cell content after column widths are determined
// +spec:inline-formatting-context:87b90d - Table height algorithms: row height = max(computed
// height, cell heights, MIN); vertical-align in cells (baseline/top/middle/bottom, sub/super/etc.
// fall back to baseline)
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose
                                                               // layout/render/parse routine (one
                                                               // branch per case)
pub(super) fn calculate_row_heights<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints<'_>,
) -> Result<()> {
    debug_table_layout!(
        ctx,
        "calculate_row_heights: num_rows={}, available_size={:?}",
        table_ctx.num_rows,
        constraints.available_size
    );

    // +spec:inline-formatting-context:a7c7a0 - row height = max of computed height, cell heights,
    // and MIN; vertical-align per cell Initialize row heights and baselines
    table_ctx.row_heights = vec![0.0; table_ctx.num_rows];
    table_ctx.row_baselines = vec![0.0; table_ctx.num_rows];

    // CSS 2.2 Section 17.6: Set collapsed rows to height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }

    // required by content; 'height' property can influence row height but does not
    // increase cell box height
    // First pass: Calculate heights for cells that don't span multiple rows
    let mut baseline_cells: Vec<(usize, f32, f32)> = Vec::new();
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }

        // The cell's width: its columns and the spacing between them.
        let cell_width = table_ctx.cell_span_width(cell_info);

        debug_table_layout!(
            ctx,
            "Cell layout: node_index={}, row={}, col={}, width={:.2}",
            cell_info.node_index,
            cell_info.row,
            cell_info.column,
            cell_width
        );

        // Layout the cell to get its height
        let cell_height = layout_cell_for_height(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            cell_width,
            constraints,
        )?;

        debug_table_layout!(
            ctx,
            "Cell height calculated: node_index={}, height={:.2}",
            cell_info.node_index,
            cell_height
        );

        //   row height = max of all single-span cell heights in the row
        if cell_info.rowspan == 1 {
            let current_height = table_ctx.row_heights[cell_info.row];
            table_ctx.row_heights[cell_info.row] = current_height.max(cell_height);
        }

        // +spec:box-model:073652 - Table height: baseline-aligned cells establish row baseline,
        // then top/bottom/middle cells positioned The baseline of a cell is the baseline of
        // its first line box (from inline layout) or the bottom of the content box if no
        // inline content.
        // Only the cells that ARE baseline-aligned set the row's baseline;
        // a middle cell's baseline used to push a baseline cell down.
        let cell_dom = tree
            .get(LayoutNodeId::new(cell_info.node_index))
            .and_then(|n| n.dom_node_id);
        if cell_info.rowspan == 1 && is_baseline_aligned(cell_vertical_align(ctx.styled_dom, cell_dom))
        {
            let cell_baseline = compute_cell_baseline(cell_info.node_index, tree);
            let current_baseline = table_ctx.row_baselines[cell_info.row];
            table_ctx.row_baselines[cell_info.row] = current_baseline.max(cell_baseline);
            baseline_cells.push((cell_info.row, cell_baseline, cell_height));
        }
    }

    // A baseline cell moves down until its first line is on the row's
    // baseline; the row grows to hold it there (CSS 2.2 17.5.3).
    for (row, cell_baseline, cell_height) in baseline_cells {
        let shifted = table_ctx.row_baselines[row] - cell_baseline + cell_height;
        if shifted.is_finite() {
            table_ctx.row_heights[row] = table_ctx.row_heights[row].max(shifted);
        }
    }

    // A row is at least as tall as its own `height` (CSS 2.2 17.5.3).
    for row in 0..table_ctx.num_rows {
        let Some(&row_index) = table_ctx.row_node_indices.get(row) else {
            continue;
        };
        let specified = tree
            .get(LayoutNodeId::new(row_index))
            .and_then(|n| n.dom_node_id)
            .and_then(|dom_id| specified_length_height(ctx.styled_dom, dom_id, ctx.viewport_size));
        if let Some(h) = specified {
            table_ctx.row_heights[row] = table_ctx.row_heights[row].max(h);
        }
    }

    // involved must be great enough to encompass the cell spanning the rows
    // Second pass: Handle cells that span multiple rows (rowspan > 1)
    for cell_info in &table_ctx.cells {
        // Skip cells that start in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }

        if cell_info.rowspan > 1 {
            // The cell's width: its columns and the spacing between them.
            let cell_width = table_ctx.cell_span_width(cell_info);

            // Layout the cell to get its height
            let cell_height = layout_cell_for_height(
                ctx,
                tree,
                text_cache,
                cell_info.node_index,
                cell_width,
                constraints,
            )?;

            // Calculate the current total height of spanned rows (excluding collapsed rows)
            // Clamp to the actual row count: a rowspan extending past the last
            // row would slice row_heights out of bounds (panic on e.g. a
            // rowspan="2" cell in a single-row table).
            let end_row = (cell_info.row + cell_info.rowspan).min(table_ctx.row_heights.len());
            let spanned_rows = (cell_info.row..end_row)
                .filter(|r| !table_ctx.collapsed_rows.contains(r))
                .count();
            // The cell's box also covers the border-spacing between its rows.
            let current_total: f32 = table_ctx.row_heights[cell_info.row..end_row]
                .iter()
                .enumerate()
                .filter(|(idx, _)| !table_ctx.collapsed_rows.contains(&(cell_info.row + idx)))
                .map(|(_, height)| height)
                .sum::<f32>()
                + table_ctx.v_spacing * spanned_rows.saturating_sub(1) as f32;

            // If the cell needs more height, distribute extra height across
            // non-collapsed spanned rows
            if cell_height > current_total {
                let extra_height = cell_height - current_total;

                // Count non-collapsed rows in span
                let non_collapsed_rows = (cell_info.row..end_row)
                    .filter(|row_idx| !table_ctx.collapsed_rows.contains(row_idx))
                    .count();

                if non_collapsed_rows > 0 {
                    let per_row = extra_height / non_collapsed_rows as f32;

                    for row_idx in cell_info.row..end_row {
                        if !table_ctx.collapsed_rows.contains(&row_idx) {
                            table_ctx.row_heights[row_idx] += per_row;
                        }
                    }
                }
            }
        }
    }

    // CSS 2.2 Section 17.6: Final pass - ensure collapsed rows have height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }

    //   visible content, the row has zero height and v-spacing on only one side
    // +spec:table-layout:7370dc - empty-cells:hide in separated borders model
    // +spec:box-model:1e9cf1 - empty-cells:hide rows get zero height with v-spacing on only one
    // side +spec:overflow:a44925 - CSS 2.2 §17.6.1.1: empty-cells:hide suppresses
    // borders/backgrounds; all-hidden rows get zero height +spec:table-layout:dc8bc3 -
    // separated borders model: border-spacing, empty-cells, row zero-height
    if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        for row_idx in 0..table_ctx.num_rows {
            if table_ctx.collapsed_rows.contains(&row_idx) {
                continue;
            }
            // Collect cells in this row
            let row_cells: Vec<usize> = table_ctx
                .cells
                .iter()
                .filter(|c| c.row == row_idx && c.rowspan == 1)
                .map(|c| c.node_index)
                .collect();
            if row_cells.is_empty() {
                continue;
            }
            // +spec:box-model:0ab9b0 - empty-cells:hide suppresses borders/backgrounds, row gets
            // zero height if all cells hidden+empty Check if ALL cells in this row have
            // empty-cells:hide and are empty
            let all_hidden_empty = row_cells.iter().all(|&cell_idx| {
                tree.get(LayoutNodeId::new(cell_idx))
                    .is_none_or(|cell_node| {
                        let ec = get_empty_cells_property(ctx, cell_node);
                        ec == StyleEmptyCells::Hide && is_cell_empty(tree, cell_idx)
                    })
            });
            if all_hidden_empty {
                table_ctx.row_heights[row_idx] = 0.0;
                table_ctx.hidden_empty_rows.insert(row_idx);
            }
        }
    }

    Ok(())
}

/// Give the rows the height they lack together to reach `target` (the
/// table's own height less its border-spacing): in proportion to their
/// heights, evenly when all are empty. Collapsed and hidden-empty rows
/// take nothing.
#[allow(clippy::cast_precision_loss)] // a row count
pub(super) fn stretch_rows_to(table_ctx: &mut TableLayoutContext, target: f32) {
    let rows: Vec<usize> = (0..table_ctx.num_rows.min(table_ctx.row_heights.len()))
        .filter(|r| {
            !table_ctx.collapsed_rows.contains(r) && !table_ctx.hidden_empty_rows.contains(r)
        })
        .collect();
    if rows.is_empty() || !target.is_finite() {
        return;
    }
    let current: f32 = rows.iter().map(|&r| table_ctx.row_heights[r]).sum();
    let extra = target - current;
    if extra <= 0.01 {
        return;
    }
    for &r in &rows {
        let share = if current > 0.0 {
            table_ctx.row_heights[r] / current
        } else {
            1.0 / rows.len() as f32
        };
        table_ctx.row_heights[r] += extra * share;
    }
}

/// Position all cells in the table grid with calculated widths and heights
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose
                                                               // layout/render/parse routine (one
                                                               // branch per case)
pub(super) fn position_table_cells<T: ParsedFontTrait>(
    table_ctx: &TableLayoutContext,
    tree: &mut LayoutTree,
    ctx: &mut LayoutContext<'_, T>,
    table_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<(BTreeMap<usize, LogicalPosition>, Vec<f32>)> {
    debug_log!(ctx, "Positioning table cells in grid");

    let mut positions = BTreeMap::new();

    // +spec:box-model:54e86a - Separated borders model: individual cell borders, border-spacing
    // between cells, empty-cells handling   rows, columns, row groups, column groups cannot
    // have borders (UA must ignore border props);   row/column/rowgroup/colgroup backgrounds
    // are invisible in border-spacing area (table bg shows through);   distance from table edge
    // to edge-cell border = table padding + border-spacing   (table padding is already
    // accounted for by the containing block; h_spacing is the border-spacing) Get border
    // spacing values if border-collapse is separate
    let (h_spacing, v_spacing) = resolve_table_border_spacing(ctx, tree, table_index);

    debug_log!(
        ctx,
        "Border spacing: h={:.2}, v={:.2}",
        h_spacing,
        v_spacing
    );

    // Calculate cumulative column positions (x-offsets) with spacing
    let mut col_positions = vec![0.0; table_ctx.columns.len()];
    let mut x_offset = h_spacing; // Start with spacing on the left
    for (i, col) in table_ctx.columns.iter().enumerate() {
        col_positions[i] = x_offset;
        if let Some(width) = col.computed_width {
            // Collapsed columns: gutters on either side collapse (width is 0, skip spacing)
            if table_ctx.collapsed_columns.contains(&i) {
                // No width, no gutter added
            } else {
                x_offset += width + h_spacing; // Add spacing between columns
            }
        }
    }
    // A right-to-left table runs its columns from the right (CSS 2.2 17.5):
    // every column mirrored in the grid's width, so a cell's left edge is
    // its LAST column's (the lowest x of its columns, read below).
    if table_ctx.rtl {
        let grid_width = x_offset;
        for (i, col) in table_ctx.columns.iter().enumerate() {
            let width = col.computed_width.unwrap_or(0.0);
            col_positions[i] = grid_width - col_positions[i] - width;
        }
    }

    // Calculate cumulative row positions (y-offsets) with spacing
    let mut row_positions = vec![0.0; table_ctx.num_rows];
    let mut y_offset = v_spacing; // Start with spacing on the top
    for (i, &height) in table_ctx.row_heights.iter().enumerate() {
        row_positions[i] = y_offset;
        // Collapsed rows: gutters on either side collapse (height is 0, skip spacing)
        if table_ctx.collapsed_rows.contains(&i) {
            // No height, no gutter added
        } else if table_ctx.hidden_empty_rows.contains(&i) {
            // Hidden-empty row: zero height, only one side of spacing
            // (we already added spacing before this row, so skip the spacing after)
            y_offset += height; // height is 0.0
        } else {
            y_offset += height + v_spacing; // Add spacing between rows
        }
    }

    // The rows, row groups and columns are boxes of the grid: their rects,
    // and the table's positioned children (row groups, rows directly in the
    // table, column groups) - see `place_table_grid_boxes`.
    let row_origins = place_table_grid_boxes(
        table_ctx,
        tree,
        table_index,
        &col_positions,
        &row_positions,
        constraints.writing_mode,
        &mut positions,
    );

    // Position each cell
    for cell_info in &table_ctx.cells {
        let precomputed_cell_baseline = compute_cell_baseline(cell_info.node_index, tree);

        let cell_node = tree
            .get_mut(LayoutNodeId::new(cell_info.node_index))
            .ok_or(LayoutError::InvalidTree)?;

        // Calculate cell position: the left edge of its columns (its first
        // column's, or its last one's in a right-to-left table).
        let span_end = (cell_info.column + cell_info.colspan).min(col_positions.len());
        let x = col_positions
            .get(cell_info.column..span_end)
            .and_then(|spanned| spanned.iter().copied().reduce(f32::min))
            .unwrap_or(0.0);
        let y = row_positions.get(cell_info.row).copied().unwrap_or(0.0);

        // Calculate cell size (sum of spanned columns/rows and the spacing
        // between them) - the width the cell was laid out at.
        let width = table_ctx.cell_span_width(cell_info);

        let mut height = 0.0;
        let end_row = cell_info.row + cell_info.rowspan;
        for row_idx in cell_info.row..end_row {
            if let Some(&row_height) = table_ctx.row_heights.get(row_idx) {
                height += row_height;
                // Add spacing between spanned rows (but not after the last one)
                if row_idx < end_row - 1 {
                    height += v_spacing;
                }
            }
        }

        // Update cell's used size and position
        let writing_mode = constraints.writing_mode;
        // Table layout works in main/cross axes, must convert back to logical width/height

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: BEFORE from_main_cross: width={}, height={}, \
             writing_mode={:?}",
            cell_info.node_index,
            width,
            height,
            writing_mode
        );

        cell_node.used_size = Some(LogicalSize::from_main_cross(height, width, writing_mode));

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: AFTER from_main_cross: used_size={:?}",
            cell_info.node_index,
            cell_node.used_size
        );

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: setting used_size to {}x{} (row_heights={:?})",
            cell_info.node_index,
            width,
            height,
            table_ctx.row_heights
        );

        // Save hot fields needed for vertical alignment before dropping the mutable borrow
        let cell_dom_node_id = cell_node.dom_node_id;
        let cell_box_props = cell_node.box_props.unpack();
        drop(cell_node);

        // +spec:inline-formatting-context:20e8e8 - table cell vertical-align alignment order
        // (baseline first, then top, then bottom/middle) receive extra top or bottom
        // padding; vertical-align determines alignment +spec:inline-formatting-context:
        // 4545e8 - vertical-align on table cells maps to align-content: top→start, bottom→end,
        // middle→center +spec:inline-formatting-context:e216be - vertical-align on table
        // cells (baseline, middle, top, bottom) +spec:positioning:156e49 - table cell
        // vertical-align ordering and extra padding per CSS 2.2 §17.5.3
        // Apply vertical-align to cell content if it has inline layout
        // We need to compute the y_offset using immutable borrows first, then apply it mutably.
        let vertical_align_adjustment = if let Some(warm_node) =
            tree.warm(LayoutNodeId::new(cell_info.node_index))
        {
            if let Some(ref cached_layout) = warm_node.inline_layout_result {
                // (d6h) Materialized: sentinel-safe content measurement.
                let inline_result = cached_layout.materialized();

                // Get vertical-align property from styled_dom
                let vertical_align = if let Some(dom_id) = cell_dom_node_id {
                    let node_state =
                        ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

                    match get_vertical_align_property(ctx.styled_dom, dom_id, &node_state) {
                        MultiValue::Exact(v) => v,
                        _ => StyleVerticalAlign::Baseline,
                    }
                } else {
                    StyleVerticalAlign::Baseline
                };

                // The content's height is its LINE BOXES' extent - what
                // the cell was sized by (`layout_cell_for_height`: the
                // IFC's `ifc_extent`), struts included. Its items' bounds
                // alone left out the strut's descent below an inline-block
                // and centred a cell's only line half of it low.
                let content_height = ifc_extent(&inline_result).height;

                // Get padding and border to calculate content-box height
                // height is border-box, but vertical alignment should be within content-box
                let padding = &cell_box_props.padding;
                let border = &cell_box_props.border;
                let content_box_height = height
                    - padding.main_start(writing_mode)
                    - padding.main_end(writing_mode)
                    - border.main_start(writing_mode)
                    - border.main_end(writing_mode);

                // top: top of cell box aligned with top of first row it spans
                // bottom: bottom of cell box aligned with bottom of last row it spans
                // middle: center of cell aligned with center of rows it spans
                //   the cell is aligned at the baseline instead
                let y_offset = match vertical_align {
                    StyleVerticalAlign::Top => 0.0,
                    StyleVerticalAlign::Middle => (content_box_height - content_height) * 0.5,
                    StyleVerticalAlign::Bottom => content_box_height - content_height,
                    // align with the row baseline. cell_baseline = distance from top of cell box
                    // to cell's baseline; row_baseline = distance from top of row to row's baseline
                    StyleVerticalAlign::Baseline
                    | StyleVerticalAlign::Sub
                    | StyleVerticalAlign::Superscript
                    | StyleVerticalAlign::TextTop
                    | StyleVerticalAlign::TextBottom
                    | StyleVerticalAlign::Percentage(_)
                    | StyleVerticalAlign::Length(_) => {
                        let row_baseline = table_ctx
                            .row_baselines
                            .get(cell_info.row)
                            .copied()
                            .unwrap_or(0.0);
                        (row_baseline - precomputed_cell_baseline).max(0.0)
                    }
                };

                debug_info!(
                    ctx,
                    "[position_table_cells] Cell {}: vertical-align={:?}, border_box_height={}, \
                     content_box_height={}, content_height={}, y_offset={}",
                    cell_info.node_index,
                    vertical_align,
                    height,
                    content_box_height,
                    content_height,
                    y_offset
                );

                if y_offset.abs() > 0.01 {
                    Some((
                        y_offset,
                        cached_layout.available_width,
                        cached_layout.has_floats,
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Apply the vertical alignment adjustment (requires mutable borrow)
        if let Some((y_offset, available_width, has_floats)) = vertical_align_adjustment {
            if let Some(warm_mut) = tree.warm_mut(LayoutNodeId::new(cell_info.node_index)) {
                if let Some(ref cached_layout) = warm_mut.inline_layout_result {
                    use std::sync::Arc;

                    use crate::text3::cache::{PositionedItem, UnifiedLayout};

                    // (d6h) Materialize the retirement sentinel before
                    // adjusting: reading the stored items raw fed EMPTY
                    // back into the rebuilt cache entry (found via
                    // caret_scroll_glide under the d7 default flip).
                    let source_items: Vec<PositionedItem> = if cached_layout.layout.items.is_empty()
                        && cached_layout
                            .dense
                            .as_deref()
                            .is_some_and(|d| !d.clusters.is_empty())
                    {
                        cached_layout
                            .dense
                            .as_deref()
                            .map(text3::dense::DenseText::to_unified_items)
                            .unwrap_or_default()
                    } else {
                        cached_layout.layout.items.clone()
                    };
                    let adjusted_items: Vec<PositionedItem> = source_items
                        .into_iter()
                        .map(|item| PositionedItem {
                            item: item.item,
                            position: text3::cache::Point {
                                x: item.position.x,
                                y: item.position.y + y_offset,
                            },
                            line_index: item.line_index,
                        })
                        .collect();

                    let adjusted_layout = UnifiedLayout {
                        items: adjusted_items,
                        overflow: cached_layout.layout.overflow.clone(),
                    };

                    // Keep the same constraint type from the cached layout
                    let mut cil = CachedInlineLayout::new(
                        Arc::new(adjusted_layout),
                        available_width,
                        has_floats,
                    );
                    // LineShift preserves content; carry the hash so Phase 2d
                    // can still validly fast-path this layout (#11).
                    cil.inline_content_hash = cached_layout.inline_content_hash;
                    warm_mut.inline_layout_result = Some(Box::new(cil));
                    // Vertical-align adjustment changed item positions
                    // within this cell's IFC — patched passes must re-emit.
                    ctx.reflowed_ifcs.insert(cell_info.node_index);
                }
            }
            // The atomic inlines of the line move with it: their boxes were
            // placed from the same layout (`layout_cell_for_height`).
            let atomic_children: Vec<usize> = tree
                .children(cell_info.node_index)
                .iter()
                .copied()
                .filter(|&c| {
                    tree.get(LayoutNodeId::new(c))
                        .is_some_and(|n| !matches!(n.formatting_context, FormattingContext::Inline))
                })
                .collect();
            for c in atomic_children {
                if let Some(pos) = tree
                    .warm_mut(LayoutNodeId::new(c))
                    .and_then(|w| w.relative_position.as_mut())
                {
                    pos.y += y_offset;
                }
            }
        }

        // +spec:inline-formatting-context:4545e8 - vertical-align on a table cell
        // centers/bottom-aligns its *content* within the cell box. The block above
        // only handles cells whose content is direct text (they carry an
        // `inline_layout_result`). A cell whose content is block-level
        // (`<td><p>…</p></td>`, `<td><div>…</div></td>`) has none, so its children
        // stayed top-aligned regardless of `vertical-align`. Shift the cell's
        // in-flow block children by the same offset so the UA default
        // `vertical-align: middle` actually centers block content, like browsers.
        let cell_has_inline = tree
            .warm(LayoutNodeId::new(cell_info.node_index))
            .is_some_and(|w| w.inline_layout_result.is_some());
        if !cell_has_inline {
            let vertical_align = cell_vertical_align(ctx.styled_dom, cell_dom_node_id);
            let children: Vec<usize> = tree.children(cell_info.node_index).to_vec();
            // Natural content height = furthest in-flow child bottom MARGIN
            // edge, measured from the cell content-box top (relative_position
            // is relative to the parent content box). A cell is a BFC root:
            // the last child's bottom margin stays inside it (CSS 2.2
            // 10.6.7), and the row height (`layout_cell_for_height`, the
            // cell's laid-out content height) counts it - measured to the
            // border edge, content that filled its cell was moved down by
            // half its bottom margin.
            let mut content_height = 0.0f32;
            let mut inflow: Vec<usize> = Vec::new();
            for &c in &children {
                let dom_id = tree.get(LayoutNodeId::new(c)).and_then(|n| n.dom_node_id);
                if matches!(
                    get_position_type(ctx.styled_dom, dom_id),
                    LayoutPosition::Absolute | LayoutPosition::Fixed
                ) {
                    continue; // out-of-flow children are unaffected by vertical-align
                }
                let top = tree
                    .warm(LayoutNodeId::new(c))
                    .and_then(|w| w.relative_position)
                    .map_or(0.0, |p| p.y);
                let (h, margin_end) = tree.get(LayoutNodeId::new(c)).map_or((0.0, 0.0), |n| {
                    (
                        n.used_size.map_or(0.0, |s| s.height),
                        n.box_props.unpack().margin.main_end(writing_mode),
                    )
                });
                content_height = content_height.max(top + h + margin_end);
                inflow.push(c);
            }
            let content_box_height = height
                - cell_box_props.padding.main_start(writing_mode)
                - cell_box_props.padding.main_end(writing_mode)
                - cell_box_props.border.main_start(writing_mode)
                - cell_box_props.border.main_end(writing_mode);
            // middle / bottom place the content in the content box; a
            // baseline cell moves its content down until its first line sits
            // on the row's baseline (the line may be deep inside a block,
            // `<td><div>data</div></td>`); top leaves it where it is.
            let y_offset = match vertical_align {
                StyleVerticalAlign::Top => 0.0,
                StyleVerticalAlign::Middle => (content_box_height - content_height) * 0.5,
                StyleVerticalAlign::Bottom => content_box_height - content_height,
                _ => {
                    let row_baseline = table_ctx
                        .row_baselines
                        .get(cell_info.row)
                        .copied()
                        .unwrap_or(0.0);
                    if cell_info.rowspan == 1 {
                        (row_baseline - precomputed_cell_baseline).max(0.0)
                    } else {
                        0.0
                    }
                }
            };
            if y_offset > 0.01 {
                for &c in &inflow {
                    if let Some(w) = tree.warm_mut(LayoutNodeId::new(c)) {
                        if let Some(pos) = w.relative_position.as_mut() {
                            pos.y += y_offset;
                        }
                    }
                }
            }
        }

        // The cell's position in the table, then relative to its row's
        // content box: a cell is its row's child, placed like any child
        // (`position_bfc_child_descendants` adds the row's position). The
        // row's own position is the table's child's (`positions`) or its
        // group's (`place_table_grid_boxes`).
        let position = LogicalPosition::from_main_cross(y, x, writing_mode);
        let (row_origin, row_content_offset) = row_origins
            .get(cell_info.row)
            .copied()
            .unwrap_or_default();
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(cell_info.node_index)) {
            warm.relative_position = Some(LogicalPosition::new(
                position.x - row_origin.x - row_content_offset.x,
                position.y - row_origin.y - row_content_offset.y,
            ));
        }

        debug_log!(
            ctx,
            "Cell at row={}, col={}: pos=({:.2}, {:.2}), size=({:.2}x{:.2})",
            cell_info.row,
            cell_info.column,
            x,
            y,
            width,
            height
        );
    }

    Ok((positions, row_positions))
}

/// The offset of a node's content box inside its border box (its left/top
/// border and padding): where its children's relative positions start.
pub(super) fn content_box_offset(tree: &LayoutTree, index: usize) -> LogicalPosition {
    tree.get(LayoutNodeId::new(index))
        .map_or_else(LogicalPosition::zero, |n| {
            let bp = n.box_props.unpack();
            LogicalPosition::new(bp.border.left + bp.padding.left, bp.border.top + bp.padding.top)
        })
}

/// The row group a row sits in (`None`: the row is the table's child).
pub(super) fn row_group_of(tree: &LayoutTree, row: usize) -> Option<usize> {
    tree.get(LayoutNodeId::new(row))
        .and_then(|n| n.parent)
        .filter(|&p| {
            tree.get(LayoutNodeId::new(p))
                .is_some_and(|n| matches!(n.formatting_context, FormattingContext::TableRowGroup))
        })
}

/// Give the table's rows, row groups, columns and column groups their boxes
/// (CSS 2.1 17.2, 17.5): a row spans the grid's columns and is as tall as
/// its row; a row group spans its rows; a `<col>` spans its column and a
/// `<colgroup>` its columns, over the height of the rows.
///
/// Every box is placed relative to its PARENT's content box, as every other
/// box in the tree is (`position_bfc_child_descendants` and the layout
/// cache walk them that way): row groups, rows directly in the table and
/// column groups go into `positions` (the table's children, relative to the
/// table's content box); a row in a group and a `<col>` get their
/// `relative_position` here. Before this the cells were the table's only
/// positioned descendants, relative to the table itself: rows and row groups
/// had no rect at all, and a later group's rows were reported at the
/// table's top.
///
/// `col_positions` / `row_positions` are the grid's column lefts and row
/// tops relative to the table's content box (spacing included). Returns, per
/// row, its origin relative to the table's content box and its content-box
/// offset - what a cell's position in the table is made relative to.
pub(super) fn place_table_grid_boxes(
    table_ctx: &TableLayoutContext,
    tree: &mut LayoutTree,
    table_index: usize,
    col_positions: &[f32],
    row_positions: &[f32],
    writing_mode: LayoutWritingMode,
    positions: &mut BTreeMap<usize, LogicalPosition>,
) -> Vec<(LogicalPosition, LogicalPosition)> {
    let col_width = |i: usize| {
        table_ctx
            .columns
            .get(i)
            .and_then(|c| c.computed_width)
            .unwrap_or(0.0)
    };
    // The grid's horizontal extent: from the leftmost column's left to the
    // rightmost column's right (the outer spacing is outside every row; in a
    // right-to-left table the first column is the rightmost).
    let grid_left = col_positions
        .iter()
        .copied()
        .reduce(f32::min)
        .unwrap_or(0.0);
    let grid_right = col_positions
        .iter()
        .enumerate()
        .map(|(i, x)| x + col_width(i))
        .fold(grid_left, f32::max);
    let grid_width = (grid_right - grid_left).max(0.0);
    let row_height = |i: usize| table_ctx.row_heights.get(i).copied().unwrap_or(0.0);
    let rows_top = row_positions.first().copied().unwrap_or(0.0);
    let rows_bottom = row_positions
        .iter()
        .enumerate()
        .map(|(i, y)| y + row_height(i))
        .fold(rows_top, f32::max);

    // Rows: their size; a row group's extent; the rows directly in the table.
    let mut group_extent: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    let mut row_origins = Vec::with_capacity(table_ctx.row_node_indices.len());
    for (i, &row) in table_ctx.row_node_indices.iter().enumerate() {
        let top = row_positions.get(i).copied().unwrap_or(0.0);
        let height = row_height(i);
        if let Some(node) = tree.get_mut(LayoutNodeId::new(row)) {
            node.used_size = Some(LogicalSize::from_main_cross(height, grid_width, writing_mode));
        }
        let origin = LogicalPosition::from_main_cross(top, grid_left, writing_mode);
        row_origins.push((origin, content_box_offset(tree, row)));
        match row_group_of(tree, row) {
            Some(group) => {
                group_extent
                    .entry(group)
                    .and_modify(|(t, b)| {
                        *t = t.min(top);
                        *b = b.max(top + height);
                    })
                    .or_insert((top, top + height));
            }
            None => {
                positions.insert(row, origin);
            }
        }
    }

    // Row groups: the extent of their rows; their rows relative to them.
    for (&group, &(top, bottom)) in &group_extent {
        if let Some(node) = tree.get_mut(LayoutNodeId::new(group)) {
            node.used_size = Some(LogicalSize::from_main_cross(
                (bottom - top).max(0.0),
                grid_width,
                writing_mode,
            ));
        }
        positions.insert(
            group,
            LogicalPosition::from_main_cross(top, grid_left, writing_mode),
        );
    }
    for (i, &row) in table_ctx.row_node_indices.iter().enumerate() {
        let Some(group) = row_group_of(tree, row) else {
            continue;
        };
        let Some(&(group_top, _)) = group_extent.get(&group) else {
            continue;
        };
        let group_origin = LogicalPosition::from_main_cross(group_top, grid_left, writing_mode);
        let offset = content_box_offset(tree, group);
        let (origin, _) = row_origins[i];
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(row)) {
            warm.relative_position = Some(LogicalPosition::new(
                origin.x - group_origin.x - offset.x,
                origin.y - group_origin.y - offset.y,
            ));
        }
    }

    // Column groups and columns, in document order over the grid's columns
    // (`span` is not read: one column per `<col>`, a group without `<col>`s
    // is one column).
    let rows_height = (rows_bottom - rows_top).max(0.0);
    let mut next_column = 0usize;
    let groups: Vec<usize> = tree
        .children(table_index)
        .iter()
        .copied()
        .filter(|&c| {
            tree.get(LayoutNodeId::new(c))
                .is_some_and(|n| matches!(n.formatting_context, FormattingContext::TableColumnGroup))
        })
        .collect();
    for group in groups {
        let cols: Vec<usize> = tree.children(group).to_vec();
        let first = next_column;
        let count = cols.len().max(1);
        next_column += count;
        let last = (first + count).min(col_positions.len());
        if first >= last {
            continue;
        }
        let left = (first..last)
            .map(|i| col_positions[i])
            .fold(col_positions[first], f32::min);
        let right = (first..last)
            .map(|i| col_positions[i] + col_width(i))
            .fold(left, f32::max);
        let group_origin = LogicalPosition::from_main_cross(rows_top, left, writing_mode);
        if let Some(node) = tree.get_mut(LayoutNodeId::new(group)) {
            node.used_size = Some(LogicalSize::from_main_cross(
                rows_height,
                (right - left).max(0.0),
                writing_mode,
            ));
        }
        positions.insert(group, group_origin);
        let offset = content_box_offset(tree, group);
        for (k, &col) in cols.iter().enumerate() {
            let column = first + k;
            if column >= last {
                break;
            }
            let origin =
                LogicalPosition::from_main_cross(rows_top, col_positions[column], writing_mode);
            if let Some(node) = tree.get_mut(LayoutNodeId::new(col)) {
                node.used_size = Some(LogicalSize::from_main_cross(
                    rows_height,
                    col_width(column),
                    writing_mode,
                ));
            }
            if let Some(warm) = tree.warm_mut(LayoutNodeId::new(col)) {
                warm.relative_position = Some(LogicalPosition::new(
                    origin.x - group_origin.x - offset.x,
                    origin.y - group_origin.y - offset.y,
                ));
            }
        }
    }

    row_origins
}
