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

/// Convert PixelValue to pixels, only for absolute units (no %, and em/rem use fallback)
/// Used where proper resolution context is not available (grid tracks, etc.)
fn pixel_value_to_pixels_fallback(pv: &PixelValue) -> Option<f32> {
    match pv.metric {
        SizeMetric::Px => Some(pv.number.get()),
        SizeMetric::Pt => Some(pv.number.get() * PT_TO_PX),
        SizeMetric::In => Some(pv.number.get() * 96.0),
        SizeMetric::Cm => Some(pv.number.get() * 96.0 / 2.54),
        SizeMetric::Mm => Some(pv.number.get() * 96.0 / 25.4),
        // For em/rem, use DEFAULT_FONT_SIZE as fallback (not ideal but needed without context)
        SizeMetric::Em | SizeMetric::Rem => Some(pv.number.get() * DEFAULT_FONT_SIZE),
        SizeMetric::Percent => None, // Cannot resolve without containing block
        // Viewport units: Cannot resolve without viewport context
        SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => None,
    }
}

pub fn grid_template_rows_to_taffy(
    val: LayoutGridTemplateRowsValue,
) -> Vec<taffy::GridTemplateComponent<String>> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks
        .tracks
        .iter()
        .map(|track| taffy::GridTemplateComponent::Single(translate_track(track)))
        .collect()
}

pub fn grid_template_columns_to_taffy(
    val: LayoutGridTemplateColumnsValue,
) -> Vec<taffy::GridTemplateComponent<String>> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks
        .tracks
        .iter()
        .map(|track| taffy::GridTemplateComponent::Single(translate_track(track)))
        .collect()
}

pub fn grid_auto_rows_to_taffy(
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

pub fn grid_auto_columns_to_taffy(
    val: LayoutGridAutoColumnsValue,
) -> Vec<taffy::TrackSizingFunction> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks.tracks.iter().map(translate_track).collect()
}

fn translate_track(track: &GridTrackSizing) -> taffy::TrackSizingFunction {
    // Helper to resolve PixelValue to absolute pixels (handles em, rem, but not %)
    // Grid track sizing in Taffy doesn't support % - only absolute values
    let px_to_float = |pv: PixelValue| -> f32 {
        // Only accept absolute units (px, pt, in, cm, mm) - no %, em, rem
        // TODO: Add proper context for em/rem resolution
        match pv.metric {
            SizeMetric::Px => pv.number.get(),
            SizeMetric::Pt => pv.number.get() * PT_TO_PX,
            SizeMetric::In => pv.number.get() * 96.0,
            SizeMetric::Cm => pv.number.get() * 96.0 / 2.54,
            SizeMetric::Mm => pv.number.get() * 96.0 / 25.4,
            // For em/rem, use DEFAULT_FONT_SIZE as fallback
            SizeMetric::Em | SizeMetric::Rem => pv.number.get() * DEFAULT_FONT_SIZE,
            SizeMetric::Percent => 0.0, // Not supported in grid tracks
            // Viewport units: Cannot resolve without viewport context, default to 0
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => 0.0,
        }
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

fn minmax(min: MinTrackSizingFunction, max: MaxTrackSizingFunction) -> taffy::TrackSizingFunction {
    TrackSizingFunction { min, max }
}

pub fn layout_display_to_taffy(val: LayoutDisplayValue) -> taffy::Display {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutDisplay::None => taffy::Display::None,
        LayoutDisplay::Flex | LayoutDisplay::InlineFlex => taffy::Display::Flex,
        LayoutDisplay::Grid | LayoutDisplay::InlineGrid => taffy::Display::Grid,
        _ => taffy::Display::Block,
    }
}

pub fn layout_position_to_taffy(val: LayoutPositionValue) -> taffy::Position {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutPosition::Absolute => taffy::Position::Absolute,
        LayoutPosition::Fixed => taffy::Position::Absolute, // Taffy kennt kein Fixed
        LayoutPosition::Relative => taffy::Position::Relative,
        LayoutPosition::Static => taffy::Position::Relative,
        LayoutPosition::Sticky => taffy::Position::Relative, // Sticky wird als Relative behandelt
    }
}

pub fn grid_auto_flow_to_taffy(val: LayoutGridAutoFlowValue) -> taffy::GridAutoFlow {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutGridAutoFlow::Row => taffy::GridAutoFlow::Row,
        LayoutGridAutoFlow::Column => taffy::GridAutoFlow::Column,
        LayoutGridAutoFlow::RowDense => taffy::GridAutoFlow::RowDense,
        LayoutGridAutoFlow::ColumnDense => taffy::GridAutoFlow::ColumnDense,
    }
}

/// Convert an azul `GridLine` (single start or end) to a Taffy `GridPlacement`.
fn grid_line_to_taffy(
    line: &azul_css::props::layout::grid::GridLine,
) -> taffy::style::GridPlacement<String> {
    use azul_css::props::layout::grid::GridLine as AzGridLine;
    use taffy::style_helpers::{TaffyGridLine, TaffyGridSpan};
    match line {
        AzGridLine::Auto => taffy::style::GridPlacement::Auto,
        AzGridLine::Line(n) => {
            taffy::style::GridPlacement::<String>::from_line_index(*n as i16)
        }
        AzGridLine::Span(n) => taffy::style::GridPlacement::<String>::from_span(*n as u16),
        AzGridLine::Named(named) => {
            // Named lines: use the name with optional span
            let name = named.grid_line_name.as_str().to_string();
            if named.span_count > 0 {
                taffy::style::GridPlacement::NamedSpan(name, named.span_count as u16)
            } else {
                taffy::style::GridPlacement::NamedLine(name, 0)
            }
        }
    }
}

/// Convert an azul `GridPlacement` (grid-column / grid-row) to a Taffy `Line<GridPlacement>`.
fn grid_placement_to_taffy(
    placement: &azul_css::props::layout::grid::GridPlacement,
) -> taffy::Line<taffy::style::GridPlacement<String>> {
    taffy::Line {
        start: grid_line_to_taffy(&placement.grid_start),
        end: grid_line_to_taffy(&placement.grid_end),
    }
}

