#![allow(unused_variables, dead_code)]

use std::ops::{Mul, Add, Sub};
use webrender::api::RenderApi;
use rusttype::{Font, Scale};
use azul_css::{
    StyleTextAlignmentHorz, StyleFontSize, ScrollbarInfo,
    StyleTextAlignmentVert, StyleLineHeight, LayoutOverflow,
};
pub use webrender::api::{
    GlyphInstance, GlyphDimensions, FontKey, FontInstanceKey,
    LayoutSize, LayoutRect, LayoutPoint,
};
/// Font size in point (pt)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct TextSizePt(pub f32);
/// Font size in pixel (px) - 1 / (72 * 96)th of a pixel
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct TextSizePx(pub f32);

pub type WordIndex = usize;
pub type GlyphIndex = usize;
pub type LineLength = f32;
pub type IndexOfLineBreak = usize;
pub type RemainingSpaceToRight = f32;

const DEFAULT_LINE_HEIGHT: f32 = 1.0;
const DEFAULT_CHAR_SPACING: f32 = 0.0;
const DEFAULT_LETTER_SPACING: f32 = 0.0;
const DEFAULT_TAB_WIDTH: f32 = 4.0;

impl Mul<f32> for TextSizePx {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        TextSizePx(self.0 * rhs)
    }
}

impl Add for TextSizePx {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        TextSizePx(self.0 + rhs.0)
    }
}

impl Sub for TextSizePx {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        TextSizePx(self.0 - rhs.0)
    }
}

impl From<TextSizePx> for TextSizePt {
    fn from(original: TextSizePx) -> TextSizePt {
        const PX_TO_PT: f32 = 72.0 / 96.0;
        TextSizePt(original.0 * PX_TO_PT)
    }
}

impl From<TextSizePt> for TextSizePx {
    fn from(original: TextSizePt) -> TextSizePx {
        const PT_TO_PX: f32 = 96.0 / 72.0;
        TextSizePx(original.0 * PT_TO_PX)
    }
}

impl TextSizePx {
    // Note rusttype::Scale is in PX, not PT.
    pub fn to_rusttype_scale(&self) -> Scale {
        Scale::uniform(self.0)
    }
}

/// Text broken up into `Tab`, `Word()`, `Return` characters
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Words {
    pub items: Vec<Word>,
}

/// Either a white-space delimited word, tab or return character
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Word {
    /// Encountered a word (delimited by spaces)
    Word(String),
    // `\t` or `x09`
    Tab,
    /// `\r`, `\n` or `\r\n`, escaped: `\x0D`, `\x0A` or `\x0D\x0A`
    Return,
    /// Space character
    Space,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineEnding {
    /// `\n` line ending
    Unix,
    /// `\r\n` line ending
    Windows,
}

pub struct ScaledWords {
    /// Font used to scale this text
    pub font_key: FontKey,
    /// Font instance used to scale this text
    pub font_instance_key: FontInstanceKey,
    /// Words scaled to their appropriate font size, but not yet positioned
    /// on the screen
    pub items: Vec<ScaledWord>,
    /// Longest word in the `self.scaled_words`, necessary for
    /// calculating overflow rectangles.
    pub longest_word_width: f32,
}

/// Word that is scaled (to a font / font instance), but not yet positioned
pub struct ScaledWord {
    /// Glyphs, positions are relative to the first character of the word
    pub glyph_instances: Vec<GlyphInstance>,
    /// Horizontal advances of each glyph, necessary for
    /// hit-testing characters later on (for text selection).
    pub glyph_dimensions: Vec<GlyphDimensions>,
    /// The sum of the width of all the characters in this word
    pub word_width: f32,
}

/// Stores the positions of the vertically laid out texts
pub(crate) struct WordPositions {
    /// Font used to scale this text
    pub font_key: FontKey,
    /// Font instance used to scale this text
    pub font_instance_key: FontInstanceKey,
    /// Options like word spacing, character spacing, etc. that were
    /// used to layout these glyphs
    pub text_layout_options: TextLayoutOptions,
    /// Horizontal offset of the last line, necessary for inline layout later on.
    /// Usually, the "trailing" of the current text block is the "leading" of the
    /// next text block, to make it seem like two text runs push into each other.
    pub trailing: f32,
    /// How many
    pub number_of_words: usize,
    pub number_of_lines: usize,
    /// Horizontal and vertical boundaries of the layouted words.
    ///
    /// Note that the vertical extent can be larger than the last words' position,
    /// because of trailing negative glyph advances.
    pub content_size: LayoutSize,
    /// Stores the positions of words.
    pub word_positions: Vec<LayoutPoint>,
    /// Font size that was used to layout this text
    pub font_size: TextSizePx,
    /// Index of the word at which the line breaks + length of line
    /// (useful for text selection + horizontal centering)
    pub line_breaks: Vec<(WordIndex, LineLength)>,
    /// Whether or not the word positions are already accounting for
    /// the scrollbar space
    pub scrollbar_style: ScrollbarStyle,
    /// The overflow value
    pub overflow: LayoutOverflow,
}

#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct ScrollbarStyle {
    /// Vertical scrollbar style, if any
    pub horizontal: Option<ScrollbarInfo>,
    /// Horizontal scrollbar style, if any
    pub vertical: Option<ScrollbarInfo>,
}

