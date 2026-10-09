//! Atomic inlines (inline-block, inline-table, replaced boxes): their size and baseline on a line.

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

/// Gathers all inline content for `text3`, recursively laying out `inline-block` children
/// to determine their size and baseline before passing them to the text engine.
///
/// This function also assigns IFC membership to all participating nodes:
/// - The IFC root gets an `ifc_id` assigned
/// - Each text/inline child gets `ifc_membership` set with a reference back to the IFC root
///
/// This mapping enables efficient cursor hit-testing: when a text node is clicked,
/// we can find its parent IFC's `inline_layout_result` via `ifc_membership.ifc_root_layout_index`.
// +spec:display-property:63a38b - inline box boundaries and out-of-flow elements are ignored for
// text adjacency (white space, line-breaking, text-transform)
/// The containing block of an atomic inline-level box (an inline-block, an
/// image) in the inline formatting context `constraints` lays out: the IFC
/// root's CONTENT box - exactly what `layout_bfc` hands its block children
/// (`children_containing_block_size`: the content width, and the containing
/// block's height while the root's own height is auto).
/// `constraints.containing_block_size` is the root's OWN containing block: a
/// quarter of it made `body > img { width: 25% }` a quarter of the window.
pub(super) const fn atomic_inline_containing_block(
    constraints: &LayoutConstraints<'_>,
) -> LogicalSize {
    constraints.available_size
}

/// What an IFC's atomic inlines are measured against, as
/// `CachedInlineContent::atomics_measured_against` records it.
pub(super) fn atomic_inline_measure_key(
    constraints: &LayoutConstraints<'_>,
) -> (LogicalSize, Text3AvailableSpace) {
    (
        atomic_inline_containing_block(constraints),
        constraints.available_width_type,
    )
}

/// Does every atomic inline-level child in a cached collection have a size in
/// THIS tree?
///
/// `CachedInlineContent` is the memo of a call with two outputs: the collection
/// (stored) and the layout of each atomic inline child (written to the tree, not
/// stored). A carried collection is therefore only reusable while the second
/// output is still present — true for repeat visits inside one layout pass
/// (min-content, max-content, definite), false the first time an IFC root is
/// visited in a rebuilt tree, where the children start unlaid.
///
/// A zero-area size counts as unlaid. It cannot be distinguished from "never
/// measured" here, and the cost of being wrong in this direction is one extra
/// measurement of a genuinely empty inline-block; the cost in the other
/// direction is an element that paints and hit-tests as nothing.
pub(super) fn atomic_inline_children_are_laid_out(
    tree: &LayoutTree,
    content: &[InlineContent],
    child_map: &HashMap<ContentIndex, usize>,
) -> bool {
    content.iter().enumerate().all(|(i, item)| {
        if !matches!(item, InlineContent::Shape(_)) {
            return true;
        }
        // Shapes are keyed by their position in the content array, item 0 —
        // the same key `collect_and_measure_inline_content_impl` inserts with.
        let key = ContentIndex {
            run_index: i as u32,
            item_index: 0,
        };
        child_map.get(&key).is_some_and(|&child| {
            tree.get(LayoutNodeId::new(child))
                .and_then(|n| n.used_size)
                .is_some_and(|s| s.width > 0.0 || s.height > 0.0)
        })
    })
}

/// The `baseline_offset` text3 wants for an atomic inline: the distance from
/// the BOTTOM of its margin box up to its baseline — the box's descent, which
/// is how `item_ascent_descent` (text3/cache.rs) reads the field.
///
/// Layout measures the opposite way: [`LayoutOutput::baseline`] is the distance
/// from the top of the box's CONTENT box down to the baseline. Converting
/// between the two is the whole of CSS 2.2 s 10.8.1 for these boxes — an atomic
/// inline's baseline is the baseline of its last in-flow line box, and a box
/// with no in-flow line boxes, or with `overflow` other than `visible`, takes
/// its bottom margin edge instead.
///
/// This existed three times over, and two of the copies passed the top-relative
/// number straight through. That puts the whole box BELOW the baseline (ascent
/// 0), so a line box holding nothing else sinks it by the strut's ascent: 12.8
/// px under a 16 px container font, which is exactly how far the button in
/// every hello-world jumped the first time its window redrew.
///
/// * `baseline_from_content_top` — [`LayoutOutput::baseline`] of the box.
/// * `border_box_height` — its used border-box height.
/// * `content_box_top` — padding + border on the box's top edge, i.e. the offset from its border
///   box to its content box.
/// * `margin_bottom` — the bottom margin, since text3 aligns the MARGIN box.
/// The used BORDER-BOX block size of an atomic inline whose height is `auto`.
///
/// `LayoutOutput::overflow_size` does not mean the same thing in every
/// formatting context. `layout_bfc` reports a CONTENT-box extent, so a
/// consumer adds padding and border to reach the border box. `layout_flex_grid`
/// reports taffy's `content_size` with only the leading BORDER stripped — the
/// padding is still inside it, deliberately, because the scrollbar geometry
/// measures in the padding box. Adding padding to THAT counts it twice: an
/// `inline-flex` button with `padding: 6px 12px` came out 12 px too tall, which
/// is every button in every hello-world and on the frontpage screenshots.
///
/// A flex or grid container has already resolved its own border box — taffy's
/// container size, which `layout_flex_grid` writes into the node's `used_size`.
/// That is the answer for those; everything else is content + padding + border.
pub(super) fn atomic_inline_auto_height(
    child_fc: Option<FormattingContext>,
    child_used_height: Option<f32>,
    content_height: f32,
    padding_border_sum: f32,
) -> f32 {
    match (child_fc, child_used_height) {
        (Some(FormattingContext::Flex | FormattingContext::Grid), Some(h)) => h,
        _ => content_height + padding_border_sum,
    }
}

