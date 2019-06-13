#![allow(unused_variables, dead_code)]

use azul_css::{LayoutSize, LayoutRect, LayoutPoint};
pub use azul_core::{
    app_resources::{
        Words, Word, WordType, GlyphInfo, GlyphPosition,
        ScaledWords, ScaledWord, WordIndex, GlyphIndex, LineLength, IndexOfLineBreak,
        RemainingSpaceToRight, LineBreaks, WordPositions, LayoutedGlyphs,
        ClusterIterator, ClusterInfo,
    },
    display_list::GlyphInstance,
    ui_solver::{ResolvedTextLayoutOptions, TextLayoutOptions, InlineTextLayout},
};
pub(crate) use azul_core::ui_solver::{
    DEFAULT_LINE_HEIGHT, DEFAULT_WORD_SPACING, DEFAULT_LETTER_SPACING, DEFAULT_TAB_WIDTH,
};

/// Whether the text overflows the parent rectangle, and if yes, by how many pixels,
/// necessary for determining if / how to show a scrollbar + aligning / centering text.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum TextOverflow {
    /// Text is overflowing, by how much (in pixels)?
    IsOverflowing(f32),
    /// Text is in bounds, how much space is available until the edge of the rectangle (in pixels)?
    InBounds(f32),
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
    font_bytes: &[u8],
    font_index: u32,
    font_size_px: f32,
) -> ScaledWords {

    use text_shaping::{self, HB_SCALE_FACTOR, HbBuffer, HbFont, HbScaledFont};
    use std::mem;
    use std::char;

    let hb_font = HbFont::from_bytes(font_bytes, font_index);
    let hb_scaled_font = HbScaledFont::from_font(&hb_font, font_size_px);

    // Get the dimensions of the space glyph
    let hb_space_buffer = HbBuffer::from_str(" ");
    let hb_shaped_space = text_shaping::shape_word_hb(&hb_space_buffer, &hb_scaled_font);
    let space_advance_px = hb_shaped_space.glyph_positions[0].x_advance as f32 / HB_SCALE_FACTOR;
    let space_codepoint = hb_shaped_space.glyph_infos[0].codepoint;

    let internal_str = words.internal_str.replace(char::is_whitespace, " ");

    let hb_buffer_entire_paragraph = HbBuffer::from_str(&internal_str);
    let hb_shaped_entire_paragraph = text_shaping::shape_word_hb(&hb_buffer_entire_paragraph, &hb_scaled_font);

    let mut shaped_word_positions = Vec::<Vec<GlyphPosition>>::new();
    let mut shaped_word_infos = Vec::<Vec<GlyphInfo>>::new();
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
            // azul-core::GlyphInfo and hb_position_t have the same size / layout
            // (both are repr(C)), so it's safe to just transmute them here
            current_word_positions.push(unsafe { mem::transmute(glyph_position) });
            current_word_infos.push(unsafe { mem::transmute(glyph_info) });
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

            let hb_glyph_positions = shaped_word_positions.get(word_idx)?.clone();
            let hb_glyph_infos = shaped_word_infos.get(word_idx)?.clone();
            let hb_word_width = text_shaping::get_word_visual_width_hb(&hb_glyph_positions);

            longest_word_width = longest_word_width.max(hb_word_width.abs());

            Some(ScaledWord {
                glyph_infos: hb_glyph_infos,
                glyph_positions: hb_glyph_positions,
                word_width: hb_word_width,
            })
        }).collect();

    ScaledWords {
        font_size_px,
        baseline_px: font_size_px, // TODO!
        items: scaled_words,
        longest_word_width: longest_word_width,
        space_advance_px,
        space_codepoint,
    }
}

/// Positions the words on the screen (does not layout any glyph positions!), necessary for estimating
/// the intrinsic width + height of the text content.
pub fn position_words(
    words: &Words,
    scaled_words: &ScaledWords,
    text_layout_options: &ResolvedTextLayoutOptions,
) -> WordPositions {

    use self::WordType::*;
    use std::f32;

    let font_size_px = text_layout_options.font_size_px;
    let space_advance = scaled_words.space_advance_px;
    let word_spacing_px = space_advance * text_layout_options.word_spacing.unwrap_or(DEFAULT_WORD_SPACING);
    let line_height_px = space_advance * text_layout_options.line_height.unwrap_or(DEFAULT_LINE_HEIGHT);
    let tab_width_px = space_advance * text_layout_options.tab_width.unwrap_or(DEFAULT_TAB_WIDTH);
    let letter_spacing_px = text_layout_options.letter_spacing.unwrap_or(DEFAULT_LETTER_SPACING);

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
            &text_layout_options.holes[..],
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
        line_caret_x += leading;
        advance_caret!(line_caret_x);
    }

    // NOTE: word_idx increases only on words, not on other symbols!
    let mut word_idx = 0;

    macro_rules! handle_word {() => ({

        let scaled_word = match scaled_words.items.get(word_idx) {
            Some(s) => s,
            None => continue,
        };

        let reserved_letter_spacing_px = match text_layout_options.letter_spacing {
            None => 0.0,
            Some(spacing_multiplier) => spacing_multiplier * scaled_word.number_of_clusters().saturating_sub(1) as f32,
        };

        // Calculate where the caret would be for the next word
        let word_advance_x = scaled_word.word_width + reserved_letter_spacing_px;

        let mut new_caret_x = line_caret_x + word_advance_x;

        // NOTE: Slightly modified "advance_caret!(new_caret_x);" - due to line breaking behaviour

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

        // If there was a line break, the position needs to be determined after the line break happened
        if is_line_break {
            let line_caret_y = get_line_y_position(line_number, font_size_px, line_height_px);
            word_positions.push(LayoutPoint::new(line_caret_x, line_caret_y));
            // important! - if the word is pushed onto the next line, the caret has to be
            // advanced by that words width!
            line_caret_x += word_advance_x;
        }

        // NOTE: Word index is increased before pushing, since word indices are 1-indexed
        // (so that paragraphs can be selected via "(0..word_index)").
        word_idx += 1;
        current_word_idx = word_idx;
    })}

    // The last word is a bit special: Any text must have at least one line break!
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
        if word.word_type == Word {
            handle_word!();
        }
        line_breaks.push((current_word_idx, line_caret_x));
    }

    let trailing = line_caret_x;
    let number_of_lines = line_number + 1;
    let number_of_words = current_word_idx + 1;

    let longest_line_width = line_breaks.iter().map(|(_word_idx, line_length)| *line_length).fold(0.0_f32, f32::max);
    let content_size_y = get_line_y_position(line_number, font_size_px, line_height_px);
    let content_size_x = text_layout_options.max_horizontal_width.unwrap_or(longest_line_width);
    let content_size = LayoutSize::new(content_size_x, content_size_y);

    WordPositions {
        text_layout_options: text_layout_options.clone(),
        trailing,
        number_of_words,
        number_of_lines,
        content_size,
        word_positions,
        line_breaks,
    }
}

