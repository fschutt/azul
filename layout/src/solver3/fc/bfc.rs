//! Block formatting contexts (CSS 2.2 9.4.1): block flow, margin collapsing, floats, clearance and fragmentation.

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

// Block Formatting Context (CSS 2.2 § 9.4.1)

/// Lays out a Block Formatting Context (BFC).
///
/// This is the corrected, architecturally-sound implementation. It solves the
/// "chicken-and-egg" problem by performing its own two-pass layout:
///
/// 1. **Sizing Pass:** It first iterates through its children and triggers their layout recursively
///    by calling `calculate_layout_for_subtree`. This ensures that the `used_size` property of each
///    child is correctly populated.
///
/// 2. **Positioning Pass:** It then iterates through the children again. Now that each child has a
///    valid size, it can apply the standard block-flow logic: stacking them vertically and
///    advancing a "pen" by each child's outer height.
///
/// # Margin Collapsing Architecture
///
/// CSS 2.1 Section 8.3.1 compliant margin collapsing:
///
/// ```text
/// layout_bfc()
///   ├─ Check parent border/padding blockers
///   ├─ For each child:
///   │   ├─ Check child border/padding blockers
///   │   ├─ is_first_child?
///   │   │   └─ Check parent-child top collapse
///   │   ├─ Sibling collapse?
///   │   │   └─ advance_pen_with_margin_collapse()
///   │   │       └─ collapse_margins(prev_bottom, curr_top)
///   │   ├─ Position child
///   │   ├─ is_empty_block()?
///   │   │   └─ Collapse own top+bottom margins (collapse through)
///   │   └─ Save bottom margin for next sibling
///   └─ Check parent-child bottom collapse
/// ```
///
/// **Collapsing Rules:**
///
/// - Sibling margins: Adjacent vertical margins collapse to max (or sum if mixed signs)
/// - Parent-child: First child's top margin can escape parent (if no border/padding)
/// - Parent-child: Last child's bottom margin can escape parent (if no border/padding/height)
/// - Empty blocks: Top+bottom margins collapse with each other, then with siblings
/// - Blockers: Border, padding, inline content, or new BFC prevents collapsing
///
/// This approach is compliant with the CSS visual formatting model and works within
/// the constraints of the existing layout engine architecture.
// +spec:display-property:f38f52 - BFC handles normal flow, relative positioning offsets, and float
// extraction (CSS 2.2 § 9.8)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose
                                                               // layout/render/parse routine (one
                                                               // branch per case)
/// Does the block container `dom_id` center its block-level children the
/// HTML legacy way (`text-align: -webkit-center`)?
///
/// HTML's rendering section: a `div`, `caption`, `thead`, `tbody`, `tfoot`,
/// `tr`, `td` or `th` with `align="center"` (or `middle`) centers its
/// text AND its block-level descendants, as if they had auto margins; the
/// value inherits like `text-align`. So the newsletter's `<td align=
/// "center"><table width="600">` centers the 600px table in the wide cell.
/// The presentational hint gives such an element `text-align: center`
/// (`azul_core::xml::attributes::presentational_css`); here the inherited
/// half: walking up while the computed `text-align` stays `center`, a
/// legacy-centering element found on the way decides. An element that
/// says something else stops the walk.
pub(super) fn centers_blocks_the_legacy_way(styled_dom: &StyledDom, dom_id: NodeId) -> bool {
    use azul_core::dom::{AttributeType, NodeType};
    use azul_css::props::style::StyleTextAlign;

    let hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    let mut current = Some(dom_id);
    for _ in 0..64 {
        let Some(id) = current else {
            return false;
        };
        let state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
        if get_text_align(styled_dom, id, state).unwrap_or_default() != StyleTextAlign::Center {
            return false;
        }
        let nd = &node_data[id];
        let takes_align = matches!(
            nd.get_node_type(),
            NodeType::Div
                | NodeType::Caption
                | NodeType::THead
                | NodeType::TBody
                | NodeType::TFoot
                | NodeType::Tr
                | NodeType::Td
                | NodeType::Th
        );
        if takes_align
            && nd.attributes().as_ref().iter().any(|a| match a {
                AttributeType::Custom(nv) => {
                    nv.attr_name.as_str().eq_ignore_ascii_case("align")
                        && (nv.value.as_str().trim().eq_ignore_ascii_case("center")
                            || nv.value.as_str().trim().eq_ignore_ascii_case("middle"))
                }
                _ => false,
            })
        {
            return true;
        }
        current = hierarchy.get(id).and_then(|h| h.parent_id());
    }
    false
}

pub(super) fn layout_bfc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
    float_cache: &mut HashMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?
        .clone();
    // +spec:block-formatting-context:4f4ff6 - writing-mode determines block flow direction (main
    // axis) for ordering block-level boxes in BFC
    let writing_mode = constraints.writing_mode;
    let mut output = LayoutOutput::default();
    // `<td align="center">` / `<div align="center">`: block children are
    // centered too (`centers_blocks_the_legacy_way`).
    let legacy_center = node
        .dom_node_id
        .is_some_and(|dom_id| centers_blocks_the_legacy_way(ctx.styled_dom, dom_id));

    debug_info!(
        ctx,
        "\n[layout_bfc] ENTERED for node_index={}, children.len()={}, incoming_bfc_state={}",
        node_index,
        tree.children(node_index).len(),
        constraints.bfc_state.is_some()
    );

    // Initialize FloatingContext for this BFC
    //
    // We always recalculate float positions in this pass, but we'll store them in the cache
    // so that subsequent layout passes (for auto-sizing) have access to the positioned floats
    let mut float_context = FloatingContext::default();

    let (children_containing_block_size, scrollbar_reservation, multicol, flow_cross_size) =
        bfc_prepare(ctx, tree, &node, node_index, constraints, writing_mode);

    // === Pass 1: Pre-compute child sizes (restored two-pass BFC) ===
    //
    // Inspired by Taffy's two-pass approach: first measure, then position.
    //
    // This was removed in commit 1a3e5850 and replaced with a single-pass approach
    // that computed sizes just-in-time during positioning. The single-pass approach
    // caused regression 8e092a2e because positioning decisions (margin collapsing,
    // float clearance, available width after floats) depend on knowing ALL sibling
    // sizes upfront, not just the ones visited so far.
    //
    // With the per-node cache (§9.1-§9.2), the re-added Pass 1 is efficient:
    // - Each child subtree is computed once and stored in NodeCache
    // - Pass 2 positioning reads sizes from tree nodes (used_size set by Pass 1)
    // - When calculate_layout_for_subtree recurses into children after layout_bfc returns, it hits
    //   the per-node cache (same available_size) — O(1) per child.
    //
    // Performance: O(n) for the tree. No double-computation thanks to caching.
    // A child that turns out to need a space-reserving scrollbar was sized in
    // this very pass against the UNreserved width (Pass 1 is the child's real
    // layout). Only the document-level loop can lay it out again, so the need
    // travels up in the result instead of dying in a local: before this, the
    // flag was raised into a temporary here, and `overflow: auto` reserved its
    // gutter only for a node that happened to BE a layout root.
    let mut child_scrollbar_reflow = false;
    {
        let mut temp_positions: super::super::PositionVec = Vec::new();

        // A `::marker` riding the first line is laid out with that line, not
        // as a block of this flow (`is_marker_on_a_line`).
        let bfc_children: Vec<usize> = {
            let shared: &LayoutTree = tree;
            shared
                .children(node_index)
                .iter()
                .copied()
                .filter(|&child| !is_marker_on_a_line(shared, ctx.styled_dom, child))
                .collect()
        };
        // [g147c az-web-lift DIAG] layout_bfc Pass-1 child-sizing loop: record bfc_children.len per
        // parent node (0x60A00+slot). If body shows len=2 but the divs never get the
        // per-child "sized" marker (0x60A40+childslot) below → the loop skips them; if they
        // DO get it but layout_formatting_context (0x609A0) stays unset →
        // calculate(child,ComputeSize) cache-hit (vs 0x60A60 miss-flag in cache.rs).
        #[cfg(feature = "web_lift")]
        unsafe {
            crate::az_mark(
                (0x60A00 + (node_index & 7) * 4) as u32,
                (bfc_children.len() as u32 | 0xC0DE0000) as u32,
            );
        }
        for &child_index in &bfc_children {
            let child_node = tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;
            let child_dom_id = child_node.dom_node_id;

            // +spec:positioning:447b06 - Absolute positioning pulls element out of flow, skip from
            // normal layout +spec:positioning:77a2d2 - Absolutely positioned children
            // are ignored for auto height +spec:positioning:b47ac2 - Only normal flow
            // children taken into account for auto height Skip absolutely/fixed
            // positioned children — they're laid out separately +spec:positioning:
            // c7e5c5 - out-of-flow elements ignored for word boundary / hyphenation
            // +spec:positioning:7dd6d1 - Absolutely positioned boxes are taken out of the normal
            // flow (no impact on later siblings, no margin collapsing)
            let position_type = get_position_type(ctx.styled_dom, child_dom_id);
            if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
                continue;
            }

            // Compute the child's full subtree layout with temporary positions.
            // Position (0,0) is intentionally wrong — Pass 1 only cares about sizing.
            // The correct positions are determined in Pass 2 below.
            // [g147c] this child IS reached by Pass-1 sizing (per-child slot).
            #[cfg(feature = "web_lift")]
            unsafe {
                crate::az_mark(
                    (0x60A40 + (child_index & 7) * 4) as u32,
                    (0xC0DE0000 | (child_index as u32 & 0xffff)) as u32,
                );
            }
            crate::solver3::cache::calculate_layout_for_subtree(
                ctx,
                tree,
                text_cache,
                child_index,
                LogicalPosition::zero(),
                &CBTY::from_flattened_with_width_type(
                    children_containing_block_size,
                    constraints.available_width_type,
                ),
                &mut temp_positions,
                &mut child_scrollbar_reflow,
                float_cache,
                crate::solver3::cache::ComputeMode::ComputeSize,
            )?;
        }
    }

    let (escaped_top_margin, escaped_bottom_margin, fragment_token_out, child_scrollbar_reflow) =
        bfc_place_children(
            ctx,
            tree,
            text_cache,
            node_index,
            constraints,
            float_cache,
            &node,
            writing_mode,
            &mut output,
            legacy_center,
            &mut float_context,
            children_containing_block_size,
            multicol.as_ref(),
            flow_cross_size,
            child_scrollbar_reflow,
        )?;

    Ok(BfcLayoutResult {
        output,
        escaped_top_margin,
        escaped_bottom_margin,
        outgoing_token: fragment_token_out,
        scrollbar_reflow_needed: child_scrollbar_reflow,
        reserved_scrollbar_width: scrollbar_reservation,
    })
}

