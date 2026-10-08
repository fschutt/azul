//! Flex and grid containers, laid out by Taffy.

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

// Flex / grid layout (taffy Bridge)
// containing block determined by grid-placement properties; Taffy handles this internally
// (grid auto-placement §8.5 and abspos grid items use grid-area CB, not just padding box)

/// Lays out a Flex or Grid formatting context using the Taffy layout engine.
///
/// # CSS Spec References
///
/// - CSS Flexbox § 9: Flex Layout Algorithm
/// - CSS Grid § 12: Grid Layout Algorithm
// gutters on either side of collapsed tracks collapse including distributed alignment space,
// minimum contribution = outer size from min-width/min-height if specified size is auto else
// min-content contribution) — all handled by Taffy grid implementation
///
/// # Implementation Notes
///
/// - Resolves explicit CSS dimensions to pixel values for `known_dimensions`
/// - Uses `InherentSize` mode when explicit dimensions are set
/// - Uses `ContentSize` mode for auto-sizing (shrink-to-fit)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub(super) fn layout_flex_grid<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints<'_>,
) -> Result<BfcLayoutResult> {
    // Available space comes directly from constraints - margins are handled by Taffy.
    //
    // `available_size` encodes "indefinite" as INFINITY (see the text/inline
    // constraint plumbing). That must NOT reach taffy as `Definite(inf)`: taffy
    // treats a definite value as a real length — a multi-line wrap container
    // takes `max(content, inf)` as its main size, a grid item stretches to
    // `inf.max(content)` in the bridge — and its layout cache compares keys
    // with `abs(a - b) < EPSILON`, where `inf - inf` is NaN, so an entry with
    // an infinite key never hits and every pass re-measures. That NaN is also
    // how the Pascal hello-world died: the FPC runtime traps InvalidOp, the
    // subtraction raised it inside taffy on the first layout. Indefinite is
    // `MaxContent` in taffy's vocabulary.
    let to_taffy = |v: f32| {
        if v.is_finite() {
            AvailableSpace::Definite(v)
        } else {
            AvailableSpace::MaxContent
        }
    };
    let available_space = TaffySize {
        width: to_taffy(constraints.available_size.width),
        height: to_taffy(constraints.available_size.height),
    };

    let node = tree
        .get(LayoutNodeId::new(node_index))
        .ok_or(LayoutError::InvalidTree)?;

    // from flex line's cross size (clamped by min/max) when align-self:stretch, cross-size:auto,
    // and neither cross-axis margin is auto. Otherwise uses hypothetical cross size.
    // NOTE: visibility:collapse strut size for flex items is handled internally by Taffy.
    //
    // Resolve explicit CSS dimensions to pixel values.
    // This is CRITICAL for align-items: stretch to work correctly!
    // Taffy uses known_dimensions to calculate cross_axis_available_space for children.
    let (explicit_width, has_explicit_width) =
        definite_or_auto(resolve_explicit_dimension_width(ctx, node, constraints));
    let (explicit_height, has_explicit_height) =
        definite_or_auto(resolve_explicit_dimension_height(ctx, node, constraints));

    // FIX: For root nodes or nodes where the parent provides a definite size,
    // use the available_size as known_dimensions if no explicit CSS width/height is set.
    // This is critical for `align-self: stretch` to work - Taffy needs to know the
    // cross-axis size of the container to stretch children to fill it.
    let is_root = node.parent.is_none();

    let bp = node.box_props.unpack();
    let width_adjustment = bp.border.left + bp.border.right + bp.padding.left + bp.padding.right;
    let height_adjustment = bp.border.top + bp.border.bottom + bp.padding.top + bp.padding.bottom;

    // `constraints.available_size` is the root's CONTENT-BOX (produced by
    // `prepare_layout_context::inner_size(final_used_size)`), not the viewport
    // border-box. Previously, the code used it as if it were border-box,
    // causing taffy to subtract padding a second time and shrink the content
    // area by 2x padding. For the root, pull the actual border-box from
    // `node.used_size` (set by `calculate_used_size_for_node` before this call).
    let root_border_box = node.used_size;

    let effective_width = if has_explicit_width {
        explicit_width
    } else if is_root {
        root_border_box.as_ref().map(|s| s.width).or_else(|| {
            if constraints.available_size.width.is_finite() {
                // Fallback: convert content-box to border-box.
                Some(constraints.available_size.width + width_adjustment)
            } else {
                None
            }
        })
    } else {
        // Non-root flex/grid container with `width: auto`: for a block-level
        // child the parent's block layout has ALREADY resolved the used width
        // (auto → fill containing block) before descending into this FC — pass
        // it through as the definite border-box width, exactly like the root
        // branch does with its used_size. Without this, known_dimensions.width
        // stays None and taffy treats a column container's cross axis as
        // INDEFINITE, so `align-items: stretch` items get the flex line's
        // max-content width instead of the container width (live bug: under
        // the injected Html menubar wrapper, AzulPaint's body laid out its
        // header AND canvas at 315.776px — the header text's max-content —
        // instead of the body's 624px).
        node.used_size.as_ref().map(|s| s.width)
    };
    let effective_height = if has_explicit_height {
        explicit_height
    } else if is_root {
        match root_border_box.as_ref().map(|s| s.height) {
            // An auto-height root's `used_size` is content-derived and is
            // still ZERO here (children have not been laid out yet).
            // Handing that to taffy as a DEFINITE main size makes a column
            // flex container think it has -60px of free space, so every
            // item with the default `flex-shrink: 1` collapses to 0 — a
            // fixed-height toolbar under `body { display: flex;
            // flex-direction: column }` simply vanished.
            //
            // CSS Flexbox §9.7: a container whose main size is INDEFINITE
            // sizes items to their hypothetical main size and performs no
            // shrinking. Leaving the dimension unknown is what makes taffy
            // content-size the container.
            //
            // A POSITIVE used height is no better: it is the sizing
            // pre-pass's estimate of the content, which leaves out the
            // root's own padding and border - taken as the border box, it
            // squeezed the items into the rest (a body column with 8px
            // padding handed its 32px text field 16px, and the field fell
            // back to its 22px min-height; an app's 24px font never grew it).
            Some(_) => None,
            None => {
                if constraints.available_size.height.is_finite() {
                    Some(constraints.available_size.height + height_adjustment)
                } else {
                    None
                }
            }
        }
    } else {
        // Non-root height pass-through, mirroring the width arm above,
        // but ONLY for absolutely/fixed-positioned containers: their
        // used height was resolved by the §10.6.4 equations (stretch-fit
        // between insets) BEFORE this run — a >0 value is the DEFINITE
        // containing-block size, exactly like the width case. Without
        // this, `inset:0; display:flex; align-items:center` centered
        // within the CONTENT height (taffy re-derived the main size and
        // clobbered the solved stretch-fit — miniword ENGINE-ISSUE 5a).
        // In-flow auto-height containers stay None (content-sized;
        // their used_size may hold a stale height on warm re-layouts).
        let is_abs = matches!(
            get_position_type(ctx.styled_dom, node.dom_node_id,),
            LayoutPosition::Absolute | LayoutPosition::Fixed
        );
        match (is_abs, node.used_size.as_ref().map(|s| s.height)) {
            (true, Some(h)) if h > 0.0 => Some(h),
            _ => None,
        }
    };
    let has_effective_width = effective_width.is_some();
    let has_effective_height = effective_height.is_some();

    // Taffy interprets known_dimensions as border-box. CSS width/height default
    // to content-box, so explicit values need +padding+border added. For the
    // ROOT element, however, we auto-apply box-sizing: border-box — the common
    // CSS reset pattern — so `height:100%` + padding fits the viewport instead
    // of overflowing by padding (which the default content-box interpretation
    // would produce, since 100% of ICB is viewport-sized content, with padding
    // added outside pushing border-box past the viewport).
    let adjusted_width = if has_explicit_width && !is_root {
        explicit_width.map(|w| w + width_adjustment)
    } else if has_explicit_width && is_root {
        explicit_width
    } else {
        effective_width
    };
    let adjusted_height = if has_explicit_height && !is_root {
        explicit_height.map(|h| h + height_adjustment)
    } else if has_explicit_height && is_root {
        explicit_height
    } else {
        effective_height
    };

    // CSS Flexbox § 9.2: Use InherentSize when explicit dimensions are set,
    // ContentSize for auto-sizing (shrink-to-fit behavior).
    let sizing_mode = if has_effective_width || has_effective_height {
        taffy::SizingMode::InherentSize
    } else {
        taffy::SizingMode::ContentSize
    };

    let known_dimensions = TaffySize {
        width: adjusted_width,
        height: adjusted_height,
    };

    // parent_size tells Taffy the size of the container's parent.
    // For root nodes, the "parent" is the viewport, but since margins are already
    // handled by calculate_used_size_for_node(), we use containing_block_size directly.
    // For non-root nodes, containing_block_size is already the parent's content-box.
    let parent_size = translate_taffy_size(constraints.containing_block_size);

    let taffy_inputs = LayoutInput {
        known_dimensions,
        parent_size,
        available_space,
        run_mode: taffy::RunMode::PerformLayout,
        sizing_mode,
        axis: taffy::RequestedAxis::Both,
        // Flex and Grid containers establish a new BFC, preventing margin collapse.
        vertical_margins_are_collapsible: Line::FALSE,
    };

    debug_info!(
        ctx,
        "CALLING LAYOUT_TAFFY FOR FLEX/GRID FC node_index={:?}",
        node_index
    );

    // For the root with auto-applied border-box: sync node.used_size so
    // display-list rendering matches the border-box we handed taffy.
    // Without this, the root's background/border would paint at the
    // inflated size from calculate_used_size_for_node while taffy placed
    // children inside a smaller content-box.
    if is_root {
        if let (Some(aw), Some(ah)) = (adjusted_width, adjusted_height) {
            if let Some(node_mut) = tree.get_mut(LayoutNodeId::new(node_index)) {
                node_mut.used_size = Some(LogicalSize::new(aw, ah));
            }
        }
    }

    // Cache border values before the mutable borrow in layout_taffy_subtree
    let border_left = bp.border.left;
    let border_top = bp.border.top;
    let padding_top = bp.padding.top;

    let taffy_output =
        taffy_bridge::layout_taffy_subtree(ctx, tree, text_cache, node_index, taffy_inputs);

    // Adopt taffy's computed container border-box as this node's used_size. This is
    // the height/width taffy actually laid the tracks/lines into. The auto-height
    // path (cache.rs apply_content_based_height) otherwise derives the container
    // height from taffy's `content_size`, but for a GRID that field measures each
    // item relative to its OWN grid area (≈ the item's own height, blind to which
    // row it sits in), so a multi-row grid collapsed to a single row's height
    // (a 2×2 grid reported 16px instead of 42px). `taffy_output.size` is the correct
    // row/line sum + gaps. Flex's `output.size` already equals the correct container
    // size, so this is a no-op there. The root syncs its own used_size above.
    if !is_root {
        let container_bb = translate_taffy_size_back(taffy_output.size);
        if let Some(node_mut) = tree.get_mut(LayoutNodeId::new(node_index)) {
            node_mut.used_size = Some(container_bb);
        }
    }

    // Collect child positions from the tree (Taffy stores results directly on nodes).
    let mut output = LayoutOutput::default();
    // Use content_size for overflow detection, not container size.
    // content_size represents the actual size of all children, which may exceed the container.
    //
    // Taffy's content_size is measured from (0,0) of the border-box, so it includes
    // border.top/left as a leading offset.  The scrollbar geometry and scroll clamp
    // both measure inside the padding-box (border stripped).  Subtract the start
    // border so that overflow_size is in the same coordinate space as the viewport
    // (padding-box), preventing extra scroll range equal to the border width.
    let raw = translate_taffy_size_back(taffy_output.content_size);
    output.overflow_size = LogicalSize::new(
        (raw.width - border_left).max(0.0),
        (raw.height - border_top).max(0.0),
    );

    let children: Vec<usize> = tree.children(node_index).to_vec();
    for &child_idx in &children {
        if let Some(warm_node) = tree.warm(LayoutNodeId::new(child_idx)) {
            if let Some(pos) = warm_node.relative_position {
                output.positions.insert(child_idx, pos);
            }
        }
    }

    // A flex / grid container's first baseline is its first item's (CSS
    // Flexbox 8.5, Grid 10.6): the first line box laid out in it, at any
    // depth (`first_line_baseline`, the table cells' walk). The atomic-inline
    // path reads it for an inline-flex / -grid box; this layout reported
    // none, so every inline-flex button sat on its line by its bottom edge -
    // a 32px button beside text made a 36px line (Chrome 32), and alone on a
    // line it hung the strut's descent below itself. Content-box relative,
    // like every `LayoutOutput::baseline`.
    output.baseline = first_line_baseline(node_index, tree, 0)
        .map(|from_border_top| from_border_top - border_top - padding_top);

    Ok(BfcLayoutResult::from_output(output))
}

