// MIT License
//
// Copyright (c) 2018 Visly Inc.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "anon_nodes_direct_childrenAS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

extern crate azul_core;
extern crate azul_css;
#[cfg(feature = "text_layout")]
pub extern crate azul_text_layout as text_layout;

use std::collections::BTreeMap;
use azul_css::LayoutRect;
use azul_core::{
    ui_solver::PositionedRectangle,
    id_tree::{NodeHierarchy, NodeDepths, NodeDataContainer},
    dom::NodeId,
    display_list::DisplayRectangle,
    traits::GetTextLayout,
};

mod anon;
mod block;
// mod flex;
mod number;
mod geometry;

pub mod style;
#[cfg(feature = "text_layout")]
pub mod ui_solver;
pub use crate::geometry::{Size, Offsets};
pub use crate::number::Number;
pub use crate::style::Style;

pub trait GetStyle {
    fn get_style(&self) -> Style;
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

impl<T: GetTextLayout> RectContent<T> {

    pub fn is_text(&self) -> bool {
        use self::RectContent::*;
        match self {
            Image(_, _) => false,
            Text(_) => true,
        }
    }

    pub fn is_image(&self) -> bool {
        use self::RectContent::*;
        match self {
            Image(_, _) => true,
            Text(_) => false,
        }
    }
}

impl SolvedUi {
    pub fn new<T: GetStyle, U: GetTextLayout>(
        bounds: LayoutRect,
        node_hierarchy: &NodeHierarchy,
        display_rects: &NodeDataContainer<T>,
        rect_contents: &mut BTreeMap<NodeId, RectContent<U>>,
        node_depths: &NodeDepths,
    ) -> Self {

        use crate::anon::AnonDom;

        let styles = display_rects.transform(|node, node_id| Style {
            aspect_ratio: match rect_contents.get(&node_id) {
                Some(RectContent::Image(w, h)) => Number::Defined(*w as f32 / *h as f32),
                _ => Number::Undefined,
            },
            .. node.get_style()
        });

        let anon_dom = AnonDom::new(
            node_hierarchy,
            &styles,
            node_depths,
            rect_contents,
        );

        // let solved_rects = flex::compute(NodeId::ZERO, node_hierarchy, &styles, rect_contents, bounds.size, node_depths);
        let solved_rects = block::compute(
            bounds.size,
            &anon_dom,
            rect_contents,
        );

        SolvedUi { solved_rects }
    }
}

impl GetStyle for DisplayRectangle {

