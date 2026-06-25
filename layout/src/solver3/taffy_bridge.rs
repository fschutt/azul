//! Bridge between Azul's CSS style system and the Taffy layout engine.
//!
//! This module translates Azul CSS properties into Taffy's `Style` struct and
//! implements Taffy's `TraversePartialTree`, `LayoutPartialTree`, `CacheTree`,
//! `LayoutFlexboxContainer`, and `LayoutGridContainer` traits via the
//! [`TaffyBridge`] struct. The main entry point is [`layout_taffy_subtree`],
//! which is called from `fc.rs` when a flex or grid formatting context is
//! encountered during layout.

use crate::solver3::calc::CalcResolveContext;
use crate::solver3::getters::{get_overflow_x, get_overflow_y};
use azul_core::dom::FormattingContext;
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            PixelValue, SizeMetric,
        },
        layout::{
            dimensions::CalcAstItemVec,
            flex::LayoutFlexBasis,
            grid::{GridAutoTracks, GridTemplate, GridTrackSizing},
            LayoutAlignContent, LayoutAlignItems, LayoutAlignSelf, LayoutDisplay,
            LayoutFlexDirection, LayoutFlexWrap, LayoutGridAutoFlow, LayoutJustifyContent,
            LayoutPosition, LayoutWritingMode,
        },
        property::{
            LayoutAlignContentValue, LayoutAlignItemsValue, LayoutAlignSelfValue,
            LayoutDisplayValue, LayoutFlexDirectionValue, LayoutFlexWrapValue,
            LayoutGridAutoColumnsValue, LayoutGridAutoFlowValue, LayoutGridAutoRowsValue,
            LayoutGridTemplateColumnsValue, LayoutGridTemplateRowsValue, LayoutJustifyContentValue,
            LayoutPositionValue,
        },
    },
};
use taffy::style::{MaxTrackSizingFunction, MinTrackSizingFunction, TrackSizingFunction};

/// CSS reference pixels per inch (96 dpi per CSS Values spec).
const CSS_PX_PER_INCH: f32 = 96.0;

/// Convert `PixelValue` to pixels, only for absolute units (no %, and em/rem use fallback)
/// Used where proper resolution context is not available (grid tracks, etc.)
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn pixel_value_to_pixels_fallback(pv: &PixelValue) -> Option<f32> {
    match pv.metric {
        SizeMetric::Px => Some(pv.number.get()),
        SizeMetric::Pt => Some(pv.number.get() * PT_TO_PX),
        SizeMetric::In => Some(pv.number.get() * CSS_PX_PER_INCH),
        SizeMetric::Cm => Some(pv.number.get() * CSS_PX_PER_INCH / 2.54),
        SizeMetric::Mm => Some(pv.number.get() * CSS_PX_PER_INCH / 25.4),
        // For em/rem, use DEFAULT_FONT_SIZE as fallback (not ideal but needed without context)
        SizeMetric::Em | SizeMetric::Rem => Some(pv.number.get() * DEFAULT_FONT_SIZE),
        SizeMetric::Percent => None, // Cannot resolve without containing block
        // Viewport units: Cannot resolve without viewport context
        SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => None,
    }
}

/// Converts an Azul `grid-template-rows` value into Taffy grid template components.
fn grid_template_rows_to_taffy(
    val: LayoutGridTemplateRowsValue,
) -> Vec<GridTemplateComponent<String>> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks
        .tracks
        .iter()
        .map(|track| GridTemplateComponent::Single(translate_track(track)))
        .collect()
}

/// Converts an Azul `grid-template-columns` value into Taffy grid template components.
fn grid_template_columns_to_taffy(
    val: LayoutGridTemplateColumnsValue,
) -> Vec<GridTemplateComponent<String>> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks
        .tracks
        .iter()
        .map(|track| GridTemplateComponent::Single(translate_track(track)))
        .collect()
}

/// Converts an Azul `grid-auto-rows` value into Taffy min/max track sizing pairs.
fn grid_auto_rows_to_taffy(
    val: LayoutGridAutoRowsValue,
) -> Vec<taffy::MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    let tracks = auto_tracks.tracks;
    tracks
        .iter()
        .map(|track| taffy::MinMax {
            min: translate_track(track).min,
            max: translate_track(track).max,
        })
        .collect()
}

/// Converts an Azul `grid-auto-columns` value into Taffy track sizing functions.
fn grid_auto_columns_to_taffy(
    val: LayoutGridAutoColumnsValue,
) -> Vec<taffy::TrackSizingFunction> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks.tracks.iter().map(translate_track).collect()
}

#[allow(clippy::cast_precision_loss)] // bounded layout/render numeric cast
fn translate_track(track: &GridTrackSizing) -> taffy::TrackSizingFunction {
    // Helper to resolve PixelValue to absolute pixels (handles em, rem, but not %)
    // Grid track sizing in Taffy doesn't support % - only absolute values
    let px_to_float = |pv: PixelValue| -> f32 {
        pixel_value_to_pixels_fallback(&pv).unwrap_or(0.0)
    };

    match track {
        GridTrackSizing::MinContent => minmax(
            taffy::MinTrackSizingFunction::min_content(),
            taffy::MaxTrackSizingFunction::min_content(),
        ),
        GridTrackSizing::MaxContent => minmax(
            taffy::MinTrackSizingFunction::max_content(),
            taffy::MaxTrackSizingFunction::max_content(),
        ),
        GridTrackSizing::MinMax(minmax_box) => minmax(
            translate_track(&minmax_box.min).min,
            translate_track(&minmax_box.max).max,
        ),
        GridTrackSizing::Fixed(px) => {
            // Fixed tracks: resolve em/rem to pixels
            // Note: % is not supported in grid track sizing (CSS Grid spec)
            let pixels = px_to_float(*px);
            minmax(
                taffy::MinTrackSizingFunction::length(pixels),
                taffy::MaxTrackSizingFunction::length(pixels),
            )
        }
        GridTrackSizing::Fr(fr) => {
            // Fr units: minmax(auto, Xfr) per CSS Grid spec
            // The min is auto, max is the fractional value
            // fr is stored as i32 * 100 (e.g., 1fr = 100, 2fr = 200)
            minmax(
                taffy::MinTrackSizingFunction::auto(),
                taffy::MaxTrackSizingFunction::fr(*fr as f32 / 100.0),
            )
        }
        GridTrackSizing::Auto => minmax(
            taffy::MinTrackSizingFunction::min_content(),
            taffy::MaxTrackSizingFunction::max_content(),
        ),
        GridTrackSizing::FitContent(px) => {
            // fit-content: resolve em/rem to pixels
            let pixels = px_to_float(*px);
            minmax(
                taffy::MinTrackSizingFunction::length(pixels),
                taffy::MaxTrackSizingFunction::max_content(),
            )
        }
    }
}

const fn minmax(min: MinTrackSizingFunction, max: MaxTrackSizingFunction) -> taffy::TrackSizingFunction {
    TrackSizingFunction { min, max }
}

fn layout_display_to_taffy(val: LayoutDisplayValue) -> Display {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutDisplay::None => Display::None,
        LayoutDisplay::Flex | LayoutDisplay::InlineFlex => Display::Flex,
        LayoutDisplay::Grid | LayoutDisplay::InlineGrid => Display::Grid,
        _ => Display::Block,
    }
}

// to determine their CB; Taffy's Position::Absolute handles this for both flex and grid
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn layout_position_to_taffy(val: LayoutPositionValue) -> Position {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutPosition::Absolute => Position::Absolute,
        LayoutPosition::Fixed => Position::Absolute, // Taffy has no Fixed variant
        LayoutPosition::Relative => Position::Relative,
        LayoutPosition::Static => Position::Relative,
        LayoutPosition::Sticky => Position::Relative, // Sticky treated as Relative
    }
}

#[allow(clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn decode_compact_grid_line(v: i16) -> GridPlacement<String> {
    if v == azul_css::compact_cache::I16_AUTO || v == azul_css::compact_cache::I16_SENTINEL {
        GridPlacement::Auto
    } else if v < 0 {
        GridPlacement::<String>::from_span((-v) as u16)
    } else {
        GridPlacement::<String>::from_line_index(v)
    }
}

fn grid_auto_flow_to_taffy(val: LayoutGridAutoFlowValue) -> GridAutoFlow {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutGridAutoFlow::Row => GridAutoFlow::Row,
        LayoutGridAutoFlow::Column => GridAutoFlow::Column,
        LayoutGridAutoFlow::RowDense => GridAutoFlow::RowDense,
        LayoutGridAutoFlow::ColumnDense => GridAutoFlow::ColumnDense,
    }
}

/// Convert an azul `GridLine` (single start or end) to a Taffy `GridPlacement`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn grid_line_to_taffy(
    line: &azul_css::props::layout::grid::GridLine,
) -> GridPlacement<String> {
    use azul_css::props::layout::grid::GridLine as AzGridLine;
    use taffy::style_helpers::{TaffyGridLine, TaffyGridSpan};
    match line {
        AzGridLine::Auto => GridPlacement::Auto,
        AzGridLine::Line(n) => {
            GridPlacement::<String>::from_line_index(*n as i16)
        }
        AzGridLine::Span(n) => GridPlacement::<String>::from_span(*n as u16),
        AzGridLine::Named(named) => {
            // Named lines: use the name with optional span
            let name = named.grid_line_name.as_str().to_string();
            if named.span_count > 0 {
                GridPlacement::NamedSpan(name, named.span_count as u16)
            } else {
                GridPlacement::NamedLine(name, 0)
            }
        }
    }
}

/// Convert an azul `GridPlacement` (grid-column / grid-row) to a Taffy `Line<GridPlacement>`.
fn grid_placement_to_taffy(
    placement: &azul_css::props::layout::grid::GridPlacement,
) -> Line<GridPlacement<String>> {
    Line {
        start: grid_line_to_taffy(&placement.grid_start),
        end: grid_line_to_taffy(&placement.grid_end),
    }
}

fn layout_flex_direction_to_taffy(val: LayoutFlexDirectionValue) -> FlexDirection {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutFlexDirection::Row => FlexDirection::Row,
        LayoutFlexDirection::RowReverse => FlexDirection::RowReverse,
        LayoutFlexDirection::Column => FlexDirection::Column,
        LayoutFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
    }
}