/// Layout options that can impact the flow of word positions
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLayoutOptions {
    /// Multiplier for the line height
    pub line_height: Option<StyleLineHeight>,
    /// Additional spacing between glyphs
    pub letter_spacing: Option<TextSizePx>,
    /// Additional spacing between words
    pub word_spacing: Option<TextSizePx>,
    /// How many spaces should a tab character emulate?
    pub tab_width: Option<f32>,
    /// Width that was used to layout these words originally
    /// (whether the text is unbounded or not).
    pub max_horizontal_width: Option<TextSizePx>,
    /// Pixel amount of "leading", into the first line
    pub leading: Option<TextSizePx>,
    /// This is more important for inline text layout where items can punch "holes"
    /// into the text flow, for example an image that floats to the right.
    ///
    /// TODO: Currently unused!
    pub holes: Vec<LayoutRect>,
    /// Horizontal text aligment
    pub horz_alignment: StyleTextAlignmentHorz,
    /// Vertical text aligment
    pub vert_alignment: StyleTextAlignmentVert,
}

/// Given the scale of words + the word positions, lays out the words in a
pub struct LeftAlignedGlyphs<'a> {
    /// Width that was used to layout these glyphs (or None if the text has overflow:visible)
    pub max_horizontal_width: Option<f32>,
    /// Actual glyph instances, copied
    pub glyphs: Vec<&'a GlyphInstance>,
    /// Rectangles of the different lines, necessary for text selection
    /// and hovering over text, etc.
    pub line_rects: &'a Vec<LayoutRect>,
    /// Horizontal and vertical extent of the text
    pub text_bbox: LayoutSize,
}

/// Returns the layouted glyphs
pub struct LayoutedGlyphs {
    pub glyphs: Vec<GlyphInstance>,
}

/// These metrics are important for showing the scrollbars
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum TextOverflow {
    /// Text is overflowing, by how much?
    /// Necessary for determining the size of the scrollbar
    IsOverflowing(TextSizePx),
    /// Text is in bounds, how much space is available until
    /// the edge of the rectangle? Necessary for centering / aligning text vertically.
    InBounds(TextSizePx),
}

/// Splits the text by whitespace into logical units (word, tab, return, whitespace).
pub(crate) fn split_text_into_words(text: &str) -> Words {

    use unicode_normalization::UnicodeNormalization;

    let mut words = Vec::new();
    let mut current_word = String::with_capacity(10);

    // Necessary because we need to handle both \n and \r\n characters
    // If we just look at the characters one-by-one, this wouldn't be possible.
    let normalized_string = text.nfc().collect::<String>();

    let mut line_iterator = normalized_string.lines().peekable();
    while let Some(line) = line_iterator.next() {
        for ch in line.chars() {
            match ch {
                '\t' => {
                    if !current_word.is_empty() {
                        words.push(Word::Word(current_word.clone()));
                        current_word.clear();
                    }
                    words.push(Word::Tab);
                },
                c if c.is_whitespace() => {
                    if !current_word.is_empty() {
                        words.push(Word::Word(current_word.clone()));
                        current_word.clear();
                    }
                    words.push(Word::Space);
                }
                c => {
                    current_word.push(c);
                }
            }
        }

        if !current_word.is_empty() {
            words.push(Word::Word(current_word.clone()));
            current_word.clear();
        }

        // If this is not the last line, push a return
        if line_iterator.peek().is_some() {
            words.push(Word::Return);
        }
    }

    Words {
        items: words,
    }
}

