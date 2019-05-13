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
    id_tree::{NodeHierarchy, NodeDataContainer}
    dom::NodeId,
};

pub type NodeDepths = Vec<(usize, NodeId)>;

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
    pub node_depths: NodeDepths,
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
        LayoutRect::union(&self.lines).unwrap_or(LayoutRect::zero())
    }
}

impl SolvedUi {
    pub fn new<T: GetRectStyle + GetRectLayout, U: GetTextLayout>(
        bounds: LayoutRect,
        node_hierarchy: &NodeHierarchy,
        display_rects: &NodeDataContainer<T>,
        inline_texts: BTreeMap<NodeId, RectContent<U>>,
    ) -> Self {

        /*
            pub enum Dimension {
                Undefined,
                Auto,
                Points(f32),
                Percent(f32),
            }

            #[derive(Copy, Clone, Debug)]
            pub struct Style {
                pub display: Display,
                pub position_type: PositionType,
                pub direction: Direction,
                pub flex_direction: FlexDirection,
                pub flex_wrap: FlexWrap,
                pub overflow: Overflow,
                pub align_items: AlignItems,
                pub align_self: AlignSelf,
                pub align_content: AlignContent,
                pub justify_content: JustifyContent,
                pub position: Rect<Dimension>,
                pub margin: Rect<Dimension>,
                pub padding: Rect<Dimension>,
                pub border: Rect<Dimension>,
                pub flex_grow: f32,
                pub flex_shrink: f32,
                pub flex_basis: Dimension,
                pub size: Size<Dimension>,
                pub min_size: Size<Dimension>,
                pub max_size: Size<Dimension>,
                pub aspect_ratio: Number,
            }
        */

        // 1.
        /*
            pub struct PositionedRectangle {
                /// Outer bounds of the rectangle
                pub bounds: LayoutRect,
                /// Size of the content, for example if a div contains an image or text,
                /// that image or the text block can be bigger than the actual rect
                pub content_size: Option<LayoutSize>,
            }
        */
        // SolvedUi {
        //     solved_rects: NodeDataContainer<PositionedRectangle>,
        //     node_depths: NodeDepths,
        // }

        /*
            fn solve(
                rect: LayoutRect,
                node_hierarchy: &NodeHierarchy,
                display_rects: &NodeDataContainer<DisplayRectangle<'a>>,
                node_data: &NodeDataContainer<NodeData<T>>, // <- only needed for words
                app_resources: &'b AppResources,            // <- only needed for words
            ) -> LayoutResult {
                rects: NodeDataContainer<PositionedRectangle>,
                node_depths: Vec<(usize, NodeId)>,

                // --- word cache

                word_cache: BTreeMap<NodeId, Words>,
                scaled_words: BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
                positioned_word_cache: BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
                layouted_glyph_cache: BTreeMap<NodeId, LayoutedGlyphs>,
            }
        */
    }
}
