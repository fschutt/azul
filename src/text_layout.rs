#![allow(unused_variables, dead_code)]

use webrender::api::*;
use euclid::{Length, TypedRect, TypedSize2D, TypedPoint2D};
use rusttype::{Font, Scale, GlyphId};
use css_parser::{TextAlignmentHorz, TextAlignmentVert, LineHeight, TextOverflowBehaviour};

/// Rusttype has a certain sizing hack, I have no idea where this number comes from
/// Without this adjustment, we won't have the correct horizontal spacing
const RUSTTYPE_SIZE_HACK: f32 = 72.0 / 41.0;

const PX_TO_PT: f32 = 72.0 / 96.0;

/// Lines is responsible for layouting the lines of the rectangle to
struct Lines<'a> {
    /// Horizontal text alignment
    horz_align: TextAlignmentHorz,
    /// Vertical text alignment (only respected when the
    /// characters don't overflow the bounds)
    vert_align: TextAlignmentVert,
    /// Line height multiplier (X * `self.font_size`) - default 1.0
    line_height: Option<LineHeight>,
    /// The font to use for layouting the characters
    font: &'a Font<'a>,
    // Font size of the font
    font_size: f32,
    /// The bounds of the lines (bounding rectangle)
    bounds: TypedRect<f32, LayoutPixel>,
}

#[derive(Debug)]
struct Word {
    // the original text
    pub text: String,
    // glyphs, positions are relative to the first character of the word
    pub glyphs: Vec<GlyphInstance>,
    // the sum of the width of all the characters
    pub total_width: f32,
}

#[derive(Debug)]
enum SemanticWordItem {
    /// Encountered a word (delimited by spaces)
    Word(Word),
    // `\t` or `x09`
    Tab,
    /// `\r`, `\n` or `\r\n`, escaped: `\x0D`, `\x0A` or `\x0D\x0A`
    Return,
}

/// Returned struct for the pass-1 text run test.
///
/// Once the text is parsed and split into words + normalized, we can calculate
/// (without looking at the text itself), if the text overflows the parent rectangle,
/// in order to determine if we need to show a scroll bar.
#[derive(Debug, Clone)]
pub(crate) struct TextOverflowPass1 {
    /// Is the text overflowing in the horizontal direction?
    pub(crate) horizontal: TextOverflow,
    /// Is the text overflowing in the vertical direction?
    pub(crate) vertical: TextOverflow,
}

/// In the case that we do overflow the rectangle (in any direction),
/// we need to now re-calculate the positions for the words (because of the reduced available
/// space that is now taken up by the scrollbars).
#[derive(Debug, Copy, Clone)]
pub(crate) struct TextOverflowPass2 {
    /// Is the text overflowing in the horizontal direction?
    pub(crate) horizontal: TextOverflow,
    /// Is the text overflowing in the vertical direction?
    pub(crate) vertical: TextOverflow,
}

/// These metrics are important for showing the scrollbars
#[derive(Debug, Copy, Clone)]
pub(crate) enum TextOverflow {
    /// Text is overflowing, by how much (in pixels)?
    /// Necessary for determining the size of the scrollbar
    IsOverflowing(f32),
    /// Text is in bounds, how much space (in pixels) is available until
    /// the edge of the rectangle? Necessary for centering / aligning text vertically.
    InBounds(f32),
}

#[derive(Debug, Copy, Clone)]
struct HarfbuzzAdjustment(pub f32);

#[derive(Debug, Copy, Clone)]
struct KnuthPlassAdjustment(pub f32);

/// Temporary struct so I don't have to pass the three parameters around seperately all the time
#[derive(Debug, Copy, Clone)]
struct FontMetrics {
    /// Width of the space character
    space_width: f32,
    /// Usually 4 * space_width
    tab_width: f32,
    /// font_size * line_height
    vertical_advance: f32,
}

