//! Table borders: the separated and the collapsing border models (CSS 2.2 17.6).

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

// +spec:table-layout:485791 - Six superimposed table layers: table, column-group, column,
// row-group, row, cell (bottom to top) +spec:table-layout:dcdf1b - Collapsing border model: border
// conflict resolution uses layer priority (cell > row > row-group > column > column-group > table)
/// Source of a border in the border conflict resolution algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderSource {
    Table = 0,
    ColumnGroup = 1,
    Column = 2,
    RowGroup = 3,
    Row = 4,
    Cell = 5,
}

/// Information about a border for conflict resolution
#[derive(Copy, Debug, Clone)]
pub struct BorderInfo {
    pub width: f32,
    pub style: BorderStyle,
    pub color: ColorU,
    pub source: BorderSource,
}

impl BorderInfo {
    #[must_use]
    pub const fn new(width: f32, style: BorderStyle, color: ColorU, source: BorderSource) -> Self {
        Self {
            width,
            style,
            color,
            source,
        }
    }

    // +spec:block-formatting-context:f772ae - border style priority for table border conflict
    // resolution
    /// Get the priority of a border style for conflict resolution
    /// Higher number = higher priority
    #[must_use]
    pub const fn style_priority(style: &BorderStyle) -> u8 {
        match style {
            BorderStyle::Hidden => 255, // Highest - suppresses all borders
            BorderStyle::None => 0,     // Lowest - loses to everything
            BorderStyle::Double => 8,
            BorderStyle::Solid => 7,
            BorderStyle::Dashed => 6,
            BorderStyle::Dotted => 5,
            BorderStyle::Ridge => 4,
            BorderStyle::Outset => 3,
            BorderStyle::Groove => 2,
            BorderStyle::Inset => 1,
        }
    }

    // +spec:box-model:2255c2 - Collapsing border conflict resolution (hidden wins, then none loses,
    // then wider wins, then style priority) +spec:box-model:b42c79 - border conflict
    // resolution: hidden wins, then wider, then style priority, then source +spec:box-model:
    // 503e9e - border conflict resolution: hidden wins, then wider, then style priority, then
    // source priority +spec:box-model:7eb217 - Border conflict resolution: hidden > none <
    // wider > style priority > source priority > left/top +spec:overflow:1fb482 - Border
    // conflict resolution per CSS 2.2 §17.6.2.1 (hidden wins, then wider, then style priority, then
    // source priority) +spec:table-layout:882560 - Border conflict resolution (17.6.2.1):
    // hidden wins, none loses, wider wins, style priority, source priority
    /// Compare two borders for conflict resolution per CSS 2.2 Section 17.6.2.1
    /// Returns the winning border
    // +spec:table-layout:21053b - border conflict resolution: hidden suppresses all, style
    // priorities +spec:table-layout:076617 - border conflict resolution algorithm and border
    // style semantics in collapsing model
    #[must_use]
    pub fn resolve_conflict(a: &Self, b: &Self) -> Option<Self> {
        // 1. 'hidden' wins and suppresses all borders
        if a.style == BorderStyle::Hidden || b.style == BorderStyle::Hidden {
            return None;
        }

        // 2. Filter out 'none' - if both are none, no border
        let a_is_none = a.style == BorderStyle::None;
        let b_is_none = b.style == BorderStyle::None;

        if a_is_none && b_is_none {
            return None;
        }
        if a_is_none {
            return Some(*b);
        }
        if b_is_none {
            return Some(*a);
        }

        // 3. Wider border wins
        if a.width > b.width {
            return Some(*a);
        }
        if b.width > a.width {
            return Some(*b);
        }

        // 4. If same width, compare style priority
        let a_priority = Self::style_priority(&a.style);
        let b_priority = Self::style_priority(&b.style);

        if a_priority > b_priority {
            return Some(*a);
        }
        if b_priority > a_priority {
            return Some(*b);
        }

        // 5. If same style, source priority:
        // Cell > Row > RowGroup > Column > ColumnGroup > Table
        if a.source > b.source {
            return Some(*a);
        }
        if b.source > a.source {
            return Some(*b);
        }

        // 6. Same priority - prefer first one (left/top in LTR)
        Some(*a)
    }
}

