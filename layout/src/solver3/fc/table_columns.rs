//! Table column widths: the fixed and the automatic table layout (CSS 2.2 17.5.2).

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

// +spec:overflow:66f584 - Fixed table layout: cells use overflow property to clip overflowing
// content +spec:positioning:46070a - Fixed table layout (17.5.2.1) and auto table layout (17.5.2.2)
// column width algorithms +spec:table-layout:875401 - Fixed table layout algorithm (17.5.2.1):
// column widths from first-row cells, remaining columns divide space equally, table width =
// max(width property, sum of columns)
/// Calculate column widths using the fixed table layout algorithm
/// // +spec:overflow:de613c - Fixed table layout algorithm (CSS 2.2 Section 17.5.2.1)
// +spec:table-layout:8b72b3 - fixed table layout: column width from column elements/first-row
// cells, remaining columns equal division
///
/// CSS 2.2 Section 17.5.2.1: In fixed table layout, the horizontal layout
/// does not depend on cell contents. Column widths are determined by:
/// 1. Column elements with explicit (non-auto) width
/// 2. First-row cells with explicit (non-auto) width
/// 3. Remaining columns equally divide remaining horizontal space
///
/// CSS 2.2 Section 17.6: Columns with visibility:collapse are excluded
/// from width calculations
// +spec:table-layout:c5e446 - Fixed table layout algorithm: column widths from col elements or
// first-row cells, remaining columns divide equally
/// +spec:width-calculation:8c958a - Fixed table layout: column widths from col elements, first-row
/// cells, then equal distribution (CSS 2.2 §17.5.2.1)
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub(super) fn calculate_column_widths_fixed<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    table_ctx: &mut TableLayoutContext,
    available_width: f32,
) {
    debug_table_layout!(
        ctx,
        "calculate_column_widths_fixed: num_cols={}, available_width={:.2}",
        table_ctx.columns.len(),
        available_width
    );

    let widths = fixed_column_widths(ctx.styled_dom, tree, table_ctx, available_width);
    for (col, width) in table_ctx.columns.iter_mut().zip(widths) {
        col.computed_width = Some(width);
    }
}