/// Takes a text broken into semantic items and a font instance and
/// scales the font accordingly.
pub(crate) fn words_to_scaled_words(
    words: &Words,
    render_api: &RenderApi,
    font_key: FontKey,
    font_instance_key: FontInstanceKey,
) -> ScaledWords {

    use self::Word::*;

    let mut longest_word_width = 0.0;

    let glyphs = words.items.iter().filter_map(|word| {
        let word = match word {
            Word(w) => w,
            _ => return None,
        };

        // Filter out all invalid indices and dimensions (usually `None` is
        // only returned for spaces, so that case will obviously not happen,
        // since we broke the text by spaces previously)
        let glyph_indices = render_api.get_glyph_indices(font_key, w);
        let glyph_indices = glyph_indices.into_iter().filter_map(|e| e).collect::<Vec<u32>>();

        let glyph_dimensions = render_api.get_glyph_dimensions(font_instance_key, glyph_indices);
        let glyph_dimensions = glyph_dimensions.into_iter().filter_map(|dim| dim).collect::<Vec<GlyphDimensions>>();

        let word_width = get_glyph_width(&glyph_dimensions);
        if word_width > longest_word_width {
            longest_word_width = word_width;
        }

        let mut glyph_instances = Vec::with_capacity(glyph_dimensions.len());
        let mut current_cursor = 0.0;

        for (index, dimensions) in glyph_indices.into_iter().zip(glyph_dimensions.iter()) {
            glyph_instances.push(GlyphInstance {
                index: index,
                point: LayoutPoint::new(current_cursor, 0.0),
            });
            current_cursor += dimensions.advance;
        }

        Some(ScaledWord {
            glyph_instances,
            glyph_dimensions,
            word_width,
        })

    }).collect();

    ScaledWords {
        font_key,
        font_instance_key,
        items: glyphs,
        longest_word_width: longest_word_width,
    }
}