/// Get border information for a node
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub(crate) fn get_border_info<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
    source: BorderSource,
) -> (BorderInfo, BorderInfo, BorderInfo, BorderInfo) {
    use azul_css::props::{
        basic::{
            pixel::{PhysicalSize, PropertyContext, ResolutionContext},
            ColorU,
        },
        style::BorderStyle,
    };
    use get_element_font_size;
    use get_parent_font_size;
    use get_root_font_size;

    let default_border = BorderInfo::new(
        0.0,
        BorderStyle::None,
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        },
        source,
    );

    let Some(dom_id) = node.dom_node_id else {
        return (
            default_border,
            default_border,
            default_border,
            default_border,
        );
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let cache = &ctx.styled_dom.css_property_cache.ptr;

    // FAST PATH: compact cache for normal state - unless a width is one the
    // cache could not resolve (`1in`, `0.25em`: a sentinel meaning "ask the
    // cascade", `getters::compact_border_width_needs_cascade`). Decoded as 0
    // here, a cell's `border-top: 1in` was no collapsed edge at all (WPT
    // collapsing-border-model-003: the table 48px short).
    let compact = cache.compact_cache.as_ref().filter(|cc| {
        let idx = dom_id.index();
        [
            cc.get_border_top_width_raw(idx),
            cc.get_border_right_width_raw(idx),
            cc.get_border_bottom_width_raw(idx),
            cc.get_border_left_width_raw(idx),
        ]
        .into_iter()
        .all(|raw| !crate::solver3::getters::compact_border_width_needs_cascade(raw))
    });
    if let Some(cc) = compact {
        let idx = dom_id.index();

        // Border styles from packed u16
        let bts = cc.get_border_top_style(idx);
        let brs = cc.get_border_right_style(idx);
        let bbs = cc.get_border_bottom_style(idx);
        let bls = cc.get_border_left_style(idx);

        // Border colors from u32 RGBA
        let make_color = |raw: u32| -> ColorU {
            if raw == 0 {
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                }
            } else {
                ColorU {
                    r: ((raw >> 24) & 0xFF) as u8,
                    g: ((raw >> 16) & 0xFF) as u8,
                    b: ((raw >> 8) & 0xFF) as u8,
                    a: (raw & 0xFF) as u8,
                }
            }
        };

        let btc = make_color(cc.get_border_top_color_raw(idx));
        let brc = make_color(cc.get_border_right_color_raw(idx));
        let bbc = make_color(cc.get_border_bottom_color_raw(idx));
        let blc = make_color(cc.get_border_left_color_raw(idx));

        // Border widths from i16 × 10
        let decode_width = |raw: i16| -> f32 {
            if raw >= azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                0.0 // sentinel → fall back to 0
            } else {
                f32::from(raw) / 10.0
            }
        };

        let btw = decode_width(cc.get_border_top_width_raw(idx));
        let brw = decode_width(cc.get_border_right_width_raw(idx));
        let bbw = decode_width(cc.get_border_bottom_width_raw(idx));
        let blw = decode_width(cc.get_border_left_width_raw(idx));

        let top = if bts == BorderStyle::None {
            default_border
        } else {
            BorderInfo::new(btw, bts, btc, source)
        };
        let right = if brs == BorderStyle::None {
            default_border
        } else {
            BorderInfo::new(brw, brs, brc, source)
        };
        let bottom = if bbs == BorderStyle::None {
            default_border
        } else {
            BorderInfo::new(bbw, bbs, bbc, source)
        };
        let left = if bls == BorderStyle::None {
            default_border
        } else {
            BorderInfo::new(blw, bls, blc, source)
        };

        return resolve_system_border_colors((top, right, bottom, left), cache);
    }

    // SLOW PATH: full cascade resolution
    let cache = &ctx.styled_dom.css_property_cache.ptr;

    // Create resolution context for border-width (em/rem support, no % support)
    let element_font_size = get_element_font_size(ctx.styled_dom, dom_id, &node_state);
    let parent_font_size = get_parent_font_size(ctx.styled_dom, dom_id, &node_state);
    let root_font_size = get_root_font_size(ctx.styled_dom, &node_state);

    let resolution_context = ResolutionContext {
        vertical_writing_mode: false,
        element_font_size,
        parent_font_size,
        root_font_size,
        // Not used for border-width
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        // Not used for border-width
        element_size: None,
        viewport_size: PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    };

    // Top border
    let top = cache
        .get_border_top_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map_or_else(
            || default_border,
            |style_val| {
                let width = cache
                    .get_border_top_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map_or(0.0, |w| {
                        w.inner
                            .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                    });
                let color = cache
                    .get_border_top_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map_or(
                        ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                        |c| c.inner,
                    );
                BorderInfo::new(width, style_val.inner, color, source)
            },
        );

    // Right border
    let right = cache
        .get_border_right_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map_or_else(
            || default_border,
            |style_val| {
                let width = cache
                    .get_border_right_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map_or(0.0, |w| {
                        w.inner
                            .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                    });
                let color = cache
                    .get_border_right_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map_or(
                        ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                        |c| c.inner,
                    );
                BorderInfo::new(width, style_val.inner, color, source)
            },
        );

    // Bottom border
    let bottom = cache
        .get_border_bottom_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map_or_else(
            || default_border,
            |style_val| {
                let width = cache
                    .get_border_bottom_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map_or(0.0, |w| {
                        w.inner
                            .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                    });
                let color = cache
                    .get_border_bottom_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map_or(
                        ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                        |c| c.inner,
                    );
                BorderInfo::new(width, style_val.inner, color, source)
            },
        );

    // Left border
    let left = cache
        .get_border_left_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map_or_else(
            || default_border,
            |style_val| {
                let width = cache
                    .get_border_left_width(node_data, &dom_id, &node_state)
                    .and_then(|w| w.get_property())
                    .map_or(0.0, |w| {
                        w.inner
                            .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                    });
                let color = cache
                    .get_border_left_color(node_data, &dom_id, &node_state)
                    .and_then(|c| c.get_property())
                    .map_or(
                        ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                        |c| c.inner,
                    );
                BorderInfo::new(width, style_val.inner, color, source)
            },
        );

    resolve_system_border_colors((top, right, bottom, left), cache)
}

