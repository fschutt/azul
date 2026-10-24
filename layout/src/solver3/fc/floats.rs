//! Floats (CSS 2.2 9.5): the floating context, float placement and clearance.

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

/// Represents a single floated element within a BFC.
#[derive(Debug, Clone, Copy)]
pub(super) struct FloatBox {
    /// The type of float (Left or Right).
    pub(super) kind: LayoutFloat,
    /// The rectangle of the float's content box (origin includes top/left margin offset).
    pub(super) rect: LogicalRect,
    /// The margin sizes (needed to calculate true margin-box bounds).
    pub(super) margin: EdgeSizes,
}

/// Manages the state of all floated elements within a Block Formatting Context.
// +spec:block-formatting-context:a4e6f9 - float rules reference only elements in the same BFC
// (scoped via BfcState) +spec:floats:2fa329 - Float positioning (left/right shift), content flow
// along sides, and clear property
/// +spec:floats:970b4c - Implements CSS2§9.5 float positioning and flow interaction
#[derive(Debug, Default, Clone)]
pub struct FloatingContext {
    /// All currently positioned floats within the BFC.
    pub floats: Vec<FloatBox>,
}

impl FloatingContext {
    /// Add a newly positioned float to the context
    pub fn add_float(&mut self, kind: LayoutFloat, rect: LogicalRect, margin: EdgeSizes) {
        self.floats.push(FloatBox { kind, rect, margin });
    }

    // +spec:box-model:0c9b13 - line boxes next to floats are shortened to make room
    // +spec:floats:148fcd - floating boxes reduce available line box width between containing block
    // edges +spec:floats:49a491 - Line boxes stacked with no separation except float clearance,
    // never overlap +spec:floats:8974e6 - text flows into vacated space by narrowing line boxes
    // around floats +spec:floats:af94f2 - content displaced by float: line boxes shrink to
    // avoid float margin boxes +spec:floats:e5961b - remaining text flows into vacated space
    // via available_line_box_space +spec:inline-formatting-context:7cbe58 - shortened line
    // boxes due to floats; shift down if too small
    /// Finds the available space on the cross-axis for a line box at a given main-axis range.
    // +spec:containing-block:4b0c44 - line boxes shortened by floats resume containing block width
    // after float
    ///
    /// Returns a tuple of (`cross_start_offset`, `cross_end_offset`) relative to the
    /// BFC content box, defining the available space for an in-flow element.
    // +spec:inline-formatting-context:e70328 - line box width reduced by floats between containing
    // block edges
    #[must_use]
    pub fn available_line_box_space(
        &self,
        main_start: f32,
        main_end: f32,
        bfc_cross_size: f32,
        wm: LayoutWritingMode,
    ) -> (f32, f32) {
        let mut available_cross_start = 0.0_f32;
        let mut available_cross_end = bfc_cross_size;

        for float in &self.floats {
            // Get the logical main-axis span of the existing float's MARGIN BOX.
            let float_main_start = float.rect.origin.main(wm) - float.margin.main_start(wm);
            let float_main_end = float_main_start
                + float.rect.size.main(wm)
                + float.margin.main_start(wm)
                + float.margin.main_end(wm);

            // Check for overlap on the main axis.
            if main_end > float_main_start && main_start < float_main_end {
                // CSS 2.2 § 9.5: border box must not overlap MARGIN BOX of floats,
                // so we include the float's margins in the cross-axis bounds.
                let float_cross_start = float.rect.origin.cross(wm) - float.margin.cross_start(wm);
                let float_cross_end = float_cross_start
                    + float.rect.size.cross(wm)
                    + float.margin.cross_start(wm)
                    + float.margin.cross_end(wm);

                // +spec:floats:17a63f - float left/right map to line-left/line-right via logical
                // coords +spec:writing-modes:e55820 - line-relative mappings:
                // left/right interpreted as line-left/line-right per writing mode
                if float.kind == LayoutFloat::Left {
                    // "line-left", i.e., cross-start
                    available_cross_start = available_cross_start.max(float_cross_end);
                } else {
                    // Float::Right, i.e., cross-end
                    available_cross_end = available_cross_end.min(float_cross_start);
                }
            }
        }
        (available_cross_start, available_cross_end)
    }