/// Positions the words on the screen (does not layout any glyph positions!),
/// necessary for estimating the width + height of items.
///
/// NOTE: This will also (depending on the `vertical_scrollbar` or `horizontal_scrollbar` parameter)
/// shift the glyphs by the scrollbar position - for example, if there are no scrollbars, i.e.
/// the rectangle is `overflow:visible`, then just pass in `None` to both parameters.
///
/// Currently you have to pass in the font size, because there is currently no way
/// to get the Space character width from a font.
pub(crate) fn position_words(
    words: &Words,
    scaled_words: &ScaledWords,
    text_layout_options: &TextLayoutOptions,
    font_size: TextSizePx,
    overflow: LayoutOverflow,
    scrollbar_style: &ScrollbarStyle,
) -> WordPositions {

    use self::Word::*;

    // TODO: Handle scrollbar / content size adjustment!
    // TODO: How to get the width of the space glyph key?
    // Currently just using the font size as the space width...

    let font_size_px = font_size.0;
    let word_spacing_px = font_size_px + text_layout_options.word_spacing.map(|s| s.0).unwrap_or(DEFAULT_CHAR_SPACING);
    let line_height_px = font_size_px * text_layout_options.line_height.map(|lh| lh.0.get()).unwrap_or(DEFAULT_LINE_HEIGHT);
    let letter_spacing_px = font_size_px * text_layout_options.letter_spacing.map(|ls| ls.0).unwrap_or(DEFAULT_LETTER_SPACING);
    let tab_width_px = font_size_px * text_layout_options.tab_width.unwrap_or(DEFAULT_TAB_WIDTH);

    let mut line_breaks = Vec::new();
    let mut word_positions = Vec::new();

    let mut line_number = 0;
    // Caret (x position) of the current line
    let mut line_caret_x = 0.0;
    let mut current_word_idx = 0;
    let mut longest_line_width = 0.0;

    macro_rules! advance_caret {($line_caret_x:expr) => ({
        let caret_intersection = caret_intersects_with_holes(
            $line_caret_x,
            line_number,
            font_size_px,
            line_height_px,
            &text_layout_options.holes,
            text_layout_options.max_horizontal_width,
        );

        if let LineCaretIntersection::PushCaretOntoNextLine(line_advance, _) = caret_intersection {
            line_breaks.push((current_word_idx, line_caret_x)); // TODO: Is this correct?
        }

        // Correct and advance the line caret position
        advance_caret(
            &mut $line_caret_x,
            &mut line_number,
            caret_intersection,
        );

        if $line_caret_x > longest_line_width {
            longest_line_width = $line_caret_x;
        }
    })}

    advance_caret!(line_caret_x);

    if let Some(leading) = text_layout_options.leading {
        line_caret_x += leading.0;
        advance_caret!(line_caret_x);
    }

    for (word_idx, word) in words.items.iter().enumerate() {
        match word {
            Word(w) => {
                let scaled_word = &scaled_words.items[word_idx];
                let mut new_caret_x = line_caret_x
                    + word_spacing_px
                    + scaled_word.word_width
                    + (scaled_word.glyph_instances.len().saturating_sub(1) as f32 * letter_spacing_px);

                advance_caret!(new_caret_x);
                line_caret_x = new_caret_x;

                current_word_idx = word_idx;
            },
            Return => {
                line_breaks.push((current_word_idx, line_caret_x));
                let new_caret_x = 0.0;
                advance_caret!(new_caret_x);
                line_caret_x = new_caret_x;
            },
            Space => {
                let mut new_caret_x = line_caret_x + word_spacing_px;
                advance_caret!(new_caret_x);
                line_caret_x = new_caret_x;
            },
            Tab => {
                let mut new_caret_x = line_caret_x + word_spacing_px + tab_width_px;
                advance_caret!(new_caret_x);
                line_caret_x = new_caret_x;
            },
        }
    }

    let trailing = line_caret_x;
    let number_of_lines = line_number;
    let number_of_words = current_word_idx;

    let content_size_y = get_line_y_position(line_number, font_size_px, line_height_px);
    let content_size_x = text_layout_options.max_horizontal_width.map(|x| x.0).unwrap_or(longest_line_width);
    let content_size = LayoutSize::new(content_size_y, content_size_y);

    WordPositions {
        font_key: scaled_words.font_key,
        font_instance_key: scaled_words.font_instance_key,
        font_size,
        text_layout_options: text_layout_options.clone(),
        trailing,
        number_of_words,
        number_of_lines,
        content_size,
        word_positions,
        line_breaks,
        overflow,
        scrollbar_style: scrollbar_style.clone(),
    }
}

/// Returns the final glyphs and positions them relative to the `rect_offset`,
/// ready for webrender to display
pub(crate) fn get_layouted_glyphs(
    word_positions: &WordPositions,
    scaled_words: &ScaledWords,
    alignment_horz: StyleTextAlignmentHorz,
    alignment_vert: StyleTextAlignmentVert,
    rect_offset: LayoutPoint,
    bounding_size_height_px: f32,
) -> LayoutedGlyphs {

    let font_size_px = word_positions.font_size.0;
    let letter_spacing_px = font_size_px * word_positions.text_layout_options.letter_spacing.map(|ls| ls.0).unwrap_or(DEFAULT_LETTER_SPACING);

    let glyphs: Vec<GlyphInstance> = scaled_words.items.iter().zip(word_positions.word_positions.iter())
        .flat_map(|(scaled_word, word_position)| {
            scaled_word.glyph_instances
            .iter()
            .cloned()
            .enumerate()
            .map(|(glyph_id, glyph)| {
                // TODO: letter spacing
                glyph.point.x += word_position.x;
                glyph.point.y += word_position.y;
                if glyph_id != 0 {
                    glyph.point.x += letter_spacing_px;
                }
                glyph
            })
        })
        .collect();

    let line_breaks = get_char_indexes(&word_positions, &scaled_words);
    let vertical_overflow = get_vertical_overflow(&word_positions, bounding_size_height_px);

    align_text_horz(&mut glyphs, alignment_horz, &line_breaks);
    align_text_vert(&mut glyphs, alignment_vert, &line_breaks, vertical_overflow);
    add_origin(&mut glyphs, rect_offset.x, rect_offset.y);

    LayoutedGlyphs {
        glyphs: glyphs,
    }
}