/// The four edges with a `border-*-color: system:<slot>` token resolved
/// against the cascade's own context - the compact cache and the slow
/// cascade both hand back the colour as declared, and for a `system:`
/// keyword that is a token, not a colour.
pub(super) fn resolve_system_border_colors(
    edges: (BorderInfo, BorderInfo, BorderInfo, BorderInfo),
    cache: &azul_core::prop_cache::CssPropertyCache,
) -> (BorderInfo, BorderInfo, BorderInfo, BorderInfo) {
    let ctx = cache.dynamic_context.as_deref();
    let resolve = |mut edge: BorderInfo| {
        edge.color = azul_css::dynamic_selector::resolve_system_color_token(edge.color, ctx);
        edge
    };
    (
        resolve(edges.0),
        resolve(edges.1),
        resolve(edges.2),
        resolve(edges.3),
    )
}

/// The collapsing border model's grid edges (CSS 2.2 17.6.2): for every
/// edge between two grid slots, or between a slot and the outside of the
/// table, the ONE border that wins there (`None`: no border - every
/// participant is `none`, one of them is `hidden`, or the edge runs inside
/// a spanning cell).
#[derive(Debug, Clone, Default)]
pub(crate) struct CollapsedBorders {
    pub(crate) num_rows: usize,
    pub(crate) num_cols: usize,
    /// `(num_rows + 1) * num_cols` edges: the edge above row `r` in column
    /// `c` is `r * num_cols + c` (row line `num_rows` is the bottom edge).
    pub(crate) horizontal: Vec<Option<BorderInfo>>,
    /// `num_rows * (num_cols + 1)` edges: the edge on column line `c` -
    /// before column `c` in the table's direction - in row `r` is
    /// `r * (num_cols + 1) + c` (column line `num_cols` is the end edge).
    /// Line `c` is the LEFT of column `c` in an ltr table, its RIGHT in an
    /// rtl one ([`Self::rtl`]).
    pub(crate) vertical: Vec<Option<BorderInfo>>,
    /// The table's `direction` is rtl: its columns run from the right
    /// (`TableLayoutContext::rtl`), so a column line's physical side flips.
    pub(crate) rtl: bool,
}