fn layout_flex_wrap_to_taffy(val: LayoutFlexWrapValue) -> FlexWrap {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutFlexWrap::NoWrap => FlexWrap::NoWrap,
        LayoutFlexWrap::Wrap => FlexWrap::Wrap,
        LayoutFlexWrap::WrapReverse => FlexWrap::WrapReverse,
    }
}

fn layout_align_items_to_taffy(val: LayoutAlignItemsValue) -> AlignItems {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignItems::Stretch => AlignItems::Stretch,
        LayoutAlignItems::Center => AlignItems::Center,
        LayoutAlignItems::Start => AlignItems::FlexStart,
        LayoutAlignItems::End => AlignItems::FlexEnd,
        LayoutAlignItems::Baseline => AlignItems::Baseline,
    }
}

fn layout_align_self_to_taffy(val: LayoutAlignSelfValue) -> Option<AlignSelf> {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignSelf::Auto => None, // Auto means inherit from parent's align-items (for non-abspos; abspos auto computes to itself per spec)
        LayoutAlignSelf::Start => Some(AlignSelf::FlexStart),
        LayoutAlignSelf::End => Some(AlignSelf::FlexEnd),
        LayoutAlignSelf::Center => Some(AlignSelf::Center),
        LayoutAlignSelf::Baseline => Some(AlignSelf::Baseline),
        LayoutAlignSelf::Stretch => Some(AlignSelf::Stretch),
    }
}

fn layout_align_content_to_taffy(val: LayoutAlignContentValue) -> AlignContent {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignContent::Start => AlignContent::FlexStart,
        LayoutAlignContent::End => AlignContent::FlexEnd,
        LayoutAlignContent::Center => AlignContent::Center,
        LayoutAlignContent::Stretch => AlignContent::Stretch,
        LayoutAlignContent::SpaceBetween => AlignContent::SpaceBetween,
        LayoutAlignContent::SpaceAround => AlignContent::SpaceAround,
    }
}

fn layout_justify_content_to_taffy(val: LayoutJustifyContentValue) -> JustifyContent {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutJustifyContent::FlexStart => JustifyContent::FlexStart,
        LayoutJustifyContent::FlexEnd => JustifyContent::FlexEnd,
        LayoutJustifyContent::Start => JustifyContent::Start,
        LayoutJustifyContent::End => JustifyContent::End,
        LayoutJustifyContent::Center => JustifyContent::Center,
        LayoutJustifyContent::SpaceBetween => JustifyContent::SpaceBetween,
        LayoutJustifyContent::SpaceAround => JustifyContent::SpaceAround,
        LayoutJustifyContent::SpaceEvenly => JustifyContent::SpaceEvenly,
    }
}

fn layout_justify_items_to_taffy(
    val: azul_css::props::property::LayoutJustifyItemsValue,
) -> AlignItems {
    use azul_css::props::layout::grid::LayoutJustifyItems;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutJustifyItems::Start => AlignItems::Start,
        LayoutJustifyItems::End => AlignItems::End,
        LayoutJustifyItems::Center => AlignItems::Center,
        LayoutJustifyItems::Stretch => AlignItems::Stretch,
    }
}

// TODO: visibility, z_index still missing
// --- CSS <-> Taffy conversion functions ---

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use azul_core::{dom::NodeId, geom::LogicalSize, styled_dom::StyledDom};
use azul_css::props::{
    layout::{LayoutHeight, LayoutWidth},
    property::{CssProperty, CssPropertyType},
};
use taffy::{
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    prelude::*, CacheTree, LayoutFlexboxContainer, LayoutGridContainer, LayoutInput, LayoutOutput,
    RunMode,
};

use crate::{
    font_traits::{FontLoaderTrait, ParsedFontTrait},
    solver3::{
        fc::{
            translate_taffy_point_back, translate_taffy_size_back, FloatingContext,
            LayoutConstraints, TextAlign as FcTextAlign,
        },
        getters::{
            get_align_content, get_align_items, get_css_border_bottom_width,
            get_css_border_left_width, get_css_border_right_width,
            get_css_border_top_width, get_css_box_sizing, get_css_bottom, get_css_height, get_css_left,
            get_css_margin_bottom, get_css_margin_left, get_css_margin_right, get_css_margin_top,
            get_css_max_height, get_css_max_width, get_css_min_height, get_css_min_width,
            get_css_padding_bottom, get_css_padding_left, get_css_padding_right,
            get_css_padding_top, get_css_right, get_css_top, get_css_width, get_flex_direction,
            get_position, MultiValue,
        },
        layout_tree::{get_display_type, LayoutTree},
        sizing, LayoutContext,
    },
};

/// Shared scrollbar detection for Taffy-managed flex/grid nodes.
///
/// When Taffy lays out a flex/grid container, it may expand the container
/// beyond the CSS-specified size (Taffy doesn't know about `overflow`).
/// This function resolves the CSS-constrained container size, computes
/// content vs. container overflow, and returns the resulting `ScrollbarRequirements`
/// plus the effective content size (for `overflow_content_size`).
///
/// Returns `(scrollbar_info, effective_content_width, effective_content_height)`.
fn compute_taffy_scrollbar_info<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    tree: &LayoutTree,
    node_idx: usize,
    result_width: f32,
    result_height: f32,
    taffy_content_width: f32,
    taffy_content_height: f32,
) -> (crate::solver3::scrollbar::ScrollbarRequirements, f32, f32) {
    use crate::solver3::scrollbar::ScrollbarRequirements;

    let node = tree.get(node_idx);
    let dom_id = node.and_then(|n| n.dom_node_id);

    let Some(dom_id) = dom_id else {
        return (ScrollbarRequirements::default(), 0.0, 0.0);
    };

    let styled_node_state = ctx
        .styled_dom
        .styled_nodes
        .as_container()
        .get(dom_id)
        .map(|s| s.styled_node_state)
        .unwrap_or_default();

    // Compute padding + border from the node's box_props
    let (padding_width, padding_height, border_width, border_height, border_left, border_top) = tree
        .get(node_idx)
        .map_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0), |node| {
            let bp = node.box_props.unpack();
            (
                bp.padding.left + bp.padding.right,
                bp.padding.top + bp.padding.bottom,
                bp.border.left + bp.border.right,
                bp.border.top + bp.border.bottom,
                bp.border.left,
                bp.border.top,
            )
        });

    // Use CSS-specified dimensions as the container constraint.
    // Taffy may have expanded the box beyond these, but the CSS spec says
    // the container clips at the specified size.
    let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);
    let css_width = get_css_width(ctx.styled_dom, dom_id, &styled_node_state);

    let result_content_w = result_width - padding_width - border_width;
    let result_content_h = result_height - padding_height - border_height;

    let css_container_w = css_width
        .exact()
        .and_then(|w| css_width_to_px(&w))
        .unwrap_or(result_content_w)
        .max(0.0);

    let css_container_h = css_height
        .exact()
        .and_then(|h| css_height_to_px(&h))
        .unwrap_or(result_content_h)
        .max(0.0);

    // Content size: use Taffy's content_size if non-zero,
    // else result size minus padding/border (Taffy expanded to fit).
    //
    // IMPORTANT: Taffy's content_size is measured from (0,0) of the border-box,
    // so it includes border.left/border.top as a leading offset. The container_size
    // is in content-box coordinates (result_width - padding - border). We must
    // subtract border.left/top from content_size to align coordinate spaces,
    // otherwise we get spurious horizontal scrollbars from the border offset.
    let content_w = if taffy_content_width > 0.0 {
        (taffy_content_width - border_left).max(0.0)
    } else {
        result_content_w.max(0.0)
    };
    let content_h = if taffy_content_height > 0.0 {
        (taffy_content_height - border_top).max(0.0)
    } else {
        result_content_h.max(0.0)
    };

    let content_size = LogicalSize::new(content_w, content_h);
    let container_size = LogicalSize::new(css_container_w, css_container_h);

    let scrollbar_info =
        crate::solver3::cache::compute_scrollbar_info_core(ctx, dom_id, &styled_node_state, content_size, container_size);

    (scrollbar_info, content_w, content_h)
}

/// Convert `LayoutWidth::Px(…)` to `f32`, returning None for non-px units.
fn css_width_to_px(w: &LayoutWidth) -> Option<f32> {
    match w {
        LayoutWidth::Px(px) => pixel_value_to_pixels_fallback(px),
        _ => None,
    }
}

/// Convert `LayoutHeight::Px(…)` to `f32`, returning None for non-px units.
fn css_height_to_px(h: &LayoutHeight) -> Option<f32> {
    match h {
        LayoutHeight::Px(px) => pixel_value_to_pixels_fallback(px),
        _ => None,
    }
}

// Helper function to convert MultiValue<PixelValue> to LengthPercentageAuto
fn multi_value_to_lpa(mv: MultiValue<PixelValue>) -> LengthPercentageAuto {
    match mv {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            LengthPercentageAuto::auto()
        }
        MultiValue::Exact(pv) => pixel_value_to_pixels_fallback(&pv)
            .map(LengthPercentageAuto::length)
            .or_else(|| {
                pv.to_percent()
                    .map(|p| LengthPercentageAuto::percent(p.get()))
            })
            .unwrap_or_else(LengthPercentageAuto::auto),
    }
}

// Helper function to convert MultiValue<PixelValue> to LengthPercentageAuto for margins
// CSS spec: margin initial value is 0, but `auto` has special centering meaning in flexbox
fn multi_value_to_lpa_margin(mv: MultiValue<PixelValue>) -> LengthPercentageAuto {
    match mv {
        MultiValue::Auto => {
            LengthPercentageAuto::auto() // Preserve auto for flexbox centering
        }
        MultiValue::Initial | MultiValue::Inherit => {
            LengthPercentageAuto::length(0.0) // Margins' initial value is 0
        }
        MultiValue::Exact(pv) => {
            pixel_value_to_pixels_fallback(&pv)
                .map(LengthPercentageAuto::length)
                .or_else(|| {
                    pv.to_percent()
                        .map(|p| LengthPercentageAuto::percent(p.get()))
                })
                .unwrap_or_else(|| LengthPercentageAuto::length(0.0)) // Fallback to 0 for
                                                                             // margins
        }
    }
}

