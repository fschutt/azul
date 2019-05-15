extern crate azul_core;
extern crate azul_css;

mod algo;
mod number;
mod geometry;
mod style;

use std::collections::BTreeMap;
use azul_css::{LayoutRect, RectLayout};
use azul_core::{
    ui_solver::PositionedRectangle,
    id_tree::{NodeHierarchy, NodeDataContainer},
    dom::NodeId,
};

pub trait GetRectLayout { fn get_rect_layout(&self) -> &RectLayout; }
pub trait GetTextLayout { fn get_text_layout(&mut self, bounds: LayoutRect) -> LayoutedInlineText; }

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct LayoutedInlineText {
    pub lines: Vec<LayoutRect>,
    pub layout_direction: InlineTextDirection,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InlineTextDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct SolvedUi {
    pub solved_rects: NodeDataContainer<PositionedRectangle>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RectContent<T: GetTextLayout> {
    // Returns the original (width, height) of the image
    Image(usize, usize),
    /// Gives access an anonymous struct which, given the text bounds,
    /// can be used to calculate the text dimensions
    Text(T),
}

impl LayoutedInlineText {
    pub fn get_bounds(&self) -> LayoutRect {
        LayoutRect::union(self.lines.iter().map(|c| *c)).unwrap_or(LayoutRect::zero())
    }
}

impl SolvedUi {
    pub fn new<T: GetRectLayout, U: GetTextLayout>(
        bounds: LayoutRect,
        node_hierarchy: &NodeHierarchy,
        display_rects: &NodeDataContainer<T>,
        rect_contents: BTreeMap<NodeId, RectContent<U>>,
    ) -> Self {

        let styles = display_rects.transform(|node, node_id| {

            use style::*;
            use geometry::{Size, Offsets};
            use number::Number;
            use azul_css::{
                CssPropertyValue, PixelValue, LayoutDisplay, LayoutPosition,
                LayoutDirection, LayoutWrap, LayoutAlignItems, LayoutAlignContent,
                LayoutJustifyContent,
            };

            let rect_layout = node.get_rect_layout();

            #[inline]
            fn translate_dimension(input: Option<CssPropertyValue<PixelValue>>) -> Dimension {
                match input {
                    None => Dimension::Undefined,
                    Some(CssPropertyValue::Auto) => Dimension::Auto,
                    Some(CssPropertyValue::None) => Dimension::Points(0.0),
                    Some(CssPropertyValue::Initial) => Dimension::Points(0.0),
                    Some(CssPropertyValue::Inherit) => Dimension::Undefined,
                    Some(CssPropertyValue::Exact(pixel_value)) => Dimension::Points(pixel_value.to_points()), // todo: percent!
                }
            }

            let image_aspect_ratio = match rect_contents.get(&node_id) {
                Some(RectContent::Image(w, h)) => Number::Defined(*w as f32 / *h as f32),
                _ => Number::Undefined,
            };

            Style {
                display: match rect_layout.display.unwrap_or_default().get_property_or_default() {
                    Some(LayoutDisplay::Flex) => Display::Flex,
                    Some(LayoutDisplay::None) => Display::None,
                    Some(LayoutDisplay::Inline) => Display::None,
                    None => Display::Flex,
                },
                position_type: match rect_layout.position.unwrap_or_default().get_property_or_default() {
                    Some(LayoutPosition::Static) => PositionType::Relative, // todo - static?
                    Some(LayoutPosition::Relative) => PositionType::Relative,
                    Some(LayoutPosition::Absolute) => PositionType::Absolute,
                    None => PositionType::Relative,
                },
                direction: Direction::LTR,
                flex_direction: match rect_layout.direction.unwrap_or_default().get_property_or_default() {
                    Some(LayoutDirection::Row) => FlexDirection::Row,
                    Some(LayoutDirection::RowReverse) => FlexDirection::RowReverse,
                    Some(LayoutDirection::Column) => FlexDirection::Column,
                    Some(LayoutDirection::ColumnReverse) => FlexDirection::ColumnReverse,
                    None => FlexDirection::Row,
                },
                flex_wrap: match rect_layout.wrap.unwrap_or_default().get_property_or_default() {
                    Some(LayoutWrap::Wrap) => FlexWrap::Wrap,
                    Some(LayoutWrap::NoWrap) => FlexWrap::NoWrap,
                    None => FlexWrap::Wrap,
                },
                overflow: Overflow::Visible, // todo!
                align_items: match rect_layout.align_items.unwrap_or_default().get_property_or_default() {
                    Some(LayoutAlignItems::Stretch) => AlignItems::Stretch,
                    Some(LayoutAlignItems::Center) => AlignItems::Center,
                    Some(LayoutAlignItems::Start) => AlignItems::FlexStart,
                    Some(LayoutAlignItems::End) => AlignItems::FlexEnd,
                    None => AlignItems::FlexStart,
                },
                align_self: AlignSelf::Auto, // todo!
                align_content: match rect_layout.align_content.unwrap_or_default().get_property_or_default() {
                    Some(LayoutAlignContent::Stretch) => AlignContent::Stretch,
                    Some(LayoutAlignContent::Center) => AlignContent::Center,
                    Some(LayoutAlignContent::Start) => AlignContent::FlexStart,
                    Some(LayoutAlignContent::End) => AlignContent::FlexEnd,
                    Some(LayoutAlignContent::SpaceBetween) => AlignContent::SpaceBetween,
                    Some(LayoutAlignContent::SpaceAround) => AlignContent::SpaceAround,
                    None => AlignContent::Stretch,
                },
                justify_content: match rect_layout.justify_content.unwrap_or_default().get_property_or_default() {
                    Some(LayoutJustifyContent::Center) => JustifyContent::Center,
                    Some(LayoutJustifyContent::Start) => JustifyContent::FlexStart,
                    Some(LayoutJustifyContent::End) => JustifyContent::FlexEnd,
                    Some(LayoutJustifyContent::SpaceBetween) => JustifyContent::SpaceBetween,
                    Some(LayoutJustifyContent::SpaceAround) => JustifyContent::SpaceAround,
                    Some(LayoutJustifyContent::SpaceEvenly) => JustifyContent::SpaceEvenly,
                    None => JustifyContent::FlexStart,
                },
                position: Offsets {
                    left: translate_dimension(rect_layout.left.map(|prop| prop.map_property(|l| l.0))),
                    right: translate_dimension(rect_layout.right.map(|prop| prop.map_property(|r| r.0))),
                    top: translate_dimension(rect_layout.top.map(|prop| prop.map_property(|t| t.0))),
                    bottom: translate_dimension(rect_layout.bottom.map(|prop| prop.map_property(|b| b.0))),
                },
                margin: Offsets {
                    left: translate_dimension(rect_layout.margin_left.map(|prop| prop.map_property(|l| l.0))),
                    right: translate_dimension(rect_layout.margin_right.map(|prop| prop.map_property(|r| r.0))),
                    top: translate_dimension(rect_layout.margin_top.map(|prop| prop.map_property(|t| t.0))),
                    bottom: translate_dimension(rect_layout.margin_bottom.map(|prop| prop.map_property(|b| b.0))),
                },
                padding: Offsets {
                    left: translate_dimension(rect_layout.padding_left.map(|prop| prop.map_property(|l| l.0))),
                    right: translate_dimension(rect_layout.padding_right.map(|prop| prop.map_property(|r| r.0))),
                    top: translate_dimension(rect_layout.padding_top.map(|prop| prop.map_property(|t| t.0))),
                    bottom: translate_dimension(rect_layout.padding_bottom.map(|prop| prop.map_property(|b| b.0))),
                },
                border: Offsets {
                    left: translate_dimension(rect_layout.border_left_width.map(|prop| prop.map_property(|l| l.0))),
                    right: translate_dimension(rect_layout.border_right_width.map(|prop| prop.map_property(|r| r.0))),
                    top: translate_dimension(rect_layout.border_top_width.map(|prop| prop.map_property(|t| t.0))),
                    bottom: translate_dimension(rect_layout.border_bottom_width.map(|prop| prop.map_property(|b| b.0))),
                },
                flex_grow: rect_layout.flex_grow.unwrap_or_default().get_property_or_default().unwrap_or_default().0.get(),
                flex_shrink: rect_layout.flex_shrink.unwrap_or_default().get_property_or_default().unwrap_or_default().0.get(),
                flex_basis: Dimension::Auto, // todo!
                size: Size {
                    width: translate_dimension(rect_layout.width.map(|prop| prop.map_property(|l| l.0))),
                    height: translate_dimension(rect_layout.height.map(|prop| prop.map_property(|l| l.0))),
                },
                min_size: Size {
                    width: translate_dimension(rect_layout.min_width.map(|prop| prop.map_property(|l| l.0))),
                    height: translate_dimension(rect_layout.min_height.map(|prop| prop.map_property(|l| l.0))),
                },
                max_size: Size {
                    width: translate_dimension(rect_layout.max_width.map(|prop| prop.map_property(|l| l.0))),
                    height: translate_dimension(rect_layout.max_height.map(|prop| prop.map_property(|l| l.0))),
                },
                aspect_ratio: image_aspect_ratio,
            }
        });

        let mut solved_rects = algo::compute(NodeId::ZERO, node_hierarchy, &styles, bounds.size);

        // Offset all layouted rectangles by the origin of the bounds
        let origin_x = bounds.origin.x;
        let origin_y = bounds.origin.y;
        for rect in solved_rects.internal.iter_mut() {
            rect.bounds.origin.x += origin_x;
            rect.bounds.origin.y += origin_y;
        }

        SolvedUi { solved_rects }
    }
}