/// Given a width, returns the vertical height and width of the text
pub(crate) fn get_positioned_word_bounding_box(word_positions: &WordPositions) -> LayoutSize {
    word_positions.content_size
}

pub(crate) fn get_vertical_overflow(word_positions: &WordPositions, bounding_size_height_px: f32) -> TextOverflow {
    let content_size = word_positions.content_size;
    if bounding_size_height_px > content_size.height {
        TextOverflow::InBounds(TextSizePx(bounding_size_height_px - content_size.height))
    } else {
        TextOverflow::IsOverflowing(TextSizePx(content_size.height - bounding_size_height_px))
    }
}

/// Combines the `words` back to a string, either using Unix or Windows line endings
pub fn words_to_string(words: &Words, line_ending: LineEnding) -> String {

    use self::LineEnding::*;
    use self::Word::*;

    if words.items.is_empty {
        return String::new();
    }

    let line_ending = match line_ending {
        Unix => "\n",
        Windows => "\r\n",
    };

    let mut string = String::with_capacity(words.len() * words[0].len());

    for w in words {
        match w {
            Word(w) => { string.push_str(w); },
            Tab => { string.push('\t'); },
            Return => { string.push_str(line_ending); },
            Space => { string.push(' '); },
        }
    }

    string
}

fn word_item_is_return(item: &Word) -> bool {
    *item == Word::Return
}

fn text_overflow_is_overflowing(overflow: &TextOverflow) -> bool {
    use self::TextOverflow::*;
    match overflow {
        IsOverflowing(_) => true,
        InBounds(_) => false,
    }
}

fn get_char_indexes(word_positions: &WordPositions, scaled_words: &ScaledWords)
-> Vec<(GlyphIndex, RemainingSpaceToRight)>
{
    let width = word_positions.content_size.width;
    let mut char_indices = Vec::new();

    if scaled_words.items.is_empty() {
        return char_indices;
    }

    let mut current_glyph_count = 0;
    let mut last_word_idx = 0;

    word_positions.line_breaks.iter().map(|(current_word_idx, line_length)| {
        let remaining_space_px = width - line_length;
        let words = &scaled_words.items[last_word_idx..*current_word_idx];
        let glyphs_in_this_line = words.iter().map(|s| s.glyph_instances.len()).sum();
        current_glyph_count += glyphs_in_this_line;
        last_word_idx = *current_word_idx;
        (current_glyph_count, remaining_space_px)
    }).collect()
}

fn get_glyph_width(glyph_dimensions: &[GlyphDimensions]) -> f32 {
    glyph_dimensions.iter().map(|g| g.advance).sum()
}

/// For a given line number, calculates the Y position of the word
fn get_line_y_position(line_number: usize, font_size_px: f32, line_height_px: f32) -> f32 {
    ((font_size_px + line_height_px) * line_number as f32) + font_size_px
}

enum LineCaretIntersection {
    /// OK: Caret does not interset any elements
    NoIntersection,
    /// In order to not intersect with any holes, the caret needs to
    /// be advanced to the position x, but can stay on the same line.
    AdvanceCaretTo(f32),
    /// Caret needs to advance X number of lines and be positioned
    /// with a leading of x
    PushCaretOntoNextLine(usize, f32),
}