impl CollapsedBorders {
    /// The edge above row `row_line` (the table's bottom edge at
    /// `num_rows`) in column `col`.
    pub(crate) fn horizontal_at(&self, row_line: usize, col: usize) -> Option<BorderInfo> {
        if col >= self.num_cols || row_line > self.num_rows {
            return None;
        }
        self.horizontal
            .get(row_line * self.num_cols + col)
            .copied()
            .flatten()
    }

    /// The edge on column line `col_line` (the table's end edge at
    /// `num_cols`; see [`Self::vertical`]) in row `row`.
    pub(crate) fn vertical_at(&self, row: usize, col_line: usize) -> Option<BorderInfo> {
        if col_line > self.num_cols || row >= self.num_rows {
            return None;
        }
        self.vertical
            .get(row * (self.num_cols + 1) + col_line)
            .copied()
            .flatten()
    }

    /// A cell's used border widths: half of the widest edge along each of
    /// its sides (a spanning cell touches several).
    pub(crate) fn cell_border(&self, cell: &TableCellInfo) -> EdgeSizes {
        let row_end = (cell.row + cell.rowspan).min(self.num_rows);
        let col_end = (cell.column + cell.colspan).min(self.num_cols);
        // The line before the cell is its left in ltr, its right in rtl.
        let (left_line, right_line) = if self.rtl {
            (col_end, cell.column)
        } else {
            (cell.column, col_end)
        };
        EdgeSizes {
            top: half_of_widest((cell.column..col_end).map(|c| self.horizontal_at(cell.row, c))),
            bottom: half_of_widest((cell.column..col_end).map(|c| self.horizontal_at(row_end, c))),
            left: half_of_widest(
                (cell.row..row_end).map(|r| self.vertical_at(r, left_line)),
            ),
            right: half_of_widest(
                (cell.row..row_end).map(|r| self.vertical_at(r, right_line)),
            ),
        }
    }

    /// The table's used border widths: half of the widest edge on each of
    /// its sides; the other half of every outer edge spills into the margin
    /// (CSS 2.2 17.6.2, as browsers take it for all four sides).
    pub(crate) fn table_border(&self) -> EdgeSizes {
        let (left_line, right_line) = if self.rtl {
            (self.num_cols, 0)
        } else {
            (0, self.num_cols)
        };
        EdgeSizes {
            top: half_of_widest((0..self.num_cols).map(|c| self.horizontal_at(0, c))),
            bottom: half_of_widest(
                (0..self.num_cols).map(|c| self.horizontal_at(self.num_rows, c)),
            ),
            left: half_of_widest((0..self.num_rows).map(|r| self.vertical_at(r, left_line))),
            right: half_of_widest(
                (0..self.num_rows).map(|r| self.vertical_at(r, right_line)),
            ),
        }
    }
}

/// Half the width of the widest of `edges` (0 without any).
pub(super) fn half_of_widest(edges: impl Iterator<Item = Option<BorderInfo>>) -> f32 {
    edges
        .map(|e| e.map_or(0.0, |b| b.width))
        .fold(0.0f32, f32::max)
        * 0.5
}