/// Publish the positions an interior layout run produced into the tree, where
/// the rest of the engine can see them.
///
/// A nested run (the one that lays out an atomic inline's own contents) reports
/// its result in `LayoutOutput::positions`, keyed by layout-node index. But
/// every later consumer — `position_bfc_child_descendants`, absolute
/// positioning, painting — reads `LayoutNodeWarm::relative_position`. Until the
/// two are joined the children are laid out correctly and then positioned as if
/// they were all at their parent's content-box origin, which shows up as every
/// margin inside the atomic inline being ignored.
///
/// The absolutely-positioned path does this in `positioning.rs` and the flex
/// path in `taffy_bridge.rs`; this is the same join for the inline path.
pub(super) fn publish_interior_positions(tree: &mut LayoutTree, output: &LayoutOutput) {
    for (child_idx, child_pos) in &output.positions {
        if let Some(w) = tree.warm_mut(LayoutNodeId::new(*child_idx)) {
            w.relative_position = Some(*child_pos);
        }
    }
}

/// The baseline of a block container's LAST line box in the normal flow,
/// from the top of its content box (CSS 2.2 s10.8.1 - what an inline-block
/// aligns by): the last in-flow child of `node_index` that has a baseline of
/// its own (`LayoutNodeWarm::baseline`, from that child's content-box top, set
/// when Pass 1 of `layout_bfc` laid the child out), at the child's position in
/// `positions` (its border-box origin) plus its top border and padding. A
/// float is not in the flow; a child without a line box is passed over.
/// `None` when no child has one. A block container used to report no
/// baseline at all, so an inline-block whose text sits in a block child was
/// aligned by its bottom edge.
pub(super) fn last_line_box_baseline(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    node_index: usize,
    positions: &BTreeMap<usize, LogicalPosition>,
) -> Option<f32> {
    tree.children(node_index).iter().rev().find_map(|&child| {
        let pos = positions.get(&child)?;
        let node = tree.get(LayoutNodeId::new(child))?;
        if get_float_property(styled_dom, node.dom_node_id) != LayoutFloat::None {
            return None;
        }
        let baseline = tree.warm(LayoutNodeId::new(child))?.baseline?;
        let bp = node.box_props.unpack();
        Some(pos.y + bp.border.top + bp.padding.top + baseline)
    })
}

pub(super) fn atomic_inline_baseline_offset(
    baseline_from_content_top: Option<f32>,
    border_box_height: f32,
    content_box_top: f32,
    margin_bottom: f32,
    overflow_is_visible: bool,
) -> f32 {
    match baseline_from_content_top {
        Some(baseline_y) if overflow_is_visible => {
            let from_border_box_top = baseline_y + content_box_top;
            (border_box_height - from_border_box_top).max(0.0) + margin_bottom
        }
        // No in-flow line box, or overflow != visible: the baseline IS the
        // bottom margin edge, so the box has no descent below it.
        _ => margin_bottom,
    }
}