/// The fixed table layout's column widths (CSS 2.2 17.5.2.1) when the
/// columns share `available_width` (the table's content width less its cell
/// spacing): a `<col>`'s width, else a first-row cell's, the rest shared
/// equally, a collapsed column 0. They never depend on the cells' content,
/// and they add up to `available_width` unless the widths given want more.
/// The table's layout and its intrinsic sizes
/// ([`fixed_table_content_width`]) both take them from here.
#[allow(clippy::cast_precision_loss)] // a column count
pub(crate) fn fixed_column_widths(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    table_ctx: &TableLayoutContext,
    available_width: f32,
) -> Vec<f32> {
    let num_cols = table_ctx.columns.len();
    let collapsed = &table_ctx.collapsed_columns;
    let visible: Vec<usize> = (0..num_cols).filter(|c| !collapsed.contains(c)).collect();
    if visible.is_empty() {
        return vec![0.0; num_cols];
    }

    // Step 1: a `<col>` with a width sets its column (a column box has no
    // padding and, for its width, no border).
    let mut widths: Vec<Option<f32>> = crate::solver3::table_width::column_element_widths(
        styled_dom,
        tree,
        &table_ctx.column_boxes,
        num_cols,
    )
    .into_iter()
    .enumerate()
    .map(|(c, width)| {
        let dom_id = table_ctx
            .column_box_at(c)
            .and_then(|b| tree.get(LayoutNodeId::new(b.node_index)))
            .and_then(|n| n.dom_node_id)?;
        fixed_layout_width(styled_dom, dom_id, width, 0.0, available_width)
    })
    .collect();

    // Step 2: otherwise a first-row cell with a width sets its column(s) -
    // its width plus its horizontal padding and border; a spanning cell's
    // width covers the spacing between its columns and is split evenly
    // over those still open.
    for cell_info in table_ctx.cells.iter().filter(|c| c.row == 0) {
        if collapsed.contains(&cell_info.column) {
            continue;
        }
        let Some(cell) = tree.get(LayoutNodeId::new(cell_info.node_index)) else {
            continue;
        };
        let Some(dom_id) = cell.dom_node_id else {
            continue;
        };
        let bp = cell.box_props.unpack();
        let h_extras = bp.padding.left + bp.padding.right + bp.border.left + bp.border.right;
        let Some(w) = fixed_layout_width(
            styled_dom,
            dom_id,
            crate::solver3::table_width::specified_width(styled_dom, dom_id, h_extras),
            h_extras,
            available_width,
        ) else {
            continue;
        };
        let span_end = (cell_info.column + cell_info.colspan).min(num_cols);
        let span: Vec<usize> = (cell_info.column..span_end)
            .filter(|c| !collapsed.contains(c))
            .collect();
        let open: Vec<usize> = span
            .iter()
            .copied()
            .filter(|&c| widths[c].is_none())
            .collect();
        if open.is_empty() {
            continue;
        }
        let taken: f32 = span.iter().filter_map(|&c| widths[c]).sum();
        let inner = table_ctx.h_spacing * span.len().saturating_sub(1) as f32;
        let per_column = (w - inner - taken).max(0.0) / open.len() as f32;
        for c in open {
            widths[c] = Some(per_column);
        }
    }

    // Step 3: the other columns share what is left, equally.
    let used: f32 = visible.iter().filter_map(|&c| widths[c]).sum();
    let open: Vec<usize> = visible
        .iter()
        .copied()
        .filter(|&c| widths[c].is_none())
        .collect();
    if !open.is_empty() {
        let per_column = (available_width - used).max(0.0) / open.len() as f32;
        for &c in &open {
            widths[c] = Some(per_column);
        }
    }

    // Step 4: a table wider than its columns (every column has a width)
    // gives them the extra, in proportion to their widths (evenly when all
    // are 0).
    let total: f32 = visible.iter().filter_map(|&c| widths[c]).sum();
    if open.is_empty() && available_width > total {
        let extra = available_width - total;
        for &c in &visible {
            let share = if total > 0.0 {
                widths[c].unwrap_or(0.0) / total
            } else {
                1.0 / visible.len() as f32
            };
            widths[c] = Some(widths[c].unwrap_or(0.0) + extra * share);
        }
    }

    (0..num_cols)
        .map(|c| {
            if collapsed.contains(&c) {
                0.0
            } else {
                widths[c].unwrap_or(0.0)
            }
        })
        .collect()
}

/// A FIXED table's content width as its columns make it (CSS 2.2
/// 17.5.2.1): its own width, or the sum of its columns' widths and the cell
/// spacing when the widths its `<col>`s and first-row cells give want more.
/// `None` for a table laid out automatically.
///
/// This is a fixed table's minimum, the floor its used width never goes
/// below - not its content's minimum (MIN, the automatic layout's floor):
/// the fixed layout does not read the cells' content. Floored at MIN, the
/// 100px table of WPT fixed-table-layout-025 came out 150px wide, its red
/// cells' 2 x 25px of padding making room for themselves.
#[allow(clippy::cast_precision_loss)] // a column count
pub(crate) fn fixed_table_content_width<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    table_index: usize,
    grid: &TableLayoutContext,
) -> Option<f32> {
    let table = tree.get(LayoutNodeId::new(table_index))?;
    if !uses_fixed_table_layout(ctx, table) {
        return None;
    }
    let dom_id = table.dom_node_id?;
    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    // Its own width as a content width; a percentage (or a `calc()`) has no
    // basis here, and its columns' widths are the floor alone.
    let own = match get_css_width(ctx.styled_dom, dom_id, node_state) {
        MultiValue::Exact(LayoutWidth::Px(px)) => {
            let em = get_element_font_size(ctx.styled_dom, dom_id, node_state);
            let rem = get_root_font_size(ctx.styled_dom, node_state);
            crate::solver3::calc::resolve_pixel_value_no_percent(&px, em, rem)
                .filter(|w| w.is_finite())
                .map_or(0.0, |w| {
                    let bp = table.box_props.unpack();
                    let content = match get_css_box_sizing(ctx.styled_dom, dom_id, node_state) {
                        MultiValue::Exact(
                            azul_css::props::layout::LayoutBoxSizing::BorderBox,
                        ) => {
                            w - bp.padding.left
                                - bp.padding.right
                                - bp.border.left
                                - bp.border.right
                        }
                        _ => w,
                    };
                    content.max(0.0)
                })
        }
        _ => 0.0,
    };
    let spacing = if grid.columns.is_empty() {
        0.0
    } else {
        grid.h_spacing * (grid.columns.len() + 1) as f32
    };
    let columns: f32 = fixed_column_widths(ctx.styled_dom, tree, grid, (own - spacing).max(0.0))
        .iter()
        .sum();
    Some((columns + spacing).max(own))
}

