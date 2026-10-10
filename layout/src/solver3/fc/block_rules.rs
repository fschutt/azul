//! What a block box is to its flow: new formatting contexts, margin collapsing, emptiness, in-flow boxes, scrollbar needs.

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

// +spec:block-formatting-context:40e03e - BFC root: block container establishing new BFC (contains
// floats, excludes external floats, suppresses margin collapsing)
/// Checks if a node establishes a new Block Formatting Context (BFC).
///
/// Per CSS 2.2 § 9.4.1, a BFC is established by:
/// - Floats (elements with float other than 'none')
/// - Absolutely positioned elements (position: absolute or fixed)
/// - Block containers that are not block boxes (e.g., inline-blocks, table-cells)
/// - Block boxes with 'overflow' other than 'visible' and 'clip'
/// - Elements with 'display: flow-root'
/// - Table cells, table captions, and inline-blocks
///
/// Normal flow block-level boxes do NOT establish a new BFC.
///
/// This is critical for correct float interaction: normal blocks should overlap floats
/// (not shrink around them), while their inline content wraps around floats.
// +spec:block-formatting-context:241d22 - block container establishes new BFC or continues
// parent's, based on overflow/position/float/display +spec:block-formatting-context:9fe441 - BFC
// establishment based on position, float, overflow, and display properties +spec:display-property:
// 3c7369 - block boxes establishing independent FC create new BFC; flex containers already do;
// non-replaced inlines cannot +spec:positioning:1e94f6 - floats, abspos,
// inline-blocks/table-cells/table-captions, overflow!=visible establish new BFC
pub(super) fn establishes_new_bfc<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
    cold: Option<&LayoutNodeCold>,
) -> bool {
    // +spec:block-formatting-context:f39cd3 - table wrapper box establishes a BFC (CSS 2.2 §17.4)
    // Anonymous table wrapper boxes have no dom_node_id but must still establish BFC
    // +spec:height-calculation:e20498 - table wrapper box establishes BFC (CSS 2.2 §17.4)
    // +spec:positioning:b780d3 - Table wrapper box establishes BFC (CSS 2.2 § 17.4)
    if cold.and_then(|c| c.anonymous_type) == Some(AnonymousBoxType::TableWrapper) {
        return true;
    }
    // CSS 2.2 s9.4.1: a table cell establishes a BFC - an ANONYMOUS cell
    // too (s17.2.1: the one a `display: table` box's `<p>` sits in). With
    // no DOM node it answered "no", its first child's top margin and its
    // last child's bottom margin escaped the cell, and no box outside a
    // cell takes them: `display: table; padding: 12px` around `<p>Hi</p>`
    // was 44px tall, Chrome 76 (MAIL6).
    if matches!(node.formatting_context, FormattingContext::TableCell) {
        return true;
    }
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    // 1. Floats establish BFC
    let float_val = get_float(ctx.styled_dom, dom_id, node_state);
    if matches!(
        float_val,
        MultiValue::Exact(LayoutFloat::Left | LayoutFloat::Right)
    ) {
        return true;
    }

    // +spec:positioning:69468c - absolute/fixed forces independent formatting context
    let position = get_position_type(ctx.styled_dom, Some(dom_id));
    if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
        return true;
    }

    // 3. Inline-blocks, table-cells, table-captions establish BFC
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    if matches!(
        display,
        MultiValue::Exact(
            LayoutDisplay::InlineBlock | LayoutDisplay::TableCell | LayoutDisplay::TableCaption
        )
    ) {
        return true;
    }

    // 4. display: flow-root establishes BFC
    // +spec:display-property:14bae6 - flow-root establishes a formatting context that
    // contains/excludes floats
    if matches!(display, MultiValue::Exact(LayoutDisplay::FlowRoot)) {
        return true;
    }

    // +spec:overflow:0a944d - clip does NOT establish BFC; hidden/scroll/auto do establish BFC
    // +spec:overflow:631a4c - scroll containers establish independent formatting context (BFC)
    // +spec:overflow:f6a186 - overflow:clip does NOT establish BFC; use display:flow-root for that
    // +spec:overflow:717de1 - overflow != visible/clip establishes BFC per CSS 2.2 §9.4.1
    // +spec:positioning:6feb32 - overflow:clip does NOT establish new formatting context;
    // hidden/scroll/auto do
    // 5. Block boxes with overflow other than 'visible' or 'clip' establish BFC
    // +spec:overflow:b34aef - Block boxes with overflow other than 'visible' or 'clip' establish
    // BFC Note: 'clip' does NOT establish BFC per CSS Overflow Module Level 3
    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, node_state);

    let creates_bfc_via_overflow = |ov: &MultiValue<LayoutOverflow>| {
        matches!(
            ov,
            &MultiValue::Exact(
                LayoutOverflow::Hidden | LayoutOverflow::Scroll | LayoutOverflow::Auto
            )
        )
    };

    if creates_bfc_via_overflow(&overflow_x) || creates_bfc_via_overflow(&overflow_y) {
        return true;
    }

    // +spec:multi-column - a multi-column container establishes a new block
    // formatting context (CSS Multicol 1 §2): its children's margins and
    // floats stay inside it, column by column.
    if crate::solver3::multicol::is_multicol_container(ctx.styled_dom, dom_id, node_state) {
        return true;
    }

    // 6. Table, Flex, and Grid containers establish BFC (via FormattingContext)
    // +spec:block-formatting-context:f15b87 - display:table participates in a BFC
    if matches!(
        node.formatting_context,
        FormattingContext::Table | FormattingContext::Flex | FormattingContext::Grid
    ) {
        return true;
    }

    // +spec:block-formatting-context:f15b87 - a flex/grid ITEM establishes an
    // independent formatting context for its contents (CSS Flexbox 1 § 3, CSS Grid 1
    // § 6). Its children's margins are therefore contained and must NOT collapse
    // through it — without this, the last child's margin-bottom escapes and the item
    // (e.g. the invoice `.head`'s inner div) reports a cross size short by that margin,
    // so the whole flex container is under-tall. Detect it from the parent's display.
    {
        let hierarchy = ctx.styled_dom.node_hierarchy.as_container();
        if let Some(parent_dom_id) = hierarchy[dom_id].parent_id() {
            let parent_display = get_display_property(ctx.styled_dom, Some(parent_dom_id));
            if matches!(
                parent_display,
                MultiValue::Exact(
                    LayoutDisplay::Flex
                        | LayoutDisplay::InlineFlex
                        | LayoutDisplay::Grid
                        | LayoutDisplay::InlineGrid
                )
            ) {
                return true;
            }
        }
    }

    // +spec:block-formatting-context:33e6cd - block container with different writing-mode than
    // parent establishes independent BFC CSS Writing Modes 4 § 3.2: if a block container has a
    // different writing-mode than its parent, its inner display type computes to flow-root
    // (i.e., it establishes BFC).
    {
        let hierarchy = ctx.styled_dom.node_hierarchy.as_container();
        if let Some(parent_dom_id) = hierarchy[dom_id].parent_id() {
            let parent_state =
                &ctx.styled_dom.styled_nodes.as_container()[parent_dom_id].styled_node_state;
            let child_wm = get_writing_mode(ctx.styled_dom, dom_id, node_state).unwrap_or_default();
            let parent_wm =
                get_writing_mode(ctx.styled_dom, parent_dom_id, parent_state).unwrap_or_default();
            if child_wm != parent_wm {
                return true;
            }
        }
    }

    // Normal flow block boxes do NOT establish BFC
    // NOTE: align-content != normal should also establish BFC per CSS-DISPLAY-3, but align-content
    // is not yet implemented for block containers
    false
}

