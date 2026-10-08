//! Table formatting contexts (CSS 2.2 17): the table's structure, its properties and the table layout.

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

// Table Formatting Context (CSS 2.2 § 17)
// +spec:display-property:d887c0 - Table wrapper box BFC, caption-side, table grid layout
// (§17.4-17.5) +spec:positioning:930891 - Table formatting context implementation (CSS 2.2 § 17
// introduction)

// +spec:inline-formatting-context:9c272d - CSS table model: row-primary structure,
// display-to-table-element mapping, visual formatting as rectangular grid
/// Lays out a Table Formatting Context.
/// Table column information for layout calculations
#[derive(Copy, Debug, Clone)]
pub struct TableColumnInfo {
    /// Minimum width required for this column
    pub min_width: f32,
    /// Maximum width desired for this column
    pub max_width: f32,
    /// Computed final width for this column
    pub computed_width: Option<f32>,
}

/// Information about a table cell for layout
#[derive(Copy, Debug, Clone)]
pub struct TableCellInfo {
    /// Node index in the layout tree
    pub node_index: usize,
    /// Column index (0-based)
    pub column: usize,
    /// Number of columns this cell spans
    pub colspan: usize,
    /// Row index (0-based)
    pub row: usize,
    /// Number of rows this cell spans
    pub rowspan: usize,
}

/// A `table-column` box (`<col>`), or a column group that has none and
/// stands for its columns itself: the grid columns `start..start + span`.
#[derive(Copy, Debug, Clone)]
pub(crate) struct TableColumnBox {
    /// Layout-tree index of the `<col>` (or of the childless `<colgroup>`).
    pub(crate) node_index: usize,
    /// Layout-tree index of the column group it belongs to.
    pub(crate) group: Option<usize>,
    pub(crate) start: usize,
    pub(crate) span: usize,
}

/// A column group (`<colgroup>`): the grid columns `start..start + span`.
#[derive(Copy, Debug, Clone)]
pub(crate) struct TableColumnGroupBox {
    pub(crate) node_index: usize,
    pub(crate) start: usize,
    pub(crate) span: usize,
}

/// Table layout context - holds all information needed for table layout
#[derive(Debug)]
pub(crate) struct TableLayoutContext {
    /// Information about each column
    pub(crate) columns: Vec<TableColumnInfo>,
    /// Information about each cell
    pub(crate) cells: Vec<TableCellInfo>,
    /// Number of rows in the table
    pub(crate) num_rows: usize,
    /// Whether to use fixed or auto layout algorithm
    pub(crate) use_fixed_layout: bool,
    /// Computed height for each row
    pub(crate) row_heights: Vec<f32>,
    /// Computed baseline offset for each row (distance from row top to row baseline)
    pub(crate) row_baselines: Vec<f32>,
    // +spec:inline-formatting-context:440ca9 - border-collapse/border-spacing/visibility:collapse
    // table properties (CSS 2.2 §17.5-17.6)
    /// Border collapse mode
    pub(crate) border_collapse: StyleBorderCollapse,
    /// Border spacing (only used when `border_collapse` is Separate)
    pub(crate) border_spacing: LayoutBorderSpacing,
    /// CSS 2.2 Section 17.4: Index of table-caption child, if any
    pub(crate) caption_index: Option<usize>,
    //   from display without forcing table re-layout
    /// CSS 2.2 Section 17.6: Rows with visibility:collapse (dynamic effects)
    /// Set of row indices that have visibility:collapse
    pub(crate) collapsed_rows: std::collections::HashSet<usize>,
    /// CSS 2.2 Section 17.6: Columns with visibility:collapse (dynamic effects)
    /// Set of column indices that have visibility:collapse
    pub(crate) collapsed_columns: std::collections::HashSet<usize>,
    /// Rows that are hidden-empty (zero height, border-spacing on only one side)
    pub(crate) hidden_empty_rows: std::collections::HashSet<usize>,
    /// Layout tree indices for each row (row index → layout node index)
    pub(crate) row_node_indices: Vec<usize>,
    /// Per row: the layout-tree index of the row group it sits in (`None`
    /// for a row straight under the table).
    pub(crate) row_groups: Vec<Option<usize>>,
    /// The column boxes (`<col>`), in column order.
    pub(crate) column_boxes: Vec<TableColumnBox>,
    /// The column groups (`<colgroup>`), in column order.
    pub(crate) column_groups: Vec<TableColumnGroupBox>,
    /// Per-column rowspan occupancy: for column `c`, the number of upcoming rows
    /// (including the current one during processing) still covered by a cell that
    /// began in an earlier row with rowspan > 1. Decremented after each row.
    /// Used so a later row's cells skip columns already taken by a spanning cell.
    col_occupied: Vec<usize>,
    /// The used horizontal `border-spacing` (0 in the collapsing model),
    /// resolved once in `layout_table_fc`.
    pub(crate) h_spacing: f32,
    /// The used vertical `border-spacing` (0 in the collapsing model).
    pub(crate) v_spacing: f32,
    /// The table's `direction` is `rtl` (CSS 2.2 17.5): its first column is
    /// the rightmost - the layout places and the painter reads the columns
    /// mirrored. Set by [`analyze_table_structure`].
    pub(crate) rtl: bool,
}

impl TableLayoutContext {
    pub(super) fn new() -> Self {
        Self {
            columns: Vec::new(),
            cells: Vec::new(),
            num_rows: 0,
            use_fixed_layout: false,
            row_heights: Vec::new(),
            row_baselines: Vec::new(),
            border_collapse: StyleBorderCollapse::Separate,
            border_spacing: LayoutBorderSpacing::default(),
            caption_index: None,
            collapsed_rows: std::collections::HashSet::new(),
            collapsed_columns: std::collections::HashSet::new(),
            hidden_empty_rows: std::collections::HashSet::new(),
            row_node_indices: Vec::new(),
            row_groups: Vec::new(),
            column_boxes: Vec::new(),
            column_groups: Vec::new(),
            col_occupied: Vec::new(),
            h_spacing: 0.0,
            v_spacing: 0.0,
            rtl: false,
        }
    }