/// A cell's (or a `<col>`'s) width for the fixed table layout, as a border
/// box, from its `width` as `table_width::specified_width` read it: a length
/// is that border box; a percentage of the columns' share of the table is the
/// CONTENT width, plus `h_extras` (the padding and border - WPT
/// fixed-table-layout-025/026), or under `box-sizing: border-box` the whole
/// border box. `None` for `auto`.
pub(super) fn fixed_layout_width(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    width: crate::solver3::table_width::SpecifiedWidth,
    h_extras: f32,
    columns_width: f32,
) -> Option<f32> {
    use crate::solver3::table_width::SpecifiedWidth;
    match width {
        SpecifiedWidth::Auto => None,
        SpecifiedWidth::Fixed(w) => Some(w),
        SpecifiedWidth::Percent(percent) => {
            let w = percent / 100.0 * columns_width;
            if !w.is_finite() {
                return None;
            }
            let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
            Some(match get_css_box_sizing(styled_dom, dom_id, node_state) {
                MultiValue::Exact(azul_css::props::layout::LayoutBoxSizing::BorderBox) => {
                    w.max(h_extras)
                }
                _ => w.max(0.0) + h_extras,
            })
        }
    }
}

/// Recursively clear the layout cache for every node in a subtree.
///
/// A fixed-depth walk is not enough: a table cell like
/// `<td><span><a>text</a></span></td>` has 4+ levels once the anonymous IFC
/// wrapper is inserted, and any stale cache below that level would feed a
/// narrow intrinsic width back into `measure_cell_content_width`.
pub(super) fn clear_subtree_cache(
    tree: &LayoutTree,
    cache_map: &mut crate::solver3::cache::LayoutCacheMap,
    root: usize,
) {
    if root < cache_map.entries.len() {
        cache_map.entries[root].clear();
    }
    let child_ids: Vec<usize> = tree.children(root).to_vec();
    for child in child_ids {
        clear_subtree_cache(tree, cache_map, child);
    }
}