pub fn layout_flex_direction_to_taffy(val: LayoutFlexDirectionValue) -> taffy::FlexDirection {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutFlexDirection::Row => taffy::FlexDirection::Row,
        LayoutFlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
        LayoutFlexDirection::Column => taffy::FlexDirection::Column,
        LayoutFlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
    }
}

pub fn layout_flex_wrap_to_taffy(val: LayoutFlexWrapValue) -> taffy::FlexWrap {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutFlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
        LayoutFlexWrap::Wrap => taffy::FlexWrap::Wrap,
        LayoutFlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
    }
}

pub fn layout_align_items_to_taffy(val: LayoutAlignItemsValue) -> taffy::AlignItems {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignItems::Stretch => taffy::AlignItems::Stretch,
        LayoutAlignItems::Center => taffy::AlignItems::Center,
        LayoutAlignItems::Start => taffy::AlignItems::FlexStart,
        LayoutAlignItems::End => taffy::AlignItems::FlexEnd,
        LayoutAlignItems::Baseline => taffy::AlignItems::Baseline,
    }
}

pub fn layout_align_self_to_taffy(val: LayoutAlignSelfValue) -> Option<taffy::AlignSelf> {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignSelf::Auto => None, // Auto means inherit from parent's align-items
        LayoutAlignSelf::Start => Some(taffy::AlignSelf::FlexStart),
        LayoutAlignSelf::End => Some(taffy::AlignSelf::FlexEnd),
        LayoutAlignSelf::Center => Some(taffy::AlignSelf::Center),
        LayoutAlignSelf::Baseline => Some(taffy::AlignSelf::Baseline),
        LayoutAlignSelf::Stretch => Some(taffy::AlignSelf::Stretch),
    }
}

pub fn layout_align_content_to_taffy(val: LayoutAlignContentValue) -> taffy::AlignContent {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignContent::Start => taffy::AlignContent::FlexStart,
        LayoutAlignContent::End => taffy::AlignContent::FlexEnd,
        LayoutAlignContent::Center => taffy::AlignContent::Center,
        LayoutAlignContent::Stretch => taffy::AlignContent::Stretch,
        LayoutAlignContent::SpaceBetween => taffy::AlignContent::SpaceBetween,
        LayoutAlignContent::SpaceAround => taffy::AlignContent::SpaceAround,
    }
}

pub fn layout_justify_content_to_taffy(val: LayoutJustifyContentValue) -> taffy::JustifyContent {
    match val.get_property_or_default().unwrap_or_default() {
        LayoutJustifyContent::FlexStart => taffy::JustifyContent::FlexStart,
        LayoutJustifyContent::FlexEnd => taffy::JustifyContent::FlexEnd,
        LayoutJustifyContent::Start => taffy::JustifyContent::Start,
        LayoutJustifyContent::End => taffy::JustifyContent::End,
        LayoutJustifyContent::Center => taffy::JustifyContent::Center,
        LayoutJustifyContent::SpaceBetween => taffy::JustifyContent::SpaceBetween,
        LayoutJustifyContent::SpaceAround => taffy::JustifyContent::SpaceAround,
        LayoutJustifyContent::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
    }
}

pub fn layout_justify_items_to_taffy(
    val: azul_css::props::property::LayoutJustifyItemsValue,
) -> taffy::AlignItems {
    use azul_css::props::layout::grid::LayoutJustifyItems;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutJustifyItems::Start => taffy::AlignItems::Start,
        LayoutJustifyItems::End => taffy::AlignItems::End,
        LayoutJustifyItems::Center => taffy::AlignItems::Center,
        LayoutJustifyItems::Stretch => taffy::AlignItems::Stretch,
    }
}

// TODO: gap, grid, visibility, z_index, flex_basis, etc. analog ergänzen
// --- CSS <-> Taffy Übersetzungsfunktionen ---

use std::{collections::BTreeMap, sync::Arc};

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
        layout_tree::{get_display_type, LayoutNode, LayoutTree},
        sizing, LayoutContext,
    },
};

// Helper function to convert MultiValue<PixelValue> to LengthPercentageAuto
fn multi_value_to_lpa(mv: MultiValue<PixelValue>) -> taffy::LengthPercentageAuto {
    match mv {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            taffy::LengthPercentageAuto::auto()
        }
        MultiValue::Exact(pv) => pixel_value_to_pixels_fallback(&pv)
            .map(taffy::LengthPercentageAuto::length)
            .or_else(|| {
                pv.to_percent()
                    .map(|p| taffy::LengthPercentageAuto::percent(p.get()))
            })
            .unwrap_or_else(taffy::LengthPercentageAuto::auto),
    }
}

// Helper function to convert MultiValue<PixelValue> to LengthPercentageAuto for margins
// CSS spec: margin initial value is 0, but `auto` has special centering meaning in flexbox
fn multi_value_to_lpa_margin(mv: MultiValue<PixelValue>) -> taffy::LengthPercentageAuto {
    match mv {
        MultiValue::Auto => {
            taffy::LengthPercentageAuto::auto() // Preserve auto for flexbox centering
        }
        MultiValue::Initial | MultiValue::Inherit => {
            taffy::LengthPercentageAuto::length(0.0) // Margins' initial value is 0
        }
        MultiValue::Exact(pv) => {
            pixel_value_to_pixels_fallback(&pv)
                .map(taffy::LengthPercentageAuto::length)
                .or_else(|| {
                    pv.to_percent()
                        .map(|p| taffy::LengthPercentageAuto::percent(p.get()))
                })
                .unwrap_or_else(|| taffy::LengthPercentageAuto::length(0.0)) // Fallback to 0 for
                                                                             // margins
        }
    }
}

// Helper function to convert MultiValue<PixelValue> to LengthPercentage
fn multi_value_to_lp(mv: MultiValue<PixelValue>) -> taffy::LengthPercentage {
    match mv {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            taffy::LengthPercentage::ZERO
        }
        MultiValue::Exact(pv) => pixel_value_to_pixels_fallback(&pv)
            .map(taffy::LengthPercentage::length)
            .or_else(|| {
                pv.to_percent()
                    .map(|p| taffy::LengthPercentage::percent(p.get()))
            })
            .unwrap_or_else(|| taffy::LengthPercentage::ZERO),
    }
}

