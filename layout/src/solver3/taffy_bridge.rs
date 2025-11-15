use azul_core::dom::FormattingContext;
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
        LayoutGridAutoFlow, LayoutHeight, LayoutJustifyContent, LayoutPosition, LayoutWidth,
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
        getters::{
            get_css_border_bottom_width, get_css_border_left_width, get_css_border_right_width,
            get_css_border_top_width, get_css_bottom, get_css_height, get_css_left,
            get_css_margin_bottom, get_css_margin_left, get_css_margin_right, get_css_margin_top,
            get_css_max_height, get_css_max_width, get_css_min_height, get_css_min_width,
            get_css_padding_bottom, get_css_padding_left, get_css_padding_right,
            get_css_padding_top, get_css_right, get_css_top, get_css_width, get_position,
            MultiValue,
        },
        layout_tree::{get_display_type, LayoutNode, LayoutTree},
        sizing, LayoutContext,
    },
    text3::cache::{FontLoaderTrait, ParsedFontTrait},
};

// Helper function to convert MultiValue<PixelValue> to LengthPercentageAuto
fn multi_value_to_lpa(mv: MultiValue<PixelValue>) -> taffy::LengthPercentageAuto {
    match mv {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            taffy::LengthPercentageAuto::auto()
        }
        MultiValue::Exact(pv) => {
            pv.to_pixels_no_percent()
                .map(taffy::LengthPercentageAuto::length)
                .or_else(|| pv.to_percent().map(|p| taffy::LengthPercentageAuto::percent(p.get())))
                .unwrap_or_else(taffy::LengthPercentageAuto::auto)
        }
    }
}

// Helper function to convert MultiValue<PixelValue> to LengthPercentage
fn multi_value_to_lp(mv: MultiValue<PixelValue>) -> taffy::LengthPercentage {
    match mv {
        MultiValue::Auto | MultiValue::Initial | MultiValue::Inherit => {
            taffy::LengthPercentage::ZERO
        }
        MultiValue::Exact(pv) => {
            pv.to_pixels_no_percent()
                .map(taffy::LengthPercentage::length)
                .or_else(|| pv.to_percent().map(|p| taffy::LengthPercentage::percent(p.get())))
                .unwrap_or_else(|| taffy::LengthPercentage::ZERO)
        }
    }
}

// Helper function to convert plain PixelValue to LengthPercentage
fn pixel_to_lp(pv: PixelValue) -> taffy::LengthPercentage {
    pv.to_pixels_no_percent()
        .map(taffy::LengthPercentage::length)
        .or_else(|| pv.to_percent().map(|p| taffy::LengthPercentage::percent(p.get())))
        .unwrap_or_else(|| taffy::LengthPercentage::ZERO)
}

/// The bridge struct that implements Taffy's traits.
/// It holds mutable references to the solver's data structures, allowing Taffy
/// to read styles and write layout results back into our `LayoutTree`.
struct TaffyBridge<'a, 'b, T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    ctx: &'a mut LayoutContext<'b, T, Q>,
    tree: &'a mut LayoutTree<T>,
}

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
        taffy_style.display = layout_display_to_taffy(
            CssPropertyValue::Exact(get_display_type(styled_dom, id))
        );

        // Position
        taffy_style.position = from_layout_position(get_position(styled_dom, id, node_state).unwrap_or_default());

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

        taffy_style.size = taffy::Size {
            width: from_layout_width(width.unwrap_or_default()),
            height: from_layout_height(height.unwrap_or_default()),
        };

        // Min/Max Size
        taffy_style.min_size = taffy::Size {
            width: pixel_to_lp(get_css_min_width(styled_dom, id, node_state).unwrap_or_default().inner).into(),
            height: pixel_to_lp(get_css_min_height(styled_dom, id, node_state).unwrap_or_default().inner).into(),
        };
        taffy_style.max_size = taffy::Size {
            width: pixel_to_lp(get_css_max_width(styled_dom, id, node_state).unwrap_or_default().inner).into(),
            height: pixel_to_lp(get_css_max_height(styled_dom, id, node_state).unwrap_or_default().inner).into(),
        };

        // Box Model (margin, padding, border)
        taffy_style.margin = taffy::Rect {
            left: multi_value_to_lpa(get_css_margin_left(styled_dom, id, node_state)),
            right: multi_value_to_lpa(get_css_margin_right(styled_dom, id, node_state)),
            top: multi_value_to_lpa(get_css_margin_top(styled_dom, id, node_state)),
            bottom: multi_value_to_lpa(get_css_margin_bottom(styled_dom, id, node_state)),
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
        FormattingContext::Flex => compute_flexbox_layout(&mut bridge, node_idx.into(), inputs),
        FormattingContext::Grid => compute_grid_layout(&mut bridge, node_idx.into(), inputs),
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
        let style = self.translate_style_to_taffy(dom_id);
        println!(
            "  -> Taffy get_core_container_style for node_idx={}, dom_id={:?}",
            node_idx, dom_id
        );
        println!(
            "     display={:?}, flex_grow={:?}, size={:?}",
            style.display, style.flex_grow, style.size
        );
        style
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &Layout) {
        let node_idx: usize = node_id.into();
        println!(
            "  -> Taffy set_unrounded_layout: node_idx={}, size={:?}, location={:?}",
            node_idx, layout.size, layout.location
        );
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
                FormattingContext::Flex => compute_flexbox_layout(tree, node_id, inputs),
                FormattingContext::Grid => compute_grid_layout(tree, node_id, inputs),
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

fn from_layout_width(val: LayoutWidth) -> Dimension {
    match val {
        LayoutWidth::Auto => Dimension::auto(),  // NEW: Handle Auto variant
        LayoutWidth::Px(px) => {
            // Try to extract pixel or percent value
            match px.to_pixels_no_percent() {
                Some(pixels) => Dimension::length(pixels),
                None => match px.to_percent() {
                    Some(p) => Dimension::percent(p.get()), // p is already normalized (0.0-1.0)
                    None => Dimension::auto(),
                },
            }
        }
        LayoutWidth::MinContent | LayoutWidth::MaxContent => Dimension::auto(),
    }
}

fn from_layout_height(val: LayoutHeight) -> Dimension {
    match val {
        LayoutHeight::Auto => Dimension::auto(),  // NEW: Handle Auto variant
        LayoutHeight::Px(px) => {
            // Try to extract pixel or percent value
            match px.to_pixels_no_percent() {
                Some(pixels) => Dimension::length(pixels),
                None => match px.to_percent() {
                    Some(p) => Dimension::percent(p.get()), // p is already normalized (0.0-1.0)
                    None => Dimension::auto(),
                },
            }
        }
        LayoutHeight::MinContent | LayoutHeight::MaxContent => Dimension::auto(),
    }
}

fn from_pixel_value_lp(val: PixelValue) -> LengthPercentage {
    match val.to_pixels_no_percent() {
        Some(px) => LengthPercentage::length(px),
        None => match val.to_percent() {
            Some(p) => LengthPercentage::percent(p.get()), // p is already normalized (0.0-1.0)
            None => LengthPercentage::length(0.0),         /* Fallback to 0 if neither px nor
                                                             * percent */
        },
    }
}

fn from_pixel_value_lpa(val: PixelValue) -> LengthPercentageAuto {
    match val.to_pixels_no_percent() {
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
        LayoutPosition::Sticky => Position::Relative, // Sticky wird als Relative behandelt
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
