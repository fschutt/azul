use std::collections::BTreeMap;
use azul_css::{
    LayoutRect, PixelValue, LayoutSize, StyleFontSize,
    StyleTextColor, ColorU as StyleColorU, Overflow,
};
use {
    app_resources::{Words, ScaledWords, FontInstanceKey, WordPositions, LayoutedGlyphs},
    id_tree::{NodeId, NodeDataContainer},
    dom::{DomHash, ScrollTagId},
    callbacks::PipelineId,
};

pub const DEFAULT_FONT_SIZE_PX: isize = 16;
pub const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize(PixelValue::const_px(DEFAULT_FONT_SIZE_PX));
pub const DEFAULT_FONT_ID: &str = "serif";
pub const DEFAULT_FONT_COLOR: StyleTextColor = StyleTextColor(StyleColorU { r: 0, b: 0, g: 0, a: 255 });
pub const DEFAULT_LINE_HEIGHT: f32 = 1.0;
pub const DEFAULT_WORD_SPACING: f32 = 1.0;
pub const DEFAULT_LETTER_SPACING: f32 = 0.0;
pub const DEFAULT_TAB_WIDTH: f32 = 4.0;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct InlineTextLayout {
    pub lines: Vec<LayoutRect>,
}

impl InlineTextLayout {
    #[inline]
    pub fn get_bounds(&self) -> LayoutRect {
        LayoutRect::union(self.lines.iter().map(|c| *c)).unwrap_or(LayoutRect::zero())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
#[repr(C)]
pub struct ExternalScrollId(pub u64, pub PipelineId);

#[derive(Default, Debug, Clone)]
pub struct ScrolledNodes {
    pub overflowing_nodes: BTreeMap<NodeId, OverflowingScrollNode>,
    pub tags_to_node_ids: BTreeMap<ScrollTagId, NodeId>,
}

#[derive(Debug, Clone)]
pub struct OverflowingScrollNode {
    pub child_rect: LayoutRect,
    pub parent_external_scroll_id: ExternalScrollId,
    pub parent_dom_hash: DomHash,
    pub scroll_tag_id: ScrollTagId,
}

#[derive(Debug, Default, Clone)]
pub struct LayoutResult {
    pub rects: NodeDataContainer<PositionedRectangle>,
    pub word_cache: BTreeMap<NodeId, Words>,
    pub scaled_words: BTreeMap<NodeId, (ScaledWords, FontInstanceKey)>,
    pub positioned_word_cache: BTreeMap<NodeId, (WordPositions, FontInstanceKey)>,
    pub layouted_glyph_cache: BTreeMap<NodeId, LayoutedGlyphs>,
    pub node_depths: Vec<(usize, NodeId)>,
}

/// Layout options that can impact the flow of word positions
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct TextLayoutOptions {
    /// Font size (in pixels) that this text has been laid out with
    pub font_size_px: PixelValue,
    /// Multiplier for the line height, default to 1.0
    pub line_height: Option<f32>,
    /// Additional spacing between glyphs (in pixels)
    pub letter_spacing: Option<PixelValue>,
    /// Additional spacing between words (in pixels)
    pub word_spacing: Option<PixelValue>,
    /// How many spaces should a tab character emulate
    /// (multiplying value, i.e. `4.0` = one tab = 4 spaces)?
    pub tab_width: Option<f32>,
    /// Maximum width of the text (in pixels) - if the text is set to `overflow:visible`, set this to None.
    pub max_horizontal_width: Option<f32>,
    /// How many pixels of leading does the first line have? Note that this added onto to the holes,
    /// so for effects like `:first-letter`, use a hole instead of a leading.
    pub leading: Option<f32>,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: Vec<LayoutRect>,
}

/// Same as `TextLayoutOptions`, but with the widths / heights of the `PixelValue`s
/// resolved to regular f32s (because `letter_spacing`, `word_spacing`, etc. may be %-based value)
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct ResolvedTextLayoutOptions {
    /// Font size (in pixels) that this text has been laid out with
    pub font_size_px: f32,
    /// Multiplier for the line height, default to 1.0
    pub line_height: Option<f32>,
    /// Additional spacing between glyphs (in pixels)
    pub letter_spacing: Option<f32>,
    /// Additional spacing between words (in pixels)
    pub word_spacing: Option<f32>,
    /// How many spaces should a tab character emulate
    /// (multiplying value, i.e. `4.0` = one tab = 4 spaces)?
    pub tab_width: Option<f32>,
    /// Maximum width of the text (in pixels) - if the text is set to `overflow:visible`, set this to None.
    pub max_horizontal_width: Option<f32>,
    /// How many pixels of leading does the first line have? Note that this added onto to the holes,
    /// so for effects like `:first-letter`, use a hole instead of a leading.
    pub leading: Option<f32>,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: Vec<LayoutRect>,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    pub const fn zero() -> Self { Self { top: 0.0, left: 0.0, right: 0.0, bottom: 0.0 } }
    pub fn total_vertical(&self) -> f32 { self.top + self.bottom }
    pub fn total_horizontal(&self) -> f32 { self.left + self.right }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct PositionedRectangle {
    /// Outer bounds of the rectangle
    pub bounds: LayoutRect,
    /// Padding of the rectangle
    pub padding: ResolvedOffsets,
    /// Margin of the rectangle
    pub margin: ResolvedOffsets,
    /// Border widths of the rectangle
    pub border_widths: ResolvedOffsets,
    /// Size of the content, for example if a div contains an image or text,
    /// that image or the text block can be bigger than the actual rect
    pub content_size: Option<LayoutSize>,
    /// If this is an inline rectangle, resolve the %-based font sizes
    /// and store them here.
    pub resolved_text_layout_options: Option<(ResolvedTextLayoutOptions, InlineTextLayout, LayoutRect)>,
    /// Determines if the rect should be clipped or not (TODO: x / y as separate fields!)
    pub overflow: Overflow,
}
