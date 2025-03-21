//! Contains functions for breaking a string into words, calculate
//! the positions of words / lines and do glyph positioning

use alloc::{string::String, vec::Vec};

use azul_core::ui_solver::TextJustification;
pub use azul_core::{
    app_resources::{
        FontMetrics, GlyphIndex, IndexOfLineBreak, LayoutedGlyphs, LineBreaks, LineLength,
        RemainingSpaceToRight, ShapedWord, ShapedWords, Word, WordIndex, WordPosition,
        WordPositions, WordType, Words,
    },
    callbacks::InlineText,
    display_list::GlyphInstance,
    ui_solver::{
        InlineTextLayout, InlineTextLine, LayoutDebugMessage, ResolvedTextLayoutOptions,
        ScriptType, TextJustification, TextLayoutOptions, TextScriptInfo, DEFAULT_LETTER_SPACING,
        DEFAULT_LINE_HEIGHT, DEFAULT_TAB_WIDTH, DEFAULT_WORD_SPACING,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
pub use azul_css::FontRef;

pub use super::shaping::ParsedFont;
use super::{shaping::ShapedTextBufferUnsized, FontImpl};

/// Creates a font from a font file (TTF, OTF, WOFF, etc.)
///
/// NOTE: EXPENSIVE function, needs to parse tables, etc.
pub fn parse_font(
    font_bytes: &[u8],
    font_index: usize,
    parse_outlines: bool,
) -> Option<ParsedFont> {
    ParsedFont::from_bytes(font_bytes, font_index, parse_outlines)
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

    for (ch_idx, ch) in normalized_chars.iter().enumerate() {
        let ch = *ch;
        let current_char_is_whitespace = ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n';

        let should_push_delimiter = match ch {
            ' ' => Some(Word {
                start: last_char_idx + 1,
                end: ch_idx + 1,
                word_type: WordType::Space,
            }),
            '\t' => Some(Word {
                start: last_char_idx + 1,
                end: ch_idx + 1,
                word_type: WordType::Tab,
            }),
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
            }
            _ => None,
        };

        // Character is a whitespace or the character is the
        // last character in the text (end of text)
        let should_push_word = if current_char_is_whitespace && !last_char_was_whitespace {
            Some(Word {
                start: current_word_start,
                end: ch_idx,
                word_type: WordType::Word,
            })
        } else {
            None
        };

        if current_char_is_whitespace {
            current_word_start = ch_idx + 1;
        }

        let mut push_words = |arr: [Option<Word>; 2]| {
            words.extend(arr.iter().filter_map(|e| e.clone())); // TODO: perf!
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
            word_type: WordType::Word,
        });
    }

    // If the last item is a `Return`, remove it
    if let Some(Word {
        word_type: WordType::Return,
        ..
    }) = words.last()
    {
        words.pop();
    }

    Words {
        items: words.into(),
        internal_str: normalized_string.into(),
        internal_chars: normalized_chars.iter().map(|c| *c as u32).collect(),
        is_rtl: false,
    }
}