// +spec:display-property:5e5420 - replaced element identification (glossary: replaced elements have
// natural dimensions, establish independent formatting context)
/// CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element
/// in the normal flow that establishes a new block formatting context [...] must not overlap
/// the margin box of any floats in the same block formatting context as the element itself."
pub(super) fn is_block_level_replaced<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
) -> bool {
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    // Check display is block-level
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    let is_block_level = matches!(
        display,
        MultiValue::Exact(LayoutDisplay::Block | LayoutDisplay::ListItem | LayoutDisplay::FlowRoot)
    );

    if !is_block_level {
        return false;
    }

    // Check if the element is a replaced element (image, video, etc.)
    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    matches!(node_data.get_node_type(), NodeType::Image(_))
}
/// Helper to determine if scrollbars are needed.
///
/// # CSS Spec Reference
/// CSS Overflow Module Level 3 § 3: Scrollable overflow
// +spec:block-formatting-context:50d915 - overflow-x handles horizontal, overflow-y handles
// vertical +spec:box-model:63d6f2 - scrollable overflow extends beyond padding edge, needs scroll
// mechanism +spec:box-model:45b5fb - scrollbar space subtracted from content area, inserted between
// inner border edge and outer padding edge +spec:box-model:70a0a4 - UAs must start assuming no
// scrollbars needed, recalculate if they are +spec:box-model:c1b0b2 - scrollbar gutter is space
// between inner border edge and outer padding edge +spec:overflow:4f5b99 - scrollable overflow
// rectangle: content_size is the minimal axis-aligned rect containing scrollable overflow
// +spec:overflow:e983f4 - overflow:auto/scroll boxes must allow user to access overflowed content
// via scrollbars +spec:overflow:97c257 - relative positioning causing overflow in auto/scroll boxes
// must trigger scrollbar creation
#[must_use]
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
    scrollbar_width_px: f32,
) -> ScrollbarRequirements {
    // Use epsilon for float comparisons to avoid showing scrollbars due to
    // floating-point rounding errors. Without this, content that exactly fits
    // may show scrollbars due to sub-pixel differences (e.g., 299.9999 vs 300.0).
    const EPSILON: f32 = 1.0;

    // +spec:height-calculation:c5af64 - assume no scrollbars initially; only add if content
    // overflows Determine if scrolling is needed based on overflow properties.
    // +spec:overflow:30a49c - start assuming no scrollbars, recalculate if needed
    // Note: scrollbar_width_px can be 0 for overlay scrollbars (e.g. macOS),
    // but we still need to register scroll nodes so that scrolling works —
    // overlay scrollbars just don't reserve any layout space.
    let mut needs_horizontal = match overflow_x {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.width > container_size.width + EPSILON,
    };

    let mut needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height + EPSILON,
    };

    // +spec:box-model:c3d73f - scrollbar presence affects available content area; padding preserved
    // at scroll end +spec:overflow:d79159 - scrollbar sizing: adding a scrollbar reduces
    // available space, which may cause content to overflow, confirming the scrollbar is needed
    // (two-pass check) A classic layout problem: a vertical scrollbar can reduce horizontal
    // space, causing a horizontal scrollbar to appear, which can reduce vertical space...
    // A full solution involves a loop, but this two-pass check handles most cases.
    // Only relevant when scrollbars reserve layout space (non-overlay).
    if scrollbar_width_px > 0.0 {
        if needs_vertical
            && !needs_horizontal
            && overflow_x == OverflowBehavior::Auto
            && content_size.width > (container_size.width - scrollbar_width_px) + EPSILON
        {
            needs_horizontal = true;
        }
        if needs_horizontal
            && !needs_vertical
            && overflow_y == OverflowBehavior::Auto
            && content_size.height > (container_size.height - scrollbar_width_px) + EPSILON
        {
            needs_vertical = true;
        }
    }

    ScrollbarRequirements {
        needs_horizontal,
        needs_vertical,
        // `bar_kind` and `visual_width_px` - whether a bar is DRAWN on the
        // axes that need one, and how thick - are set by the caller
        // (`compute_scrollbar_info_core`), since this function doesn't have
        // access to the CSS style context. Until then: no bar.
        bar_kind: ScrollbarKind::None,
        scrollbar_width: if needs_vertical {
            scrollbar_width_px
        } else {
            0.0
        },
        scrollbar_height: if needs_horizontal {
            scrollbar_width_px
        } else {
            0.0
        },
        visual_width_px: 0.0,
    }
}