/// Resolves explicit CSS width to pixel value for Taffy layout.
/// Axis selector for `border_box_to_content`.
#[derive(Clone, Copy)]
pub(super) enum Axis {
    Width,
    Height,
}

/// Convert a resolved explicit CSS dimension to the CONTENT-box value the
/// `known_dimensions` pipeline expects.
///
/// The flex/grid `known_dimensions` code resolves the explicit CSS dimension and
/// then unconditionally re-adds border+padding to reach taffy's border-box (see
/// `adjusted_width`/`adjusted_height`). That is correct only when the resolved
/// value is a content-box measurement. For `box-sizing:border-box`, an ABSOLUTE
/// length (px/em/calc) IS already the border-box size, so re-adding border+padding
/// double-counts it (a `height:100px; border:5px` container came out 110px instead
/// of 100). Subtract the axis border+padding here so the caller's re-add restores
/// the intended border-box. Percentages already resolve against the content-box
/// available size, so they are left unchanged.
pub(super) fn border_box_to_content<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
    id: NodeId,
    node_state: &StyledNodeState,
    resolved: f32,
    is_percentage: bool,
    axis: Axis,
) -> f32 {
    if is_percentage {
        return resolved;
    }
    let is_border_box = matches!(
        get_css_box_sizing(ctx.styled_dom, id, node_state),
        MultiValue::Exact(azul_css::props::layout::LayoutBoxSizing::BorderBox)
    );
    if !is_border_box {
        return resolved;
    }
    let bp = node.box_props.unpack();
    let adjustment = match axis {
        Axis::Width => bp.border.left + bp.border.right + bp.padding.left + bp.padding.right,
        Axis::Height => bp.border.top + bp.border.bottom + bp.padding.top + bp.padding.bottom,
    };
    (resolved - adjustment).max(0.0)
}

