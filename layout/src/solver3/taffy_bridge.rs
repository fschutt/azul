use azul_css::{
    css::CssPropertyValue,
    props::{
        layout::grid::{GridAutoTracks, GridTemplate, GridTrackSizing},
        property::{
            LayoutAlignItemsValue, LayoutDisplayValue, LayoutFlexDirectionValue,
            LayoutFlexWrapValue, LayoutGridAutoColumnsValue, LayoutGridAutoFlowValue,
            LayoutGridAutoRowsValue, LayoutGridTemplateColumnsValue, LayoutGridTemplateRowsValue,
            LayoutJustifyContentValue, LayoutPositionValue,
        },
    },
};
use taffy::style::{MaxTrackSizingFunction, MinTrackSizingFunction, TrackSizingFunction};

pub fn grid_template_rows_to_taffy(
    val: LayoutGridTemplateRowsValue,
) -> Vec<taffy::TrackSizingFunction> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks.tracks.iter().map(translate_track).collect()
}

pub fn grid_template_columns_to_taffy(
    val: LayoutGridTemplateColumnsValue,
) -> Vec<taffy::TrackSizingFunction> {
    let auto_tracks = val.get_property_or_default().unwrap_or_default();
    auto_tracks.tracks.iter().map(translate_track).collect()
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
    match track {
        GridTrackSizing::MinContent => minmax(
            taffy::MinTrackSizingFunction::min_content(),
            taffy::MaxTrackSizingFunction::min_content(),
        ),
        GridTrackSizing::MaxContent => minmax(
            taffy::MinTrackSizingFunction::max_content(),
            taffy::MaxTrackSizingFunction::max_content(),
        ),
        GridTrackSizing::MinMax(min, max) => {
            minmax(translate_track(min).min, translate_track(max).max)
        }
        GridTrackSizing::Fixed(px) => minmax(
            taffy::MinTrackSizingFunction::length(px.to_pixels(0.0)),
            taffy::MaxTrackSizingFunction::length(px.to_pixels(0.0)),
        ),
        GridTrackSizing::Fr(_fr) => {
            // TODO: taffy 0.9.1 doesn't seem to have a direct fr() method for
            // Min/MaxTrackSizingFunction For now, using auto() as a workaround. This
            // needs proper investigation.
            minmax(
                taffy::MinTrackSizingFunction::auto(),
                taffy::MaxTrackSizingFunction::auto(),
            )
        }
        GridTrackSizing::Auto => minmax(
            taffy::MinTrackSizingFunction::min_content(),
            taffy::MaxTrackSizingFunction::max_content(),
        ),
        GridTrackSizing::FitContent(px) => minmax(
            taffy::MinTrackSizingFunction::length(px.to_pixels(0.0)),
            taffy::MaxTrackSizingFunction::max_content(),
        ),
    }
}

fn minmax(min: MinTrackSizingFunction, max: MaxTrackSizingFunction) -> taffy::TrackSizingFunction {
    TrackSizingFunction { min, max }
}

pub fn layout_display_to_taffy(val: LayoutDisplayValue) -> taffy::Display {
    use azul_css::props::layout::LayoutDisplay;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutDisplay::None => taffy::Display::None,
        LayoutDisplay::Flex | LayoutDisplay::InlineFlex => taffy::Display::Flex,
        LayoutDisplay::Grid | LayoutDisplay::InlineGrid => taffy::Display::Grid,
        _ => taffy::Display::Block,
    }
}

pub fn layout_position_to_taffy(val: LayoutPositionValue) -> taffy::Position {
    use azul_css::props::layout::LayoutPosition;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutPosition::Absolute => taffy::Position::Absolute,
        LayoutPosition::Fixed => taffy::Position::Absolute, // Taffy kennt kein Fixed
        LayoutPosition::Relative => taffy::Position::Relative,
        LayoutPosition::Static => taffy::Position::Relative,
        LayoutPosition::Sticky => taffy::Position::Relative, // Sticky wird als Relative behandelt
    }
}

pub fn grid_auto_flow_to_taffy(val: LayoutGridAutoFlowValue) -> taffy::GridAutoFlow {
    use azul_css::props::layout::LayoutGridAutoFlow;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutGridAutoFlow::Row => taffy::GridAutoFlow::Row,
        LayoutGridAutoFlow::Column => taffy::GridAutoFlow::Column,
        LayoutGridAutoFlow::RowDense => taffy::GridAutoFlow::RowDense,
        LayoutGridAutoFlow::ColumnDense => taffy::GridAutoFlow::ColumnDense,
    }
}

pub fn layout_flex_direction_to_taffy(val: LayoutFlexDirectionValue) -> taffy::FlexDirection {
    use azul_css::props::layout::LayoutFlexDirection;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutFlexDirection::Row => taffy::FlexDirection::Row,
        LayoutFlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
        LayoutFlexDirection::Column => taffy::FlexDirection::Column,
        LayoutFlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
    }
}