/// What [`layout_bfc`] decides before its first pass: the containing block its
/// children are sized against (less the scrollbar gutter; one column of a
/// multi-column container), the inline size they are placed across, and their
/// box props re-resolved against that containing block.
///
/// Out of line, so that none of its locals is on the stack while the first pass
/// recurses into the children: a debug build keeps every local of a function in
/// its frame, and one frame of `layout_bfc` is on the stack per nesting level.
#[inline(never)]
fn bfc_prepare<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    node: &LayoutNodeHot,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
    writing_mode: LayoutWritingMode,
) -> (LogicalSize, f32, Option<BlockColumns>, f32) {
    // +spec:containing-block:42b75f - Block element establishes containing block for inline content
    // (IFC) Calculate this node's content-box size for use as containing block for children
    // CSS 2.2 § 10.1: The containing block for in-flow children is formed by the
    // content edge of the parent's content box.
    //
    // We use constraints.available_size directly as this already represents the
    // content-box available to this node (set by parent). For nodes with explicit
    // sizes, used_size contains the border-box which we convert to content-box.
    //
    // NOTE(writing-modes): The containing block size uses physical width/height.
    // In vertical writing modes, the block progression direction is horizontal,
    // so the "available width" for children maps to the physical height of
    // the containing block. The main_pen variable below tracks block progression
    // using logical main-axis coordinates; the WritingModeContext in constraints
    // determines how main/cross map to physical x/y via from_main_cross().
    // +spec:inline-block:17944a - orthogonal flow roots get infinite available inline space here
    // (not yet detected) +spec:inline-block:a60e22 - other layout models pass through infinite
    // inline space to contained block containers
    let mut children_containing_block_size = node.used_size.map_or_else(
        // No used_size yet - use available_size directly (this is already content-box
        // when coming from parent's layout constraints)
        || constraints.available_size,
        |used_size| {
            // Node has used_size (border-box) - convert to content-box.
            // For auto-height containers, the pre-layout `used_size.height` is a
            // placeholder (calculate_used_size_for_node returns 0 for block-level
            // auto-height; apply_content_based_height resolves it after children lay
            // out). In that window, `constraints.available_size.height` holds what
            // `cache::prepare_layout_context` decided the children's percentage
            // heights resolve against: the containing block's height where this box's
            // height is decided by its surroundings, or an indefinite one (INFINITY)
            // where its content decides it (`cache::forwards_containing_block_height`,
            // CSS 2.2 10.5).
            let inner = node.box_props.inner_size(used_size, writing_mode);
            // A percentage height against this box's own indefinite containing
            // block is `auto` as well (CSS 2.2 10.5): its used height is the
            // placeholder too (AzMail's `height: 100%` paper, an inline-block).
            let height_is_auto = tree.warm(LayoutNodeId::new(node_index)).is_none_or(|w| {
                crate::solver3::sizing::height_is_auto_for_children(
                    &node.formatting_context,
                    w.computed_style.height.as_ref(),
                    constraints.containing_block_size.height.is_finite(),
                )
            });
            if height_is_auto {
                LogicalSize::new(inner.width, constraints.available_size.height)
            } else {
                inner
            }
        },
    );

    // +spec:overflow:ffe6f7 - scrollbar space subtracted from containing block per spec §11.1.1
    // Reserve space for vertical scrollbar when appropriate.
    //
    // - overflow: scroll  → ALWAYS reserve (CSS spec: scrollbar always shown)
    // - overflow: auto    → Reserve ONLY when a previous pass already determined a scrollbar is
    //   needed. On the very first pass the node has no scrollbar_info yet, so no space is reserved.
    //   After `compute_scrollbar_info` detects overflow it sets `reflow_needed_for_scrollbars =
    //   true`, triggering a second pass where `node.scrollbar_info.needs_vertical == true` and
    //   space IS reserved. Each pass replaces `scrollbar_info` with the current state; the outer
    //   layout loop's iteration cap handles oscillation safety.
    let scrollbar_reservation = node.dom_node_id.map_or(0.0, |dom_id| {
        let styled_node_state = ctx
            .styled_dom
            .styled_nodes
            .as_container()
            .get(dom_id)
            .map(|s| s.styled_node_state)
            .unwrap_or_default();
        let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state);
        match overflow_y.unwrap_or_default() {
            LayoutOverflow::Scroll => crate::solver3::getters::get_layout_scrollbar_width_px(
                ctx,
                dom_id,
                &styled_node_state,
            ),
            LayoutOverflow::Auto => {
                let already_needs = tree
                    .warm(LayoutNodeId::new(node_index))
                    .and_then(|w| w.scrollbar_info.as_ref())
                    .is_some_and(|s| s.needs_vertical);
                if already_needs {
                    crate::solver3::getters::get_layout_scrollbar_width_px(
                        ctx,
                        dom_id,
                        &styled_node_state,
                    )
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    });

    if scrollbar_reservation > 0.0 {
        children_containing_block_size.width =
            (children_containing_block_size.width - scrollbar_reservation).max(0.0);
    }

    // CSS Multicol 1: a multi-column block container lays its children out
    // as ONE column of the column width - Passes 1 and 2 below, unchanged
    // but for that width - and then cuts that column into its columns
    // (`distribute_into_columns`, after Pass 2).
    let multicol = block_columns(
        ctx,
        tree,
        &node,
        node_index,
        constraints,
        children_containing_block_size,
    );
    if let Some(columns) = &multicol {
        children_containing_block_size.width = columns.geometry.width;
    }
    // The inline size the children are placed across (floats, auto
    // margins, right to left): one column of a multi-column container.
    let flow_cross_size = multicol.as_ref().map_or_else(
        || constraints.available_size.cross(writing_mode),
        |columns| columns.geometry.width,
    );

    // +spec:width-calculation:bef810 - margin percentages resolve against the containing block
    // +spec:box-model:66e123 - ...whose INLINE size is the basis in CSS3 (writing-modes-4 §7.2)
    // The tree-build resolution used the VIEWPORT as a placeholder containing
    // block (the real one is only known here), so every percentage margin or
    // padding in block flow was viewport-based. Re-resolve each child's box
    // props against this BFC's content box before any of them are read; the
    // correct em/rem bases are re-derived from the cascade (the tree build
    // resolved an em against the PARENT's font size - a `zoom: 2; font-size:
    // 10px; padding: 1em` box had 16px of padding, not 20).
    // BEFORE Pass 1: Pass 1 sizes every child with these props
    // (`calculate_layout_for_subtree` -> `prepare_layout_context`) and Pass 2
    // keeps that `used_size`; re-resolved after it, they moved the margins
    // but never the size.
    {
        let root_fs = crate::solver3::layout_tree::get_root_font_size(ctx.styled_dom);
        let flow_children: Vec<usize> = {
            let shared: &LayoutTree = tree;
            shared
                .children(node_index)
                .iter()
                .copied()
                .filter(|&child| !is_marker_on_a_line(shared, ctx.styled_dom, child))
                .collect()
        };
        for &child_index in &flow_children {
            let Some(child_dom_id) = tree
                .get(LayoutNodeId::new(child_index))
                .and_then(|n| n.dom_node_id)
            else {
                continue;
            };
            let efs =
                crate::solver3::layout_tree::get_element_font_size(ctx.styled_dom, child_dom_id);
            tree.resolve_box_props(
                child_index,
                children_containing_block_size,
                ctx.viewport_size,
                efs,
                root_fs,
            );
        }
    }

    (
        children_containing_block_size,
        scrollbar_reservation,
        multicol,
        flow_cross_size,
    )
}

/// [`layout_bfc`] after its first pass: places the children that pass sized
/// (margin collapsing, floats and clearance, fragmentation), cuts them into the
/// container's columns, and resolves the margins that escape the box, its
/// content height and its baseline. Returns the escaped top and bottom margins,
/// the outgoing break token and whether a descendant needs another layout pass
/// for a scrollbar.
///
/// Out of line, so that none of its locals is on the stack while the first pass
/// recurses into the children (see [`bfc_prepare`]).
#[inline(never)]
fn bfc_place_children<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
    float_cache: &mut HashMap<usize, FloatingContext>,
    node: &LayoutNodeHot,
    writing_mode: LayoutWritingMode,
    output: &mut LayoutOutput,
    legacy_center: bool,
    float_context: &mut FloatingContext,
    children_containing_block_size: LogicalSize,
    multicol: Option<&BlockColumns>,
    flow_cross_size: f32,
    mut child_scrollbar_reflow: bool,
) -> Result<(
    Option<f32>,
    Option<f32>,
    Option<crate::solver3::break_token::BreakToken>,
    bool,
)> {
    // +spec:block-formatting-context:98b633 - CSS 2.2 § 9.4.1: boxes laid out vertically, margins
    // collapse === Pass 2: Position children using known sizes ===
    //
    // All children now have used_size set from Pass 1. This pass handles:
    // - Margin collapsing (parent-child + sibling-sibling)
    // - Float positioning and clearance
    // - Normal flow block positioning

    let mut main_pen = 0.0f32;
    let mut max_cross_size = 0.0f32;

    // Track escaped margins separately from content-box height
    // CSS 2.2 § 8.3.1: Escaped margins don't contribute to parent's content-box height,
    // but DO affect sibling positioning within the parent
    let mut total_escaped_top_margin = 0.0f32;
    // Track all inter-sibling margins (collapsed) - these are also not part of content height
    let mut total_sibling_margins = 0.0f32;

    // Margin collapsing state
    let mut last_margin_bottom = 0.0f32;
    let mut is_first_child = true;
    let mut first_child_index: Option<usize> = None;
    let mut last_child_index: Option<usize> = None;

    // Parent's own margins (for escape calculation)
    let node_bp = node.box_props.unpack();
    let parent_margin_top = node_bp.margin.main_start(writing_mode);
    let parent_margin_bottom = node_bp.margin.main_end(writing_mode);

    // margins do not collapse across formatting context boundaries: an independent
    // BFC (float, overflow != visible, display: flex/grid, etc.) isolates its
    // children's margins. The DOM root is NOT a BFC boundary for this purpose —
    // its first child's margin still collapses through it (then gets absorbed at
    // the root, since there's no grandparent to escape to).
    let establishes_own_bfc =
        establishes_new_bfc(ctx, &node, tree.cold(LayoutNodeId::new(node_index)));
    let is_bfc_root = node.parent.is_none() || establishes_own_bfc;

    // parent_has_*_blocker inhibits parent-child margin collapse per CSS 2.2 §8.3.1.
    // An explicit border/padding blocks, and an independent BFC blocks, but the
    // root on its own does not.
    let parent_has_top_blocker =
        establishes_own_bfc || has_margin_collapse_blocker(&node_bp, writing_mode, true);
    let parent_has_bottom_blocker =
        establishes_own_bfc || has_margin_collapse_blocker(&node_bp, writing_mode, false);

    // Track accumulated top margin for first-child escape
    let mut accumulated_top_margin = 0.0f32;
    let mut top_margin_resolved = false;
    // Track if first child's margin escaped (for return value)
    let mut top_margin_escaped = false;

    // Track if we have any actual content (non-empty blocks)
    let mut has_content = false;

    // +spec:display-property:9f6e18 - BFC dispatches normal flow, floats, and relative positioning
    // (CSS 2.2 §9.8)
    // A `::marker` riding the first line is no block of this flow
    // (`is_marker_on_a_line`): it took a line of its own.
    let pos_children: Vec<usize> = {
        let shared: &LayoutTree = tree;
        shared
            .children(node_index)
            .iter()
            .copied()
            .filter(|&child| !is_marker_on_a_line(shared, ctx.styled_dom, child))
            .collect()
    };
    // (Each child's box props were re-resolved against this BFC's content
    // box before Pass 1 - see there.)

    // K30b fragmentation state (inert when `constraints.fragmentainer` is
    // None — the continuous path). Resume = skip every finished sibling
    // before the token's first unfinished child WITH ZERO side effects
    // (before any margin/pen/float bookkeeping); break = emit the
    // unfinished tail as the outgoing token and stop consuming children.
    let fragment_resume_from: Option<usize> = constraints
        .fragmentainer
        .as_ref()
        .and_then(|fs| fs.resume)
        .and_then(crate::solver3::break_token::resume_plan)
        .map(|p| p.first_unfinished);
    // K30b part 2: per-child resume tokens (ResumeIn entries). A child in
    // this map continues from ITS token in a re-laid subtree; BreakBefore
    // children (and Inline tokens, v1) lay out from scratch.
    let fragment_resume_tokens: BTreeMap<usize, &crate::solver3::break_token::BreakToken> =
        constraints
            .fragmentainer
            .as_ref()
            .and_then(|fs| fs.resume)
            .map(|tok| {
                tok.children
                    .iter()
                    .filter_map(|e| match e {
                        crate::solver3::break_token::ChildBreakEntry::ResumeIn { child, token } => {
                            Some((*child, &**token))
                        }
                        crate::solver3::break_token::ChildBreakEntry::BreakBefore { .. } => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
    let mut fragment_resume_reached = fragment_resume_from.is_none();
    let mut fragment_placed_content = false;
    let mut fragment_token_out: Option<crate::solver3::break_token::BreakToken> = None;
    // css-break-3 §5.2: margins adjoining an UNFORCED break truncate; a
    // FORCED break (break-before: page / <pagebreak/>) keeps them. Pending
    // until the first resumed child places.
    let mut fragment_truncate_first_margin: bool = constraints
        .fragmentainer
        .as_ref()
        .and_then(|fs| fs.resume)
        .and_then(|tok| tok.children.first())
        .is_some_and(|entry| match entry {
            crate::solver3::break_token::ChildBreakEntry::BreakBefore { forced, .. } => !*forced,
            // A ResumeIn child CONTINUES mid-box: its top decoration/margin
            // belongs to its first fragment — nothing to apply here anyway.
            crate::solver3::break_token::ChildBreakEntry::ResumeIn { .. } => true,
        });

    // Fragment passes mark every child they DON'T place with a sentinel
    // relative position: the positioning descent then computes far-negative
    // absolutes for the whole stale subtree and the display-list builder's
    // unassigned-position guard drops the items (the sentinel survives
    // offset arithmetic by magnitude — that is why UNASSIGNED_POSITION_LIMIT
    // is f32::MIN / 2). Without this, skipped/broken children reappear on
    // every page at their CONTINUOUS positions.
    let fragment_pass = constraints.fragmentainer.is_some();
    macro_rules! clear_fragment_pos {
        ($child:expr) => {
            if fragment_pass {
                if let Some(w) = tree.warm_mut($child) {
                    w.relative_position = Some(LogicalPosition::new(f32::MIN, f32::MIN));
                }
            }
        };
    }

    // The block size of a `::marker` laid out at the content start with no
    // line box to ride (see the loop): the content box is at least this tall.
    let mut marker_without_line_main = 0.0f32;
    for &child_index in &pos_children {
        // A token emitted while PLACING the previous child (break-descend /
        // resumed-child continuation) stops sibling consumption here — the
        // rest of the tail is not on this page.
        if fragment_token_out.is_some() {
            clear_fragment_pos!(LayoutNodeId::new(child_index));
            continue;
        }
        if !fragment_resume_reached {
            if Some(child_index) == fragment_resume_from {
                fragment_resume_reached = true;
            } else {
                // Finished on an earlier fragmentainer.
                clear_fragment_pos!(LayoutNodeId::new(child_index));
                continue;
            }
        }

        // K31: snapshot the pen BEFORE this child contributes anything —
        // a break emitted at this child rolls the pen back here (the margin
        // adjoining an unforced break truncates on BOTH sides; the pen had
        // already advanced past the child's collapsed top margin when the
        // fit check runs).
        let fragment_pen_at_child = main_pen;

        // K31 forced breaks: `break-before: page` (incl. the UA rule on
        // `<pagebreak/>` nodes — which are EMPTY blocks and short-circuit
        // before the fit check, hence this sits at the loop top). Only
        // once this fragmentainer holds content (a forced break at the top
        // of a fresh page is vacuously satisfied, else every page would
        // re-break forever).
        if fragment_pass
            && fragment_placed_content
            && fragment_token_out.is_none()
            && crate::solver3::getters::get_break_before(
                ctx.styled_dom,
                tree.get(LayoutNodeId::new(child_index))
                    .and_then(|n| n.dom_node_id),
            ) != azul_css::props::layout::fragmentation::PageBreak::Auto
        {
            let later: Vec<usize> = pos_children
                .iter()
                .copied()
                .skip_while(|&c| c != child_index)
                .skip(1)
                .filter(|&c| {
                    let pt = get_position_type(
                        ctx.styled_dom,
                        tree.get(LayoutNodeId::new(c)).and_then(|n| n.dom_node_id),
                    );
                    pt != LayoutPosition::Absolute && pt != LayoutPosition::Fixed
                })
                .collect();
            let mut children =
                alloc::vec![crate::solver3::break_token::ChildBreakEntry::BreakBefore {
                    child: child_index,
                    forced: true,
                }];
            children.extend(later.into_iter().map(|child| {
                crate::solver3::break_token::ChildBreakEntry::BreakBefore {
                    child,
                    forced: false,
                }
            }));
            fragment_token_out = Some(crate::solver3::break_token::BreakToken::Block(
                crate::solver3::break_token::BlockBreakToken {
                    node: node_index,
                    consumed_block_size: main_pen,
                    children,
                    generation: 0,
                },
            ));
            clear_fragment_pos!(LayoutNodeId::new(child_index));
            continue;
        }

        let mut fragment_child_resumed = false;
        // K30b part 2, RESUME arm: this child carries a Block resume token —
        // RE-LAY its subtree from that token inside the remaining extent
        // (its used_size then reflects only the remaining content). If it
        // STILL does not finish, emit its continuation and stop after
        // placing it. Inline tokens re-lay from scratch in v1.
        if let Some(fs) = constraints.fragmentainer.as_ref().copied() {
            if let Some(crate::solver3::break_token::BreakToken::Block(child_tok)) =
                fragment_resume_tokens.get(&child_index).copied()
            {
                let child_space = FragmentainerSpace {
                    remaining_block_extent: (fs.remaining_block_extent - main_pen).max(0.0),
                    next_fragmentainer_extent: fs.next_fragmentainer_extent,
                    is_first: false,
                    resume: Some(child_tok),
                };
                let mut child_out: Option<crate::solver3::break_token::BreakToken> = None;
                let mut tmp_positions: super::super::PositionVec = Vec::new();
                let mut tmp_scrollbars = false;
                crate::solver3::cache::calculate_layout_for_subtree_fragment(
                    ctx,
                    tree,
                    text_cache,
                    child_index,
                    LogicalPosition::zero(),
                    &CBTY::from_flattened_with_width_type(
                        children_containing_block_size,
                        constraints.available_width_type,
                    ),
                    &mut tmp_positions,
                    &mut tmp_scrollbars,
                    float_cache,
                    crate::solver3::cache::ComputeMode::ComputeSize,
                    Some(child_space),
                    Some(&mut child_out),
                )?;
                fragment_child_resumed = true;
                if let Some(cont) = child_out {
                    let later: Vec<usize> = pos_children
                        .iter()
                        .copied()
                        .skip_while(|&c| c != child_index)
                        .skip(1)
                        .filter(|&c| {
                            let pt = get_position_type(
                                ctx.styled_dom,
                                tree.get(LayoutNodeId::new(c)).and_then(|n| n.dom_node_id),
                            );
                            pt != LayoutPosition::Absolute && pt != LayoutPosition::Fixed
                        })
                        .collect();
                    let mut children =
                        alloc::vec![crate::solver3::break_token::ChildBreakEntry::ResumeIn {
                            child: child_index,
                            token: Box::new(cont),
                        }];
                    children.extend(later.into_iter().map(|child| {
                        crate::solver3::break_token::ChildBreakEntry::BreakBefore {
                            child,
                            forced: false,
                        }
                    }));
                    fragment_token_out = Some(crate::solver3::break_token::BreakToken::Block(
                        crate::solver3::break_token::BlockBreakToken {
                            node: node_index,
                            consumed_block_size: main_pen,
                            children,
                            generation: 0,
                        },
                    ));
                    // fall through: PLACE the fitted part of this child;
                    // the loop-top guard stops the following siblings.
                }
            }
        }

        let child_node = tree
            .get(LayoutNodeId::new(child_index))
            .ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // A `::marker` still in this flow has no line box to ride (the ones
        // on a line were filtered out of `pos_children`; `marker_line_host`
        // is None: an empty item, a first block with no line in it, a
        // table). Chrome lays it out at the item's content start, OUT of the
        // flow - the first block starts where it starts - and the item is
        // as tall as the taller of the two (LayoutNG's
        // `PositionListMarkerWithoutLineBoxes`: "3 out of 4 impls" let it
        // extend the block size, csswg-drafts#2418). It took a line of its
        // own above the first block: `<li><div style="height: 50px">` was
        // a line too tall. Checked before the position / float tests: the
        // marker carries its LIST ITEM's DOM node, whose `position` and
        // `float` are not the marker's. An OUTSIDE marker only: an inside
        // one is an inline box of the item's flow - with no line of the
        // item's own to share it is one in an anonymous block, a line before
        // the first block (CSS 2.2 12.5.1; it hung over that block here).
        let child_is_marker = is_marker_box(tree, child_index);
        if child_is_marker && is_outside_marker(tree, ctx.styled_dom, child_index) {
            marker_without_line_main = marker_without_line_main.max(
                child_node
                    .used_size
                    .map_or(0.0, |size| size.main(writing_mode)),
            );
            output.positions.insert(
                child_index,
                LogicalPosition::from_main_cross(0.0, 0.0, writing_mode),
            );
            continue;
        }

        // +spec:floats:2cec1b - 'position' and 'float' determine the positioning algorithm
        // +spec:positioning:dccad6 - floats only apply to non-absolutely-positioned boxes
        // (An inside marker in this flow carries its ITEM's DOM node: the
        // item's `position` and `float` are not the marker's.)
        let position_type = if child_is_marker {
            LayoutPosition::Static
        } else {
            get_position_type(ctx.styled_dom, child_dom_id)
        };
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            // Its STATIC position (CSS 2.2 10.3.7 / 10.6.4): the border-box
            // origin it would have had as a block of this flow - after the
            // blocks placed so far and the margin they leave, at its own
            // start margins - which an `auto` inset resolves to
            // (`positioning`). It takes no room: the pen stays. It was
            // never recorded, and every absolute box sat at its parent's
            // content-box origin, on top of the blocks before it.
            let child_margin = child_node.box_props.unpack().margin;
            let static_main = main_pen
                + collapse_margins(last_margin_bottom, child_margin.main_start(writing_mode));
            output.static_positions.insert(
                child_index,
                LogicalPosition::from_main_cross(
                    static_main,
                    child_margin.cross_start(writing_mode),
                    writing_mode,
                ),
            );
            continue;
        }

        // +spec:floats:2cec1b - float property determines positioning algorithm (float path)
        // +spec:floats:f6c0b2 - floats only processed in BFC; other formatting contexts (flex/grid)
        // inhibit floating Check if this child is a float - if so, position it at current
        // main_pen
        if let Some(node_id) = child_dom_id.filter(|_| !child_is_marker) {
            let float_type = get_float_property(ctx.styled_dom, Some(node_id));

            if float_type != LayoutFloat::None {
                // Calculate float size just-in-time if not already computed
                let float_size = if let Some(size) = child_node.used_size {
                    size
                } else {
                    let intrinsic = tree
                        .warm(LayoutNodeId::new(child_index))
                        .and_then(|w| w.intrinsic_sizes)
                        .unwrap_or_default();
                    let child_bp = child_node.box_props.unpack();
                    let computed_size = crate::solver3::sizing::calculate_used_size_for_node(
                        ctx.styled_dom,
                        child_dom_id,
                        &CBTY::from_flattened_with_width_type(
                            children_containing_block_size,
                            constraints.available_width_type,
                        ),
                        intrinsic,
                        &child_bp,
                        &ctx.viewport_size,
                    )?;
                    if let Some(node_mut) = tree.get_mut(LayoutNodeId::new(child_index)) {
                        node_mut.used_size = Some(computed_size);
                    }
                    computed_size
                };
                // Re-borrow after potential mutation
                let child_node = tree
                    .get(LayoutNodeId::new(child_index))
                    .ok_or(LayoutError::InvalidTree)?;
                let child_bp2 = child_node.box_props.unpack();
                let float_margin = &child_bp2.margin;

                // +spec:floats:d0d163 - clear on floats adds constraint #10: float top below
                // cleared floats' bottom +spec:floats:7adb9d - Clear on floats:
                // constraint #10, top outer edge must be below earlier cleared floats
                let float_clear = get_clear_property(ctx.styled_dom, Some(node_id));
                let float_y = if float_clear == LayoutClear::None {
                    // +spec:floats:ef96cb - Float margins never collapse with adjacent margins
                    // CSS 2.2 § 9.5: Float margins don't collapse with any other margins.
                    main_pen + last_margin_bottom
                } else {
                    float_context.clearance_offset(
                        float_clear,
                        main_pen + last_margin_bottom,
                        writing_mode,
                    )
                };

                debug_info!(
                    ctx,
                    "[layout_bfc] Positioning float: index={}, type={:?}, size={:?}, at Y={} \
                     (main_pen={} + last_margin={})",
                    child_index,
                    float_type,
                    float_size,
                    float_y,
                    main_pen,
                    last_margin_bottom
                );

                // Position the float at the CURRENT main_pen + last margin (respects DOM order!)
                let float_rect = position_float(
                    &float_context,
                    float_type,
                    float_size,
                    float_margin,
                    // Include last_margin_bottom since float margins don't collapse!
                    float_y,
                    flow_cross_size,
                    writing_mode,
                );

                debug_info!(ctx, "[layout_bfc] Float positioned at: {:?}", float_rect);

                // K32: floats participate in fragmentation ATOMICALLY (an
                // anchored image never splits — the Word model). A float
                // that does not fit the remaining extent moves WHOLE to the
                // next fragmentainer via the unfinished tail; its exclusion
                // geometry then belongs to THAT page only (each layout_bfc
                // call seeds a fresh FloatingContext, so nothing leaks
                // across fragmentainers by construction).
                if let Some(fs) = constraints.fragmentainer.as_ref() {
                    let float_bottom =
                        float_rect.origin.main(writing_mode) + float_rect.size.main(writing_mode);
                    let fits = float_bottom <= fs.remaining_block_extent + 0.01;
                    if !fits && fragment_placed_content {
                        let later: Vec<usize> = pos_children
                            .iter()
                            .copied()
                            .skip_while(|&c| c != child_index)
                            .skip(1)
                            .filter(|&c| {
                                let pt = get_position_type(
                                    ctx.styled_dom,
                                    tree.get(LayoutNodeId::new(c)).and_then(|n| n.dom_node_id),
                                );
                                pt != LayoutPosition::Absolute && pt != LayoutPosition::Fixed
                            })
                            .collect();
                        main_pen = fragment_pen_at_child;
                        fragment_token_out = Some(crate::solver3::break_token::tail_token(
                            node_index,
                            main_pen,
                            child_index,
                            later.into_iter(),
                        ));
                        clear_fragment_pos!(LayoutNodeId::new(child_index));
                        continue;
                    }
                    // First content overflowing every page: monolith-place
                    // (falls through), same rule as atomic blocks.
                }

                // Add to float context BEFORE positioning next element
                float_context.add_float(float_type, float_rect, *float_margin);

                // Store position in output
                output.positions.insert(child_index, float_rect.origin);

                debug_info!(
                    ctx,
                    "[layout_bfc] *** FLOAT POSITIONED: child={}, main_pen={} (unchanged - floats \
                     don't advance pen)",
                    child_index,
                    main_pen
                );

                if constraints.fragmentainer.is_some() {
                    fragment_placed_content = true;
                }

                // Floats are taken out of normal flow - DON'T advance main_pen
                // Continue to next child
                continue;
            }
        }

        // Floats `continue` above; everything reaching here is normal-flow
        // (non-float) content.

        // From here: normal flow (non-float) children only

        // Track first and last in-flow children for parent-child collapse
        if first_child_index.is_none() {
            first_child_index = Some(child_index);
        }
        last_child_index = Some(child_index);

        // Calculate child's used_size just-in-time if not already computed
        // This replaces the old "Pass 1" that recursively laid out grandchildren with wrong
        // positions
        let child_size = if let Some(size) = child_node.used_size {
            size
        } else {
            // Calculate size without recursive layout
            let intrinsic = tree
                .warm(LayoutNodeId::new(child_index))
                .and_then(|w| w.intrinsic_sizes)
                .unwrap_or_default();
            let child_used_size = crate::solver3::sizing::calculate_used_size_for_node(
                ctx.styled_dom,
                child_dom_id,
                &CBTY::from_flattened_with_width_type(
                    children_containing_block_size,
                    constraints.available_width_type,
                ),
                intrinsic,
                &child_node.box_props.unpack(),
                &ctx.viewport_size,
            )?;
            // Update the node with computed size (we need to re-borrow mutably)
            if let Some(node_mut) = tree.get_mut(LayoutNodeId::new(child_index)) {
                node_mut.used_size = Some(child_used_size);
            }
            child_used_size
        };
        // Re-borrow child_node after potential mutation
        let child_node = tree
            .get(LayoutNodeId::new(child_index))
            .ok_or(LayoutError::InvalidTree)?;
        let child_bp = child_node.box_props.unpack();
        let child_margin = &child_bp.margin;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} margin from box_props: top={}, right={}, bottom={}, left={}",
            child_index,
            child_margin.top,
            child_margin.right,
            child_margin.bottom,
            child_margin.left
        );

        // +spec:block-formatting-context:0f802c - margins use containing block's writing mode for
        // collapsing/auto expansion in orthogonal flows
        let child_own_margin_top = child_margin.main_start(writing_mode);
        let child_own_margin_bottom = child_margin.main_end(writing_mode);

        // CSS 2.2 § 8.3.1: If a child has no top blocker (no padding/border) and its
        // own BFC layout produced an escaped_top_margin, that margin represents the
        // collapsed value of (child's margin, child's first child's margin, ...).
        // Use it for sibling collapse instead of the child's own margin.
        let child_escaped_top = if has_margin_collapse_blocker(&child_bp, writing_mode, true) {
            None
        } else {
            tree.warm(LayoutNodeId::new(child_index))
                .and_then(|w| w.escaped_top_margin)
        };
        let child_escaped_bottom = if has_margin_collapse_blocker(&child_bp, writing_mode, false) {
            None
        } else {
            tree.warm(LayoutNodeId::new(child_index))
                .and_then(|w| w.escaped_bottom_margin)
        };

        let mut child_margin_top = child_escaped_top.unwrap_or(child_own_margin_top);
        let child_margin_bottom = child_escaped_bottom.unwrap_or(child_own_margin_bottom);
        // K31: the first child placed after an UNFORCED fragmentation break
        // starts flush at the fragmentainer top (css-break-3 §5.2).
        if fragment_truncate_first_margin && !fragment_placed_content {
            child_margin_top = 0.0;
            fragment_truncate_first_margin = false;
        }

        debug_info!(
            ctx,
            "[layout_bfc] Child {} final margins: margin_top={}, margin_bottom={}",
            child_index,
            child_margin_top,
            child_margin_bottom
        );

        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker = has_margin_collapse_blocker(&child_bp, writing_mode, true);
        let child_has_bottom_blocker = has_margin_collapse_blocker(&child_bp, writing_mode, false);

        // +spec:floats:dc195a - Clear property only applies to block-level elements (CSS 2.2 §
        // 9.5.2) Check for clear property FIRST - clearance affects whether element is
        // considered empty CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"
        // An element with clearance is NOT empty even if it has no content
        let child_clear = if let Some(node_id) = child_dom_id {
            get_clear_property(ctx.styled_dom, Some(node_id))
        } else {
            LayoutClear::None
        };
        debug_info!(
            ctx,
            "[layout_bfc] Child {} clear property: {:?}",
            child_index,
            child_clear
        );

        // PHASE 1: Empty Block Detection & Self-Collapse
        let is_empty = is_empty_block(tree, child_index);

        // Handle empty blocks FIRST (they collapse through and don't participate in layout)
        // EXCEPTION: Elements with clear property are NOT skipped even if empty!
        // CSS 2.2 § 9.5.2: Clear property affects positioning even for empty elements
        if is_empty
            && !child_has_top_blocker
            && !child_has_bottom_blocker
            && child_clear == LayoutClear::None
        {
            // Empty block: collapse its own top and bottom margins FIRST
            let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);

            // Then collapse with previous margin (sibling or parent)
            let seam_main;
            if is_first_child {
                is_first_child = false;
                // Empty first child: its collapsed margin can escape with parent's
                if parent_has_top_blocker {
                    // Parent has a top blocker (padding / border): the empty
                    // child's collapsed-through margin is ONE margin inside the
                    // parent's content box. It is CARRIED in `last_margin_bottom`
                    // — the next sibling's top margin collapses with it, or the
                    // parent's bottom blocker adds it once at the end — and the
                    // pen does not move. Advancing the pen by it here AND
                    // carrying it counted it twice: `<div style="padding:4px">
                    // <p></p></div>` came out 4 + 13 + 13 + 4 instead of
                    // 4 + 13 + 4 (the AzWidgets placeholder sat 13 px too low).
                    // The parent's own top margin lives in the GRANDPARENT's
                    // coordinate space and is never added here (see the
                    // non-empty blocked case below).
                    top_margin_resolved = true;
                    accumulated_top_margin = 0.0;
                    // The empty block's border edges sit after its collapsed
                    // top margin (Chrome: offsetTop = padding + margin).
                    seam_main = main_pen + self_collapsed;
                } else {
                    accumulated_top_margin = collapse_margins(parent_margin_top, self_collapsed);
                    seam_main = main_pen;
                }
                last_margin_bottom = self_collapsed;
            } else {
                // Empty sibling: collapse with previous sibling's bottom margin
                last_margin_bottom = collapse_margins(last_margin_bottom, self_collapsed);
                seam_main = main_pen + last_margin_bottom;
            }

            // A collapsed-through empty block still HAS a position — CSS 2.2
            // §8.3.1 makes its top and bottom border edges coincide inside the
            // collapsed seam, it does not remove the box. Omitting it from
            // `output.positions` left the node AND its whole subtree at the
            // POSITION_UNSET sentinel, so their Border/HitTestArea items were
            // emitted at (f32::MIN, f32::MIN) and dropped by the display-list
            // guard (the MicrophoneWidget/CameraWidget invisible-div pattern:
            // an empty dataset-carrier div silently lost its hit area).
            output.positions.insert(
                child_index,
                LogicalPosition::from_main_cross(
                    seam_main,
                    child_bp.margin.cross_start(writing_mode),
                    writing_mode,
                ),
            );

            // Skip pen advance (empty has no visual presence)
            continue;
        }

        // From here on: non-empty blocks only (or empty blocks with clear property)

        // Apply clearance if needed
        // +spec:floats:148ee6 - clear:left pushes element below float; clearance added above top
        // margin CSS 2.2 § 9.5.2: Clearance inhibits margin collapsing.
        //
        // Per CSS 2.2 § 9.5.2, the clearance computation works as follows:
        // 1. Compute the "hypothetical position" — where the border edge would be with normal
        //    margin collapsing (as if clear:none).
        // 2. If the hypothetical position is NOT past the relevant floats, clearance is introduced
        //    and the border edge is placed at float bottom.
        // 3. The final border edge = max(float_bottom, hypothetical_position).
        //
        // This means child_margin_top is already accounted for in the hypothetical
        // position and must NOT be added again after clearance positions main_pen.
        let clearance_applied = if child_clear == LayoutClear::None {
            false
        } else {
            let hypothetical = main_pen + collapse_margins(last_margin_bottom, child_margin_top);
            let cleared_position =
                float_context.clearance_offset(child_clear, hypothetical, writing_mode);
            debug_info!(
                ctx,
                "[layout_bfc] Child {} clearance check: cleared_position={}, hypothetical={} \
                 (main_pen={} + collapse({}, {}))",
                child_index,
                cleared_position,
                hypothetical,
                main_pen,
                last_margin_bottom,
                child_margin_top
            );
            if cleared_position > hypothetical {
                debug_info!(
                    ctx,
                    "[layout_bfc] Applying clearance: child={}, clear={:?}, old_pen={}, new_pen={}",
                    child_index,
                    child_clear,
                    main_pen,
                    cleared_position
                );
                main_pen = cleared_position;
                true // Signal that clearance was applied
            } else {
                false
            }
        };

        // PHASE 2: Parent-Child Top Margin Escape (First Child)
        //
        // CSS 2.2 § 8.3.1: "The top margin of a box is adjacent to the top margin of its first
        // in-flow child if the box has no top border, no top padding, and the child has no
        // clearance." CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

        if is_first_child {
            is_first_child = false;

            // Clearance prevents collapse (acts as invisible blocker)
            if clearance_applied {
                // Clearance inhibits all margin collapsing for this element
                // The clearance has already positioned main_pen at the correct
                // border-edge position (= max(float_bottom, hypothetical)).
                // The hypothetical already includes child_margin_top via
                // collapse_margins, so we must NOT add it again here.
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} with CLEARANCE: no collapse, child_margin={}, \
                     main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else if !parent_has_top_blocker {
                // Margin Escape Case
                //
                // CSS 2.2 § 8.3.1: "The top margin of an in-flow block element collapses with
                // its first in-flow block-level child's top margin if the element has no top
                // border, no top padding, and the child has no clearance."
                //
                // When margins collapse, they "escape" upward through the parent to be resolved
                // in the grandparent's coordinate space. This is critical for understanding the
                // coordinate system separation:
                //
                // Example:
                // <body padding=20>
                //  <div margin=0>
                //      <div margin=30></div>
                //  </div>
                // </body>
                //
                //   - Middle div (our parent) has no padding → margins can escape
                //   - Inner div's 30px margin collapses with middle div's 0px margin = 30px
                //   - This 30px margin "escapes" to be handled by body's BFC
                //   - Body positions middle div at Y=30 (relative to body's content-box)
                //   - Middle div's content-box height does NOT include the escaped 30px
                //   - Inner div is positioned at Y=0 in middle div's content-box
                //
                // **NOTE**: This is a subtle but critical distinction in coordinate systems:
                //
                //   - Parent's margin belongs to grandparent's coordinate space
                //   - Child's margin (when escaped) also belongs to grandparent's coordinate space
                //   - They collapse BEFORE entering this BFC's coordinate space
                //   - We return the collapsed margin so grandparent can position parent correctly
                //
                // **NOTE**: Child's own blocker status (padding/border) is IRRELEVANT for
                // parent-child  collapse. The child may have padding that prevents
                // collapse with ITS OWN  children, but this doesn't prevent its
                // margin from escaping  through its parent.
                //
                // **NOTE**: Previously, we incorrectly added parent_margin_top to main_pen in
                //  the blocked case, which double-counted the margin by mixing
                //  coordinate systems. The parent's margin is NEVER in our (the
                //  parent's content-box) coordinate system!
                //
                // We collapse the parent's margin with the child's margin.
                // This combined margin is what "escapes" to the grandparent.
                // The grandparent uses this to position the parent.
                //
                // Effectively, we are saying "The parent starts here, but its effective
                // top margin is now max(parent_margin, child_margin)".

                accumulated_top_margin = collapse_margins(parent_margin_top, child_margin_top);
                top_margin_resolved = true;
                top_margin_escaped = true;

                // Track escaped margin so it gets subtracted from content-box height
                // The escaped margin is NOT part of our content-box - it belongs to our
                // parent's parent
                total_escaped_top_margin = accumulated_top_margin;

                // Position child at pen (no margin applied - it escaped!)
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} margin ESCAPES: parent_margin={}, \
                     child_margin={}, collapsed={}, total_escaped={}",
                    child_index,
                    parent_margin_top,
                    child_margin_top,
                    accumulated_top_margin,
                    total_escaped_top_margin
                );
            } else {
                // Margin Blocked Case
                //
                // CSS 2.2 § 8.3.1: "no top padding and no top border" required for collapse.
                // When padding or border exists, margins do NOT collapse and exist in different
                // coordinate spaces.
                //
                // CRITICAL COORDINATE SYSTEM SEPARATION:
                //
                //   This is where the architecture becomes subtle. When layout_bfc() is called:
                //   1. We are INSIDE the parent's content-box coordinate space (main_pen starts at
                //      0)
                //   2. The parent's own margin was ALREADY RESOLVED by the grandparent's BFC
                //   3. The parent's margin is in the grandparent's coordinate space, not ours
                //   4. We NEVER reference the parent's margin in this BFC - it's outside our scope
                //
                // Example:
                //
                // <body padding=20>
                //   <div margin=30 padding=20>
                //      <div margin=30></div>
                //   </div>
                // </body>
                //
                //   - Middle div has padding=20 → blocker exists, margins don't collapse
                //   - Body's BFC positions middle div at Y=30 (middle div's margin, in body's
                //     space)
                //   - Middle div's BFC starts at its content-box (after the padding)
                //   - main_pen=0 at the top of middle div's content-box
                //   - Inner div has margin=30 → we add 30 to main_pen (in OUR coordinate space)
                //   - Inner div positioned at Y=30 (relative to middle div's content-box)
                //   - Absolute position: 20 (body padding) + 30 (middle margin) + 20 (middle
                //     padding) + 30 (inner margin) = 100px
                //
                // **NOTE**: Previous code incorrectly added parent_margin_top to main_pen here:
                //
                //     - main_pen += parent_margin_top;  // WRONG! Mixes coordinate systems
                //     - main_pen += child_margin_top;
                //
                //   This caused the "double margin" bug where margins were applied twice:
                //
                //   - Once by grandparent positioning parent (correct)
                //   - Again inside parent's BFC (INCORRECT - wrong coordinate system)
                //
                //   The parent's margin belongs to GRANDPARENT's coordinate space and was already
                //   used to position the parent. Adding it again here is like adding feet to
                //   meters.
                //
                //   We ONLY add the child's margin in our (parent's content-box) coordinate space.
                //   The parent's margin is irrelevant to us - it's outside our scope.

                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} BLOCKED: parent_has_blocker={}, advanced by \
                     child_margin={}, main_pen={}",
                    child_index,
                    parent_has_top_blocker,
                    child_margin_top,
                    main_pen
                );
            }
        } else {
            // Not first child: handle sibling collapse
            // CSS 2.2 § 8.3.1 Rule 1: "Vertical margins of adjacent block boxes in the normal flow
            // collapse" CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

            // Resolve accumulated top margin if not yet done (for parent's first in-flow child)
            if !top_margin_resolved {
                main_pen += accumulated_top_margin;
                top_margin_resolved = true;
                debug_info!(
                    ctx,
                    "[layout_bfc] RESOLVED top margin for node {} at sibling {}: accumulated={}, \
                     main_pen={}",
                    node_index,
                    child_index,
                    accumulated_top_margin,
                    main_pen
                );
            }

            if clearance_applied {
                // Clearance has already positioned main_pen at the correct
                // border-edge = max(float_bottom, hypothetical). The hypothetical
                // already includes collapse_margins(last_margin_bottom, child_margin_top),
                // so we must NOT add child_margin_top again here.
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} with CLEARANCE: no collapse with sibling, \
                     child_margin_top={}, main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else {
                // Sibling Margin Collapse
                //
                // CSS 2.2 § 8.3.1: "Vertical margins of adjacent block boxes in the normal
                // flow collapse." The collapsed margin is the maximum of the two margins.
                //
                // IMPORTANT: Sibling margins ARE part of the parent's content-box height!
                //
                // Unlike escaped margins (which belong to grandparent's space), sibling margins
                // are the space BETWEEN children within our content-box.
                //
                // Example:
                //
                // <div>
                //  <div margin-bottom=30></div>
                //  <div margin-top=40></div>
                // </div>
                //
                //   - First child ends at Y=100 (including its content + margins)
                //   - Collapsed margin = max(30, 40) = 40px
                //   - Second child starts at Y=140 (100 + 40)
                //   - Parent's content-box height includes this 40px gap
                //
                // We track total_sibling_margins for debugging, but NOTE: we do **not**
                // subtract these from content-box height! They are part of the layout space.
                //
                // Previously we subtracted total_sibling_margins from content-box height:
                //
                //   content_box_height = main_pen - total_escaped_top_margin -
                // total_sibling_margins;
                //
                // This was wrong because sibling margins are between boxes (part of content),
                // not outside boxes (like escaped margins).

                let collapsed = collapse_margins(last_margin_bottom, child_margin_top);
                main_pen += collapsed;
                total_sibling_margins += collapsed;
                debug_info!(
                    ctx,
                    "[layout_bfc] Sibling collapse for child {}: last_margin_bottom={}, \
                     child_margin_top={}, collapsed={}, main_pen={}, total_sibling_margins={}",
                    child_index,
                    last_margin_bottom,
                    child_margin_top,
                    collapsed,
                    main_pen,
                    total_sibling_margins
                );
            }
        }

        // K30b fit check: `main_pen` is final for this child (margins
        // resolved above). A child that does not fit the remaining
        // fragmentainer extent breaks BEFORE itself — it and every later
        // in-flow sibling become the outgoing token's unfinished tail.
        // A first-content child that can never fit places as a MONOLITH
        // (overflows the fragmentainer; never torn, never looped —
        // reporting arrives with the page-loop driver).
        if let Some(fs) = constraints.fragmentainer.as_ref() {
            use crate::solver3::break_token::{fragment_fit, tail_token, FitDecision};
            // A BLOCK CONTAINER with children is never a true monolith —
            // the monolith rule (place-overflowing) is for ATOMS. A
            // container that does not fit DESCENDS whenever there is usable
            // space, regardless of the placed_any/monolith classification
            // (a first-child wrapper taller than every page must split, not
            // overflow).
            let child_fits =
                main_pen + child_size.main(writing_mode) <= fs.remaining_block_extent + 0.01;
            let container_descend = !child_fits
                && tree.get(LayoutNodeId::new(child_index)).is_some_and(|n| {
                    matches!(n.formatting_context, FormattingContext::Block { .. })
                        && !tree.children(child_index).is_empty()
                })
                && (fs.remaining_block_extent - main_pen) >= 40.0;
            match if container_descend {
                FitDecision::BreakBeforeHere
            } else {
                fragment_fit(
                    main_pen,
                    child_size.main(writing_mode),
                    fs.remaining_block_extent,
                    fs.next_fragmentainer_extent,
                    fragment_placed_content,
                )
            } {
                FitDecision::Fits => {
                    // The fragmentainer PROPAGATES into fitting container
                    // children too: a forced break (or a deep unforced one
                    // behind conservative Pass-1 sizes) can hide INSIDE a
                    // child that fits — e.g. body fits the page whole, but
                    // a <pagebreak/> lives in it. Re-lay containers under
                    // the remaining extent; a returned token wraps as
                    // ResumeIn and stops sibling consumption after this
                    // child places its fitted part. NEVER for the child the
                    // RESUME arm just re-laid — a second pass with
                    // resume: None would clobber the resumed fragment and
                    // regenerate page 1's token forever (no-progress halt).
                    let child_is_block_container = !fragment_child_resumed
                        && tree.get(LayoutNodeId::new(child_index)).is_some_and(|n| {
                            matches!(n.formatting_context, FormattingContext::Block { .. })
                                && !tree.children(child_index).is_empty()
                        });
                    if child_is_block_container {
                        let child_space = FragmentainerSpace {
                            remaining_block_extent: fs.remaining_block_extent - main_pen,
                            next_fragmentainer_extent: fs.next_fragmentainer_extent,
                            is_first: fs.is_first && !fragment_placed_content,
                            resume: None,
                        };
                        let mut child_out: Option<crate::solver3::break_token::BreakToken> = None;
                        let mut tmp_positions: super::super::PositionVec = Vec::new();
                        let mut tmp_scrollbars = false;
                        crate::solver3::cache::calculate_layout_for_subtree_fragment(
                            ctx,
                            tree,
                            text_cache,
                            child_index,
                            LogicalPosition::zero(),
                            &CBTY::from_flattened_with_width_type(
                                children_containing_block_size,
                                constraints.available_width_type,
                            ),
                            &mut tmp_positions,
                            &mut tmp_scrollbars,
                            float_cache,
                            crate::solver3::cache::ComputeMode::ComputeSize,
                            Some(child_space),
                            Some(&mut child_out),
                        )?;
                        if let Some(cont) = child_out {
                            let later: Vec<usize> = pos_children
                                .iter()
                                .copied()
                                .skip_while(|&c| c != child_index)
                                .skip(1)
                                .filter(|&c| {
                                    let pt = get_position_type(
                                        ctx.styled_dom,
                                        tree.get(LayoutNodeId::new(c)).and_then(|n| n.dom_node_id),
                                    );
                                    pt != LayoutPosition::Absolute && pt != LayoutPosition::Fixed
                                })
                                .collect();
                            let mut children = alloc::vec![
                                crate::solver3::break_token::ChildBreakEntry::ResumeIn {
                                    child: child_index,
                                    token: Box::new(cont),
                                }
                            ];
                            children.extend(later.into_iter().map(|child| {
                                crate::solver3::break_token::ChildBreakEntry::BreakBefore {
                                    child,
                                    forced: false,
                                }
                            }));
                            fragment_token_out =
                                Some(crate::solver3::break_token::BreakToken::Block(
                                    crate::solver3::break_token::BlockBreakToken {
                                        node: node_index,
                                        consumed_block_size: main_pen,
                                        children,
                                        generation: 0,
                                    },
                                ));
                            // No break: this child PLACES its fitted part.
                        }
                    }
                }
                FitDecision::MonolithOverflow => {}
                FitDecision::BreakBeforeHere => {
                    let later: Vec<usize> = pos_children
                        .iter()
                        .copied()
                        .skip_while(|&c| c != child_index)
                        .skip(1)
                        .filter(|&c| {
                            let pt = get_position_type(
                                ctx.styled_dom,
                                tree.get(LayoutNodeId::new(c)).and_then(|n| n.dom_node_id),
                            );
                            pt != LayoutPosition::Absolute && pt != LayoutPosition::Fixed
                        })
                        .collect();

                    // K30b part 2, BREAK-DESCEND arm: a breakable BLOCK
                    // container with usable space left gets PART of itself
                    // on this fragmentainer — re-lay it inside the
                    // remaining extent; its own token becomes a ResumeIn
                    // entry. Leaf/IFC/atomic children (and sliver spaces
                    // < MIN_DESCEND_EXTENT) keep the whole-child
                    // BreakBefore of part 1.
                    const MIN_DESCEND_EXTENT: f32 = 40.0;
                    let child_is_block_container =
                        tree.get(LayoutNodeId::new(child_index)).is_some_and(|n| {
                            matches!(n.formatting_context, FormattingContext::Block { .. })
                                && !tree.children(child_index).is_empty()
                        });
                    let usable = fs.remaining_block_extent - main_pen;
                    if child_is_block_container && usable >= MIN_DESCEND_EXTENT {
                        let child_space = FragmentainerSpace {
                            remaining_block_extent: usable,
                            next_fragmentainer_extent: fs.next_fragmentainer_extent,
                            is_first: fs.is_first && !fragment_placed_content,
                            resume: None,
                        };
                        let mut child_out: Option<crate::solver3::break_token::BreakToken> = None;
                        let mut tmp_positions: super::super::PositionVec = Vec::new();
                        let mut tmp_scrollbars = false;
                        crate::solver3::cache::calculate_layout_for_subtree_fragment(
                            ctx,
                            tree,
                            text_cache,
                            child_index,
                            LogicalPosition::zero(),
                            &CBTY::from_flattened_with_width_type(
                                children_containing_block_size,
                                constraints.available_width_type,
                            ),
                            &mut tmp_positions,
                            &mut tmp_scrollbars,
                            float_cache,
                            crate::solver3::cache::ComputeMode::ComputeSize,
                            Some(child_space),
                            Some(&mut child_out),
                        )?;
                        if let Some(cont) = child_out {
                            // The child SPLIT: place its fitted part (fall
                            // through with the shortened used_size) and
                            // resume the rest on the next fragmentainer.
                            let mut children = alloc::vec![
                                crate::solver3::break_token::ChildBreakEntry::ResumeIn {
                                    child: child_index,
                                    token: Box::new(cont),
                                }
                            ];
                            children.extend(later.into_iter().map(|child| {
                                crate::solver3::break_token::ChildBreakEntry::BreakBefore {
                                    child,
                                    forced: false,
                                }
                            }));
                            fragment_token_out =
                                Some(crate::solver3::break_token::BreakToken::Block(
                                    crate::solver3::break_token::BlockBreakToken {
                                        node: node_index,
                                        consumed_block_size: main_pen,
                                        children,
                                        generation: 0,
                                    },
                                ));
                            // NO `break`: the loop-top guard stops the NEXT
                            // sibling; this child still places below.
                        } else {
                            // The child fit entirely once re-laid (its
                            // Pass-1 size was stale/conservative): place it,
                            // no token from this child.
                        }
                    } else {
                        // Roll the pen back: the margin that advanced it
                        // for THIS child adjoins the break and truncates.
                        main_pen = fragment_pen_at_child;
                        fragment_token_out = Some(tail_token(
                            node_index,
                            main_pen,
                            child_index,
                            later.into_iter(),
                        ));
                        clear_fragment_pos!(LayoutNodeId::new(child_index));
                        continue;
                    }
                }
            }
            fragment_placed_content = true;
        }

        // K30b: a descend/resume re-lay above may have SHORTENED this
        // child's used_size — refresh the local before positioning.
        let child_size = tree
            .get(LayoutNodeId::new(child_index))
            .and_then(|n| n.used_size)
            .unwrap_or(child_size);

        // Position child (non-empty blocks only reach here)
        //
        // +spec:block-formatting-context:1dada5 - Normal flow boxes in BFC touch containing block
        // edge +spec:block-formatting-context:9f56cb - each box's left outer edge touches
        // containing block left edge; new BFC may shrink due to floats CSS 2.2 § 9.4.1: "In
        // a block formatting context, each box's left outer edge touches the left edge of
        // the containing block (for right-to-left formatting, right edges touch).
        // This is true even in the presence of floats (although a box's line boxes may shrink
        // due to the floats), unless the box establishes a new block formatting context
        // (in which case the box itself may become narrower due to the floats)."
        //
        // +spec:block-formatting-context:3d2811 - Float overlap with normal flow element borders
        // +spec:display-property:796059 - BFC/replaced/table border box must not overlap float
        // margin boxes; line boxes shorten around floats +spec:floats:5214a6 -
        // BFC/replaced/table border box must not overlap float margin boxes; shrink or clear below
        // CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element
        // in the normal flow that establishes a new block formatting context (such as an element
        // with 'overflow' other than 'visible') must not overlap any floats in the same block
        // formatting context as the element itself."

        // +spec:floats:a29f70 - BFC roots, tables, and block-level replaced elements must not
        // overlap float margin boxes
        let child_node = tree
            .get(LayoutNodeId::new(child_index))
            .ok_or(LayoutError::InvalidTree)?;
        let avoids_floats =
            establishes_new_bfc(ctx, child_node, tree.cold(LayoutNodeId::new(child_index)))
                || is_block_level_replaced(ctx, child_node);

        // Query available space considering floats ONLY if child avoids floats
        let (cross_start, cross_end, available_cross) = if avoids_floats {
            // New BFC / replaced / table: Must shrink or move down to avoid overlapping floats
            let child_cross_needed = child_size.cross(writing_mode);
            let bfc_cross = flow_cross_size;

            let (mut start, mut end) = float_context.available_line_box_space(
                main_pen,
                main_pen + child_size.main(writing_mode),
                bfc_cross,
                writing_mode,
            );
            let mut available = end - start;

            // CSS 2.2 § 9.5: "If necessary, implementations should clear the said element
            // by placing it below any preceding floats, but may place it adjacent to such
            // floats if there is sufficient space."
            if available < child_cross_needed && !float_context.floats.is_empty() {
                let clear_to = float_context
                    .floats
                    .iter()
                    .filter(|f| {
                        let f_main_start =
                            f.rect.origin.main(writing_mode) - f.margin.main_start(writing_mode);
                        let f_main_end = f_main_start
                            + f.rect.size.main(writing_mode)
                            + f.margin.main_start(writing_mode)
                            + f.margin.main_end(writing_mode);
                        f_main_end > main_pen
                            && f_main_start < main_pen + child_size.main(writing_mode)
                    })
                    .map(|f| {
                        f.rect.origin.main(writing_mode)
                            + f.rect.size.main(writing_mode)
                            + f.margin.main_end(writing_mode)
                    })
                    .fold(main_pen, f32::max);

                if clear_to > main_pen {
                    main_pen = clear_to;
                    let (s, e) = float_context.available_line_box_space(
                        main_pen,
                        main_pen + child_size.main(writing_mode),
                        bfc_cross,
                        writing_mode,
                    );
                    start = s;
                    end = e;
                    available = end - start;
                }
            }

            debug_info!(
                ctx,
                "[layout_bfc] Child {} avoids floats: shrinking to avoid floats, \
                 cross_range={}..{}, available_cross={}",
                child_index,
                start,
                end,
                available
            );

            (start, end, available)
        } else {
            // Normal flow: Overlaps floats, positioned at full width
            // Only the child's INLINE CONTENT (if any) wraps around floats
            let start = 0.0;
            let end = flow_cross_size;
            let available = end - start;

            debug_info!(
                ctx,
                "[layout_bfc] Child {} is normal flow: overlapping floats at full width, \
                 available_cross={}",
                child_index,
                available
            );

            (start, end, available)
        };

        // Get child's margin, margin_auto, size, and formatting context
        let (
            child_margin_cloned,
            child_margin_auto,
            child_used_size,
            is_inline_fc,
            child_dom_id_for_debug,
        ) = {
            let child_node = tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;
            let cbp = child_node.box_props.unpack();
            (
                cbp.margin,
                cbp.margin_auto,
                child_node.used_size.unwrap_or_default(),
                child_node.formatting_context == FormattingContext::Inline,
                child_node.dom_node_id,
            )
        };
        let child_margin = &child_margin_cloned;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} margin_auto: left={}, right={}, top={}, bottom={}",
            child_index,
            child_margin_auto.left,
            child_margin_auto.right,
            child_margin_auto.top,
            child_margin_auto.bottom
        );
        debug_info!(
            ctx,
            "[layout_bfc] Child {} used_size: width={}, height={}",
            child_index,
            child_used_size.width,
            child_used_size.height
        );

        // Position child
        // For normal flow blocks (including IFCs): position at full width (cross_start = 0)
        // For BFC-establishing blocks: position in available space between floats
        //
        // CSS 2.2 § 10.3.3: If margin-left and margin-right are both auto,
        // their used values are equal, centering the element horizontally.

        let (child_cross_pos, mut child_main_pos) = if avoids_floats {
            // BFC: Position in float-free space, but also check margin:auto centering.
            // A flex container or overflow:hidden box establishes a BFC (must avoid floats)
            // but can still be centered via margin:auto — these are independent concepts.
            let cross_pos = if child_margin_auto.left && child_margin_auto.right {
                let remaining = (available_cross - child_used_size.cross(writing_mode)).max(0.0);
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} BFC + margin:auto centering: available={}, size={}, \
                     offset={}",
                    child_index,
                    available_cross,
                    child_used_size.cross(writing_mode),
                    remaining / 2.0
                );
                cross_start + remaining / 2.0
            } else if child_margin_auto.left {
                let remaining =
                    (available_cross - child_used_size.cross(writing_mode) - child_margin.right)
                        .max(0.0);
                cross_start + remaining
            } else if legacy_center {
                let remaining = (available_cross
                    - child_used_size.cross(writing_mode)
                    - child_margin.cross_start(writing_mode)
                    - child_margin.cross_end(writing_mode))
                .max(0.0);
                cross_start + child_margin.cross_start(writing_mode) + remaining / 2.0
            } else {
                cross_start + child_margin.cross_start(writing_mode)
            };
            (cross_pos, main_pen)
        } else {
            // Normal flow: Check for margin: auto centering
            let available_cross = flow_cross_size;
            let child_cross_size = child_used_size.cross(writing_mode);

            debug_info!(
                ctx,
                "[layout_bfc] Child {} centering check: available_cross={}, child_cross_size={}, \
                 margin_auto.left={}, margin_auto.right={}",
                child_index,
                available_cross,
                child_cross_size,
                child_margin_auto.left,
                child_margin_auto.right
            );

            // +spec:block-formatting-context:d52ce5 - auto margins resolved per containing block's
            // writing mode for centering +spec:width-calculation:0c5044 - auto margins
            // center element on cross axis (respects writing mode)
            // +spec:width-calculation:25c2fc - §10.3.3: block-level margin auto centering and
            // over-constrained resolution +spec:width-calculation:ba691f - auto margins
            // treated as zero when element overflows containing block (via .max(0.0) on
            // remaining_space) +spec:width-calculation:324e7e - both margin-left and
            // margin-right auto => equal used values (centering) CSS 2.2 § 10.3.3: If
            // both margin-left and margin-right are auto, center the element within the
            // available space
            let cross_pos = if child_margin_auto.left && child_margin_auto.right {
                // Center: (available - child_width) / 2
                let remaining_space = (available_cross - child_cross_size).max(0.0);
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} CENTERING: remaining_space={}, cross_pos={}",
                    child_index,
                    remaining_space,
                    remaining_space / 2.0
                );
                remaining_space / 2.0
            } else if child_margin_auto.left {
                // Only left is auto: push element to the right
                let remaining_space =
                    (available_cross - child_cross_size - child_margin.right).max(0.0);
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} margin-left:auto only, pushing right: \
                     remaining_space={}",
                    child_index,
                    remaining_space
                );
                remaining_space
            } else if child_margin_auto.right {
                // Only right is auto: element stays at left with its margin
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} margin-right:auto only, using left margin={}",
                    child_index,
                    child_margin.cross_start(writing_mode)
                );
                child_margin.cross_start(writing_mode)
            } else if legacy_center {
                let remaining = (available_cross
                    - child_cross_size
                    - child_margin.cross_start(writing_mode)
                    - child_margin.cross_end(writing_mode))
                .max(0.0);
                child_margin.cross_start(writing_mode) + remaining / 2.0
            } else {
                // +spec:box-model:218643 - over-constrained: drop end margin per containing block
                // writing mode +spec:width-calculation:d172a4 - over-constrained:
                // LTR ignores margin-right, RTL ignores margin-left
                // in LTR, margin-right is ignored (element positioned at margin-left);
                // in RTL, margin-left is ignored (element positioned from right edge)
                let is_rtl = tree
                    .get(LayoutNodeId::new(node_index))
                    .and_then(|n| n.dom_node_id)
                    .is_some_and(|cb_dom_id| {
                        let node_state = ctx
                            .styled_dom
                            .styled_nodes
                            .as_container()
                            .get(cb_dom_id)
                            .map(|s| s.styled_node_state)
                            .unwrap_or_default();
                        matches!(
                            get_direction_property(ctx.styled_dom, cb_dom_id, &node_state),
                            MultiValue::Exact(StyleDirection::Rtl)
                        )
                    });
                let cross_pos = if is_rtl {
                    // RTL: ignore margin-left, position from right edge
                    available_cross - child_cross_size - child_margin.cross_end(writing_mode)
                } else {
                    // LTR (default): ignore margin-right, position at margin-left
                    child_margin.cross_start(writing_mode)
                };
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} NO auto margins (over-constrained), is_rtl={}, \
                     cross_pos={}",
                    child_index,
                    is_rtl,
                    cross_pos
                );
                cross_pos
            };

            (cross_pos, main_pen)
        };

        // NOTE: We do NOT adjust child_main_pos based on child's escaped_top_margin here!
        // The escaped_top_margin represents margins that escaped FROM the child's own children.
        // The child's position in THIS BFC is determined by main_pen and the child's own margin
        // (which was already handled in the margin collapse logic above).
        //
        // Previously, this code incorrectly added child_escaped_margin to child_main_pos,
        // which caused double-application of margins because:
        // 1. The child's margin was used to calculate its position in THIS BFC
        // 2. Then its escaped_top_margin (which included its own margin) was added again
        //
        // The correct behavior per CSS 2.2 § 8.3.1 is:
        // - The child's escaped_top_margin is used by THIS node's parent to position THIS node
        // - It does NOT affect how we position the child within our content-box

        // final_pos is [CoordinateSpace::Parent] - relative to this BFC's content-box
        let final_pos =
            LogicalPosition::from_main_cross(child_main_pos, child_cross_pos, writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child={}, final_pos={:?}, \
             main_pen={}, avoids_floats={}",
            child_index,
            final_pos,
            main_pen,
            avoids_floats
        );

        // Re-layout IFC children with float context for correct text wrapping
        // Normal flow blocks WITH inline content need float context propagated
        if is_inline_fc && !avoids_floats {
            // Use cached floats if available (from previous layout passes),
            // otherwise use the floats positioned in this pass
            let floats_for_ifc = float_cache.get(&node_index).unwrap_or(&float_context);

            debug_info!(
                ctx,
                "[layout_bfc] Re-layouting IFC child {} (normal flow) with parent's float context \
                 at Y={}, child_cross_pos={}",
                child_index,
                main_pen,
                child_cross_pos
            );
            debug_info!(
                ctx,
                "[layout_bfc]   Using {} floats (from cache: {})",
                floats_for_ifc.floats.len(),
                float_cache.contains_key(&node_index)
            );

            // Translate float coordinates from BFC-relative to IFC-relative
            // The IFC child is positioned at (child_cross_pos, main_pen) in BFC coordinates
            // Floats need to be relative to the IFC's CONTENT-BOX origin (inside padding/border)
            let child_node = tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;
            let cbp = child_node.box_props.unpack();
            let padding_border_cross =
                cbp.padding.cross_start(writing_mode) + cbp.border.cross_start(writing_mode);
            let padding_border_main =
                cbp.padding.main_start(writing_mode) + cbp.border.main_start(writing_mode);

            // Content-box origin in BFC coordinates
            let content_box_cross = child_cross_pos + padding_border_cross;
            let content_box_main = main_pen + padding_border_main;

            debug_info!(
                ctx,
                "[layout_bfc]   Border-box at ({}, {}), Content-box at ({}, {}), \
                 padding+border=({}, {})",
                child_cross_pos,
                main_pen,
                content_box_cross,
                content_box_main,
                padding_border_cross,
                padding_border_main
            );

            let mut ifc_floats = FloatingContext::default();
            for float_box in &floats_for_ifc.floats {
                // Convert float position from BFC coords to IFC CONTENT-BOX relative coords
                let float_rel_to_ifc = LogicalRect {
                    origin: LogicalPosition {
                        x: float_box.rect.origin.x - content_box_cross,
                        y: float_box.rect.origin.y - content_box_main,
                    },
                    size: float_box.rect.size,
                };

                debug_info!(
                    ctx,
                    "[layout_bfc] Float {:?}: BFC coords = {:?}, IFC-content-relative = {:?}",
                    float_box.kind,
                    float_box.rect,
                    float_rel_to_ifc
                );

                ifc_floats.add_float(float_box.kind, float_rel_to_ifc, float_box.margin);
            }

            // Create a BfcState with IFC-relative float coordinates
            let mut bfc_state = BfcState {
                pen: LogicalPosition::zero(), // IFC starts at its own origin
                floats: ifc_floats.clone(),
                margins: MarginCollapseContext::default(),
            };

            debug_info!(
                ctx,
                "[layout_bfc]   Created IFC-relative FloatingContext with {} floats",
                ifc_floats.floats.len()
            );

            // Get the IFC child's content-box size (after padding/border)
            let child_node = tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;
            let child_dom_id = child_node.dom_node_id;

            // +spec:containing-block:a8ada9 - line box width determined by containing block and
            // floats For inline elements (display: inline), use containing block width
            // as available width. Inline elements flow within the containing block and
            // wrap at its width. CSS 2.2 § 10.3.1: For inline elements, available width
            // = containing block width.
            let display = get_display_property(ctx.styled_dom, child_dom_id).unwrap_or_default();
            let child_content_size = if display == LayoutDisplay::Inline {
                // Inline elements use the containing block's content-box width
                LogicalSize::new(
                    children_containing_block_size.width,
                    children_containing_block_size.height,
                )
            } else {
                // Block-level elements use their own content-box - its HEIGHT
                // only where the height is the box's own, as the child's own
                // layout offers its content (`cache::prepare_layout_context`):
                // an auto-height box (or a percentage one computing to auto)
                // has the height its content gives it, so its content sees
                // an indefinite one (CSS 2.2 10.5) - or the containing
                // block's, where the box forwards it. Pass 1's used height
                // here is that content's own result: offered back as a
                // definite height, a `height: 100%` inline-block on the
                // body's line resolved against the body's line box (AzMail's
                // paper 118 / 414 tall for content of 114 / 400).
                let inner = child_node.box_props.inner_size(child_size, writing_mode);
                let height_is_auto = tree.warm(LayoutNodeId::new(child_index)).is_none_or(|w| {
                    crate::solver3::sizing::height_is_auto_for_children(
                        &child_node.formatting_context,
                        w.computed_style.height.as_ref(),
                        children_containing_block_size.height.is_finite(),
                    )
                });
                if !height_is_auto {
                    inner
                } else if crate::solver3::cache::forwards_containing_block_height(tree, child_index)
                {
                    LogicalSize::new(inner.width, children_containing_block_size.height)
                } else {
                    LogicalSize::new(inner.width, f32::INFINITY)
                }
            };

            debug_info!(
                ctx,
                "[layout_bfc]   IFC child size: border-box={:?}, content-box={:?}",
                child_size,
                child_content_size
            );

            // Create new constraints with float context
            // IMPORTANT: Use the child's CONTENT-BOX width, not the BFC width!
            let ifc_constraints = LayoutConstraints {
                available_size: child_content_size,
                bfc_state: Some(&mut bfc_state),
                writing_mode,
                writing_mode_ctx: constraints.writing_mode_ctx,
                text_align: constraints.text_align,
                containing_block_size: constraints.containing_block_size,
                available_width_type: Text3AvailableSpace::Definite(child_content_size.width),
                fragmentainer: None,
                column_flow: None,
            };

            // Re-layout the IFC with float awareness
            // This will pass floats as exclusion zones to text3 for line wrapping
            let ifc_result = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &ifc_constraints,
                float_cache,
            )?;
            child_scrollbar_reflow |= ifc_result.scrollbar_reflow_needed;

            // DON'T update used_size - the box keeps its full width!
            // Only the text layout inside changes to wrap around floats

            debug_info!(
                ctx,
                "[layout_bfc] IFC child {} re-layouted with float context (text will wrap, box \
                 stays full width)",
                child_index
            );

            // NOTE: We do NOT merge inline-block positions from the IFC's output.positions here!
            // The IFC's inline-block children will be correctly positioned when
            // calculate_layout_for_subtree recursively processes the IFC node (child_index).
            // At that point, layout_ifc will be called again, and the inline-block positions
            // will be relative to the IFC's content-box, which is what we want.
            //
            // Merging them here would cause them to be processed by process_inflow_child
            // with the BFC's content-box position (self_content_box_pos of the BFC),
            // resulting in incorrect absolute positions.
        }

        output.positions.insert(child_index, final_pos);

        // CSS margin collapse: escaped margins are handled via accumulated_top_margin
        // at the START of layout, not by adjusting positions after layout.
        // We simply advance by the child's actual size.
        main_pen += child_size.main(writing_mode);
        has_content = true;

        // Update last margin for next sibling
        // CSS 2.2 § 8.3.1: The bottom margin of this box will collapse with the top margin
        // of the next sibling (if no clearance or blockers intervene)
        // element (between prev sibling's bottom and this element's top margin). The cleared
        // element's bottom margin is still available for normal collapsing with the next sibling.
        // CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing and acts as spacing above
        // the margin-top of an element."
        last_margin_bottom = child_margin_bottom;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} positioned at final_pos={:?}, size={:?}, advanced main_pen to \
             {}, last_margin_bottom={}, clearance_applied={}",
            child_index,
            final_pos,
            child_size,
            main_pen,
            last_margin_bottom,
            clearance_applied
        );

        // Track the maximum cross-axis size to determine the BFC's overflow size.
        let child_cross_extent =
            child_cross_pos + child_size.cross(writing_mode) + child_margin.cross_end(writing_mode);
        max_cross_size = max_cross_size.max(child_cross_extent);
    }

    // CSS Multicol 1: the children stand in ONE column of the column width;
    // cut it into the container's columns and move them there.
    let multicol_extent = match &multicol {
        Some(columns) if has_content => Some(distribute_into_columns(
            ctx,
            tree,
            text_cache,
            float_cache,
            columns,
            &pos_children,
            &mut output.positions,
            !float_context.floats.is_empty(),
            constraints,
        )?),
        _ => None,
    };

    // Store the float context in cache for future layout passes
    // This happens after ALL children (floats and normal) have been positioned
    debug_info!(
        ctx,
        "[layout_bfc] Storing {} floats in cache for node {}",
        float_context.floats.len(),
        node_index
    );
    float_cache.insert(node_index, float_context.clone());

    // PHASE 3: Parent-Child Bottom Margin Escape
    let mut escaped_top_margin = None;
    let mut escaped_bottom_margin = None;

    // Handle top margin escape
    if top_margin_escaped {
        // First child's margin escaped through parent
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Returning escaped top margin: accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved && accumulated_top_margin > 0.0 {
        // No content was positioned, all margins accumulated (empty blocks)
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else {
        // Don't set escaped_top_margin = Some(0) — that would override the child's
        // own margin (e.g., 30px) with 0 during sibling collapse.
        debug_info!(
            ctx,
            "[layout_bfc] NOT escaping top margin: top_margin_resolved={}, escaped={}, \
             accumulated={}, node={}",
            top_margin_resolved,
            top_margin_escaped,
            accumulated_top_margin,
            node_index
        );
    }

    // Handle bottom margin escape
    if let Some(last_idx) = last_child_index {
        let last_child = tree
            .get(LayoutNodeId::new(last_idx))
            .ok_or(LayoutError::InvalidTree)?;
        let last_child_bp = last_child.box_props.unpack();
        let last_has_bottom_blocker =
            has_margin_collapse_blocker(&last_child_bp, writing_mode, false);

        debug_info!(
            ctx,
            "[layout_bfc] Bottom margin for node {}: parent_has_bottom_blocker={}, \
             last_has_bottom_blocker={}, last_margin_bottom={}, main_pen_before={}",
            node_index,
            parent_has_bottom_blocker,
            last_has_bottom_blocker,
            last_margin_bottom,
            main_pen
        );

        if !parent_has_bottom_blocker && has_content {
            // CSS 2.2 section 8.3.1: the bottom margin of the LAST in-flow child
            // adjoins the parent's bottom margin whenever the parent has auto
            // height and no bottom padding/border. The child's OWN bottom
            // padding/border is irrelevant to THIS adjacency — it only decides
            // whether the child's descendants' margins were already merged into
            // `last_margin_bottom` (handled where child_escaped_bottom is read).
            // An earlier version required the last child to be blocker-free too
            // and exported only the parent's own margin otherwise: a padded
            // child with margin-bottom 50 under a parent with margin-bottom 40
            // produced a 40px sibling gap instead of Chrome's 50px, shifting
            // everything below (block-margin-collapse-complex-001, -10px per
            // section). The margin is NOT added to main_pen either way — it
            // escapes the content box (counting it double-counted the height,
            // nested-container came out 180px instead of 130px).
            let collapsed_bottom = collapse_margins(parent_margin_bottom, last_margin_bottom);
            escaped_bottom_margin = Some(collapsed_bottom);
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin ESCAPED for node {}: collapsed={}",
                node_index,
                collapsed_bottom
            );
        } else {
            // Can't escape: add to pen
            main_pen += last_margin_bottom;
            // NOTE: We do NOT add parent_margin_bottom to main_pen here!
            // parent_margin_bottom is added OUTSIDE the content-box (in the margin-box)
            // The content-box height should only include children's content and margins
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin BLOCKED for node {}: added last_margin_bottom={}, \
                 main_pen_after={}",
                node_index,
                last_margin_bottom,
                main_pen
            );
        }
    } else {
        // No in-flow children: the content box is EMPTY, so the pen stays
        // where it is. This node's own margins live in the PARENT's
        // coordinate space (exactly as the branches above say) and never
        // count towards its content height. They used to be added here, so
        // every childless block reported a content height equal to its
        // margin sum (`<div style="margin: 20px 0 30px">` came out 50px
        // tall) — and, because `is_empty_block` reads `used_size`, the
        // parent then refused to collapse that "non-empty" block's margins
        // through (CSS 2.2 §8.3.1): a(mb 10) / empty(mt 20, mb 30) / b(mt 5)
        // stacked 50 + 10 + 20 + 50 + 30 instead of 50 + max(10, 20, 30, 5).
        // Floats inside a BFC root still grow it below (§10.6.7).
    }

    // CRITICAL: If this is a root node (no parent), apply escaped margins directly
    // instead of propagating them upward (since there's no parent to receive them)
    let is_root_node = node.parent.is_none();
    if is_root_node {
        if let Some(top) = escaped_top_margin {
            // Adjust all child positions downward by the escaped top margin -
            // the absolute children's static positions too: they were taken
            // with the same pen (an absolute box after the first paragraph
            // of a page sat that margin above the block after it).
            for pos in output
                .positions
                .values_mut()
                .chain(output.static_positions.values_mut())
            {
                let current_main = pos.main(writing_mode);
                *pos = LogicalPosition::from_main_cross(
                    current_main + top,
                    pos.cross(writing_mode),
                    writing_mode,
                );
            }
            main_pen += top;
        }
        if let Some(bottom) = escaped_bottom_margin {
            main_pen += bottom;
        }
        // For root nodes, don't propagate margins further
        escaped_top_margin = None;
        escaped_bottom_margin = None;
    }

    // CSS 2.2 § 9.5: Floats don't contribute to container height with overflow:visible
    //
    // However, browsers DO expand containers to contain floats in specific cases:
    //
    // 1. If there's NO in-flow content (main_pen == 0), floats determine height
    // 2. If container establishes a BFC (overflow != visible)
    //
    // In this case, we have in-flow content (main_pen > 0) and overflow:visible,
    // so floats should NOT expand the container. Their margins can "bleed" beyond
    // the container boundaries into the parent.
    //
    // This matches Chrome/Firefox behavior where float margins escape through
    // the container's padding when there's existing in-flow content.

    // +spec:block-formatting-context:7954a2 - 10.6.3: auto height for block-level non-replaced
    // elements in normal flow Content-box Height Calculation
    //
    // CSS 2.2 § 8.3.1: "The top border edge of the box is defined to coincide with
    // the top border edge of the [first] child" when margins collapse/escape.
    //
    // This means escaped margins do NOT contribute to the parent's content-box height.
    //
    // Calculation:
    //
    //   main_pen = total vertical space used by all children and margins
    //
    //   Components of main_pen:
    //
    //   1. Children's border-boxes (always included)
    //   2. Sibling collapsed margins (space BETWEEN children - part of content)
    //   3. First child's position (0 if margin escaped, margin_top if blocked)
    //
    //   What to subtract:
    //
    //   - total_escaped_top_margin: First child's margin that went to grandparent's space This
    //     margin is OUTSIDE our content-box, so we must subtract it.
    //
    //   What NOT to subtract:
    //
    //   - total_sibling_margins: These are the gaps BETWEEN children, which are
    //    legitimately part of our content area's layout space.
    //
    // Example with escaped margin:
    //   <div class="parent" padding=0>              <!-- Node 2 -->
    //     <div class="child1" margin=30></div>      <!-- Node 3, margin escapes -->
    //     <div class="child2" margin=40></div>      <!-- Node 5 -->
    //   </div>
    //
    //   Layout process:
    //
    //   - Node 3 positioned at main_pen=0 (margin escaped)
    //   - Node 3 size=140px → main_pen advances to 140
    //   - Sibling collapse: max(30 child1 bottom, 40 child2 top) = 40px
    //   - main_pen advances to 180
    //   - Node 5 size=130px → main_pen advances to 310
    //   - total_escaped_top_margin = 30
    //   - total_sibling_margins = 40 (tracked but NOT subtracted)
    //   - content_box_height = 310 - 30 = 280px ✓
    //
    // Previously, we calculated:
    //
    //   content_box_height = main_pen - total_escaped_top_margin - total_sibling_margins
    //
    // This incorrectly subtracted sibling margins, making parent too small.
    // Sibling margins are *between* boxes (part of layout), not *outside* boxes
    // (like escaped margins).

    // +spec:box-model:4eebed - auto height for BFC = top margin-edge of topmost child to bottom
    // margin-edge of bottommost child +spec:box-model:4eebed - auto height = top margin-edge of
    // topmost child to bottom margin-edge of bottommost child +spec:height-calculation:d65226 -
    // §10.6.7 auto heights for BFC roots: block children use margin-edge of topmost/bottommost,
    // floats extend height if below content edge +spec:positioning:1a05bb - 10.6.7 auto height
    // for BFC roots: block children use margin edges, abspos ignored (skipped in Pass 1/2),
    // relative considered without offset (applied after layout), floats whose bottom margin
    // edge exceeds content edge expand height (below) +spec:positioning:e6712c - Auto height
    // for BFC: distance between top/bottom margin-edges of block children (minus escaped
    // margins), ignoring absolutely positioned children (skipped at line ~966), considering
    // relatively positioned boxes without offset (applied after layout), and extending to
    // include floats whose bottom margin edge exceeds content edge +spec:positioning:f94d22 -
    // 10.6.3: block-level non-replaced auto height = distance from top content edge to last in-flow
    // child bottom margin edge (or zero) CSS 2.2 §8.3.1: escaped margins (both top and bottom)
    // don't contribute to parent height
    let mut content_box_height = if is_root_node {
        // Root: the escaped margins were re-added to `main_pen` just above (there is no
        // grandparent to receive them); subtract them back out so the root's content box
        // still excludes them. Net effect is the pre-escape span.
        main_pen - total_escaped_top_margin - escaped_bottom_margin.unwrap_or(0.0)
    } else {
        // Non-root: the first in-flow child was positioned at main_pen == 0 (its top
        // margin escaped, NOT added to the pen) and an escaped bottom margin was never
        // advanced into the pen either. So `main_pen` already spans the first child's
        // border-top to the last child's border-bottom — exactly the content-box height
        // (CSS 2.2 §8.3.1). The escaped margins live in the PARENT's coordinate space and
        // reach it via `escaped_top_margin` / `escaped_bottom_margin`. Subtracting them
        // from THIS box's height double-removes them and collapses it (#20: a <div> around
        // a single <p> came out 0px tall, pulling the following sibling up by a line).
        main_pen
    };

    // +spec:block-formatting-context:f73d3e - BFC root grows to fully contain its floats; floats
    // from outside cannot protrude in whose bottom margin edge exceeds bottom content edge;
    // only floats participating in this BFC are counted (not floats inside abspos descendants
    // or nested BFCs) +spec:box-model:1d4798 - auto height includes floats whose bottom margin
    // edge exceeds content edge only floats participating in this BFC are counted (not floats
    // inside abspos descendants or nested BFCs)
    if is_bfc_root {
        for float_box in &float_context.floats {
            let float_bottom_margin_edge = float_box.rect.origin.main(writing_mode)
                + float_box.rect.size.main(writing_mode)
                + float_box.margin.main_end(writing_mode);
            if float_bottom_margin_edge > content_box_height {
                content_box_height = float_bottom_margin_edge;
            }
        }
    }

    // A list item is at least as tall as its marker laid out with no line
    // box (the loop above; the taller of the two, never their sum).
    content_box_height = content_box_height.max(marker_without_line_main);

    // A multi-column container is as tall as its tallest column (its
    // floats are in the columns too) and as wide as its columns reach.
    if let Some(extent) = multicol_extent {
        content_box_height = extent.height;
        max_cross_size = extent.width;
    }

    // +spec:display-contents:f6de1a - content height overflow tracked via overflow_size
    // +spec:overflow:043182 - overflow computed from box bounds + children overflow
    output.overflow_size =
        LogicalSize::from_main_cross(content_box_height, max_cross_size, writing_mode);

    debug_info!(
        ctx,
        "[layout_bfc] FINAL for node {}: main_pen={}, total_escaped_top={}, \
         total_sibling_margins={}, content_box_height={}",
        node_index,
        main_pen,
        total_escaped_top_margin,
        total_sibling_margins,
        content_box_height
    );

    // +spec:inline-formatting-context:2227a4 - atomic inline baseline for inline-block/inline-table
    // CSS2 §10.8.1: For inline-block, baseline is the baseline of the last
    // line box in normal flow, or the bottom margin edge if no line boxes
    // (`None`, which `atomic_inline_baseline_offset` turns into that edge).
    output.baseline = last_line_box_baseline(tree, ctx.styled_dom, node_index, &output.positions);

    // Store escaped margins in the LayoutNode for use by parent
    if let Some(warm_mut) = tree.warm_mut(LayoutNodeId::new(node_index)) {
        warm_mut.escaped_top_margin = escaped_top_margin;
        warm_mut.escaped_bottom_margin = escaped_bottom_margin;
    }

    if let Some(warm_mut) = tree.warm_mut(LayoutNodeId::new(node_index)) {
        warm_mut.baseline = output.baseline;
    }

    Ok((
        escaped_top_margin,
        escaped_bottom_margin,
        fragment_token_out,
        child_scrollbar_reflow,
    ))
}