/// An explicit size that came out non-finite is `auto`: it resolved a
/// percentage (or a `calc()` with percent terms) against the INDEFINITE basis a
/// measurement pass carries as `INFINITY` in `available_size` - a flex basis
/// or a row's cross size measured on a block that holds a `height: 100%`
/// flex container. CSS 2.2 10.5 / css-sizing-3 5.2.1: such a percentage
/// behaves as `auto`, so the container is content-sized; handed to taffy as a
/// known size, the infinity became the height of every box above it (the
/// `OfficeShell` chain of `AzNews` / `AzCode`, blank screenshots). The same net
/// `calculate_used_size_for_node` keeps for a non-finite width.
pub(super) fn definite_or_auto((size, explicit): (Option<f32>, bool)) -> (Option<f32>, bool) {
    match size {
        Some(px) if !px.is_finite() => (None, false),
        _ => (size, explicit),
    }
}

pub(super) fn resolve_explicit_dimension_width<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
    constraints: &LayoutConstraints<'_>,
) -> (Option<f32>, bool) {
    node.dom_node_id.map_or((None, false), |id| {
        let width = get_css_width(
            ctx.styled_dom,
            id,
            &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
        );
        match width.unwrap_or_default() {
            LayoutWidth::Auto
            | LayoutWidth::MinContent
            | LayoutWidth::MaxContent
            | LayoutWidth::FitContent(_) => (None, false),
            LayoutWidth::Px(px) => {
                let node_state = &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state;
                let pixels = resolve_size_metric(
                    px.metric,
                    px.number.get(),
                    constraints.available_size.width,
                    ctx.viewport_size,
                    get_element_font_size(ctx.styled_dom, id, node_state),
                    get_root_font_size(ctx.styled_dom, node_state),
                );
                // CSS `zoom` scales an absolute length (LAYOUT7).
                let pixels =
                    crate::solver3::getters::zoomed_length(ctx.styled_dom, id, px.metric, pixels);
                let content_px = border_box_to_content(
                    ctx,
                    node,
                    id,
                    node_state,
                    pixels,
                    px.metric == SizeMetric::Percent,
                    Axis::Width,
                );
                (Some(content_px), true)
            }
            LayoutWidth::Calc(items) => {
                let node_state = &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state;
                let em = get_element_font_size(ctx.styled_dom, id, node_state);
                let calc_ctx = super::super::calc::CalcResolveContext {
                    items,
                    em_size: em,
                    rem_size: DEFAULT_FONT_SIZE,
                };
                let px = super::super::calc::evaluate_calc(&calc_ctx, constraints.available_size.width);
                let content_px =
                    border_box_to_content(ctx, node, id, node_state, px, false, Axis::Width);
                (Some(content_px), true)
            }
        }
    })
}