pub fn layout_flex_wrap_to_taffy(val: LayoutFlexWrapValue) -> taffy::FlexWrap {
    use azul_css::props::layout::LayoutFlexWrap;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutFlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
        LayoutFlexWrap::Wrap => taffy::FlexWrap::Wrap,
        LayoutFlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
    }
}

pub fn layout_align_items_to_taffy(val: LayoutAlignItemsValue) -> taffy::AlignItems {
    use azul_css::props::layout::LayoutAlignItems;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignItems::Stretch => taffy::AlignItems::Stretch,
        LayoutAlignItems::Center => taffy::AlignItems::Center,
        LayoutAlignItems::Start => taffy::AlignItems::FlexStart,
        LayoutAlignItems::End => taffy::AlignItems::FlexEnd,
        LayoutAlignItems::Baseline => taffy::AlignItems::Baseline,
    }
}

pub fn layout_align_self_to_taffy(val: LayoutAlignItemsValue) -> taffy::AlignSelf {
    use azul_css::props::layout::LayoutAlignItems;
    match val.get_property_or_default().unwrap_or_default() {
        LayoutAlignItems::Start => taffy::AlignSelf::FlexStart,
        LayoutAlignItems::End => taffy::AlignSelf::FlexEnd,
        LayoutAlignItems::Center => taffy::AlignSelf::Center,
        LayoutAlignItems::Baseline => taffy::AlignSelf::Baseline,
        LayoutAlignItems::Stretch => taffy::AlignSelf::Stretch,
    }
}