/// Enhanced version of split_text_into_words that incorporates hyphenation
pub fn split_text_into_words_with_hyphenation(
    text: &str,
    options: &ResolvedTextLayoutOptions,
    hyphenation_cache: &HyphenationCache,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Words {
    use unicode_normalization::UnicodeNormalization;

    // Add debug message if enabled
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Processing text: {}", text).into(),
            location: "split_text_into_words_with_hyphenation".to_string().into(),
        });
    }

    // Normalize text (same as original function)
    let normalized_string = text.nfc().collect::<String>();
    let normalized_chars = normalized_string.chars().collect::<Vec<char>>();

    let mut words = Vec::new();
    let mut current_word_start = 0;
    let mut last_char_idx = 0;
    let mut last_char_was_whitespace = false;

    // Detect if the text is RTL
    let is_rtl = if options.is_rtl != ScriptType::Mixed {
        options.is_rtl == ScriptType::RTL
    } else {
        let direction = detect_text_direction(text);
        direction == ScriptType::RTL
    };

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Text direction detected as: {}",
                if is_rtl { "RTL" } else { "LTR" }
            )
            .into(),
            location: "split_text_into_words_with_hyphenation".to_string().into(),
        });
    }

    for (ch_idx, ch) in normalized_chars.iter().enumerate() {
        let ch = *ch;
        let current_char_is_whitespace = ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n';

        let should_push_delimiter = match ch {
            ' ' => Some(Word {
                start: last_char_idx + 1,
                end: ch_idx + 1,
                word_type: WordType::Space,
            }),
            '\t' => Some(Word {
                start: last_char_idx + 1,
                end: ch_idx + 1,
                word_type: WordType::Tab,
            }),
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
            }
            _ => None,
        };

        // Character is a whitespace or the character is the last character in the text (end of
        // text)
        let should_push_word = if current_char_is_whitespace && !last_char_was_whitespace {
            Some(Word {
                start: current_word_start,
                end: ch_idx,
                word_type: WordType::Word,
            })
        } else {
            None
        };

        if current_char_is_whitespace {
            current_word_start = ch_idx + 1;
        }

        let mut push_words = |arr: [Option<Word>; 2]| {
            words.extend(arr.iter().filter_map(|e| e.clone())); // TODO: perf!
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
            word_type: WordType::Word,
        });
    }

    // If the last item is a `Return`, remove it
    if let Some(Word {
        word_type: WordType::Return,
        ..
    }) = words.last()
    {
        words.pop();
    }

    // Add hyphenation points for words if enabled
    if options.can_hyphenate {
        let hyphenator = hyphenation_cache.get_hyphenator("en");

        if let Some(hyphenator) = hyphenator {
            let mut hyphenated_words = Vec::new();

            for word in words {
                if word.word_type == WordType::Word {
                    let word_text = &normalized_string[word.start..word.end];
                    let hyphenation_points = find_hyphenation_points(word_text, hyphenator);

                    if !hyphenation_points.is_empty() && word_text.len() > 4 {
                        if let Some(messages) = debug_messages {
                            messages.push(LayoutDebugMessage {
                                message: format!(
                                    "Hyphenation points for '{}': {:?}",
                                    word_text, hyphenation_points
                                )
                                .into(),
                                location: "split_text_into_words_with_hyphenation"
                                    .to_string()
                                    .into(),
                            });
                        }

                        // Add hyphenation metadata to the word
                        let word_with_hyphenation = Word {
                            start: word.start,
                            end: word.end,
                            word_type: WordType::WordWithHyphenation(hyphenation_points.into()),
                        };

                        hyphenated_words.push(word_with_hyphenation);
                    } else {
                        hyphenated_words.push(word);
                    }
                } else {
                    hyphenated_words.push(word);
                }
            }

            words = hyphenated_words;
        }
    }

    // Create and return Words struct
    Words {
        items: words.into(),
        internal_str: normalized_string.into(),
        internal_chars: normalized_chars.iter().map(|c| *c as u32).collect(),
        is_rtl,
    }
}

/// Takes a text broken into semantic items and shape all the words
/// (does NOT scale the words, only shapes them)
pub fn shape_words<F: FontImpl>(words: &Words, font: &F) -> ShapedWords {
    let (script, lang) = super::shaping::estimate_script_and_language(&words.internal_str);

    // Get the dimensions of the space glyph
    let space_advance = font
        .get_space_width()
        .unwrap_or(font.get_font_metrics().units_per_em as usize);

    let mut longest_word_width = 0_usize;

    // NOTE: This takes the longest part of the entire layout process -- NEED TO PARALLELIZE
    let shaped_words = words
        .items
        .iter()
        .filter(|w| w.word_type == WordType::Word)
        .map(|word| {
            let chars = &words.internal_chars.as_ref()[word.start..word.end];
            let shaped_word = font.shape(chars, script, lang);
            let word_width = shaped_word.get_word_visual_width_unscaled();

            longest_word_width = longest_word_width.max(word_width);

            let ShapedTextBufferUnsized { infos } = shaped_word;

            ShapedWord {
                glyph_infos: infos.into(),
                word_width,
            }
        })
        .collect();

    let font_metrics = font.get_font_metrics();

    ShapedWords {
        items: shaped_words,
        longest_word_width,
        space_advance,
        font_metrics_units_per_em: font_metrics.units_per_em,
        font_metrics_ascender: font_metrics.ascender,
        font_metrics_descender: font_metrics.descender,
        font_metrics_line_gap: font_metrics.line_gap,
    }
}

