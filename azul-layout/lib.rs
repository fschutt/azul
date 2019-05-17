extern crate azul_core;
extern crate azul_css;

use std::collections::BTreeMap;
use azul_css::LayoutRect;
use azul_core::{
    ui_solver::{PositionedRectangle, TextLayoutOptions},
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
pub trait GetTextLayout { fn get_text_layout(&mut self, text_layout_options: TextLayoutOptions) -> InlineTextLayout; }

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct InlineTextLayout {
    pub lines: Vec<LayoutRect>,
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

impl SolvedUi {
    pub fn new<T: GetStyle, U: GetTextLayout>(
        bounds: LayoutRect,
        node_hierarchy: &NodeHierarchy,
        display_rects: &NodeDataContainer<T>,
        rect_contents: &mut BTreeMap<NodeId, RectContent<U>>,
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

        let mut solved_rects = algo::compute(NodeId::ZERO, node_hierarchy, &styles, rect_contents, bounds.size);

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