impl<'a> Lines<'a>
{
    #[inline]
    pub(crate) fn from_bounds(
        bounds: &TypedRect<f32, LayoutPixel>,
        horiz_alignment: TextAlignmentHorz,
        vert_alignment: TextAlignmentVert,
        font: &'a Font<'a>,
        font_size: f32,
        line_height: Option<LineHeight>)
    -> Self
    {
        Self {
            horz_align: horiz_alignment,
            vert_align: vert_alignment,
            line_height: line_height,
            font: font,
            bounds: *bounds,
            font_size: font_size,
        }
    }

    /// NOTE: The glyphs are in the space of the bounds, not of the layer!
    /// You'd need to offset them by `bounds.origin` to get the correct position
    ///
    /// This function will only process the glyphs until they overflow
    /// (we don't process glyphs that are out of the bounds of the rectangle, since
    /// they don't get drawn anyway).
    pub(crate) fn get_glyphs(&mut self, text: &str, overflow_behaviour: TextOverflowBehaviour)
    -> (Vec<GlyphInstance>, TextOverflowPass2)
    {
        let font = &self.font;
        let font_size = Scale::uniform(self.font_size);
        let max_horizontal_width = self.bounds.size.width;

        let line_height = match self.line_height { Some(lh) => (lh.0).number, None => 1.0 };
        // Maximum number of lines that can be shown in the rectangle
        // before the text overflows
        let max_lines_before_overflow = (self.bounds.size.height / (self.font_size * line_height)).floor() as usize;
        // Width of the ' ' (space) character (for adding spacing between words)
        let space_width = self.font.glyph(' ').scaled(Scale::uniform(self.font_size)).h_metrics().advance_width;

        let tab_width = 4.0 * space_width; // TODO: make this configurable

        let font_metrics = FontMetrics {
            vertical_advance: self.font_size * line_height,
            space_width: space_width,
            tab_width: tab_width,
        };

        // (1) Split the text into semantic items (word, tab or newline)
        // This function also normalizes the unicode characters and calculates kerning.
        //
        // TODO: cache the words somewhere
        let words = split_text_into_words(text, font, font_size);

        // (2) Calculate the additions / subtractions that have to be take into account
        let harfbuzz_adjustments = calculate_harfbuzz_adjustments(&text, font);

        // (3) Determine if the words will overflow the bounding rectangle
        let overflow_pass_1 = estimate_overflow_pass_1(&words, &self.bounds.size, &font_metrics, &overflow_behaviour);

        // (4) If the lines overflow, subtract the space needed for the scrollbars and calculate the length
        // again (TODO: already layout characters here?)
        let overflow_pass_2 = estimate_overflow_pass_2(&mut words, &self.bounds.size, &font_metrics, &overflow_behaviour, overflow_pass_1);

        // (5) Align text to the left, initial layout of glyphs
        let (mut positioned_glyphs, line_break_offsets) =
            words_to_left_aligned_glyphs(words, font, self.font_size, max_horizontal_width, max_lines_before_overflow, &font_metrics);

        // (6) Add the harfbuzz adjustments to the positioned glyphs
        apply_harfbuzz_adjustments(&mut positioned_glyphs, harfbuzz_adjustments);

        // (7) Calculate the Knuth-Plass adjustments for the (now layouted) glyphs
        let knuth_plass_adjustments = calculate_knuth_plass_adjustments(&positioned_glyphs, &line_break_offsets);

        // (8) Add the Knuth-Plass adjustments to the positioned glyphs
        apply_knuth_plass_adjustments(&mut positioned_glyphs, knuth_plass_adjustments);

        // (9) Align text horizontally (early return if left-aligned)
        align_text_horz(self.horz_align, &mut positioned_glyphs, &line_break_offsets, &overflow_pass_2);

        // (10) Align text vertically (early return if text overflows)
        align_text_vert(self.vert_align, &mut positioned_glyphs, &line_break_offsets, &overflow_pass_2);

        // (11) Add the self.origin to all the glyphs to bring them from glyph space into world space
        add_origin(&mut positioned_glyphs, self.bounds.origin.x, self.bounds.origin.y);

        (positioned_glyphs, overflow_pass_2)
    }
}