/// Positions the words on the screen (does not layout any glyph positions!), necessary for
/// estimating the intrinsic width + height of the text content.
pub fn position_words(
    words: &Words,
    shaped_words: &ShapedWords,
    text_layout_options: &ResolvedTextLayoutOptions,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> WordPositions {
    use core::f32;

    use azul_core::{app_resources::WordPosition, ui_solver::InlineTextLine};

    use self::{LineCaretIntersection::*, WordType::*};

    let font_size_px = text_layout_options.font_size_px;
    let space_advance_px = shaped_words.get_space_advance_px(text_layout_options.font_size_px);
    let word_spacing_px = space_advance_px
        * text_layout_options
            .word_spacing
            .as_ref()
            .copied()
            .unwrap_or(DEFAULT_WORD_SPACING);
    let line_height_px = space_advance_px
        * text_layout_options
            .line_height
            .as_ref()
            .copied()
            .unwrap_or(DEFAULT_LINE_HEIGHT);
    let tab_width_px = space_advance_px
        * text_layout_options
            .tab_width
            .as_ref()
            .copied()
            .unwrap_or(DEFAULT_TAB_WIDTH);
    let spacing_multiplier = text_layout_options
        .letter_spacing
        .as_ref()
        .copied()
        .unwrap_or(0.0);

    // Get hyphen width from first shaped word or use default
    let hyphen_width_px = get_hyphen_width_px(
        shaped_words,
        text_layout_options
            .hyphenation_character
            .into_option()
            .and_then(|s| char::from_u32(s))
            .unwrap_or('-'),
        text_layout_options.font_size_px,
    );

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Positioning with options: max_width={:?}, can_break={}, can_hyphenate={}, \
                 is_rtl={:?}",
                text_layout_options.max_horizontal_width,
                text_layout_options.can_break,
                text_layout_options.can_hyphenate,
                text_layout_options.is_rtl
            )
            .into(),
            location: "position_words_enhanced".to_string().into(),
        });
    }

    let mut line_breaks = Vec::new();
    let mut word_positions = Vec::new();
    let mut line_caret_x = text_layout_options.leading.as_ref().copied().unwrap_or(0.0);
    let mut line_caret_y = font_size_px + line_height_px;
    let mut shaped_word_idx = 0;
    let mut last_shaped_word_word_idx = 0;
    let mut last_line_start_idx = 0;
    let mut current_word_idx = 0;

    let is_rtl = words.is_rtl;
    let last_word_idx = words.items.len().saturating_sub(1);

    // Early exit for single-line input that can't break
    if !text_layout_options.can_break {
        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: "Text can't break - positioning as single line"
                    .to_string()
                    .into(),
                location: "position_words_enhanced".to_string().into(),
            });
        }

        let mut single_line_word_positions = position_words_single_line(
            words,
            shaped_words,
            text_layout_options,
            is_rtl,
            line_caret_x,
            line_caret_y,
        );

        // Create a single line break that encompasses all words
        let total_width = single_line_word_positions
            .iter()
            .map(|pos| pos.size.width)
            .sum::<f32>();

        line_breaks.push(InlineTextLine {
            word_start: 0,
            word_end: last_word_idx,
            bounds: LogicalRect::new(
                LogicalPosition::new(0.0, line_caret_y),
                LogicalSize::new(total_width, font_size_px + line_height_px),
            ),
        });

        return WordPositions {
            text_layout_options: text_layout_options.clone(),
            trailing: total_width,
            number_of_shaped_words: shaped_word_idx,
            number_of_lines: 1,
            content_size: LogicalSize::new(total_width, font_size_px + line_height_px),
            word_positions: single_line_word_positions,
            line_breaks,
            is_rtl,
        };
    }

    // Store words to position for current line (needed for RTL layout)
    let mut current_line_words = Vec::new();

    // Check if we should stop layout due to max_vertical_height
    let mut should_stop_layout = false;

    // The last word is a bit special: Any text must have at least one line break!
    for (word_idx, word) in words.items.iter().enumerate() {
        // Check if we've exceeded the max vertical height
        if let Some(max_y) = text_layout_options.max_vertical_height.into_option() {
            if line_caret_y > max_y {
                if let Some(messages) = debug_messages {
                    messages.push(LayoutDebugMessage {
                        message: format!(
                            "Reached max vertical height ({}) - stopping layout",
                            max_y
                        )
                        .into(),
                        location: "position_words_enhanced".to_string().into(),
                    });
                }
                should_stop_layout = true;
                break;
            }
        }

        match &word.word_type {
            Word | WordWithHyphenation(_) => {
                // shaped words only contains the actual shaped words, not spaces / tabs / return
                // chars
                let shaped_word = match shaped_words.items.get(shaped_word_idx) {
                    Some(s) => s,
                    None => continue,
                };

                let letter_spacing_px =
                    spacing_multiplier * shaped_word.number_of_glyphs().saturating_sub(1) as f32;

                // Calculate where the caret would be for the next word
                let shaped_word_width = shaped_word.get_word_width(
                    shaped_words.font_metrics_units_per_em,
                    text_layout_options.font_size_px,
                ) + letter_spacing_px;

                // Determine if a line break is necessary
                let (caret_intersection, should_hyphenate, hyphen_position) =
                    check_line_intersection(
                        line_caret_x,
                        shaped_word_width,
                        line_caret_y,
                        font_size_px + line_height_px,
                        text_layout_options.max_horizontal_width.as_ref().copied(),
                        word,
                        text_layout_options.can_hyphenate,
                        hyphen_width_px,
                        words,
                    );

                // Add to current line for RTL handling
                current_line_words.push((
                    word_idx,
                    shaped_word_idx,
                    word.clone(),
                    shaped_word_width,
                ));

                // Correct and advance the line caret position
                match caret_intersection {
                    NoLineBreak { new_x, new_y } => {
                        if !should_hyphenate {
                            // Regular word, no hyphenation needed
                            word_positions.push(WordPosition {
                                shaped_word_index: Some(shaped_word_idx),
                                position: LogicalPosition::new(line_caret_x, line_caret_y),
                                size: LogicalSize::new(
                                    shaped_word_width,
                                    font_size_px + line_height_px,
                                ),
                                hyphenated: false,
                            });
                            line_caret_x = new_x;
                            line_caret_y = new_y;
                        } else {
                            // Hyphenation required - split the word
                            if let Some(hyphen_pos) = hyphen_position {
                                if let Some(messages) = debug_messages {
                                    messages.push(LayoutDebugMessage {
                                        message: format!(
                                            "Hyphenating word at position {}",
                                            hyphen_pos
                                        )
                                        .into(),
                                        location: "position_words_enhanced".to_string().into(),
                                    });
                                }

                                // Calculate width of first part of word plus hyphen
                                let word_str = &words.internal_str[word.start..word.end];
                                let first_part_ratio = hyphen_pos as f32 / word_str.len() as f32;
                                let first_part_width = shaped_word_width * first_part_ratio;

                                // Position first part with hyphen
                                word_positions.push(WordPosition {
                                    shaped_word_index: Some(shaped_word_idx),
                                    position: LogicalPosition::new(line_caret_x, line_caret_y),
                                    size: LogicalSize::new(
                                        first_part_width + hyphen_width_px,
                                        font_size_px + line_height_px,
                                    ),
                                    hyphenated: true,
                                });

                                // Create line break
                                line_breaks.push(InlineTextLine {
                                    word_start: last_line_start_idx,
                                    word_end: word_idx.saturating_sub(1).max(last_line_start_idx),
                                    bounds: LogicalRect::new(
                                        LogicalPosition::new(0.0, line_caret_y),
                                        LogicalSize::new(
                                            line_caret_x + first_part_width + hyphen_width_px,
                                            font_size_px + line_height_px,
                                        ),
                                    ),
                                });

                                // Move to next line for second part of word
                                last_line_start_idx = word_idx;
                                line_caret_x = 0.0;
                                line_caret_y += font_size_px + line_height_px;

                                // Position second part of word
                                let second_part_width = shaped_word_width - first_part_width;
                                word_positions.push(WordPosition {
                                    shaped_word_index: Some(shaped_word_idx),
                                    position: LogicalPosition::new(line_caret_x, line_caret_y),
                                    size: LogicalSize::new(
                                        second_part_width,
                                        font_size_px + line_height_px,
                                    ),
                                    hyphenated: false,
                                });

                                line_caret_x = second_part_width;
                            } else {
                                // Hyphenation was requested but no good position found
                                word_positions.push(WordPosition {
                                    shaped_word_index: Some(shaped_word_idx),
                                    position: LogicalPosition::new(line_caret_x, line_caret_y),
                                    size: LogicalSize::new(
                                        shaped_word_width,
                                        font_size_px + line_height_px,
                                    ),
                                    hyphenated: false,
                                });
                                line_caret_x = new_x;
                                line_caret_y = new_y;
                            }
                        }
                    }
                    LineBreak { new_x, new_y } => {
                        // push the line break first
                        line_breaks.push(InlineTextLine {
                            word_start: last_line_start_idx,
                            word_end: word_idx.saturating_sub(1).max(last_line_start_idx),
                            bounds: LogicalRect::new(
                                LogicalPosition::new(0.0, line_caret_y),
                                LogicalSize::new(line_caret_x, font_size_px + line_height_px),
                            ),
                        });
                        last_line_start_idx = word_idx;

                        // For RTL text, we need to reposition all words in the current line
                        if is_rtl {
                            position_rtl_line(
                                &mut word_positions,
                                &current_line_words,
                                line_caret_x,
                            );
                            current_line_words.clear();
                        }

                        word_positions.push(WordPosition {
                            shaped_word_index: Some(shaped_word_idx),
                            position: LogicalPosition::new(new_x, new_y),
                            size: LogicalSize::new(
                                shaped_word_width,
                                font_size_px + line_height_px,
                            ),
                            hyphenated: false,
                        });
                        line_caret_x = new_x + shaped_word_width; // add word width for the next word
                        line_caret_y = new_y;
                    }
                }

                shaped_word_idx += 1;
                last_shaped_word_word_idx = word_idx;
            }
            Return => {
                if word_idx != last_word_idx {
                    line_breaks.push(InlineTextLine {
                        word_start: last_line_start_idx,
                        word_end: word_idx.saturating_sub(1).max(last_line_start_idx),
                        bounds: LogicalRect::new(
                            LogicalPosition::new(0.0, line_caret_y),
                            LogicalSize::new(line_caret_x, font_size_px + line_height_px),
                        ),
                    });

                    // For RTL text, reposition words in current line
                    if is_rtl {
                        position_rtl_line(&mut word_positions, &current_line_words, line_caret_x);
                        current_line_words.clear();
                    }

                    // don't include the return char in the next line again
                    last_line_start_idx = word_idx + 1;
                }
                word_positions.push(WordPosition {
                    shaped_word_index: None,
                    position: LogicalPosition::new(line_caret_x, line_caret_y),
                    size: LogicalSize::new(0.0, font_size_px + line_height_px),
                    hyphenated: false,
                });
                if word_idx != last_word_idx {
                    line_caret_x = 0.0;
                    line_caret_y = line_caret_y + font_size_px + line_height_px;
                }
            }
            Space | Tab => {
                let x_advance = match word.word_type {
                    Space => word_spacing_px,
                    Tab => tab_width_px,
                    _ => word_spacing_px, // unreachable
                };

                let caret_intersection = LineCaretIntersection::new(
                    line_caret_x,
                    x_advance, // advance by space / tab width
                    line_caret_y,
                    font_size_px + line_height_px,
                    text_layout_options.max_horizontal_width.as_ref().copied(),
                );

                match caret_intersection {
                    NoLineBreak { new_x, new_y } => {
                        word_positions.push(WordPosition {
                            shaped_word_index: None,
                            position: LogicalPosition::new(line_caret_x, line_caret_y),
                            size: LogicalSize::new(x_advance, font_size_px + line_height_px),
                            hyphenated: false,
                        });
                        line_caret_x = new_x;
                        line_caret_y = new_y;
                    }
                    LineBreak { new_x, new_y } => {
                        // push the line break before increasing
                        if word_idx != last_word_idx {
                            line_breaks.push(InlineTextLine {
                                word_start: last_line_start_idx,
                                word_end: word_idx.saturating_sub(1).max(last_line_start_idx),
                                bounds: LogicalRect::new(
                                    LogicalPosition::new(0.0, line_caret_y),
                                    LogicalSize::new(line_caret_x, font_size_px + line_height_px),
                                ),
                            });

                            // For RTL text, reposition words in current line
                            if is_rtl {
                                position_rtl_line(
                                    &mut word_positions,
                                    &current_line_words,
                                    line_caret_x,
                                );
                                current_line_words.clear();
                            }

                            last_line_start_idx = word_idx;
                        }
                        word_positions.push(WordPosition {
                            shaped_word_index: None,
                            position: LogicalPosition::new(line_caret_x, line_caret_y),
                            size: LogicalSize::new(x_advance, font_size_px + line_height_px),
                            hyphenated: false,
                        });
                        if word_idx != last_word_idx {
                            line_caret_x = new_x; // don't add the space width here when pushing onto new line
                            line_caret_y = new_y;
                        }
                    }
                }
            }
        }

        current_word_idx = word_idx;
    }

    // Add the final line break if we haven't stopped layout early
    if !should_stop_layout {
        line_breaks.push(InlineTextLine {
            word_start: last_line_start_idx,
            word_end: last_shaped_word_word_idx,
            bounds: LogicalRect::new(
                LogicalPosition::new(0.0, line_caret_y),
                LogicalSize::new(line_caret_x, font_size_px + line_height_px),
            ),
        });

        // For RTL text, reposition words in the last line
        if is_rtl && !current_line_words.is_empty() {
            position_rtl_line(&mut word_positions, &current_line_words, line_caret_x);
        }
    }

    // Apply text justification if needed
    if text_layout_options.text_justify != TextJustification::None {
        apply_text_justification(
            &mut word_positions,
            &line_breaks,
            text_layout_options.text_justify,
            text_layout_options.max_horizontal_width.into(),
            word_spacing_px,
            debug_messages,
        );
    }

    let longest_line_width = line_breaks
        .iter()
        .map(|line| line.bounds.size.width)
        .fold(0.0_f32, f32::max);

    let content_size_y = line_breaks.len() as f32 * (font_size_px + line_height_px);
    let content_size_x = text_layout_options
        .max_horizontal_width
        .as_ref()
        .copied()
        .unwrap_or(longest_line_width);
    let content_size = LogicalSize::new(content_size_x, content_size_y);

    WordPositions {
        text_layout_options: text_layout_options.clone(),
        trailing: line_caret_x,
        number_of_shaped_words: shaped_word_idx,
        number_of_lines: line_breaks.len(),
        content_size,
        word_positions,
        line_breaks,
        is_rtl,
    }
}