    /// The grid column after the last column box so far.
    fn column_box_end(&self) -> usize {
        self.column_boxes.last().map_or(0, |c| c.start + c.span)
    }

    /// Append a column box for the next `span` grid columns.
    fn push_column_box(&mut self, node_index: usize, group: Option<usize>, span: usize) {
        let start = self.column_box_end();
        self.column_boxes.push(TableColumnBox {
            node_index,
            group,
            start,
            span: span.max(1),
        });
    }

    /// The last column box has `visibility: collapse` (CSS 2.2 17.6).
    fn collapse_last_column_box(&mut self) {
        if let Some(c) = self.column_boxes.last().copied() {
            self.collapsed_columns.extend(c.start..c.start + c.span);
        }
    }

    /// The column box covering grid column `col`.
    pub(crate) fn column_box_at(&self, col: usize) -> Option<&TableColumnBox> {
        self.column_boxes
            .iter()
            .find(|c| (c.start..c.start + c.span).contains(&col))
    }

    /// The column group covering grid column `col`.
    pub(crate) fn column_group_at(&self, col: usize) -> Option<&TableColumnGroupBox> {
        self.column_groups
            .iter()
            .find(|g| (g.start..g.start + g.span).contains(&col))
    }

    /// A cell's border-box width: its columns and the border-spacing
    /// between them (CSS 2.2 17.6.1) - the width it is laid out at and the
    /// width it is placed at alike.
    #[allow(clippy::cast_precision_loss)] // a column count
    pub(crate) fn cell_span_width(&self, cell: &TableCellInfo) -> f32 {
        let end = (cell.column + cell.colspan).min(self.columns.len());
        let start = cell.column.min(end);
        let widths: f32 = self.columns[start..end]
            .iter()
            .filter_map(|c| c.computed_width)
            .sum();
        widths + self.h_spacing * (end - start).saturating_sub(1) as f32
    }

    /// Per grid slot (`row * columns + column`), the index into `cells` of
    /// the cell covering it.
    pub(crate) fn slot_owners(&self) -> Vec<Option<usize>> {
        let cols = self.columns.len();
        let mut owners = vec![None; self.num_rows * cols];
        for (i, cell) in self.cells.iter().enumerate() {
            for r in cell.row..(cell.row + cell.rowspan).min(self.num_rows) {
                for c in cell.column..(cell.column + cell.colspan).min(cols) {
                    if let Some(slot) = owners.get_mut(r * cols + c) {
                        if slot.is_none() {
                            *slot = Some(i);
                        }
                    }
                }
            }
        }
        owners
    }
}