#[inline(always)]
fn split_text_into_words<'a>(text: &str, font: &Font<'a>, font_size: Scale)
-> Vec<SemanticWordItem>
{
    use unicode_normalization::UnicodeNormalization;

    let mut words = Vec::new();

    let mut word_caret = 0.0;
    let mut cur_word_length = 0.0;
    let mut chars_in_this_word = Vec::new();
    let mut glyphs_in_this_word = Vec::new();
    let mut last_glyph = None;

    fn end_word(words: &mut Vec<SemanticWordItem>,
                chars_in_this_word: &mut Vec<char>,
                glyphs_in_this_word: &mut Vec<GlyphInstance>,
                cur_word_length: &mut f32,
                word_caret: &mut f32,
                last_glyph: &mut Option<GlyphId>)
    {
        // End of word
        words.push(SemanticWordItem::Word(Word {
            text: chars_in_this_word.drain(..).collect(),
            glyphs: glyphs_in_this_word.drain(..).collect(),
            total_width: *cur_word_length,
        }));

        // Reset everything
        *last_glyph = None;
        *word_caret = 0.0;
        *cur_word_length = 0.0;
    }

    for cur_char in text.nfc() {
        match cur_char {
            '\t' => {
                // End of word + tab
                if !chars_in_this_word.is_empty() {
                    end_word(
                        &mut words,
                        &mut chars_in_this_word,
                        &mut glyphs_in_this_word,
                        &mut cur_word_length,
                        &mut word_caret,
                        &mut last_glyph);
                }
                words.push(SemanticWordItem::Tab);
            },
            '\n' => {
                // End of word + newline
                if !chars_in_this_word.is_empty() {
                    end_word(
                        &mut words,
                        &mut chars_in_this_word,
                        &mut glyphs_in_this_word,
                        &mut cur_word_length,
                        &mut word_caret,
                        &mut last_glyph);
                }
                words.push(SemanticWordItem::Return);
            },
            ' ' => {
                if !chars_in_this_word.is_empty() {
                    end_word(
                        &mut words,
                        &mut chars_in_this_word,
                        &mut glyphs_in_this_word,
                        &mut cur_word_length,
                        &mut word_caret,
                        &mut last_glyph);
                }
            },
            cur_char =>  {
                // Regular character
                use rusttype::Point;

                let g = font.glyph(cur_char).scaled(font_size);
                let id = g.id();

                if let Some(last) = last_glyph {
                    word_caret += font.pair_kerning(font_size, last, g.id());
                }

                let g = g.positioned(Point { x: word_caret, y: 0.0 });
                last_glyph = Some(id);
                let horiz_advance = g.unpositioned().h_metrics().advance_width;
                word_caret += horiz_advance;
                cur_word_length += horiz_advance;

                glyphs_in_this_word.push(GlyphInstance {
                    index: id.0,
                    point: TypedPoint2D::new(g.position().x, g.position().y),
                });

                chars_in_this_word.push(cur_char);
            }
        }
    }

    // Push last word
    if !chars_in_this_word.is_empty() {
        end_word(
            &mut words,
            &mut chars_in_this_word,
            &mut glyphs_in_this_word,
            &mut cur_word_length,
            &mut word_caret,
            &mut last_glyph);
    }

    words
}

// First pass: calculate if the words will overflow (using the tabs)
#[inline(always)]
fn estimate_overflow_pass_1(
    words: &[SemanticWordItem],
    rect: &TypedSize2D<f32, LayoutPixel>,
    font_metrics: &FontMetrics,
    overflow_behaviour: &TextOverflowBehaviour)
