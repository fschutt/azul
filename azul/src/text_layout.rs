#![allow(unused_variables, dead_code)]

use std::ops::{Mul, Add, Sub};
use webrender::api::{FontKey, FontInstanceKey};
use azul_css::{
    StyleTextAlignmentHorz, ScrollbarInfo,
    StyleTextAlignmentVert, StyleLineHeight, LayoutOverflow,
};
pub use webrender::api::{
    GlyphInstance, LayoutSize, LayoutRect, LayoutPoint,
};
pub use text_shaping::{GlyphPosition, GlyphInfo};
use app_resources::LoadedFont;
use app_units::Au;

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
const DEFAULT_WORD_SPACING: f32 = 1.0;
const DEFAULT_LETTER_SPACING: f32 = 0.0;
const DEFAULT_TAB_WIDTH: f32 = 4.0;

const PX_TO_PT: f32 = 72.0 / 96.0;
const PT_TO_PX: f32 = 96.0 / 72.0;

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
        TextSizePt(original.0 * PX_TO_PT)
    }
}

impl From<TextSizePt> for TextSizePx {
    fn from(original: TextSizePt) -> TextSizePx {
        TextSizePx(original.0 * PT_TO_PX)
    }
}

/// Text broken up into `Tab`, `Word()`, `Return` characters
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Words {
    pub items: Vec<Word>,
    // NOTE: Can't be a string, because it wouldn't be possible to take substrings
    // (since in UTF-8, multiple characters can be encoded in one byte).
    internal_str: String,
    internal_chars: Vec<char>,
}

impl Words {

    pub fn get_substr(&self, word: &Word) -> String {
        self.internal_chars[word.start..word.end].iter().collect()
    }

    pub fn get_str(&self) -> &str {
        &self.internal_str
    }

    pub fn get_char(&self, idx: usize) -> Option<char> {
        self.internal_chars.get(idx).cloned()
    }
}

/// Section of a certain type
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word {
    pub start: usize,
    pub end: usize,
    pub word_type: WordType,
}

/// Either a white-space delimited word, tab or return character
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordType {
    /// Encountered a word (delimited by spaces)
    Word,
    // `\t` or `x09`
    Tab,
    /// `\r`, `\n` or `\r\n`, escaped: `\x0D`, `\x0A` or `\x0D\x0A`
    Return,
    /// Space character
    Space,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LineEnding {
    /// `\n` line ending
    Unix,
    /// `\r\n` line ending
    Windows,
}

#[derive(Debug, Clone)]
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
    /// Horizontal advance of the space glyph
    pub space_advance_px: f32,
    /// Glyph index of the space character
    pub space_codepoint: u32,
}

/// Word that is scaled (to a font / font instance), but not yet positioned
#[derive(Debug, Clone)]
pub struct ScaledWord {
    /// Glyphs, positions are relative to the first character of the word
    pub glyph_infos: Vec<GlyphInfo>,
    /// Horizontal advances of each glyph, necessary for
    /// hit-testing characters later on (for text selection).
    pub glyph_positions: Vec<GlyphPosition>,
    /// The sum of the width of all the characters in this word
    pub word_width: f32,
}