// Helper function to check if line intersection with potential hyphenation
fn check_line_intersection(
    current_x: f32,
    word_width: f32,
    current_y: f32,
    line_height: f32,
    max_width: Option<f32>,
    word: &Word,
    can_hyphenate: bool,
    hyphen_width: f32,
    words: &Words,
) -> (LineCaretIntersection, bool, Option<usize>) {
    // First check if we need to consider hyphenation
    if !can_hyphenate || max_width.is_none() {
        // If hyphenation is disabled or no max width, use normal line intersection
        return (
            LineCaretIntersection::new(current_x, word_width, current_y, line_height, max_width),
            false,
            None,
        );
    }

    let max_width = max_width.unwrap();

    // Check if word fits without hyphenation
    if current_x + word_width <= max_width {
        return (
            LineCaretIntersection::new(
                current_x,
                word_width,
                current_y,
                line_height,
                Some(max_width),
            ),
            false,
            None,
        );
    }

    // Word doesn't fit, check if we're at the beginning of line
    if current_x == 0.0 {
        // At beginning of line, too long for whole line, check if we can hyphenate
        match &word.word_type {
            WordType::WordWithHyphenation(points) if !points.is_empty() => {
                // Find best hyphenation point that fits
                let word_text = &words.internal_str[word.start..word.end];
                for point in points.as_slice().iter() {
                    let portion = *point as f32 / word_text.len() as f32;
                    let partial_width = word_width * portion + hyphen_width;

                    if partial_width <= max_width {
                        // This hyphenation point fits
                        return (
                            LineCaretIntersection::NoLineBreak {
                                new_x: current_x + partial_width,
                                new_y: current_y,
                            },
                            true,
                            Some(*point as usize),
                        );
                    }
                }

                // No hyphenation point fits, place entire word
                (
                    LineCaretIntersection::NoLineBreak {
                        new_x: current_x + word_width,
                        new_y: current_y,
                    },
                    false,
                    None,
                )
            }
            _ => {
                // No hyphenation data, place entire word
                (
                    LineCaretIntersection::NoLineBreak {
                        new_x: current_x + word_width,
                        new_y: current_y,
                    },
                    false,
                    None,
                )
            }
        }
    } else {
        // Not at beginning of line, wrap to next line
        (
            LineCaretIntersection::LineBreak {
                new_x: 0.0,
                new_y: current_y + line_height,
            },
            false,
            None,
        )
    }
}

