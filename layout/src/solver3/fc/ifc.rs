//! Inline formatting contexts: line layout through text3, text-box-trim and initial letters.

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
        ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
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
            AnonymousBoxType, CachedInlineLayout, InlineMeasureLayouts, LayoutNode,
            LayoutNodeCold, LayoutNodeHot, LayoutNodeId, LayoutNodeWarm, LayoutTree,
            PseudoElement,
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

// Inline Formatting Context (CSS 2.2 § 9.4.2)
// +spec:display-property:ede6f4 - inline layout: mixed stream of text and inline-level boxes

/// Lays out an Inline Formatting Context (IFC) by delegating to the `text3` engine.
///
/// This function acts as a bridge between the box-tree world of `solver3` and the
/// rich text layout world of `text3`. Its responsibilities are:
///
/// 1. **Collect Content**: Traverse the direct children of the IFC root and convert them into a
///    `Vec<InlineContent>`, the input format for `text3`. This involves:
///
///     - Recursively laying out `inline-block` children to determine their final size and baseline,
///       which are then passed to `text3` as opaque objects.
///     - Extracting raw text runs from inline text nodes.
///
/// 2. **Translate Constraints**: Convert the `LayoutConstraints` (available space, floats) from
///    `solver3` into the more detailed `UnifiedConstraints` that `text3` requires.
///
/// 3. **Invoke Text Layout**: Shape the content (`shape_content`) and lay its lines out
///    (`lay_out_shaped_ifc`): BIDI analysis, shaping, line breaking, justification, and vertical
///    alignment.
///    +spec:display-property:e96c82 - inline formatting context: flow of elements/text wrapped into
///    lines
///
/// 4. **Integrate Results**: Process the `UnifiedLayout` returned by `text3`:
///
///     - Store the rich layout result on the IFC root `LayoutNode` for the display list generation
///       pass.
///     - Update the `positions` map for all `inline-block` children based on the positions
///       calculated by `text3`.
///     - Extract the final overflow size and baseline for the IFC root itself
// NOTE(writing-modes): The IFC currently assumes inline direction = horizontal
// and block direction = vertical. In vertical writing modes, line boxes would
// stack horizontally and inline content would flow vertically. The writing mode
// is now available via constraints.writing_mode_ctx for agents to use when
// implementing vertical text layout in the text3 engine.
// +spec:display-property:574e7b - text-box-trim for inline boxes trims block-end to content edge
// (TODO: implement trimming per text-box-edge metric) +spec:display-property:da284a - IFC: flow
// inline-level boxes into line boxes, size/position each fragment +spec:inline-formatting-context:
// 275f64 - IFC: boxes laid out horizontally into line boxes, respecting margins/borders/padding
#[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// CSS Inline 3 §6.2 text-box-trim for the IFC's block container, applied to
/// the finished [`LayoutOutput`]. MUST run on EVERY `layout_ifc` exit that
/// produces content bounds - the incremental cache-reuse arms included -
/// or measure passes that hit the cache size the box untrimmed.
pub(super) fn apply_text_box_trim(
    styled_dom: &StyledDom,
    ifc_root_dom_id: NodeId,
    cached_constraints: &UnifiedConstraints,
    has_items: bool,
    output: &mut LayoutOutput,
) {
    // +spec:box-model:929f42 - text-box-trim: trim half-leading from first/last formatted line
    // +spec:box-model:02e0f9 - text-box-trim: trim-end and trim-both, no effect with non-zero
    // padding/border
    //
    // CSS Inline 3 § 6.2: For block containers, trim the block-start/block-end side
    // of the first/last formatted line. If there is intervening non-zero padding or
    // borders, there is no effect. Does not apply to flex, grid, or table contexts.
    let ifc_node_state = &styled_dom.styled_nodes.as_container()[ifc_root_dom_id].styled_node_state;
    // Fast path: if no node in the DOM declared text-box-trim, the cascade
    // walk would always return None → skip it.
    let text_box_trim = {
        let skip = styled_dom
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .is_some_and(|cc| {
                cc.dom_declared_flags & azul_css::compact_cache::DOM_HAS_TEXT_BOX_TRIM == 0
            });
        if skip {
            StyleTextBoxTrim::None
        } else {
            get_text_box_trim_property(styled_dom, ifc_root_dom_id, ifc_node_state)
                .unwrap_or(StyleTextBoxTrim::None)
        }
    };

    if text_box_trim != StyleTextBoxTrim::None && has_items {
        // Half-leading = (line-height - (ascent + descent)) / 2
        let half_leading = (cached_constraints.resolved_line_height()
            - (cached_constraints.strut_ascent + cached_constraints.strut_descent))
            / 2.0;
        let half_leading = half_leading.max(0.0);

        // +spec:display-property:db5125 - text-box-edge selects the metric the trim cuts to
        // +spec:font-metrics:d3b654 - cap/alphabetic edges use the cap-height and alphabetic
        // baseline The over edge trims PAST the half-leading down to the chosen
        // metric: `cap` cuts ascent - cap-height further, `ex` cuts
        // ascent - x-height; the under edge's `alphabetic` cuts the whole
        // descent (down to the baseline). `text` (and `auto`, and the
        // ideographic metrics we have no strut data for) trim the
        // half-leading only. Trimming reduces the IFC's block size; the
        // first line's glyphs keep their positions (the same model the
        // half-leading-only implementation used - a start-trim shift of
        // the line stack is still TODO).
        let edge = get_text_box_edge_property(styled_dom, ifc_root_dom_id, ifc_node_state)
            .unwrap_or(azul_css::props::style::text::StyleTextBoxEdge::AUTO);
        let over_extra = match edge.over {
            azul_css::props::style::text::TextBoxEdgeOver::Cap => {
                (cached_constraints.strut_ascent - cached_constraints.strut_cap_height).max(0.0)
            }
            azul_css::props::style::text::TextBoxEdgeOver::Ex => {
                (cached_constraints.strut_ascent - cached_constraints.strut_x_height).max(0.0)
            }
            _ => 0.0,
        };
        let under_extra = match edge.under {
            azul_css::props::style::text::TextBoxEdgeUnder::Alphabetic => {
                cached_constraints.strut_descent.max(0.0)
            }
            _ => 0.0,
        };

        // Check for intervening non-zero padding/border on block-start (top)
        let has_pad_or_border_top =
            match get_css_padding_top(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            } || match get_css_border_top_width(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            };

        // Check for intervening non-zero padding/border on block-end (bottom)
        let has_pad_or_border_bottom =
            match get_css_padding_bottom(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            } || match get_css_border_bottom_width(styled_dom, ifc_root_dom_id, ifc_node_state) {
                MultiValue::Exact(pv) => pv.number.get() != 0.0,
                _ => false,
            };

        let trim_start = matches!(
            text_box_trim,
            StyleTextBoxTrim::TrimStart | StyleTextBoxTrim::TrimBoth
        ) && !has_pad_or_border_top;
        let trim_end = matches!(
            text_box_trim,
            StyleTextBoxTrim::TrimEnd | StyleTextBoxTrim::TrimBoth
        ) && !has_pad_or_border_bottom;

        let mut height_reduction = 0.0;
        if trim_start {
            height_reduction += half_leading + over_extra;
        }
        if trim_end {
            height_reduction += half_leading + under_extra;
        }

        if height_reduction > 0.0 {
            output.overflow_size.height = (output.overflow_size.height - height_reduction).max(0.0);
        }
    }
}

/// The height of the strut line box an EMPTY IFC root keeps when it is an
/// editing host (or inside one): one `line-height` of its font, the same
/// rect the caret painter uses (`display_list::empty_editable_caret_rect`).
/// `None` for every other empty IFC — those render nothing.
pub(super) fn editing_host_strut_height<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    node_index: usize,
) -> Option<f32> {
    let dom_id = tree.get(LayoutNodeId::new(node_index))?.dom_node_id?;
    if !crate::solver3::getters::is_node_contenteditable_inherited(ctx.styled_dom, dom_id) {
        return None;
    }
    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let font_size = get_element_font_size(ctx.styled_dom, dom_id, node_state);
    // `normal` as 1.2em. (This read `normalized()` as a factor, so an absolute
    // line-height - stored negative - gave a 1px strut.)
    let line_height = crate::solver3::getters::get_used_line_height(
        ctx.styled_dom,
        dom_id,
        node_state,
        font_size,
        PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    )
    .resolve(font_size, 0.0, 0.0, 0.0, 0);
    Some(
        crate::solver3::display_list::empty_editable_caret_rect(line_height)
            .size
            .height,
    )
}

/// The extent of a laid-out IFC: its `overflow_size` - the ONE computation
/// every exit of [`layout_ifc`] reports, the fresh layout and the cache-reuse
/// path alike, so two layouts of unchanged content cannot drift (a textarea
/// measured 13.0 px tall once and 12.999999 px after a slider drag when the
/// reuse exit took `bounds()` alone).
///
/// The UNCLIPPED content bounds, not `bounds()` alone. `bounds()` maxes over
/// `layout.items`, which under dense-text retention is an empty sentinel (the
/// real clusters live in the dense view) - so `bounds()` collapses to 0 and a
/// horizontal scroll container's overflow was never detected
/// (`overflow_size.width == 0` → `needs_horizontal == false` → the value `<p>`
/// of a single-line field never became a scroll box the caret-reveal could
/// shift: the append-only caret bug). `unclipped_bounds` is captured during
/// line breaking to enclose every positioned item - and the line box a lone
/// `<br>` ends, which no item spans - and survives the sentinel swap. The max
/// of the two, so a path that leaves `unclipped_bounds` at its default still
/// gets the fragment's own bounds.
pub(super) fn ifc_extent(main_frag: &text3::cache::UnifiedLayout) -> LogicalSize {
    extent_of(main_frag.bounds(), main_frag.overflow.unclipped_bounds)
}

/// [`ifc_extent`] from a layout's `bounds()` and the unclipped bounds of its
/// overflow info: the one place the two are combined.
const fn extent_of(frag_bounds: text3::cache::Rect, unclipped: text3::cache::Rect) -> LogicalSize {
    LogicalSize::new(
        frag_bounds.width.max(unclipped.width),
        frag_bounds.height.max(unclipped.height),
    )
}