/// The border that wins one grid edge (CSS 2.2 17.6.2.1) among
/// `participants`, listed left / top first: `hidden` anywhere suppresses
/// the edge, a `none` or zero-width border takes no part, then
/// [`BorderInfo::resolve_conflict`] decides - width, style, element - and
/// on a full tie the earlier (further left / further up) one stays.
pub(super) fn collapse_edge(participants: &[BorderInfo]) -> Option<BorderInfo> {
    if participants.iter().any(|b| b.style == BorderStyle::Hidden) {
        return None;
    }
    let mut winner: Option<BorderInfo> = None;
    for b in participants {
        if b.style == BorderStyle::None || b.width <= 0.0 {
            continue;
        }
        winner = Some(match winner {
            None => *b,
            Some(w) => BorderInfo::resolve_conflict(&w, b).unwrap_or(w),
        });
    }
    winner
}

/// Resolve every grid edge of a `border-collapse: collapse` table (CSS 2.2
/// 17.6.2.1). The borders that meet on an edge are those of the cells on
/// either side of it, of the rows and row groups it bounds, of the columns
/// and column groups it bounds, and of the table on its outside.
///
/// One resolution for the layout (half of each edge goes into the cells'
/// and the table's box, [`apply_table_border_model`]) and for the
/// painting (`paint_collapsed_table_borders` in `display_list.rs`).
#[allow(clippy::too_many_lines)] // one participant list per edge kind
pub(crate) fn resolve_collapsed_borders<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    table_index: usize,
    grid: &TableLayoutContext,
) -> CollapsedBorders {
    const TOP: usize = 0;
    const RIGHT: usize = 1;
    const BOTTOM: usize = 2;
    const LEFT: usize = 3;

    let rows = grid.num_rows;
    let cols = grid.columns.len();
    let mut out = CollapsedBorders {
        num_rows: rows,
        num_cols: cols,
        horizontal: vec![None; (rows + 1) * cols],
        vertical: vec![None; rows * (cols + 1)],
        rtl: grid.rtl,
    };
    // A column line's two sides in the table's direction (CSS 2.2 17.5): the
    // cell BEFORE a line meets it with its end side (right in ltr, left in
    // rtl), the cell after it with its start side.
    let (start_side, end_side) = if grid.rtl { (RIGHT, LEFT) } else { (LEFT, RIGHT) };
    if rows == 0 || cols == 0 {
        return out;
    }

    let sides = |index: usize, source: BorderSource| -> [BorderInfo; 4] {
        tree.get(LayoutNodeId::new(index)).map_or_else(
            || {
                let none = BorderInfo::new(
                    0.0,
                    BorderStyle::None,
                    ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    },
                    source,
                );
                [none; 4]
            },
            // (top, right, bottom, left), in that order.
            |node| get_border_info(ctx, node, source).into(),
        )
    };

    let owners = grid.slot_owners();
    let owner = |r: usize, c: usize| owners.get(r * cols + c).copied().flatten();
    let cell_sides: Vec<[BorderInfo; 4]> = grid
        .cells
        .iter()
        .map(|cell| sides(cell.node_index, BorderSource::Cell))
        .collect();
    let row_sides: Vec<[BorderInfo; 4]> = (0..rows)
        .map(|r| {
            grid.row_node_indices
                .get(r)
                .map_or_else(|| sides(usize::MAX, BorderSource::Row), |&i| sides(i, BorderSource::Row))
        })
        .collect();
    let group_of = |r: usize| grid.row_groups.get(r).copied().flatten();
    let group_sides: BTreeMap<usize, [BorderInfo; 4]> = (0..rows)
        .filter_map(&group_of)
        .map(|g| (g, sides(g, BorderSource::RowGroup)))
        .collect();
    let first_in_group = |r: usize| r == 0 || group_of(r - 1) != group_of(r);
    let last_in_group = |r: usize| r + 1 >= rows || group_of(r + 1) != group_of(r);
    let column_sides: Vec<Option<(TableColumnBox, [BorderInfo; 4])>> = (0..cols)
        .map(|c| {
            grid.column_box_at(c)
                .map(|b| (*b, sides(b.node_index, BorderSource::Column)))
        })
        .collect();
    let column_group_sides: Vec<Option<(TableColumnGroupBox, [BorderInfo; 4])>> = (0..cols)
        .map(|c| {
            grid.column_group_at(c)
                .map(|g| (*g, sides(g.node_index, BorderSource::ColumnGroup)))
        })
        .collect();
    let table = sides(table_index, BorderSource::Table);

    let mut participants: Vec<BorderInfo> = Vec::with_capacity(12);

    // Horizontal edges: row line `r` (0 = the table's top), column `c`.
    for r in 0..=rows {
        for c in 0..cols {
            let above = if r > 0 { owner(r - 1, c) } else { None };
            let below = if r < rows { owner(r, c) } else { None };
            if above.is_some() && above == below {
                continue; // inside a cell that spans both rows
            }
            participants.clear();
            if let Some(a) = above {
                participants.push(cell_sides[a][BOTTOM]);
            }
            if let Some(b) = below {
                participants.push(cell_sides[b][TOP]);
            }
            if r > 0 {
                participants.push(row_sides[r - 1][BOTTOM]);
            }
            if r < rows {
                participants.push(row_sides[r][TOP]);
            }
            if r > 0 && last_in_group(r - 1) {
                if let Some(g) = group_of(r - 1).and_then(|g| group_sides.get(&g)) {
                    participants.push(g[BOTTOM]);
                }
            }
            if r < rows && first_in_group(r) {
                if let Some(g) = group_of(r).and_then(|g| group_sides.get(&g)) {
                    participants.push(g[TOP]);
                }
            }
            if r == 0 || r == rows {
                let side = if r == 0 { TOP } else { BOTTOM };
                if let Some((_, s)) = &column_sides[c] {
                    participants.push(s[side]);
                }
                if let Some((_, s)) = &column_group_sides[c] {
                    participants.push(s[side]);
                }
                participants.push(table[side]);
            }
            out.horizontal[r * cols + c] = collapse_edge(&participants);
        }
    }

    // Vertical edges: row `r`, column line `c` (0 = the table's start: its
    // left in ltr, its right in rtl).
    for (r, row_side) in row_sides.iter().enumerate().take(rows) {
        for c in 0..=cols {
            let before = if c > 0 { owner(r, c - 1) } else { None };
            let after = if c < cols { owner(r, c) } else { None };
            if before.is_some() && before == after {
                continue; // inside a cell that spans both columns
            }
            participants.clear();
            if let Some(b) = before {
                participants.push(cell_sides[b][end_side]);
            }
            if let Some(a) = after {
                participants.push(cell_sides[a][start_side]);
            }
            if c == 0 || c == cols {
                let side = if c == 0 { start_side } else { end_side };
                participants.push(row_side[side]);
                if let Some(g) = group_of(r).and_then(|g| group_sides.get(&g)) {
                    participants.push(g[side]);
                }
            }
            if c > 0 {
                if let Some((b, s)) = &column_sides[c - 1] {
                    if b.start + b.span == c {
                        participants.push(s[end_side]);
                    }
                }
            }
            if c < cols {
                if let Some((b, s)) = &column_sides[c] {
                    if b.start == c {
                        participants.push(s[start_side]);
                    }
                }
            }
            if c > 0 {
                if let Some((g, s)) = &column_group_sides[c - 1] {
                    if g.start + g.span == c {
                        participants.push(s[end_side]);
                    }
                }
            }
            if c < cols {
                if let Some((g, s)) = &column_group_sides[c] {
                    if g.start == c {
                        participants.push(s[start_side]);
                    }
                }
            }
            if c == 0 {
                participants.push(table[start_side]);
            } else if c == cols {
                participants.push(table[end_side]);
            }
            out.vertical[r * (cols + 1) + c] = collapse_edge(&participants);
        }
    }

    out
}