/// Measure a cell's content width for a given intrinsic sizing mode.
///
/// CSS 2.2 Section 17.5.2.2: shared helper for min-content and max-content
/// width measurement. Lays out the cell subtree in `ComputeSize` mode and
/// returns the border-box width (content + padding + border).
pub(super) fn measure_cell_content_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints<'_>,
    sizing_mode: text3::cache::AvailableSpace,
) -> Result<f32> {
    let width_type = match sizing_mode {
        text3::cache::AvailableSpace::MinContent => Text3AvailableSpace::MinContent,
        text3::cache::AvailableSpace::MaxContent => Text3AvailableSpace::MaxContent,
        text3::cache::AvailableSpace::Definite(w) => Text3AvailableSpace::Definite(w),
    };
    let cell_constraints = LayoutConstraints {
        available_size: LogicalSize {
            width: sizing_mode.to_f32_for_layout(),
            height: f32::INFINITY,
        },
        writing_mode: constraints.writing_mode,
        writing_mode_ctx: constraints.writing_mode_ctx,
        bfc_state: None,
        text_align: constraints.text_align,
        containing_block_size: constraints.containing_block_size,
        available_width_type: width_type,
        fragmentainer: None,
        column_flow: None,
    };

    let mut temp_positions: super::super::PositionVec = Vec::new();
    let mut temp_scrollbar_reflow = false;
    let mut temp_float_cache = HashMap::new();

    // Clear cached layout for this cell and ALL its descendants so that
    // min/max-content measurement uses unconstrained width, not a stale
    // result from a previous pass with narrower constraints. Deeply nested
    // inlines (`<td><span><a>text</a></span></td>`) need recursion; a fixed
    // 2-level walk left the `<a>` at level 3 with a stale cached 0-width.
    clear_subtree_cache(tree, &mut ctx.cache_map, cell_index);
    // Same for the cell's own size: a table cell's `used_size` is never
    // overwritten by its own layout once set, and `layout_bfc` lays the
    // cell's children out inside it - so without this, a re-layout measured
    // the content inside the column width of the PREVIOUS layout.
    if let Some(cell) = tree.get_mut(LayoutNodeId::new(cell_index)) {
        cell.used_size = None;
    }

    // The measurement is a CONSTRAINT, not a length. The flattened
    // `f32::MAX / 2` above is only the legacy cache key: handed to
    // `from_flattened_with_width_type` it is FINITE, so it came back as a
    // DEFINITE 1.7e38 px containing block. A cell without text (`<td
    // colspan="2"><hr></td>`, a `width: 100%` rule) filled it, both spanned
    // columns came out ~0.85e38 wide and the next column's text was placed
    // ~0.85e38 px to the right: the AzMail receipt lost its price column and
    // the CPU rasterizer panicked on it. Typed, the cell's auto width is its
    // measured contribution and a percentage inside it behaves as auto
    // (css-sizing-3 section 5.2.1).
    let cell_cb = match width_type {
        Text3AvailableSpace::Definite(_) => {
            CBTY::from_flattened_with_width_type(cell_constraints.available_size, width_type)
        }
        indefinite => CBTY::from_axes(
            indefinite,
            Text3AvailableSpace::MaxContent,
            cell_constraints.available_size,
        ),
    };

    // A cell of loose text with only inline-level children IS one inline
    // formatting context (CSS 2.2 9.4.2), and its intrinsic widths are that
    // IFC's: its longest word under the min-content constraint, its longest
    // line under max-content. THIS is the measurement path; the final pass
    // lays the cell out at its column width in `layout_cell_for_height` and
    // never comes here. The generic subtree layout below would run the cell
    // through `layout_bfc`, where the text child is a block-level box sized
    // at its MAX-content width - so the min pass reported the max.
    let cell_is_ifc = tree
        .get(LayoutNodeId::new(cell_index))
        .and_then(|n| n.dom_node_id)
        .is_some_and(|dom_id| cell_is_inline_formatting_context(ctx.styled_dom, dom_id));
    if cell_is_ifc {
        let output = layout_ifc(ctx, text_cache, tree, cell_index, &cell_constraints)?;
        // The measurement's lines are not the cell's: nothing may read a
        // min-content line layout as the final one (`layout_cell_for_height`
        // lays the cell out again at its column width).
        if let Some(warm) = tree.warm_mut(LayoutNodeId::new(cell_index)) {
            warm.inline_layout_result = None;
        }
        let cell_bp = tree
            .get(LayoutNodeId::new(cell_index))
            .ok_or(LayoutError::InvalidTree)?
            .box_props
            .unpack();
        let wm = constraints.writing_mode;
        let content_width = if output.overflow_size.width.is_finite() {
            output.overflow_size.width.max(0.0)
        } else {
            0.0
        };
        return Ok(content_width
            + cell_bp.padding.cross_start(wm)
            + cell_bp.padding.cross_end(wm)
            + cell_bp.border.cross_start(wm)
            + cell_bp.border.cross_end(wm));
    }

    crate::solver3::cache::calculate_layout_for_subtree(
        ctx,
        tree,
        text_cache,
        cell_index,
        LogicalPosition::zero(),
        &cell_cb,
        &mut temp_positions,
        &mut temp_scrollbar_reflow,
        &mut temp_float_cache,
        crate::solver3::cache::ComputeMode::ComputeSize,
    )?;

    let cell_bp = tree
        .get(LayoutNodeId::new(cell_index))
        .ok_or(LayoutError::InvalidTree)?
        .box_props
        .unpack();
    let padding = &cell_bp.padding;
    let border = &cell_bp.border;
    let wm = constraints.writing_mode;

    // For min/max-content measurement, use the overflow content size (actual
    // content width) rather than used_size. used_size for auto-width blocks
    // fills the containing block, which is huge (f32::MAX/2) during
    // intrinsic sizing — that would make every column appear infinitely wide.
    let content_width = tree
        .warm(LayoutNodeId::new(cell_index))
        .and_then(|w| w.overflow_content_size)
        .map_or_else(
            || {
                tree.get(LayoutNodeId::new(cell_index))
                    .and_then(|n| n.used_size)
                    .map_or(0.0, |s| s.width)
            },
            |s| s.width,
        );

    Ok(content_width
        + padding.cross_start(wm)
        + padding.cross_end(wm)
        + border.cross_start(wm)
        + border.cross_end(wm))
}