/// Stores the positions of the vertically laid out texts
#[derive(Debug, Clone, PartialEq)]
pub struct WordPositions {
    /// Font used to scale this text
    pub(crate) font_key: FontKey,
    /// Font instance used to scale this text
    pub(crate) font_instance_key: FontInstanceKey,
    /// Options like word spacing, character spacing, etc. that were
    /// used to layout these glyphs
    pub text_layout_options: TextLayoutOptions,
    /// Horizontal offset of the last line, necessary for inline layout later on.
    /// Usually, the "trailing" of the current text block is the "leading" of the
    /// next text block, to make it seem like two text runs push into each other.
    pub trailing: f32,
    /// How many words are in the text?
    pub number_of_words: usize,
    /// How many lines (NOTE: virtual lines, meaning line breaks in the layouted text) are there?
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
#[derive(Debug, Clone, PartialEq)]
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

/// Returns the layouted glyph instances
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutedGlyphs {
    pub glyphs: Vec<GlyphInstance>,
}

/// These metrics are important for showing the scrollbars
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum TextOverflow {
    /// Text is overflowing, by how much?
    /// Necessary for determining the size of the scrollbar
    IsOverflowing(TextSizePx),
    /// Text is in bounds, how much space is available until
    /// the edge of the rectangle? Necessary for centering / aligning text vertically.
    InBounds(TextSizePx),
}

/// Splits the text by whitespace into logical units (word, tab, return, whitespace).
pub fn split_text_into_words(text: &str) -> Words {

    use unicode_normalization::UnicodeNormalization;

    // Necessary because we need to handle both \n and \r\n characters
    // If we just look at the characters one-by-one, this wouldn't be possible.
    let normalized_string = text.nfc().collect::<String>();
    let normalized_chars = normalized_string.chars().collect::<Vec<char>>();

    let mut words = Vec::new();

    // Instead of storing the actual word, the word is only stored as an index instead,
    // which reduces allocations and is important for later on introducing RTL text
    // (where the position of the character data does not correspond to the actual glyph order).
    let mut current_word_start = 0;
    let mut last_char_idx = 0;
    let mut last_char_was_whitespace = false;

    let char_len = normalized_chars.len();

    for (ch_idx, ch) in normalized_chars.iter().enumerate() {

        let ch = *ch;
        let current_char_is_whitespace = ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n';

        let should_push_delimiter = match ch {
            ' ' => {
                Some(Word {
                    start: last_char_idx + 1,
                    end: ch_idx + 1,
                    word_type: WordType::Space
                })
            },
            '\t' => {
                Some(Word {
                    start: last_char_idx + 1,
                    end: ch_idx + 1,
                    word_type: WordType::Tab
                })
            },
            '\n' => {
                Some(if normalized_chars[last_char_idx] == '\r' {
                    // "\r\n" return
                    Word {
                        start: last_char_idx,
                        end: ch_idx + 1,
                        word_type: WordType::Return,
                    }
                } else {
                    // "\n" return
                    Word {
                        start: last_char_idx + 1,
                        end: ch_idx + 1,
                        word_type: WordType::Return,
                    }
                })
            },
            _ => None,
        };

        // Character is a whitespace or the character is the last character in the text (end of text)
        let should_push_word = if current_char_is_whitespace && !last_char_was_whitespace {
            Some(Word {
                start: current_word_start,
                end: ch_idx,
                word_type: WordType::Word
            })
        } else {
            None
        };

        if current_char_is_whitespace {
            current_word_start = ch_idx + 1;
        }

        let mut push_words = |arr: [Option<Word>;2]| {
            words.extend(arr.into_iter().filter_map(|e| *e));
        };

        push_words([should_push_word, should_push_delimiter]);

        last_char_was_whitespace = current_char_is_whitespace;
        last_char_idx = ch_idx;
    }

    // Push the last word
    if current_word_start != last_char_idx + 1 {
        words.push(Word {
            start: current_word_start,
            end: normalized_chars.len(),
            word_type: WordType::Word
        });
    }

    // If the last item is a `Return`, remove it
    if let Some(Word { word_type: WordType::Return, .. }) = words.last() {
        words.pop();
    }

    Words {
        items: words,
        internal_str: normalized_string,
        internal_chars: normalized_chars,
    }
}

/// Takes a text broken into semantic items and a font instance and
/// scales the font accordingly.
pub fn words_to_scaled_words(
    words: &Words,
    font: &LoadedFont,
    font_size: Au
) -> ScaledWords {

    use text_shaping::{self, HbBuffer, HbFont, HbScaledFont};

    let hb_font = HbFont::from_loaded_font(font);
    let hb_scaled_font = HbScaledFont::from_font(&hb_font, font_size);

    // Get the dimensions of the space glyph
    let hb_space_buffer = HbBuffer::from_str(" ");
    let hb_shaped_space = text_shaping::shape_word_hb(&hb_space_buffer, &hb_scaled_font);
    let space_advance_px = hb_shaped_space.glyph_positions[0].x_advance as f32 / 128.0; // TODO: Half width for spaces?
    let space_codepoint = hb_shaped_space.glyph_infos[0].codepoint;

    // Things like "tab" or "return" might confuse harfbuzz, need to shape
    // the entire font like it's just one long line
    let internal_str = words.internal_str.replace("\t", " ");
    let internal_str = internal_str.replace("\n", " ");
    let internal_str = internal_str.replace("\r\n", " ");

    let hb_buffer_entire_paragraph = HbBuffer::from_str(&internal_str);
    let hb_shaped_entire_paragraph = text_shaping::shape_word_hb(&hb_buffer_entire_paragraph, &hb_scaled_font);

    let mut shaped_word_positions = Vec::new();
    let mut shaped_word_infos = Vec::new();
    let mut current_word_positions = Vec::new();
    let mut current_word_infos = Vec::new();

    for i in 0..hb_shaped_entire_paragraph.glyph_positions.len() {
        let glyph_info = hb_shaped_entire_paragraph.glyph_infos[i];
        let glyph_position = hb_shaped_entire_paragraph.glyph_positions[i];

        let is_space = glyph_info.codepoint == space_codepoint;
        if is_space {
            shaped_word_positions.push(current_word_positions.clone());
            shaped_word_infos.push(current_word_infos.clone());
            current_word_positions.clear();
            current_word_infos.clear();
        } else {
            current_word_positions.push(glyph_position);
            current_word_infos.push(glyph_info);
        }
    }

    if !current_word_positions.is_empty() {
        shaped_word_positions.push(current_word_positions);
        shaped_word_infos.push(current_word_infos);
    }

    let mut longest_word_width = 0.0_f32;

    let scaled_words = words.items.iter()
        .filter(|w| w.word_type == WordType::Word)
        .enumerate()
        .filter_map(|(word_idx, word)| {

            let hb_glyph_positions = shaped_word_positions.get(word_idx)?;
            let hb_glyph_infos = shaped_word_infos.get(word_idx)?;

            let hb_word_width = text_shaping::get_word_visual_width_hb(&hb_glyph_positions);
            let hb_glyph_positions = text_shaping::get_glyph_positions_hb(&hb_glyph_positions);
            let hb_glyph_infos = text_shaping::get_glyph_infos_hb(&hb_glyph_infos);

            longest_word_width = longest_word_width.max(hb_word_width.abs());

            Some(ScaledWord {
                glyph_infos: hb_glyph_infos,
                glyph_positions: hb_glyph_positions,
                word_width: hb_word_width,
            })
        }).collect();

    ScaledWords {
        font_key: font.key,
        font_instance_key: font.font_instances[&font_size],
        items: scaled_words,
        longest_word_width: longest_word_width,
        space_advance_px,
        space_codepoint,
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
pub fn position_words(
    words: &Words,
    scaled_words: &ScaledWords,
    text_layout_options: &TextLayoutOptions,
    font_size: TextSizePx,
    overflow: LayoutOverflow,
    scrollbar_style: &ScrollbarStyle,
) -> WordPositions {

    use self::WordType::*;
    use std::f32;

    // TODO: Handle scrollbar / content size adjustment!

    let font_size_px = font_size.0;
    let space_advance = scaled_words.space_advance_px;
    let word_spacing_px = space_advance * text_layout_options.word_spacing.map(|s| s.0).unwrap_or(DEFAULT_WORD_SPACING);
    let line_height_px = space_advance * text_layout_options.line_height.map(|lh| lh.0.get()).unwrap_or(DEFAULT_LINE_HEIGHT);
    let tab_width_px = space_advance * text_layout_options.tab_width.unwrap_or(DEFAULT_TAB_WIDTH);
    let letter_spacing_percent = text_layout_options.letter_spacing.map(|ls| ls.0).unwrap_or(DEFAULT_LETTER_SPACING);

    let mut line_breaks = Vec::new();
    let mut word_positions = Vec::new();

    let mut line_number = 0;
    let mut line_caret_x = 0.0;
    let mut current_word_idx = 0;

    macro_rules! advance_caret {($line_caret_x:expr) => ({
        let caret_intersection = caret_intersects_with_holes(
            $line_caret_x,
            line_number,
            font_size_px,
            line_height_px,
            &text_layout_options.holes,
            text_layout_options.max_horizontal_width,
        );

        if let LineCaretIntersection::PushCaretOntoNextLine(_, _) = caret_intersection {
             line_breaks.push((current_word_idx, line_caret_x));
        }

        // Correct and advance the line caret position
        advance_caret(
            &mut $line_caret_x,
            &mut line_number,
            caret_intersection,
        );
    })}

    advance_caret!(line_caret_x);

    if let Some(leading) = text_layout_options.leading {
        line_caret_x += leading.0;
        advance_caret!(line_caret_x);
    }

    // NOTE: word_idx increases only on words, not on other symbols!
    let mut word_idx = 0;

    macro_rules! handle_word {() => ({
        let scaled_word = match scaled_words.items.get(word_idx) {
            Some(s) => s,
            None => continue,
        };

        // let line_caret_y = get_line_y_position(line_number, font_size_px, line_height_px);
        // word_positions.push(LayoutPoint::new(line_caret_x, line_caret_y));

        // Calculate where the caret would be for the next word
        let word_advance_x =
            scaled_word.word_width
          + (scaled_word.glyph_infos.len().saturating_sub(1) as f32 * letter_spacing_percent);

        let mut new_caret_x = line_caret_x + word_advance_x;

        // advance_caret!(new_caret_x);

        let caret_intersection = caret_intersects_with_holes(
            new_caret_x,
            line_number,
            font_size_px,
            line_height_px,
            &text_layout_options.holes,
            text_layout_options.max_horizontal_width,
        );

        let mut is_line_break = false;
        if let LineCaretIntersection::PushCaretOntoNextLine(_, _) = caret_intersection {
            line_breaks.push((current_word_idx, line_caret_x));
            is_line_break = true;
        }

        if !is_line_break {
            let line_caret_y = get_line_y_position(line_number, font_size_px, line_height_px);
            word_positions.push(LayoutPoint::new(line_caret_x, line_caret_y));
        }

        // Correct and advance the line caret position
        advance_caret(
            &mut new_caret_x,
            &mut line_number,
            caret_intersection,
        );

        line_caret_x = new_caret_x;

        if is_line_break {
            let line_caret_y = get_line_y_position(line_number, font_size_px, line_height_px);
            word_positions.push(LayoutPoint::new(line_caret_x, line_caret_y));
            line_caret_x += word_advance_x;
        }

        // NOTE: Increase before pushing, word indices are 1-indexed (0..word_index)!
        word_idx += 1;
        current_word_idx = word_idx;
    })}

    for word in words.items.iter().take(words.items.len().saturating_sub(1)) {
        match word.word_type {
            Word => {
                handle_word!();
            },
            Return => {
                line_breaks.push((current_word_idx, line_caret_x));
                line_number += 1;
                let mut new_caret_x = 0.0;
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

    // Handle the last word, but ignore any last Return, Space or Tab characters
    for word in &words.items[words.items.len().saturating_sub(1)..] {
        handle_word!();
        line_breaks.push((current_word_idx, line_caret_x));
    }

    let trailing = line_caret_x;
    let number_of_lines = line_number + 1;
    let number_of_words = current_word_idx + 1;

    let longest_line_width = line_breaks.iter().map(|(_word_idx, line_length)| *line_length).fold(0.0_f32, f32::max);
    let content_size_y = get_line_y_position(line_number, font_size_px, line_height_px);
    let content_size_x = text_layout_options.max_horizontal_width.map(|x| x.0).unwrap_or(longest_line_width);
    let content_size = LayoutSize::new(content_size_x, content_size_y);

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
pub fn get_layouted_glyphs(
    word_positions: &WordPositions,
    scaled_words: &ScaledWords,
    alignment_horz: StyleTextAlignmentHorz,
    alignment_vert: StyleTextAlignmentVert,
    rect_offset: LayoutPoint,
    bounding_size_height_px: f32,
) -> LayoutedGlyphs {

    use text_shaping;

    let font_size_px = word_positions.font_size.0;
    let letter_spacing_px = font_size_px * word_positions.text_layout_options.letter_spacing.map(|ls| ls.0).unwrap_or(DEFAULT_LETTER_SPACING);

    let mut glyphs = Vec::with_capacity(scaled_words.items.len());

    for (scaled_word, word_position) in scaled_words.items.iter().zip(word_positions.word_positions.iter()) {
        glyphs.extend(
            text_shaping::get_glyph_instances_hb(&scaled_word.glyph_infos, &scaled_word.glyph_positions)
            .into_iter()
            .enumerate()
            .map(|(glyph_id, mut glyph)| {
                glyph.point.x += word_position.x;
                glyph.point.y += word_position.y;
                if glyph_id != 0 {
                    glyph.point.x += letter_spacing_px;
                }
                glyph
            })
        )
    }

    let line_breaks = get_char_indices(&word_positions, &scaled_words);
    let vertical_overflow = get_vertical_overflow(&word_positions, bounding_size_height_px);

    align_text_horz(&mut glyphs, alignment_horz, &line_breaks);
    align_text_vert(&mut glyphs, alignment_vert, &line_breaks, vertical_overflow);
    add_origin(&mut glyphs, rect_offset.x, rect_offset.y);

    LayoutedGlyphs {
        glyphs: glyphs,
    }
}

/// Given a width, returns the vertical height and width of the text
pub fn get_positioned_word_bounding_box(word_positions: &WordPositions) -> LayoutSize {
    word_positions.content_size
}

pub fn get_vertical_overflow(word_positions: &WordPositions, bounding_size_height_px: f32) -> TextOverflow {
    let content_size = word_positions.content_size;
    if bounding_size_height_px > content_size.height {
        TextOverflow::InBounds(TextSizePx(bounding_size_height_px - content_size.height))
    } else {
        TextOverflow::IsOverflowing(TextSizePx(content_size.height - bounding_size_height_px))
    }
}

pub fn word_item_is_return(item: &Word) -> bool {
    item.word_type == WordType::Return
}

pub fn text_overflow_is_overflowing(overflow: &TextOverflow) -> bool {
    use self::TextOverflow::*;
    match overflow {
        IsOverflowing(_) => true,
        InBounds(_) => false,
    }
}

pub fn get_char_indices(word_positions: &WordPositions, scaled_words: &ScaledWords)
-> Vec<(GlyphIndex, RemainingSpaceToRight)>
{
    let width = word_positions.content_size.width;

    if scaled_words.items.is_empty() {
        return Vec::new();
    }

    let mut current_glyph_count = 0;
    let mut last_word_idx = 0;

    word_positions.line_breaks.iter().map(|(current_word_idx, line_length)| {
        let remaining_space_px = width - line_length;
        let words = &scaled_words.items[last_word_idx..*current_word_idx];
        let glyphs_in_this_line: usize = words.iter().map(|w| w.glyph_infos.len()).sum::<usize>();

        current_glyph_count += glyphs_in_this_line;
        last_word_idx = *current_word_idx;

        (current_glyph_count, remaining_space_px)
    }).collect()
}

/// For a given line number (**NOTE: 0-indexed!**), calculates the Y
/// position of the bottom left corner
pub fn get_line_y_position(line_number: usize, font_size_px: f32, line_height_px: f32) -> f32 {
    ((font_size_px + line_height_px) * line_number as f32) + font_size_px
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
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

    // If the caret is outside of the max_width, move it to the start of a new line
    if let Some(max_width) = max_width {
        if line_caret_x > max_width.0 {
            new_line_caret_x = Some(0.0);
            line_advance += 1;
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
                if hole.origin.x + hole.size.width >= max_width.0 {
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

pub fn align_text_horz(
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

    // // assert that the last info in the line_breaks vec has the same glyph index
    // // i.e. the last line has to end with the last glyph
    // assert!(glyphs.len() - 1 == line_breaks[line_breaks.len() - 1].0);

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
        for glyph in &mut glyphs[start_range_char..*line_break_char] {
            let old_glyph_x = glyph.point.x;
            glyph.point.x += line_break_amount * multiply_factor;
        }
        start_range_char = *line_break_char; // NOTE: beware off-by-one error - note the +1!
    }
}

pub fn align_text_vert(
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

    // // Die if we have a line break at a position bigger than the position of the last glyph,
    // // because something went horribly wrong!
    // //
    // // The next unwrap is always safe as line_breaks will have a minimum of one entry!
    // assert!(glyphs.len() - 1 == line_breaks.last().unwrap().0);

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
pub fn add_origin(positioned_glyphs: &mut [GlyphInstance], x: f32, y: f32) {
    for c in positioned_glyphs {
        c.point.x += x;
        c.point.y += y;
    }
}

/// Returned result from the `layout_text` function
#[derive(Debug, Clone)]
pub struct LayoutTextResult {
    /// The words, broken up by whitespace
    pub words: Words,
    /// Words, scaled by a certain font size (with font metrics)
    pub scaled_words: ScaledWords,
    /// Layout of the positions, word-by-word
    pub word_positions: WordPositions,
}

#[test]
fn test_split_words() {

    fn print_words(w: &Words) {
        println!("-- string: {:?}", w.get_str());
        for item in &w.items {
            println!("{:?} - ({}..{}) = {:?}", w.get_substr(item), item.start, item.end, item.word_type);
        }
    }

    fn string_to_vec(s: String) -> Vec<char> {
        s.chars().collect()
    }

    fn assert_words(expected: &Words, got_words: &Words) {
        for (idx, expected_word) in expected.items.iter().enumerate() {
            let got = got_words.items.get(idx);
            if got != Some(expected_word) {
                println!("expected: ");
                print_words(expected);
                println!("got: ");
                print_words(got_words);
                panic!("Expected word idx {} - expected: {:#?}, got: {:#?}", idx, Some(expected_word), got);
            }
        }
    }

    let ascii_str = String::from("abc\tdef  \nghi\r\njkl");
    let words_ascii = split_text_into_words(&ascii_str);
    let words_ascii_expected = Words {
        internal_str: ascii_str.clone(),
        internal_chars: string_to_vec(ascii_str),
        items: vec![
            Word { start: 0,    end: 3,     word_type: WordType::Word     }, // "abc" - (0..3) = Word
            Word { start: 3,    end: 4,     word_type: WordType::Tab      }, // "\t" - (3..4) = Tab
            Word { start: 4,    end: 7,     word_type: WordType::Word     }, // "def" - (4..7) = Word
            Word { start: 7,    end: 8,     word_type: WordType::Space    }, // " " - (7..8) = Space
            Word { start: 8,    end: 9,     word_type: WordType::Space    }, // " " - (8..9) = Space
            Word { start: 9,    end: 10,    word_type: WordType::Return   }, // "\n" - (9..10) = Return
            Word { start: 10,   end: 13,    word_type: WordType::Word     }, // "ghi" - (10..13) = Word
            Word { start: 13,   end: 15,    word_type: WordType::Return   }, // "\r\n" - (13..15) = Return
            Word { start: 15,   end: 18,    word_type: WordType::Word     }, // "jkl" - (15..18) = Word
        ],
    };

    assert_words(&words_ascii_expected, &words_ascii);

    let unicode_str = String::from("㌊㌋㌌㌍㌎㌏㌐㌑ ㌒㌓㌔㌕㌖㌗");
    let words_unicode = split_text_into_words(&unicode_str);
    let words_unicode_expected = Words {
        internal_str: unicode_str.clone(),
        internal_chars: string_to_vec(unicode_str),
        items: vec![
            Word { start: 0,        end: 8,         word_type: WordType::Word   }, // "㌊㌋㌌㌍㌎㌏㌐㌑"
            Word { start: 8,        end: 9,         word_type: WordType::Space  }, // " "
            Word { start: 9,        end: 15,        word_type: WordType::Word   }, // "㌒㌓㌔㌕㌖㌗"
        ],
    };

    assert_words(&words_unicode_expected, &words_unicode);

    let single_str = String::from("A");
    let words_single_str = split_text_into_words(&single_str);
    let words_single_str_expected = Words {
        internal_str: single_str.clone(),
        internal_chars: string_to_vec(single_str),
        items: vec![
            Word { start: 0,        end: 1,         word_type: WordType::Word   }, // "A"
        ],
    };

    assert_words(&words_single_str_expected, &words_single_str);
}

#[test]
fn test_get_line_y_position() {

    assert_eq!(get_line_y_position(0, 20.0, 0.0), 20.0);
    assert_eq!(get_line_y_position(1, 20.0, 0.0), 40.0);
    assert_eq!(get_line_y_position(2, 20.0, 0.0), 60.0);

    // lines:
    // 0 - height 20, padding 5 = 20.0 (padding is for the next line)
    // 1 - height 20, padding 5 = 45.0 ( = 20 + 20 + 5)
    // 2 - height 20, padding 5 = 70.0 ( = 20 + 20 + 5 + 20 + 5)
    assert_eq!(get_line_y_position(0, 20.0, 5.0), 20.0);
    assert_eq!(get_line_y_position(1, 20.0, 5.0), 45.0);
    assert_eq!(get_line_y_position(2, 20.0, 5.0), 70.0);
}

// Scenario 1:
//
// +---------+
// |+ ------>|+
// |         |
// +---------+
// rectangle: 100x200
// max-width: none, line-height 1.0, font-size: 20
// cursor is at: 0x, 20y
// expect cursor to advance to 100x, 20y
//
#[test]
fn test_caret_intersects_with_holes_1() {
    let line_caret_x = 0.0;
    let line_number = 0;
    let font_size_px = 20.0;
    let line_height_px = 0.0;
    let max_width = None;
    let holes = vec![LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(200.0, 100.0))];

    let result = caret_intersects_with_holes(
        line_caret_x,
        line_number,
        font_size_px,
        line_height_px,
        &holes,
        max_width,
    );

    assert_eq!(result, LineCaretIntersection::AdvanceCaretTo(200.0));
}

// Scenario 2:
//
// +---------+
// |+ -----> |
// |-------> |
// |---------|
// |+        |
// |         |
// +---------+
// rectangle: 100x200
// max-width: 200px, line-height 1.0, font-size: 20
// cursor is at: 0x, 20y
// expect cursor to advance to 0x, 100y (+= 4 lines)
//
#[test]
fn test_caret_intersects_with_holes_2() {
    let line_caret_x = 0.0;
    let line_number = 0;
    let font_size_px = 20.0;
    let line_height_px = 0.0;
    let max_width = Some(TextSizePx(200.0));
    let holes = vec![LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(200.0, 100.0))];

    let result = caret_intersects_with_holes(
        line_caret_x,
        line_number,
        font_size_px,
        line_height_px,
        &holes,
        max_width,
    );

    assert_eq!(result, LineCaretIntersection::PushCaretOntoNextLine(4, 0.0));
}

// Scenario 3:
//
// +----------------+
// |      |         |  +----->
// |------->+       |
// |------+         |
// |                |
// |                |
// +----------------+
// rectangle: 100x200
// max-width: 400px, line-height 1.0, font-size: 20
// cursor is at: 450x, 20y
// expect cursor to advance to 200x, 40y (+= 1 lines, leading of 200px)
//
#[test]
fn test_caret_intersects_with_holes_3() {
    let line_caret_x = 450.0;
    let line_number = 0;
    let font_size_px = 20.0;
    let line_height_px = 0.0;
    let max_width = Some(TextSizePx(400.0));
    let holes = vec![LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(200.0, 100.0))];

    let result = caret_intersects_with_holes(
        line_caret_x,
        line_number,
        font_size_px,
        line_height_px,
        &holes,
        max_width,
    );

    assert_eq!(result, LineCaretIntersection::PushCaretOntoNextLine(1, 200.0));
}

// Scenario 4:
//
// +----------------+
// | +   +------+   |
// |     |      |   |
// |     |      |   |
// |     +------+   |
// |                |
// +----------------+
// rectangle: 100x200 @ 80.0x, 20.0y
// max-width: 400px, line-height 1.0, font-size: 20
// cursor is at: 40x, 20y
// expect cursor to not advance at all
//
#[test]
fn test_caret_intersects_with_holes_4() {
    let line_caret_x = 40.0;
    let line_number = 0;
    let font_size_px = 20.0;
    let line_height_px = 0.0;
    let max_width = Some(TextSizePx(400.0));
    let holes = vec![LayoutRect::new(LayoutPoint::new(80.0, 20.0), LayoutSize::new(200.0, 100.0))];

    let result = caret_intersects_with_holes(
        line_caret_x,
        line_number,
        font_size_px,
        line_height_px,
        &holes,
        max_width,
    );

    assert_eq!(result, LineCaretIntersection::NoIntersection);
}
