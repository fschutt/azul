#![allow(dead_code)]

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
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use crate::{
    geometry::{Offsets, DEFAULT_OFFSETS, DEFAULT_SIZE, Size},
    number::Number,
};
use azul_css::PixelValue;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AlignItems {
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

impl AlignItems {
    pub(crate) const DEFAULT: AlignItems = AlignItems::FlexStart;
}

impl Default for AlignItems {
    fn default() -> AlignItems {
        AlignItems::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AlignSelf {
    Auto,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

impl AlignSelf {
    pub(crate) const DEFAULT: AlignSelf = AlignSelf::Auto;
}

impl Default for AlignSelf {
    fn default() -> AlignSelf {
        AlignSelf::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AlignContent {
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
}

impl AlignContent {
    pub(crate) const DEFAULT: AlignContent = AlignContent::Stretch;
}

impl Default for AlignContent {
    fn default() -> AlignContent {
        AlignContent::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Direction {
    Inherit,
    LTR,
    RTL,
}

impl Direction {
    const DEFAULT: Direction = Direction::Inherit;
}

impl Default for Direction {
    fn default() -> Direction {
        Direction::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Display {
    Flex,
    Block,
    InlineBlock,
    None,
}

impl Display {
    const DEFAULT: Display = Display::Block;
}

impl Default for Display {
    fn default() -> Display {
        Display::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FlexDirection {
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

impl FlexDirection {
    pub(crate) const DEFAULT: FlexDirection = FlexDirection::Row;
}

impl Default for FlexDirection {
    fn default() -> FlexDirection {
        FlexDirection::DEFAULT
    }
}

impl FlexDirection {
    pub(crate) fn is_row(self) -> bool {
        self == FlexDirection::Row || self == FlexDirection::RowReverse
    }

    pub(crate) fn is_column(self) -> bool {
        self == FlexDirection::Column || self == FlexDirection::ColumnReverse
    }

    pub(crate) fn is_reverse(self) -> bool {
        self == FlexDirection::RowReverse || self == FlexDirection::ColumnReverse
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl JustifyContent {
    pub(crate) const DEFAULT: JustifyContent = JustifyContent::FlexStart;
}

impl Default for JustifyContent {
    fn default() -> JustifyContent {
        JustifyContent::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Overflow {
    Auto,
    Visible,
    Hidden,
    Scroll,
}

impl Overflow {
    pub(crate) const DEFAULT: Overflow = Overflow::Scroll;
}

impl Default for Overflow {
    fn default() -> Overflow {
        Overflow::DEFAULT
    }
}

impl Overflow {
    pub fn allows_horizontal_overflow(&self) -> bool {
        use self::Overflow::*;
        match self {
            Auto => false,
            Visible => true,
            Hidden => true,
            Scroll => false,
        }
    }

    pub fn allows_vertical_overflow(&self) -> bool {
        true
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PositionType {
    Relative,
    Absolute,
    Static,
    Fixed,
}

impl PositionType {
    pub(crate) const DEFAULT: PositionType = PositionType::Relative;
}

impl Default for PositionType {
    fn default() -> PositionType {
        PositionType::Relative
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub(crate) const DEFAULT: FlexWrap = FlexWrap::Wrap;
}

impl Default for FlexWrap {
    fn default() -> FlexWrap {
        FlexWrap::DEFAULT
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Dimension {
    Undefined,
    Auto,
    Pixels(f32),
    Percent(f32),
}

impl Dimension {
    pub(crate) const DEFAULT: Dimension = Dimension::Undefined;
}

impl Default for Dimension {
    #[inline]
    fn default() -> Dimension {
        Dimension::DEFAULT
    }
}

impl Dimension {
    pub(crate) fn resolve(self, parent_width: Number) -> Number {
        match self {
            Dimension::Pixels(pixels) => Number::Defined(pixels),
            Dimension::Percent(percent) => parent_width * (percent / 100.0),
            _ => Number::Undefined,
        }
    }

    pub(crate) fn is_defined(self) -> bool {
        match self {
            Dimension::Pixels(_) => true,
            Dimension::Percent(_) => true,
            _ => false,
        }
    }
}

impl Default for Offsets<Dimension> {
    #[inline]
    fn default() -> Offsets<Dimension> {
        DEFAULT_OFFSETS
    }
}

impl Default for Size<Dimension> {
    #[inline]
    fn default() -> Size<Dimension> {
        DEFAULT_SIZE
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

impl BoxSizing {
    pub(crate) const DEFAULT: BoxSizing = BoxSizing::ContentBox;
}

impl Default for BoxSizing {
    #[inline]
    fn default() -> BoxSizing {
        BoxSizing::DEFAULT
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Style {pub display: Display,
    pub box_sizing: BoxSizing,
    pub position_type: PositionType,
    pub direction: Direction,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub overflow: Overflow,
    pub align_items: AlignItems,
    pub align_self: AlignSelf,
    pub align_content: AlignContent,
    pub justify_content: JustifyContent,
    pub position: Offsets<Dimension>,
    pub margin: Offsets<Dimension>,
    pub padding: Offsets<Dimension>,
    pub border: Offsets<Dimension>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub size: Size<Dimension>,
    pub min_size: Size<Dimension>,
    pub max_size: Size<Dimension>,
    pub aspect_ratio: Number,
    pub font_size_px: PixelValue,
    pub letter_spacing: Option<PixelValue>,
    pub word_spacing: Option<PixelValue>,
    pub line_height: Option<f32>,
    pub tab_width: Option<f32>,
}

impl Style {
    pub(crate) const DEFAULT: Style = Style {
        display: Display::DEFAULT,
        box_sizing: BoxSizing::DEFAULT,
        position_type: PositionType::DEFAULT,
        direction: Direction::DEFAULT,
        flex_direction: FlexDirection::DEFAULT,
        flex_wrap: FlexWrap::DEFAULT,
        overflow: Overflow::DEFAULT,
        align_items: AlignItems::DEFAULT,
        align_self: AlignSelf::DEFAULT,
        align_content: AlignContent::DEFAULT,
        justify_content: JustifyContent::DEFAULT,
        position: DEFAULT_OFFSETS,
        margin: DEFAULT_OFFSETS,
        padding: DEFAULT_OFFSETS,
        border: DEFAULT_OFFSETS,
        flex_grow: 0.0,
        flex_shrink: 1.0,
        flex_basis: Dimension::Auto,
        size: DEFAULT_SIZE,
        min_size: DEFAULT_SIZE,
        max_size: DEFAULT_SIZE,
        aspect_ratio: Number::Undefined,
        font_size_px: PixelValue::const_px(10),
        letter_spacing: None,
        line_height: None,
        word_spacing: None,
        tab_width: None,
    };
}

pub(crate) static DEFAULT_STYLE: Style = Style::DEFAULT;

impl Default for Style {
    fn default() -> Style {
        Style::DEFAULT
    }
}

impl Style {
    pub(crate) fn min_main_size(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.min_size.width,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.min_size.height,
        }
    }

    pub(crate) fn max_main_size(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.max_size.width,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.max_size.height,
        }
    }

    pub(crate) fn main_margin_start(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.margin.left,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.margin.top,
        }
    }

    pub(crate) fn main_margin_end(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.margin.right,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.margin.bottom,
        }
    }

    pub(crate) fn cross_size(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.size.height,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.size.width,
        }
    }

    pub(crate) fn min_cross_size(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.min_size.height,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.min_size.width,
        }
    }

    pub(crate) fn max_cross_size(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.max_size.height,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.max_size.width,
        }
    }

    pub(crate) fn cross_margin_start(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.margin.top,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.margin.left,
        }
    }

    pub(crate) fn cross_margin_end(&self, direction: FlexDirection) -> Dimension {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.margin.bottom,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.margin.right,
        }
    }

    pub(crate) fn align_self(&self, parent: &Style) -> AlignSelf {
        if self.align_self == AlignSelf::Auto {
            match parent.align_items {
                AlignItems::FlexStart => AlignSelf::FlexStart,
                AlignItems::FlexEnd => AlignSelf::FlexEnd,
                AlignItems::Center => AlignSelf::Center,
                AlignItems::Baseline => AlignSelf::Baseline,
                AlignItems::Stretch => AlignSelf::Stretch,
            }
        } else {
            self.align_self
        }
    }
}