/// How often `layout_ifc` left through its reuse exit on this thread, and how
/// often that exit expanded a retired layout's dense view into items again
/// (`audit_tests::a3_solver::ifc`: a reused label is not expanded).
#[cfg(test)]
// Test instrumentation: a count kept per test thread, built only under cfg(test).
thread_local! {
    pub(crate) static IFC_REUSE_EXITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(crate) static IFC_REUSE_EXPANSIONS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// How many min- or max-content measures `layout_ifc` stored on this thread,
/// and how many of those stores built the layout's paint runs
/// (`audit_tests::a3_solver::ifc`: none).
#[cfg(test)]
// Test instrumentation: a count kept per test thread, built only under cfg(test).
thread_local! {
    pub(crate) static IFC_MEASURE_STORES: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    pub(crate) static IFC_MEASURE_STORE_RUN_BUILDS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Lays an inline formatting context out from the paragraph its shaped
/// items make in its constraints (`ParagraphToBreak`): its lines broken and
/// positioned (`perform_fragment_layout`), and what its box reports from
/// them (`finish_inline_layout`).
///
/// The solver's layout of an IFC (`layout_ifc`) and a relayout after an edit
/// both lay it out here; they differ only in where its content, and so its
/// shaped items, come from.
///
/// # Errors
///
/// Returns a `LayoutError` if the lines cannot be laid out.
pub(crate) fn lay_out_shaped_ifc<T: ParsedFontTrait>(
    logical_items: &[text3::cache::LogicalItem],
    paragraph: &text3::cache::ParagraphToBreak<'_>,
    fonts: &text3::cache::LoadedFonts<T>,
    styled_dom: &StyledDom,
    ifc_root_dom_id: NodeId,
    child_map: &HashMap<ContentIndex, usize>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> core::result::Result<
    (Arc<text3::cache::UnifiedLayout>, LayoutOutput),
    text3::cache::LayoutError,
> {
    let constraints = paragraph.constraints();
    let mut cursor = paragraph.cursor();
    let layout = Arc::new(text3::cache::perform_fragment_layout(
        &mut cursor,
        logical_items,
        constraints,
        debug_messages,
        fonts,
    )?);
    let output = finish_inline_layout(&layout, constraints, styled_dom, ifc_root_dom_id, child_map);
    Ok((layout, output))
}

/// What an inline formatting context's box reports once its lines stand.
///
/// That is the extent of the lines (`ifc_extent`), less what `text-box-trim`
/// trims (`apply_text_box_trim`), the last line's baseline, and where each
/// atomic inline child (an inline-block, an image) stands, relative to the
/// IFC's content box: where its line put the item `child_map` names it by.
/// Every layout of an IFC reports these from here, a fresh one, the cache's
/// reuse of an earlier one and a relayout after an edit alike.
pub(crate) fn finish_inline_layout(
    layout: &text3::cache::UnifiedLayout,
    constraints: &UnifiedConstraints,
    styled_dom: &StyledDom,
    ifc_root_dom_id: NodeId,
    child_map: &HashMap<ContentIndex, usize>,
) -> LayoutOutput {
    // +spec:display-property:a0d0ab - IFC height = top of topmost line box to bottom of
    // bottommost line box +spec:display-property:a63b8f - baseline-source defaults to
    // auto (last baseline for inline-block/IFC)
    let mut output = LayoutOutput {
        overflow_size: ifc_extent(layout),
        baseline: layout.last_baseline(),
        ..LayoutOutput::default()
    };
    apply_text_box_trim(
        styled_dom,
        ifc_root_dom_id,
        constraints,
        !layout.items.is_empty(),
        &mut output,
    );
    for positioned_item in &layout.items {
        if let ShapedItem::Object { source, .. } = &positioned_item.item {
            if let Some(&child_node_index) = child_map.get(source) {
                output.positions.insert(
                    child_node_index,
                    LogicalPosition {
                        x: positioned_item.position.x,
                        y: positioned_item.position.y,
                    },
                );
            }
        }
    }
    output
}

/// [`finish_inline_layout`] for a RETIRED layout - one stored as its dense
/// view alone - from the measures kept when it was stored
/// (`CachedInlineLayout::retired_metrics`: the `bounds()` and the
/// `last_baseline()` of the layout the dense view expands to) and its
/// overflow info, without expanding it.
///
/// A retired layout holds clusters and nothing else, and at least one
/// (`CachedInlineLayout::retire_sparse`): it has items for `text-box-trim`,
/// and no atomic inline whose position it would report.
fn finish_retired_inline_layout(
    kept: &crate::solver3::layout_tree::RetiredLayoutMetrics,
    overflow: &text3::cache::OverflowInfo,
    constraints: &UnifiedConstraints,
    styled_dom: &StyledDom,
    ifc_root_dom_id: NodeId,
) -> LayoutOutput {
    let mut output = LayoutOutput {
        overflow_size: extent_of(kept.bounds, overflow.unclipped_bounds),
        baseline: kept.last_baseline,
        ..LayoutOutput::default()
    };
    apply_text_box_trim(styled_dom, ifc_root_dom_id, constraints, true, &mut output);
    output
}

/// Where a box taken out of an inline formatting context would have stood
/// in it, in the context's content-box coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InlineStaticPlace {
    /// The inline position right after the content before the box - or, for
    /// a box nothing comes before, at the start of the content after it.
    pub x: f32,
    /// The top of the line that content stands on.
    pub line_top: f32,
    /// The bottom of that line.
    pub line_bottom: f32,
    /// Whether any content of the context comes before the box.
    pub after_content: bool,
    /// The index of that line in the context.
    pub line: usize,
}

/// The place of the out-of-flow box of DOM node `child` among `items`, the
/// laid-out content of the inline formatting context it was taken out of:
/// the end of the content that comes before it in document order, on that
/// content's line - else the start of the first content after it. `None` for
/// a context without content.
///
/// CSS 2.1 10.3.7 and 10.6.4 define the static position by "a hypothetical
/// box that would have been the first box of the element" had it been
/// `position: static` and `float: none`, and allow that "user agents are
/// free to make a guess at its probable position". The guess made here: an
/// empty box that follows the last character before it stands on that
/// character's line (it is not moved to the next line when that character
/// ends its line), and the line's extent is the extent of the items on it -
/// the line boxes themselves are not kept by the text layout.
///
/// `node_of` names the DOM node an item comes from; DOM node ids are in
/// document (pre-order) order (`StyledDom`), so "before" is "a lower id".
///
/// The definition, by one walk over the items per box. The layout asks
/// through [`StaticPlaces`] / [`TreeStaticPlaces`], which list the items
/// once for many boxes and are tested against this.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn inline_static_place(
    items: &[text3::cache::PositionedItem],
    node_of: impl Fn(&ShapedItem) -> Option<NodeId>,
    child: NodeId,
) -> Option<InlineStaticPlace> {
    let mut before: Option<(usize, usize)> = None; // (node index, item index)
    let mut after: Option<(usize, usize)> = None;
    for (i, item) in items.iter().enumerate() {
        #[cfg(test)]
        STATIC_PLACE_ITEM_VISITS.with(|n| n.set(n.get() + 1));
        let Some(node) = node_of(&item.item) else {
            continue;
        };
        if node.index() < child.index() {
            if before.is_none_or(|(n, _)| node.index() >= n) {
                before = Some((node.index(), i));
            }
        } else if after.is_none_or(|(n, _)| node.index() < n) {
            after = Some((node.index(), i));
        }
    }
    let (anchor, after_content) = match (before, after) {
        (Some((_, i)), _) => (&items[i], true),
        (None, Some((_, i))) => (&items[i], false),
        (None, None) => return None,
    };
    let mut line: Option<(f32, f32)> = None;
    for item in items.iter().filter(|i| i.line_index == anchor.line_index) {
        #[cfg(test)]
        STATIC_PLACE_ITEM_VISITS.with(|n| n.set(n.get() + 1));
        line = widen_line_extent(line, item);
    }
    Some(static_place_at(anchor, after_content, line))
}

/// The extent `(top, bottom)` of a line with one more of its items taken
/// in: the items that have a height span it.
fn widen_line_extent(
    extent: Option<(f32, f32)>,
    item: &text3::cache::PositionedItem,
) -> Option<(f32, f32)> {
    let height = item.item.bounds().height;
    if height.is_nan() || height <= 0.0 {
        return extent;
    }
    let (top, bottom) = (item.position.y, item.position.y + height);
    Some(extent.map_or((top, bottom), |(t, b)| (t.min(top), b.max(bottom))))
}

/// The place at `anchor`, the content the box comes after (`after_content`)
/// or before, on a line of the extent `line` (`None`: no item of the line
/// has a height).
fn static_place_at(
    anchor: &text3::cache::PositionedItem,
    after_content: bool,
    line: Option<(f32, f32)>,
) -> InlineStaticPlace {
    let rtl = matches!(
        &anchor.item,
        ShapedItem::Cluster(c) if c.direction == text3::cache::BidiDirection::Rtl
    );
    let width = anchor.item.bounds().width;
    // The END edge of the content before the box, the START edge of the
    // content after it.
    let x = if after_content == rtl {
        anchor.position.x
    } else {
        anchor.position.x + width
    };
    let (line_top, line_bottom) = line.unwrap_or((anchor.position.y, anchor.position.y));
    InlineStaticPlace {
        x,
        line_top,
        line_bottom,
        after_content,
        line: anchor.line_index,
    }
}

/// How many laid-out items the search for static places looked at on this
/// thread (`audit_tests::a3_solver::ifc`: once per item for all the boxes
/// taken out of a context, not once per box).
#[cfg(test)]
// Test instrumentation: a count kept per test thread, built only under cfg(test).
thread_local! {
    pub(crate) static STATIC_PLACE_ITEM_VISITS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// [`inline_static_place`] for MANY boxes taken out of one inline formatting
/// context: what the search reads of the laid-out `items`, listed once.
///
/// `inline_static_place` walks all items for every box (which content comes
/// before it, which after) and then all items again for the anchor's line:
/// boxes x items for a paragraph with many absolutely positioned children.
/// The first walk only asks, of the items of each DOM node, for the LAST one
/// (the anchor when the node is the nearest before the box) and the FIRST
/// one (when it is the nearest at or after it); the second folds the items
/// of one line in their order.
struct StaticPlaces<'a> {
    items: &'a [text3::cache::PositionedItem],
    tables: StaticPlaceTables,
}

impl<'a> StaticPlaces<'a> {
    fn new(
        items: &'a [text3::cache::PositionedItem],
        node_of: impl Fn(&ShapedItem) -> Option<NodeId>,
    ) -> Self {
        Self {
            items,
            tables: StaticPlaceTables::new(items, node_of),
        }
    }

    /// [`inline_static_place`] of `child` among the items.
    fn place(&mut self, child: NodeId) -> Option<InlineStaticPlace> {
        self.tables.place(self.items, child)
    }
}

/// What [`StaticPlaces`] lists of a context's items, without the items: a
/// holder that owns the items keeps this beside them ([`TreeStaticPlaces`])
/// and hands the same items to every call.
struct StaticPlaceTables {
    /// Per DOM node that has items, by node index: its first and its last
    /// item.
    by_node: BTreeMap<usize, (usize, usize)>,
    /// `(line, item)` of every item, sorted: the items of a line together,
    /// in their order. Made when the first line is asked for.
    by_line: Option<Vec<(usize, usize)>>,
    /// The extent of each line asked for so far ([`widen_line_extent`]).
    line_extents: BTreeMap<usize, Option<(f32, f32)>>,
}

impl StaticPlaceTables {
    fn new(
        items: &[text3::cache::PositionedItem],
        node_of: impl Fn(&ShapedItem) -> Option<NodeId>,
    ) -> Self {
        let mut by_node: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        for (i, item) in items.iter().enumerate() {
            #[cfg(test)]
            STATIC_PLACE_ITEM_VISITS.with(|n| n.set(n.get() + 1));
            let Some(node) = node_of(&item.item) else {
                continue;
            };
            // The items come in their order: the first one seen of a node
            // is its first, every later one its last so far.
            by_node
                .entry(node.index())
                .and_modify(|first_and_last| first_and_last.1 = i)
                .or_insert((i, i));
        }
        Self {
            by_node,
            by_line: None,
            line_extents: BTreeMap::new(),
        }
    }

    /// [`inline_static_place`] of `child` among `items`, the items the
    /// tables were made of.
    fn place(
        &mut self,
        items: &[text3::cache::PositionedItem],
        child: NodeId,
    ) -> Option<InlineStaticPlace> {
        // The scan keeps, of the nodes below `child`, the greatest and of its
        // items the last (`>=`); of the nodes at or above it the smallest
        // and of its items the first (`<`).
        let before = self
            .by_node
            .range(..child.index())
            .next_back()
            .map(|(_, &(_, last))| last);
        let after = self
            .by_node
            .range(child.index()..)
            .next()
            .map(|(_, &(first, _))| first);
        let (anchor_index, after_content) = match (before, after) {
            (Some(i), _) => (i, true),
            (None, Some(i)) => (i, false),
            (None, None) => return None,
        };
        let anchor = items.get(anchor_index)?;
        let line = self.line_extent(items, anchor.line_index);
        Some(static_place_at(anchor, after_content, line))
    }

    /// The extent of line `line_index`: [`widen_line_extent`] over its items
    /// in their order, as the scan folds them.
    fn line_extent(
        &mut self,
        items: &[text3::cache::PositionedItem],
        line_index: usize,
    ) -> Option<(f32, f32)> {
        if let Some(known) = self.line_extents.get(&line_index) {
            return *known;
        }
        let by_line = self.by_line.get_or_insert_with(|| {
            let mut by_line: Vec<(usize, usize)> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    #[cfg(test)]
                    STATIC_PLACE_ITEM_VISITS.with(|n| n.set(n.get() + 1));
                    (item.line_index, i)
                })
                .collect();
            by_line.sort_unstable();
            by_line
        });
        let from = by_line.partition_point(|&(line, _)| line < line_index);
        let mut extent: Option<(f32, f32)> = None;
        for &(line, i) in &by_line[from..] {
            if line != line_index {
                break;
            }
            #[cfg(test)]
            STATIC_PLACE_ITEM_VISITS.with(|n| n.set(n.get() + 1));
            extent = widen_line_extent(extent, &items[i]);
        }
        self.line_extents.insert(line_index, extent);
        extent
    }
}