// Helper function to convert plain PixelValue to LengthPercentage
fn pixel_to_lp(pv: PixelValue) -> taffy::LengthPercentage {
    pixel_value_to_pixels_fallback(&pv)
        .map(taffy::LengthPercentage::length)
        .or_else(|| {
            pv.to_percent()
                .map(|p| taffy::LengthPercentage::percent(p.get()))
        })
        .unwrap_or_else(|| taffy::LengthPercentage::ZERO)
}

/// Slow path for flex-basis: full property cache lookup + decode.
/// Extracted to avoid duplicating the logic in the compact fast-path fallback.
fn flex_basis_slow_path(
    cache: &azul_core::prop_cache::CssPropertyCache,
    node_data: &azul_core::dom::NodeData,
    id: &NodeId,
    node_state: &azul_core::styled_dom::StyledNodeState,
    taffy_style: &mut Style,
) -> taffy::Dimension {
    cache
        .get_property(node_data, id, node_state, &CssPropertyType::FlexBasis)
        .and_then(|p| {
            if let CssProperty::FlexBasis(v) = p {
                let basis = match v.get_property_or_default().unwrap_or_default() {
                    LayoutFlexBasis::Auto => taffy::Dimension::auto(),
                    LayoutFlexBasis::Exact(pv) => pixel_value_to_pixels_fallback(&pv)
                        .map(taffy::Dimension::length)
                        .or_else(|| pv.to_percent().map(|p| taffy::Dimension::percent(p.get())))
                        .unwrap_or_else(taffy::Dimension::auto),
                };
                // WORKAROUND: If flex-basis is set and not auto, clear width to let flex-basis
                // take precedence. Workaround for Taffy not properly prioritizing flex-basis over width
                if !matches!(basis, _auto if _auto == taffy::Dimension::auto()) {
                    taffy_style.size.width = taffy::Dimension::auto();
                }
                Some(basis)
            } else {
                None
            }
        })
        .unwrap_or_else(taffy::Dimension::auto)
}

