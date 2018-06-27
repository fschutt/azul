#![allow(unused_variables, dead_code)]

use resources::AppResources;
use display_list::TextInfo;
use webrender::api::*;
use euclid::{Length, TypedRect, TypedSize2D, TypedPoint2D};
use rusttype::{Font, Scale, GlyphId};
use css_parser::{TextAlignmentHorz, FontSize, BackgroundColor, Font as FontId, TextAlignmentVert, LineHeight, LayoutOverflow};

/// Rusttype has a certain sizing hack, I have no idea where this number comes from
/// Without this adjustment, we won't have the correct horizontal spacing
pub(crate) const RUSTTYPE_SIZE_HACK: f32 = 72.0 / 41.0;

pub(crate) const PX_TO_PT: f32 = 72.0 / 96.0;

#[derive(Debug, Clone)]
pub(crate) struct Word {
    /// The original text. TODO: Move this out of here,
    /// this field gets unnecessarily cloned
    pub text: String,
    /// Glyphs, positions are relative to the first character of the word
    pub glyphs: Vec<GlyphInstance>,
    /// The sum of the width of all the characters
    pub total_width: f32,
}

#[derive(Debug, Clone)]
pub(crate) enum SemanticWordItem {
    /// Encountered a word (delimited by spaces)
    Word(Word),
    // `\t` or `x09`
    Tab,
    /// `\r`, `\n` or `\r\n`, escaped: `\x0D`, `\x0A` or `\x0D\x0A`
    Return,
}