/// Measure one ATOMIC inline-level box of the IFC `constraints` lays out (an
/// inline-block, an inline-flex / -grid / -table box): its used border-box
/// size from its own CSS (`calculate_used_size_for_node` against the IFC
/// root's content box, [`atomic_inline_containing_block`]), its contents laid
/// out to find its height and baseline, its used size stored in the tree.
/// Returns the margin-box shape the line layout places; the caller pushes it
/// and maps its content index to `child_index`, so the box is positioned.
///
/// THE one measurement for every place an IFC meets an atomic inline: a child
/// of the IFC root, of an anonymous IFC wrapper, and one nested in inline
/// spans. (Three copies had drifted: the span one sized the box from its
/// max-content width alone - no width, height, padding or border - and never
/// positioned it; the anonymous-wrapper one resolved the box's percentages
/// against the root's own containing block.)
pub(super) fn measure_atomic_inline<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    child_index: usize,
    dom_id: NodeId,
    constraints: &LayoutConstraints<'_>,
) -> Result<InlineShape> {
    // The intrinsic sizing pass has already calculated its preferred size.
    let intrinsic_size = tree
        .warm(LayoutNodeId::new(child_index))
        .and_then(|w| w.intrinsic_sizes)
        .unwrap_or_default();
    let box_props = tree
        .get(LayoutNodeId::new(child_index))
        .ok_or(LayoutError::InvalidTree)?
        .box_props
        .unpack();

    let styled_node_state = ctx
        .styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|n| n.styled_node_state)
        .unwrap_or_default();

    // Calculate tentative border-box size based on CSS properties
    // This correctly handles explicit width/height, box-sizing, and constraints
    let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
        ctx.styled_dom,
        Some(dom_id),
        &CBTY::from_flattened_with_width_type(
            atomic_inline_containing_block(constraints),
            constraints.available_width_type,
        ),
        intrinsic_size,
        &box_props,
        &ctx.viewport_size,
    )?;

    let writing_mode =
        get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default();

    // Determine content-box size for laying out children
    let content_box_size = box_props.inner_size(tentative_size, writing_mode);

    debug_info!(
        ctx,
        "[measure_atomic_inline] Inline-block NodeId({:?}): tentative_border_box={:?}, \
         content_box={:?}",
        dom_id,
        tentative_size,
        content_box_size
    );

    // The box's contents are laid out in the size just resolved for it - as
    // `cache::calculate_layout_for_subtree` does for every other box (its
    // "Phase 1.5"). What `used_size` held here was an earlier measurement -
    // the previous pass's, on a node the reconcile carried over
    // (`clone_node_from_old` keeps it), or this pass's under another
    // constraint - and the layouts below read a set one as decided: a flex /
    // grid container as its definite width (`layout_flex_grid`), a table as
    // its width, a block as its children's containing block. An
    // `inline-flex` button whose label grew was laid out at its OLD width and
    // wrapped the new label onto a second line. The measured height is set
    // below.
    if let Some(node) = tree.get_mut(LayoutNodeId::new(child_index)) {
        node.used_size = Some(tentative_size);
    }

    // To find its height and baseline, we must lay out its contents.
    let child_wm_ctx = super::super::geometry::WritingModeContext::new(
        writing_mode,
        get_direction_property(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default(),
        get_text_orientation_property(ctx.styled_dom, dom_id, &styled_node_state)
            .unwrap_or_default(),
    );
    let child_constraints = LayoutConstraints {
        available_size: LogicalSize::new(content_box_size.width, f32::INFINITY),
        writing_mode,
        writing_mode_ctx: child_wm_ctx,
        // Inline-blocks establish a new BFC, so no state is passed in.
        bfc_state: None,
        // Does not affect size/baseline of the container.
        text_align: TextAlign::Start,
        containing_block_size: atomic_inline_containing_block(constraints),
        available_width_type: Text3AvailableSpace::Definite(content_box_size.width),
        fragmentainer: None,
        column_flow: None,
    };

    // Recursively lay out the inline-block to get its final height and baseline.
    // Note: This does not affect its final position, only its dimensions.
    let mut empty_float_cache = HashMap::new();
    let layout_result = layout_formatting_context(
        ctx,
        tree,
        text_cache,
        child_index,
        &child_constraints,
        &mut empty_float_cache,
    )?;

    publish_interior_positions(tree, &layout_result.output);
    let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);

    // Replaced elements (image / VirtualView) have no flow content, so the
    // measured content_height is 0 — treat their auto height like an explicit
    // height (use the CSS/intrinsic-resolved tentative_size). Fixes 0-height
    // images / VirtualViews laid out as atomic inline-blocks.
    let is_replaced_atomic = {
        let nd = &ctx.styled_dom.node_data.as_container()[dom_id];
        matches!(nd.get_node_type(), NodeType::Image(_)) || nd.is_virtual_view_node()
    };
    // A percentage height against the IFC's indefinite block size computes to
    // `auto` (CSS 2.2 10.5): as tall as the content, like an `auto` height -
    // `tentative_size` holds only the sizing estimate for it (AzMail's
    // `height: 100%` paper ended hundreds of px above the mail's end).
    let percentage_is_auto = crate::solver3::sizing::percentage_height_computes_to_auto(
        css_height.as_exact(),
        atomic_inline_containing_block(constraints)
            .height
            .is_finite(),
    );
    let height_is_auto =
        percentage_is_auto || matches!(css_height.clone().unwrap_or_default(), LayoutHeight::Auto);
    // Determine final border-box height
    let final_height = if height_is_auto && !is_replaced_atomic { atomic_inline_auto_height(
        tree.get(LayoutNodeId::new(child_index))
            .map(|n| n.formatting_context),
        tree.get(LayoutNodeId::new(child_index))
            .and_then(|n| n.used_size)
            .map(|s| s.height),
        layout_result.output.overflow_size.height,
        box_props.padding.main_sum(writing_mode) + box_props.border.main_sum(writing_mode),
    ) } else { tentative_size.height };

    debug_info!(
        ctx,
        "[measure_atomic_inline] Inline-block NodeId({:?}): layout_content_height={}, \
         css_height={:?}, final_border_box_height={}",
        dom_id,
        layout_result.output.overflow_size.height,
        css_height,
        final_height
    );

    let final_size = LogicalSize::new(tentative_size.width, final_height);

    // Update the node in the tree with its now-known used size.
    if let Some(node) = tree.get_mut(LayoutNodeId::new(child_index)) {
        node.used_size = Some(final_size);
    }

    // CSS 2.2 s 10.8.1, via `atomic_inline_baseline_offset`. Its `overflow`
    // rule (a clipping box sits on its bottom margin edge) is the
    // inline-block's alone: a flex or grid box keeps its first item's
    // baseline whatever its overflow (Chrome: an `overflow: hidden`
    // inline-flex button sits on its label's baseline), so its overflow is
    // read as `visible` here.
    let baseline_ignores_overflow = tree.get(LayoutNodeId::new(child_index)).is_some_and(|n| {
        matches!(
            n.formatting_context,
            FormattingContext::Flex | FormattingContext::Grid
        )
    });
    let overflow_x = if baseline_ignores_overflow {
        LayoutOverflow::Visible
    } else {
        get_overflow_x(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default()
    };
    let overflow_y = if baseline_ignores_overflow {
        LayoutOverflow::Visible
    } else {
        get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default()
    };
    let overflow_is_visible = matches!(
        (overflow_x, overflow_y),
        (LayoutOverflow::Visible, LayoutOverflow::Visible)
    );
    // An inline-table's baseline is its first row's and an inline-flex /
    // -grid box's its first item's (their own layouts report them); an
    // inline-block's is its last line box (`inline_block_baseline`, from the
    // border box's top - `layout_bfc` reports none, `layout_ifc` only the raw
    // ascent of its last item).
    let content_box_top = box_props.padding.top + box_props.border.top;
    let baseline_from_top = match tree
        .get(LayoutNodeId::new(child_index))
        .map(|n| n.formatting_context)
    {
        Some(FormattingContext::Table | FormattingContext::Flex | FormattingContext::Grid) => {
            layout_result.output.baseline
        }
        _ => inline_block_baseline(child_index, tree, 0)
            .map(|from_border_box_top| from_border_box_top - content_box_top),
    };
    let baseline_offset = atomic_inline_baseline_offset(
        baseline_from_top,
        final_height,
        content_box_top,
        box_props.margin.bottom,
        overflow_is_visible,
    );

    debug_info!(
        ctx,
        "[measure_atomic_inline] Inline-block NodeId({:?}): baseline_from_top={:?}, \
         final_height={}, baseline_offset_from_bottom={}",
        dom_id,
        baseline_from_top,
        final_height,
        baseline_offset
    );

    // +spec:box-model:66ad24 - inline-axis margins, borders, padding respected for
    // inline-level boxes (no collapsing). "The box used for alignment is the
    // margin box": text3 positions the margin box, so the spacing is kept.
    let margin = &box_props.margin;
    let margin_box_width = final_size.width + margin.left + margin.right;
    let margin_box_height = final_size.height + margin.top + margin.bottom;

    Ok(InlineShape {
        shape_def: ShapeDefinition::Rectangle {
            size: crate::text3::cache::Size {
                // Use margin-box size for positioning in inline flow
                width: margin_box_width,
                height: margin_box_height,
            },
            corner_radius: None,
        },
        fill: None,
        stroke: None,
        // Already measured from the margin box's bottom edge.
        baseline_offset,
        alignment: crate::solver3::getters::get_vertical_align_for_node(
            ctx.styled_dom,
            dom_id,
            PhysicalSize::new(ctx.viewport_size.width, ctx.viewport_size.height),
        ),
        source_node_id: Some(dom_id),
    })
}
