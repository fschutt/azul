//! The entry point: lays a node out with the algorithm of its formatting context.

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

// Entry Point & Dispatcher

/// Main dispatcher for formatting context layout.
///
/// Routes layout to the appropriate formatting context handler based on the node's
/// `formatting_context` property. This is the main entry point for all layout operations.
///
/// # CSS Spec References
/// - CSS 2.2 § 9.4: Formatting contexts
/// - CSS Flexbox § 3: Flex formatting contexts
/// - CSS Grid § 5: Grid formatting contexts
// +spec:block-formatting-context:b04653 - dispatches layout by formatting context type (BFC, IFC,
// Table, Flex, Grid) +spec:block-formatting-context:e46499 - inner display type determines
// formatting context (BFC, IFC, table, flex, grid)
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
/// # Errors
///
/// Returns a `LayoutError` if laying out the formatting context fails.
pub fn layout_formatting_context<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
    float_cache: &mut HashMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    // [g147e az-web-lift DIAG] PURE-CONSTANT entry marker (0x609E0+slot) — fires before any node
    // read, so it reliably shows whether layout_formatting_context is ENTERED for the nested
    // div nodes 1,2.
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark((0x609E0 + (node_index & 7) * 4) as u32, (0xC0DE0042) as u32);
    }
    let node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?;
    // [g147i az-web-lift DIAG] node REFERENCE address (0x60B80+slot) — NOT a field deref, so
    // reliable. If nodes 0,1,2 aren't spaced by sizeof(LayoutNodeHot) → tree.get(index>0)
    // mis-lifts the Vec stride, making nodes 1,2 garbage references (which would explain FC
    // reading garbage + reads destabilizing).
    #[cfg(feature = "web_lift")]
    unsafe {
        crate::az_mark(
            (0x60B80 + (node_index & 7) * 4) as u32,
            ((node as *const _ as usize) as u32) as u32,
        );
    }

    // [g147 az-web-lift] Recompute the IFC decision from the DOM: on the lift, the stored
    // `node.formatting_context` reads GARBAGE for nested inline divs (2026-06-10 re-test WITHOUT
    // this bypass: nodes 1/2 dispatch to the `_` arm + determine_formatting_context_for_display's
    // markers never fire → the FC ASSIGNMENT path itself mis-lifts upstream — NOT fixed by the
    // repr(C,u8) guard, NOT fixed by the leak-gated SP restore; same family as the enum/jump-table
    // devirt class). The styled_dom IS reliable, so a block container whose children are all
    // inline-level establishes an IFC (CSS 2.2 §9.2.1) — semantically valid recomputation, not a
    // hack on top of garbage. web_lift-gated → native untouched. Remove when the FC-assignment
    // mis-lift is root-caused (follow-up: bisect LayoutTreeBuilder's determine_/display match).
    #[cfg(feature = "web_lift")]
    {
        let force_ifc = node.dom_node_id.map_or(false, |dom_id| {
            crate::solver3::layout_tree::has_only_inline_children(ctx.styled_dom, dom_id)
        });
        if force_ifc {
            unsafe {
                crate::az_mark((0x60BA0 + (node_index & 7) * 4) as u32, (0xC0DE1FC0) as u32);
            }
            return layout_ifc(ctx, text_cache, tree, node_index, constraints)
                .map(BfcLayoutResult::from_output);
        }
    }

    // [g147b az-web-lift DIAG] per-node FormattingContext discriminant at layout_formatting_context
    // entry (0x609A0+slot). Pairs with the dispatch-arm marker (0x609C0+slot) inside each match
    // arm: if a text-div's FC reads Inline(2) but the arm marker shows Block(1) → match
    // dispatch mis-lifts; if FC reads Block(1) → tree-construction FC assignment is wrong; if
    // 0x609A0 stays unset for the div node → layout_formatting_context is never called for it
    // (cache-hit short-circuit upstream).
    #[cfg(feature = "web_lift")]
    unsafe {
        let fc_disc = match node.formatting_context {
            FormattingContext::Block { .. } => 1u32,
            FormattingContext::Inline => 2,
            FormattingContext::InlineBlock => 3,
            FormattingContext::Flex => 4,
            FormattingContext::Grid => 5,
            FormattingContext::Table => 6,
            FormattingContext::TableCell => 7,
            FormattingContext::TableCaption => 8,
            _ => 0,
        };
        crate::az_mark(
            (0x609A0 + (node_index & 7) * 4) as u32,
            (fc_disc | 0xC0DE0000) as u32,
        );
    }

    debug_info!(
        ctx,
        "[layout_formatting_context] node_index={}, fc={:?}, available_size={:?}",
        node_index,
        node.formatting_context,
        constraints.available_size
    );

    // +spec:block-formatting-context:06a24f - CSS 2.2 § 9.4: block-level boxes → BFC, inline-level
    // → IFC +spec:block-formatting-context:9428cf - block container can establish both BFC and
    // IFC simultaneously +spec:inline-formatting-context:8bfe73 - display:flow generates inline
    // box (Inline) or block container (Block) based on outer display type
    match node.formatting_context {
        FormattingContext::Block { .. } => {
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0001) as u32);
            }
            let _p = crate::probe::Probe::span("fc_block");
            layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache)
        }
        // +spec:inline-formatting-context:a180ed - IFC establishment: inline-level boxes fragmented
        // into line boxes with baseline alignment
        FormattingContext::Inline => {
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0002) as u32);
            }
            let _p = crate::probe::Probe::span("fc_inline");
            layout_ifc(ctx, text_cache, tree, node_index, constraints)
                .map(BfcLayoutResult::from_output)
        }
        FormattingContext::InlineBlock => {
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0003) as u32);
            }
            // +spec:display-property:1f5ddf - inline-level boxes with non-flow inner display
            // establish new formatting context +spec:inline-formatting-context:1ad004 -
            // atomic inline (inline-block) establishes new formatting context CSS 2.2 §
            // 9.4.1: "inline-blocks... establish new block formatting contexts"
            // +spec:inline-block:8d21f6 - inline-block generates inline-level block container (BFC
            // inside, atomic inline outside) InlineBlock ALWAYS establishes a BFC for
            // its contents. The element itself participates as an atomic inline in its
            // parent's IFC, but its children are laid out in a BFC, not an IFC.
            let _p = crate::probe::Probe::span("fc_inline_block");
            let mut temp_float_cache = HashMap::new();
            layout_bfc(
                ctx,
                tree,
                text_cache,
                node_index,
                constraints,
                &mut temp_float_cache,
            )
        }
        // +spec:table-layout:753687 - CSS 2.2 §17.2 table model: display values map to
        // FormattingContext variants and dispatch table layout
        FormattingContext::Table => {
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0006) as u32);
            }
            layout_table_fc(ctx, tree, text_cache, node_index, constraints)
                .map(BfcLayoutResult::from_output)
        }
        // Table-internal flex items are blockified during tree construction
        // (blockify_flex_item_if_table_internal in layout_tree.rs), so they arrive
        // here as Block, not TableCell etc.
        FormattingContext::Flex | FormattingContext::Grid => {
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0004) as u32);
            }
            let _p = crate::probe::Probe::span("fc_flex_grid");
            layout_flex_grid(ctx, tree, text_cache, node_index, constraints)
        }
        // that are not block boxes, so they establish new BFCs for their contents
        FormattingContext::TableCell | FormattingContext::TableCaption => {
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0007) as u32);
            }
            let mut temp_float_cache = HashMap::new();
            layout_bfc(
                ctx,
                tree,
                text_cache,
                node_index,
                constraints,
                &mut temp_float_cache,
            )
        }
        _ => {
            // [g147g az-web-lift DIAG] read the RAW discriminant byte (offset 0 under repr(C,u8))
            // of the node that fell through to `_`. node 0 won't hit `_`; nodes 1,2
            // (divs) write their disc to 0x60B40+slot. disc=1 ⇒ value IS Inline but the
            // dispatch match mis-branched (match/jump-table lift bug); disc≠1 ⇒
            // tree-construction stored the wrong/garbage FC for the nested div.
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark((0x609C0 + (node_index & 7) * 4) as u32, (0xC0DE0009) as u32);
                let disc: u8 = core::ptr::read_volatile(
                    (&node.formatting_context) as *const FormattingContext as *const u8,
                );
                crate::az_mark(
                    (0x60B40 + (node_index & 7) * 4) as u32,
                    (0xC0DE0000 | (disc as u32)) as u32,
                );
            }
            // Unknown formatting context - fall back to BFC
            let mut temp_float_cache = HashMap::new();
            layout_bfc(
                ctx,
                tree,
                text_cache,
                node_index,
                constraints,
                &mut temp_float_cache,
            )
        }
    }
}