/// The borders the boxes of every table are laid out with, decided before
/// anything is measured (the intrinsic pass calls this first: the table's
/// shrink-to-fit width, the column measurement and the cells' final layout
/// all read the box props, so patching them here is what makes every one of
/// them see the same table).
///
/// - Rows, row groups, columns and column groups have no border of their own
///   in layout, in either model (CSS 2.2 17.6.1: the separated model ignores
///   their border properties; 17.6.2: in the collapsing model they take part
///   in the grid's edges, which the cells and the table carry). The cells are
///   placed inside their row's content box, so a `tr { border-bottom }`
///   moved them by the row's border.
/// - The collapsing border model (CSS 2.2 17.6.2): a cell's border is half of
///   each collapsed edge it touches, the table's half of its widest outer
///   edge on each side, and the table has no padding ("in this model, a
///   table does not have padding").
///
/// The unresolved props are patched too, or a parent's re-resolution
/// (`layout_bfc` re-resolves its children's box props) undid it. Idempotent:
/// it starts from the cascade every time.
pub(crate) fn apply_table_border_model<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &mut LayoutTree,
) {
    let tables: Vec<usize> = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.formatting_context, FormattingContext::Table))
        .map(|(i, _)| i)
        .collect();
    for table_index in tables {
        let Some(table) = tree.get(LayoutNodeId::new(table_index)) else {
            continue;
        };
        let collapsed =
            get_border_collapse_property(ctx, table) == StyleBorderCollapse::Collapse;
        let Ok(grid) = analyze_table_structure(tree, table_index, ctx) else {
            continue;
        };
        // Everything that reads the table node is decided before any box
        // is patched.
        let collapsed_boxes = collapsed.then(|| {
            let borders = resolve_collapsed_borders(ctx, tree, table_index, &grid);
            let table_border = if borders.num_rows == 0 || borders.num_cols == 0 {
                // No grid to collapse with: the table's own border stands.
                let (t, r, b, l) = get_border_info(ctx, table, BorderSource::Table);
                let used = |e: BorderInfo| {
                    if matches!(e.style, BorderStyle::None | BorderStyle::Hidden) {
                        0.0
                    } else {
                        e.width
                    }
                };
                EdgeSizes {
                    top: used(t),
                    right: used(r),
                    bottom: used(b),
                    left: used(l),
                }
            } else {
                borders.table_border()
            };
            let cells: Vec<(usize, EdgeSizes)> = grid
                .cells
                .iter()
                .map(|cell| (cell.node_index, borders.cell_border(cell)))
                .collect();
            (table_border, cells)
        });

        let mut grid_boxes: Vec<usize> = grid.row_node_indices.clone();
        grid_boxes.extend(grid.row_groups.iter().flatten().copied());
        grid_boxes.extend(grid.column_boxes.iter().map(|c| c.node_index));
        grid_boxes.extend(grid.column_groups.iter().map(|g| g.node_index));
        grid_boxes.sort_unstable();
        grid_boxes.dedup();
        for index in grid_boxes {
            set_collapsed_box(tree, index, EdgeSizes::default(), false);
        }

        if let Some((table_border, cells)) = collapsed_boxes {
            set_collapsed_box(tree, table_index, table_border, true);
            for (cell, border) in cells {
                set_collapsed_box(tree, cell, border, false);
            }
        }
    }
}

/// Give a box of a table its used border widths (and, for a collapsed
/// table, no padding), in the resolved AND the unresolved box props.
pub(super) fn set_collapsed_box(tree: &mut LayoutTree, index: usize, border: EdgeSizes, no_padding: bool) {
    use azul_css::props::basic::pixel::PixelValue;

    use crate::solver3::geometry::{PackedBoxProps, UnresolvedEdge};

    if let Some(hot) = tree.nodes.get_mut(index) {
        let mut bp = hot.box_props.unpack();
        bp.border = border;
        if no_padding {
            bp.padding = EdgeSizes::default();
        }
        hot.box_props = PackedBoxProps::pack(&bp);
    }
    if let Some(cold) = tree.cold.get_mut(index) {
        cold.unresolved_box_props.border = UnresolvedEdge::new(
            PixelValue::px(border.top),
            PixelValue::px(border.right),
            PixelValue::px(border.bottom),
            PixelValue::px(border.left),
        );
        if no_padding {
            cold.unresolved_box_props.padding = UnresolvedEdge::new(
                PixelValue::px(0.0),
                PixelValue::px(0.0),
                PixelValue::px(0.0),
                PixelValue::px(0.0),
            );
        }
    }
}
