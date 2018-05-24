#![allow(unused_variables, dead_code)]

use webrender::api::*;
use euclid::{Length, TypedRect, TypedPoint2D};
use rusttype::{Font, Scale};
use css_parser::{TextAlignment, TextOverflowBehaviour};

/// Rusttype has a certain sizing hack, I have no idea where this number comes from
/// Without this adjustment, we won't have the correct horizontal spacing
const RUSTTYPE_SIZE_HACK: f32 = 72.0 / 41.0;

const PX_TO_PT: f32 = 72.0 / 96.0;

/// Lines is responsible for layouting the lines of the rectangle to
struct Lines<'a> {
    align: TextAlignment,
    max_lines_before_overflow: usize,
    line_height: Length<f32, LayoutPixel>,
    max_horizontal_width: Length<f32, LayoutPixel>,
    font: &'a Font<'a>,
    font_size: Scale,
    origin: TypedPoint2D<f32, LayoutPixel>,
    current_line: usize,
}

#[derive(Debug)]
struct Word<'a> {
    // the original text
    pub text: &'a str,
    // glyphs, positions are relative to the first character of the word
    pub glyphs: Vec<GlyphInstance>,
    // the sum of the width of all the characters
    pub total_width: f32,
}

pub(crate) enum TextOverflow {
    /// Text is overflowing in the vertical direction
    IsOverflowing,
    /// Text is in bounds
    InBounds,
}


#[derive(Debug, Copy, Clone)]
struct HarfbuzzAdjustment(pub f32);

impl<'a> Lines<'a> {

    pub(crate) fn from_bounds(
        bounds: &TypedRect<f32, LayoutPixel>,
        alignment: TextAlignment,
        font: &'a Font<'a>,
        font_size: Length<f32, LayoutPixel>)
    -> Self
    {
        let max_lines_before_overflow = (bounds.size.height / font_size.0).floor() as usize;
        let max_horizontal_width = Length::new(bounds.size.width);

        Self {
            align: alignment,
            max_lines_before_overflow: max_lines_before_overflow,
            line_height: font_size,
            font: font,
            origin: bounds.origin,
            max_horizontal_width: max_horizontal_width,
            font_size: Scale::uniform(font_size.0),
            current_line: 0,
        }
    }

    /// NOTE: The glyphs are in the space of the bounds, not of the layer!
    /// You'd need to offset them by `bounds.origin` to get the correct position
    ///
    /// This function will only process the glyphs until they overflow
    /// (we don't process glyphs that are out of the bounds of the rectangle, since
    /// they don't get drawn anyway).
    pub(crate) fn get_glyphs(&mut self, text: &str, _overflow_behaviour: TextOverflowBehaviour) -> (Vec<GlyphInstance>, TextOverflow) {

        let font = &self.font;
        let font_size = self.font_size;
        let max_horizontal_width = self.max_horizontal_width.0;
        let max_lines_before_overflow = self.max_lines_before_overflow;

        // (1) Normalize characters, i.e. A + ^ = Â
        let text = normalize_unicode_characters(text);

        // (2) Harfbuzz pass, for getting glyph-individual character shaping offsets
        let harfbuzz_adjustments = calculate_harfbuzz_adjustments(&text, font);

        // (3) Split the text into words
        let words = split_text_into_words(&text, font, font_size);

        // (4) Align text to the left
        let (mut positioned_glyphs, line_break_offsets) = words_to_left_aligned_glyphs(words, font, font_size, max_horizontal_width, max_lines_before_overflow);

        // (5) Add the harfbuzz adjustments to the positioned glyphs
        apply_harfbuzz_adjustments(&mut positioned_glyphs, harfbuzz_adjustments);

        // (6) Knuth-Plass layout, TODO
        knuth_plass(&mut positioned_glyphs);

        // (7) Center- or right align text if necessary (modifies words)
        align_text(self.align, &mut positioned_glyphs, &line_break_offsets);

        // (8) (Optional) - Add the self.origin to all the glyphs to bring them from
        add_origin(&mut positioned_glyphs, self.origin.x, self.origin.y);

        (positioned_glyphs, TextOverflow::InBounds)
    }
}

/// Adds the X and Y offset to each glyph in the positioned glyph
#[inline]
fn add_origin(positioned_glyphs: &mut [GlyphInstance], x: f32, y: f32) {
    for c in positioned_glyphs {
        c.point.x += x;
        c.point.y += y;
    }
}