/// Calculates a single collapsed margin from two adjoining vertical margins.
///
/// Implements the rules from CSS 2.1 section 8.3.1:
/// - If both margins are positive, the result is the larger of the two.
/// - If both margins are negative, the result is the more negative of the two.
/// - If the margins have mixed signs, they are effectively summed.
// +spec:margin-collapsing:814a26 - vertical margins between sibling blocks collapse
#[must_use]
pub fn collapse_margins(a: f32, b: f32) -> f32 {
    if a.is_sign_positive() && b.is_sign_positive() {
        a.max(b)
    } else if a.is_sign_negative() && b.is_sign_negative() {
        a.min(b)
    } else {
        a + b
    }
}

/// Helper function to advance the pen position with margin collapsing.
///
/// This implements CSS 2.1 margin collapsing for adjacent block-level boxes in a BFC.
///
/// - `pen` - Current main-axis position (will be modified)
/// - `last_margin_bottom` - The bottom margin of the previous in-flow element
/// - `current_margin_top` - The top margin of the current element
///
/// # Returns
///
/// The new `last_margin_bottom` value (the bottom margin of the current element)
///
/// # CSS Spec Compliance
///
/// Per CSS 2.1 Section 8.3.1 "Collapsing margins":
///
/// - Adjacent vertical margins of block boxes collapse
/// - The resulting margin width is the maximum of the adjoining margins (if both positive)
/// - Or the sum of the most positive and most negative (if signs differ)
pub(super) fn advance_pen_with_margin_collapse(
    pen: &mut f32,
    last_margin_bottom: f32,
    current_margin_top: f32,
) -> f32 {
    // Collapse the previous element's bottom margin with current element's top margin
    let collapsed_margin = collapse_margins(last_margin_bottom, current_margin_top);

    // Advance pen by the collapsed margin
    *pen += collapsed_margin;

    // Return collapsed_margin so caller knows how much space was actually added
    collapsed_margin
}