impl SemanticWordItem {
    pub fn is_return(&self) -> bool {
        use self::SemanticWordItem::*;
        match self {
            Return => true,
            _ => false,
        }
    }
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

impl TextOverflow {
    pub fn is_overflowing(&self) -> bool {
        use self::TextOverflow::*;
        match self {
            IsOverflowing(_) => true,
            InBounds(_) => false,
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct HarfbuzzAdjustment(pub f32);

#[derive(Debug, Copy, Clone)]
struct KnuthPlassAdjustment(pub f32);

/// Holds info necessary for layouting / styling scrollbars
#[derive(Debug, Clone)]
pub(crate) struct ScrollbarInfo {
    /// Total width (for vertical scrollbars) or height (for horizontal scrollbars)
    /// of the scrollbar in pixels
    pub(crate) width: usize,
    /// Padding of the scrollbar, in pixels. The inner bar is `width - padding` pixels wide.
    pub(crate) padding: usize,
    /// Style of the scrollbar (how to draw it)
    pub(crate) bar_color: BackgroundColor,
    /// How to draw the "up / down" arrows
    pub(crate) triangle_color: BackgroundColor,
    /// Style of the scrollbar background
    pub(crate) background_color: BackgroundColor,
}

/// Temporary struct so I don't have to pass the three parameters around seperately all the time
#[derive(Debug, Copy, Clone)]
struct FontMetrics {
    /// Width of the space character
    space_width: f32,
    /// Usually 4 * space_width
    tab_width: f32,
    /// font_size * line_height
    vertical_advance: f32,
    /// Offset of the font from the top of the bounding rectangle
    offset_top: f32,
}

// TODO: hacky hacky shit. Seperate the text itself from the representation
// so we don't have to clone the strings when we change or zoom the font
fn get_string_from_words(words: &[SemanticWordItem]) -> String {
    use self::SemanticWordItem::*;
    let mut target = String::with_capacity(words.len());
    for word in words {
        match word {
            Word(w) => target += &w.text,
            Tab => target.push('\t'),
            Return => target.push('\n'),
        }
    }
    target
}

/// ## Inputs
///
/// - `app_resources`: This is only used for caching - if you already have a `LargeString`, which
///    stores the word boundaries for the given font, we don't have to re-calculate the font metrics again.
/// - `bounds`: The bounds of the rectangle containing the text
/// - `horiz_alignment`: Usually parsed from the `text-align` attribute: horizontal alignment of the text
/// - `vert_alignment`: Usually parsed from the `align-items` attribute on the parent node
///    or the `align-self` on the child node: horizontal alignment of the text
/// - `font`: The font to use for layouting (only the ID)
/// - `font_size`: The font size (without line height)
/// - `line_height`: The line height (100% = 1.0). I.e. `line-height = 1.2;` scales the text vertically by 1.2x
/// - `text`: The actual text to layout. Will be unicode-normalized after the Unicode Normalization Form C
///   (canonical decomposition followed by canonical composition).
/// - `overflow`: If the scrollbars should be show, parsed from the `overflow-{x / y}` fields
/// - `scrollbar_info`: Mostly used to reserve space for the scrollbar, if necessary.
///
/// ## Returns
///
/// - `Vec<GlyphInstance>`: The layouted glyphs. If a scrollbar is necessary, they will be layouted so that
///   the scrollbar has space to the left or bottom (so it doesn't overlay the text)
/// - `TextOverflowPass2`: This is internally used for aligning text (horizontally / vertically), but
///   it is necessary for drawing the scrollbars later on, to determine the height of the bar. Contains
///   info about if the text has overflown the rectangle, and if yes, by how many pixels
///
/// ## Notes
///
/// This function is currently very expensive, since it doesn't cache the string. So it does many small
/// allocations. This should be cleaned up in the future by caching `BlobStrings` and only re-layouting
/// when it's absolutely necessary.
pub(crate) fn get_glyphs<'a>(
    app_resources: &AppResources<'a>,
    bounds: &TypedRect<f32, LayoutPixel>,
    horiz_alignment: TextAlignmentHorz,
    vert_alignment: TextAlignmentVert,
    target_font_id: &FontId,
    target_font_size: &FontSize,
    line_height: Option<LineHeight>,
    text: &TextInfo<'a>,
    overflow: &LayoutOverflow,
    scrollbar_info: &ScrollbarInfo)
-> (Vec<GlyphInstance>, TextOverflowPass2)
{
    use css_parser::{TextOverflowBehaviour, TextOverflowBehaviourInner};
    use text_cache::LargeString;

    let target_font = app_resources.font_data.get(target_font_id)
        .expect("Drawing with invalid font!");

    let target_font_size_f32 = target_font_size.0.to_pixels() * RUSTTYPE_SIZE_HACK * PX_TO_PT;
    let line_height = match line_height { Some(lh) => (lh.0).number, None => 1.0 };
    let font_size_with_line_height = Scale::uniform(target_font_size_f32 * line_height);
    let font_size_no_line_height = Scale::uniform(target_font_size_f32);
    let space_width = target_font.0.glyph(' ').scaled(font_size_no_line_height).h_metrics().advance_width;
    let tab_width = 4.0 * space_width; // TODO: make this configurable

    let v_metrics_scaled = target_font.0.v_metrics(font_size_with_line_height);
    let v_advance_scaled = v_metrics_scaled.ascent - v_metrics_scaled.descent + v_metrics_scaled.line_gap;
    let offset_top = v_metrics_scaled.ascent;

    let font_metrics = FontMetrics {
        vertical_advance: v_advance_scaled,
        space_width: space_width,
        tab_width: tab_width,
        offset_top: offset_top,
    };

    // (1) Split the text into semantic items (word, tab or newline) OR get the cached
    // text and scale it accordingly.
    //
    // This function also normalizes the unicode characters and calculates kerning.
    //
    // NOTE: This should be revisited, the caching does unnecessary cloning.
    let (word_scale_factor, mut words) = match text {
        TextInfo::Cached(text_id) => {
            match app_resources.text_cache.cached_strings.get(text_id) {
                Some(LargeString::Cached { font, size, words }) => {
                    if font == target_font_id {
                        use std::rc::Rc;
                        // If the target font is the same as the initial font, but the font size differs,
                        // all we have to do is to scale the widths of the words on the words
                        let cloned_words: Vec<SemanticWordItem> = (&*(words.clone())).clone();
                        if size == target_font_size {
                            (None, cloned_words)
                        } else {
                            (Some(target_font_size.0.to_pixels() / size.0.to_pixels()), cloned_words)
                        }
                    } else {
                        // generate new words struct based on the previous words
                        let new_words = split_text_into_words(&get_string_from_words(words), &target_font.0, font_size_no_line_height);
                        (None, new_words)
                    }
                },
                Some(LargeString::Raw(s)) => {
                    (None, split_text_into_words(s, &target_font.0, font_size_no_line_height))
                },
                None => panic!("Invalid TextId \"{:?}\" encountered in text_layout::get_glyphs", text_id),
            }
        },
        TextInfo::Uncached(s) => (None, split_text_into_words(s, &target_font.0, font_size_no_line_height)),
    };

    // Scale the horizontal width of the words to match the new font size
    // Since each word has a local origin (i.e. the first character of each word
    // is at (0, 0)), we can simply scale the X position of each glyph by a
    // certain factor.
    //
    // So if we previously had a 12pt font and now a 13pt font,
    // we simply scale each glyph position by 13 / 12. This is faster than
    // re-calculating the font metrics (from Rusttype) each time we scale a
    // large amount of text.
    if let Some(scale_factor) = word_scale_factor {
        for word in words.iter_mut() {
            if let SemanticWordItem::Word(ref mut w) = word {
                w.glyphs.iter_mut().for_each(|g| g.point.x *= scale_factor);
                w.total_width *= scale_factor;
            }
        }
    }

    // (2) Calculate the additions / subtractions that have to be take into account
    // let harfbuzz_adjustments = calculate_harfbuzz_adjustments(&text, &target_font.0);

    // (3) Determine if the words will overflow the bounding rectangle
    let overflow_pass_1 = estimate_overflow_pass_1(&words, &bounds.size, &font_metrics, &overflow);

    // (4) If the lines overflow, subtract the space needed for the scrollbars and calculate the length
    // again (TODO: already layout characters here?)
    let (new_size, overflow_pass_2) =
        estimate_overflow_pass_2(&words, &bounds.size, &font_metrics, &overflow, scrollbar_info, overflow_pass_1);

    let max_horizontal_text_width = if overflow.allows_horizontal_overflow() { None } else { Some(new_size.width) };

    // (5) Align text to the left, initial layout of glyphs
    let (mut positioned_glyphs, line_break_offsets) =
        words_to_left_aligned_glyphs(words, &target_font.0, max_horizontal_text_width, &font_metrics);

    // (6) Add the harfbuzz adjustments to the positioned glyphs
    // apply_harfbuzz_adjustments(&mut positioned_glyphs, harfbuzz_adjustments);

    // (7) Calculate the Knuth-Plass adjustments for the (now layouted) glyphs
    let knuth_plass_adjustments = calculate_knuth_plass_adjustments(&positioned_glyphs, &line_break_offsets);

    // (8) Add the Knuth-Plass adjustments to the positioned glyphs
    apply_knuth_plass_adjustments(&mut positioned_glyphs, knuth_plass_adjustments);

    // (9) Align text horizontally (early return if left-aligned)
    align_text_horz(horiz_alignment, &mut positioned_glyphs, &line_break_offsets, &overflow_pass_2);

    // (10) Align text vertically (early return if text overflows)
    align_text_vert(vert_alignment, &mut positioned_glyphs, &line_break_offsets, &overflow_pass_2);

    // (11) Add the self.origin to all the glyphs to bring them from glyph space into world space
    add_origin(&mut positioned_glyphs, bounds.origin.x, bounds.origin.y);

    (positioned_glyphs, overflow_pass_2)
}

/// This function is also used in the `text_cache` module for caching large strings.
///
/// It is one of the most expensive functions, use with care.
pub(crate) fn split_text_into_words<'a>(text: &str, font: &Font<'a>, font_size: Scale)
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
    rect_dimensions: &TypedSize2D<f32, LayoutPixel>,
    font_metrics: &FontMetrics,
    overflow: &LayoutOverflow)
-> TextOverflowPass1
{
    use self::SemanticWordItem::*;

    let FontMetrics { space_width, tab_width, vertical_advance, offset_top } = *font_metrics;

    let max_text_line_len_horizontal = 0.0;

    // Determine the maximum width and height that the text needs for layout

    // This is actually tricky. Horizontal scrollbars and vertical scrollbars
    // behave differently.
    //
    // Vertical scrollbars always show the length - when the
    // vertical length = the height of the rectangle, the scrollbar (the actual bar)
    // is 0.5x the height of the rectangle, aligned at the top.
    //
    // Horizontal scrollbars, on the other hand, are 1.0x the width of the rectangle,
    // when the width is filled.

    // TODO: this is duplicated code

    let mut max_hor_len = None;

    let vertical_length = {
        if overflow.allows_horizontal_overflow() {
            // If we can overflow horizontally, we only need to sum up the `Return`
            // characters, since the actual length of the line doesn't matter
            words.iter().filter(|w| w.is_return()).count() as f32 * vertical_advance
        } else {
            // TODO: should this be cached? The calculation is probably quick, but this
            // is essentially the same thing as we do in the actual text layout stage
            let mut max_line_cursor: f32 = 0.0;
            let mut cur_line_cursor = 0.0;
            let mut cur_line = 0;

            for w in words {
                match w {
                    Word(w) => {
                        if cur_line_cursor + w.total_width > rect_dimensions.width {
                            max_line_cursor = max_line_cursor.max(cur_line_cursor);
                            cur_line_cursor = 0.0;
                            cur_line += 1;
                        }
                        cur_line_cursor += w.total_width + space_width;
                    },
                    // TODO: also check for rect break after tabs? Kinda pointless, isn't it?
                    Tab => cur_line_cursor += tab_width,
                    Return => {
                        max_line_cursor = max_line_cursor.max(cur_line_cursor);
                        cur_line_cursor = 0.0;
                        cur_line += 1;
                    }
                }
            }

            max_hor_len = Some(cur_line_cursor);

            let cur_vertical = (cur_line as f32 * vertical_advance) + offset_top;

            cur_vertical
        }
    };

    let vertical_length = if vertical_length > rect_dimensions.height {
        TextOverflow::IsOverflowing(vertical_length - rect_dimensions.height)
    } else {
        TextOverflow::InBounds(rect_dimensions.height - vertical_length)
    };

    let horizontal_length = {

        let horz_max = if overflow.allows_horizontal_overflow() {

            let mut cur_line_cursor = 0.0;
            let mut max_line_cursor: f32 = 0.0;

            for w in words {
                match w {
                    Word(w) => cur_line_cursor += w.total_width,
                    Tab => cur_line_cursor += tab_width,
                    Return => {
                        max_line_cursor = max_line_cursor.max(cur_line_cursor);
                        cur_line_cursor = 0.0;
                    }
                }
            }

            max_line_cursor
        } else {
           max_hor_len.unwrap()
        };

        if horz_max > rect_dimensions.width {
            TextOverflow::IsOverflowing(horz_max - rect_dimensions.width)
        } else {
            TextOverflow::InBounds(rect_dimensions.width - horz_max)
        }
    };

    TextOverflowPass1 {
        horizontal: horizontal_length,
        vertical: vertical_length,
    }
}

#[inline(always)]
fn estimate_overflow_pass_2(
    words: &[SemanticWordItem],
    rect_dimensions: &TypedSize2D<f32, LayoutPixel>,
    font_metrics: &FontMetrics,
    overflow: &LayoutOverflow,
    scrollbar_info: &ScrollbarInfo,
    pass1: TextOverflowPass1)
-> (TypedSize2D<f32, LayoutPixel>, TextOverflowPass2)
{
    let FontMetrics { space_width, tab_width, vertical_advance, offset_top } = *font_metrics;

    let mut new_size = *rect_dimensions;

    // TODO: make this 10px stylable

    // Subtract the space necessary for the scrollbars from the rectangle
    //
    // NOTE: this is switched around - if the text overflows vertically, the
    // scrollbar gets shown on the right edge, so we need to subtract from the
    // **width** of the rectangle.

    if pass1.horizontal.is_overflowing() {
        new_size.height -= scrollbar_info.width as f32;
    }

    if pass1.vertical.is_overflowing() {
        new_size.width -= scrollbar_info.width as f32;
    }

    // If the words are not overflowing, just take the result from the first pass
    let recalc_scrollbar_info = if pass1.horizontal.is_overflowing() || pass1.vertical.is_overflowing() {
        estimate_overflow_pass_1(words, &new_size, font_metrics, overflow)
    } else {
        pass1
    };

    (new_size, TextOverflowPass2 {
        horizontal: recalc_scrollbar_info.horizontal,
        vertical: recalc_scrollbar_info.vertical,
    })
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

/// If `max_horizontal_width` is `None`, it means that the text is allowed to overflow
/// the rectangle horizontally
#[inline(always)]
fn words_to_left_aligned_glyphs<'a>(
    words: Vec<SemanticWordItem>,
    font: &Font<'a>,
    max_horizontal_width: Option<f32>,
    font_metrics: &FontMetrics)
-> (Vec<GlyphInstance>, Vec<(usize, f32)>)
{
    let FontMetrics { space_width, tab_width, vertical_advance, offset_top } = *font_metrics;

    // left_aligned_glyphs stores the X and Y coordinates of the positioned glyphs,
    // left-aligned
    let mut left_aligned_glyphs = Vec::<GlyphInstance>::new();

    enum WordCaretMax {
        SomeMaxWidth(f32),
        NoMaxWidth(f32),
    }

    // The line break offsets (neded for center- / right-aligned text contains:
    //
    // - The index of the glyph at which the line breaks
    // - How much space each line has (to the right edge of the containing rectangle)
    let mut line_break_offsets = Vec::<(usize, WordCaretMax)>::new();

    // word_caret is the current X position of the "pen" we are writing with
    let mut word_caret = 0.0;
    let mut current_line_num = 0;
    let mut max_word_caret = 0.0;

    for word in words {
        use self::SemanticWordItem::*;
        match word {
            Word(word) => {
                let text_overflows_rect = match max_horizontal_width {
                    Some(max) => word_caret + word.total_width > max,
                    // If we don't have a maximum horizontal width, the text can overflow the
                    // bounding rectangle in the horizontal direction
                    None => false,
                };

                if text_overflows_rect {
                    let space_until_horz_return = match max_horizontal_width {
                        Some(s) => WordCaretMax::SomeMaxWidth(s - word_caret),
                        None => WordCaretMax::NoMaxWidth(word_caret),
                    };
                    line_break_offsets.push((left_aligned_glyphs.len() - 1, space_until_horz_return));
                    if word_caret > max_word_caret {
                        max_word_caret = word_caret;
                    }
                    word_caret = 0.0;
                    current_line_num += 1;
                }

                for mut glyph in word.glyphs {
                    let push_x = word_caret;
                    let push_y = (current_line_num as f32 * vertical_advance) + offset_top;
                    glyph.point.x += push_x;
                    glyph.point.y += push_y;
                    left_aligned_glyphs.push(glyph);
                }

                // Add the word width to the current word_caret
                word_caret += word.total_width + space_width;
            },
            Tab => {
                word_caret += tab_width;
            },
            Return => {
                // TODO: dupliated code
                let space_until_horz_return = match max_horizontal_width {
                    Some(s) => WordCaretMax::SomeMaxWidth(s - word_caret),
                    None => WordCaretMax::NoMaxWidth(word_caret),
                };
                line_break_offsets.push((left_aligned_glyphs.len() - 1, space_until_horz_return));
                if word_caret > max_word_caret {
                    max_word_caret = word_caret;
                }
                word_caret = 0.0;
                current_line_num += 1;
            },
        }
    }

    // push the infos about the last line
    if !left_aligned_glyphs.is_empty() {
        let space_until_horz_return = match max_horizontal_width {
            Some(s) => WordCaretMax::SomeMaxWidth(s - word_caret),
            None => WordCaretMax::NoMaxWidth(word_caret),
        };
        line_break_offsets.push((left_aligned_glyphs.len() - 1, space_until_horz_return));
        if word_caret > max_word_caret {
            max_word_caret = word_caret;
        }
    }

    let line_break_offsets = line_break_offsets.into_iter().map(|(line, space_r)| {
        let space_r = match space_r {
            WordCaretMax::SomeMaxWidth(s) => s,
            WordCaretMax::NoMaxWidth(word_caret) => max_word_caret - word_caret,
        };
        (line, space_r)
    }).collect();

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

    let multiply_factor = match alignment {
        Left => { return; },
        Center => 0.5, // move the line by the half width
        Right => 1.0, // move the line by the full width
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

    use self::TextOverflow::*;
    use self::TextAlignmentVert::*;

    assert!(glyphs.len() - 1 == line_breaks[line_breaks.len() - 1].0);

    let multiply_factor = match alignment {
        Top => return,
        Center => 0.5,
        Bottom => 1.0,
    };

    let space_to_add = match overflow.vertical {
        IsOverflowing(_) => return,
        InBounds(s) => s * multiply_factor,
    };

    glyphs.iter_mut().for_each(|g| g.point.y += space_to_add);
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