// Helper function to convert MultiValue<PixelValue> to LengthPercentage
fn multi_value_to_lp(mv: MultiValue<PixelValue>) -> LengthPercentage {
    match mv {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            LengthPercentage::ZERO
        }
        MultiValue::Exact(pv) => pixel_value_to_pixels_fallback(&pv)
            .map(LengthPercentage::length)
            .or_else(|| {
                pv.to_percent()
                    .map(|p| LengthPercentage::percent(p.get()))
            })
            .unwrap_or(LengthPercentage::ZERO),
    }
}

// Helper function to convert plain PixelValue to LengthPercentage
/// Converts Azul's CSS overflow value to Taffy's Overflow enum.
///
/// Taffy only has Visible, Clip, Hidden, Scroll (no Auto).
/// CSS `auto` behaves like `scroll` from a layout perspective —
/// it constrains the container and enables scrolling.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn azul_overflow_to_taffy(ov: MultiValue<azul_css::props::layout::LayoutOverflow>) -> taffy::Overflow {
    use azul_css::props::layout::LayoutOverflow;
    match ov {
        MultiValue::Exact(LayoutOverflow::Visible) => taffy::Overflow::Visible,
        MultiValue::Exact(LayoutOverflow::Hidden) => taffy::Overflow::Hidden,
        MultiValue::Exact(LayoutOverflow::Scroll) => taffy::Overflow::Scroll,
        MultiValue::Exact(LayoutOverflow::Auto) => taffy::Overflow::Scroll, // Auto acts like scroll for layout
        MultiValue::Exact(LayoutOverflow::Clip) => taffy::Overflow::Clip,
        _ => taffy::Overflow::Visible, // default
    }
}

fn pixel_to_lp(pv: PixelValue) -> LengthPercentage {
    pixel_value_to_pixels_fallback(&pv)
        .map(LengthPercentage::length)
        .or_else(|| {
            pv.to_percent()
                .map(|p| LengthPercentage::percent(p.get()))
        })
        .unwrap_or(LengthPercentage::ZERO)
}

/// Slow path for flex-basis: full property cache lookup + decode.
/// Extracted to avoid duplicating the logic in the compact fast-path fallback.
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn flex_basis_slow_path(
    cache: &azul_core::prop_cache::CssPropertyCache,
    node_data: &azul_core::dom::NodeData,
    id: &NodeId,
    node_state: &azul_core::styled_dom::StyledNodeState,
    taffy_style: &mut Style,
) -> Dimension {
    cache
        .get_property(node_data, id, node_state, &CssPropertyType::FlexBasis)
        .and_then(|p| {
            if let CssProperty::FlexBasis(v) = p {
                let basis = match v.get_property_or_default().unwrap_or_default() {
                    LayoutFlexBasis::Auto => Dimension::auto(),
                    LayoutFlexBasis::Exact(pv) => pixel_value_to_pixels_fallback(&pv)
                        .map(Dimension::length)
                        .or_else(|| pv.to_percent().map(|p| Dimension::percent(p.get())))
                        .unwrap_or_else(Dimension::auto),
                };
                // WORKAROUND: If flex-basis is set and not auto, clear width to let flex-basis
                // take precedence. Workaround for Taffy not properly prioritizing flex-basis over width
                if !matches!(basis, auto if auto == Dimension::auto()) {
                    taffy_style.size.width = Dimension::auto();
                }
                Some(basis)
            } else {
                None
            }
        })
        .unwrap_or_else(Dimension::auto)
}

/// The bridge struct that implements Taffy's traits.
/// It holds mutable references to the solver's data structures, allowing Taffy
/// to read styles and write layout results back into our `LayoutTree`.
struct TaffyBridge<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    tree: &'a mut LayoutTree,
    /// Raw pointer to text cache - needed because we can't have multiple &mut references
    /// SAFETY: This pointer is only valid for the lifetime of the `TaffyBridge`
    /// and must only be used within `compute_child_layout` callbacks
    text_cache: *mut crate::font_traits::TextLayoutCache,
    /// Heap-pinned `CalcResolveContext`s whose addresses are passed into taffy
    /// `Dimension::calc(ptr)`. Kept alive for the duration of the layout pass.
    /// Uses `RefCell` because `get_core_container_style` takes `&self`.
    // Box gives each CalcResolveContext a stable heap address for the `*const` handed to
    // taffy `Dimension::calc()`; a plain Vec<T> would invalidate those pointers on realloc.
    #[allow(clippy::vec_box)]
    calc_storage: std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    /// Memoised `translate_style_to_taffy` results, keyed by DOM node id
    /// (`usize` = `NodeId::index`). Taffy calls
    /// `get_core_container_style` and `should_suppress_cross_intrinsic`
    /// many times per node during a single layout pass; each call
    /// triggers ~13 `cache.get_property` cascade walks for grid/flex
    /// props. Caching the built `Style` cuts this to one build per node.
    style_memo: std::cell::RefCell<HashMap<usize, Style>>,
}