// Function to get hyphen width from shaped words
fn get_hyphen_width_px(shaped_words: &ShapedWords, hyphen_char: char, font_size_px: f32) -> f32 {
    // Default to a reasonable value if we can't determine it
    let default_width = font_size_px * 0.25;

    // Try to find hyphen width from shaped words
    if shaped_words.items.is_empty() {
        return default_width;
    }

    // Simplistic approximation - in reality, we'd need to shape the hyphen character
    // For now, use a fraction of the space advance as an estimate
    shaped_words.get_space_advance_px(font_size_px) * 0.5
}

// Position a single line of text (for non-breaking text)
fn position_words_single_line(
    words: &Words,
    shaped_words: &ShapedWords,
    text_layout_options: &ResolvedTextLayoutOptions,
    is_rtl: bool,
    start_x: f32,
    line_y: f32,
) -> Vec<WordPosition> {
    let font_size_px = text_layout_options.font_size_px;
    let space_advance_px = shaped_words.get_space_advance_px(text_layout_options.font_size_px);
    let word_spacing_px = space_advance_px
        * text_layout_options
            .word_spacing
            .as_ref()
            .copied()
            .unwrap_or(DEFAULT_WORD_SPACING);
    let line_height_px = space_advance_px
        * text_layout_options
            .line_height
            .as_ref()
            .copied()
            .unwrap_or(DEFAULT_LINE_HEIGHT);
    let tab_width_px = space_advance_px
        * text_layout_options
            .tab_width
            .as_ref()
            .copied()
            .unwrap_or(DEFAULT_TAB_WIDTH);
    let spacing_multiplier = text_layout_options
        .letter_spacing
        .as_ref()
        .copied()
        .unwrap_or(0.0);

    let mut word_positions = Vec::new();
    let mut shaped_word_idx = 0;
    let mut current_x = start_x;

    // Collect all words with their widths first
    let mut word_infos = Vec::new();

    for word in words.items.iter() {
        match &word.word_type {
            WordType::Word | WordType::WordWithHyphenation(_) => {
                let shaped_word = match shaped_words.items.get(shaped_word_idx) {
                    Some(s) => s,
                    None => continue,
                };

                let letter_spacing_px =
                    spacing_multiplier * shaped_word.number_of_glyphs().saturating_sub(1) as f32;

                let word_width = shaped_word.get_word_width(
                    shaped_words.font_metrics_units_per_em,
                    text_layout_options.font_size_px,
                ) + letter_spacing_px;

                word_infos.push((shaped_word_idx, word_width, WordType::Word));
                shaped_word_idx += 1;
            }
            WordType::Space => {
                word_infos.push((usize::MAX, word_spacing_px, WordType::Space));
            }
            WordType::Tab => {
                word_infos.push((usize::MAX, tab_width_px, WordType::Tab));
            }
            WordType::Return => {
                // Ignore returns in single-line mode
            }
        }
    }

    // Position the words (either LTR or RTL)
    if is_rtl {
        // For RTL, position from right to left
        let total_width: f32 = word_infos.iter().map(|(_, width, _)| *width).sum();
        let mut current_x = start_x + total_width;

        for (shaped_idx, width, word_type) in word_infos {
            current_x -= width;

            word_positions.push(WordPosition {
                shaped_word_index: if shaped_idx != usize::MAX {
                    Some(shaped_idx)
                } else {
                    None
                },
                position: LogicalPosition::new(current_x, line_y),
                size: LogicalSize::new(width, font_size_px + line_height_px),
                hyphenated: false,
            });
        }
    } else {
        // For LTR, position from left to right
        for (shaped_idx, width, word_type) in word_infos {
            word_positions.push(WordPosition {
                shaped_word_index: if shaped_idx != usize::MAX {
                    Some(shaped_idx)
                } else {
                    None
                },
                position: LogicalPosition::new(current_x, line_y),
                size: LogicalSize::new(width, font_size_px + line_height_px),
                hyphenated: false,
            });

            current_x += width;
        }
    }

    word_positions
}

