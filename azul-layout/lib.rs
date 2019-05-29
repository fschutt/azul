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

extern crate azul_core;
extern crate azul_css;

use std::collections::BTreeMap;
use azul_css::LayoutRect;
use azul_core::{
    ui_solver::{PositionedRectangle, ResolvedTextLayoutOptions, InlineTextLayout},
    id_tree::{NodeHierarchy, NodeDataContainer},
    dom::NodeId,
};
use style::Style;

mod algo;
mod number;
mod geometry;

pub mod style;
pub use geometry::{Size, Offsets};
pub use number::Number;

pub trait GetStyle { fn get_style(&self) -> Style; }
pub trait GetTextLayout { fn get_text_layout(&mut self, text_layout_options: &ResolvedTextLayoutOptions) -> InlineTextLayout; }

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
        mut rect_contents: BTreeMap<NodeId, RectContent<U>>,
    ) -> Self {

        let styles = display_rects.transform(|node, node_id| {

            let image_aspect_ratio = match rect_contents.get(&node_id) {
                Some(RectContent::Image(w, h)) => Number::Defined(*w as f32 / *h as f32),
                _ => Number::Undefined,
            };

            let mut style = node.get_style();
            style.aspect_ratio = image_aspect_ratio;
            style
        });

        let mut solved_rects = algo::compute(NodeId::ZERO, node_hierarchy, &styles, &mut rect_contents, bounds.size);

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