/// The bridge struct that implements Taffy's traits.
/// It holds mutable references to the solver's data structures, allowing Taffy
/// to read styles and write layout results back into our `LayoutTree`.
struct TaffyBridge<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    tree: &'a mut LayoutTree,
    /// Raw pointer to text cache - needed because we can't have multiple &mut references
    /// SAFETY: This pointer is only valid for the lifetime of the TaffyBridge
    /// and must only be used within compute_child_layout callbacks
    text_cache: *mut crate::font_traits::TextLayoutCache,
    /// Heap-pinned `CalcResolveContext`s whose addresses are passed into taffy
    /// `Dimension::calc(ptr)`. Kept alive for the duration of the layout pass.
    /// Uses `RefCell` because `get_core_container_style` takes `&self`.
    calc_storage: std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
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
        }
    }

    /// Translates CSS properties from the `StyledDom` into a `taffy::Style` struct.
    /// This is the core of the integration, mapping one style system to another.
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
            azul_css::props::layout::LayoutBoxSizing::BorderBox => taffy::BoxSizing::BorderBox,
            azul_css::props::layout::LayoutBoxSizing::ContentBox => taffy::BoxSizing::ContentBox,
        };

        // Display Mode
        taffy_style.display =
            layout_display_to_taffy(CssPropertyValue::Exact(get_display_type(styled_dom, id)));

        // Position
        taffy_style.position =
            from_layout_position(get_position(styled_dom, id, node_state).unwrap_or_default());

        // Inset (top, left, bottom, right)
        taffy_style.inset = taffy::Rect {
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

        taffy_style.size = taffy::Size {
            width: taffy_width,
            height: taffy_height,
        };

        // Min/Max Size
        // NOTE: In CSS, the default min-width/min-height for flex items is `auto`
        // (which resolves to `min-content`), preventing them from shrinking below
        // their content size. We must map Auto to Dimension::Auto, NOT to 0px.
        let min_width_css = get_css_min_width(styled_dom, id, node_state);
        let min_height_css = get_css_min_height(styled_dom, id, node_state);

        taffy_style.min_size = taffy::Size {
            width: match min_width_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    taffy::Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
            height: match min_height_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    taffy::Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
        };

        // For max-size, we need to handle Auto specially - it should translate to Taffy's auto, not
        // a concrete value This is CRITICAL for flexbox stretch to work: items with
        // max-height: auto CAN be stretched
        let max_width_css = get_css_max_width(styled_dom, id, node_state);
        let max_height_css = get_css_max_height(styled_dom, id, node_state);

        taffy_style.max_size = taffy::Size {
            width: match max_width_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    taffy::Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
            height: match max_height_css {
                MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
                    taffy::Dimension::auto()
                }
                MultiValue::Exact(v) => pixel_to_lp(v.inner).into(),
            },
        };

        // Box Model (margin, padding, border)
        let margin_left_css = get_css_margin_left(styled_dom, id, node_state);
        let margin_right_css = get_css_margin_right(styled_dom, id, node_state);
        let margin_top_css = get_css_margin_top(styled_dom, id, node_state);
        let margin_bottom_css = get_css_margin_bottom(styled_dom, id, node_state);

        taffy_style.margin = taffy::Rect {
            left: multi_value_to_lpa_margin(margin_left_css),
            right: multi_value_to_lpa_margin(margin_right_css),
            top: multi_value_to_lpa_margin(margin_top_css),
            bottom: multi_value_to_lpa_margin(margin_bottom_css),
        };

        taffy_style.padding = taffy::Rect {
            left: multi_value_to_lp(get_css_padding_left(styled_dom, id, node_state)),
            right: multi_value_to_lp(get_css_padding_right(styled_dom, id, node_state)),
            top: multi_value_to_lp(get_css_padding_top(styled_dom, id, node_state)),
            bottom: multi_value_to_lp(get_css_padding_bottom(styled_dom, id, node_state)),
        };

        taffy_style.border = taffy::Rect {
            left: multi_value_to_lp(get_css_border_left_width(styled_dom, id, node_state)),
            right: multi_value_to_lp(get_css_border_right_width(styled_dom, id, node_state)),
            top: multi_value_to_lp(get_css_border_top_width(styled_dom, id, node_state)),
            bottom: multi_value_to_lp(get_css_border_bottom_width(styled_dom, id, node_state)),
        };

        // Grid & gap properties
        taffy_style.gap = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::Gap)
            .and_then(|p| {
                if let CssProperty::Gap(v) = p {
                    Some(v)
                } else {
                    None
                }
            })
            .map(|v| {
                let val = v.get_property_or_default().unwrap_or_default().inner;
                // Gap can use %, em, rem - convert properly
                let gap_lp = pixel_to_lp(val);
                Size {
                    width: gap_lp,
                    height: gap_lp,
                }
            })
            .unwrap_or_else(Size::zero);

        // Grid template rows - convert GridTemplate to Vec<GridTemplateComponent>
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
            .map(|v| grid_template_rows_to_taffy(v).into())
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
            .map(|v| grid_template_columns_to_taffy(v).into())
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
                    .into()
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
            .map(|v| grid_auto_rows_to_taffy(v))
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
            .map(|v| grid_auto_columns_to_taffy(v))
            .unwrap_or_default();

        taffy_style.grid_auto_flow = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::GridAutoFlow)
            .and_then(|p| {
                if let CssProperty::GridAutoFlow(v) = p {
                    Some(*v)
                } else {
                    None
                }
            })
            .map(|v| grid_auto_flow_to_taffy(v))
            .unwrap_or_default();

        // Grid item placement (grid-column, grid-row)
        if let Some(grid_col) = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::GridColumn)
            .and_then(|p| {
                if let CssProperty::GridColumn(v) = p {
                    v.get_property().cloned()
                } else {
                    None
                }
            })
        {
            taffy_style.grid_column = grid_placement_to_taffy(&grid_col);
        }

        if let Some(grid_row) = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::GridRow)
            .and_then(|p| {
                if let CssProperty::GridRow(v) = p {
                    v.get_property().cloned()
                } else {
                    None
                }
            })
        {
            taffy_style.grid_row = grid_placement_to_taffy(&grid_row);
        }

        // Flexbox
        taffy_style.flex_direction = match get_flex_direction(styled_dom, id, node_state) {
            MultiValue::Exact(v) => layout_flex_direction_to_taffy(CssPropertyValue::Exact(v)),
            _ => taffy::FlexDirection::Row,
        };
        // COMPACT FAST PATH: flex_wrap is Tier 1 enum
        taffy_style.flex_wrap = if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                layout_flex_wrap_to_taffy(CssPropertyValue::Exact(cc.get_flex_wrap(id.index())))
            } else {
                cache
                    .get_property(node_data, &id, node_state, &CssPropertyType::FlexWrap)
                    .and_then(|p| if let CssProperty::FlexWrap(v) = p { Some(*v) } else { None })
                    .map(layout_flex_wrap_to_taffy)
                    .unwrap_or(taffy::FlexWrap::NoWrap)
            }
        } else {
            cache
                .get_property(node_data, &id, node_state, &CssPropertyType::FlexWrap)
                .and_then(|p| if let CssProperty::FlexWrap(v) = p { Some(*v) } else { None })
                .map(layout_flex_wrap_to_taffy)
                .unwrap_or(taffy::FlexWrap::NoWrap)
        };
        taffy_style.align_items = match get_align_items(styled_dom, id, node_state) {
            MultiValue::Exact(v) => Some(layout_align_items_to_taffy(CssPropertyValue::Exact(v))),
            _ => None,
        };
                // CSS spec: default align-items is "normal" which acts like "stretch"
                // for non-replaced grid/flex items. Taffy handles this internally when
                // align_items is None, so we should NOT force a default here.
        taffy_style.justify_items = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::JustifyItems)
            .and_then(|p| {
                if let CssProperty::JustifyItems(v) = p {
                    Some(*v)
                } else {
                    None
                }
            })
            .map(|v| layout_justify_items_to_taffy(v));
        taffy_style.justify_content = cache
                .get_property(node_data, &id, node_state, &CssPropertyType::JustifyContent)
                .and_then(|p| {
                    if let CssProperty::JustifyContent(v) = p {
                        Some(*v)
                    } else {
                        None
                    }
                })
                .map(layout_justify_content_to_taffy);
                // CSS spec: default justify-content is "normal". Taffy handles
                // this internally when justify_content is None.
        // COMPACT FAST PATH: flex_grow stored as u16 × 100
        taffy_style.flex_grow = if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(v) = cc.get_flex_grow(id.index()) {
                    v
                } else {
                    // Sentinel: fall through to slow path
                    cache
                        .get_property(node_data, &id, node_state, &CssPropertyType::FlexGrow)
                        .and_then(|p| if let CssProperty::FlexGrow(v) = p {
                            Some(v.get_property_or_default().unwrap_or_default().inner.get())
                        } else { None })
                        .unwrap_or(0.0)
                }
            } else {
                cache
                    .get_property(node_data, &id, node_state, &CssPropertyType::FlexGrow)
                    .and_then(|p| if let CssProperty::FlexGrow(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner.get())
                    } else { None })
                    .unwrap_or(0.0)
            }
        } else {
            cache
                .get_property(node_data, &id, node_state, &CssPropertyType::FlexGrow)
                .and_then(|p| if let CssProperty::FlexGrow(v) = p {
                    Some(v.get_property_or_default().unwrap_or_default().inner.get())
                } else { None })
                .unwrap_or(0.0)
        };

        // COMPACT FAST PATH: flex_shrink stored as u16 × 100
        taffy_style.flex_shrink = if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(v) = cc.get_flex_shrink(id.index()) {
                    v
                } else {
                    // Sentinel: fall through to slow path
                    cache
                        .get_property(node_data, &id, node_state, &CssPropertyType::FlexShrink)
                        .and_then(|p| if let CssProperty::FlexShrink(v) = p {
                            Some(v.get_property_or_default().unwrap_or_default().inner.get())
                        } else { None })
                        .unwrap_or(1.0)
                }
            } else {
                cache
                    .get_property(node_data, &id, node_state, &CssPropertyType::FlexShrink)
                    .and_then(|p| if let CssProperty::FlexShrink(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner.get())
                    } else { None })
                    .unwrap_or(1.0)
            }
        } else {
            cache
                .get_property(node_data, &id, node_state, &CssPropertyType::FlexShrink)
                .and_then(|p| if let CssProperty::FlexShrink(v) = p {
                    Some(v.get_property_or_default().unwrap_or_default().inner.get())
                } else { None })
                .unwrap_or(1.0)
        };
        // COMPACT FAST PATH: flex_basis stored as u32 with PixelValue encoding
        taffy_style.flex_basis = if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_flex_basis_raw(id.index());
                match raw {
                    azul_css::compact_cache::U32_AUTO
                    | azul_css::compact_cache::U32_NONE
                    | azul_css::compact_cache::U32_INITIAL => taffy::Dimension::auto(),
                    azul_css::compact_cache::U32_SENTINEL
                    | azul_css::compact_cache::U32_INHERIT => {
                        // Sentinel/inherit: fall through to slow path
                        flex_basis_slow_path(cache, node_data, &id, node_state, &mut taffy_style)
                    }
                    _ => {
                        // Try to decode the PixelValue from compact u32
                        if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                            let basis = pixel_value_to_pixels_fallback(&pv)
                                .map(taffy::Dimension::length)
                                .or_else(|| pv.to_percent().map(|p| taffy::Dimension::percent(p.get())))
                                .unwrap_or_else(taffy::Dimension::auto);
                            if !matches!(basis, _auto if _auto == taffy::Dimension::auto()) {
                                taffy_style.size.width = taffy::Dimension::auto();
                            }
                            basis
                        } else {
                            taffy::Dimension::auto()
                        }
                    }
                }
            } else {
                flex_basis_slow_path(cache, node_data, &id, node_state, &mut taffy_style)
            }
        } else {
            flex_basis_slow_path(cache, node_data, &id, node_state, &mut taffy_style)
        };
        taffy_style.align_self = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::AlignSelf)
            .and_then(|p| {
                if let CssProperty::AlignSelf(v) = p {
                    layout_align_self_to_taffy(*v)
                } else {
                    None
                }
            });
        taffy_style.justify_self = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::JustifySelf)
            .and_then(|p| {
                if let CssProperty::JustifySelf(v) = p {
                    use azul_css::props::layout::grid::LayoutJustifySelf;
                    match v.get_property_or_default().unwrap_or_default() {
                        LayoutJustifySelf::Auto => None,
                        LayoutJustifySelf::Start => Some(taffy::AlignSelf::Start),
                        LayoutJustifySelf::End => Some(taffy::AlignSelf::End),
                        LayoutJustifySelf::Center => Some(taffy::AlignSelf::Center),
                        LayoutJustifySelf::Stretch => Some(taffy::AlignSelf::Stretch),
                    }
                } else {
                    None
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
        let mut style = self.translate_style_to_taffy(dom_id);
        
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
        let is_root = self.tree.get(node_idx).map(|n| n.parent.is_none()).unwrap_or(false);
        if is_root {
            style.margin = taffy::Rect::zero();
        }
        
        // FIX: Apply cross-axis intrinsic size suppression for stretch alignment.
        // This enables align-self: stretch to work correctly by ensuring Taffy
        // sees the cross-axis size as Auto (allowing stretch) rather than a definite value.
        let (suppress_width, suppress_height) = self.should_suppress_cross_intrinsic(node_idx, &style);

        if suppress_width {
            // Force width to Auto and set min-width to 0 to allow stretching.
            // Taffy treats Auto size + Stretch alignment as a signal to fill the container.
            style.size.width = taffy::Dimension::auto(); 
            style.min_size.width = taffy::Dimension::length(0.0);
        }

        if suppress_height {
            style.size.height = taffy::Dimension::auto();
            style.min_size.height = taffy::Dimension::length(0.0);
        }

        style
    }

    /// Determines if cross-axis intrinsic size should be suppressed for stretching.
    ///
    /// Per CSS Flexbox spec, align-items: stretch makes items fill the cross-axis
    /// ONLY if the item's cross-size is 'auto' AND the item has no intrinsic cross-size.
    ///
    /// Returns (suppress_width, suppress_height) booleans.
    fn should_suppress_cross_intrinsic(&self, node_idx: usize, style: &Style) -> (bool, bool) {
        let Some(node) = self.tree.get(node_idx) else {
            return (false, false);
        };

        // Check if parent is a flex or grid container
        let Some(ref parent_fc) = node.parent_formatting_context else {
            return (false, false);
        };

        match parent_fc {
            FormattingContext::Flex => {
                // Get parent node to check its flex-direction and align-items
                let Some(parent_idx) = node.parent else {
                    return (false, false);
                };
                let parent_style = self.get_taffy_style(parent_idx);

                // Determine if flex container is row or column
                let is_row = matches!(
                    parent_style.flex_direction,
                    taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
                );

                // Get effective align value for this item
                // align-self overrides parent's align-items
                let align = style
                    .align_self
                    .or(parent_style.align_items)
                    .unwrap_or(taffy::AlignSelf::Stretch);

                let should_stretch = matches!(align, taffy::AlignSelf::Stretch);

                if !should_stretch {
                    return (false, false);
                }

                // Check if cross-axis size is auto
                // For row flex: cross-axis is height
                // For column flex: cross-axis is width
                let cross_size_is_auto = if is_row {
                    style.size.height == taffy::Dimension::auto()
                } else {
                    style.size.width == taffy::Dimension::auto()
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
        let Some(node) = self.tree.get(node_idx) else {
            return Vec::new();
        };

        node.children
            .iter()
            .filter(|&&child_idx| {
                let Some(child_node) = self.tree.get(child_idx) else {
                    return false;
                };
                let Some(child_dom_id) = child_node.dom_node_id else {
                    return true;
                };

                // Check if child has display: none
                use crate::solver3::getters::{get_display_property, MultiValue};
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
/// This function now accepts a text_cache parameter so that IFC layout can be
/// performed inline during Taffy's measure callbacks, rather than as a post-processing step.
pub fn layout_taffy_subtree<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_idx: usize,
    inputs: LayoutInput,
) -> LayoutOutput {
    let children: Vec<usize> = tree.get(node_idx).unwrap().children.clone();

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
        if let Some(child) = tree.get_mut(child_idx) {
            child.taffy_cache.clear();
        }
    }

    // SAFETY: We pass text_cache as a raw pointer because TaffyBridge needs to call
    // layout_ifc from within compute_child_layout, but we already have &mut ctx and &mut tree.
    // The pointer is only valid for the duration of this function call.
    let text_cache_ptr = text_cache as *mut crate::font_traits::TextLayoutCache;

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
                    child_idx, child.used_size, child.relative_position
                ));
            }
        }
    }

    output
}

// --- Trait Implementations for the Bridge ---

impl<'a, 'b, T: ParsedFontTrait> TraversePartialTree for TaffyBridge<'a, 'b, T> {
    type ChildIter<'c>
        = std::vec::IntoIter<taffy::NodeId>
    where
        Self: 'c;

    fn child_ids(&self, node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        let node_idx: usize = node_id.into();
        let children = self.get_layout_children(node_idx);
        children
            .into_iter()
            .map(|id| id.into())
            .collect::<Vec<taffy::NodeId>>()
            .into_iter()
    }

    fn child_count(&self, node_id: taffy::NodeId) -> usize {
        let node_idx: usize = node_id.into();
        let count = self.get_layout_children(node_idx).len();
        count
    }

    fn get_child_id(&self, node_id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.get_layout_children(node_id.into())[index].into()
    }
}

impl<'a, 'b, T: ParsedFontTrait> LayoutPartialTree for TaffyBridge<'a, 'b, T> {
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
                    if let Some(parent) = self.tree.get(parent_idx) {
                        (
                            parent.box_props.border.left,
                            parent.box_props.border.top,
                            parent.box_props.padding.left,
                            parent.box_props.padding.top,
                        )
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
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
            node.relative_position = Some(pos);
        }
    }

    fn resolve_calc_value(&self, val: *const (), basis: f32) -> f32 {
        // SAFETY: `val` came from `store_calc_and_make_dimension` which stored
        // a `Box<CalcResolveContext>` in `self.calc_storage`. The Box is alive for
        // the lifetime of this TaffyBridge, and taffy only clears the low 3 bits.
        let ctx = unsafe { &*(val as *const CalcResolveContext) };
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
            .map(|s| s.formatting_context.clone())
            .unwrap_or_default();

        let result = compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let node_idx: usize = node_id.into();
            let fc = tree
                .tree
                .get(node_idx)
                .map(|s| s.formatting_context.clone())
                .unwrap_or_default();

            match fc {
                FormattingContext::Flex => compute_flexbox_layout(tree, node_id, inputs),
                FormattingContext::Grid => compute_grid_layout(tree, node_id, inputs),
                // For Block, Inline, Table, InlineBlock - delegate to layout_formatting_context
                // This ensures proper recursive layout of all formatting contexts
                _ => tree.compute_non_flex_layout(node_idx, inputs),
            }
        });

        // DEBUG: Log the computed result
        if self.ctx.debug_messages.is_some() {
            self.ctx.debug_info_inner(format!(
                "[TAFFY compute_child_layout RESULT] node_idx={} result_size=({:?}, {:?})",
                node_idx, result.size.width, result.size.height
            ));
        }

        // Store layout for container nodes - Taffy only calls set_unrounded_layout for leaf nodes
        if let Some(node) = self.tree.get_mut(node_idx) {
            let size = translate_taffy_size_back(result.size);
            node.used_size = Some(size);
        }

        result
    }
}