/// [`inline_static_place`] for the out-of-flow layout child `child_index`
/// of the inline formatting context root `ifc_root`, from the layout the
/// root holds. `None` when the root holds no inline layout or no content.
///
/// One box at a time: the root's stored text is expanded and its items are
/// walked twice for every box asked about. The callers ask through
/// [`TreeStaticPlaces`]; this is what its answers are compared with.
#[cfg(test)]
pub(crate) fn inline_static_place_in_tree(
    tree: &LayoutTree,
    ifc_root: usize,
    child_index: usize,
) -> Option<InlineStaticPlace> {
    let child = tree.get(LayoutNodeId::new(child_index))?.dom_node_id?;
    let layout = tree.materialized_inline_layout_for_node(ifc_root)?;
    let child_map = tree
        .warm(LayoutNodeId::new(ifc_root))
        .and_then(|w| w.inline_content_cache.as_deref())
        .map(|c| Arc::clone(&c.child_map));
    inline_static_place(
        &layout.items,
        |item| item_dom_node(tree, child_map.as_deref(), item),
        child,
    )
}

/// The places of MANY boxes taken out of the inline formatting contexts of
/// one tree (the footnotes of a page, of a column): where
/// [`inline_static_place`] puts the out-of-flow layout child `child_index`
/// of the context root `ifc_root`, from the layout the root holds.
///
/// A root's layout is expanded (`LayoutTree::materialized_inline_layout_for_node`:
/// a copy of every item of a text kept dense) and its items are listed
/// ([`StaticPlaceTables`]) when the first box taken out of it asks, and both
/// are kept for the boxes after it. Asked box by box, a paragraph with k
/// footnotes was expanded k times and its items walked 2 k times: boxes x
/// items per page.
///
/// For a tree as it stands while this lives: nothing here sees a layout of
/// one of its contexts.
#[derive(Default)]
pub(crate) struct TreeStaticPlaces {
    /// Per context root asked about: its expanded layout and what is listed
    /// of it; `None` for a root without an inline layout.
    by_root: BTreeMap<usize, Option<(Arc<text3::cache::UnifiedLayout>, StaticPlaceTables)>>,
}

impl TreeStaticPlaces {
    /// The place of `child_index` in the context of `ifc_root`. `None` when
    /// the root holds no inline layout or no content.
    pub(crate) fn place(
        &mut self,
        tree: &LayoutTree,
        ifc_root: usize,
        child_index: usize,
    ) -> Option<InlineStaticPlace> {
        let child = tree.get(LayoutNodeId::new(child_index))?.dom_node_id?;
        let listed = self.by_root.entry(ifc_root).or_insert_with(|| {
            let layout = tree.materialized_inline_layout_for_node(ifc_root)?;
            let child_map = tree
                .warm(LayoutNodeId::new(ifc_root))
                .and_then(|w| w.inline_content_cache.as_deref())
                .map(|c| Arc::clone(&c.child_map));
            let tables = StaticPlaceTables::new(&layout.items, |item| {
                item_dom_node(tree, child_map.as_deref(), item)
            });
            Some((layout, tables))
        });
        let (layout, tables) = listed.as_mut()?;
        tables.place(&layout.items, child)
    }
}

/// The DOM node a laid-out inline item comes from: a cluster's text node,
/// an atomic inline's element (through `child_map`, the layout child each
/// object stands for).
fn item_dom_node(
    tree: &LayoutTree,
    child_map: Option<&HashMap<ContentIndex, usize>>,
    item: &ShapedItem,
) -> Option<NodeId> {
    match item {
        ShapedItem::Cluster(c) => c.source_node_id,
        ShapedItem::Object { source, .. } => child_map
            .and_then(|m| m.get(source))
            .and_then(|&i| tree.get(LayoutNodeId::new(i)))
            .and_then(|n| n.dom_node_id),
        _ => None,
    }
}

/// The STATIC positions of the out-of-flow (absolute / fixed) boxes taken
/// out of the inline formatting context of `node_index` - its children and
/// those of the inline boxes in it, at any depth
/// ([`out_of_flow_inline_descendants`]) - relative to ITS content box, from
/// its laid-out `items` (`LayoutOutput::static_positions`).
///
/// CSS 2.1 10.3.7 (`left`) and 10.6.4 (`top`): the static position is where
/// "a hypothetical box that would have been the first box of the element"
/// would stand if the element were `position: static`. In an inline
/// formatting context that box is
///
/// - for an inline-level element (its specified `display`, before the
///   blockification of CSS Display 3 2.7): an inline box on the line, after the content before
///   it - the line's top, the end of that content ([`inline_static_place`]);
/// - for a block-level element: a block that ends the line before it - the containing block's
///   start edge, below the line of the content before it (at the top of the first line when
///   nothing comes before it).
///
/// The context used to record none, and every such box took its parent's
/// content-box origin: a `position: absolute` span in the third line of a
/// paragraph stood on the first.
pub(super) fn out_of_flow_static_positions(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    node_index: usize,
    items: &[text3::cache::PositionedItem],
    child_map: &HashMap<ContentIndex, usize>,
) -> BTreeMap<usize, LogicalPosition> {
    static_positions_of(
        styled_dom,
        tree,
        &out_of_flow_inline_descendants(styled_dom, tree, node_index),
        items,
        child_map,
    )
}

/// [`out_of_flow_static_positions`] for the boxes `out_of_flow`, the
/// [`out_of_flow_inline_descendants`] of the context: none for none of them,
/// whatever `items` holds.
fn static_positions_of(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    out_of_flow: &[usize],
    items: &[text3::cache::PositionedItem],
    child_map: &HashMap<ContentIndex, usize>,
) -> BTreeMap<usize, LogicalPosition> {
    let mut out = BTreeMap::new();
    if out_of_flow.is_empty() {
        return out;
    }
    // The items are listed once for all the boxes (`StaticPlaces`), not
    // walked for each of them.
    let mut places = StaticPlaces::new(items, |item| item_dom_node(tree, Some(child_map), item));
    for &child_index in out_of_flow {
        let Some(child) = tree
            .get(LayoutNodeId::new(child_index))
            .and_then(|n| n.dom_node_id)
        else {
            continue;
        };
        let Some(place) = places.place(child) else {
            continue;
        };
        let display = get_display_property(styled_dom, Some(child)).unwrap_or_default();
        let inline_level = crate::solver3::getters::blockify_display(display) != display;
        let pos = if inline_level {
            LogicalPosition::new(place.x, place.line_top)
        } else if place.after_content {
            LogicalPosition::new(0.0, place.line_bottom)
        } else {
            LogicalPosition::new(0.0, place.line_top)
        };
        out.insert(child_index, pos);
    }
    out
}

/// Whether the layout box `index` is an in-flow INLINE BOX (`<em>`, `<a>`,
/// a `<span>`): a box the lines of its inline formatting context run
/// through, its content a part of that context.
fn is_in_flow_inline_box(styled_dom: &StyledDom, tree: &LayoutTree, index: usize) -> bool {
    let Some(dom) = tree.get(LayoutNodeId::new(index)).and_then(|n| n.dom_node_id) else {
        return false;
    };
    crate::solver3::getters::is_inline_box_display(
        get_display_property(styled_dom, Some(dom)).unwrap_or_default(),
    ) && !matches!(
        get_position_type(styled_dom, Some(dom)),
        LayoutPosition::Absolute | LayoutPosition::Fixed
    ) && get_float_property(styled_dom, Some(dom)) == LayoutFloat::None
        && !is_marker_box(tree, index)
}