// Reposition words in an RTL line
fn position_rtl_line(
    word_positions: &mut Vec<WordPosition>,
    line_words: &[(usize, usize, Word, f32)],
    line_width: f32,
) {
    if line_words.is_empty() {
        return;
    }

    // Calculate total width of the line
    let total_width: f32 = line_words.iter().map(|(_, _, _, width)| *width).sum();

    // Find indices of words in this line in the word_positions vector
    let start_idx = word_positions.len() - line_words.len();
    let end_idx = word_positions.len();

    // Calculate right edge position
    let right_edge = line_width;

    // Reposition each word from right to left
    let mut current_right = right_edge;
    for (i, (_, _, _, width)) in line_words.iter().enumerate() {
        let pos_idx = start_idx + i;
        if pos_idx < word_positions.len() {
            let current_width = word_positions[pos_idx].size.width;

            // Update position (right-aligned)
            word_positions[pos_idx].position.x = current_right - current_width;

            // Move right edge for next word
            current_right -= current_width;
        }
    }
}

// Apply text justification
fn apply_text_justification(
    word_positions: &mut Vec<WordPosition>,
    line_breaks: &[InlineTextLine],
    justify: TextJustification,
    max_width: Option<f32>,
    default_space_width: f32,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    if word_positions.is_empty() || line_breaks.is_empty() || max_width.is_none() {
        return;
    }

    let max_width = max_width.unwrap();

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Applying text justification: {:?}", justify).into(),
            location: "apply_text_justification".to_string().into(),
        });
    }

    for line in line_breaks {
        // Skip justification for the last line in full justification mode
        let is_last_line = line.word_end == word_positions.len() - 1;
        if justify == TextJustification::Full && is_last_line {
            continue;
        }

        let line_width = line.bounds.size.width;
        let available_space = max_width - line_width;

        if available_space <= 0.0 {
            continue;
        }

        match justify {
            TextJustification::Left => {
                // Left justification is the default, no need to adjust
            }
            TextJustification::Right => {
                // Move all words in the line to the right
                for i in line.word_start..=line.word_end {
                    if i < word_positions.len() {
                        word_positions[i].position.x += available_space;
                    }
                }
            }
            TextJustification::Center => {
                // Center all words in the line
                let offset = available_space / 2.0;
                for i in line.word_start..=line.word_end {
                    if i < word_positions.len() {
                        word_positions[i].position.x += offset;
                    }
                }
            }
            TextJustification::Full => {
                // Count the number of spaces in this line
                let mut space_count = 0;
                for i in line.word_start..=line.word_end {
                    if i < word_positions.len() && word_positions[i].shaped_word_index.is_none() {
                        // This is a space
                        space_count += 1;
                    }
                }

                if space_count > 0 {
                    // Calculate additional space for each space character
                    let additional_space = available_space / space_count as f32;

                    // Apply progressive offsets
                    let mut cumulative_offset = 0.0;

                    for i in line.word_start..=line.word_end {
                        if i < word_positions.len() {
                            // Adjust position by cumulative offset
                            word_positions[i].position.x += cumulative_offset;

                            // If this is a space, increase the width and update cumulative offset
                            if word_positions[i].shaped_word_index.is_none() {
                                word_positions[i].size.width += additional_space;
                                cumulative_offset += additional_space;
                            }
                        }
                    }
                }
            }
            TextJustification::None => {
                // No justification, leave as is
            }
        }
    }
}