-> TextOverflowPass1
{
    let FontMetrics { space_width, tab_width, vertical_advance } = *font_metrics;

    /*
        /// Always shows a scroll bar, overflows on scroll
        Scroll,
        /// Does not show a scroll bar by default, only when text is overflowing
        Auto,
        /// Never shows a scroll bar, simply clips text
        Hidden,
        /// Doesn't show a scroll bar, simply overflows the text
        Visible,
    */

    let mut min_w = 0.0;
    // Minimum height necessary for all the returns
    let mut min_h = 0.0;

    for word in words {
        match word {
            SemanticWordItem::Word(Word { total_width, .. }) => { },
            SemanticWordItem::Tab => { },
            SemanticWordItem::Return => { },
        }
    }
}

#[inline(always)]
fn estimate_overflow_pass_2(
    words: &[SemanticWordItem],
    rect: &TypedSize2D<f32, LayoutPixel>,
    font_metrics: &FontMetrics,
    overflow_behaviour: &TextOverflowBehaviour,
    pass1: TextOverflowPass1)
-> TextOverflowPass2
{
    let FontMetrics { space_width, tab_width, vertical_advance } = *font_metrics;

}

#[inline(always)]
fn calculate_harfbuzz_adjustments<'a>(text: &str, font: &Font<'a>)
-> Vec<HarfbuzzAdjustment>
{
    use harfbuzz_rs::*;
    use harfbuzz_rs::rusttype::SetRustTypeFuncs;
    /*
    let path = "path/to/some/font_file.otf";
    let index = 0; //< face index in the font file
    let face = Face::from_file(path, index).unwrap();
    let mut font = Font::new(face);

    font.set_rusttype_funcs();

    let output = UnicodeBuffer::new().add_str(text).shape(&font, &[]);
    let positions = output.get_glyph_positions();
    let infos = output.get_glyph_infos();

    for (position, info) in positions.iter().zip(infos) {
        println!("gid: {:?}, cluster: {:?}, x_advance: {:?}, x_offset: {:?}, y_offset: {:?}",
            info.codepoint, info.cluster, position.x_advance, position.x_offset, position.y_offset);
    }
    */
    Vec::new() // TODO
}

#[inline(always)]
fn words_to_left_aligned_glyphs<'a>(
    words: Vec<SemanticWordItem>,
    font: &Font<'a>,
    font_size: f32,
    max_horizontal_width: f32,
    max_lines_before_overflow: usize,
    font_metrics: &FontMetrics)
-> (Vec<GlyphInstance>, Vec<(usize, f32)>)
{
    let FontMetrics { space_width, tab_width, vertical_advance } = *font_metrics;

    // left_aligned_glyphs stores the X and Y coordinates of the positioned glyphs,
    // left-aligned
    let mut left_aligned_glyphs = Vec::<GlyphInstance>::new();

    // The line break offsets (neded for center- / right-aligned text contains:
    //
    // - The index of the glyph at which the line breaks
    // - How much space each line has (to the right edge of the containing rectangle)
    let mut line_break_offsets = Vec::<(usize, f32)>::new();

    let v_metrics_scaled = font.v_metrics(Scale::uniform(vertical_advance));
    let v_advance_scaled = v_metrics_scaled.ascent - v_metrics_scaled.descent + v_metrics_scaled.line_gap;

    let offset_top = v_metrics_scaled.ascent;

    // word_caret is the current X position of the "pen" we are writing with
    let mut word_caret = 0.0;
    let mut current_line_num = 0;

    for word in words {
        use self::SemanticWordItem::*;
        match word {
            Word(word) => {
                let text_overflows_rect = word_caret + word.total_width > max_horizontal_width;

                // Line break occurred
                if text_overflows_rect {
                    line_break_offsets.push((left_aligned_glyphs.len() - 1, max_horizontal_width - word_caret));
                    word_caret = 0.0;
                    current_line_num += 1;
                }

                for mut glyph in word.glyphs {
                    let push_x = word_caret;
                    let push_y = (current_line_num as f32 * v_advance_scaled) + offset_top;
                    glyph.point.x += push_x;
                    glyph.point.y += push_y;
                    left_aligned_glyphs.push(glyph);
                }

                // Add the word width to the current word_caret
                // NOTE: has to happen BEFORE the `break` statment, since we use the word_caret
                // later for the last line
                word_caret += word.total_width + space_width;

                if current_line_num > max_lines_before_overflow {
                    break;
                }
            },
            Tab => {
                word_caret += tab_width;
            },
            Return => {
                // TODO: dupliated code
                line_break_offsets.push((left_aligned_glyphs.len() - 1, max_horizontal_width - word_caret));
                word_caret = 0.0;
                current_line_num += 1;
            },
        }
    }

    // push the infos about the last line
    if !left_aligned_glyphs.is_empty() {
        line_break_offsets.push((left_aligned_glyphs.len() - 1, max_horizontal_width - word_caret));
    }

    (left_aligned_glyphs, line_break_offsets)
}