/// The out-of-flow (absolute / fixed) boxes taken out of the inline
/// formatting context of `ifc_root`: its out-of-flow children and those of
/// the inline boxes in it, at any depth - `<p>a <em>b <span abs></span></em></p>`.
/// The content of an inline box is content of the context it is in (CSS 2.1
/// 9.2.2), so the hypothetical box of an out-of-flow box written in it
/// (10.3.7, 10.6.4) would have stood on a line of that context.
pub(crate) fn out_of_flow_inline_descendants(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    ifc_root: usize,
) -> Vec<usize> {
    let mut found = Vec::new();
    let mut stack: Vec<usize> = tree.children(ifc_root).iter().rev().copied().collect();
    while let Some(index) = stack.pop() {
        let Some(dom) = tree.get(LayoutNodeId::new(index)).and_then(|n| n.dom_node_id) else {
            continue;
        };
        if is_marker_box(tree, index) {
            continue;
        }
        if matches!(
            get_position_type(styled_dom, Some(dom)),
            LayoutPosition::Absolute | LayoutPosition::Fixed
        ) {
            found.push(index);
        } else if is_in_flow_inline_box(styled_dom, tree, index) {
            stack.extend(tree.children(index).iter().rev().copied());
        }
    }
    found
}

/// The root of the inline formatting context the out-of-flow box `index`
/// was taken out of: its parent, or the box its parent's chain of inline
/// boxes stands in. `None` when that box holds no inline layout (the box
/// was taken out of a block flow).
pub(crate) fn inline_context_of(
    styled_dom: &StyledDom,
    tree: &LayoutTree,
    index: usize,
) -> Option<usize> {
    let mut parent = tree.get(LayoutNodeId::new(index))?.parent?;
    // At most one step per box: a malformed parent chain cannot loop.
    for _ in 0..tree.nodes.len() {
        if !is_in_flow_inline_box(styled_dom, tree, parent) {
            break;
        }
        parent = tree.get(LayoutNodeId::new(parent))?.parent?;
    }
    tree.warm(LayoutNodeId::new(parent))
        .is_some_and(|w| w.inline_layout_result.is_some())
        .then_some(parent)
}

/// Where the content box of the PARENT of `index` stands, seen from the
/// content box of `ifc_root`, as the positioning pass derives it
/// (`cache::position_bfc_child_descendants`): through the inline boxes
/// between them, each at its stored relative position inside its own
/// parent's content box, its content box inside its borders and padding.
/// Zero for a child of `ifc_root` itself.
///
/// A static position is recorded relative to the root of the inline
/// formatting context ([`out_of_flow_static_positions`]); a box is
/// positioned relative to its parent's content box. This is the difference.
pub(crate) fn inline_parent_offset(tree: &LayoutTree, ifc_root: usize, index: usize) -> LogicalPosition {
    let mut offset = LogicalPosition::zero();
    let mut current = tree.get(LayoutNodeId::new(index)).and_then(|n| n.parent);
    for _ in 0..tree.nodes.len() {
        let Some(box_index) = current.filter(|c| *c != ifc_root) else {
            break;
        };
        let Some(node) = tree.get(LayoutNodeId::new(box_index)) else {
            break;
        };
        let bp = node.box_props.unpack();
        let rel = tree
            .warm(LayoutNodeId::new(box_index))
            .and_then(|w| w.relative_position)
            .unwrap_or_default();
        offset.x += rel.x + bp.border.left + bp.padding.left;
        offset.y += rel.y + bp.border.top + bp.padding.top;
        current = node.parent;
    }
    offset
}