impl<'a, 'b, T: ParsedFontTrait> TaffyBridge<'a, 'b, T> {
    /// Compute layout for non-flex/grid nodes by delegating to layout_formatting_context.
    /// This handles Block, Inline, Table, InlineBlock formatting contexts recursively.
    fn compute_non_flex_layout(&mut self, node_idx: usize, inputs: LayoutInput) -> LayoutOutput {
        // Determine available size from Taffy's inputs
        // For MinContent/MaxContent, we need to handle differently - use 0 for MinContent
        // to get the minimum width, and infinity for MaxContent
        // FIX: For MinContent, we should use INFINITY and let the text layout
        // calculate its actual min-content width (widest word). Using 0.0 was wrong
        // because it forced text to wrap after every character.
        let available_width = inputs
            .known_dimensions
            .width
            .or_else(|| match inputs.available_space.width {
                AvailableSpace::Definite(w) => Some(w),
                AvailableSpace::MinContent => None, // Use infinity, return intrinsic min-content
                AvailableSpace::MaxContent => None, // Use infinity for max-content
            })
            .unwrap_or(f32::INFINITY);

        let available_height = inputs
            .known_dimensions
            .height
            .or_else(|| match inputs.available_space.height {
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

        // Convert Taffy's AvailableSpace to our Text3AvailableSpace for caching
        let available_width_type = match inputs.available_space.width {
            AvailableSpace::Definite(w) => crate::text3::cache::AvailableSpace::Definite(w),
            AvailableSpace::MinContent => crate::text3::cache::AvailableSpace::MinContent,
            AvailableSpace::MaxContent => crate::text3::cache::AvailableSpace::MaxContent,
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

        // SAFETY: text_cache pointer is valid for the lifetime of TaffyBridge
        let text_cache = unsafe { &mut *self.text_cache };

        let constraints = LayoutConstraints {
            available_size,
            writing_mode: LayoutWritingMode::HorizontalTb,
            bfc_state: None,
            text_align: fc_text_align,
            containing_block_size: available_size,
            available_width_type,
        };

        // Use a temporary float cache for this subtree
        let mut float_cache = std::collections::BTreeMap::new();

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

                // Get padding and border from the node's box_props
                let (padding_width, padding_height, border_width, border_height) = self
                    .tree
                    .get(node_idx)
                    .map(|node| {
                        let bp = &node.box_props;
                        let pw = bp.padding.left + bp.padding.right;
                        let ph = bp.padding.top + bp.padding.bottom;
                        let bw = bp.border.left + bp.border.right;
                        let bh = bp.border.top + bp.border.bottom;
                        (pw, ph, bw, bh)
                    })
                    .unwrap_or((0.0, 0.0, 0.0, 0.0));

                // Get intrinsic sizes for min/max-content queries
                let intrinsic = self
                    .tree
                    .get(node_idx)
                    .and_then(|n| n.intrinsic_sizes)
                    .unwrap_or_default();

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
                    .map(|s| s.formatting_context.clone())
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

                // Convert content-box size to border-box size (for when we compute our own size)
                let border_box_width = effective_content_width + padding_width + border_width;
                let border_box_height = content_height + padding_height + border_height;

                // CRITICAL: Taffy passes content-box as known_dimensions (it subtracts
                // padding/border when resolving percentage widths). But Taffy uses
                // our returned size for positioning the next element. So we MUST
                // return border-box size to avoid gaps/overlaps.
                // When known_dimensions is set, we add padding/border to convert to border-box.
                // When it's None, we use our computed border_box size.
                let final_width = match inputs.known_dimensions.width {
                    Some(content_w) => content_w + padding_width + border_width,
                    None => border_box_width,
                };

                // For grid items: if known_dimensions.height is None but available_space.height
                // is definite, use the available space. This ensures empty grid items stretch
                // to fill their grid cell, per CSS Grid spec behavior.
                let final_height = match inputs.known_dimensions.height {
                    Some(content_h) => content_h + padding_height + border_height,
                    None => {
                        // Check if parent is a grid container and available_space is definite
                        let parent_is_grid = self
                            .tree
                            .get(node_idx)
                            .and_then(|n| n.parent)
                            .and_then(|p| self.tree.get(p))
                            .map(|p| matches!(p.formatting_context, FormattingContext::Grid))
                            .unwrap_or(false);

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
                    }
                };

                // CRITICAL: Transfer positions from layout_formatting_context to child nodes.
                // Without this, children of flex items won't have their relative_position set,
                // causing them to all render at (0,0) relative to their parent.
                for (child_idx, child_pos) in output.positions.iter() {
                    if let Some(child_node) = self.tree.get_mut(*child_idx) {
                        child_node.relative_position = Some(*child_pos);
                    }
                }

                // Compute scrollbar_info for this node (it's a child of a Flex/Grid container,
                // so calculate_layout_for_subtree won't be called for it)
                let scrollbar_info = {
                    let node = self.tree.get(node_idx);
                    node.and_then(|n| n.dom_node_id)
                        .map(|dom_id| {
                            let styled_node_state = self
                                .ctx
                                .styled_dom
                                .styled_nodes
                                .as_container()
                                .get(dom_id)
                                .map(|s| s.styled_node_state.clone())
                                .unwrap_or_default();
                            let overflow_x =
                                get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
                            let overflow_y =
                                get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);

                            // For scrollbar detection, we need to compare content size against
                            // the CSS-specified container size, not the final laid-out size.
                            // For nodes with explicit height + overflow:auto, the CSS height is
                            // the constraint, while content may overflow that.
                            let css_height =
                                get_css_height(self.ctx.styled_dom, dom_id, &styled_node_state);
                            let css_width =
                                get_css_width(self.ctx.styled_dom, dom_id, &styled_node_state);

                            // Helper to extract pixel value from LayoutHeight/LayoutWidth
                            let height_to_px =
                                |h: azul_css::props::layout::LayoutHeight| -> Option<f32> {
                                    match h {
                                        azul_css::props::layout::LayoutHeight::Px(px) => {
                                            pixel_value_to_pixels_fallback(&px)
                                        }
                                        _ => None,
                                    }
                                };
                            let width_to_px =
                                |w: azul_css::props::layout::LayoutWidth| -> Option<f32> {
                                    match w {
                                        azul_css::props::layout::LayoutWidth::Px(px) => {
                                            pixel_value_to_pixels_fallback(&px)
                                        }
                                        _ => None,
                                    }
                                };

                            // Use CSS-specified size if available, otherwise fall back to final size
                            let css_container_height = css_height
                                .exact()
                                .and_then(|h| height_to_px(h))
                                .unwrap_or(final_height - padding_height - border_height);
                            let css_container_width = css_width
                                .exact()
                                .and_then(|w| width_to_px(w))
                                .unwrap_or(final_width - padding_width - border_width);

                            let content_size = LogicalSize::new(content_width, content_height);
                            let container_size =
                                LogicalSize::new(css_container_width, css_container_height);

                            // Use per-node CSS scrollbar-width + OS overlay preference
                            let scrollbar_width_px =
                                crate::solver3::getters::get_layout_scrollbar_width_px(
                                    self.ctx, dom_id, &styled_node_state,
                                );

                            let scrollbar_result = crate::solver3::fc::check_scrollbar_necessity(
                                content_size,
                                container_size,
                                crate::solver3::cache::to_overflow_behavior(overflow_x),
                                crate::solver3::cache::to_overflow_behavior(overflow_y),
                                scrollbar_width_px,
                            );

                            scrollbar_result
                        })
                        .unwrap_or_default()
                };

                // Store the border-box size and scrollbar_info on the node for display list generation
                if let Some(node) = self.tree.get_mut(node_idx) {
                    node.used_size = Some(LogicalSize {
                        width: final_width,
                        height: final_height,
                    });
                    node.scrollbar_info = Some(scrollbar_info);
                    // Store the actual content size for scroll calculations
                    node.overflow_content_size = Some(LogicalSize {
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
                let node = self.tree.get(node_idx);
                let intrinsic = node.and_then(|n| n.intrinsic_sizes).unwrap_or_default();

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

impl<'a, 'b, T: ParsedFontTrait> CacheTree for TaffyBridge<'a, 'b, T> {
    fn cache_get(
        &self,
        node_id: taffy::NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        let node_idx: usize = node_id.into();
        self.tree
            .get(node_idx)?
            .taffy_cache
            .get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: taffy::NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: LayoutOutput,
    ) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.taffy_cache
                .store(known_dimensions, available_space, run_mode, layout_output);
        }
    }

    fn cache_clear(&mut self, node_id: taffy::NodeId) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }
}

impl<'a, 'b, T: ParsedFontTrait> LayoutFlexboxContainer for TaffyBridge<'a, 'b, T> {
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

impl<'a, 'b, T: ParsedFontTrait> LayoutGridContainer for TaffyBridge<'a, 'b, T> {
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

fn from_layout_width(
    val: LayoutWidth,
    calc_storage: &std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    em_size: f32,
    rem_size: f32,
) -> Dimension {
    match val {
        LayoutWidth::Auto => Dimension::auto(),
        LayoutWidth::Px(px) => {
            match pixel_value_to_pixels_fallback(&px) {
                Some(pixels) => Dimension::length(pixels),
                None => match px.to_percent() {
                    Some(p) => Dimension::percent(p.get()),
                    None => Dimension::auto(),
                },
            }
        }
        LayoutWidth::MinContent | LayoutWidth::MaxContent => Dimension::auto(),
        LayoutWidth::Calc(items) => store_calc_and_make_dimension(items, calc_storage, em_size, rem_size),
    }
}

fn from_layout_height(
    val: LayoutHeight,
    calc_storage: &std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    em_size: f32,
    rem_size: f32,
) -> Dimension {
    match val {
        LayoutHeight::Auto => Dimension::auto(),
        LayoutHeight::Px(px) => {
            match pixel_value_to_pixels_fallback(&px) {
                Some(pixels) => Dimension::length(pixels),
                None => match px.to_percent() {
                    Some(p) => Dimension::percent(p.get()),
                    None => Dimension::auto(),
                },
            }
        }
        LayoutHeight::MinContent | LayoutHeight::MaxContent => Dimension::auto(),
        LayoutHeight::Calc(items) => store_calc_and_make_dimension(items, calc_storage, em_size, rem_size),
    }
}

/// Stores the calc AST + font-size context in heap-pinned storage and returns
/// a `Dimension::calc(ptr)` with a stable pointer to the `CalcResolveContext`.
///
/// The `Box` ensures the address doesn't move when the outer `Vec` reallocates.
/// The `RefCell<Vec<…>>` keeps all boxes alive for the layout pass duration.
fn store_calc_and_make_dimension(
    items: CalcAstItemVec,
    storage: &std::cell::RefCell<Vec<Box<CalcResolveContext>>>,
    em_size: f32,
    rem_size: f32,
) -> Dimension {
    let boxed = Box::new(CalcResolveContext { items, em_size, rem_size });
    let ptr: *const CalcResolveContext = &*boxed;
    storage.borrow_mut().push(boxed);
    // SAFETY: Box gives ≥8-byte-aligned heap pointer; taffy masks low 3 bits.
    Dimension::calc(ptr as *const ())
}

fn from_pixel_value_lp(val: PixelValue) -> LengthPercentage {
    match pixel_value_to_pixels_fallback(&val) {
        Some(px) => LengthPercentage::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentage::percent(p.get()), // p is already normalized (0.0-1.0)
            None => LengthPercentage::length(0.0),         /* Fallback to 0 if neither px nor
                                                             * percent */
        },
    }
}

fn from_pixel_value_lpa(val: PixelValue) -> LengthPercentageAuto {
    match pixel_value_to_pixels_fallback(&val) {
        Some(px) => LengthPercentageAuto::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentageAuto::percent(p.get()), // p is already normalized (0.0-1.0)
            None => LengthPercentageAuto::auto(),
        },
    }
}

fn from_taffy_size(val: Size<f32>) -> azul_core::geom::LogicalSize {
    azul_core::geom::LogicalSize {
        width: val.width,
        height: val.height,
    }
}

#[allow(dead_code)]
fn from_logical_size(val: azul_core::geom::LogicalSize) -> Size<AvailableSpace> {
    Size {
        width: AvailableSpace::Definite(val.width),
        height: AvailableSpace::Definite(val.height),
    }
}

fn from_layout_position(val: LayoutPosition) -> Position {
    match val {
        LayoutPosition::Static => Position::Relative, // Taffy treats Static as Relative
        LayoutPosition::Relative => Position::Relative,
        LayoutPosition::Absolute => Position::Absolute,
        LayoutPosition::Fixed => Position::Absolute, // Taffy doesn't distinguish Fixed
        LayoutPosition::Sticky => Position::Relative, // Sticky = Relative for Taffy
    }
}

fn from_taffy_point(val: taffy::Point<f32>) -> azul_core::geom::LogicalPosition {
    azul_core::geom::LogicalPosition { x: val.x, y: val.y }
}

fn from_flex_wrap(val: LayoutFlexWrap) -> FlexWrap {
    match val {
        LayoutFlexWrap::NoWrap => FlexWrap::NoWrap,
        LayoutFlexWrap::Wrap => FlexWrap::Wrap,
        LayoutFlexWrap::WrapReverse => FlexWrap::WrapReverse,
    }
}

fn from_flex_direction(val: LayoutFlexDirection) -> FlexDirection {
    match val {
        LayoutFlexDirection::Row => FlexDirection::Row,
        LayoutFlexDirection::RowReverse => FlexDirection::RowReverse,
        LayoutFlexDirection::Column => FlexDirection::Column,
        LayoutFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
    }
}

fn from_align_items(val: LayoutAlignItems) -> AlignItems {
    match val {
        LayoutAlignItems::Start => AlignItems::FlexStart,
        LayoutAlignItems::End => AlignItems::FlexEnd,
        LayoutAlignItems::Center => AlignItems::Center,
        LayoutAlignItems::Baseline => AlignItems::Baseline,
        LayoutAlignItems::Stretch => AlignItems::Stretch,
    }
}

fn from_align_self(val: LayoutAlignSelf) -> AlignSelf {
    match val {
        LayoutAlignSelf::Auto => AlignSelf::FlexStart, // Taffy doesn't have Auto for AlignSelf
        LayoutAlignSelf::Start => AlignSelf::FlexStart,
        LayoutAlignSelf::End => AlignSelf::FlexEnd,
        LayoutAlignSelf::Center => AlignSelf::Center,
        LayoutAlignSelf::Baseline => AlignSelf::Baseline,
        LayoutAlignSelf::Stretch => AlignSelf::Stretch,
    }
}

fn from_justify_content(val: LayoutJustifyContent) -> JustifyContent {
    match val {
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