#[inline(always)]
fn apply_harfbuzz_adjustments(positioned_glyphs: &mut [GlyphInstance], harfbuzz_adjustments: Vec<HarfbuzzAdjustment>)
{
    // TODO
}

#[inline(always)]
fn calculate_knuth_plass_adjustments(positioned_glyphs: &[GlyphInstance], line_break_offsets: &[(usize, f32)])
-> Vec<KnuthPlassAdjustment>
{
    // TODO
    Vec::new()
}

#[inline(always)]
fn apply_knuth_plass_adjustments(positioned_glyphs: &mut [GlyphInstance], knuth_plass_adjustments: Vec<KnuthPlassAdjustment>)
{
    // TODO
}

#[inline(always)]
fn align_text_horz(alignment: TextAlignmentHorz, glyphs: &mut [GlyphInstance], line_breaks: &[(usize, f32)], overflow: &TextOverflowPass2)
{
    use css_parser::TextAlignmentHorz::*;

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

    if line_breaks.is_empty() {
        return; // ??? maybe a 0-height rectangle?
    }

    // assert that the last info in the line_breaks vec has the same glyph index
    // i.e. the last line has to end with the last glyph
    assert!(glyphs.len() - 1 == line_breaks[line_breaks.len() - 1].0);

    if alignment == TextAlignmentHorz::Left {
        return;
    }

    let multiply_factor = match alignment {
        Left => { return; },
        Right => 1.0, // move the line by the full width
        Center => 0.5, // move the line by the half width
    };

    let mut current_line_num = 0;
    for (glyph_idx, glyph) in glyphs.iter_mut().enumerate() {
        if glyph_idx > line_breaks[current_line_num].0 {
            current_line_num += 1;
        }
        let space_added_full = line_breaks[current_line_num].1;
        glyph.point.x += space_added_full * multiply_factor;
    }
}

#[inline(always)]
fn align_text_vert(alignment: TextAlignmentVert, glyphs: &mut [GlyphInstance], line_breaks: &[(usize, f32)], overflow: &TextOverflowPass2) {

}

/// Adds the X and Y offset to each glyph in the positioned glyph
#[inline(always)]
fn add_origin(positioned_glyphs: &mut [GlyphInstance], x: f32, y: f32)
{
    for c in positioned_glyphs {
        c.point.x += x;
        c.point.y += y;
    }
}

pub(crate) fn put_text_in_bounds<'a>(
    text: &str,
    font: &Font<'a>,
    font_size: f32,
    line_height: Option<LineHeight>,
    horz_align: TextAlignmentHorz,
    vert_align: TextAlignmentVert,
    overflow_behaviour: TextOverflowBehaviour,
    bounds: &TypedRect<f32, LayoutPixel>)
-> Vec<GlyphInstance>
{
    let mut lines = Lines::from_bounds(
        bounds,
        horz_align,
        vert_align,
        font,
        font_size * RUSTTYPE_SIZE_HACK * PX_TO_PT,
        line_height);

    let (glyphs, overflow) = lines.get_glyphs(text, overflow_behaviour);
    glyphs
}