/// Check if the caret intersects with any holes and if yes,
/// what should be done about that.
///
/// NOTE: `holes` need to be sorted by `y` origin
fn caret_intersects_with_holes(
    line_caret_x: f32,
    line_number: usize,
    font_size_px: f32,
    line_height_px: f32,
    holes: &[LayoutRect],
    max_width: Option<TextSizePx>,
) -> LineCaretIntersection {

    let mut new_line_caret_x = None;
    let mut line_advance = 0;
    let mut caret_has_moved = false;

    // If the caret is outside of the max_width, move it to the start of a new line
    if let Some(max_width) = max_width {
        if line_caret_x > max_width.0 {
            new_line_caret_x = Some(0.0);
            line_advance += 1;
            caret_has_moved = true;
        }
    }

    for hole in holes {

        let mut should_move_caret = false;
        let mut current_line_advance = 0;
        let mut new_line_number = line_number + current_line_advance;
        let mut current_caret = LayoutPoint::new(
            new_line_caret_x.unwrap_or(line_caret_x),
            get_line_y_position(new_line_number, font_size_px, line_height_px)
        );

        // NOTE: holes need to be sorted by Y origin (from smallest to largest Y),
        // and be sorted from left to right
        while hole.contains(&current_caret) {
            should_move_caret = true;
            if let Some(max_width) = max_width {
                if hole.origin.x + hole.size.width > max_width.0 {
                    // Need to break the line here
                    current_line_advance += 1;
                    new_line_number = line_number + current_line_advance;
                    current_caret = LayoutPoint::new(
                        new_line_caret_x.unwrap_or(line_caret_x),
                        get_line_y_position(new_line_number, font_size_px, line_height_px)
                    );
                } else {
                    new_line_number = line_number + current_line_advance;
                    current_caret = LayoutPoint::new(
                        hole.origin.x + hole.size.width,
                        get_line_y_position(new_line_number, font_size_px, line_height_px)
                    );
                }
            } else {
                // No max width, so no need to break the line, move the caret to the right side of the hole
                new_line_number = line_number + current_line_advance;
                current_caret = LayoutPoint::new(
                    hole.origin.x + hole.size.width,
                    get_line_y_position(new_line_number, font_size_px, line_height_px)
                );
            }
        }

        if should_move_caret {
            new_line_caret_x = Some(current_caret.x);
            line_advance += current_line_advance;
        }
    }

    if let Some(new_line_caret_x) = new_line_caret_x {
        if line_advance == 0 {
            LineCaretIntersection::AdvanceCaretTo(new_line_caret_x)
        } else {
            LineCaretIntersection::PushCaretOntoNextLine(line_advance, new_line_caret_x)
        }
    } else {
        LineCaretIntersection::NoIntersection
    }
}

fn advance_caret(caret: &mut f32, line_number: &mut usize, intersection: LineCaretIntersection) {
    use self::LineCaretIntersection::*;
    match intersection {
        NoIntersection => { },
        AdvanceCaretTo(x) => { *caret = x; },
        PushCaretOntoNextLine(num_lines, x) => { *line_number += num_lines; *caret = x; },
    }
}