impl<'a, 'b, T: ParsedFontTrait> TaffyBridge<'a, 'b, T> {
    fn new(
        ctx: &'a mut LayoutContext<'b, T>,
        tree: &'a mut LayoutTree,
        text_cache: *mut crate::font_traits::TextLayoutCache,
    ) -> Self {
        Self {
            ctx,
            tree,
            text_cache,
            calc_storage: std::cell::RefCell::new(Vec::new()),
            style_memo: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Cache-backed wrapper for `translate_style_to_taffy`. Returns a
    /// clone of the memoised `Style` on cache hit, builds + inserts on
    /// miss. Keyed by DOM node index (not tree index) because the
    /// result depends only on the styled DOM, not on the transient
    /// layout tree.
    fn translate_style_to_taffy_cached(&self, dom_id: Option<NodeId>) -> Style {
        let Some(id) = dom_id else {
            return Style::default();
        };
        let key = id.index();
        if let Some(style) = self.style_memo.borrow().get(&key) {
            return style.clone();
        }
        let style = self.translate_style_to_taffy(dom_id);
        self.style_memo.borrow_mut().insert(key, style.clone());
        style
    }

    /// Translates CSS properties from the `StyledDom` into a `taffy::Style` struct.
    /// This is the core of the integration, mapping one style system to another.
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn translate_style_to_taffy(&self, dom_id: Option<NodeId>) -> Style {
        let Some(id) = dom_id else {
            return Style::default();
        };
        let styled_dom = &self.ctx.styled_dom;
        let node_data = &styled_dom.node_data.as_ref()[id.index()];
        let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
        let cache = &styled_dom.css_property_cache.ptr;
        let mut taffy_style = Style::default();

        // Box Sizing — CSS default is content-box, but Taffy defaults to border-box
        taffy_style.box_sizing = match get_css_box_sizing(styled_dom, id, node_state).unwrap_or_default() {
            azul_css::props::layout::LayoutBoxSizing::BorderBox => BoxSizing::BorderBox,
            azul_css::props::layout::LayoutBoxSizing::ContentBox => BoxSizing::ContentBox,
        };

        // Display Mode
        taffy_style.display =
            layout_display_to_taffy(CssPropertyValue::Exact(get_display_type(styled_dom, id)));

        // Position
        taffy_style.position =
            from_layout_position(get_position(styled_dom, id, node_state).unwrap_or_default());

        // Inset (top, left, bottom, right)
        taffy_style.inset = Rect {
            left: multi_value_to_lpa(get_css_left(styled_dom, id, node_state)),
            right: multi_value_to_lpa(get_css_right(styled_dom, id, node_state)),
            top: multi_value_to_lpa(get_css_top(styled_dom, id, node_state)),
            bottom: multi_value_to_lpa(get_css_bottom(styled_dom, id, node_state)),
        };

        // Size
        let width = get_css_width(self.ctx.styled_dom, id, node_state);
        let height = get_css_height(self.ctx.styled_dom, id, node_state);

        // Resolve node-local font sizes for calc() em/rem resolution
        let em_size = crate::solver3::getters::get_element_font_size(styled_dom, id, node_state);
        let rem_size = {
            let root_id = NodeId::new(0);
            let root_state = &styled_dom.styled_nodes.as_container()[root_id].styled_node_state;
            crate::solver3::getters::get_element_font_size(styled_dom, root_id, root_state)
        };

        let taffy_width = from_layout_width(width.unwrap_or_default(), &self.calc_storage, em_size, rem_size);
        let taffy_height = from_layout_height(height.unwrap_or_default(), &self.calc_storage, em_size, rem_size);

        taffy_style.size = Size {
            width: taffy_width,
            height: taffy_height,
        };

        // Overflow — CRITICAL for scroll containers.
        // Without this, Taffy's flexbox algorithm uses content size as automatic
        // minimum size, causing flex containers with overflow:auto/scroll to
        // expand to fit all content instead of clipping at the explicit size.
        // With overflow: Hidden/Scroll, Taffy sets automatic min size to 0 and
        // constrains the container.
        let overflow_x = get_overflow_x(styled_dom, id, node_state);
        let overflow_y = get_overflow_y(styled_dom, id, node_state);
        taffy_style.overflow = taffy::Point {
            x: azul_overflow_to_taffy(overflow_x),
            y: azul_overflow_to_taffy(overflow_y),
        };

        // Min/Max Size
        // min-size:auto enables Taffy's auto minimum size algorithm which computes the
        // content size suggestion (min-content in main axis) and transferred size suggestion
        // (cross size converted through aspect ratio, if any). NOTE: aspect_ratio is not yet
        // forwarded to Taffy, so the transferred size suggestion path is incomplete.
        // NOTE: In CSS, the default min-width/min-height for flex items is `auto`
        // (which resolves to `min-content`), preventing them from shrinking below
        // their content size. We must map Auto to Dimension::Auto, NOT to 0px.
        let min_width_css = get_css_min_width(styled_dom, id, node_state);
        let min_height_css = get_css_min_height(styled_dom, id, node_state);

        taffy_style.min_size = Size {
            width: match min_width_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
            height: match min_height_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
        };

        // For max-size, we need to handle Auto specially - it should translate to Taffy's auto, not
        // a concrete value This is CRITICAL for flexbox stretch to work: items with
        // max-height: auto CAN be stretched
        let max_width_css = get_css_max_width(styled_dom, id, node_state);
        let max_height_css = get_css_max_height(styled_dom, id, node_state);

        taffy_style.max_size = Size {
            width: match max_width_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
            height: match max_height_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
        };

        // Box Model (margin, padding, border)
        let margin_left_css = get_css_margin_left(styled_dom, id, node_state);
        let margin_right_css = get_css_margin_right(styled_dom, id, node_state);
        let margin_top_css = get_css_margin_top(styled_dom, id, node_state);
        let margin_bottom_css = get_css_margin_bottom(styled_dom, id, node_state);

        taffy_style.margin = Rect {
            left: multi_value_to_lpa_margin(margin_left_css),
            right: multi_value_to_lpa_margin(margin_right_css),
            top: multi_value_to_lpa_margin(margin_top_css),
            bottom: multi_value_to_lpa_margin(margin_bottom_css),
        };

        taffy_style.padding = Rect {
            left: multi_value_to_lp(get_css_padding_left(styled_dom, id, node_state)),
            right: multi_value_to_lp(get_css_padding_right(styled_dom, id, node_state)),
            top: multi_value_to_lp(get_css_padding_top(styled_dom, id, node_state)),
            bottom: multi_value_to_lp(get_css_padding_bottom(styled_dom, id, node_state)),
        };

        taffy_style.border = Rect {
            left: multi_value_to_lp(get_css_border_left_width(styled_dom, id, node_state)),
            right: multi_value_to_lp(get_css_border_right_width(styled_dom, id, node_state)),
            top: multi_value_to_lp(get_css_border_top_width(styled_dom, id, node_state)),
            bottom: multi_value_to_lp(get_css_border_bottom_width(styled_dom, id, node_state)),
        };

        // Grid & gap properties — COMPACT FAST PATH: row_gap/column_gap are
        // i16 px × 10 in tier2_dims. The slow-path lookup would walk the
        // cascade for every node even though the answer is already encoded.
        taffy_style.gap = cache.compact_cache.as_ref().map_or_else(|| cache
                .get_property(node_data, &id, node_state, &CssPropertyType::Gap)
                .and_then(|p| if let CssProperty::Gap(v) = p { Some(v) } else { None })
                .map_or_else(Size::zero, |v| {
                    let val = v.get_property_or_default().unwrap_or_default().inner;
                    let gap_lp = pixel_to_lp(val);
                    Size { width: gap_lp, height: gap_lp }
                }), |cc| {
            let row = cc.tier2_dims[id.index()].row_gap;
            let col = cc.tier2_dims[id.index()].column_gap;
            let decode = |raw: i16| -> LengthPercentage {
                if raw >= azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                    LengthPercentage::length(0.0)
                } else {
                    LengthPercentage::length(f32::from(raw) / 10.0)
                }
            };
            Size {
                width: decode(col),
                height: decode(row),
            }
        });

        // Skip grid properties when not in a grid context.
        // Grid container props: only if this node has display:grid.
        // Grid item props: only if parent has display:grid.
        let (self_is_grid, parent_is_grid) = cache.compact_cache.as_ref().map_or((false, false), |cc| {
            #[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
            use azul_css::compact_cache::*;
            let self_t1 = cc.tier1_enums[id.index()];
            let self_display = ((self_t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8;
            let grid_val = layout_display_to_u8(LayoutDisplay::Grid);
            let self_grid = self_display == grid_val;

            let parent_idx = styled_dom.node_hierarchy.as_ref()[id.index()].parent_id()
                .map_or(0, |p| p.index());
            let parent_t1 = cc.tier1_enums[parent_idx];
            let parent_display = ((parent_t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8;
            let parent_grid = parent_display == grid_val;
            (self_grid, parent_grid)
        });

        if self_is_grid {
        taffy_style.grid_template_rows = cache
            .get_property(
                node_data,
                &id,
                node_state,
                &CssPropertyType::GridTemplateRows,
            )
            .and_then(|p| {
                if let CssProperty::GridTemplateRows(v) = p {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .map(grid_template_rows_to_taffy)
            .unwrap_or_default();

        // Grid template columns - convert GridTemplate to Vec<GridTemplateComponent>
        taffy_style.grid_template_columns = cache
            .get_property(
                node_data,
                &id,
                node_state,
                &CssPropertyType::GridTemplateColumns,
            )
            .and_then(|p| {
                if let CssProperty::GridTemplateColumns(v) = p {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .map(grid_template_columns_to_taffy)
            .unwrap_or_default();

        // Grid template areas - convert GridTemplateAreas to Vec<taffy::GridTemplateArea<String>>
        taffy_style.grid_template_areas = cache
            .get_property(
                node_data,
                &id,
                node_state,
                &CssPropertyType::GridTemplateAreas,
            )
            .and_then(|p| {
                if let CssProperty::GridTemplateAreas(v) = p {
                    v.get_property().cloned()
                } else {
                    None
                }
            })
            .map(|areas| {
                areas
                    .areas
                    .as_ref()
                    .iter()
                    .map(|a| taffy::GridTemplateArea {
                        name: a.name.as_str().to_string(),
                        row_start: a.row_start,
                        row_end: a.row_end,
                        column_start: a.column_start,
                        column_end: a.column_end,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        taffy_style.grid_auto_rows = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::GridAutoRows)
            .and_then(|p| {
                if let CssProperty::GridAutoRows(v) = p {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .map(grid_auto_rows_to_taffy)
            .unwrap_or_default();

        taffy_style.grid_auto_columns = cache
            .get_property(
                node_data,
                &id,
                node_state,
                &CssPropertyType::GridAutoColumns,
            )
            .and_then(|p| {
                if let CssProperty::GridAutoColumns(v) = p {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .map(grid_auto_columns_to_taffy)
            .unwrap_or_default();

        taffy_style.grid_auto_flow = cache.compact_cache.as_ref().map_or_else(|| cache
                .get_property(node_data, &id, node_state, &CssPropertyType::GridAutoFlow)
                .and_then(|p| if let CssProperty::GridAutoFlow(v) = p { Some(*v) } else { None })
                .map(grid_auto_flow_to_taffy)
                .unwrap_or_default(), |cc| {
            #[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
            use azul_css::compact_cache::*;
            let bits = ((cc.tier1_enums[id.index()] >> GRID_AUTO_FLOW_SHIFT) & GRID_AUTO_FLOW_MASK) as u8;
            let val = layout_grid_auto_flow_from_u8(bits);
            grid_auto_flow_to_taffy(CssPropertyValue::Exact(val))
        });

        } // end if self_is_grid

        if parent_is_grid {
        // Grid item placement — read from compact cold cache (Auto/Line/Span)
        if let Some(cc) = cache.compact_cache.as_ref() {
            let cs = cc.tier2_cold[id.index()].grid_col_start;
            let ce = cc.tier2_cold[id.index()].grid_col_end;
            if cs != azul_css::compact_cache::I16_AUTO || ce != azul_css::compact_cache::I16_AUTO {
                taffy_style.grid_column = Line { start: decode_compact_grid_line(cs), end: decode_compact_grid_line(ce) };
            }
            let rs = cc.tier2_cold[id.index()].grid_row_start;
            let re = cc.tier2_cold[id.index()].grid_row_end;
            if rs != azul_css::compact_cache::I16_AUTO || re != azul_css::compact_cache::I16_AUTO {
                taffy_style.grid_row = Line { start: decode_compact_grid_line(rs), end: decode_compact_grid_line(re) };
            }
        } else {
            if let Some(grid_col) = cache
                .get_property(node_data, &id, node_state, &CssPropertyType::GridColumn)
                .and_then(|p| if let CssProperty::GridColumn(v) = p { v.get_property().cloned() } else { None })
            { taffy_style.grid_column = grid_placement_to_taffy(&grid_col); }
            if let Some(grid_row) = cache
                .get_property(node_data, &id, node_state, &CssPropertyType::GridRow)
                .and_then(|p| if let CssProperty::GridRow(v) = p { v.get_property().cloned() } else { None })
            { taffy_style.grid_row = grid_placement_to_taffy(&grid_row); }
        }
        } // end if parent_is_grid

        // Flexbox
        taffy_style.flex_direction = match get_flex_direction(styled_dom, id, node_state) {
            MultiValue::Exact(v) => layout_flex_direction_to_taffy(CssPropertyValue::Exact(v)),
            _ => FlexDirection::Row,
        };
        // COMPACT FAST PATH: flex_wrap is Tier 1 enum
        taffy_style.flex_wrap = {
            let compact = if node_state.is_normal() {
                cache.compact_cache.as_ref().map(|cc| {
                    layout_flex_wrap_to_taffy(CssPropertyValue::Exact(cc.get_flex_wrap(id.index())))
                })
            } else {
                None
            };
            compact.unwrap_or_else(|| {
                cache
                    .get_property(node_data, &id, node_state, &CssPropertyType::FlexWrap)
                    .and_then(|p| if let CssProperty::FlexWrap(v) = p { Some(*v) } else { None })
                    .map_or(FlexWrap::NoWrap, layout_flex_wrap_to_taffy)
            })
        };
        taffy_style.align_items = match get_align_items(styled_dom, id, node_state) {
            MultiValue::Exact(v) => Some(layout_align_items_to_taffy(CssPropertyValue::Exact(v))),
            _ => None,
        };
                // CSS spec: default align-items is "normal" which acts like "stretch"
                // for non-replaced grid/flex items. Taffy handles this internally when
                // align_items is None, so we should NOT force a default here.
        taffy_style.justify_items = cache.compact_cache.as_ref().map_or_else(|| cache
                .get_property(node_data, &id, node_state, &CssPropertyType::JustifyItems)
                .and_then(|p| if let CssProperty::JustifyItems(v) = p { Some(*v) } else { None })
                .map(layout_justify_items_to_taffy), |cc| {
            #[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
            use azul_css::compact_cache::*;
            use azul_css::props::layout::grid::LayoutJustifyItems;
            let bits = ((cc.tier1_enums[id.index()] >> JUSTIFY_ITEMS_SHIFT) & JUSTIFY_ITEMS_MASK) as u8;
            let val = layout_justify_items_from_u8(bits);
            Some(match val {
                LayoutJustifyItems::Start => AlignItems::Start,
                LayoutJustifyItems::End => AlignItems::End,
                LayoutJustifyItems::Center => AlignItems::Center,
                LayoutJustifyItems::Stretch => AlignItems::Stretch,
            })
        });
        // COMPACT FAST PATH: justify-content is in tier1 bits 21-23.
        taffy_style.justify_content = cache.compact_cache.as_ref().map_or_else(|| cache
                .get_property(node_data, &id, node_state, &CssPropertyType::JustifyContent)
                .and_then(|p| if let CssProperty::JustifyContent(v) = p { Some(v) } else { None })
                .map(|v| layout_justify_content_to_taffy(*v)), |cc| {
            #[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
            use azul_css::compact_cache::*;
            use azul_css::props::layout::LayoutJustifyContent;
            let bits = ((cc.tier1_enums[id.index()] >> JUSTIFY_CONTENT_SHIFT) & JUSTIFY_MASK) as u8;
            Some(match layout_justify_content_from_u8(bits) {
                LayoutJustifyContent::FlexStart => JustifyContent::FlexStart,
                LayoutJustifyContent::FlexEnd => JustifyContent::FlexEnd,
                LayoutJustifyContent::Start => JustifyContent::Start,
                LayoutJustifyContent::End => JustifyContent::End,
                LayoutJustifyContent::Center => JustifyContent::Center,
                LayoutJustifyContent::SpaceBetween => JustifyContent::SpaceBetween,
                LayoutJustifyContent::SpaceAround => JustifyContent::SpaceAround,
                LayoutJustifyContent::SpaceEvenly => JustifyContent::SpaceEvenly,
            })
        });
                // CSS spec: default justify-content is "normal". Taffy handles
                // this internally when justify_content is None.
        // COMPACT FAST PATH: flex_grow stored as u16 × 100
        taffy_style.flex_grow = {
            let compact = if node_state.is_normal() {
                cache.compact_cache.as_ref().and_then(|cc| cc.get_flex_grow(id.index()))
            } else {
                None
            };
            compact.unwrap_or_else(|| {
                cache
                    .get_property(node_data, &id, node_state, &CssPropertyType::FlexGrow)
                    .and_then(|p| if let CssProperty::FlexGrow(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner.get())
                    } else { None })
                    .unwrap_or(0.0)
            })
        };

        // COMPACT FAST PATH: flex_shrink stored as u16 × 100
        taffy_style.flex_shrink = {
            let compact = if node_state.is_normal() {
                cache.compact_cache.as_ref().and_then(|cc| cc.get_flex_shrink(id.index()))
            } else {
                None
            };
            compact.unwrap_or_else(|| {
                cache
                    .get_property(node_data, &id, node_state, &CssPropertyType::FlexShrink)
                    .and_then(|p| if let CssProperty::FlexShrink(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner.get())
                    } else { None })
                    .unwrap_or(1.0)
            })
        };
        // COMPACT FAST PATH: flex_basis stored as u32 with PixelValue encoding
        taffy_style.flex_basis = {
            let compact = if node_state.is_normal() {
                cache.compact_cache.as_ref().and_then(|cc| {
                    let raw = cc.get_flex_basis_raw(id.index());
                    match raw {
                        azul_css::compact_cache::U32_AUTO
                        | azul_css::compact_cache::U32_NONE
                        | azul_css::compact_cache::U32_INITIAL => Some(Dimension::auto()),
                        azul_css::compact_cache::U32_SENTINEL
                        | azul_css::compact_cache::U32_INHERIT => None,
                        _ => {
                            if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                                let basis = pixel_value_to_pixels_fallback(&pv)
                                    .map(Dimension::length)
                                    .or_else(|| pv.to_percent().map(|p| Dimension::percent(p.get())))
                                    .unwrap_or_else(Dimension::auto);
                                if !matches!(basis, auto if auto == Dimension::auto()) {
                                    taffy_style.size.width = Dimension::auto();
                                }
                                Some(basis)
                            } else {
                                Some(Dimension::auto())
                            }
                        }
                    }
                })
            } else {
                None
            };
            compact.unwrap_or_else(|| {
                flex_basis_slow_path(cache, node_data, &id, node_state, &mut taffy_style)
            })
        };
        taffy_style.align_self = cache.compact_cache.as_ref().map_or_else(|| cache
                .get_property(node_data, &id, node_state, &CssPropertyType::AlignSelf)
                .and_then(|p| if let CssProperty::AlignSelf(v) = p { layout_align_self_to_taffy(*v) } else { None }), |cc| {
            #[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
            use azul_css::compact_cache::*;
            let bits = ((cc.tier1_enums[id.index()] >> ALIGN_SELF_SHIFT) & ALIGN_SELF_MASK) as u8;
            let val = layout_align_self_from_u8(bits);
            match val {
                LayoutAlignSelf::Auto => None,
                LayoutAlignSelf::Start => Some(AlignSelf::FlexStart),
                LayoutAlignSelf::End => Some(AlignSelf::FlexEnd),
                LayoutAlignSelf::Center => Some(AlignSelf::Center),
                LayoutAlignSelf::Baseline => Some(AlignSelf::Baseline),
                LayoutAlignSelf::Stretch => Some(AlignSelf::Stretch),
            }
        });
        taffy_style.justify_self = cache.compact_cache.as_ref().map_or_else(|| cache
                .get_property(node_data, &id, node_state, &CssPropertyType::JustifySelf)
                .and_then(|p| if let CssProperty::JustifySelf(v) = p {
                    use azul_css::props::layout::grid::LayoutJustifySelf;
                    match v.get_property_or_default().unwrap_or_default() {
                        LayoutJustifySelf::Auto => None,
                        LayoutJustifySelf::Start => Some(AlignSelf::Start),
                        LayoutJustifySelf::End => Some(AlignSelf::End),
                        LayoutJustifySelf::Center => Some(AlignSelf::Center),
                        LayoutJustifySelf::Stretch => Some(AlignSelf::Stretch),
                    }
                } else { None }), |cc| {
            #[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
            use azul_css::compact_cache::*;
            use azul_css::props::layout::grid::LayoutJustifySelf;
            let bits = ((cc.tier1_enums[id.index()] >> JUSTIFY_SELF_SHIFT) & JUSTIFY_SELF_MASK) as u8;
            let val = layout_justify_self_from_u8(bits);
            match val {
                LayoutJustifySelf::Auto => None,
                LayoutJustifySelf::Start => Some(AlignSelf::Start),
                LayoutJustifySelf::End => Some(AlignSelf::End),
                LayoutJustifySelf::Center => Some(AlignSelf::Center),
                LayoutJustifySelf::Stretch => Some(AlignSelf::Stretch),
            }
        });
        taffy_style.align_content = match get_align_content(styled_dom, id, node_state) {
            MultiValue::Exact(v) => Some(layout_align_content_to_taffy(CssPropertyValue::Exact(v))),
            _ => None,
        };
                // CSS spec: default align-content is "normal". Taffy handles
                // this internally when align_content is None.

        taffy_style
    }

    /// Gets or computes the Taffy style for a given node index.
    fn get_taffy_style(&self, node_idx: usize) -> Style {
        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        let mut style = self.translate_style_to_taffy_cached(dom_id);
        
        // CSS 2.1 § 10.3.3: Root element margin handling for Flex/Grid.
        //
        // The root element's margin is already resolved and subtracted from
        // available_size by calculate_used_size_for_node() (sizing.rs). The
        // resulting margin-adjusted size is passed to Taffy as known_dimensions.
        //
        // Taffy's layout algorithm reads margin from the style and subtracts it
        // from known_dimensions internally. If we also pass the margin through
        // the style, it gets subtracted twice:
        //   1. calculate_used_size_for_node: viewport - margin → available_size
        //   2. Taffy: known_dimensions - style.margin → content_area
        //
        // Zeroing the style margin for root nodes prevents this double-subtraction.
        // This is NOT a hack — it's the correct integration point between Azul's
        // BFC-level sizing and Taffy's Flex/Grid algorithm.
        let is_root = self.tree.get(node_idx).is_some_and(|n| n.parent.is_none());
        if is_root {
            style.margin = Rect::zero();
        }
        
        // FIX: Apply cross-axis intrinsic size suppression for stretch alignment.
        // This enables align-self: stretch to work correctly by ensuring Taffy
        // sees the cross-axis size as Auto (allowing stretch) rather than a definite value.
        let (suppress_width, suppress_height) = self.should_suppress_cross_intrinsic(node_idx, &style);

        if suppress_width {
            // Force width to Auto and set min-width to 0 to allow stretching.
            // Taffy treats Auto size + Stretch alignment as a signal to fill the container.
            style.size.width = Dimension::auto(); 
            style.min_size.width = Dimension::length(0.0);
        }

        if suppress_height {
            style.size.height = Dimension::auto();
            style.min_size.height = Dimension::length(0.0);
        }

        style
    }

    /// Determines if cross-axis intrinsic size should be suppressed for stretching.
    ///
    /// Per CSS Flexbox spec, align-items: stretch makes items fill the cross-axis
    /// ONLY if the item's cross-size is 'auto' AND the item has no intrinsic cross-size.
    ///
    /// Returns (`suppress_width`, `suppress_height`) booleans.
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    fn should_suppress_cross_intrinsic(&self, node_idx: usize, style: &Style) -> (bool, bool) {
        let Some(node) = self.tree.get(node_idx) else {
            return (false, false);
        };

        // Check if parent is a flex or grid container
        let Some(parent_fc) = self.tree.warm(node_idx).and_then(|w| w.parent_formatting_context) else {
            return (false, false);
        };

        match parent_fc {
            FormattingContext::Flex => {
                // Get parent node to check its flex-direction and align-items
                let Some(parent_idx) = node.parent else {
                    return (false, false);
                };
                let parent_dom_id = self.tree.get(parent_idx).and_then(|n| n.dom_node_id);
                let parent_style = self.translate_style_to_taffy_cached(parent_dom_id);

                // Determine if flex container is row or column
                let is_row = matches!(
                    parent_style.flex_direction,
                    FlexDirection::Row | FlexDirection::RowReverse
                );

                // Get effective align value for this item
                // align-self overrides parent's align-items
                let align = style
                    .align_self
                    .or(parent_style.align_items)
                    .unwrap_or(AlignSelf::Stretch);

                let should_stretch = matches!(align, AlignSelf::Stretch);

                if !should_stretch {
                    return (false, false);
                }

                // Check if cross-axis size is auto
                // For row flex: cross-axis is height
                // For column flex: cross-axis is width
                let cross_size_is_auto = if is_row {
                    style.size.height == Dimension::auto()
                } else {
                    style.size.width == Dimension::auto()
                };

                if !cross_size_is_auto {
                    return (false, false);
                }

                // All conditions met: suppress intrinsic cross-size
                if is_row {
                    (false, true) // Suppress height for row flex
                } else {
                    (true, false) // Suppress width for column flex
                }
            }
            FormattingContext::Grid => {
                // TODO: Implement grid stretch detection
                // Grid is more complex because:
                // 1. Default align-items is 'start', not 'stretch'
                // 2. Items can stretch in both axes simultaneously
                // 3. Need to check grid-auto-flow and track sizing
                (false, false)
            }
            _ => (false, false),
        }
    }

    /// Helper to get children that participate in layout (i.e., not `display: none`).
    fn get_layout_children(&self, node_idx: usize) -> Vec<usize> {
        use crate::solver3::getters::{get_display_property, MultiValue};
        let Some(node) = self.tree.get(node_idx) else {
            return Vec::new();
        };

        self.tree.children(node_idx)
            .iter()
            .filter(|&&child_idx| {
                let Some(child_node) = self.tree.get(child_idx) else {
                    return false;
                };
                let Some(child_dom_id) = child_node.dom_node_id else {
                    return true;
                };

                // Check if child has display: none
                let display = get_display_property(self.ctx.styled_dom, Some(child_dom_id));
                let is_display_none = matches!(display, MultiValue::Exact(LayoutDisplay::None));

                !is_display_none
            })
            .copied()
            .collect()
    }
}

/// Main entry point for laying out a Flexbox or Grid container using Taffy.
///
/// This function now accepts a `text_cache` parameter so that IFC layout can be
/// performed inline during Taffy's measure callbacks, rather than as a post-processing step.
/// # Panics
///
/// Panics if `node_idx` is not present in the layout tree.
pub fn layout_taffy_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_idx: usize,
    inputs: LayoutInput,
) -> LayoutOutput {
    let children: Vec<usize> = tree.children(node_idx).to_vec();

    // DEBUG: Log Taffy inputs
    if ctx.debug_messages.is_some() {
        ctx.debug_info_inner(format!(
            "[TAFFY INPUT] node_idx={} known_dims=({:?}, {:?}) available=({:?}, {:?}) \
             parent_size=({:?}, {:?}) children={:?}",
            node_idx,
            inputs.known_dimensions.width,
            inputs.known_dimensions.height,
            inputs.available_space.width,
            inputs.available_space.height,
            inputs.parent_size.width,
            inputs.parent_size.height,
            children
        ));
    }

    // Clear cache to force re-measure
    for &child_idx in &children {
        if let Some(child) = tree.warm_mut(child_idx) {
            child.taffy_cache.clear();
        }
    }

    // SAFETY: We pass text_cache as a raw pointer because TaffyBridge needs to call
    // layout_ifc from within compute_child_layout, but we already have &mut ctx and &mut tree.
    // The pointer is only valid for the duration of this function call.
    let text_cache_ptr = core::ptr::from_mut::<crate::font_traits::TextLayoutCache>(text_cache);

    let mut bridge = TaffyBridge::new(ctx, tree, text_cache_ptr);
    let node = bridge.tree.get(node_idx).unwrap();

    let output = match node.formatting_context {
        FormattingContext::Flex => compute_flexbox_layout(&mut bridge, node_idx.into(), inputs),
        FormattingContext::Grid => compute_grid_layout(&mut bridge, node_idx.into(), inputs),
        _ => LayoutOutput::HIDDEN,
    };

    // DEBUG: Log Taffy output
    if bridge.ctx.debug_messages.is_some() {
        bridge.ctx.debug_info_inner(format!(
            "[TAFFY OUTPUT] node_idx={} output_size=({:?}, {:?})",
            node_idx, output.size.width, output.size.height
        ));

        // Log child layout results
        for &child_idx in &children {
            if let Some(child) = bridge.tree.get(child_idx) {
                bridge.ctx.debug_info_inner(format!(
                    "[TAFFY CHILD RESULT] child_idx={} used_size={:?} relative_pos={:?}",
                    child_idx, child.used_size, bridge.tree.warm(child_idx).and_then(|w| w.relative_position)
                ));
            }
        }
    }

    output
}

// --- Trait Implementations for the Bridge ---

impl<T: ParsedFontTrait> TraversePartialTree for TaffyBridge<'_, '_, T> {
    type ChildIter<'c>
        = std::vec::IntoIter<taffy::NodeId>
    where
        Self: 'c;

    fn child_ids(&self, node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        let node_idx: usize = node_id.into();
        let children = self.get_layout_children(node_idx);
        children
            .into_iter()
            .map(Into::into)
            .collect::<Vec<taffy::NodeId>>()
            .into_iter()
    }

    fn child_count(&self, node_id: taffy::NodeId) -> usize {
        let node_idx: usize = node_id.into();
        
        self.get_layout_children(node_idx).len()
    }

    fn get_child_id(&self, node_id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.get_layout_children(node_id.into())[index].into()
    }
}

impl<T: ParsedFontTrait> LayoutPartialTree for TaffyBridge<'_, '_, T> {
    type CoreContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type CustomIdent = String;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        let node_idx: usize = node_id.into();
        // Use get_taffy_style instead of translate_style_to_taffy to apply
        // cross-axis intrinsic suppression for stretch alignment
        self.get_taffy_style(node_idx)
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &Layout) {
        let node_idx: usize = node_id.into();

        // FIX: Retrieve parent border/padding to adjust position.
        // Taffy positions are relative to the parent's Border Box origin.
        // Azul expects positions relative to the parent's Content Box origin.
        // We must subtract the parent's border and padding from the Taffy-returned position.
        let (parent_border_left, parent_border_top, parent_padding_left, parent_padding_top) = {
            if let Some(child) = self.tree.get(node_idx) {
                if let Some(parent_idx) = child.parent {
                    self.tree.get(parent_idx).map_or((0.0, 0.0, 0.0, 0.0), |parent| {
                        let pbp = parent.box_props.unpack();
                        (
                            pbp.border.left,
                            pbp.border.top,
                            pbp.padding.left,
                            pbp.padding.top,
                        )
                    })
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                }
            } else {
                (0.0, 0.0, 0.0, 0.0)
            }
        };

        if let Some(node) = self.tree.get_mut(node_idx) {
            let size = translate_taffy_size_back(layout.size);
            let mut pos = translate_taffy_point_back(layout.location);

            // DEBUG: Log Taffy's raw layout result before adjustment
            if self.ctx.debug_messages.is_some() {
                self.ctx.debug_info_inner(format!(
                    "[TAFFY set_unrounded_layout] node_idx={} taffy_size=({:.2}, {:.2}) \
                     taffy_pos=({:.2}, {:.2}) parent_border=({:.2}, {:.2}) parent_padding=({:.2}, \
                     {:.2})",
                    node_idx,
                    layout.size.width,
                    layout.size.height,
                    layout.location.x,
                    layout.location.y,
                    parent_border_left,
                    parent_border_top,
                    parent_padding_left,
                    parent_padding_top
                ));
            }

            // Subtract parent's border and padding offset to convert
            // from border-box-relative to content-box-relative position
            pos.x -= parent_border_left + parent_padding_left;
            pos.y -= parent_border_top + parent_padding_top;

            node.used_size = Some(size);
        }
        if let Some(warm) = self.tree.warm_mut(node_idx) {
            let mut pos = translate_taffy_point_back(layout.location);
            pos.x -= parent_border_left + parent_padding_left;
            pos.y -= parent_border_top + parent_padding_top;
            warm.relative_position = Some(pos);
        }
    }

    fn resolve_calc_value(&self, val: *const (), basis: f32) -> f32 {
        // SAFETY: `val` came from `store_calc_and_make_dimension` which stored
        // a `Box<CalcResolveContext>` in `self.calc_storage`. The Box is alive for
        // the lifetime of this TaffyBridge, and taffy only clears the low 3 bits.
        let ctx = unsafe { &*val.cast::<CalcResolveContext>() };
        crate::solver3::calc::evaluate_calc(ctx, basis)
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: LayoutInput,
    ) -> LayoutOutput {
        let node_idx: usize = node_id.into();

        // DEBUG: Log the style being used for this child
        if self.ctx.debug_messages.is_some() {
            let style = self.get_taffy_style(node_idx);
            self.ctx.debug_info_inner(format!(
                "[TAFFY compute_child_layout] node_idx={} flex_grow={} flex_shrink={} \
                 flex_basis={:?} size=({:?}, {:?}) inputs.known_dims=({:?}, {:?})",
                node_idx,
                style.flex_grow,
                style.flex_shrink,
                style.flex_basis,
                style.size.width,
                style.size.height,
                inputs.known_dimensions.width,
                inputs.known_dimensions.height
            ));
        }

        // Get formatting context
        let fc = self
            .tree
            .get(node_idx)
            .map(|s| s.formatting_context)
            .unwrap_or_default();

        let mut result = compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let node_idx: usize = node_id.into();
            let fc = tree
                .tree
                .get(node_idx)
                .map(|s| s.formatting_context)
                .unwrap_or_default();

            match fc {
                FormattingContext::Flex => compute_flexbox_layout(tree, node_id, inputs),
                FormattingContext::Grid => compute_grid_layout(tree, node_id, inputs),
                // For Block, Inline, Table, InlineBlock - delegate to layout_formatting_context
                // This ensures proper recursive layout of all formatting contexts
                _ => tree.compute_non_flex_layout(node_idx, inputs),
            }
        });

        // Store layout for container nodes - Taffy only calls set_unrounded_layout for leaf nodes
        if let Some(node) = self.tree.get_mut(node_idx) {
            let size = translate_taffy_size_back(result.size);
            node.used_size = Some(size);
        }

        // CRITICAL FIX: For Flex/Grid children with overflow:auto/scroll,
        // compute scrollbar_info by comparing Taffy's content_size against the
        // CSS-specified container size.
        //
        // We skip when content_size is (0,0) because that's the sizing pass
        // where Taffy hasn't determined actual content size yet. The final
        // layout pass always has non-zero content_size for nodes that need
        // scroll. This avoids 2/3 of the compute_taffy_scrollbar_info calls
        // (one sizing pass per axis) while still getting correct final values.
        if matches!(fc, FormattingContext::Flex | FormattingContext::Grid) {
            let taffy_content_width = result.content_size.width;
            let taffy_content_height = result.content_size.height;

            // Skip on sizing pass where content_size is still zero:
            // scrollbar_info computed from zero content would be wrong anyway.
            if taffy_content_width <= 0.0 && taffy_content_height <= 0.0 {
                return result;
            }

            let (scrollbar_info, eff_content_w, eff_content_h) =
                compute_taffy_scrollbar_info(
                    self.ctx,
                    self.tree,
                    node_idx,
                    result.size.width,
                    result.size.height,
                    taffy_content_width,
                    taffy_content_height,
                );

            if let Some(warm) = self.tree.warm_mut(node_idx) {
                warm.scrollbar_info = Some(scrollbar_info);
                // eff_content_w/h are already in content-box coordinates
                // (border.left/top subtracted in compute_taffy_scrollbar_info),
                // so store directly without further subtraction.
                warm.overflow_content_size = Some(LogicalSize::new(
                    eff_content_w,
                    eff_content_h,
                ));
            }
        }

        result
    }
}

impl<T: ParsedFontTrait> TaffyBridge<'_, '_, T> {
    /// Compute layout for non-flex/grid nodes by delegating to `layout_formatting_context`.
    /// This handles Block, Inline, Table, `InlineBlock` formatting contexts recursively.
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn compute_non_flex_layout(&mut self, node_idx: usize, inputs: LayoutInput) -> LayoutOutput {
        // Taffy's known_dimensions are BORDER-BOX sizes (the child's outer size
        // as determined by the parent flex/grid algorithm, e.g. via stretch alignment).
        // Our BFC/IFC layout expects the available_size to be the CONTENT-BOX width
        // (i.e. the space available for the child's own content, excluding the child's
        // own padding and border).
        //
        // Get padding/border early so we can convert border-box → content-box.
        let (node_padding_width, node_padding_height, node_border_width, node_border_height) = self
            .tree
            .get(node_idx)
            .map_or((0.0, 0.0, 0.0, 0.0), |node| {
                let bp = node.box_props.unpack();
                (
                    bp.padding.left + bp.padding.right,
                    bp.padding.top + bp.padding.bottom,
                    bp.border.left + bp.border.right,
                    bp.border.top + bp.border.bottom,
                )
            });

        // Determine available size from Taffy's inputs.
        // When known_dimensions is set (e.g. flex stretch), subtract the child's own
        // padding+border to convert from border-box to content-box available space.
        // For MinContent/MaxContent, use INFINITY and let the text layout calculate
        // its actual intrinsic width.
        let available_width = inputs
            .known_dimensions
            .width
            .map(|kw| (kw - node_padding_width - node_border_width).max(0.0))
            .or(match inputs.available_space.width {
                AvailableSpace::Definite(w) => Some(w),
                AvailableSpace::MinContent => None, // Use infinity, return intrinsic min-content
                AvailableSpace::MaxContent => None, // Use infinity for max-content
            })
            .unwrap_or(f32::INFINITY);

        let available_height = inputs
            .known_dimensions
            .height
            .map(|kh| (kh - node_padding_height - node_border_height).max(0.0))
            .or(match inputs.available_space.height {
                AvailableSpace::Definite(h) => Some(h),
                AvailableSpace::MinContent => None, // Use infinity, return intrinsic min-content
                AvailableSpace::MaxContent => None,
            })
            .unwrap_or(f32::INFINITY);

        let mut available_size = LogicalSize {
            width: available_width,
            height: available_height,
        };

        // NOTE: Scrollbar reservation is handled inside layout_bfc() where it subtracts
        // scrollbar width from children_containing_block_size. We do NOT subtract here
        // to avoid double-subtraction when compute_non_flex_layout delegates to
        // layout_formatting_context → layout_bfc.

        // Convert Taffy's AvailableSpace to our Text3AvailableSpace for caching.
        // When the child has known_dimensions.width (from flex/grid layout), use that
        // instead of the parent's available_space — otherwise text centers/wraps in
        // the wrong width (e.g., 404px parent instead of 120px child).
        let available_width_type = if inputs.known_dimensions.width.is_some() {
            crate::text3::cache::AvailableSpace::Definite(available_width)
        } else {
            match inputs.available_space.width {
                AvailableSpace::Definite(w) => crate::text3::cache::AvailableSpace::Definite(w),
                AvailableSpace::MinContent => crate::text3::cache::AvailableSpace::MinContent,
                AvailableSpace::MaxContent => crate::text3::cache::AvailableSpace::MaxContent,
            }
        };

        // Get text-align from CSS for this node (important for centering content in flex items)
        let text_align = self
            .tree
            .get(node_idx)
            .and_then(|node| node.dom_node_id)
            .map(|dom_id| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
                crate::solver3::getters::get_text_align(self.ctx.styled_dom, dom_id, node_state)
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Convert CSS text-align to our internal TextAlign enum
        let fc_text_align = match text_align {
            azul_css::props::style::StyleTextAlign::Left => FcTextAlign::Start,
            azul_css::props::style::StyleTextAlign::Right => FcTextAlign::End,
            azul_css::props::style::StyleTextAlign::Center => FcTextAlign::Center,
            azul_css::props::style::StyleTextAlign::Justify => FcTextAlign::Justify,
            azul_css::props::style::StyleTextAlign::Start => FcTextAlign::Start,
            azul_css::props::style::StyleTextAlign::End => FcTextAlign::End,
        };

        // SAFETY: `self.text_cache` was derived from `&mut TextLayoutCache` in
        // `layout_taffy_subtree` and no other reference to it exists at this point.
        // The raw pointer is necessary because we already hold `&mut self` (which
        // borrows `ctx` and `tree`), and Rust's borrow checker cannot express the
        // disjointness of text_cache from ctx/tree.
        let text_cache = unsafe { &mut *self.text_cache };

        let constraints = LayoutConstraints {
            available_size,
            writing_mode: LayoutWritingMode::HorizontalTb,
            writing_mode_ctx: super::geometry::WritingModeContext::default(),
            bfc_state: None,
            text_align: fc_text_align,
            containing_block_size: available_size,
            available_width_type,
        };

        // Use a temporary float cache for this subtree
        let mut float_cache = HashMap::new();

        // Call layout_formatting_context - this handles ALL formatting context types
        // including nested flex/grid, tables, BFC, and IFC
        let fc_result = crate::solver3::fc::layout_formatting_context(
            self.ctx,
            self.tree,
            text_cache,
            node_idx,
            &constraints,
            &mut float_cache,
        );

        match fc_result {
            Ok(bfc_result) => {
                let output = bfc_result.output;
                let content_width = output.overflow_size.width;
                let content_height = output.overflow_size.height;

                // Padding/border already computed at start of function
                let padding_width = node_padding_width;
                let padding_height = node_padding_height;
                let border_width = node_border_width;
                let border_height = node_border_height;

                // Get intrinsic sizes for min/max-content queries
                let intrinsic = self
                    .tree
                    .warm(node_idx)
                    .and_then(|w| w.intrinsic_sizes)
                    .unwrap_or_default();

                // min-content size in the main axis; for items with a preferred aspect ratio, it
                // should be clamped by definite min/max cross sizes converted through the ratio.
                // For MinContent/MaxContent queries, use intrinsic sizes instead of layout result.
                // HOWEVER: If intrinsic sizes are 0 but content_width is non-zero, use content_width.
                // This happens for FormattingContext::Inline nodes that are measured by their
                // parent IFC root and don't have their own intrinsic sizes stored.
                //
                // CRITICAL FIX: For InlineBlock elements with width: auto (known_dimensions.width = None),
                // we must use intrinsic max-content width instead of content_width from BFC layout.
                // The BFC layout was done with the full container width, but InlineBlock should
                // shrink-to-fit its content. This is per CSS 2.1 § 10.3.9: "shrink-to-fit width".
                let fc = self
                    .tree
                    .get(node_idx)
                    .map(|s| s.formatting_context)
                    .unwrap_or_default();
                
                let is_shrink_to_fit = matches!(fc, FormattingContext::InlineBlock)
                    && inputs.known_dimensions.width.is_none();
                
                let effective_content_width = match inputs.available_space.width {
                    AvailableSpace::MinContent => {
                        if intrinsic.min_content_width > 0.0 {
                            intrinsic.min_content_width
                        } else {
                            content_width
                        }
                    }
                    AvailableSpace::MaxContent => {
                        if intrinsic.max_content_width > 0.0 {
                            intrinsic.max_content_width
                        } else {
                            content_width
                        }
                    }
                    AvailableSpace::Definite(_) => {
                        // For shrink-to-fit elements (InlineBlock with auto width),
                        // use intrinsic max-content width clamped by available space.
                        // CSS 2.1 § 10.3.9: shrink-to-fit = min(max(preferred minimum, available), preferred)
                        if is_shrink_to_fit && intrinsic.max_content_width > 0.0 {
                            // Use max-content (preferred width) - already clamped by min/max-width in sizing
                            intrinsic.max_content_width
                        } else {
                            content_width
                        }
                    }
                };

                // Replaced elements (image / VirtualView) have NO flow content, so the
                // BFC content_height above is 0 (and shrink-to-fit width may be wrong).
                // Their content size is the CSS/intrinsic-resolved size from
                // calculate_used_size_for_node (border-box) — strip padding+border back
                // to content-box. Fixes blank / 0-height images as flex/grid items.
                let (effective_content_width, content_height) = {
                    let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
                    let is_replaced = dom_id
                        .is_some_and(|id| {
                            let nd = &self.ctx.styled_dom.node_data.as_container()[id];
                            matches!(nd.get_node_type(), azul_core::dom::NodeType::Image(_))
                                || nd.is_virtual_view_node()
                        });
                    match (is_replaced, dom_id) {
                        (true, Some(id)) => {
                            let bp = self.tree.get(node_idx).unwrap().box_props.unpack();
                            crate::solver3::sizing::calculate_used_size_for_node(
                                self.ctx.styled_dom,
                                Some(id),
                                &constraints.containing_block_size,
                                intrinsic,
                                &bp,
                                &self.ctx.viewport_size,
                            ).map_or((effective_content_width, content_height), |sz| (
                                    (sz.width - padding_width - border_width).max(0.0),
                                    (sz.height - padding_height - border_height).max(0.0),
                                ))
                        }
                        _ => (effective_content_width, content_height),
                    }
                };

                // Convert content-box size to border-box size (for when we compute our own size)
                let border_box_width = effective_content_width + padding_width + border_width;
                let border_box_height = content_height + padding_height + border_height;

                // CRITICAL: Taffy's known_dimensions is BORDER-BOX (the child's
                // outer size as set by the parent flex/grid algorithm). Our BFC/IFC
                // layout computes content-box sizes, but Taffy expects the returned
                // `size` to be BORDER-BOX for correct positioning of subsequent items.
                //
                // When known_dimensions is set: use it directly (it's already border-box).
                // When it's None: add padding+border to our content-box result.
                let final_width = inputs.known_dimensions.width.map_or(border_box_width, |border_box_w| border_box_w);

                // For grid items: if known_dimensions.height is None but available_space.height
                // is definite, use the available space. This ensures empty grid items stretch
                // to fill their grid cell, per CSS Grid spec behavior.
                let final_height = if let Some(border_box_h) = inputs.known_dimensions.height { border_box_h } else {
                    // Check if parent is a grid container and available_space is definite
                    let parent_is_grid = self
                        .tree
                        .get(node_idx)
                        .and_then(|n| n.parent)
                        .and_then(|p| self.tree.get(p))
                        .is_some_and(|p| matches!(p.formatting_context, FormattingContext::Grid));

                    if parent_is_grid {
                        // For grid items, use available space if content is smaller
                        match inputs.available_space.height {
                            AvailableSpace::Definite(h) => {
                                // Grid items stretch to fill their cell by default
                                // Use the larger of content size or available space
                                h.max(border_box_height)
                            }
                            _ => border_box_height,
                        }
                    } else {
                        border_box_height
                    }
                };

                // CRITICAL: Transfer positions from layout_formatting_context to child nodes.
                // Without this, children of flex items won't have their relative_position set,
                // causing them to all render at (0,0) relative to their parent.
                for (child_idx, child_pos) in &output.positions {
                    if let Some(child_warm) = self.tree.warm_mut(*child_idx) {
                        child_warm.relative_position = Some(*child_pos);
                    }
                }

                // Compute scrollbar_info for this node (it's a child of a Flex/Grid container,
                // so calculate_layout_for_subtree won't be called for it).
                // Uses the unified compute_scrollbar_info_core path.
                let (scrollbar_info, _, _) = compute_taffy_scrollbar_info(
                    self.ctx,
                    self.tree,
                    node_idx,
                    final_width,
                    final_height,
                    content_width,
                    content_height,
                );

                // Store the border-box size and scrollbar_info on the node for display list generation
                if let Some(node) = self.tree.get_mut(node_idx) {
                    node.used_size = Some(LogicalSize {
                        width: final_width,
                        height: final_height,
                    });
                }
                if let Some(warm) = self.tree.warm_mut(node_idx) {
                    warm.scrollbar_info = Some(scrollbar_info);
                    // Store the actual content size for scroll calculations
                    warm.overflow_content_size = Some(LogicalSize {
                        width: content_width,
                        height: content_height,
                    });
                }

                // Return the same size to Taffy for correct positioning
                LayoutOutput {
                    size: Size {
                        width: final_width,
                        height: final_height,
                    },
                    content_size: Size {
                        width: content_width,
                        height: content_height,
                    },
                    first_baselines: taffy::Point {
                        x: None,
                        y: output.baseline,
                    },
                    top_margin: taffy::CollapsibleMarginSet::ZERO,
                    bottom_margin: taffy::CollapsibleMarginSet::ZERO,
                    margins_can_collapse_through: false,
                }
            }
            Err(_e) => {
                // Fallback to intrinsic sizes if layout fails
                let intrinsic = self.tree.warm(node_idx).and_then(|w| w.intrinsic_sizes).unwrap_or_default();

                let width = inputs
                    .known_dimensions
                    .width
                    .unwrap_or(intrinsic.max_content_width);
                let height = inputs
                    .known_dimensions
                    .height
                    .unwrap_or(intrinsic.max_content_height);

                LayoutOutput {
                    size: Size { width, height },
                    content_size: Size { width, height },
                    first_baselines: taffy::Point { x: None, y: None },
                    top_margin: taffy::CollapsibleMarginSet::ZERO,
                    bottom_margin: taffy::CollapsibleMarginSet::ZERO,
                    margins_can_collapse_through: false,
                }
            }
        }
    }
}

impl<T: ParsedFontTrait> CacheTree for TaffyBridge<'_, '_, T> {
    fn cache_get(
        &self,
        node_id: taffy::NodeId,
        input: &LayoutInput,
    ) -> Option<LayoutOutput> {
        let node_idx: usize = node_id.into();
        self.tree
            .warm(node_idx)?
            .taffy_cache
            .get(input)
    }

    fn cache_store(
        &mut self,
        node_id: taffy::NodeId,
        input: &LayoutInput,
        layout_output: LayoutOutput,
    ) {
        let node_idx: usize = node_id.into();
        if let Some(warm) = self.tree.warm_mut(node_idx) {
            warm.taffy_cache
                .store(input, layout_output);
        }
    }

    fn cache_clear(&mut self, node_id: taffy::NodeId) {
        let node_idx: usize = node_id.into();
        if let Some(warm) = self.tree.warm_mut(node_idx) {
            warm.taffy_cache.clear();
        }
    }
}

impl<T: ParsedFontTrait> LayoutFlexboxContainer for TaffyBridge<'_, '_, T> {
    type FlexboxContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type FlexboxItemStyle<'c>
        = Style
    where
        Self: 'c;

    fn get_flexbox_container_style(
        &self,
        node_id: taffy::NodeId,
    ) -> Self::FlexboxContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_flexbox_child_style(&self, child_node_id: taffy::NodeId) -> Self::FlexboxItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl<T: ParsedFontTrait> LayoutGridContainer for TaffyBridge<'_, '_, T> {
    type GridContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type GridItemStyle<'c>
        = Style
    where
        Self: 'c;

    fn get_grid_container_style(&self, node_id: taffy::NodeId) -> Self::GridContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_grid_child_style(&self, child_node_id: taffy::NodeId) -> Self::GridItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

// --- Conversion Functions ---

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::vec_box)] // calc_storage Box gives stable addresses for taffy calc() pointers
fn from_layout_width(
    val: LayoutWidth,
    calc_storage: &std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    em_size: f32,
    rem_size: f32,
) -> Dimension {
    match val {
        LayoutWidth::Auto => Dimension::auto(),
        LayoutWidth::Px(px) => pixel_value_to_pixels_fallback(&px).map_or_else(
            || px.to_percent().map_or_else(Dimension::auto, |p| Dimension::percent(p.get())),
            Dimension::length,
        ),
        LayoutWidth::MinContent | LayoutWidth::MaxContent | LayoutWidth::FitContent(_) => Dimension::auto(),
        LayoutWidth::Calc(items) => store_calc_and_make_dimension(items, calc_storage, em_size, rem_size),
    }
}

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::vec_box)] // calc_storage Box gives stable addresses for taffy calc() pointers
fn from_layout_height(
    val: LayoutHeight,
    calc_storage: &std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    em_size: f32,
    rem_size: f32,
) -> Dimension {
    match val {
        LayoutHeight::Auto => Dimension::auto(),
        LayoutHeight::Px(px) => pixel_value_to_pixels_fallback(&px).map_or_else(
            || px.to_percent().map_or_else(Dimension::auto, |p| Dimension::percent(p.get())),
            Dimension::length,
        ),
        LayoutHeight::MinContent | LayoutHeight::MaxContent | LayoutHeight::FitContent(_) => Dimension::auto(),
        LayoutHeight::Calc(items) => store_calc_and_make_dimension(items, calc_storage, em_size, rem_size),
    }
}

/// Stores the calc AST + font-size context in heap-pinned storage and returns
/// a `Dimension::calc(ptr)` with a stable pointer to the `CalcResolveContext`.
///
/// The `Box` ensures the address doesn't move when the outer `Vec` reallocates.
/// The `RefCell<Vec<…>>` keeps all boxes alive for the layout pass duration.
#[allow(clippy::vec_box)] // calc_storage Box gives stable addresses for taffy calc() pointers
fn store_calc_and_make_dimension(
    items: CalcAstItemVec,
    storage: &std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    em_size: f32,
    rem_size: f32,
) -> Dimension {
    let boxed = Box::new(CalcResolveContext { items, em_size, rem_size });
    let ptr: *const CalcResolveContext = &raw const *boxed;
    storage.borrow_mut().push(boxed);
    // SAFETY: Box gives ≥8-byte-aligned heap pointer; taffy masks low 3 bits.
    Dimension::calc(ptr.cast::<()>())
}

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn from_layout_position(val: LayoutPosition) -> Position {
    match val {
        LayoutPosition::Static => Position::Relative, // Taffy treats Static as Relative
        LayoutPosition::Relative => Position::Relative,
        LayoutPosition::Absolute => Position::Absolute,
        LayoutPosition::Fixed => Position::Absolute, // Taffy doesn't distinguish Fixed
        LayoutPosition::Sticky => Position::Relative, // Sticky = Relative for Taffy
    }
}