pub(super) fn layout_ifc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<LayoutOutput> {
    unsafe {
        crate::az_mark(0x60704_u32, (0x20u32));
    }
    // [g147 az-web-lift DIAG] CALLER-side tree validity at layout_ifc entry, indexed by node_index
    // (0x60900+ = nodes.len, 0x60920+ = tree ptr) to dodge marker-overwrite across multiple IFCs.
    // Compare vs _impl's CALLEE-side (0x60940+/0x60960+): ptr differs ⇒ &mut tree mis-passes across
    // the call; ptr same but len differs ⇒ the tree's `nodes` Vec is emptied in place.
    #[cfg(feature = "web_lift")]
    unsafe {
        let slot = (node_index & 7) * 4;
        crate::az_mark((0x60900 + slot) as u32, (tree.nodes.len() as u32) as u32);
        crate::az_mark(
            (0x60920 + slot) as u32,
            ((&*tree as *const LayoutTree as usize) as u32) as u32,
        );
    }
    let float_count = constraints
        .bfc_state
        .as_ref()
        .map_or(0, |s| s.floats.floats.len());
    debug_info!(
        ctx,
        "[layout_ifc] ENTRY: node_index={}, has_bfc_state={}, float_count={}",
        node_index,
        constraints.bfc_state.is_some(),
        float_count
    );
    debug_ifc_layout!(ctx, "CALLED for node_index={}", node_index);

    // +spec:display-property:7f3c1d - Anonymous inline boxes: text directly in block containers
    // treated as anonymous inline elements in IFC +spec:display-property:5a795c - root inline
    // box: block container generates anonymous inline box holding all inline-level contents,
    // inheriting from parent For anonymous boxes, we need to find the DOM ID from a parent or
    // child CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?;
    // An anonymous box borrows an element's id to resolve its style, but
    // only the INHERITED properties are its own (§9.2.1.1): it has no
    // columns of the element's (see translate_to_text3_constraints).
    let ifc_root_is_anonymous = node.dom_node_id.is_none();
    let ifc_root_dom_id =
        ifc_root_style_dom_id(tree, node_index).ok_or(LayoutError::InvalidTree)?;

    debug_ifc_layout!(ctx, "ifc_root_dom_id={:?}", ifc_root_dom_id);

    // +spec:display-property:a469a6 - line boxes created as needed for inline-level content in IFC
    // +spec:display-property:f3c875 - calculate layout bounds (size contributions) of each
    // inline-level box Phase 1: Collect and measure all inline-level children.
    // Fold the IFC subtree's per-node fingerprints into one key. The
    // reconcile pass already computes and stores these (they are what marks
    // a node clean/dirty), so this is a handful of hashes over data already
    // in cache — versus re-resolving the FULL cascade
    // (`get_style_properties`) for every text run and inline span, which is
    // what collection does and what made it 2 ms per IFC / 32 ms per
    // pagination even when the line layout was going to be reused.
    let subtree_fingerprint = {
        use core::hash::{Hash, Hasher};
        let mut h = azul_core::hash::FastHasher::new();

        // vw/vh/vmin/vmax resolve against the viewport, so a resize MUST
        // invalidate collected styles — but ONLY for documents that actually
        // use a viewport unit. Folding the viewport in unconditionally
        // invalidated EVERY collection on EVERY resize (552 re-collections
        // ≈ 19.6 ms per resize on big.md) to protect a feature the document
        // did not use. `uses_viewport_units` is detected at compact-build
        // time from the same values the cache encodes; a missing compact
        // cache degrades to the old always-invalidate behaviour.
        let doc_uses_viewport_units = ctx
            .styled_dom
            .css_property_cache
            .ptr
            .compact_cache
            .as_ref()
            .is_none_or(|cc| cc.uses_viewport_units);
        if doc_uses_viewport_units {
            ctx.viewport_size.width.to_bits().hash(&mut h);
            ctx.viewport_size.height.to_bits().hash(&mut h);
        }

        let compact = ctx.styled_dom.css_property_cache.ptr.compact_cache.as_ref();
        let mut stack = alloc::vec![node_index];
        while let Some(idx) = stack.pop() {
            if let Some(cold) = tree.cold(LayoutNodeId::new(idx)) {
                cold.node_data_fingerprint.hash(&mut h);
                // NodeDataFingerprint covers the node's own data and INLINE
                // css — not what the AUTHOR STYLESHEET resolved onto it. Two
                // DOMs with identical nodes but different stylesheets would
                // otherwise share a key and serve each other's styles. The
                // compact cache holds the RESOLVED values, so folding this
                // node's entries in makes any cascade change invalidate.
                let dom_id_opt = tree.get(LayoutNodeId::new(idx)).and_then(|n| n.dom_node_id);
                if let (Some(cc), Some(dom_id)) = (compact, dom_id_opt) {
                    hash_resolved_style(cc, dom_id.index(), &mut h);
                }
            }
            idx.hash(&mut h);
            for &c in tree.children(idx) {
                stack.push(c);
            }
        }
        h.finish()
    };

    // Visit-type census (AZ_PROFILE=cpu): which constraint TYPE reaches this
    // IFC. Min/Max-content visits are measures (a container asking how tall
    // the content is at that width): they read the slot their earlier
    // measures filled (`InlineMeasureLayouts`), so one that falls through to
    // layout_flow measures this content for the first time.
    drop(crate::probe::Probe::span(
        match constraints.available_width_type {
            Text3AvailableSpace::Definite(_) => "ifc_visit_definite",
            Text3AvailableSpace::MinContent => "ifc_visit_min",
            Text3AvailableSpace::MaxContent => "ifc_visit_max",
        },
    ));

    let cached_collection = tree
        .warm(LayoutNodeId::new(node_index))
        .and_then(|w| w.inline_content_cache.as_ref())
        .filter(|c| c.subtree_fingerprint == subtree_fingerprint)
        .filter(|c| {
            c.atomics_measured_against
                .is_none_or(|key| key == atomic_inline_measure_key(constraints))
        })
        .map(|c| (c.content.clone(), c.child_map.clone(), c.content_hash_base));

    // Second precondition, checked HERE because this is the only place that can.
    //
    // The fingerprint proves the collection is unchanged. It says nothing about
    // the cache's other half: `collect_and_measure_inline_content` does not only
    // collect, it LAYS OUT every atomic inline-level child, and that layout is
    // written to the TREE, not to the cache. Reuse the collection in a tree
    // where those children have not been laid out and the only call that would
    // size them is skipped — they keep their 0x0 default while the cached
    // `InlineShape` still carries the size measured in some earlier tree. The
    // text then flows correctly around a box that paints and hit-tests as
    // nothing.
    let cached_collection = match cached_collection {
        Some((content, child_map, base))
            if atomic_inline_children_are_laid_out(tree, &content, &child_map) =>
        {
            Some((content, child_map, base))
        }
        Some(_) => {
            debug_info!(
                ctx,
                "[layout_ifc] node {}: cached collection REJECTED — an atomic inline child is not \
                 laid out in this tree; re-collecting",
                node_index
            );
            None
        }
        None => None,
    };

    // `content_hash_base` rides with the collection: hashed ONCE per rebuild,
    // reused by every subsequent visit (see CachedInlineContent::content_hash_base
    // for the 29 ms this replaces).
    let (collect_result, content_hash_base) =
        if let Some((content, child_map, base)) = cached_collection {
            drop(crate::probe::Probe::span("ifc_collect_cached"));
            (Ok((content, child_map)), Some(base))
        } else {
            let _p = crate::probe::Probe::span("ifc_collect_content");
            let res =
                collect_and_measure_inline_content(ctx, text_cache, tree, node_index, constraints)
                    .map(|(content, child_map)| (Arc::new(content), Arc::new(child_map)));
            let mut base = None;
            if let Ok((content, child_map)) = res.as_ref() {
                let computed_base = {
                    let _p = crate::probe::Probe::span("ifc_content_hash_base");
                    use std::hash::{Hash, Hasher};
                    let mut h = azul_core::hash::FastHasher::new();
                    content.hash(&mut h);
                    h.finish()
                };
                base = Some(computed_base);
                if let Some(w) = tree.warm_mut(LayoutNodeId::new(node_index)) {
                    w.inline_content_cache =
                        Some(Box::new(crate::solver3::layout_tree::CachedInlineContent {
                            content: Arc::clone(content),
                            child_map: Arc::clone(child_map),
                            subtree_fingerprint,
                            content_hash_base: computed_base,
                            atomics_measured_against: content
                                .iter()
                                .any(|c| matches!(c, InlineContent::Shape(_)))
                                .then(|| atomic_inline_measure_key(constraints)),
                        }));
                }
            }
            (res, base)
        };
    // [g133 az-web-lift DIAG] which early-return fires in POSITIONING's layout_ifc.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark(
            (0x60680) as u32,
            (collect_result.as_ref().map(|(c, _)| c.len()).unwrap_or(0) as u32) as u32,
        );
        crate::az_mark(
            (0x60684) as u32,
            (if collect_result.is_ok() {
                0xC0DE0680u32
            } else {
                0x000000EEu32
            }) as u32,
        );
    }
    let (inline_content, child_map) = collect_result?;

    // #11 fix: hash the inline content once. Used to (a) skip stale Phase 2d
    // fast-path reuse and (b) force a cache REPLACE when content changed even
    // though available width is unchanged — the display-list generator paints
    // text from the cached `inline_layout_result` (display_list.rs), so a
    // content change at a same-width constraint MUST overwrite it or the old
    // glyphs keep rendering (#11 stale display list).
    // Phase 2 (translate early): resolve the container-level (IFC) constraints now,
    // so the Phase 2d cache-reuse decision below can key on them too. Reuse was keyed
    // on available width + per-run content hash only; a change to a container-level
    // property (text-align, text-align-last, text-indent, direction, line-height,
    // white-space, columns) — which is NOT covered by the per-run content hash — would
    // otherwise silently reuse a stale, differently-aligned/indented cached layout.
    let line_style = LineStyle::of(
        ctx.styled_dom,
        ifc_root_dom_id,
        &mut ctx.style_cache,
        ctx.system_style.as_ref(),
        PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    );
    let mut text3_constraints =
        translate_to_text3_constraints(ctx, constraints, &line_style, ifc_root_is_anonymous);
    // The root keeps the styles its lines are broken with: a restyle above
    // it that moves them makes the reconcile lay it out anew
    // (`cache::reconcile_node`), where its clean text alone would keep it.
    if let Some(cold) = tree
        .cold_mut(LayoutNodeId::new(node_index))
        .filter(|cold| cold.line_style.as_deref() != Some(&line_style))
    {
        cold.line_style = Some(Arc::new(line_style));
    }
    // CSS 2.1 s16.1 / CSS Text 3 s8.1: `text-indent` indents the FIRST
    // formatted line of the block container. An anonymous block (which
    // borrows the container's style) holds that line only when it is the
    // container's first in-flow box: the text after a nested block
    // (`<div>first<div>..</div>after</div>`) is not indented (Chrome);
    // it was (TEXT7's finding). `each-line` keeps its own rule.
    if ifc_root_is_anonymous
        && !text3_constraints.text_indent_each_line
        && !anonymous_block_holds_the_first_line(tree, ctx.styled_dom, node_index)
    {
        text3_constraints.text_indent = 0.0;
    }
    // An SVG `<text>`: its sizes are its `<svg>`'s user units, on the screen
    // at the scale of the viewBox onto the `<svg>`'s current box and of its
    // transforms (`solver3::svg`). The cached collection stays in user units;
    // the scale enters the validity key, so a resized `<svg>` lays it out
    // again.
    let (inline_content, content_hash_base) =
        match super::super::svg::text_scale(ctx.styled_dom, tree, node_index) {
            Some(scale) => {
                super::super::svg::scale_constraints(&mut text3_constraints, scale);
                (
                    Arc::new(super::super::svg::svg_text_content(ctx.styled_dom, &inline_content, scale)),
                    content_hash_base.map(|base| base ^ u64::from(scale.to_bits()).rotate_left(17)),
                )
            }
            None => (inline_content, content_hash_base),
        };

    let current_content_hash = {
        let _p = crate::probe::Probe::span("ifc_content_hash");
        use std::hash::{Hash, Hasher};
        let mut h = azul_core::hash::FastHasher::new();
        // The content component comes pre-hashed from the collection cache —
        // an equal subtree_fingerprint admitted it, so its bytes are the ones
        // this hash used to re-derive per visit. `None` cannot happen when
        // `inline_content` exists (the miss arm computes a base for every Ok
        // collection), but fall back to hashing rather than unwrapping.
        match content_hash_base {
            Some(base) => base.hash(&mut h),
            None => inline_content.hash(&mut h),
        }
        // Fold the container's styles into the validity key: everything the
        // lines are broken with besides the room they get (alignment,
        // white-space, direction, the split into columns or a multi-column
        // flow, word-break, overflow-wrap, hyphens, ...), and the indent to
        // the bit (`hash_line_inputs` rounds it to whole pixels). Most of
        // these are inherited: a restyle above the root changes them while
        // the root and its text keep their own data.
        text3_constraints.hash_line_inputs(&mut h);
        text3_constraints.text_indent.to_bits().hash(&mut h);
        h.finish()
    };

    debug_info!(
        ctx,
        "[layout_ifc] Collected {} inline content items for node {}",
        inline_content.len(),
        node_index
    );
    for (i, item) in inline_content.iter().enumerate() {
        match item {
            InlineContent::Text(run) => debug_info!(ctx, "  [{}] Text: '{}'", i, run.text),
            InlineContent::Marker {
                run,
                position_outside,
            } => debug_info!(
                ctx,
                "  [{}] Marker: '{}' (outside={})",
                i,
                run.text,
                position_outside
            ),
            InlineContent::Shape(_) => debug_info!(ctx, "  [{}] Shape", i),
            InlineContent::Image(_) => debug_info!(ctx, "  [{}] Image", i),
            _ => debug_info!(ctx, "  [{}] Other", i),
        }
    }

    debug_ifc_layout!(
        ctx,
        "Collected {} inline content items",
        inline_content.len()
    );

    if inline_content.is_empty() {
        debug_warning!(ctx, "inline_content is empty, returning default output!");
        // THE EDITING-HOST STRUT. An IFC root with nothing to type into yet
        // — an empty TextInput's value `<p>`, a `<div contenteditable>`
        // before its first character — still gets ONE line box, the strut:
        // one line-height tall, no items. That is the line the caret stands
        // on (display_list `paint_cursor` paints the strut caret for an
        // inline layout with no items) and the height the block keeps, so a
        // focused empty field is neither blank nor collapsed. Browsers do
        // the same (the editing host's placeholder `<br>`). Every OTHER
        // empty IFC renders nothing, as before.
        if let Some(strut_height) = editing_host_strut_height(ctx, tree, node_index) {
            if let Some(warm_node) = tree.warm_mut(LayoutNodeId::new(node_index)) {
                // WITH the IFC's constraints, exactly like a real line box:
                // `reshape_text_node` shapes an edit into the cached layout's
                // constraints and bails when there are none. Stored bare
                // (`CachedInlineLayout::new`), the strut was a line the caret
                // could stand on but nothing could be typed into — the first
                // keystroke into an empty TextInput was silently dropped.
                warm_node.inline_layout_result =
                    Some(Box::new(CachedInlineLayout::new_with_constraints(
                        Arc::new(text3::cache::UnifiedLayout {
                            items: Vec::new(),
                            overflow: text3::cache::OverflowInfo::default(),
                        }),
                        constraints.available_width_type,
                        false,
                        text3_constraints,
                    )));
                // What the IFC measured before it emptied is stale.
                warm_node.inline_measure_layouts = None;
                // A strut has no glyphs: its baseline sits where a line of
                // this font would put one (ascent ~ 0.8 em within the line).
                warm_node.baseline = Some(strut_height * 0.8);
            }
            ctx.reflowed_ifcs.insert(node_index);
            return Ok(LayoutOutput {
                positions: BTreeMap::new(),
                overflow_size: LogicalSize::new(0.0, strut_height),
                baseline: Some(strut_height * 0.8),
                static_positions: BTreeMap::new(),
            });
        }
        // The node has no inline-level content this pass (e.g. its only
        // inline child — a text run or an inline image — was removed by a
        // relayout). Any `inline_layout_result` left over from a previous
        // frame is now stale: the display-list generator paints inline
        // objects (images, inline-block shapes) straight out of this cached
        // layout (see display_list.rs `paint_inline_*`), so a leftover entry
        // would re-emit the removed content AND index `styled_nodes` with a
        // `source_node_id` that no longer exists in the new DOM (OOB panic).
        // Clear it so the empty IFC renders nothing, and its measures with it.
        if let Some(warm_node) = tree.warm_mut(LayoutNodeId::new(node_index)) {
            warm_node.inline_layout_result = None;
            warm_node.inline_measure_layouts = None;
        }
        return Ok(LayoutOutput::default());
    }

    // === Phase 2d: reuse of the cached layout ===
    //
    // A cached layout of the same content at the same width is this pass's
    // layout; anything else falls through to the full layout.
    {
        // A measure (a min- or max-content width) is answered from the slot
        // its earlier measures filled, a definite width from the final
        // layout. The final layout answers a measure too while it holds one:
        // an IFC measured before its first definite layout.
        let warm = tree.warm(LayoutNodeId::new(node_index));
        let cached_ifc = warm
            .and_then(|n| n.inline_measure_layouts.as_deref())
            .and_then(|m| m.get(constraints.available_width_type))
            .or_else(|| warm.and_then(|n| n.inline_layout_result.as_deref()));
        // A3-solver-17c: the lines a journaled measure flowed at a definite
        // width (`InlineMeasureLayouts::definite`), for a visit at that
        // width that finds the final layout at another.
        let kept_definite = warm
            .and_then(|n| n.inline_measure_layouts.as_deref())
            .and_then(|m| m.definite.as_ref());

        // Only reuse the cached inline layout when the available WIDTH is unchanged.
        // This fast path was built for text edits (content changes, width constant); on a
        // viewport/container resize the width differs and the text must RE-WRAP, so the
        // cached old-width layout must NOT be reused — fall through to the full layout
        // below. Without this guard, resizing kept the stale line breaks (#45). A text
        // edit lays its paragraph out again through the same stages as this function
        // (`LayoutWindow::reshape_text_node`).
        let resize_has_floats = constraints
            .bfc_state
            .as_ref()
            .is_some_and(|s| !s.floats.floats.is_empty());
        // The kept lines answer where the final layout does not: at their
        // width, of this content. They then become the final layout (below),
        // as a store of them would have made them.
        let answers = |c: &CachedInlineLayout| {
            c.is_valid_for(constraints.available_width_type, resize_has_floats)
                && c.inline_content_hash == current_content_hash
                && c.constraints.is_some()
        };
        let from_kept_definite = matches!(
            constraints.available_width_type,
            Text3AvailableSpace::Definite(_)
        ) && kept_definite.is_some_and(answers)
            && !cached_ifc.is_some_and(answers);
        let cached_ifc = if from_kept_definite {
            kept_definite
        } else {
            cached_ifc
        };
        // #11 fix: cache validity is keyed on WIDTH only, so a same-width
        // RefreshDom whose text CHANGED would otherwise reuse the stale shaped
        // layout. Require the inline content hash to match too.
        //
        // Re-flow triage (AZ_PROFILE=cpu): a steady-state fixed-page-width
        // resize still ran text_layout_flow 477× — these markers name which
        // gate rejected the cached layout for every one of those.
        let cached_ifc = match cached_ifc {
            None => {
                drop(crate::probe::Probe::span("ifc_reflow_cold"));
                None
            }
            Some(c) if !c.is_valid_for(constraints.available_width_type, resize_has_floats) => {
                // Finer buckets: dd = both DEFINITE (then by delta size —
                // "small" is sub-pixel jitter above the 0.1 eps, a rounding
                // provenance bug, not a real width change), type = the
                // constraint TYPE flipped (measure min/max-content vs final
                // definite), float = the float-gain rule.
                use Text3AvailableSpace as Avs;
                let reason = if resize_has_floats && !c.has_floats {
                    "ifc_reflow_width_floatgain"
                } else {
                    match (c.available_width, constraints.available_width_type) {
                        (Avs::Definite(old), Avs::Definite(new)) => {
                            if (old - new).abs() < 1.0 {
                                "ifc_reflow_width_dd_small"
                            } else {
                                "ifc_reflow_width_dd_big"
                            }
                        }
                        _ => "ifc_reflow_width_type",
                    }
                };
                drop(crate::probe::Probe::span(reason));
                None
            }
            Some(c) if c.inline_content_hash != current_content_hash => {
                drop(crate::probe::Probe::span("ifc_reflow_content"));
                None
            }
            Some(c) => Some(c),
        };

        if let Some(cached) = cached_ifc {
            // Only a layout this function made (it keeps its constraints)
            // is reused as it is: one another pass moved afterwards (a
            // table cell's `vertical-align` shift) is laid out again.
            if cached.constraints.is_none() {
                drop(crate::probe::Probe::span("ifc_reflow_no_constraints"));
            } else {
                // The content and the width are unchanged: the cached layout
                // is this pass's.
                debug_info!(ctx, "[layout_ifc] Phase 2d: reusing the cached layout");
                #[cfg(test)]
                IFC_REUSE_EXITS.with(|n| n.set(n.get() + 1));
                // The boxes taken out of this context: their static
                // positions are read off the laid-out items.
                let out_of_flow =
                    out_of_flow_inline_descendants(ctx.styled_dom, tree, node_index);
                // A retired layout (its items live in the dense view alone)
                // kept what this exit reads off its lines when it was
                // stored: with no box taken out of the context, nothing
                // else needs its items, and they are not expanded again.
                let kept = if out_of_flow.is_empty() {
                    cached.retired_metrics
                } else {
                    None
                };
                // This exit REPLAYS what the full path writes, from the
                // one `finish_inline_layout`: the extent with
                // its text-box-trim, the baseline the box's sizing and
                // alignment read, and the inline-block children's
                // positions. The children's sizes are assumed to be in
                // the tree already: a carried `inline_layout_result` (a
                // clone's, and a matched anonymous block's since
                // `try_reuse_anon_wrapper` carries layout state) can reach
                // this exit in a new tree, and the assumption still holds
                // there: the collection above is only reused while
                // `atomic_inline_children_are_laid_out` says the atomic
                // children it places have sizes (carried by their own
                // clones), and a re-collection lays them out.
                let output = if let Some(kept) = kept {
                    finish_retired_inline_layout(
                        &kept,
                        &cached.overflow,
                        &text3_constraints,
                        ctx.styled_dom,
                        ifc_root_dom_id,
                    )
                } else {
                    // (d6h) Materialized: the stored layout may be the
                    // retirement sentinel; measuring it raw zeroed the
                    // reuse path's overflow_size (scrollbars vanished on
                    // every reuse).
                    let main_frag = cached.materialized();
                    #[cfg(test)]
                    {
                        // `materialized` hands back the stored `Arc` unless
                        // it expanded the dense view.
                        if !Arc::ptr_eq(&main_frag, &cached.layout) {
                            IFC_REUSE_EXPANSIONS.with(|n| n.set(n.get() + 1));
                        }
                    }
                    let mut output = finish_inline_layout(
                        &main_frag,
                        &text3_constraints,
                        ctx.styled_dom,
                        ifc_root_dom_id,
                        &child_map,
                    );
                    output.static_positions = static_positions_of(
                        ctx.styled_dom,
                        tree,
                        &out_of_flow,
                        &main_frag.items,
                        &child_map,
                    );
                    output
                };
                if let Some(warm_node) = tree.warm_mut(LayoutNodeId::new(node_index)) {
                    warm_node.baseline = output.baseline;
                    if from_kept_definite {
                        // The lines a journaled measure kept at this width
                        // are the node's layout now: what is painted and
                        // hit-tested is read off `inline_layout_result`.
                        if let Some(lines) = warm_node
                            .inline_measure_layouts
                            .as_deref_mut()
                            .and_then(|kept| kept.definite.take())
                        {
                            warm_node.inline_layout_result = Some(Box::new(lines));
                            ctx.reflowed_ifcs.insert(node_index);
                        }
                        if warm_node
                            .inline_measure_layouts
                            .as_deref()
                            .is_some_and(InlineMeasureLayouts::is_empty)
                        {
                            warm_node.inline_measure_layouts = None;
                        }
                    }
                }
                return Ok(output);
            }
        }
    }

    // Phase 2: text3_constraints was resolved early (above) so the cache-reuse key
    // could include container-level properties.

    debug_info!(
        ctx,
        "[layout_ifc] laying out the inline content of node {} with {} exclusions",
        node_index,
        text3_constraints.shape_exclusions.len()
    );

    // Phase 3: Invoke the text layout engine: the content is shaped
    // (`shape_content`), then broken, positioned and reported
    // (`ParagraphToBreak`, `lay_out_shaped_ifc`), the same functions a
    // relayout after an edit runs.
    // Get pre-loaded fonts from font manager (fonts should be loaded before
    // layout): asked once per pass (`LayoutContext::loaded_fonts`), not once
    // per inline formatting context.
    let loaded_fonts = ctx.loaded_fonts();
    let flow_probe = crate::probe::Probe::span("text_layout_flow");
    // What the intrinsic measure of this root shaped, when it is what
    // `shape_content` would hand out here (`sizing::MeasuredParagraphs`):
    // the same content, direction and faces within the same pass.
    let measured = if ctx.debug_messages.is_none() {
        ctx.pass_memo.measured.take(
            node_index,
            &inline_content,
            &text3_constraints,
            ctx.font_manager.registered_fonts.generation(),
        )
    } else {
        None
    };
    let shaped = match measured {
        Some(shaped) => Ok(shaped),
        None => text_cache.shape_content(
            &inline_content,
            &[],
            &text3_constraints,
            &ctx.font_manager.font_chain_cache,
            &ctx.font_manager.fc_cache,
            &loaded_fonts,
            ctx.debug_messages,
        ),
    };
    let laid_out = shaped.and_then(|(logical_items, shaped_items)| {
        let paragraph = text3::cache::ParagraphToBreak::new(shaped_items, &text3_constraints);
        lay_out_shaped_ifc(
            &logical_items,
            &paragraph,
            &loaded_fonts,
            ctx.styled_dom,
            ifc_root_dom_id,
            &child_map,
            ctx.debug_messages,
        )
    });
    drop(flow_probe);
    let (main_frag, finished) = match laid_out {
        Ok(result) => {
            // [g133 az-web-lift DIAG] layout_flow returned Ok.
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x60688) as u32, (0xC0DE0688u32) as u32);
            }
            result
        }
        Err(e) => {
            // [g133 az-web-lift DIAG] layout_flow returned Err → zero-sized (text not positioned).
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x60688) as u32, (0x000000EEu32) as u32);
                // Read the error's first byte (discriminant) for the marker — a
                // `*const u8` read is always aligned + in-bounds; the old
                // `*const u32` read was UB on a 1-aligned / <4-byte enum.
                crate::az_mark((0x6068C) as u32, (*(&e as *const _ as *const u8)) as u32);
            }
            // Font errors should not stop layout of other elements.
            // Log the error and return a zero-sized layout.
            debug_warning!(ctx, "Text layout failed: {:?}", e);
            debug_warning!(
                ctx,
                "Continuing with zero-sized layout for node {}",
                node_index
            );

            return Ok(LayoutOutput {
                overflow_size: LogicalSize::new(0.0, 0.0),
                ..Default::default()
            });
        }
    };
    // The out-of-flow children's static positions, from the lines as they
    // stand now (`out_of_flow_static_positions`).
    let static_positions = out_of_flow_static_positions(
        ctx.styled_dom,
        tree,
        node_index,
        &main_frag.items,
        &child_map,
    );
    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();

    {
        let main_frag = &main_frag;
        let frag_bounds = main_frag.bounds();
        debug_ifc_layout!(
            ctx,
            "Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        );
        debug_ifc_layout!(ctx, "Storing inline_layout_result on node {}", node_index);

        // Determine if we should store this layout result using the new
        // CachedInlineLayout system. The key insight is that inline layouts
        // depend on available width:
        //
        // - Min-content measurement uses width ≈ 0 (maximum line wrapping)
        // - Max-content measurement uses width = ∞ (no line wrapping)
        // - Final layout uses the actual column/container width
        //
        // We must track which constraint type was used, otherwise a min-content
        // measurement would incorrectly be reused for final rendering.
        let has_floats = constraints
            .bfc_state
            .as_ref()
            .is_some_and(|s| !s.floats.floats.is_empty());
        let current_width_type = constraints.available_width_type;

        // A layout that placed NOTHING for non-empty text content is a
        // font-race artifact, not a layout: the first pass can run before
        // `load_missing_for_chains` has parsed this run's font, shaping
        // yields zero items, and text3's own caches self-heal on the next
        // call — but THIS per-node store is keyed by width+content only, so
        // an empty result would be served to every later pass ("reuse")
        // and the paragraph would measure 0.0 forever. (miniword: the
        // sample document reported 1 page; WHICH node got poisoned flipped
        // on a single leading whitespace character re-ordering the first
        // font-less pass.) Skip the store; the next pass recomputes with
        // fonts present.
        let content_has_text = inline_content
            .iter()
            .any(|c| matches!(c, InlineContent::Text(r) if !r.text.trim().is_empty()));
        if content_has_text && main_frag.items.is_empty() {
            debug_info!(
                ctx,
                "[layout_ifc] NOT caching empty layout for node {} (fonts not loaded yet?)",
                node_index
            );
            output.overflow_size = LogicalSize::zero();
            return Ok(output);
        }

        let warm_node = tree
            .warm_mut(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;

        // A measure - a layout at a min- or max-content width - is kept in a
        // slot of its own (`InlineMeasureLayouts`), so the next measure at
        // that width flows no text, and it never replaces a definite layout
        // of the same content: that one is what the box shows, and taking its
        // place made the final layout flow the text again after every measure.
        let is_measure = !matches!(current_width_type, Text3AvailableSpace::Definite(_));
        let should_store = match &warm_node.inline_layout_result {
            None => {
                // No cached result - always store
                debug_info!(
                    ctx,
                    "[layout_ifc] Storing NEW inline_layout_result for node {} (width_type={:?}, \
                     has_floats={})",
                    node_index,
                    current_width_type,
                    has_floats
                );
                true
            }
            Some(cached) => {
                // Check if the new result should replace the cached one
                let replaces_width = if is_measure {
                    !matches!(cached.available_width, Text3AvailableSpace::Definite(_))
                } else {
                    cached.should_replace_with(current_width_type, has_floats)
                };
                if replaces_width || cached.inline_content_hash != current_content_hash {
                    // #11 fix: the cached layout is what the display-list
                    // generator paints from; replace it when the inline content
                    // changed, even if the width constraint is unchanged.
                    debug_info!(
                        ctx,
                        "[layout_ifc] REPLACING inline_layout_result for node {} (old: \
                         width={:?}, floats={}) with (new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    true
                } else {
                    debug_info!(
                        ctx,
                        "[layout_ifc] KEEPING cached inline_layout_result for node {} (cached: \
                         width={:?}, floats={}, new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    false
                }
            }
        };

        if should_store || is_measure {
            #[cfg(test)]
            let runs_built_before =
                crate::solver3::layout_tree::GLYPH_RUNS_BUILT_AT_STORE.with(core::cell::Cell::get);
            // A measure's lines are painted only while they are still the
            // final layout when the display list is made: their paint runs
            // are made when they are first read, not for every measure
            // (`CachedInlineLayout::new_measure_with_constraints`).
            let mut cil = if is_measure {
                CachedInlineLayout::new_measure_with_constraints(
                    main_frag.clone(),
                    current_width_type,
                    has_floats,
                    text3_constraints.clone(),
                )
            } else {
                CachedInlineLayout::new_with_constraints(
                    main_frag.clone(),
                    current_width_type,
                    has_floats,
                    text3_constraints.clone(),
                )
            };
            #[cfg(test)]
            {
                if is_measure {
                    let built = crate::solver3::layout_tree::GLYPH_RUNS_BUILT_AT_STORE
                        .with(core::cell::Cell::get)
                        - runs_built_before;
                    IFC_MEASURE_STORES.with(|n| n.set(n.get() + 1));
                    IFC_MEASURE_STORE_RUN_BUILDS.with(|n| n.set(n.get() + built));
                }
            }
            // #11 fix: record the content hash so Phase 2d only fast-path-reuses
            // this layout when the inline content is genuinely unchanged, and so
            // the store decision above can detect content changes.
            cil.inline_content_hash = current_content_hash;
            // A measure goes to its own slot, and to the final one as well
            // while that holds no definite layout of this content.
            let (for_final, for_measures) = match (should_store, is_measure) {
                (true, true) => (Some(cil.clone()), Some(cil)),
                (true, false) => (Some(cil), None),
                (false, _) => (None, Some(cil)),
            };
            // The measures of other content are stale with this layout in,
            // and so are the lines a journaled measure kept at a definite
            // width once a layout at a definite width is the final one.
            if let Some(measures) = warm_node.inline_measure_layouts.as_deref_mut() {
                measures.retain_content(current_content_hash);
                if for_final.is_some() && !is_measure {
                    measures.definite = None;
                }
            }
            if let Some(measure) = for_measures {
                warm_node
                    .inline_measure_layouts
                    .get_or_insert_with(Box::default)
                    .keep(measure);
            }
            if warm_node
                .inline_measure_layouts
                .as_deref()
                .is_some_and(InlineMeasureLayouts::is_empty)
            {
                warm_node.inline_measure_layouts = None;
            }
            if let Some(cil) = for_final {
                warm_node.inline_layout_result = Some(Box::new(cil));
                // DL-patching invalidation: this IFC's line layout was
                // recomputed — its text items must re-emit on a patched pass.
                ctx.reflowed_ifcs.insert(node_index);
            }
        }

        // The IFC's extent, baseline and inline-block positions, from
        // `finish_inline_layout`: the cache-reuse exit above reports the
        // same, so two layouts of unchanged content agree to the bit.
        output = finished;
        output.static_positions = static_positions;
        warm_node.baseline = output.baseline;
    }

    // [g132 az-web-lift VERIFY] Capture the IFC content geometry (the line-box bounds from
    // main_frag.bounds(), set above as output.overflow_size). height>0 proves the text LAID OUT
    // (not just shaped). Free-band addrs, f32 bits. REVERT at cleanup.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark(
            (0x60670) as u32,
            (output.overflow_size.width.to_bits()) as u32,
        );
        crate::az_mark(
            (0x60674) as u32,
            (output.overflow_size.height.to_bits()) as u32,
        );
        crate::az_mark((0x60678) as u32, (output.positions.len() as u32) as u32);
        crate::az_mark((0x6067C) as u32, (0xC0DE0132u32) as u32);
    }

    Ok(output)
}

/// Folds the RESOLVED style of DOM node `i` - every compact tier the inline
/// collection reads - into the inline-collection key of `layout_ifc`: the
/// tier-1 enums, the tier-2 dimensions (font size, and the box sizes and
/// edges of inline boxes and atomic inlines, whose measured sizes the
/// collection caches) and the whole tier-2b text block (font family,
/// colour, line height, letter / word spacing, indent).
///
/// The node fingerprints cover a node's own data and inline CSS, not what an
/// author STYLESHEET resolved onto it: the same DOM under another stylesheet
/// (or an inherited value moved by an ancestor's restyle) compared equal on
/// the tier-1 enums and the font family alone, and reused runs of the old
/// font size and atomic inlines of the old size (FIX9 1.2, TEXT7 found (c)).
/// The compact cache stores computed values (inherited ones included), so a
/// change anywhere above reaches every node of the subtree here.
pub(super) fn hash_resolved_style<H: core::hash::Hasher>(
    cc: &azul_css::compact_cache::CompactLayoutCache,
    i: usize,
    h: &mut H,
) {
    use core::hash::Hash;
    if let Some(t1) = cc.tier1_enums.get(i) {
        t1.hash(h);
    }
    if let Some(d) = cc.tier2_dims.get(i) {
        (
            d.width,
            d.height,
            d.min_width,
            d.max_width,
            d.min_height,
            d.max_height,
            d.flex_basis,
            d.font_size,
        )
            .hash(h);
        (
            d.padding_top,
            d.padding_right,
            d.padding_bottom,
            d.padding_left,
            d.margin_top,
            d.margin_right,
            d.margin_bottom,
            d.margin_left,
        )
            .hash(h);
        (
            d.border_top_width,
            d.border_right_width,
            d.border_bottom_width,
            d.border_left_width,
            d.top,
            d.right,
            d.bottom,
            d.left,
        )
            .hash(h);
        (d.flex_grow, d.flex_shrink, d.row_gap, d.column_gap).hash(h);
    }
    if let Some(t) = cc.tier2b_text.get(i) {
        (
            t.text_color,
            t.font_family_hash,
            t.line_height,
            t.letter_spacing,
            t.word_spacing,
            t.text_indent,
        )
            .hash(h);
    }
}

// ============================================================================
// INITIAL LETTER / DROP CAPS STUB
// ============================================================================

/// Computes the geometric exclusion area for an initial letter (drop cap).
///
/// CSS Inline Layout Module Level 3, section 3:
/// The `initial-letter` property specifies styling for dropped, raised, and sunken
/// initial letters. When set, the first glyph(s) of the first line are enlarged to
/// span multiple lines, with the remaining text wrapping around them.
// +spec:box-model:c93797 - initial-letter alignment points determined from contents (not
// border-box)
///
/// # Algorithm
///
/// 1. The letter box height spans `size` lines: `height = size * line_height`.
/// 2. The letter box width is estimated using a typical capital letter aspect ratio
///    (cap-height-to-advance-width ~0.7 for Latin text). A proper implementation would measure the
///    actual glyph, but this gives a reasonable default.
/// 3. The letter is positioned at the inline-start of the first line.
/// 4. The `sink` value determines how many lines the letter drops below the first baseline. When
///    `sink == size`, this is a classic drop cap. When `sink < size`, the letter rises above the
///    first line (raised cap).
/// 5. A small gap (4px default) is added between the letter box and adjacent text.
///
/// # Parameters
/// - `initial_letter_size`: The number of lines the initial letter should span (e.g., 3.0)
/// - `initial_letter_sink`: How many lines the letter sinks below the first line
/// - `content_box_width`: Available width in the content box (for clamping)
/// - `line_height`: The computed line height for the containing block
///
/// # Returns
/// A tuple of `(letter_width, letter_height)` representing the space reserved for
/// the initial letter exclusion, or `(0.0, 0.0)` if the parameters are invalid.
///
/// The caller should use these dimensions to create a float-like exclusion at the
/// start of the block container, causing subsequent lines to wrap around the letter.
// +spec:width-calculation:7f4f68 - initial-letter-wrap exclusion area (none behavior; first/grid
// require glyph outlines)
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[must_use]
pub fn layout_initial_letter(
    initial_letter_size: f32,
    initial_letter_sink: u32,
    content_box_width: f32,
    line_height: f32,
) -> (f32, f32) {
    // Estimate the letter width using a typical Latin capital letter aspect ratio.
    // The advance width of a capital letter is approximately 0.7x the cap height.
    // This is a heuristic; a full implementation would measure the actual glyph(s).
    const CAP_WIDTH_RATIO: f32 = 0.7;

    // Add a small gap between the letter box and the adjacent inline content.
    // CSS Inline Level 3 section 3.5: browsers typically add ~4px padding.
    const LETTER_GAP: f32 = 4.0;

    // Guard against degenerate values
    if initial_letter_size <= 0.0 || line_height <= 0.0 || content_box_width <= 0.0 {
        return (0.0, 0.0);
    }

    // +spec:overflow:dd0679 - auto-sized initial letter content box fits exactly to content;
    // alignment props do not apply +spec:width-calculation:170742 - atomic initial letters with
    // auto block size use inline initial letter sizing CSS Inline Level 3 section 3.3: The
    // initial letter box height spans `size` lines.
    let letter_height = initial_letter_size * line_height;

    let letter_width_raw = letter_height * CAP_WIDTH_RATIO;

    let letter_width = (letter_width_raw + LETTER_GAP).min(content_box_width);

    // +spec:containing-block:67fd99 - block-axis positioning: size >= sink shifts by
    // (sink-1)*line_height toward block-end The actual exclusion height accounts for the sink
    // value. sink == size means the letter is fully dropped (classic drop cap).
    // sink < size means part of the letter rises above the first line (raised cap).
    // The exclusion area height is always `sink * line_height` since that's how
    // many lines of subsequent text need to wrap around the letter.
    let exclusion_height = (initial_letter_sink as f32) * line_height;

    // Use the larger of exclusion_height and letter_height as the actual
    // vertical space consumed. For raised caps (sink < size), the letter
    // extends above the first line but the exclusion only covers sink lines.
    // For sunken caps (sink >= size), the exclusion covers the full letter height.
    let effective_height = exclusion_height.max(letter_height);

    (letter_width, effective_height)
}

/// What a named flow lays a paragraph out from, region by region: its
/// shaped content, the constraints its lines are broken with, and the
/// layout nodes of its atomic inlines.
#[derive(Debug, Clone)]
pub(crate) struct FlowParagraph {
    /// The paragraph's logical items.
    pub(crate) logical_items: Arc<Vec<text3::cache::LogicalItem>>,
    /// Its shaped items.
    pub(crate) shaped_items: Arc<Vec<text3::cache::ShapedItem>>,
    /// The constraints of its lines at the width it was asked for, with no
    /// height: each region sets its own room.
    pub(crate) constraints: UnifiedConstraints,
    /// `ContentIndex` -> the layout node of an atomic inline.
    pub(crate) child_map: Arc<HashMap<ContentIndex, usize>>,
    /// The DOM node its style is read from.
    pub(crate) style_dom_id: NodeId,
}

/// The paragraph `node_index` (the root of an inline formatting context) as
/// a named flow lays it out, its lines `width` wide: the content the main
/// pass collected for it, shaped (the shaping cache holds it), and its line
/// constraints. `None` when the main pass collected nothing for it.
pub(crate) fn flow_paragraph<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &LayoutTree,
    node_index: usize,
    width: f32,
) -> Option<FlowParagraph> {
    let node = tree.get(LayoutNodeId::new(node_index))?;
    let anonymous = node.dom_node_id.is_none();
    let style_dom_id = ifc_root_style_dom_id(tree, node_index)?;
    let collected = tree
        .warm(LayoutNodeId::new(node_index))?
        .inline_content_cache
        .as_ref()?;
    let content = Arc::clone(&collected.content);
    let child_map = Arc::clone(&collected.child_map);
    if content.is_empty() {
        return None;
    }
    let line_style = LineStyle::of(
        ctx.styled_dom,
        style_dom_id,
        &mut ctx.style_cache,
        ctx.system_style.as_ref(),
        PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
    );
    let room = LogicalSize::new(width, f32::INFINITY);
    let constraints = LayoutConstraints {
        available_size: room,
        writing_mode: LayoutWritingMode::HorizontalTb,
        writing_mode_ctx: crate::solver3::geometry::WritingModeContext::default(),
        bfc_state: None,
        text_align: TextAlign::Start,
        containing_block_size: room,
        available_width_type: Text3AvailableSpace::Definite(width),
        fragmentainer: None,
        column_flow: None,
    };
    let mut text3_constraints =
        translate_to_text3_constraints(ctx, &constraints, &line_style, anonymous);
    // The anonymous block holds its container's first line only when it is
    // the container's first in-flow box (as in `layout_ifc`).
    if anonymous
        && !text3_constraints.text_indent_each_line
        && !anonymous_block_holds_the_first_line(tree, ctx.styled_dom, node_index)
    {
        text3_constraints.text_indent = 0.0;
    }
    let loaded_fonts = ctx.loaded_fonts();
    let (logical_items, shaped_items) = text_cache
        .shape_content(
            &content,
            &[],
            &text3_constraints,
            &ctx.font_manager.font_chain_cache,
            &ctx.font_manager.fc_cache,
            &loaded_fonts,
            ctx.debug_messages,
        )
        .ok()?;
    Some(FlowParagraph {
        logical_items,
        shaped_items,
        constraints: text3_constraints,
        child_map,
        style_dom_id,
    })
}