fn align_text_horz(
    glyphs: &mut [GlyphInstance],
    alignment: StyleTextAlignmentHorz,
    line_breaks: &[(usize, f32)]
) {
    use azul_css::StyleTextAlignmentHorz::*;

    // Text alignment is theoretically very simple:
    //
    // If we have a bunch of text, such as this (the `glyphs`):

    // ^^^^^^^^^^^^
    // ^^^^^^^^
    // ^^^^^^^^^^^^^^^^
    // ^^^^^^^^^^

    // and we have information about how much space each line has to the right:
    // (the "---" is the space)

    // ^^^^^^^^^^^^----
    // ^^^^^^^^--------
    // ^^^^^^^^^^^^^^^^
    // ^^^^^^^^^^------

    // Then we can center-align the text, by just taking the "-----", dividing
    // it by 2 and moving all characters to the right:

    // --^^^^^^^^^^^^--
    // ----^^^^^^^^----
    // ^^^^^^^^^^^^^^^^
    // ---^^^^^^^^^^---

    // Same for right-aligned text, but without the "divide by 2 step"

    if line_breaks.is_empty() || glyphs.is_empty() {
        return; // ??? maybe a 0-height rectangle?
    }

    // assert that the last info in the line_breaks vec has the same glyph index
    // i.e. the last line has to end with the last glyph
    assert!(glyphs.len() - 1 == line_breaks[line_breaks.len() - 1].0);

    let multiply_factor = match alignment {
        Left => return,
        Center => 0.5, // move the line by the half width
        Right => 1.0, // move the line by the full width
    };

    // If we have the characters "ABC\n\nDEF", this will result in:
    //
    //     [ Glyph(A), Glyph(B), Glyph(C), Glyph(D), Glyph(E), Glyph(F)]
    //
    //     [LineBreak(2), LineBreak(2), LineBreak(5)]
    //
    // If we'd just shift every character after the line break, we'd get into
    // the problem of shifting the 3rd character twice, because of the \n\n.
    //
    // To avoid the double-line-break problem, we can use ranges:
    //
    // - from 0..=2, shift the characters at i by X amount
    // - from 3..3 (e.g. 0 characters) shift the characters at i by X amount
    // - from 3..=5 shift the characters by X amount
    //
    // Because the middle range selects 0 characters, the shift is effectively
    // ignored, which is what we want - because there are no characters to shift.

    let mut start_range_char = 0;

    for (line_break_char, line_break_amount) in line_breaks {

        // NOTE: Inclusive range - beware: off-by-one-errors!
        for glyph in &mut glyphs[start_range_char..=*line_break_char] {
            let old_glyph_x = glyph.point.x;
            glyph.point.x += line_break_amount * multiply_factor;
        }
        start_range_char = *line_break_char + 1; // NOTE: beware off-by-one error - note the +1!
    }
}

fn align_text_vert(
    glyphs: &mut [GlyphInstance],
    alignment: StyleTextAlignmentVert,
    line_breaks: &[(usize, f32)],
    vertical_overflow: TextOverflow,
){
    use self::TextOverflow::*;
    use self::StyleTextAlignmentVert::*;

    if line_breaks.is_empty() || glyphs.is_empty() {
        return;
    }

    // Die if we have a line break at a position bigger than the position of the last glyph,
    // because something went horribly wrong!
    //
    // The next unwrap is always safe as line_breaks will have a minimum of one entry!
    assert!(glyphs.len() - 1 == line_breaks.last().unwrap().0);

    let multiply_factor = match alignment {
        Top => return,
        Center => 0.5,
        Bottom => 1.0,
    };

    let space_to_add = match vertical_overflow {
        IsOverflowing(_) => return,
        InBounds(remaining_space_px) => {
            // Total text height (including last leading!)
            // All metrics in pixels
            (remaining_space_px * multiply_factor)
        },
    };

    glyphs.iter_mut().for_each(|g| g.point.y += space_to_add.0);
}

/// Adds the X and Y offset to each glyph in the positioned glyph
fn add_origin(positioned_glyphs: &mut [GlyphInstance], x: f32, y: f32) {
    for c in positioned_glyphs {
        c.point.x += x;
        c.point.y += y;
    }
}

// -------------------------- PUBLIC API -------------------------- //

/// Temporary struct that contains various metrics related to a font -
/// useful so we don't have to access the font to look up certain widths
#[derive(Debug, Clone)]
pub struct FontMetrics {
    /// Width of the space character
    pub space_width: TextSizePx,
    /// Usually 4 * space_width
    pub tab_width: TextSizePx,
    /// font_size * line_height
    pub vertical_advance: TextSizePx,
    /// Font size, including line height
    pub font_size_with_line_height: TextSizePx,
    /// Same as `font_size_with_line_height` but without the
    /// `self.line height` incorporated. Used for horizontal layouting
    pub font_size_no_line_height: TextSizePx,
    /// Some fonts have a base height of 2048 or something weird like that
    pub height_for_1px: f32,
    /// Spacing of the letters, or 0.0 by default
    pub letter_spacing: Option<TextSizePx>,
    /// Slightly duplicated: The layout options for the text
    pub layout_options: TextLayoutOptions,
}