/// Resolves explicit CSS height to pixel value for Taffy layout.
pub(super) fn resolve_explicit_dimension_height<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNodeHot,
    constraints: &LayoutConstraints<'_>,
) -> (Option<f32>, bool) {
    node.dom_node_id.map_or((None, false), |id| {
        let height = get_css_height(
            ctx.styled_dom,
            id,
            &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
        );
        match height.unwrap_or_default() {
            LayoutHeight::Auto
            | LayoutHeight::MinContent
            | LayoutHeight::MaxContent
            | LayoutHeight::FitContent(_) => (None, false),
            LayoutHeight::Px(px) => {
                let node_state = &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state;
                let pixels = resolve_size_metric(
                    px.metric,
                    px.number.get(),
                    constraints.available_size.height,
                    ctx.viewport_size,
                    get_element_font_size(ctx.styled_dom, id, node_state),
                    get_root_font_size(ctx.styled_dom, node_state),
                );
                // CSS `zoom` scales an absolute length (LAYOUT7).
                let pixels =
                    crate::solver3::getters::zoomed_length(ctx.styled_dom, id, px.metric, pixels);
                // box-sizing:border-box + an ABSOLUTE length is a border-box
                // value; the caller re-adds border+padding to reach the
                // taffy border-box, so convert to content-box here to avoid
                // double-counting. (Percentages already resolve against the
                // content-box available size, so leave those alone.)
                let content_px = border_box_to_content(
                    ctx,
                    node,
                    id,
                    node_state,
                    pixels,
                    px.metric == SizeMetric::Percent,
                    Axis::Height,
                );
                (Some(content_px), true)
            }
            LayoutHeight::Calc(items) => {
                let node_state = &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state;
                let em = get_element_font_size(ctx.styled_dom, id, node_state);
                let calc_ctx = super::super::calc::CalcResolveContext {
                    items,
                    em_size: em,
                    rem_size: DEFAULT_FONT_SIZE,
                };
                let px = super::super::calc::evaluate_calc(&calc_ctx, constraints.available_size.height);
                let content_px =
                    border_box_to_content(ctx, node, id, node_state, px, false, Axis::Height);
                (Some(content_px), true)
            }
        }
    })
}