    // +spec:block-formatting-context:d06e6e - clearance computation for clear property on blocks
    // and floats (CSS 2.2 § 9.5.2) +spec:floats:31a3d5 - Clearance computation: places border
    // edge even with bottom outer edge of lowest float to be cleared +spec:floats:f9bef1 -
    // clear property moves element below preceding floats
    /// Returns the main-axis offset needed to be clear of floats of the given type.
    // +spec:block-formatting-context:7f6bde - CSS 2.2 § 9.5.2 clear property: clearance places
    // border edge below bottom outer edge of cleared floats +spec:block-formatting-context:
    // ef493f - clearance computation: places border edge even with bottom outer edge of lowest
    // float to be cleared; inhibits margin collapsing +spec:box-model:b118fe - top border edge
    // must be below bottom outer edge of earlier floats +spec:floats:415066 - Clear property:
    // top border edge below bottom outer edge of cleared floats +spec:floats:7e4ad6 - clear
    // property: element box may not be adjacent to earlier floats; only considers floats in same
    // BFC +spec:floats:32e45d - clear:right causes sibling to flow below right floats
    // +spec:floats:7f417a - clear property prevents content from flowing next to floats
    // +spec:floats:d06304 - clear property moves element below floats, leaving blank space
    // +spec:overflow:1a7aff - clearance calculation (incl. negative clearance) and clear on floats
    // (constraint #10) +spec:positioning:1c2508 - clearance calculation: places border edge
    // even with bottom outer edge of lowest cleared float (CSS 2.2 § 9.5.2) +spec:positioning:
    // fe0912 - clearance computation: places border edge below bottom outer edge of cleared floats
    // (clearance = amount to place border edge even with bottom outer edge of lowest
    // float to be cleared); clearance can be negative per spec example 2
    // +spec:floats:054a1e - Clearance computation: positions border edge below bottom outer edge of
    // cleared floats +spec:floats:cb984c - Clearance can be negative per spec example 2;
    // inhibits margin collapsing
    #[must_use]
    pub fn clearance_offset(
        &self,
        clear: LayoutClear,
        current_main_offset: f32,
        wm: LayoutWritingMode,
    ) -> f32 {
        let mut max_end_offset = 0.0_f32;

        let check_left = clear == LayoutClear::Left || clear == LayoutClear::Both;
        let check_right = clear == LayoutClear::Right || clear == LayoutClear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == LayoutFloat::Left)
                || (check_right && float.kind == LayoutFloat::Right);

            if should_clear_this_float {
                // CSS 2.2 § 9.5.2: "the top border edge of the box be below the bottom outer edge"
                // Outer edge = margin-box boundary (content + padding + border + margin)
                let float_margin_box_end = float.rect.origin.main(wm)
                    + float.rect.size.main(wm)
                    + float.margin.main_end(wm);
                max_end_offset = max_end_offset.max(float_margin_box_end);
            }
        }

        if max_end_offset > current_main_offset {
            max_end_offset
        } else {
            current_main_offset
        }
    }
}

// +spec:floats:167a2c - Float positioning rules (CSS 2.2 § 9.5.1): left/right/none, precise
// placement constraints +spec:floats:6a1769 - Float shortens line boxes, margins never collapse,
// stacking order +spec:floats:15bfd9 - float:right positions element at line-right edge within BFC
// +spec:floats:afc8e2 - Float positioning rules (CSS 2.2 § 9.5 rules 1-8): left/right edge
// containment, earlier-float stacking, outer-top constraints, and "move down" when insufficient
// space
/// Position a float within a BFC, considering existing floats.
/// Returns the `LogicalRect` (margin box) for the float.
// +spec:box-model:db0f02 - Float positioning: line boxes shortened by floats, floats shift down if
// no space, BFC elements must not overlap float margin boxes +spec:containing-block:136e45 - Float
// shifted left/right until outer edge touches containing block edge or another float
// +spec:containing-block:3ebb4e - Content moves below floats when containing block too narrow
// +spec:floats:45fce7 - Float positioning: pulled out of flow, line boxes shortened around float
// +spec:floats:f6c218 - float pulled out of flow, line boxes shorten around it
// +spec:height-calculation:86142a - CSS 2.2 §9.5 float positioning, clearance, and margin
// non-collapsing +spec:width-calculation:761677 - float positioning: content flows around floats,
// line boxes shortened by float presence
pub(super) fn position_float(
    float_ctx: &FloatingContext,
    float_type: LayoutFloat,
    size: LogicalSize,
    margin: &EdgeSizes,
    current_main_offset: f32,
    bfc_cross_size: f32,
    wm: LayoutWritingMode,
) -> LogicalRect {
    // Start at the current main-axis position (Y in horizontal-tb)
    let mut main_start = current_main_offset;

    // Calculate total size including margins
    let total_main = size.main(wm) + margin.main_start(wm) + margin.main_end(wm);
    let total_cross = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);

    // +spec:floats:3d89d8 - shift float downward when not enough horizontal room
    // Find a position where the float fits
    let cross_start = loop {
        let (avail_start, avail_end) = float_ctx.available_line_box_space(
            main_start,
            main_start + total_main,
            bfc_cross_size,
            wm,
        );

        let available_width = avail_end - avail_start;

        if available_width >= total_cross {
            // +spec:floats:449158 - left float positioned at line-left, content flows on right
            // Found space that fits
            if float_type == LayoutFloat::Left {
                // +spec:writing-modes:84bcba - floats positioned at line-left / line-right
                // Position at line-left (avail_start)
                break avail_start + margin.cross_start(wm);
            }
            // Position at line-right (avail_end - size)
            break avail_end - total_cross + margin.cross_start(wm);
        }

        // top is moved lower than earlier float's bottom (outer edge / margin box bottom)
        // Not enough space at this Y, move down past the lowest overlapping float's margin box
        // bottom
        let next_main = float_ctx
            .floats
            .iter()
            .filter(|f| {
                let f_main_start = f.rect.origin.main(wm) - f.margin.main_start(wm);
                let f_main_end = f_main_start
                    + f.rect.size.main(wm)
                    + f.margin.main_start(wm)
                    + f.margin.main_end(wm);
                f_main_end > main_start && f_main_start < main_start + total_main
            })
            .map(|f| f.rect.origin.main(wm) + f.rect.size.main(wm) + f.margin.main_end(wm))
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(next) = next_main {
            main_start = next;
        } else {
            // No overlapping floats found, use current position anyway
            if float_type == LayoutFloat::Left {
                break avail_start + margin.cross_start(wm);
            }
            break avail_end - total_cross + margin.cross_start(wm);
        }
    };

    LogicalRect {
        origin: LogicalPosition::from_main_cross(
            main_start + margin.main_start(wm),
            cross_start,
            wm,
        ),
        size,
    }
}