/// A definite `height` (px, em, rem, vw, ...) of an element, or `None` for
/// `auto` and percentages (no basis here).
pub(super) fn specified_length_height(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    viewport: LogicalSize,
) -> Option<f32> {
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let MultiValue::Exact(LayoutHeight::Px(px)) = get_css_height(styled_dom, dom_id, node_state)
    else {
        return None;
    };
    let em = get_element_font_size(styled_dom, dom_id, node_state);
    let rem = get_root_font_size(styled_dom, node_state);
    crate::solver3::calc::resolve_pixel_value_no_percent_with_viewport(
        &px,
        em,
        rem,
        viewport.width,
        viewport.height,
    )
    .filter(|h| h.is_finite())
    .map(|h| h.max(0.0))
}

/// A table cell's specified `height` as a BORDER-box length (`None` for
/// `auto` and percentages): the cell's row is at least that tall.
pub(crate) fn cell_specified_border_box_height(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    bp: &BoxProps,
    viewport: LogicalSize,
) -> Option<f32> {
    let h = specified_length_height(styled_dom, dom_id, viewport)?;
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let extras = bp.padding.top + bp.padding.bottom + bp.border.top + bp.border.bottom;
    Some(match get_css_box_sizing(styled_dom, dom_id, node_state) {
        MultiValue::Exact(azul_css::props::layout::LayoutBoxSizing::BorderBox) => h.max(extras),
        _ => h + extras,
    })
}

/// Measure a cell's minimum and maximum content widths for its column (CSS
/// 2.2 17.5.2.2): the content laid out with maximum wrapping and without
/// wrapping, as border-box widths. The cell's own `width` is read beside
/// them by the column model (`table_width::specified_width`).
pub(super) fn measure_cell_widths<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<(f32, f32)> {
    let min_content = measure_cell_content_width(
        ctx,
        tree,
        text_cache,
        cell_index,
        constraints,
        text3::cache::AvailableSpace::MinContent,
    )?;
    let max_content = measure_cell_content_width(
        ctx,
        tree,
        text_cache,
        cell_index,
        constraints,
        text3::cache::AvailableSpace::MaxContent,
    )?;
    Ok((min_content, max_content.max(min_content)))
}

/// Calculate column widths using the auto table layout algorithm
pub(super) fn calculate_column_widths_auto<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints<'_>,
) -> Result<()> {
    calculate_column_widths_auto_with_width(
        table_ctx,
        tree,
        text_cache,
        ctx,
        constraints,
        constraints.available_size.width,
    )
}

