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

use std::ops::Add;
use azul_core::ui_solver::ResolvedOffsets;
use crate::{
    number::Number,
    style::{FlexDirection, Dimension},
};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Rect {
    pub origin: RectOrigin,
    pub size: RectSize,
    pub margin: ResolvedOffsets,
    pub padding: ResolvedOffsets,
    pub border_widths: ResolvedOffsets,
}

impl Rect {
    pub const fn undefined() -> Self {
        Self {
            origin: RectOrigin::undefined(),
            size: RectSize::undefined(),
            margin: ResolvedOffsets::zero(),
            padding: ResolvedOffsets::zero(),
            border_widths: ResolvedOffsets::zero(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RectOrigin {
    pub x: Number,
    pub y: Number,
}

impl RectOrigin {
    pub const fn undefined() -> Self {
        Self {
            x: Number::Undefined,
            y: Number::Undefined,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RectSize {
    pub width: Number,
    pub height: Number,
}

impl RectSize {
    pub const fn undefined() -> Self {
        Self {
            width: Number::Undefined,
            height: Number::Undefined,
        }
    }

    pub(crate) fn main(self, direction: FlexDirection) -> Number {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.width,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.height,
        }
    }

    pub(crate) fn cross(self, direction: FlexDirection) -> Number {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.height,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.width,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Offsets<T> {
    pub top: T,
    pub left: T,
    pub bottom: T,
    pub right: T,
}

pub(crate) const DEFAULT_OFFSETS: Offsets<Dimension> = Offsets {
    top: Dimension::DEFAULT,
    left: Dimension::DEFAULT,
    bottom: Dimension::DEFAULT,
    right: Dimension::DEFAULT,
};

impl<T> Offsets<T> {
    pub(crate) fn map<R, F: Fn(T) -> R>(self, f: F) -> Offsets<R> {
        Offsets { left: f(self.left), right: f(self.right), top: f(self.top), bottom: f(self.bottom) }
    }
}

impl<T: Add<Output = T> + Copy + Clone> Offsets<T> {
    pub(crate) fn horizontal(&self) -> T {
        self.left + self.right
    }

    pub(crate) fn vertical(&self) -> T {
        self.top + self.bottom
    }

    pub(crate) fn main(&self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.left + self.right,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.top + self.bottom,
        }
    }

    pub(crate) fn cross(&self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.top + self.bottom,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.left + self.right,
        }
    }
}

impl Add<RectSize> for RectSize {
    type Output = RectSize;

    fn add(self, rhs: RectSize) -> RectSize {
        RectSize {
            width: self.width + rhs.width,
            height: self.height + rhs.height,
        }
    }
}

impl<T: Add<Output = T>> Add<Offsets<T>> for Offsets<T> {
    type Output = Offsets<T>;

    fn add(self, rhs: Offsets<T>) -> Offsets<T> {
        Offsets {
            left: self.left + rhs.left,
            right: self.right + rhs.right,
            top: self.top + rhs.top,
            bottom: self.bottom + rhs.bottom,
        }
    }
}

impl<T: Copy + Clone> Offsets<T> {
    pub(crate) fn main_start(&self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.left,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.top,
        }
    }

    pub(crate) fn main_end(&self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.right,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.bottom,
        }
    }

    pub(crate) fn cross_start(&self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.top,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.left,
        }
    }

    pub(crate) fn cross_end(&self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.bottom,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.right,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

pub(crate) const DEFAULT_SIZE: Size<Dimension> = Size {
    width: Dimension::DEFAULT,
    height: Dimension::DEFAULT,
};

impl<T> Size<T> {
    pub(crate) fn map<R, F>(self, f: F) -> Size<R>
    where
        F: Fn(T) -> R,
    {
        Size { width: f(self.width), height: f(self.height) }
    }

    pub(crate) fn set_main(&mut self, direction: FlexDirection, value: T) {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.width = value,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.height = value,
        }
    }

    pub(crate) fn set_cross(&mut self, direction: FlexDirection, value: T) {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.height = value,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.width = value,
        }
    }

    pub(crate) fn main(self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.width,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.height,
        }
    }

    pub(crate) fn cross(self, direction: FlexDirection) -> T {
        match direction {
            FlexDirection::Row | FlexDirection::RowReverse => self.height,
            FlexDirection::Column | FlexDirection::ColumnReverse => self.width,
        }
    }
}