/// Returns the (left-aligned!) bounding boxes of the indidividual text lines
pub fn word_positions_to_inline_text_layout(word_positions: &WordPositions) -> InlineTextLayout {
    InlineTextLayout {
        lines: word_positions.line_breaks.clone().into(),
        content_size: word_positions.content_size,
    }
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
enum LineCaretIntersection {
    /// In order to not intersect with any holes, the caret needs to
    /// be advanced to the position x, but can stay on the same line.
    NoLineBreak { new_x: f32, new_y: f32 },
    /// Caret needs to advance X number of lines and be positioned
    /// with a leading of x
    LineBreak { new_x: f32, new_y: f32 },
}

impl LineCaretIntersection {
    #[inline]
    fn new(
        current_x: f32,
        word_width: f32,
        current_y: f32,
        line_height: f32,
        max_width: Option<f32>,
    ) -> Self {
        match max_width {
            None => LineCaretIntersection::NoLineBreak {
                new_x: current_x + word_width,
                new_y: current_y,
            },
            Some(max) => {
                // window smaller than minimum word content: don't break line
                if current_x == 0.0 && max < word_width {
                    LineCaretIntersection::NoLineBreak {
                        new_x: current_x + word_width,
                        new_y: current_y,
                    }
                } else if (current_x + word_width) > max {
                    LineCaretIntersection::LineBreak {
                        new_x: 0.0,
                        new_y: current_y + line_height,
                    }
                } else {
                    LineCaretIntersection::NoLineBreak {
                        new_x: current_x + word_width,
                        new_y: current_y,
                    }
                }
            }
        }
    }
}

pub fn shape_text(font: &FontRef, text: &str, options: &ResolvedTextLayoutOptions) -> InlineText {
    let font_data = font.get_data();
    let parsed_font_downcasted = unsafe { &*(font_data.parsed as *const ParsedFont) };

    let words = split_text_into_words(text);
    let shaped_words = shape_words(&words, parsed_font_downcasted);
    let word_positions = position_words(&words, &shaped_words, options, &mut None);
    let inline_text_layout = word_positions_to_inline_text_layout(&word_positions);

    azul_core::app_resources::get_inline_text(
        &words,
        &shaped_words,
        &word_positions,
        &inline_text_layout,
    )
}

use hyphenation::{Language, Load, Standard};

/// A struct that caches hyphenators for different languages
#[derive(Debug)]
pub struct HyphenationCache {
    english: Option<Standard>,
    // Add more languages as needed
}

impl HyphenationCache {
    /// Creates a new hyphenation cache with common languages
    pub fn new() -> Self {
        // Try to load hyphenators for common languages
        let english = Standard::from_embedded(Language::EnglishUS).ok();

        HyphenationCache { english }
    }

    /// Gets the hyphenator for the given language code
    pub fn get_hyphenator(&self, lang_code: &str) -> Option<&Standard> {
        match lang_code.to_lowercase().as_str() {
            "en" | "en-us" => self.english.as_ref(),
            // Add more languages as needed
            _ => self.english.as_ref(), // Fallback to English
        }
    }
}

/// Find possible hyphenation points in a word
#[cfg(feature = "text_layout")]
pub fn find_hyphenation_points(word: &str, hyphenator: &Standard) -> Vec<u32> {
    use hyphenation::Hyphenator;

    if word.len() < 4 {
        return Vec::new(); // Don't hyphenate very short words
    }

    let hyphenated = hyphenator.hyphenate(word);
    hyphenated
        .breaks
        .iter()
        .map(|s| (*s).min(core::u32::MAX as usize) as u32)
        .collect()
}

/// Determines whether a script is right-to-left
pub fn is_rtl_script(script: crate::text::script::Script) -> bool {
    use crate::text::script::Script;
    match script {
        Script::Arabic | Script::Hebrew => true,
        _ => false,
    }
}

/// Detects the script direction for a text span
pub fn detect_text_direction(text: &str) -> ScriptType {
    use crate::text::script::detect_script;

    let script = detect_script(text);
    match script {
        Some(s) if is_rtl_script(s) => ScriptType::RTL,
        Some(_) => ScriptType::LTR,
        None => ScriptType::LTR, // Default to LTR if script cannot be detected
    }
}

/// Splits text into spans of the same script direction
pub fn split_by_direction(text: &str) -> Vec<TextScriptInfo> {
    let mut result = Vec::new();
    let chars: Vec<char> = text.chars().collect();

    if chars.is_empty() {
        return result;
    }

    let mut start = 0;
    let mut current_script = detect_text_direction(&chars[0].to_string());

    for i in 1..chars.len() {
        let char_script = detect_text_direction(&chars[i].to_string());
        if char_script != current_script {
            // Script changed, save the current span
            result.push(TextScriptInfo {
                script: current_script,
                start,
                end: i,
            });

            start = i;
            current_script = char_script;
        }
    }

    // Add the last span
    result.push(TextScriptInfo {
        script: current_script,
        start,
        end: chars.len(),
    });

    result
}