// +spec:table-layout:c5e446 - table-layout property (auto|fixed) controls layout algorithm
// selection
/// Get the table-layout property for a table node
pub(super) fn get_table_layout_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> LayoutTableLayout {
    let Some(dom_id) = node.dom_node_id else {
        return LayoutTableLayout::Auto;
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_table_layout(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(LayoutTableLayout::Auto)
}

/// Is the table laid out with the fixed table layout (CSS 2.2 17.5.2.1)?
/// `table-layout: fixed` and a width of its own: browsers lay a fixed table
/// whose width is `auto` out automatically.
pub(crate) fn uses_fixed_table_layout<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> bool {
    matches!(get_table_layout_property(ctx, node), LayoutTableLayout::Fixed)
        && table_has_definite_width(ctx, node)
}

/// Does the table have a width of its own (a length, a percentage, a
/// `calc()`)? `auto` and the intrinsic keywords do not count.
pub(super) fn table_has_definite_width<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> bool {
    node.dom_node_id.is_some_and(|dom_id| {
        let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        matches!(
            get_css_width(ctx.styled_dom, dom_id, node_state),
            MultiValue::Exact(LayoutWidth::Px(_) | LayoutWidth::Calc(_))
        )
    })
}

/// Get the border-collapse property for a table node
pub(super) fn get_border_collapse_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> StyleBorderCollapse {
    let Some(dom_id) = node.dom_node_id else {
        return StyleBorderCollapse::Separate;
    };

    // FAST PATH: compact cache
    if let Some(ref cc) = ctx.styled_dom.css_property_cache.ptr.compact_cache {
        return cc.get_border_collapse(dom_id.index());
    }

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_border_collapse(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(StyleBorderCollapse::Separate)
}

/// Get the border-spacing property for a table node
pub(super) fn get_border_spacing_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> LayoutBorderSpacing {
    if let Some(dom_id) = node.dom_node_id {
        // FAST PATH: compact cache
        if let Some(ref cc) = ctx.styled_dom.css_property_cache.ptr.compact_cache {
            let idx = dom_id.index();
            let h_raw = cc.get_border_spacing_h_raw(idx);
            let v_raw = cc.get_border_spacing_v_raw(idx);
            // If both are non-sentinel, use compact values
            if h_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
                && v_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
            {
                return LayoutBorderSpacing::new_separate(
                    azul_css::props::basic::pixel::PixelValue::px(f32::from(h_raw) / 10.0),
                    azul_css::props::basic::pixel::PixelValue::px(f32::from(v_raw) / 10.0),
                );
            }
            // sentinel → fall through to slow path
        }

        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_border_spacing(
            node_data,
            &dom_id,
            &node_state,
        ) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }

    LayoutBorderSpacing::default() // Default: 0
}

/// The table's resolved `border-spacing` `(horizontal, vertical)` in px:
/// `(0, 0)` in the collapsing border model (CSS 2.2 17.6.2) and for an
/// anonymous table (no styled node to resolve font-relative units against).
///
/// The one resolution the table's intrinsic sizes, its width and its cell
/// positions share (it was written out twice in this file).
pub(crate) fn resolve_table_border_spacing<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    table_index: usize,
) -> (f32, f32) {
    let Some(table_node) = tree.get(LayoutNodeId::new(table_index)) else {
        return (0.0, 0.0);
    };
    let Some(table_id) = table_node.dom_node_id else {
        return (0.0, 0.0);
    };
    if get_border_collapse_property(ctx, table_node) == StyleBorderCollapse::Collapse {
        return (0.0, 0.0);
    }
    let spacing = get_border_spacing_property(ctx, table_node);
    let styled_dom = ctx.styled_dom;
    let table_state = &styled_dom.styled_nodes.as_container()[table_id].styled_node_state;
    let spacing_context = ResolutionContext {
        vertical_writing_mode: false,
        element_font_size: get_element_font_size(styled_dom, table_id, table_state),
        parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
        root_font_size: get_root_font_size(styled_dom, table_state),
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        viewport_size: PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    };
    let h = spacing
        .horizontal
        .resolve_with_context(&spacing_context, PropertyContext::Other)
        .max(0.0);
    let v = spacing
        .vertical
        .resolve_with_context(&spacing_context, PropertyContext::Other)
        .max(0.0);
    (
        if h.is_finite() { h } else { 0.0 },
        if v.is_finite() { v } else { 0.0 },
    )
}

/// Get the empty-cells property for a table-cell node.
/// Returns Show (default) or Hide.
pub(crate) fn get_empty_cells_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> StyleEmptyCells {
    let Some(dom_id) = node.dom_node_id else {
        return StyleEmptyCells::Show;
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_empty_cells(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(StyleEmptyCells::Show)
}

/// CSS 2.2 Section 17.4 - Tables in the visual formatting model:
///
/// "The caption box is a block box that retains its own content, padding,
/// border, and margin areas. The caption-side property specifies the position
/// of the caption box with respect to the table box."
///
/// Get the caption-side property for a table node.
/// Returns Top (default) or Bottom.
pub(super) fn get_caption_side_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> StyleCaptionSide {
    specified_caption_side(ctx, node).unwrap_or(StyleCaptionSide::Top) // Default per CSS 2.2
}

/// The `caption-side` the cascade gives `node`, `None` when nothing set it.
pub(super) fn specified_caption_side<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> Option<StyleCaptionSide> {
    let dom_id = node.dom_node_id?;
    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_caption_side(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
}

//   removes entire row or column from display; space made available for other content;
//   spanned content clipped; does not otherwise affect table layout
// +spec:inline-formatting-context:9f5f31 - visibility:collapse for table rows/columns,
// border-collapse and border-spacing
/// CSS 2.2 Section 17.6 - Dynamic row and column effects:
// +spec:box-model:547563 - visibility:collapse removes table rows/columns; elsewhere same as hidden
/// "The 'visibility' value 'collapse' removes a row or column from display,
/// but it has a different effect than 'visibility: hidden' on other elements.
/// When a row or column is collapsed, the space normally occupied by the row
/// or column is removed."
///
/// Check if a node has visibility:collapse set.
///
/// This is used for table rows and columns to optimize dynamic hiding.
/// // +spec:overflow:ebb1f9 - For non-table elements, collapse == hidden (no special handling
/// needed)
pub(super) fn is_visibility_collapsed<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> bool {
    if let Some(dom_id) = node.dom_node_id {
        let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

        if let MultiValue::Exact(value) = get_visibility(ctx.styled_dom, dom_id, &node_state) {
            return matches!(value, StyleVisibility::Collapse);
        }
    }

    false
}

// +spec:overflow:af97a8 - empty-cells in separated borders model; collapsing border overflow
// +spec:table-layout:dcdf1b - empty-cells property controls rendering of borders/backgrounds around
// empty cells in separated borders model
/// CSS 2.2 Section 17.6.1.1 - Borders and Backgrounds around empty cells
///
/// In the separated borders model, the 'empty-cells' property controls the rendering of
/// borders and backgrounds around cells that have no visible content. Empty means it has no
/// children, or has children that are only collapsed whitespace."
///
/// Check if a table cell is empty (has no visible content).
///
/// This is used by the rendering pipeline to decide whether to paint borders/backgrounds
/// when empty-cells: hide is set in separated border model.
//   in-flow content (including empty elements) other than collapsed whitespace
/// A cell is considered empty if:
///
/// - It has no children, OR
/// - It has children but no `inline_layout_result` (no rendered content)
///
/// Note: Full whitespace detection would require checking text content during rendering.
/// This function provides a basic check suitable for layout phase.
pub(crate) fn is_cell_empty(tree: &LayoutTree, cell_index: usize) -> bool {
    if tree.get(LayoutNodeId::new(cell_index)).is_none() {
        return true; // Invalid cell is considered empty
    }

    // No children = empty
    if tree.children(cell_index).is_empty() {
        return true;
    }

    // If cell has an inline layout result, check if it's empty
    if let Some(warm_node) = tree.warm(LayoutNodeId::new(cell_index)) {
        if let Some(ref cached_layout) = warm_node.inline_layout_result {
            // Check if inline layout has any rendered content
            // Empty inline layouts have no items (glyphs/fragments)
            // Note: This is a heuristic - full detection requires text content analysis
            // (d6h) Dense-first: the stored sparse may be the retirement
            // sentinel (empty) while the dense view carries the content.
            if let Some(d) = cached_layout.dense.as_deref() {
                return d.clusters.is_empty();
            }
            return cached_layout.layout.items.is_empty();
        }
    }

    // Check if all children have no content
    // A more thorough check would recursively examine all descendants
    //
    // For now, we use a simple heuristic: if there are children, assume not empty
    // unless proven otherwise by inline_layout_result

    // Cell with children but no inline layout = likely has block-level content = not empty
    false
}

/// Main function to layout a table formatting context
// +spec:table-layout:235e8e - CSS 2.2 §17.1-17.2 table model: fixed/auto algorithms,
// row/column/cell/caption structure +spec:table-layout:a6422d - CSS table model: table structure
// analysis, row/column/cell layout, caption, border-collapse
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if the table's root node has no associated DOM node id.
/// # Errors
///
/// Returns a `LayoutError` if laying out the table fails.
pub fn layout_table_fc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<LayoutOutput> {
    debug_log!(ctx, "Laying out table");

    debug_table_layout!(
        ctx,
        "node_index={}, available_size={:?}, writing_mode={:?}",
        node_index,
        constraints.available_size,
        constraints.writing_mode
    );

    // Multi-pass table layout algorithm:
    //
    // 1. Analyze table structure - identify rows, cells, columns
    // 2. Determine table-layout property (fixed vs auto)
    // 3. Calculate column widths
    // 4. Layout cells and calculate row heights
    // 5. Position cells in final grid

    // Get the table node to read CSS properties
    let table_node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?
        .clone();

    // The table's border-box width for column distribution. Whoever lays the
    // table out decides its used size first and writes it to `used_size`
    // before this runs (`calculate_layout_for_subtree`'s phase 1.5, a flex
    // item's known size, an absolutely positioned box's solved size) - the
    // auto-width rule `max(MIN, min(MAX, available))` and a `width` floored
    // at MIN (CSS 2.1 17.5.2.2) are in `calculate_used_size_for_node`.
    // Re-resolving the table's `width` against `constraints` here (the
    // table's own content box) took a percentage of the wrong box. Only a
    // measurement that set no size (taffy's intrinsic queries) resolves it
    // here, against the constraint it was given.
    let table_border_box_width = if let Some(used) = table_node
        .used_size
        .filter(|size| size.width.is_finite())
    {
        used.width
    } else if let Some(dom_id) = table_node.dom_node_id {
        // Use calculate_used_size_for_node to resolve table width (respects width:100%)
        let intrinsic = tree
            .warm(LayoutNodeId::new(node_index))
            .and_then(|w| w.intrinsic_sizes)
            .unwrap_or_default();
        let containing_block_size = LogicalSize {
            width: constraints.available_size.width,
            height: constraints.available_size.height,
        };

        let table_bp = table_node.box_props.unpack();
        let table_size = crate::solver3::sizing::calculate_used_size_for_node(
            ctx.styled_dom,
            Some(dom_id),
            &CBTY::from_flattened_with_width_type(
                containing_block_size,
                constraints.available_width_type,
            ),
            intrinsic,
            &table_bp,
            &ctx.viewport_size,
        )?;

        table_size.width
    } else {
        constraints.available_size.width
    };

    // Subtract padding and border to get content-box width for column distribution
    let tbp = table_node.box_props.unpack();
    let table_content_box_width = {
        let padding_width = tbp.padding.left + tbp.padding.right;
        let border_width = tbp.border.left + tbp.border.right;
        (table_border_box_width - padding_width - border_width).max(0.0)
    };

    debug_table_layout!(ctx, "Table Layout Debug");
    debug_table_layout!(ctx, "Node index: {}", node_index);
    debug_table_layout!(
        ctx,
        "Available size from parent: {:.2} x {:.2}",
        constraints.available_size.width,
        constraints.available_size.height
    );
    debug_table_layout!(ctx, "Table border-box width: {:.2}", table_border_box_width);
    debug_table_layout!(
        ctx,
        "Table content-box width: {:.2}",
        table_content_box_width
    );
    debug_table_layout!(
        ctx,
        "Table padding: L={:.2} R={:.2}",
        tbp.padding.left,
        tbp.padding.right
    );
    debug_table_layout!(
        ctx,
        "Table border: L={:.2} R={:.2}",
        tbp.border.left,
        tbp.border.right
    );
    debug_table_layout!(ctx, "=");

    // Phase 1: Analyze table structure
    let mut table_ctx = analyze_table_structure(tree, node_index, ctx)?;
    debug_log!(
        ctx,
        "Table structure: {} rows, {} columns, {} cells, caption: {}",
        table_ctx.num_rows,
        table_ctx.columns.len(),
        table_ctx.cells.len(),
        table_ctx.caption_index.is_some()
    );

    // The cell spacing (0 in the collapsing model): the columns share the
    // content width minus one spacing per gutter, the outer two included
    // (CSS 2.2 17.6.1: the table's width runs from the left inner padding
    // edge to the right one, spacing included).
    let (h_spacing, v_spacing) = resolve_table_border_spacing(ctx, tree, node_index);
    #[allow(clippy::cast_precision_loss)] // a column count
    let columns_width = if table_ctx.columns.is_empty() {
        table_content_box_width
    } else {
        (table_content_box_width - h_spacing * (table_ctx.columns.len() + 1) as f32).max(0.0)
    };

    // +spec:table-layout:ff5671 - table-layout property (fixed vs auto) controls column width
    // algorithm +spec:width-calculation:7a5b23 - table-layout property determines fixed vs auto
    // algorithm (CSS 2.2 §17.5.2) Phase 2: Read CSS properties and determine layout algorithm
    let table_layout = get_table_layout_property(ctx, &table_node);
    // The fixed algorithm fixes the table's width: browsers lay a
    // `table-layout: fixed` table whose width is `auto` out automatically.
    table_ctx.use_fixed_layout = uses_fixed_table_layout(ctx, &table_node);

    // +spec:containing-block:cc1453 - collapsing border model: border-collapse property drives
    // table border handling Read border properties
    table_ctx.border_collapse = get_border_collapse_property(ctx, &table_node);
    table_ctx.border_spacing = get_border_spacing_property(ctx, &table_node);
    // The spacing resolved above, for the span widths and the fixed layout.
    table_ctx.h_spacing = h_spacing;
    table_ctx.v_spacing = v_spacing;

    debug_log!(
        ctx,
        "Table layout: {:?}, border-collapse: {:?}, border-spacing: {:?}",
        table_layout,
        table_ctx.border_collapse,
        table_ctx.border_spacing
    );

    // +spec:width-calculation:431d60 - fixed vs auto table layout column width algorithms (CSS 2.2
    // §17.5.2.1, §17.5.2.2) Phase 3: Calculate column widths
    if table_ctx.use_fixed_layout {
        // DEBUG: Log available width passed into fixed column calculation
        debug_table_layout!(
            ctx,
            "FIXED layout: table_content_box_width={:.2}",
            table_content_box_width
        );
        calculate_column_widths_fixed(ctx, tree, &mut table_ctx, columns_width);
    } else {
        // The columns share the content width minus the cell spacing.
        calculate_column_widths_auto_with_width(
            &mut table_ctx,
            tree,
            text_cache,
            ctx,
            constraints,
            columns_width,
        )?;
    }

    debug_table_layout!(ctx, "After column width calculation:");
    debug_table_layout!(ctx, "  Number of columns: {}", table_ctx.columns.len());
    for (i, col) in table_ctx.columns.iter().enumerate() {
        debug_table_layout!(
            ctx,
            "  Column {}: width={:.2}",
            i,
            col.computed_width.unwrap_or(0.0)
        );
    }
    let total_col_width: f32 = table_ctx
        .columns
        .iter()
        .filter_map(|c| c.computed_width)
        .sum();
    debug_table_layout!(ctx, "  Total column width: {:.2}", total_col_width);

    // Phase 4: Calculate row heights based on cell content
    calculate_row_heights(&mut table_ctx, tree, text_cache, ctx, constraints)?;

    // A table taller than its rows (its own `height`, CSS 2.2 17.5.3) gives
    // the rest to its rows, so its cells fill it.
    if let Some(table_dom) = table_node.dom_node_id {
        if let Some(h) = specified_length_height(ctx.styled_dom, table_dom, ctx.viewport_size) {
            let node_state = &ctx.styled_dom.styled_nodes.as_container()[table_dom].styled_node_state;
            let content_h = match get_css_box_sizing(ctx.styled_dom, table_dom, node_state) {
                MultiValue::Exact(azul_css::props::layout::LayoutBoxSizing::BorderBox) => {
                    h - tbp.padding.top - tbp.padding.bottom - tbp.border.top - tbp.border.bottom
                }
                _ => h,
            };
            let spacings = if table_ctx.num_rows == 0 {
                0.0
            } else {
                (table_ctx.num_rows + 1).saturating_sub(table_ctx.hidden_empty_rows.len()) as f32
            };
            let target = content_h - table_ctx.v_spacing * spacings;
            stretch_rows_to(&mut table_ctx, target);
        }
    }

    // Phase 5: Position cells in final grid and collect positions
    // The table's positioned children (row groups, rows, column groups) and
    // the rows' tops, relative to the table's content box.
    let (mut cell_positions, row_tops) =
        position_table_cells(&table_ctx, tree, ctx, node_index, constraints)?;

    // Calculate final table size including border-spacing
    let mut table_width: f32 = table_ctx
        .columns
        .iter()
        .filter_map(|col| col.computed_width)
        .sum();
    let mut table_height: f32 = table_ctx.row_heights.iter().sum();

    debug_table_layout!(
        ctx,
        "After calculate_row_heights: table_height={:.2}, row_heights={:?}",
        table_height,
        table_ctx.row_heights
    );

    // +spec:box-model:494f6b - collapsing border model: row-width formula and table border width
    // computation +spec:box-model:e7d0a3 - Separated borders model: border-spacing,
    // empty-cells, collapsing border width calculation +spec:box-sizing:ee702c - separated
    // borders model: border-spacing between adjoining cells Add border-spacing to table size if
    // border-collapse is separate +spec:box-model:acb81f - separated borders model:
    // border-spacing between adjoining cell borders +spec:box-model:e480b1 - table width = left
    // inner padding edge to right inner padding edge (including border-spacing)
    // The spacing (resolved above, 0 in the collapsing model): one per
    // gutter, the outer two included.
    {
        // Add spacing: left + (n-1 between columns) + right = n+1 spacings
        let num_cols = table_ctx.columns.len();
        if num_cols > 0 {
            table_width += h_spacing * (num_cols + 1) as f32;
        }

        // Add spacing: top + (n-1 between rows) + bottom = n+1 spacings
        if table_ctx.num_rows > 0 {
            let full_spacings = (table_ctx.num_rows + 1) as f32;
            // Each hidden-empty row loses one side of border-spacing
            let hidden_empty_count = table_ctx.hidden_empty_rows.len() as f32;
            table_height += v_spacing * (full_spacings - hidden_empty_count);
        }
    }

    // +spec:table-layout:24dbf9 - §17.4 table wrapper box model: caption positioning, BFC
    // establishment +spec:width-calculation:600f98 - caption-side positions caption above/below
    // table box (CSS 2.2 §17.4) CSS 2.2 Section 17.4: Layout and position the caption if
    // present
    //
    // "The caption box is a block box that retains its own content,
    // padding, border, and margin areas."
    // `caption-side` applies to the caption (CSS 2.2 17.4.1; inherited, so
    // a table's value reaches a caption that sets none): the caption's own
    // value first, the table's otherwise.
    let caption_side = table_ctx
        .caption_index
        .and_then(|caption_idx| tree.get(LayoutNodeId::new(caption_idx)))
        .and_then(|caption| specified_caption_side(ctx, caption))
        .unwrap_or_else(|| get_caption_side_property(ctx, &table_node));
    let mut caption_height = 0.0;
    let mut table_y_offset = 0.0;

    if let Some(caption_idx) = table_ctx.caption_index {
        debug_log!(
            ctx,
            "Laying out caption with caption-side: {:?}",
            caption_side
        );

        // Layout caption as a block with the table's width as available width
        let caption_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: table_width,
                height: constraints.available_size.height,
            },
            writing_mode: constraints.writing_mode,
            writing_mode_ctx: constraints.writing_mode_ctx,
            bfc_state: None, // Caption creates its own BFC
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            available_width_type: Text3AvailableSpace::Definite(table_width),
            fragmentainer: None,
            column_flow: None,
        };

        // Layout the caption node as the block box it is: sized against the
        // table's width (its `used_size`, which paints and hit-tests it),
        // its children laid out inside. Laid out through the formatting
        // context alone, as this was, it kept no size and had no box.
        let mut caption_scrollbar_reflow = false;
        let mut caption_float_cache = HashMap::new();
        let mut caption_positions: super::super::PositionVec = Vec::new();
        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            caption_idx,
            LogicalPosition::zero(),
            &CBTY::from_flattened_with_width_type(
                caption_constraints.available_size,
                caption_constraints.available_width_type,
            ),
            &mut caption_positions,
            &mut caption_scrollbar_reflow,
            &mut caption_float_cache,
            crate::solver3::cache::ComputeMode::ComputeSize,
        )?;
        let caption_margin = tree
            .get(LayoutNodeId::new(caption_idx))
            .map(|n| n.box_props.unpack().margin)
            .unwrap_or_default();
        let caption_box_height = tree
            .get(LayoutNodeId::new(caption_idx))
            .and_then(|n| n.used_size)
            .map_or(0.0, |size| size.height);
        caption_height = caption_box_height + caption_margin.top + caption_margin.bottom;

        // The caption's border box, inside its margins.
        let caption_position = match caption_side {
            StyleCaptionSide::Top => {
                // Caption on top: position at y=0, table starts below caption
                table_y_offset = caption_height;
                LogicalPosition {
                    x: caption_margin.left,
                    y: caption_margin.top,
                }
            }
            StyleCaptionSide::Bottom => {
                // Caption on bottom: table starts at y=0, caption below table
                LogicalPosition {
                    x: caption_margin.left,
                    y: table_height + caption_margin.top,
                }
            }
        };

        // Add caption position to the positions map
        cell_positions.insert(caption_idx, caption_position);

        debug_log!(
            ctx,
            "Caption positioned at x={:.2}, y={:.2}, height={:.2}",
            caption_position.x,
            caption_position.y,
            caption_height
        );
    }

    // Adjust all table cell positions if caption is on top
    if table_y_offset > 0.0 {
        debug_log!(
            ctx,
            "Adjusting table cells by y offset: {:.2}",
            table_y_offset
        );

        // Everything but the caption moves below it: the row groups, the
        // rows and column groups in the map (the cells, rows in groups and
        // columns are relative to those).
        for (&child, pos) in &mut cell_positions {
            if Some(child) != table_ctx.caption_index {
                pos.y += table_y_offset;
            }
        }
    }

    let total_height = table_height + caption_height;

    debug_table_layout!(ctx, "Final table dimensions:");
    debug_table_layout!(ctx, "  Content width (columns): {:.2}", table_width);
    debug_table_layout!(ctx, "  Content height (rows): {:.2}", table_height);
    debug_table_layout!(ctx, "  Caption height: {:.2}", caption_height);
    debug_table_layout!(ctx, "  Total height: {:.2}", total_height);
    debug_table_layout!(ctx, "End Table Debug");

    // CSS 2.2 §10.8.1: the baseline of a table is the baseline of its first
    // in-flow row — used when the table is an `inline-table` aligned on a line.
    // `row_baselines[0]` is that row's baseline measured from the row's top;
    // every row-0 cell shares the row top, so add any row-0 cell's (already
    // caption-adjusted) y position. Falls back to `None` for an empty table,
    // where the caller treats the bottom content edge as the baseline.
    // TODO(superplan): a rowspan cell that *starts* in row 0 but whose content
    // baseline sits in a later row is approximated by `row_baselines[0]` here.
    //
    // A first row with no baseline-aligned cell (the HTML default: cells
    // inherit `vertical-align: middle` from their row) has no baseline of its
    // own; CSS 2.2 17.5.3 puts it at the bottom content edge of the row's
    // lowest cell. Left at 0 (the row's top), an inline-table hung a whole
    // table height below the line it sat on.
    let row0_baseline = table_ctx.row_baselines.first().copied().map(|baseline| {
        let row0_cells: Vec<&TableCellInfo> =
            table_ctx.cells.iter().filter(|c| c.row == 0).collect();
        let any_baseline_cell = row0_cells.iter().any(|c| {
            let dom = tree
                .get(LayoutNodeId::new(c.node_index))
                .and_then(|n| n.dom_node_id);
            is_baseline_aligned(cell_vertical_align(ctx.styled_dom, dom))
        });
        if any_baseline_cell {
            return baseline;
        }
        let row_height = table_ctx.row_heights.first().copied().unwrap_or(0.0);
        let bottom_extras = row0_cells
            .iter()
            .filter(|c| c.rowspan == 1)
            .filter_map(|c| tree.get(LayoutNodeId::new(c.node_index)))
            .map(|n| {
                let bp = n.box_props.unpack();
                bp.padding.bottom + bp.border.bottom
            })
            .fold(f32::INFINITY, f32::min);
        if bottom_extras.is_finite() {
            (row_height - bottom_extras).max(0.0)
        } else {
            row_height
        }
    });
    let table_baseline = row0_baseline.and_then(|row0_baseline| {
        row_tops
            .first()
            .map(|top| top + table_y_offset + row0_baseline)
    });

    // Create output with the table's final size and cell positions
    // +spec:box-model:52fcfe - overflow_size must include borders that spill into margin in
    // collapsing border model
    let output = LayoutOutput {
        overflow_size: LogicalSize {
            width: table_width,
            height: total_height,
        },
        // Cell positions calculated in position_table_cells
        positions: cell_positions,
        // First in-flow row's baseline (CSS 2.2 §10.8.1); None ⇒ bottom edge.
        baseline: table_baseline,
        static_positions: BTreeMap::new(),
    };

    Ok(output)
}

// +spec:display-property:f47f8a - Table structure analysis: caption positioning,
// row/column/row-group traversal per CSS 2.2 §17.4-17.5
/// Analyze the table structure: its caption, its column boxes, its rows (in
/// grid order, each with the row group it belongs to) and its cells with
/// their grid slots (CSS 2.2 17.5 cell placement).
///
/// This is THE placement of a table's grid: the table's layout, the
/// collapsing border model's edge resolution ([`resolve_collapsed_borders`])
/// and the table's painting (`paint_table_items` in `display_list.rs`) all
/// read it, so a cell sits in the same slot for each of them. It reads the
/// tree and the cascade and nothing else.
pub(crate) fn analyze_table_structure<T: ParsedFontTrait>(
    tree: &LayoutTree,
    table_index: usize,
    ctx: &LayoutContext<'_, T>,
) -> Result<TableLayoutContext> {
    let mut table_ctx = TableLayoutContext::new();
    let table_node = tree
        .get(LayoutNodeId::new(table_index))
        .ok_or(LayoutError::InvalidTree)?;
    // CSS 2.2 17.5: the columns run in the table's `direction` (inherited:
    // `<td dir="rtl">` reverses the table inside it).
    table_ctx.rtl = table_node.dom_node_id.is_some_and(|dom_id| {
        let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        matches!(
            get_direction_property(ctx.styled_dom, dom_id, node_state),
            MultiValue::Exact(StyleDirection::Rtl)
        )
    });

    // +spec:width-calculation:0a2766 - table internal elements form rectangular grid of
    // rows/columns (CSS 2.2 §17.5) CSS 2.2 Section 17.4: A table may have one table-caption
    // child. Traverse children to find caption, columns/colgroups, rows, and row groups
    //
    // In VISUAL order (CSS 2.1 17.2): the first `table-header-group` before
    // every other row and row group, the first `table-footer-group` after
    // them, wherever they are in the markup (any further header or footer
    // group is an ordinary row group). Rows are numbered in this order, so
    // the grid, the row positions and the row groups' boxes follow it.
    let group_display = |idx: usize| {
        tree.get(LayoutNodeId::new(idx))
            .filter(|n| matches!(n.formatting_context, FormattingContext::TableRowGroup))
            .and_then(|n| n.dom_node_id)
            .map(|dom_id| crate::solver3::layout_tree::get_display_type(ctx.styled_dom, dom_id))
    };
    let children: Vec<usize> = tree.children(table_index).to_vec();
    let header = children
        .iter()
        .copied()
        .find(|&c| group_display(c) == Some(LayoutDisplay::TableHeaderGroup));
    let footer = children
        .iter()
        .copied()
        .find(|&c| group_display(c) == Some(LayoutDisplay::TableFooterGroup));
    let visual_order: Vec<usize> = header
        .into_iter()
        .chain(
            children
                .iter()
                .copied()
                .filter(|&c| Some(c) != header && Some(c) != footer),
        )
        .chain(footer)
        .collect();
    for child_idx in visual_order {
        let Some(child) = tree.get(LayoutNodeId::new(child_idx)) else {
            continue;
        };
        match child.formatting_context {
            FormattingContext::TableCaption => {
                table_ctx.caption_index = Some(child_idx);
            }
            // CSS 2.2 Section 17.2: column groups contain columns
            FormattingContext::TableColumnGroup => {
                analyze_table_colgroup(tree, child_idx, &mut table_ctx, ctx);
            }
            FormattingContext::TableRow => {
                analyze_table_row(tree, child_idx, None, &mut table_ctx, ctx)?;
            }
            FormattingContext::TableRowGroup => {
                // Process rows within the row group
                for &row_idx in tree.children(child_idx) {
                    let is_row = tree
                        .get(LayoutNodeId::new(row_idx))
                        .is_some_and(|row| matches!(row.formatting_context, FormattingContext::TableRow));
                    if is_row {
                        analyze_table_row(tree, row_idx, Some(child_idx), &mut table_ctx, ctx)?;
                    }
                }
            }
            // A `table-column` straight under the table (no column group).
            _ if is_table_column_box(tree, child_idx) => {
                let span = table_column_span(ctx.styled_dom, child);
                table_ctx.push_column_box(child_idx, None, span);
                if is_visibility_collapsed(ctx, child) {
                    table_ctx.collapse_last_column_box();
                }
            }
            _ => {}
        }
    }
    // A column that only a `<col>` makes - past the last cell - is a grid
    // column when the col gives it a definite LENGTH (it takes that room
    // in the table); one with a percentage, a calc() with one, `auto` or a
    // 0 width names none (browsers; WPT col-definite-size-001). The grid
    // had a column only where a cell was.
    let column_widths = crate::solver3::table_width::column_element_widths(
        ctx.styled_dom,
        tree,
        &table_ctx.column_boxes,
        table_ctx.column_box_end(),
    );
    let definite_columns = column_widths
        .iter()
        .rposition(|w| {
            matches!(w, crate::solver3::table_width::SpecifiedWidth::Fixed(px) if *px > 0.0)
        })
        .map_or(0, |last| last + 1);
    while table_ctx.columns.len() < definite_columns {
        table_ctx.columns.push(TableColumnInfo {
            min_width: 0.0,
            max_width: 0.0,
            computed_width: None,
        });
    }

    // A collapsed column box past the last cell names no grid column.
    let num_cols = table_ctx.columns.len();
    table_ctx.collapsed_columns.retain(|&c| c < num_cols);

    Ok(table_ctx)
}

/// Is this layout node a `table-column` box (`<col>`)? Column boxes
/// establish no formatting context, so the display type tells them apart.
pub(super) fn is_table_column_box(tree: &LayoutTree, index: usize) -> bool {
    tree.warm(LayoutNodeId::new(index))
        .is_some_and(|w| w.computed_style.display == LayoutDisplay::TableColumn)
}

/// How many grid columns a `<col>` / `<colgroup>` stands for: its `span`
/// (carried as `AttributeType::ColSpan`, `Dom::create_col`), 1 by default.
pub(super) fn table_column_span(styled_dom: &StyledDom, node: &LayoutNodeHot) -> usize {
    node.dom_node_id
        .map_or(1, |dom_id| get_cell_spans(styled_dom, dom_id).0)
}

/// Analyze a table column group: its `table-column` children become column
/// boxes; a group without any stands for `span` columns itself (HTML
/// `<colgroup span>`).
///
/// - CSS 2.2 Section 17.2: Column groups contain columns
/// - CSS 2.2 Section 17.6: Columns can have visibility:collapse
pub(super) fn analyze_table_colgroup<T: ParsedFontTrait>(
    tree: &LayoutTree,
    colgroup_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &LayoutContext<'_, T>,
) {
    let Some(colgroup_node) = tree.get(LayoutNodeId::new(colgroup_index)) else {
        return;
    };
    let group_collapsed = is_visibility_collapsed(ctx, colgroup_node);
    let start = table_ctx.column_box_end();
    for &col_idx in tree.children(colgroup_index) {
        if !is_table_column_box(tree, col_idx) {
            continue;
        }
        let Some(col_node) = tree.get(LayoutNodeId::new(col_idx)) else {
            continue;
        };
        table_ctx.push_column_box(
            col_idx,
            Some(colgroup_index),
            table_column_span(ctx.styled_dom, col_node),
        );
        if group_collapsed || is_visibility_collapsed(ctx, col_node) {
            table_ctx.collapse_last_column_box();
        }
    }
    if table_ctx.column_box_end() == start {
        table_ctx.push_column_box(
            colgroup_index,
            Some(colgroup_index),
            table_column_span(ctx.styled_dom, colgroup_node),
        );
        if group_collapsed {
            table_ctx.collapse_last_column_box();
        }
    }
    table_ctx.column_groups.push(TableColumnGroupBox {
        node_index: colgroup_index,
        start,
        span: table_ctx.column_box_end() - start,
    });
}

/// Read the HTML `colspan` / `rowspan` of a table cell from its DOM node.
///
/// These are HTML presentational attributes (`AttributeType::ColSpan`/`RowSpan`
/// on `NodeData`), not CSS properties. Missing or non-positive values default to
/// 1 per the HTML parsing rules. Shared with the table's intrinsic sizing
/// (`sizing::calculate_table_intrinsic_sizes`).
#[allow(clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
pub(crate) fn get_cell_spans(styled_dom: &StyledDom, dom_id: NodeId) -> (usize, usize) {
    let mut colspan = 1usize;
    let mut rowspan = 1usize;
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    for attr in node_data.attributes().as_ref() {
        match attr {
            // Clamp to the HTML limits (colspan 1000, rowspan 65534): an
            // unclamped span grows the column/row vectors unboundedly -> OOM/hang.
            azul_core::dom::AttributeType::ColSpan(n) => colspan = (*n).clamp(1, 1000) as usize,
            azul_core::dom::AttributeType::RowSpan(n) => rowspan = (*n).clamp(1, 65534) as usize,
            _ => {}
        }
    }
    (colspan, rowspan)
}

// +spec:display-property:7f167c - Table grid cell placement: rows fill table top-to-bottom, cells
// placed left-to-right with colspan/rowspan
/// Analyze a table row: place its cells in the grid (CSS 2.2 17.5) and
/// grow the column count. `group` is the layout index of the row group the
/// row sits in (`None` for a row straight under the table).
pub(super) fn analyze_table_row<T: ParsedFontTrait>(
    tree: &LayoutTree,
    row_index: usize,
    group: Option<usize>,
    table_ctx: &mut TableLayoutContext,
    ctx: &LayoutContext<'_, T>,
) -> Result<()> {
    // +spec:inline-formatting-context:3f8091 - table visual layout: cells occupy grid cells,
    // row/column spanning
    let row_node = tree
        .get(LayoutNodeId::new(row_index))
        .ok_or(LayoutError::InvalidTree)?;
    let row_num = table_ctx.num_rows;
    table_ctx.num_rows += 1;
    // Track the layout tree index for this row (for positioning/painting)
    if table_ctx.row_node_indices.len() <= row_num {
        table_ctx.row_node_indices.resize(row_num + 1, 0);
    }
    table_ctx.row_node_indices[row_num] = row_index;
    if table_ctx.row_groups.len() <= row_num {
        table_ctx.row_groups.resize(row_num + 1, None);
    }
    table_ctx.row_groups[row_num] = group;

    // CSS 2.2 Section 17.6: Check if this row has visibility:collapse
    if is_visibility_collapsed(ctx, row_node) {
        table_ctx.collapsed_rows.insert(row_num);
    }

    let mut col_index = 0;

    for &cell_idx in tree.children(row_index) {
        if let Some(cell) = tree.get(LayoutNodeId::new(cell_idx)) {
            if matches!(cell.formatting_context, FormattingContext::TableCell) {
                // Read colspan/rowspan from the cell's HTML attributes (default 1).
                let (colspan, rowspan) = cell
                    .dom_node_id
                    .map_or((1, 1), |dom_id| get_cell_spans(ctx.styled_dom, dom_id));

                // Skip columns still occupied by a rowspan cell from an earlier row,
                // so this cell lands in the next free grid slot (CSS 2.2 §17.5 cell
                // placement). Without this, a cell under a rowspan overlapped it.
                while table_ctx
                    .col_occupied
                    .get(col_index)
                    .is_some_and(|&n| n > 0)
                {
                    col_index += 1;
                }

                let cell_info = TableCellInfo {
                    node_index: cell_idx,
                    column: col_index,
                    colspan,
                    row: row_num,
                    rowspan,
                };

                table_ctx.cells.push(cell_info);

                // Update column count
                let max_col = col_index + colspan;
                while table_ctx.columns.len() < max_col {
                    table_ctx.columns.push(TableColumnInfo {
                        min_width: 0.0,
                        max_width: 0.0,
                        computed_width: None,
                    });
                }

                // Reserve this cell's columns for the rows it spans downward. Store
                // the full rowspan; the end-of-row decrement below turns it into the
                // count of REMAINING rows for subsequent rows to skip.
                if rowspan > 1 {
                    if table_ctx.col_occupied.len() < max_col {
                        table_ctx.col_occupied.resize(max_col, 0);
                    }
                    for occ in &mut table_ctx.col_occupied[col_index..max_col] {
                        *occ = rowspan;
                    }
                }

                col_index += colspan;
            }
        }
    }

    // End of row: one row of every pending rowspan has now been consumed.
    for occ in &mut table_ctx.col_occupied {
        *occ = occ.saturating_sub(1);
    }

    Ok(())
}