/// Positions a floated child within the BFC and updates the floating context.
/// This function is fully writing-mode aware.
pub(super) fn position_floated_child(
    _child_index: usize,
    child_margin_box_size: LogicalSize,
    float_type: LayoutFloat,
    constraints: &LayoutConstraints<'_>,
    _bfc_content_box: LogicalRect,
    current_main_offset: f32,
    floating_context: &mut FloatingContext,
) -> Result<LogicalPosition> {
    let wm = constraints.writing_mode;
    let child_main_size = child_margin_box_size.main(wm);
    let child_cross_size = child_margin_box_size.cross(wm);
    let bfc_cross_size = constraints.available_size.cross(wm);
    let mut placement_main_offset = current_main_offset;

    loop {
        // 1. Determine the available cross-axis space at the current
        // `placement_main_offset`.
        let (available_cross_start, available_cross_end) = floating_context
            .available_line_box_space(
                placement_main_offset,
                placement_main_offset + child_main_size,
                bfc_cross_size,
                wm,
            );

        let available_cross_width = available_cross_end - available_cross_start;

        // 2. Check if the new float can fit in the available space.
        if child_cross_size <= available_cross_width {
            // It fits! Determine the final position and add it to the context.
            // +spec:floats:5cfc93 - float:right positions box at cross-end, content flows on left
            let final_cross_pos = match float_type {
                LayoutFloat::Left => available_cross_start,
                // +spec:floats:5cfc93 - float:right positions box at cross-end, content flows on
                // left
                LayoutFloat::Right => available_cross_end - child_cross_size,
                LayoutFloat::None => {
                    return Err(LayoutError::PositioningFailed);
                }
            };
            let final_pos =
                LogicalPosition::from_main_cross(placement_main_offset, final_cross_pos, wm);

            let new_float_box = FloatBox {
                kind: float_type,
                rect: LogicalRect::new(final_pos, child_margin_box_size),
                margin: EdgeSizes::default(), // TODO: Pass actual margin if this function is used
            };
            floating_context.floats.push(new_float_box);
            return Ok(final_pos);
        }
        {
            // +spec:floats:3d89d8 - shift float downward when not enough horizontal room
            // It doesn't fit. We must move the float down past an obstacle.
            // Find the lowest main-axis end of all floats that are blocking
            // the current line.
            let mut next_main_offset = f32::INFINITY;
            for existing_float in &floating_context.floats {
                let float_main_start = existing_float.rect.origin.main(wm);
                let float_main_end = float_main_start + existing_float.rect.size.main(wm);

                // Consider only floats that are above or at the current placement line.
                if placement_main_offset < float_main_end {
                    next_main_offset = next_main_offset.min(float_main_end);
                }
            }

            if next_main_offset.is_infinite() {
                // This indicates an unrecoverable state, e.g., a float wider
                // than the container.
                return Err(LayoutError::PositioningFailed);
            }
            placement_main_offset = next_main_offset;
        }
    }
}

// CSS Property Getters

/// Get the CSS `float` property for a node.
pub(super) fn get_float_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutFloat {
    let Some(id) = dom_id else {
        return LayoutFloat::None;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_float(styled_dom, id, node_state).unwrap_or(LayoutFloat::None)
}

pub(super) fn get_clear_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutClear {
    let Some(id) = dom_id else {
        return LayoutClear::None;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_clear(styled_dom, id, node_state).unwrap_or(LayoutClear::None)
}
