//! Text layout with float integration
//!
//! This module extends the basic text layout to support text flowing around floated elements

use std::collections::BTreeMap;

use azul_core::{
    app_resources::{ShapedTextBufferUnsized, ShapedWords, WordPosition, Words},
    ui_solver::{
        InlineTextLayout, InlineTextLayoutRustInternal, InlineTextLine, ResolvedTextLayoutOptions,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::{
    props::{
        basic::{FontMetrics as CssFontMetrics, FontRef as CssFontRef},
        style::StyleTextAlign,
    },
    LayoutDebugMessage,
};

pub mod layout;
pub mod script;
pub mod shaping;

use azul_core::app_resources::{ExclusionSide, TextExclusionArea};

use self::layout::{position_words, word_positions_to_inline_text_layout};
use crate::{
    parsedfont::ParsedFont,
    solver2::layout::{adjust_rect_for_floats, get_relevant_floats},
};

/// Data structure representing padding for text layout
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayoutOffsets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl TextLayoutOffsets {
    pub fn zero() -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }
}

/// Trait for font implementations that can be used for text shaping and layout.
/// This abstraction allows for mocking fonts during testing.
pub trait FontImpl {
    /// Returns the width of the space character, if available
    fn get_space_width(&self) -> Option<usize>;

    /// Returns the horizontal advance of a glyph
    fn get_horizontal_advance(&self, glyph_index: u16) -> u16;

    /// Returns the size (width, height) of a glyph, if available
    fn get_glyph_size(&self, glyph_index: u16) -> Option<(i32, i32)>;

    /// Shapes text using the font
    fn shape(&self, text: &[u32], script: u32, lang: Option<u32>) -> ShapedTextBufferUnsized;

    /// Looks up a glyph index from a Unicode codepoint
    fn lookup_glyph_index(&self, c: u32) -> Option<u16>;

    /// Returns a reference to the font metrics
    fn get_font_metrics(&self) -> &azul_css::props::basic::FontMetrics;
}

/// Layout text with exclusion areas for floats
pub fn layout_text_with_floats(
    words: &Words,
    shaped_words: &ShapedWords,
    text_layout_options: &ResolvedTextLayoutOptions,
    exclusion_areas: &[TextExclusionArea],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> InlineTextLayout {
    // If no exclusion areas, use standard text layout
    if exclusion_areas.is_empty() {
        let word_positions =
            position_words(words, shaped_words, text_layout_options, debug_messages);
        let mut inline_text_layout = word_positions_to_inline_text_layout(&word_positions);

        // Apply text alignment if needed
        if let Some(text_align) = text_layout_options.text_justify.into_option() {
            if text_align != StyleTextAlign::Left {
                if let Some(max_width) = text_layout_options.max_horizontal_width.into_option() {
                    let parent_size = LogicalSize::new(max_width, 0.0); // Height doesn't matter for horizontal alignment
                    inline_text_layout.align_children_horizontal(&parent_size, text_align);
                }
            }
        }

        return inline_text_layout;
    }

    // Create modified layout options with holes for the exclusion areas
    let mut modified_options = text_layout_options.clone();
    modified_options.holes = exclusion_areas
        .iter()
        .map(|area| area.rect)
        .collect::<Vec<_>>()
        .into();

    // Perform text layout with the modified options
    let word_positions = position_words(words, shaped_words, &modified_options, debug_messages);

    // Create line boxes
    let mut line_boxes = Vec::new();

    // Adjust line boxes for exclusion areas
    for line in word_positions.line_breaks.as_slice() {
        let mut adjusted_line = line.clone();

        // Find exclusions that intersect with this line
        let line_y = line.bounds.origin.y;
        let line_height = line.bounds.size.height;

        let relevant_floats = get_relevant_floats(exclusion_areas, (line_y, line_y + line_height));

        if !relevant_floats.is_empty() {
            // Adjust line width based on exclusions
            adjusted_line.bounds =
                adjust_rect_for_floats(adjusted_line.bounds, &relevant_floats, debug_messages);
        }

        line_boxes.push(adjusted_line);
    }

    // Apply text alignment if needed
    if let Some(text_align) = text_layout_options.text_justify.into_option() {
        if text_align != StyleTextAlign::Left {
            for line in &mut line_boxes {
                // For each line, adjust word positions according to line bounds and alignment
                adjust_line_alignment(
                    line,
                    &word_positions.word_positions,
                    text_align,
                    debug_messages,
                );
            }
        }
    }

    // Create the final inline text layout
    InlineTextLayout {
        lines: line_boxes.into(),
        content_size: word_positions.content_size,
    }
}

/// Adjust the alignment of words within a line
fn adjust_line_alignment(
    line: &mut InlineTextLine,
    word_positions: &[WordPosition],
    text_align: StyleTextAlign,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalRect {
    // Only handle words that are in this line
    let line_words: Vec<&WordPosition> = word_positions
        .iter()
        .skip(line.word_start)
        .take(line.word_end - line.word_start + 1)
        .collect();

    if line_words.is_empty() {
        return line.bounds;
    }

    // Calculate the current line width based on the rightmost word
    let rightmost_word = line_words
        .iter()
        .max_by(|a, b| {
            let a_right = a.position.x + a.size.width;
            let b_right = b.position.x + b.size.width;
            a_right.partial_cmp(&b_right).unwrap()
        })
        .unwrap();

    let line_width = rightmost_word.position.x + rightmost_word.size.width - line.bounds.origin.x;
    let available_width = line.bounds.size.width;

    // Don't adjust if there's no room
    if line_width >= available_width {
        return line.bounds;
    }

    // Calculate the offset based on alignment
    let offset = match text_align {
        StyleTextAlign::Left | StyleTextAlign::Start => 0.0, // No adjustment needed
        StyleTextAlign::Center => (available_width - line_width) / 2.0,
        StyleTextAlign::Right | StyleTextAlign::End => available_width - line_width,
        StyleTextAlign::Justify => {
            // For justify, we'd need to adjust spacing between words
            // This is more complex and would require modifying the WordPositions
            // We'll ignore this for now
            0.0
        }
    };

    if offset > 0.0 {
        // Update the line's horizontal position
        line.bounds.origin.x += offset;
    }

    line.bounds
}

pub fn get_font_metrics_fontref(font_ref: &CssFontRef) -> CssFontMetrics {
    let parsed_font = unsafe { &*(font_ref.get_data().parsed as *const ParsedFont) };
    parsed_font.font_metrics.clone()
}
