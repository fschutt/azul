extern crate azul_core;
extern crate azul_css;

mod algo;
mod number;
mod geometry;
mod style;

use std::collections::BTreeMap;
use azul_css::{LayoutRect, RectLayout, RectStyle};
use azul_core::{
    ui_solver::PositionedRectangle,
    id_tree::{NodeHierarchy, NodeDataContainer},
    dom::NodeId,
};

pub trait GetRectStyle { fn get_rect_style(&self) -> &RectStyle; }
pub trait GetRectLayout { fn get_rect_layout(&self) -> &RectLayout; }
pub trait GetTextLayout { fn get_text_layout(&mut self, bounds: LayoutRect) -> LayoutedInlineText; }

pub struct LayoutedInlineText {
    pub lines: Vec<LayoutRect>,
    pub layout_direction: InlineTextDirection,
}

pub enum InlineTextDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

pub struct SolvedUi {
    pub solved_rects: NodeDataContainer<PositionedRectangle>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RectContent<T: GetTextLayout> {
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
    pub fn new<T: GetRectStyle + GetRectLayout, U: GetTextLayout>(
        bounds: LayoutRect,
        node_hierarchy: &NodeHierarchy,
        display_rects: &NodeDataContainer<T>,
        rect_contents: BTreeMap<NodeId, RectContent<U>>,
    ) -> Self {

        use style::Style;

        let styles = display_rects.transform(|node, node_id| {
            Style {
                display: Display,
                position_type: PositionType,
                direction: Direction,
                flex_direction: FlexDirection,
                flex_wrap: FlexWrap,
                overflow: Overflow,
                align_items: AlignItems,
                align_self: AlignSelf,
                align_content: AlignContent,
                justify_content: JustifyContent,
                position: Rect<Dimension>,
                margin: Rect<Dimension>,
                padding: Rect<Dimension>,
                border: Rect<Dimension>,
                flex_grow: f32,
                flex_shrink: f32,
                flex_basis: Dimension,
                size: Size { width: Dimension, height: Dimension },
                min_size: Size<Dimension>,
                max_size: Size<Dimension>,
                aspect_ratio: Number,
            }
            /*
                match rect_contents.get(node_id) {
                    Some(RectContent::Image(w, h)) => { },
                    Some(RectContent(text_impl)) => { text_impl.get_text_layout().get_bounds(); },
                    None => { },
                }
            */
        });

        // TODO: Actually solve the rects
        let solved_rects = display_rects.transform(|node, node_id| PositionedRectangle {

        });

        SolvedUi { solved_rects }

        /*
            pub enum Dimension {
                Undefined,
                Auto,
                Points(f32),
                Percent(f32),
            }
        */

        // 1. do layout pass without any text, only images
        // 2. for each display: inline
        /*
            pub struct PositionedRectangle {
                /// Outer bounds of the rectangle
                pub bounds: LayoutRect,
                /// Size of the content, for example if a div contains an image or text,
                /// that image or the text block can be bigger than the actual rect
                pub content_size: Option<LayoutSize>,
            }
        // SolvedUi {
        //     solved_rects: NodeDataContainer<PositionedRectangle>,
        //     node_depths: NodeDepths,
        // }

        */
    }
}