/// Checks if an element's border or padding prevents margin collapsing.
///
/// Per CSS 2.1 Section 8.3.1:
///
/// - Border between margins prevents collapsing
/// - Padding between margins prevents collapsing
///
/// # Arguments
///
/// - `box_props` - The box properties containing border and padding
/// - `writing_mode` - The writing mode to determine main axis
/// - `check_start` - If true, check main-start (top); if false, check main-end (bottom)
///
/// # Returns
///
/// `true` if border or padding exists and prevents collapsing
// +spec:box-model:ca8ceb - margin collapsing uses block-start/block-end per writing mode
pub(super) fn has_margin_collapse_blocker(
    box_props: &BoxProps,
    writing_mode: LayoutWritingMode,
    check_start: bool, // true = check top/start, false = check bottom/end
) -> bool {
    if check_start {
        // Check if there's border-top or padding-top
        let border_start = box_props.border.main_start(writing_mode);
        let padding_start = box_props.padding.main_start(writing_mode);
        border_start > 0.0 || padding_start > 0.0
    } else {
        // Check if there's border-bottom or padding-bottom
        let border_end = box_props.border.main_end(writing_mode);
        let padding_end = box_props.padding.main_end(writing_mode);
        border_end > 0.0 || padding_end > 0.0
    }
}

/// Checks if an element is empty (has no content).
///
/// Per CSS 2.1 Section 8.3.1:
///
/// > If a block element has no border, padding, inline content, height, or min-height,
/// > then its top and bottom margins collapse with each other.
///
/// # Arguments
///
/// - `node` - The layout node to check
///
/// # Returns
///
/// `true` if the element is empty and its margins can collapse internally
pub(super) fn is_empty_block(tree: &LayoutTree, node_index: usize) -> bool {
    let Some(node) = tree.get(LayoutNodeId::new(node_index)) else {
        return true;
    };
    // Per CSS 2.2 § 8.3.1: An empty block is one that:
    // - Has zero computed 'min-height'
    // - Has zero or 'auto' computed 'height'
    // - Has no in-flow children
    // - Has no line boxes (no text/inline content)

    // Check if node has children
    if !tree.children(node_index).is_empty() {
        return false;
    }

    // Check if node has inline content (text)
    if tree
        .warm(LayoutNodeId::new(node_index))
        .and_then(|w| w.inline_layout_result.as_ref())
        .is_some()
    {
        return false;
    }

    // Check if node has explicit height > 0
    // CSS 2.2 § 8.3.1: Elements with explicit height are NOT empty
    if let Some(size) = node.used_size {
        if size.height > 0.0 {
            return false;
        }
    }

    // Empty block: no children, no inline content, no height
    true
}

/// Whether the box at `index` is in its parent's flow: not a `::marker`,
/// not absolutely positioned, not floated. Anonymous boxes are.
pub(super) fn is_in_flow_box(tree: &LayoutTree, styled_dom: &StyledDom, index: usize) -> bool {
    if is_marker_box(tree, index) {
        return false;
    }
    let Some(dom_id) = tree
        .get(LayoutNodeId::new(index))
        .and_then(|n| n.dom_node_id)
    else {
        return true;
    };
    !matches!(
        get_position_type(styled_dom, Some(dom_id)),
        LayoutPosition::Absolute | LayoutPosition::Fixed
    ) && get_float_property(styled_dom, Some(dom_id)) == LayoutFloat::None
}

/// The first in-flow child box of `index` ([`is_in_flow_box`]).
pub(super) fn first_in_flow_child(tree: &LayoutTree, styled_dom: &StyledDom, index: usize) -> Option<usize> {
    tree.children(index)
        .iter()
        .copied()
        .find(|&child| is_in_flow_box(tree, styled_dom, child))
}