/// A containing-block size as taffy's `parent_size`. A non-finite side is the
/// measurement passes' "indefinite" (`INFINITY` in `available_size`), not a
/// size: handed to taffy as `Some(inf)`, a `height: 100%` resolved to an
/// infinite height and spread up the `OfficeShell` chain (`AzNews` / `AzCode`
/// painted blank). CSS 2.2 10.5: a percentage of an indefinite height is
/// `auto` - taffy's `None`.
pub(super) const fn translate_taffy_size(size: LogicalSize) -> TaffySize<Option<f32>> {
    TaffySize {
        width: if size.width.is_finite() { Some(size.width) } else { None },
        height: if size.height.is_finite() { Some(size.height) } else { None },
    }
}

/// Resolves a CSS size metric to pixels.
///
/// - `metric`: The CSS unit (px, pt, em, vw, etc.)
/// - `value`: The numeric value
/// - `containing_block_size`: Size of containing block (for percentage)
/// - `viewport_size`: Viewport dimensions (for vw, vh, vmin, vmax)
/// - `element_font_size`: The element's own computed font-size (for `em`)
/// - `root_font_size`: The root element's computed font-size (for `rem`)
#[inline]
pub(super) fn resolve_size_metric(
    metric: SizeMetric,
    value: f32,
    containing_block_size: f32,
    viewport_size: LogicalSize,
    element_font_size: f32,
    root_font_size: f32,
) -> f32 {
    match metric {
        SizeMetric::Px => value,
        SizeMetric::Pt => value * PT_TO_PX,
        SizeMetric::Percent => value / 100.0 * containing_block_size,
        SizeMetric::Em => value * element_font_size,
        SizeMetric::Rem => value * root_font_size,
        SizeMetric::Vw => value / 100.0 * viewport_size.width,
        SizeMetric::Vh => value / 100.0 * viewport_size.height,
        SizeMetric::Vmin => value / 100.0 * viewport_size.width.min(viewport_size.height),
        SizeMetric::Vmax => value / 100.0 * viewport_size.width.max(viewport_size.height),
        SizeMetric::In => value * super::super::calc::PX_PER_INCH,
        SizeMetric::Cm => value * super::super::calc::PX_PER_INCH / super::super::calc::CM_PER_INCH,
        SizeMetric::Mm => value * super::super::calc::PX_PER_INCH / super::super::calc::MM_PER_INCH,
    }
}

#[must_use]
pub const fn translate_taffy_size_back(size: TaffySize<f32>) -> LogicalSize {
    LogicalSize {
        width: size.width,
        height: size.height,
    }
}

#[must_use]
pub const fn translate_taffy_point_back(point: taffy::Point<f32>) -> LogicalPosition {
    LogicalPosition {
        x: point.x,
        y: point.y,
    }
}