impl FontMetrics {
    /// Given a font, font size and line height, calculates the `FontMetrics` necessary
    /// which are later used to layout a block of text
    pub fn new<'a>(font: &Font<'a>, font_size: &StyleFontSize, layout_options: &TextLayoutOptions) -> Self {
        let font_size_no_line_height = TextSizePx(font_size.0.to_pixels());
        let line_height = layout_options.line_height.and_then(|lh| Some(lh.0.get())).unwrap_or(1.0);
        let font_size_with_line_height = font_size_no_line_height * line_height;

        let space_glyph = font.glyph(' ').scaled(font_size_no_line_height.to_rusttype_scale());
        let height_for_1px = font.glyph(' ').standalone().get_data().unwrap().scale_for_1_pixel;
        let space_width = TextSizePx(space_glyph.h_metrics().advance_width);
        let tab_width = space_width * 4.0; // TODO: make this configurable

        let v_metrics_scaled = font.v_metrics(font_size_with_line_height.to_rusttype_scale());
        let v_advance_scaled = TextSizePx(v_metrics_scaled.ascent - v_metrics_scaled.descent + v_metrics_scaled.line_gap);

        FontMetrics {
            vertical_advance: v_advance_scaled,
            space_width,
            tab_width,
            height_for_1px,
            font_size_with_line_height,
            font_size_no_line_height,
            letter_spacing: layout_options.letter_spacing,
            layout_options: *layout_options,
        }
    }

    /// Necessary to un-do the scaling done by the text layout
    pub fn get_svg_font_scale_factor(&self) -> f32 {
        self.font_size_no_line_height.0 * self.height_for_1px
    }
}

/// Returned result from the `layout_text` function
#[derive(Debug, Clone)]
pub struct LayoutTextResult {
    /// The words, broken up by whitespace
    pub words: Words,
    pub scaled_words: ScaledWords,
    /// The line_breaks contain:
    ///
    /// - The index of the glyph at which the line breaks (index into the `self.layouted_glyphs`)
    /// - How much space each line has (to the right edge of the containing rectangle)
    pub line_breaks: Vec<(IndexOfLineBreak, RemainingSpaceToRight)>,
    /// Minimal width of the layouted text
    pub min_width: TextSizePx,
    /// Minimal height of the layouted text
    pub min_height: TextSizePx,
    pub font_metrics: FontMetrics,
}

impl LayoutTextResult {

    /// Returns the index of what character was hit by the x and y coordinates.
    ///
    /// The x and y values have to be relative to the top left corner of the
    /// LayoutTextResult and must be axis aligned (i.e no rotation or anything
    /// is calculated, assumed that the text is a regular, horizontal text string
    /// without any rotation).
    pub fn get_hit_glyph_idx(&self, x: f32, y: f32) -> Option<usize> {
        use webrender::api::{LayoutSize, LayoutRect, LayoutPoint};
        let font_size_no_line_height = self.font_metrics.font_size_no_line_height;

        // NOTE: This is shit, will not fire for the last character of
        // the string
        self.layouted_glyphs.iter().zip(self.layouted_glyphs.iter().skip(1))
        .position(|(glyph, next_glyph)| {
            let x_diff = next_glyph.point - glyph.point;
            let rect = LayoutRect::new(
                glyph.point,
                LayoutSize::new(x_diff.x, font_size_no_line_height.0)
            );
            rect.contains(&LayoutPoint::new(x, y))
        })
    }
}

/// Layout a string of text horizontally, given a font with its metrics.
pub fn layout_text<'a>(
    text: &str,
    font: &Font<'a>,
    font_metrics: &FontMetrics
) -> LayoutTextResult {

    // NOTE: This function is different from the get_glyphs function that is
    // used internally to Azul.
    //
    // This function simply lays out a text, without trying to fit it into a rectangle.
    // This function does not calculate any overflow.
    let words = split_text_into_words(text);
    let scaled_words = words_to_scaled_words(&words);
    let words = split_text_into_words(text, font, font_metrics.font_size_no_line_height, font_metrics.letter_spacing);
    let (mut layouted_glyphs, line_breaks, min_width, min_height) =
        words_to_left_aligned_glyphs(&words, font, None, font_metrics);

    align_text_horz(font_metrics.layout_options.horz_alignment, &mut layouted_glyphs, &line_breaks);

    LayoutTextResult {
        words, layouted_glyphs, line_breaks, min_width, min_height, font_metrics: *font_metrics,
    }
}