pub fn layout_justify_content_to_taffy(val: LayoutJustifyContentValue) -> taffy::JustifyContent {
    use azul_css::props::layout::LayoutJustifyContent;
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

pub fn pixel_value_to_length(
    val: azul_css::props::basic::pixel::PixelValue,
) -> taffy::LengthPercentage {
    taffy::LengthPercentage::from_length(val.to_pixels(0.0))
}

pub fn pixel_value_to_length_percentage_auto(
    val: azul_css::props::basic::pixel::PixelValue,
) -> taffy::LengthPercentageAuto {
    taffy::LengthPercentageAuto::length(val.to_pixels(0.0))
}

// TODO: gap, grid, visibility, z_index, flex_basis, etc. analog ergänzen
// --- CSS <-> Taffy Übersetzungsfunktionen ---

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{dom::NodeId, styled_dom::StyledDom};
use azul_css::props::{
    basic::pixel::PixelValue,
    layout::{
        LayoutAlignItems, LayoutAlignSelf, LayoutDisplay, LayoutFlexDirection, LayoutFlexWrap,
        LayoutGridAutoFlow, LayoutJustifyContent, LayoutPosition,
    },
    property::{CssProperty, CssPropertyType},
};
use taffy::{
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    prelude::*, CacheTree, LayoutFlexboxContainer, LayoutGridContainer, LayoutInput, LayoutOutput,
    RunMode,
};

use crate::{
    solver3::{
        fc::{translate_taffy_point_back, translate_taffy_size_back},
        geometry::CssSize,
        layout_tree::{LayoutNode, LayoutTree},
        sizing, LayoutContext,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};

/// The bridge struct that implements Taffy's traits.
/// It holds mutable references to the solver's data structures, allowing Taffy
/// to read styles and write layout results back into our `LayoutTree`.
struct TaffyBridge<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a mut LayoutContext<'b, T, Q>,
    tree: &'a mut LayoutTree<T>,
}

/*

pub struct Style<S: CheapCloneStr = DefaultCheapStr> {
    /// This is a dummy field which is necessary to make Taffy compile with the `grid` feature disabled
    /// It should always be set to `core::marker::PhantomData`.
    pub dummy: core::marker::PhantomData<S>,
    /// What layout strategy should be used?
    pub display: Display,
    /// Whether a child is display:table or not. This affects children of block layouts.
    /// This should really be part of `Display`, but it is currently seperate because table layout isn't implemented
    pub item_is_table: bool,
    /// Is it a replaced element like an image or form field?
    /// <https://drafts.csswg.org/css-sizing-3/#min-content-zero>
    pub item_is_replaced: bool,
    /// Should size styles apply to the content box or the border box of the node
    pub box_sizing: BoxSizing,

    // Overflow properties
    /// How children overflowing their container should affect layout
    pub overflow: Point<Overflow>,
    /// How much space (in points) should be reserved for the scrollbars of `Overflow::Scroll` and `Overflow::Auto` nodes.
    pub scrollbar_width: f32,

    // Position properties
    /// What should the `position` value of this struct use as a base offset?
    pub position: Position,
    /// How should the position of this element be tweaked relative to the layout defined?
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
    pub inset: Rect<LengthPercentageAuto>,

    // Size properties
    /// Sets the initial size of the item
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
    pub size: Size<Dimension>,
    /// Controls the minimum size of the item
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
    pub min_size: Size<Dimension>,
    /// Controls the maximum size of the item
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
    pub max_size: Size<Dimension>,
    /// Sets the preferred aspect ratio for the item
    ///
    /// The ratio is calculated as width divided by height.
    pub aspect_ratio: Option<f32>,

    // Spacing Properties
    /// How large should the margin be on each side?
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
    pub margin: Rect<LengthPercentageAuto>,
    /// How large should the padding be on each side?
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
    pub padding: Rect<LengthPercentage>,
    /// How large should the border be on each side?
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
    pub border: Rect<LengthPercentage>,

    // Alignment properties
    /// How this node's children aligned in the cross/block axis?
    #[cfg(any(feature = "flexbox", feature = "grid"))]
    pub align_items: Option<AlignItems>,
    /// How this node should be aligned in the cross/block axis
    /// Falls back to the parents [`AlignItems`] if not set
    #[cfg(any(feature = "flexbox", feature = "grid"))]
    pub align_self: Option<AlignSelf>,
    /// How this node's children should be aligned in the inline axis
    #[cfg(feature = "grid")]
    pub justify_items: Option<AlignItems>,
    /// How this node should be aligned in the inline axis
    /// Falls back to the parents [`JustifyItems`] if not set
    #[cfg(feature = "grid")]
    pub justify_self: Option<AlignSelf>,
    /// How should content contained within this item be aligned in the cross/block axis
    #[cfg(any(feature = "flexbox", feature = "grid"))]
    pub align_content: Option<AlignContent>,
    /// How should content contained within this item be aligned in the main/inline axis
    #[cfg(any(feature = "flexbox", feature = "grid"))]
    pub justify_content: Option<JustifyContent>,
    /// How large should the gaps between items in a grid or flex container be?
    #[cfg(any(feature = "flexbox", feature = "grid"))]
    #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
    pub gap: Size<LengthPercentage>,

    // Block container properties
    /// How items elements should aligned in the inline axis
    #[cfg(feature = "block_layout")]
    pub text_align: TextAlign,

    // Flexbox container properties
    /// Which direction does the main axis flow in?
    #[cfg(feature = "flexbox")]
    pub flex_direction: FlexDirection,
    /// Should elements wrap, or stay in a single line?
    #[cfg(feature = "flexbox")]
    pub flex_wrap: FlexWrap,

    // Flexbox item properties
    /// Sets the initial main axis size of the item
    #[cfg(feature = "flexbox")]
    pub flex_basis: Dimension,
    /// The relative rate at which this item grows when it is expanding to fill space
    ///
    /// 0.0 is the default value, and this value must be positive.
    #[cfg(feature = "flexbox")]
    pub flex_grow: f32,
    /// The relative rate at which this item shrinks when it is contracting to fit into space
    ///
    /// 1.0 is the default value, and this value must be positive.
    #[cfg(feature = "flexbox")]
    pub flex_shrink: f32,

    // Grid container properies
    /// Defines the track sizing functions (heights) of the grid rows
    #[cfg(feature = "grid")]
    pub grid_template_rows: GridTrackVec<GridTemplateComponent<S>>,
    /// Defines the track sizing functions (widths) of the grid columns
    #[cfg(feature = "grid")]
    pub grid_template_columns: GridTrackVec<GridTemplateComponent<S>>,
    /// Defines the size of implicitly created rows
    #[cfg(feature = "grid")]
    pub grid_auto_rows: GridTrackVec<TrackSizingFunction>,
    /// Defined the size of implicitly created columns
    #[cfg(feature = "grid")]
    pub grid_auto_columns: GridTrackVec<TrackSizingFunction>,
    /// Controls how items get placed into the grid for auto-placed items
    #[cfg(feature = "grid")]
    pub grid_auto_flow: GridAutoFlow,

    // Grid container named properties
    /// Defines the rectangular grid areas
    #[cfg(feature = "grid")]
    pub grid_template_areas: GridTrackVec<GridTemplateArea<S>>,
    /// The named lines between the columns
    #[cfg(feature = "grid")]
    pub grid_template_column_names: GridTrackVec<GridTrackVec<S>>,
    /// The named lines between the rows
    #[cfg(feature = "grid")]
    pub grid_template_row_names: GridTrackVec<GridTrackVec<S>>,

    // Grid child properties
    /// Defines which row in the grid the item should start and end at
    #[cfg(feature = "grid")]
    pub grid_row: Line<GridPlacement<S>>,
    /// Defines which column in the grid the item should start and end at
    #[cfg(feature = "grid")]
    pub grid_column: Line<GridPlacement<S>>,
}

*/

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TaffyBridge<'a, 'b, T, Q> {
    fn new(ctx: &'a mut LayoutContext<'b, T, Q>, tree: &'a mut LayoutTree<T>) -> Self {
        Self { ctx, tree }
    }

    /// Translates CSS properties from the `StyledDom` into a `taffy::Style` struct.
    /// This is the core of the integration, mapping one style system to another.
    fn translate_style_to_taffy(&self, dom_id: Option<NodeId>) -> Style {
        let Some(id) = dom_id else {
            return Style::default();
        };
        let styled_dom = &self.ctx.styled_dom;
        let node_data = &styled_dom.node_data.as_ref()[id.index()];
        let node_state = &styled_dom.styled_nodes.as_container()[id].state;
        let cache = &styled_dom.css_property_cache.ptr;
        let mut taffy_style = Style::default();

        // Display Mode
        taffy_style.display = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::Display)
            .and_then(|p| {
                if let CssProperty::Display(d) = p {
                    Some(*d)
                } else {
                    None
                }
            })
            .map(layout_display_to_taffy)
            .unwrap_or(taffy::Display::Block);

        // Position
        taffy_style.position = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::Position)
            .and_then(|p| {
                if let CssProperty::Position(pos) = p {
                    Some(*pos)
                } else {
                    None
                }
            })
            .map(layout_position_to_taffy)
            .unwrap_or(taffy::Position::Relative);

        // Inset (top, left, bottom, right)
        taffy_style.inset = taffy::Rect {
            left: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::Left)
                .and_then(|p| {
                    if let CssProperty::Left(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(pixel_value_to_length_percentage_auto)
                .unwrap_or_else(taffy::LengthPercentageAuto::auto),
            right: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::Right)
                .and_then(|p| {
                    if let CssProperty::Right(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(pixel_value_to_length_percentage_auto)
                .unwrap_or_else(taffy::LengthPercentageAuto::auto),
            top: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::Top)
                .and_then(|p| {
                    if let CssProperty::Top(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(pixel_value_to_length_percentage_auto)
                .unwrap_or_else(taffy::LengthPercentageAuto::auto),
            bottom: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::Bottom)
                .and_then(|p| {
                    if let CssProperty::Bottom(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(pixel_value_to_length_percentage_auto)
                .unwrap_or_else(taffy::LengthPercentageAuto::auto),
        };

        // Size
        let width = sizing::get_css_width(self.ctx.styled_dom, dom_id);
        let height = sizing::get_css_height(self.ctx.styled_dom, dom_id);
        taffy_style.size = taffy::Size {
            width: from_css_size(width),
            height: from_css_size(height),
        };

        // Min/Max Size
        taffy_style.min_size = taffy::Size {
            width: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MinWidth)
                .and_then(|p| {
                    if let CssProperty::MinWidth(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(|px| Dimension::length(px.to_pixels(0.0)))
                .unwrap_or(Dimension::auto()),
            height: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MinHeight)
                .and_then(|p| {
                    if let CssProperty::MinHeight(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(|px| Dimension::length(px.to_pixels(0.0)))
                .unwrap_or(Dimension::auto()),
        };
        taffy_style.max_size = taffy::Size {
            width: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MaxWidth)
                .and_then(|p| {
                    if let CssProperty::MaxWidth(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(|px| Dimension::length(px.to_pixels(0.0)))
                .unwrap_or(Dimension::auto()),
            height: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MaxHeight)
                .and_then(|p| {
                    if let CssProperty::MaxHeight(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(|px| Dimension::length(px.to_pixels(0.0)))
                .unwrap_or(Dimension::auto()),
        };

        // Box Model (margin, padding, border)
        taffy_style.margin = taffy::Rect {
            left: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MarginLeft)
                .and_then(|p| {
                    if let CssProperty::MarginLeft(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lpa)
                .unwrap_or_else(|| LengthPercentageAuto::AUTO),
            right: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MarginRight)
                .and_then(|p| {
                    if let CssProperty::MarginRight(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lpa)
                .unwrap_or_else(|| LengthPercentageAuto::AUTO),
            top: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MarginTop)
                .and_then(|p| {
                    if let CssProperty::MarginTop(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lpa)
                .unwrap_or_else(|| LengthPercentageAuto::AUTO),
            bottom: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::MarginBottom)
                .and_then(|p| {
                    if let CssProperty::MarginBottom(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lpa)
                .unwrap_or_else(|| LengthPercentageAuto::AUTO),
        };

        taffy_style.padding = taffy::Rect {
            left: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::PaddingLeft)
                .and_then(|p| {
                    if let CssProperty::PaddingLeft(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
            right: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::PaddingRight)
                .and_then(|p| {
                    if let CssProperty::PaddingRight(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
            top: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::PaddingTop)
                .and_then(|p| {
                    if let CssProperty::PaddingTop(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
            bottom: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::PaddingBottom)
                .and_then(|p| {
                    if let CssProperty::PaddingBottom(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
        };

        taffy_style.border = taffy::Rect {
            left: cache
                .get_property(
                    node_data,
                    &id,
                    node_state,
                    &CssPropertyType::BorderLeftWidth,
                )
                .and_then(|p| {
                    if let CssProperty::BorderLeftWidth(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
            right: cache
                .get_property(
                    node_data,
                    &id,
                    node_state,
                    &CssPropertyType::BorderRightWidth,
                )
                .and_then(|p| {
                    if let CssProperty::BorderRightWidth(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
            top: cache
                .get_property(node_data, &id, node_state, &CssPropertyType::BorderTopWidth)
                .and_then(|p| {
                    if let CssProperty::BorderTopWidth(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
            bottom: cache
                .get_property(
                    node_data,
                    &id,
                    node_state,
                    &CssPropertyType::BorderBottomWidth,
                )
                .and_then(|p| {
                    if let CssProperty::BorderBottomWidth(v) = p {
                        Some(v.get_property_or_default().unwrap_or_default().inner)
                    } else {
                        None
                    }
                })
                .map(from_pixel_value_lp)
                .unwrap_or_else(|| LengthPercentage::ZERO),
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
                Size {
                    width: LengthPercentage::length(val.to_pixels(0.0)),
                    height: LengthPercentage::length(val.to_pixels(0.0)),
                }
            })
            .unwrap_or_else(Size::zero);

        // TODO: grid_template_rows expects GridTrackVec<GridTemplateComponent>, not
        // Vec<TrackSizingFunction> Need to properly convert GridTemplate to
        // GridTemplateComponent taffy_style.grid_template_rows = cache
        //     .get_property(
        //         node_data,
        //         &id,
        //         node_state,
        //         &CssPropertyType::GridTemplateRows,
        //     )
        //     .and_then(|p| {
        //         if let CssProperty::GridTemplateRows(v) = p {
        //             Some(v.clone())
        //         } else {
        //             None
        //         }
        //     })
        //     .map(|v| grid_template_rows_to_taffy(v))
        //     .unwrap_or_default();

        // TODO: grid_template_columns expects GridTrackVec<GridTemplateComponent>, not
        // Vec<TrackSizingFunction> Need to properly convert GridTemplate to
        // GridTemplateComponent taffy_style.grid_template_columns = cache
        //     .get_property(
        //         node_data,
        //         &id,
        //         node_state,
        //         &CssPropertyType::GridTemplateColumns,
        //     )
        //     .and_then(|p| {
        //         if let CssProperty::GridTemplateColumns(v) = p {
        //             Some(v.clone())
        //         } else {
        //             None
        //         }
        //     })
        //     .map(|v| grid_template_columns_to_taffy(v))
        //     .unwrap_or_default();

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

        // Flexbox
        taffy_style.flex_direction = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::FlexDirection)
            .and_then(|p| {
                if let CssProperty::FlexDirection(v) = p {
                    Some(*v)
                } else {
                    None
                }
            })
            .map(layout_flex_direction_to_taffy)
            .unwrap_or(taffy::FlexDirection::Row);
        taffy_style.flex_wrap = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::FlexWrap)
            .and_then(|p| {
                if let CssProperty::FlexWrap(v) = p {
                    Some(*v)
                } else {
                    None
                }
            })
            .map(layout_flex_wrap_to_taffy)
            .unwrap_or(taffy::FlexWrap::NoWrap);
        taffy_style.align_items = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::AlignItems)
            .and_then(|p| {
                if let CssProperty::AlignItems(v) = p {
                    Some(*v)
                } else {
                    None
                }
            })
            .map(layout_align_items_to_taffy);
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
        taffy_style.flex_grow = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::FlexGrow)
            .and_then(|p| {
                if let CssProperty::FlexGrow(v) = p {
                    Some(v.get_property_or_default().unwrap_or_default().inner.get())
                } else {
                    None
                }
            })
            .unwrap_or(0.0);
        taffy_style.flex_shrink = cache
            .get_property(node_data, &id, node_state, &CssPropertyType::FlexShrink)
            .and_then(|p| {
                if let CssProperty::FlexShrink(v) = p {
                    Some(v.get_property_or_default().unwrap_or_default().inner.get())
                } else {
                    None
                }
            })
            .unwrap_or(1.0);
        // TODO: flex_basis, align_self, gap, grid

        taffy_style
    }

    /// Gets or computes the Taffy style for a given node index.
    fn get_taffy_style(&self, node_idx: usize) -> Style {
        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        let style = self.translate_style_to_taffy(dom_id);
        style
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
                let node_data = &self.ctx.styled_dom.node_data.as_container()[child_dom_id];
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[child_dom_id].state;

                if let Some(display_prop) = self.ctx.styled_dom.css_property_cache.ptr.get_property(
                    node_data,
                    &child_dom_id,
                    node_state,
                    &CssPropertyType::Display,
                ) {
                    if let CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::None)) =
                        display_prop
                    {
                        return false;
                    }
                }
                true
            })
            .copied()
            .collect()
    }
}

/// Main entry point for laying out a Flexbox or Grid container using Taffy.
pub fn layout_taffy_subtree<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    ctx: &mut LayoutContext<T, Q>,
    tree: &mut LayoutTree<T>,
    node_idx: usize,
    inputs: LayoutInput,
) -> LayoutOutput {
    let mut bridge = TaffyBridge::new(ctx, tree);
    let node = bridge.tree.get(node_idx).unwrap();
    let output = match node.formatting_context {
        azul_core::ui_solver::FormattingContext::Flex => {
            compute_flexbox_layout(&mut bridge, node_idx.into(), inputs)
        }
        azul_core::ui_solver::FormattingContext::Grid => {
            compute_grid_layout(&mut bridge, node_idx.into(), inputs)
        }
        _ => LayoutOutput::HIDDEN,
    };
    output
}

// --- Trait Implementations for the Bridge ---

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> TraversePartialTree
    for TaffyBridge<'a, 'b, T, Q>
{
    type ChildIter<'c>
        = std::vec::IntoIter<taffy::NodeId>
    where
        Self: 'c;

    fn child_ids(&self, node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        self.get_layout_children(node_id.into())
            .into_iter()
            .map(|id| id.into())
            .collect::<Vec<taffy::NodeId>>()
            .into_iter()
    }

    fn child_count(&self, node_id: taffy::NodeId) -> usize {
        self.get_layout_children(node_id.into()).len()
    }

    fn get_child_id(&self, node_id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.get_layout_children(node_id.into())[index].into()
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutPartialTree
    for TaffyBridge<'a, 'b, T, Q>
{
    type CoreContainerStyle<'c>
        = Style
    where
        Self: 'c;
    type CustomIdent = String;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        let node_idx: usize = node_id.into();
        let dom_id = self.tree.get(node_idx).and_then(|n| n.dom_node_id);
        self.translate_style_to_taffy(dom_id)
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &Layout) {
        let node_idx: usize = node_id.into();
        if let Some(node) = self.tree.get_mut(node_idx) {
            node.used_size = Some(translate_taffy_size_back(layout.size));
            node.relative_position = Some(translate_taffy_point_back(layout.location));
        }
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: LayoutInput,
    ) -> LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            let node_idx: usize = node_id.into();
            let fc = tree
                .tree
                .get(node_idx)
                .map(|s| s.formatting_context.clone())
                .unwrap_or_default();

            match fc {
                azul_core::ui_solver::FormattingContext::Flex => {
                    compute_flexbox_layout(tree, node_id, inputs)
                }
                azul_core::ui_solver::FormattingContext::Grid => {
                    compute_grid_layout(tree, node_id, inputs)
                }
                _ => {
                    let node = tree.tree.get(node_idx).unwrap();
                    let style = tree.get_taffy_style(node_idx);
                    compute_leaf_layout(
                        inputs,
                        &style,
                        |_, _| 0.0,
                        |known_dimensions, _available_space| {
                            let intrinsic = node.intrinsic_sizes.unwrap_or_default();
                            Size {
                                width: known_dimensions
                                    .width
                                    .unwrap_or(intrinsic.max_content_width),
                                height: known_dimensions
                                    .height
                                    .unwrap_or(intrinsic.max_content_height),
                            }
                        },
                    )
                }
            }
        })
    }
}

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> CacheTree for TaffyBridge<'a, 'b, T, Q> {
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

/*

    /// A typed representation of the CSS style information for a single node.
    ///
    /// The most important idea in flexbox is the notion of a "main" and "cross" axis, which are always perpendicular to each other.
    /// The orientation of these axes are controlled via the [`FlexDirection`] field of this struct.
    ///
    /// This struct follows the [CSS equivalent](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_Flexible_Box_Layout/Basic_Concepts_of_Flexbox) directly;
    /// information about the behavior on the web should transfer directly.
    ///
    /// Detailed information about the exact behavior of each of these fields
    /// can be found on [MDN](https://developer.mozilla.org/en-US/docs/Web/CSS) by searching for the field name.
    /// The distinction between margin, padding and border is explained well in
    /// this [introduction to the box model](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_Box_Model/Introduction_to_the_CSS_box_model).
    ///
    /// If the behavior does not match the flexbox layout algorithm on the web, please file a bug!
    #[derive(Clone, PartialEq, Debug)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub struct Style<S: CheapCloneStr = DefaultCheapStr> {
        /// This is a dummy field which is necessary to make Taffy compile with the `grid` feature disabled
        /// It should always be set to `core::marker::PhantomData`.
        pub dummy: core::marker::PhantomData<S>,
        /// What layout strategy should be used?
        pub display: Display,
        /// Whether a child is display:table or not. This affects children of block layouts.
        /// This should really be part of `Display`, but it is currently seperate because table layout isn't implemented
        pub item_is_table: bool,
        /// Is it a replaced element like an image or form field?
        /// <https://drafts.csswg.org/css-sizing-3/#min-content-zero>
        pub item_is_replaced: bool,
        /// Should size styles apply to the content box or the border box of the node
        pub box_sizing: BoxSizing,

        // Overflow properties
        /// How children overflowing their container should affect layout
        pub overflow: Point<Overflow>,
        /// How much space (in points) should be reserved for the scrollbars of `Overflow::Scroll` and `Overflow::Auto` nodes.
        pub scrollbar_width: f32,

        // Position properties
        /// What should the `position` value of this struct use as a base offset?
        pub position: Position,
        /// How should the position of this element be tweaked relative to the layout defined?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub inset: Rect<LengthPercentageAuto>,

        // Size properties
        /// Sets the initial size of the item
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub size: Size<Dimension>,
        /// Controls the minimum size of the item
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub min_size: Size<Dimension>,
        /// Controls the maximum size of the item
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::auto"))]
        pub max_size: Size<Dimension>,
        /// Sets the preferred aspect ratio for the item
        ///
        /// The ratio is calculated as width divided by height.
        pub aspect_ratio: Option<f32>,

        // Spacing Properties
        /// How large should the margin be on each side?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub margin: Rect<LengthPercentageAuto>,
        /// How large should the padding be on each side?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub padding: Rect<LengthPercentage>,
        /// How large should the border be on each side?
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub border: Rect<LengthPercentage>,

        // Alignment properties
        /// How this node's children aligned in the cross/block axis?
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub align_items: Option<AlignItems>,
        /// How this node should be aligned in the cross/block axis
        /// Falls back to the parents [`AlignItems`] if not set
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub align_self: Option<AlignSelf>,
        /// How this node's children should be aligned in the inline axis
        #[cfg(feature = "grid")]
        pub justify_items: Option<AlignItems>,
        /// How this node should be aligned in the inline axis
        /// Falls back to the parents [`JustifyItems`] if not set
        #[cfg(feature = "grid")]
        pub justify_self: Option<AlignSelf>,
        /// How should content contained within this item be aligned in the cross/block axis
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub align_content: Option<AlignContent>,
        /// How should content contained within this item be aligned in the main/inline axis
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        pub justify_content: Option<JustifyContent>,
        /// How large should the gaps between items in a grid or flex container be?
        #[cfg(any(feature = "flexbox", feature = "grid"))]
        #[cfg_attr(feature = "serde", serde(default = "style_helpers::zero"))]
        pub gap: Size<LengthPercentage>,

        // Block container properties
        /// How items elements should aligned in the inline axis
        #[cfg(feature = "block_layout")]
        pub text_align: TextAlign,

        // Flexbox container properties
        /// Which direction does the main axis flow in?
        #[cfg(feature = "flexbox")]
        pub flex_direction: FlexDirection,
        /// Should elements wrap, or stay in a single line?
        #[cfg(feature = "flexbox")]
        pub flex_wrap: FlexWrap,

        // Flexbox item properties
        /// Sets the initial main axis size of the item
        #[cfg(feature = "flexbox")]
        pub flex_basis: Dimension,
        /// The relative rate at which this item grows when it is expanding to fill space
        ///
        /// 0.0 is the default value, and this value must be positive.
        #[cfg(feature = "flexbox")]
        pub flex_grow: f32,
        /// The relative rate at which this item shrinks when it is contracting to fit into space
        ///
        /// 1.0 is the default value, and this value must be positive.
        #[cfg(feature = "flexbox")]
        pub flex_shrink: f32,

        // Grid container properies
        /// Defines the track sizing functions (heights) of the grid rows
        #[cfg(feature = "grid")]
        pub grid_template_rows: GridTrackVec<GridTemplateComponent<S>>,
        /// Defines the track sizing functions (widths) of the grid columns
        #[cfg(feature = "grid")]
        pub grid_template_columns: GridTrackVec<GridTemplateComponent<S>>,
        /// Defines the size of implicitly created rows
        #[cfg(feature = "grid")]
        pub grid_auto_rows: GridTrackVec<TrackSizingFunction>,
        /// Defined the size of implicitly created columns
        #[cfg(feature = "grid")]
        pub grid_auto_columns: GridTrackVec<TrackSizingFunction>,
        /// Controls how items get placed into the grid for auto-placed items
        #[cfg(feature = "grid")]
        pub grid_auto_flow: GridAutoFlow,

        // Grid container named properties
        /// Defines the rectangular grid areas
        #[cfg(feature = "grid")]
        pub grid_template_areas: GridTrackVec<GridTemplateArea<S>>,
        /// The named lines between the columns
        #[cfg(feature = "grid")]
        pub grid_template_column_names: GridTrackVec<GridTrackVec<S>>,
        /// The named lines between the rows
        #[cfg(feature = "grid")]
        pub grid_template_row_names: GridTrackVec<GridTrackVec<S>>,

        // Grid child properties
        /// Defines which row in the grid the item should start and end at
        #[cfg(feature = "grid")]
        pub grid_row: Line<GridPlacement<S>>,
        /// Defines which column in the grid the item should start and end at
        #[cfg(feature = "grid")]
        pub grid_column: Line<GridPlacement<S>>,
    }

    impl<S: CheapCloneStr> Style<S> {
        /// The [`Default`] layout, in a form that can be used in const functions
        pub const DEFAULT: Style<S> = Style {
            dummy: core::marker::PhantomData,
            display: Display::DEFAULT,
            item_is_table: false,
            item_is_replaced: false,
            box_sizing: BoxSizing::BorderBox,
            overflow: Point { x: Overflow::Visible, y: Overflow::Visible },
            scrollbar_width: 0.0,
            position: Position::Relative,
            inset: Rect::auto(),
            margin: Rect::zero(),
            padding: Rect::zero(),
            border: Rect::zero(),
            size: Size::auto(),
            min_size: Size::auto(),
            max_size: Size::auto(),
            aspect_ratio: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            gap: Size::zero(),
            // Alignment
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            align_items: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            align_self: None,
            #[cfg(feature = "grid")]
            justify_items: None,
            #[cfg(feature = "grid")]
            justify_self: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            align_content: None,
            #[cfg(any(feature = "flexbox", feature = "grid"))]
            justify_content: None,
            // Block
            #[cfg(feature = "block_layout")]
            text_align: TextAlign::Auto,
            // Flexbox
            #[cfg(feature = "flexbox")]
            flex_direction: FlexDirection::Row,
            #[cfg(feature = "flexbox")]
            flex_wrap: FlexWrap::NoWrap,
            #[cfg(feature = "flexbox")]
            flex_grow: 0.0,
            #[cfg(feature = "flexbox")]
            flex_shrink: 1.0,
            #[cfg(feature = "flexbox")]
            flex_basis: Dimension::AUTO,
            // Grid
            #[cfg(feature = "grid")]
            grid_template_rows: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_columns: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_areas: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_column_names: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_template_row_names: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_auto_rows: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_auto_columns: GridTrackVec::new(),
            #[cfg(feature = "grid")]
            grid_auto_flow: GridAutoFlow::Row,
            #[cfg(feature = "grid")]
            grid_row: Line { start: GridPlacement::<S>::Auto, end: GridPlacement::<S>::Auto },
            #[cfg(feature = "grid")]
            grid_column: Line { start: GridPlacement::<S>::Auto, end: GridPlacement::<S>::Auto },
        };
    }

    impl<S: CheapCloneStr> Default for Style<S> {
        fn default() -> Self {
            Style::DEFAULT
        }
    }

*/

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutFlexboxContainer
    for TaffyBridge<'a, 'b, T, Q>
{
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

impl<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> LayoutGridContainer
    for TaffyBridge<'a, 'b, T, Q>
{
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

fn from_css_size(val: CssSize) -> Dimension {
    match val {
        CssSize::Px(px) => Dimension::length(px),
        CssSize::Percent(p) => Dimension::percent(p / 100.0),
        CssSize::MinContent | CssSize::MaxContent | CssSize::Auto => Dimension::auto(),
    }
}

fn from_pixel_value_lp(val: PixelValue) -> LengthPercentage {
    match val.to_pixels_no_percent() {
        Some(px) => LengthPercentage::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentage::percent(p),
            None => LengthPercentage::length(0.0), // Fallback to 0 if neither px nor percent
        },
    }
}

fn from_pixel_value_lpa(val: PixelValue) -> LengthPercentageAuto {
    match val.to_pixels_no_percent() {
        Some(px) => LengthPercentageAuto::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentageAuto::percent(p),
            None => LengthPercentageAuto::auto(),
        },
    }
}

fn from_taffy_size(val: Size<f32>) -> azul_core::window::LogicalSize {
    azul_core::window::LogicalSize {
        width: val.width,
        height: val.height,
    }
}

#[allow(dead_code)]
fn from_logical_size(val: azul_core::window::LogicalSize) -> Size<AvailableSpace> {
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
        LayoutPosition::Sticky => Position::Relative, // Sticky wird als Relative behandelt
    }
}

fn from_taffy_point(val: taffy::Point<f32>) -> azul_core::window::LogicalPosition {
    azul_core::window::LogicalPosition { x: val.x, y: val.y }
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