/// Returns the (left-aligned!) bounding boxes of the indidividual text lines
pub fn word_positions_to_inline_text_layout(
    word_positions: &WordPositions,
    scaled_words: &ScaledWords
) -> InlineTextLayout {

    use azul_core::ui_solver::InlineTextLine;

    let font_size_px = word_positions.text_layout_options.font_size_px;
    let space_advance = scaled_words.space_advance_px;
    let line_height_px = space_advance * word_positions.text_layout_options.line_height.unwrap_or(DEFAULT_LINE_HEIGHT);
    let content_width = word_positions.content_size.width;

    let mut last_word_index = 0;

    InlineTextLayout {
        lines: word_positions.line_breaks
            .iter()
            .enumerate()
            .map(|(line_number, (word_idx, line_length))| {
                let start_word_idx = last_word_index;
                let line = InlineTextLine {
                    bounds: LayoutRect {
                        origin: LayoutPoint { x: 0.0, y: get_line_y_position(line_number, font_size_px, line_height_px) },
                        size: LayoutSize { width: *line_length, height: font_size_px },
                    },
                    word_start: start_word_idx,
                    word_end: *word_idx,
                };
                last_word_index = *word_idx;
                line
        }).collect(),
    }
}

pub fn get_layouted_glyphs(
    word_positions: &WordPositions,
    scaled_words: &ScaledWords,
    inline_text_layout: &InlineTextLayout,
    origin: LayoutPoint
) -> LayoutedGlyphs {

    use text_shaping;

    let letter_spacing_px = word_positions.text_layout_options.letter_spacing.unwrap_or(0.0);
    let mut all_glyphs = Vec::with_capacity(scaled_words.items.len());

    for line in inline_text_layout.lines.iter() {

        let line_x = line.bounds.origin.x;
        let line_y = line.bounds.origin.y;

        let scaled_words_in_this_line = &scaled_words.items[line.word_start..line.word_end];
        let word_positions_in_this_line = &word_positions.word_positions[line.word_start..line.word_end];

        for (scaled_word, word_position) in scaled_words_in_this_line.iter().zip(word_positions_in_this_line.iter()) {
            let mut glyphs = text_shaping::get_glyph_instances_hb(&scaled_word.glyph_infos, &scaled_word.glyph_positions);
            for (glyph, cluster_info) in glyphs.iter_mut().zip(scaled_word.cluster_iter()) {
                glyph.point.x += origin.x + line_x + word_position.x + (letter_spacing_px * cluster_info.cluster_idx as f32);
                glyph.point.y += origin.y + line_y;
            }

            all_glyphs.append(&mut glyphs);
        }
    }

    LayoutedGlyphs { glyphs: all_glyphs }
}

pub fn word_item_is_return(item: &Word) -> bool {
    item.word_type == WordType::Return
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

/// Check if the caret intersects with any holes and if yes, if the cursor should move to a new line.
///
/// # Inputs
///
/// - `line_caret_x`: The current horizontal caret position
/// - `line_number`: The current line number
/// - `holes`: Whether the text should respect any rectangular regions
///    where the text can't flow (preparation for inline / float layout).
/// - `max_width`: Does the text have a restriction on how wide it can be (in pixels)
fn caret_intersects_with_holes(
    line_caret_x: f32,
    line_number: usize,
    font_size_px: f32,
    line_height_px: f32,
    holes: &[LayoutRect],
    max_width: Option<f32>,
) -> LineCaretIntersection {

    let mut new_line_caret_x = None;
    let mut line_advance = 0;

    // If the caret is outside of the max_width, move it to the start of a new line
    if let Some(max_width) = max_width {
        if line_caret_x > max_width {
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
                if hole.origin.x + hole.size.width >= max_width {
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
    let max_width = Some(200.0);
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
    let max_width = Some(400.0);
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
    let max_width = Some(400.0);
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