#[cfg(test)]
mod audit_a3_solver_tests {
    use super::*;

    /// A positioned cluster of the text node `node`: one glyph 10px wide, at `(x, y)` on line
    /// `line`, right-to-left or not.
    fn cluster(node: usize, line: usize, x: f32, y: f32, rtl: bool) -> text3::cache::PositionedItem {
        use crate::text3::{
            cache::{
                BidiDirection, BidiLevel, ClusterFlags, ContentIndex, GlyphKind,
                GraphemeClusterId, LayoutFontMetrics, Point, PositionedItem, ShapedCluster,
                ShapedGlyph, ShapedItem, StyleProperties,
            },
            script::Script,
        };
        let direction = if rtl {
            BidiDirection::Rtl
        } else {
            BidiDirection::Ltr
        };
        let glyph = ShapedGlyph {
            kind: GlyphKind::Character,
            glyph_id: 7,
            cluster_offset: 0,
            advance: 10.0,
            kerning: 0.0,
            offset: Point { x: 0.0, y: 0.0 },
            vertical_advance: 0.0,
            vertical_offset: Point { x: 0.0, y: 0.0 },
            script: Script::Latin,
            font_hash: 0xF0F0,
            font_metrics: LayoutFontMetrics {
                ascent: 800.0,
                descent: -200.0,
                line_gap: 0.0,
                units_per_em: 1000,
                x_height: None,
                cap_height: None,
                browser_ascent_boost: false,
            },
        };
        PositionedItem {
            item: ShapedItem::Cluster(ShapedCluster {
                flags: ClusterFlags::classify("a"),
                source_text: Arc::from("a"),
                source_byte_len: 1,
                source_cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                source_content_index: ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                source_node_id: Some(NodeId::new(node)),
                glyphs: core::iter::once(glyph).collect(),
                advance: 10.0,
                direction,
                bidi_level: BidiLevel::paragraph(direction),
                style: Arc::new(StyleProperties {
                    font_size_px: 20.0,
                    ..StyleProperties::default()
                }),
                marker_position_outside: None,
                is_first_fragment: true,
                is_last_fragment: true,
            }),
            position: Point { x, y },
            line_index: line,
        }
    }