    fn get_style(&self) -> Style {

        use crate::style::*;
        use azul_css::{
            PixelValue, LayoutDisplay, LayoutDirection, LayoutWrap, LayoutPosition,
            LayoutAlignItems, LayoutAlignContent, LayoutJustifyContent,
            LayoutBoxSizing, Overflow as LayoutOverflow, CssPropertyValue,
        };
        use azul_core::ui_solver::DEFAULT_FONT_SIZE;

        let rect_layout = &self.layout;
        let rect_style = &self.style;

        #[inline]
        fn translate_dimension(input: Option<CssPropertyValue<PixelValue>>) -> Dimension {
            use azul_css::{SizeMetric, EM_HEIGHT, PT_TO_PX};
            match input {
                None => Dimension::Undefined,
                Some(CssPropertyValue::Auto) => Dimension::Auto,
                Some(CssPropertyValue::None) => Dimension::Pixels(0.0),
                Some(CssPropertyValue::Initial) => Dimension::Undefined,
                Some(CssPropertyValue::Inherit) => Dimension::Undefined,
                Some(CssPropertyValue::Exact(pixel_value)) => match pixel_value.metric {
                    SizeMetric::Px => Dimension::Pixels(pixel_value.number.get()),
                    SizeMetric::Percent => Dimension::Percent(pixel_value.number.get()),
                    SizeMetric::Pt => Dimension::Pixels(pixel_value.number.get() * PT_TO_PX),
                    SizeMetric::Em => Dimension::Pixels(pixel_value.number.get() * EM_HEIGHT),
                }
            }
        }

        Style {
            display: match rect_layout.display {
                None => Display::Block,
                Some(CssPropertyValue::None) => Display::None,
                Some(CssPropertyValue::Auto) => Display::Block,
                Some(CssPropertyValue::Initial) => Display::Block,
                Some(CssPropertyValue::Inherit) => Display::Block,
                Some(CssPropertyValue::Exact(LayoutDisplay::Block)) => Display::Block,
                Some(CssPropertyValue::Exact(LayoutDisplay::Flex)) => Display::Flex,
                Some(CssPropertyValue::Exact(LayoutDisplay::InlineBlock)) => Display::InlineBlock,
            },
            box_sizing: match rect_layout.box_sizing.unwrap_or_default().get_property_or_default() {
                None => BoxSizing::ContentBox,
                Some(LayoutBoxSizing::ContentBox) => BoxSizing::ContentBox,
                Some(LayoutBoxSizing::BorderBox) => BoxSizing::BorderBox,
            },
            position_type: match rect_layout.position.unwrap_or_default().get_property_or_default() {
                Some(LayoutPosition::Static) => PositionType::Static,
                Some(LayoutPosition::Relative) => PositionType::Relative,
                Some(LayoutPosition::Absolute) => PositionType::Absolute,
                Some(LayoutPosition::Fixed) => PositionType::Fixed,
                None => PositionType::Static,
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
            overflow: match rect_layout.overflow_x.unwrap_or_default().get_property_or_default() {
                Some(LayoutOverflow::Scroll) => Overflow::Scroll,
                Some(LayoutOverflow::Auto) => Overflow::Scroll,
                Some(LayoutOverflow::Hidden) => Overflow::Hidden,
                Some(LayoutOverflow::Visible) => Overflow::Visible,
                None => Overflow::Scroll,
            },
            align_items: match rect_layout.align_items.unwrap_or_default().get_property_or_default() {
                Some(LayoutAlignItems::Stretch) => AlignItems::Stretch,
                Some(LayoutAlignItems::Center) => AlignItems::Center,
                Some(LayoutAlignItems::Start) => AlignItems::FlexStart,
                Some(LayoutAlignItems::End) => AlignItems::FlexEnd,
                None => AlignItems::FlexStart,
            },
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
                left: translate_dimension(rect_layout.left.map(|prop| prop.map_property(|l| l.inner))),
                right: translate_dimension(rect_layout.right.map(|prop| prop.map_property(|r| r.inner))),
                top: translate_dimension(rect_layout.top.map(|prop| prop.map_property(|t| t.inner))),
                bottom: translate_dimension(rect_layout.bottom.map(|prop| prop.map_property(|b| b.inner))),
            },
            margin: Offsets {
                left: translate_dimension(rect_layout.margin_left.map(|prop| prop.map_property(|l| l.inner))),
                right: translate_dimension(rect_layout.margin_right.map(|prop| prop.map_property(|r| r.inner))),
                top: translate_dimension(rect_layout.margin_top.map(|prop| prop.map_property(|t| t.inner))),
                bottom: translate_dimension(rect_layout.margin_bottom.map(|prop| prop.map_property(|b| b.inner))),
            },
            padding: Offsets {
                left: translate_dimension(rect_layout.padding_left.map(|prop| prop.map_property(|l| l.inner))),
                right: translate_dimension(rect_layout.padding_right.map(|prop| prop.map_property(|r| r.inner))),
                top: translate_dimension(rect_layout.padding_top.map(|prop| prop.map_property(|t| t.inner))),
                bottom: translate_dimension(rect_layout.padding_bottom.map(|prop| prop.map_property(|b| b.inner))),
            },
            border: Offsets {
                left: translate_dimension(rect_layout.border_left_width.map(|prop| prop.map_property(|l| l.inner))),
                right: translate_dimension(rect_layout.border_right_width.map(|prop| prop.map_property(|r| r.inner))),
                top: translate_dimension(rect_layout.border_top_width.map(|prop| prop.map_property(|t| t.inner))),
                bottom: translate_dimension(rect_layout.border_bottom_width.map(|prop| prop.map_property(|b| b.inner))),
            },
            flex_grow: rect_layout.flex_grow.unwrap_or_default().get_property_or_default().unwrap_or_default().inner.get(),
            flex_shrink: rect_layout.flex_shrink.unwrap_or_default().get_property_or_default().unwrap_or_default().inner.get(),
            size: Size {
                width: translate_dimension(rect_layout.width.map(|prop| prop.map_property(|l| l.inner))),
                height: translate_dimension(rect_layout.height.map(|prop| prop.map_property(|l| l.inner))),
            },
            min_size: Size {
                width: translate_dimension(rect_layout.min_width.map(|prop| prop.map_property(|l| l.inner))),
                height: translate_dimension(rect_layout.min_height.map(|prop| prop.map_property(|l| l.inner))),
            },
            max_size: Size {
                width: translate_dimension(rect_layout.max_width.map(|prop| prop.map_property(|l| l.inner))),
                height: translate_dimension(rect_layout.max_height.map(|prop| prop.map_property(|l| l.inner))),
            },
            align_self: AlignSelf::default(), // todo!
            flex_basis: Dimension::default(), // todo!
            aspect_ratio: Number::Undefined,
            font_size_px: rect_style.font_size.and_then(|fs| fs.get_property_owned()).unwrap_or(DEFAULT_FONT_SIZE).inner,
            line_height: rect_style.line_height.and_then(|lh| lh.map_property(|lh| lh.inner).get_property_owned()).map(|lh| lh.get()),
            letter_spacing: rect_style.letter_spacing.and_then(|ls| ls.map_property(|ls| ls.inner).get_property_owned()),
            word_spacing: rect_style.word_spacing.and_then(|ws| ws.map_property(|ws| ws.inner).get_property_owned()),
            tab_width: rect_style.tab_width.and_then(|tw| tw.map_property(|tw| tw.inner).get_property_owned()).map(|tw| tw.get()),
        }
    }
}