/// Calculate column widths using the auto table layout algorithm with explicit table width
// +spec:display-property:05c8e8 - CSS 2.2 §17.5.2.2 automatic table layout: column min/max widths,
// table width = max(W or CB, CAPMIN, MIN), extra width distributed over columns
/// +spec:overflow:29edde - CSS 2.2 §17.5.2.2 automatic table layout: MCW/max-content per cell,
/// column min/max, colspan distribution, final width determination
// +spec:table-layout:23a215 - automatic table layout: MCW/max cell widths, column min/max, colspan
// distribution, table width from MAX/MIN/CAPMIN +spec:table-layout:5e1145 - Automatic table layout:
// MCW/max-content per cell, column min/max, colspan distribution, final width from MIN/MAX
// +spec:width-calculation:42dfca - CSS 2.2 §17.5.2.2 automatic table layout: MCW/max-content per
// cell, column min/max, multi-span distribution, final table width
/// +spec:width-calculation:335ef1 - Automatic table layout: width given by column widths and
/// borders (CSS 2.2 §17.5.2.2)
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub(super) fn calculate_column_widths_auto_with_width<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints<'_>,
    table_width: f32,
) -> Result<()> {
    // Auto layout: calculate min/max content width for each cell
    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return Ok(());
    }

    // Step 1: every cell's min/max-content: a one-column cell's into its
    // column, a spanning cell's kept for Step 2, where it is spread over its
    // columns after every one-column cell (and every `width`) is in, by
    // increasing span. (Measured and spread in document order, a spanning
    // cell spread its demand over columns whose own cells were not measured
    // yet, and the columns came out wider than any cell needed.) Cells in,
    // or spanning into, collapsed columns take no part (CSS 2.2 17.6).
    let mut spanning: Vec<(TableCellInfo, f32, f32)> = Vec::new();
    for cell_info in table_ctx.cells.clone() {
        if (cell_info.column..cell_info.column + cell_info.colspan)
            .any(|c| table_ctx.collapsed_columns.contains(&c))
        {
            continue;
        }
        let (min_width, max_width) =
            measure_cell_widths(ctx, tree, text_cache, cell_info.node_index, constraints)?;
        if cell_info.colspan == 1 {
            let col = &mut table_ctx.columns[cell_info.column];
            col.min_width = col.min_width.max(min_width);
            col.max_width = col.max_width.max(max_width);
        } else {
            spanning.push((cell_info, min_width, max_width));
        }
    }
    // Step 2: the columns' constraints (CSS Tables 3 3.8) - the measured
    // min/max-content above, plus what the cells' and the `<col>`s' `width`
    // make of a column (a constrained column, a percentage column) - and the
    // distribution of the table's width over them (3.9.3), both in
    // `table_width`, which the table's intrinsic sizes use too. Every column
    // used to get a share of the excess in proportion to its max-content
    // whatever its `width` said (`<td width="100">a</td><td>a</td>` in a
    // 400px table came out 200 / 200, not 100 / 300), and percentages were
    // not read at all.
    use crate::solver3::table_width as tw;

    let mut accumulators = vec![tw::ColumnAccumulator::default(); num_cols];
    for cell_info in &table_ctx.cells {
        if cell_info.colspan != 1 || cell_info.column >= num_cols {
            continue;
        }
        let Some(cell) = tree.get(LayoutNodeId::new(cell_info.node_index)) else {
            continue;
        };
        let Some(dom_id) = cell.dom_node_id else {
            continue;
        };
        let bp = cell.box_props.unpack();
        let h_extras = bp.padding.left + bp.padding.right + bp.border.left + bp.border.right;
        accumulators[cell_info.column].add_width(tw::specified_width(
            ctx.styled_dom,
            dom_id,
            h_extras,
        ));
    }
    for (accumulator, width) in accumulators.iter_mut().zip(tw::column_element_widths(
        ctx.styled_dom,
        tree,
        &table_ctx.column_boxes,
        num_cols,
    )) {
        accumulator.add_width(width);
    }

    let mut column_constraints: Vec<tw::ColumnConstraint> = table_ctx
        .columns
        .iter()
        .zip(accumulators)
        .enumerate()
        .map(|(idx, (col, mut accumulator))| {
            if table_ctx.collapsed_columns.contains(&idx) {
                return tw::ColumnConstraint::default();
            }
            accumulator.raise(col.min_width, col.max_width);
            accumulator.finish()
        })
        .collect();
    // The spanning cells, after every one-column cell, by increasing span:
    // their min/max-content and their own `width` spread over the columns
    // they span (`table_width::distribute_spanning_cell`, the rule the
    // table's intrinsic sizes use too).
    spanning.sort_by_key(|(cell, _, _)| cell.colspan);
    for (cell_info, min_width, max_width) in spanning {
        let width = tree
            .get(LayoutNodeId::new(cell_info.node_index))
            .and_then(|cell| {
                let dom_id = cell.dom_node_id?;
                let bp = cell.box_props.unpack();
                let h_extras =
                    bp.padding.left + bp.padding.right + bp.border.left + bp.border.right;
                Some(tw::specified_width(ctx.styled_dom, dom_id, h_extras))
            })
            .unwrap_or(tw::SpecifiedWidth::Auto);
        tw::distribute_spanning_cell(
            &mut column_constraints,
            cell_info.column,
            cell_info.colspan,
            min_width,
            max_width,
            width,
            table_ctx.h_spacing,
            &table_ctx.collapsed_columns,
        );
    }
    tw::clamp_percentages(&mut column_constraints);
    let widths = tw::distribute_to_columns(&column_constraints, table_width);

    debug_table_layout!(
        ctx,
        "calculate_column_widths_auto: table_width={:.2}, constraints={:?}, widths={:?}",
        table_width,
        column_constraints,
        widths
    );

    for (col_idx, (col, width)) in table_ctx.columns.iter_mut().zip(widths).enumerate() {
        col.computed_width = Some(if table_ctx.collapsed_columns.contains(&col_idx) {
            0.0
        } else {
            width
        });
    }

    Ok(())
}
