extern crate azul_core;
extern crate azul_css;

pub mod algo;

pub type NodeDepths = Vec<(usize, NodeId)>;

pub trait GetRectStyle {
    fn get_rect_style(&self) -> &RectStyle;
}

pub trait GetRectLayout {
    fn get_rect_layout(&self) -> &RectLayout;
}

/// Trait that is implemented for a type that returns the text positions
pub trait GetTextLayout {
    fn get_text_layout(&mut self, bounds: LayoutRect) -> LayoutedInlineText;
}

// Impl GetTextLayout for fn create_word_positions()

pub struct LayoutedInlineText {
    pub lines: Vec<LayoutedInlineTextLine>,
    pub layout_direction: InlineTextDirection,
}

impl LayoutedInlineText {
    pub fn get_bounds(&self) -> LayoutRect {
        match self.layout_direction {
            LeftToRight => {

            },
            RightToLeft => {

            },
            TopToBottom => {

            },
            BottomToTop => {

            },
        }
    }
}

pub enum InlineTextDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

// Single line of text inside an inline text
pub struct LayoutedInlineTextLine {
    pub words: Vec<LayoutedInlineWord>,
}

impl LayoutedInlineTextLine {
    pub fn get_bounds(&self) -> LayoutRect {
        let width = self.words
    }
}

pub struct LayoutedInlineWord {
    /// Laid out glyph clusters + index of the glyph cluster
    pub glyph_clusters: Vec<(LayoutRect, usize)>,
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

pub fn solve_ui<T: GetRectStyle + GetRectLayout, U: GetTextLayout>(
    rect: LayoutRect,
    node_hierarchy: &NodeHierarchy,
    display_rects: &NodeDataContainer<T>,
    inline_texts: BTreeMap<NodeId, RectContent<U>>,
) -> SolvedUi {

}

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