    /// A tab `height` tall at the top of line `line`: an item of no DOM node that widens its
    /// line.
    fn tab(line: usize, x: f32, height: f32) -> text3::cache::PositionedItem {
        text3::cache::PositionedItem {
            item: text3::cache::ShapedItem::Tab {
                source: text3::cache::ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                bounds: text3::cache::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 8.0,
                    height,
                },
            },
            position: text3::cache::Point { x, y: 0.0 },
            line_index: line,
        }
    }

    /// A forced break on line `line`: an item without a height.
    fn line_break(line: usize) -> text3::cache::PositionedItem {
        text3::cache::PositionedItem {
            item: text3::cache::ShapedItem::Break {
                source: text3::cache::ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                break_info: InlineBreak {
                    break_type: BreakType::Hard,
                    clear: ClearType::None,
                    content_index: 0,
                },
            },
            position: text3::cache::Point { x: 0.0, y: 0.0 },
            line_index: line,
        }
    }

    /// A3-solver-16: the places `StaticPlaces` reads off one listing of the items are, to the
    /// bit, the places `inline_static_place` finds by walking the items for each box - for
    /// every box before, between and after the nodes with content, asked in any order, over
    /// items whose nodes come in no order, with a node on two lines, right-to-left anchors, an
    /// item of no node and an item of no height.
    ///
    /// Mutation: `StaticPlaces::place` taking a node's FIRST item where the box comes after
    /// the node puts a box after node 3 on line 0 instead of line 2.
    #[test]
    fn the_places_listed_once_are_the_places_the_scan_finds() {
        let items = vec![
            cluster(3, 0, 0.0, 2.0, false),
            cluster(3, 0, 10.0, 2.0, false),
            tab(0, 20.0, 30.0),
            cluster(7, 0, 40.0, 4.0, true),
            cluster(7, 1, 0.0, 30.0, true),
            cluster(5, 1, 10.0, 28.0, false),
            line_break(1),
            cluster(9, 2, 0.0, 60.0, false),
            cluster(3, 2, 10.0, 61.0, false),
        ];
        let reversed: Vec<text3::cache::PositionedItem> = items.iter().rev().cloned().collect();
        let node_of = |item: &ShapedItem| match item {
            ShapedItem::Cluster(c) => c.source_node_id,
            _ => None,
        };
        let bits = |place: Option<InlineStaticPlace>| {
            place.map(|p| {
                (
                    p.x.to_bits(),
                    p.line_top.to_bits(),
                    p.line_bottom.to_bits(),
                    p.after_content,
                    p.line,
                )
            })
        };
        for list in [&items[..], &reversed[..], &items[..0]] {
            let mut places = StaticPlaces::new(list, node_of);
            // Asked out of order and twice: the extents of the lines are kept between asks.
            for child in (0..12).chain((0..12).rev()) {
                let child = NodeId::new(child);
                assert_eq!(
                    bits(places.place(child)),
                    bits(inline_static_place(list, node_of, child)),
                    "the box {child:?} among {} items",
                    list.len()
                );
            }
        }
        // Node 3 has items on the lines 0 and 2: a box right after it stands after its LAST
        // item, a box before all content before its FIRST.
        let mut places = StaticPlaces::new(&items, node_of);
        let after_node_3 = places.place(NodeId::new(4)).expect("content comes before it");
        assert!(after_node_3.after_content && after_node_3.line == 2);
        let before_all = places.place(NodeId::new(0)).expect("content comes after it");
        assert!(!before_all.after_content && before_all.line == 0);
    }
}