#[inline]
fn normalize_unicode_characters(text: &str) -> String {
    // TODO: This is currently done on the whole string
    // (should it be done after split_text_into_words?)
    // TODO: THis is an expensive operation!
    use unicode_normalization::UnicodeNormalization;
    text.nfc().filter(|c| !c.is_control()).collect::<String>()
}

#[inline]
fn calculate_harfbuzz_adjustments<'a>(text: &str, font: &Font<'a>) -> Vec<HarfbuzzAdjustment> {

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

#[inline]
fn split_text_into_words<'a>(text: &'a str, font: &Font<'a>, font_size: Scale) -> Vec<Word<'a>> {

    // TODO: this will currently split the whole text (all words)
    //
    // A basic optimization would be to track whether we have words that will
    // step outside the maximum rectangle width
    //
    // I.e. only split words until the bounds of the rectangle can't contain
    // them anymore (using a rough estimation)

    let mut words = Vec::new();

    for line in text.lines() {
        for word in line.split_whitespace() {

            let mut caret = 0.0;
            let mut cur_word_length = 0.0;
            let mut glyphs_in_this_word = Vec::new();
            let mut last_glyph = None;

            for c in word.chars() {

                use rusttype::Point;

                let g = font.glyph(c).scaled(font_size);
                let id = g.id();

                if c.is_control() {
                    continue;
                }

                if let Some(last) = last_glyph {
                    caret += font.pair_kerning(font_size, last, g.id());
                }

                let g = g.positioned(Point { x: caret, y: 0.0 });
                last_glyph = Some(id);
                let horiz_advance = g.unpositioned().h_metrics().advance_width;
                caret += horiz_advance;
                cur_word_length += horiz_advance;

                glyphs_in_this_word.push(GlyphInstance {
                    index: id.0,
                    point: TypedPoint2D::new(g.position().x, g.position().y),
                })
            }

            words.push(Word {
                text: word,
                glyphs: glyphs_in_this_word,
                total_width: cur_word_length,
            })
        }
    }

    words
}

#[inline]
fn words_to_left_aligned_glyphs<'a>(
    words: Vec<Word<'a>>,
    font: &Font<'a>,
    font_size: Scale,
    max_horizontal_width: f32,
    max_lines_before_overflow: usize)
-> (Vec<GlyphInstance>, Vec<(usize, f32)>)
{
    // left_aligned_glyphs stores the X and Y coordinates of the positioned glyphs,
    // left-aligned
    let mut left_aligned_glyphs = Vec::<GlyphInstance>::new();

    // The line break offsets (neded for center- / right-aligned text contains:
    //
    // - The index of the glyph at which the line breaks
    // - How much space each line has (to the right edge of the containing rectangle)
    let mut line_break_offsets = Vec::<(usize, f32)>::new();

    let v_metrics_scaled = font.v_metrics(font_size);
    let v_advance_scaled = v_metrics_scaled.ascent - v_metrics_scaled.descent + v_metrics_scaled.line_gap;

    let offset_top = v_metrics_scaled.ascent;

    // In order to space between words, we need to
    let space_width = font.glyph(' ').scaled(font_size).h_metrics().advance_width;

    // word_caret is the current X position of the "pen" we are writing with
    let mut word_caret = 0.0;
    let mut current_line_num = 0;

    for word in words {

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
    }

    // push the infos about the last line
    line_break_offsets.push((left_aligned_glyphs.len() - 1, max_horizontal_width - word_caret));

    (left_aligned_glyphs, line_break_offsets)
}

#[inline]
fn apply_harfbuzz_adjustments(positioned_glyphs: &mut [GlyphInstance], harfbuzz_adjustments: Vec<HarfbuzzAdjustment>) {
    // TODO
}

#[inline]
fn knuth_plass(positioned_glyphs: &mut [GlyphInstance]) {
    // TODO
}

#[inline]
fn align_text(alignment: TextAlignment, glyphs: &mut Vec<GlyphInstance>, line_breaks: &[(usize, f32)]) {

    use css_parser::TextAlignment::*;

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

    if alignment == TextAlignment::Left {
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

#[inline]
pub(crate) fn put_text_in_bounds<'a>(
    text: &str,
    font: &Font<'a>,
    font_size: Length<f32, LayoutPixel>,
    alignment: TextAlignment,
    overflow_behaviour: TextOverflowBehaviour,
    bounds: &TypedRect<f32, LayoutPixel>)
-> Vec<GlyphInstance>
{
    let mut lines = Lines::from_bounds(bounds, alignment, font, font_size * RUSTTYPE_SIZE_HACK * PX_TO_PT);
    let (glyphs, overflow) = lines.get_glyphs(text, overflow_behaviour);
    glyphs
}