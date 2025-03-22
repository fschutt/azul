//! Text layout handling for the new layout solver
//!
//! This module extends the existing text layout functionality with support for padding,
//! margins, and float integration according to the CSS visual formatting model.
//!
//! The text layout system:
//! 1. Extracts text and font styling from the DOM
//! 2. Shapes the text using the appropriate font
//! 3. Positions words respecting line breaks, hyphenation and text alignment
//! 4. Adjusts text flow around floated elements

use std::collections::BTreeMap;

use azul_core::{
    app_resources::{
        ExclusionSide, LineCaretIntersection, RendererResources, RendererResourcesTrait,
        ShapedWord, ShapedWords, TextExclusionArea, Word, WordPosition, WordPositions, WordType,
        Words,
    },
    dom::{NodeData, NodeType},
    id_tree::{NodeDataContainer, NodeId},
    styled_dom::{StyleFontFamiliesHash, StyledDom},
    ui_solver::{
        InlineTextLayout, InlineTextLayoutRustInternal, InlineTextLine, LayoutDebugMessage,
        PositionInfo, PositionInfoInner, PositionedRectangle, ResolvedTextLayoutOptions,
        ScriptType, DEFAULT_LINE_HEIGHT, DEFAULT_TAB_WIDTH, DEFAULT_WORD_SPACING,
    },
    window::{LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::*;

use super::{
    shaping::{ParsedFont, ShapedTextBufferUnsized},
    FontImpl, TextLayoutOffsets,
};
use crate::solver2::{context::FormattingContext, intrinsic::IntrinsicSizes};

/// Process a text node during layout
pub fn process_text_node_layout<T: RendererResourcesTrait>(
    node_id: NodeId,
    styled_dom: &StyledDom,
    formatting_context: &FormattingContext,
    available_rect: LogicalRect,
    renderer_resources: &T,
    positioned_rects: &mut azul_core::id_tree::NodeDataContainerRefMut<PositionedRectangle>,
    floats: &[TextExclusionArea],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {
    // Check if this is a text node
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if !matches!(node_data.get_node_type(), NodeType::Text(_)) {
        return LogicalSize::zero();
    }

    // Layout the text node
    let text_layout = match layout_text_node(
        node_id,
        styled_dom,
        formatting_context,
        available_rect,
        renderer_resources,
        debug_messages,
    ) {
        Some(layout) => layout,
        None => return LogicalSize::zero(),
    };

    // Calculate padding and margins
    let padding = calculate_text_padding(node_id, styled_dom, available_rect);
    let margin = calculate_text_margin(node_id, styled_dom, available_rect);

    // Update the positioned rectangle for this node
    let total_width =
        text_layout.content_size.width + padding.left + padding.right + margin.left + margin.right;
    let total_height =
        text_layout.content_size.height + padding.top + padding.bottom + margin.top + margin.bottom;

    // Create positioned rectangle with text layout
    let mut rect = positioned_rects[node_id].clone();

    // Update position
    rect.position = PositionInfo::Static(PositionInfoInner {
        x_offset: available_rect.origin.x,
        y_offset: available_rect.origin.y,
        static_x_offset: available_rect.origin.x,
        static_y_offset: available_rect.origin.y,
    });

    // Update size
    rect.size = LogicalSize::new(total_width, total_height);

    // Update padding and margin
    rect.padding.left = padding.left;
    rect.padding.right = padding.right;
    rect.padding.top = padding.top;
    rect.padding.bottom = padding.bottom;

    rect.margin.left = margin.left;
    rect.margin.right = margin.right;
    rect.margin.top = margin.top;
    rect.margin.bottom = margin.bottom;

    // Store the text layout
    rect.resolved_text_layout_options = Some((
        Default::default(), // We don't need to store the options here
        text_layout,
    ));

    positioned_rects[node_id] = rect;

    // Return content size
    LogicalSize::new(total_width, total_height)
}

/// Helper function to calculate padding for a text node
fn calculate_text_padding(
    node_id: NodeId,
    styled_dom: &StyledDom,
    available_rect: LogicalRect,
) -> TextLayoutOffsets {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    let parent_width = available_rect.size.width;
    let parent_height = available_rect.size.height;

    let padding_left = css_property_cache
        .get_padding_left(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let padding_right = css_property_cache
        .get_padding_right(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let padding_top = css_property_cache
        .get_padding_top(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    let padding_bottom = css_property_cache
        .get_padding_bottom(node_data, &node_id, styled_node_state)
        .and_then(|p| Some(p.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    TextLayoutOffsets {
        left: padding_left,
        right: padding_right,
        top: padding_top,
        bottom: padding_bottom,
    }
}

/// Helper function to calculate margin for a text node
fn calculate_text_margin(
    node_id: NodeId,
    styled_dom: &StyledDom,
    available_rect: LogicalRect,
) -> TextLayoutOffsets {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    let parent_width = available_rect.size.width;
    let parent_height = available_rect.size.height;

    let margin_left = css_property_cache
        .get_margin_left(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let margin_right = css_property_cache
        .get_margin_right(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_width)))
        .unwrap_or(0.0);

    let margin_top = css_property_cache
        .get_margin_top(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    let margin_bottom = css_property_cache
        .get_margin_bottom(node_data, &node_id, styled_node_state)
        .and_then(|m| Some(m.get_property()?.inner.to_pixels(parent_height)))
        .unwrap_or(0.0);

    TextLayoutOffsets {
        left: margin_left,
        right: margin_right,
        top: margin_top,
        bottom: margin_bottom,
    }
}

/// Creates text layout options from CSS properties
fn create_text_layout_options(
    node_id: NodeId,
    node_data: &NodeData,
    styled_dom: &StyledDom,
    available_rect: LogicalRect,
    font_size: StyleFontSize,
) -> ResolvedTextLayoutOptions {
    let css_property_cache = styled_dom.get_css_property_cache();
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Extract font properties
    let font_size_px = font_size.inner.to_pixels(100.0); // 100.0 is a default reference for percentage

    // Line height
    let line_height = css_property_cache
        .get_line_height(node_data, &node_id, styled_node_state)
        .and_then(|lh| Some(lh.get_property()?.inner.get()))
        .into();

    // Letter spacing
    let letter_spacing = css_property_cache
        .get_letter_spacing(node_data, &node_id, styled_node_state)
        .and_then(|ls| Some(ls.get_property()?.inner.to_pixels(font_size_px)))
        .into();

    // Word spacing
    let word_spacing = css_property_cache
        .get_word_spacing(node_data, &node_id, styled_node_state)
        .and_then(|ws| Some(ws.get_property()?.inner.to_pixels(font_size_px)))
        .into();

    // Tab width
    let tab_width = css_property_cache
        .get_tab_width(node_data, &node_id, styled_node_state)
        .and_then(|tw| Some(tw.get_property()?.inner.get()))
        .into();

    // Text direction
    let direction = css_property_cache
        .get_direction(node_data, &node_id, styled_node_state)
        .and_then(|dir| dir.get_property().copied())
        .unwrap_or_default();

    let is_rtl = if direction == StyleDirection::Rtl {
        ScriptType::RTL
    } else {
        ScriptType::LTR
    };

    // Text justification
    let text_justify = css_property_cache
        .get_text_align(node_data, &node_id, styled_node_state)
        .and_then(|ta| ta.get_property().copied())
        .into();

    // Create the resolved text layout options
    ResolvedTextLayoutOptions {
        font_size_px,
        line_height,
        letter_spacing,
        word_spacing,
        tab_width,
        max_horizontal_width: Some(available_rect.size.width).into(),
        leading: None.into(),
        holes: Vec::new().into(), // Will be populated for text with floats
        max_vertical_height: Some(available_rect.size.height).into(),
        can_break: true,
        can_hyphenate: true, // Enable hyphenation by default
        hyphenation_character: Some('-' as u32).into(),
        is_rtl,
        text_justify,
    }
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
            if line_caret_y + (font_size_px + line_height_px) > max_y {
                if let Some(messages) = debug_messages {
                    messages.push(LayoutDebugMessage {
                        message: format!(
                            "Reached max vertical height ({}) at position {} - stopping layout",
                            max_y, line_caret_y
                        )
                        .into(),
                        location: "position_words".to_string().into(),
                    });
                }
                should_stop_layout = true;
                break;
            }
        }

        match &word.word_type {
            WordType::Word | WordType::WordWithHyphenation(_) => {
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
                    LineCaretIntersection::NoLineBreak { new_x, new_y } => {
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

                                // For RTL text, reposition words in the current line
                                if is_rtl {
                                    position_rtl_line(
                                        &mut word_positions,
                                        &current_line_words,
                                        text_layout_options
                                            .max_horizontal_width
                                            .into_option()
                                            .unwrap_or(line_caret_x),
                                        debug_messages,
                                    );
                                    current_line_words.clear();
                                }

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
                    LineCaretIntersection::LineBreak { new_x, new_y } => {
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
                                text_layout_options
                                    .max_horizontal_width
                                    .into_option()
                                    .unwrap_or(line_caret_x),
                                debug_messages,
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
            WordType::Return => {
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
                            text_layout_options
                                .max_horizontal_width
                                .into_option()
                                .unwrap_or(line_caret_x),
                            debug_messages,
                        );
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
            WordType::Space | WordType::Tab => {
                let x_advance = match word.word_type {
                    WordType::Space => word_spacing_px,
                    WordType::Tab => tab_width_px,
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
                    LineCaretIntersection::NoLineBreak { new_x, new_y } => {
                        word_positions.push(WordPosition {
                            shaped_word_index: None,
                            position: LogicalPosition::new(line_caret_x, line_caret_y),
                            size: LogicalSize::new(x_advance, font_size_px + line_height_px),
                            hyphenated: false,
                        });
                        line_caret_x = new_x;
                        line_caret_y = new_y;
                    }
                    LineCaretIntersection::LineBreak { new_x, new_y } => {
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
                                    text_layout_options
                                        .max_horizontal_width
                                        .into_option()
                                        .unwrap_or(line_caret_x),
                                    debug_messages,
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
            position_rtl_line(
                &mut word_positions,
                &current_line_words,
                text_layout_options
                    .max_horizontal_width
                    .into_option()
                    .unwrap_or(line_caret_x),
                debug_messages,
            );
        }
    }

    // Apply text justification if needed
    if let Some(justify) = text_layout_options.text_justify.into_option() {
        apply_text_justification(
            &mut word_positions,
            &line_breaks,
            justify,
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

// Reposition words in an RTL line
fn position_rtl_line(
    word_positions: &mut Vec<WordPosition>,
    line_words: &[(usize, usize, Word, f32)],
    line_width: f32,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) {
    if line_words.is_empty() {
        return;
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Positioning RTL line with {} words, line width: {}",
                line_words.len(),
                line_width
            )
            .into(),
            location: "position_rtl_line".to_string().into(),
        });
    }

    // Calculate total width of the line
    let total_width: f32 = line_words.iter().map(|(_, _, _, width)| *width).sum();

    // Find indices of words in this line in the word_positions vector
    let start_idx = word_positions.len() - line_words.len();
    let end_idx = word_positions.len();

    // Calculate right edge position - crucial for RTL layout
    let right_edge = line_width;

    // Log the calculations
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "RTL line: start_idx={}, end_idx={}, total_width={}, right_edge={}",
                start_idx, end_idx, total_width, right_edge
            )
            .into(),
            location: "position_rtl_line".to_string().into(),
        });
    }

    // Reposition each word from right to left
    let mut current_right = right_edge;

    // For RTL, we need to reverse the order of words
    for i in (0..line_words.len()).rev() {
        let (_, _, _, width) = line_words[i];
        let pos_idx = start_idx + i;

        if pos_idx < word_positions.len() {
            // Update position (right-aligned)
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!(
                        "RTL word {}: original pos={}, new right={}, width={}",
                        i, word_positions[pos_idx].position.x, current_right, width
                    )
                    .into(),
                    location: "position_rtl_line".to_string().into(),
                });
            }

            // Set word position - for RTL we align to the right edge
            word_positions[pos_idx].position.x = current_right - width;

            // Move right edge for next word
            current_right -= width;
        }
    }
}

// Apply text justification
fn apply_text_justification(
    word_positions: &mut Vec<WordPosition>,
    line_breaks: &[InlineTextLine],
    justify: StyleTextAlign,
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
        if justify == StyleTextAlign::Justify && is_last_line {
            continue;
        }

        let line_width = line.bounds.size.width;
        let available_space = max_width - line_width;

        if available_space <= 0.0 {
            continue;
        }

        match justify {
            StyleTextAlign::Left => {
                // Left justification is the default, no need to adjust
            }
            StyleTextAlign::Right => {
                // Move all words in the line to the right
                for i in line.word_start..=line.word_end {
                    if i < word_positions.len() {
                        word_positions[i].position.x += available_space;
                    }
                }
            }
            StyleTextAlign::Center => {
                // Center all words in the line
                let offset = available_space / 2.0;
                for i in line.word_start..=line.word_end {
                    if i < word_positions.len() {
                        word_positions[i].position.x += offset;
                    }
                }
            }
            StyleTextAlign::Justify => {
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
        }
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

    // TODO: Simplistic approximation - in reality, we'd need to shape the hyphen character
    // For now, use a fraction of the space advance as an estimate
    shaped_words.get_space_advance_px(font_size_px) * 0.5
}

/// Returns the (left-aligned!) bounding boxes of the indidividual text lines
pub fn word_positions_to_inline_text_layout(word_positions: &WordPositions) -> InlineTextLayout {
    InlineTextLayout {
        lines: word_positions.line_breaks.clone().into(),
        content_size: word_positions.content_size,
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

/// Detects the script direction for a text span
pub fn detect_text_direction(text: &str) -> ScriptType {
    use crate::text2::script::detect_script;

    let script = detect_script(text);
    match script {
        Some(s) if is_rtl_script(s) => ScriptType::RTL,
        Some(_) => ScriptType::LTR,
        None => ScriptType::LTR, // Default to LTR if script cannot be detected
    }
}

/// Determines whether a script is right-to-left
pub fn is_rtl_script(script: crate::text2::script::Script) -> bool {
    use crate::text2::script::Script;
    match script {
        Script::Arabic | Script::Hebrew => true,
        _ => false,
    }
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

/// Performs text layout for a text node, considering its CSS styling
pub fn layout_text_node<T: RendererResourcesTrait>(
    node_id: NodeId,
    styled_dom: &StyledDom,
    formatting_context: &FormattingContext,
    available_rect: LogicalRect,
    renderer_resources: &T,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Option<InlineTextLayout> {
    // Log entry point
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Beginning layout_text_node for node {}", node_id.index()).into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Get text content
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_text = match node_data.get_node_type() {
        NodeType::Text(text_content) => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!(
                        "Node {} has text: '{}'",
                        node_id.index(),
                        text_content.as_str()
                    )
                    .into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            text_content.as_str()
        }
        _ => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!("Node {} is not a text node", node_id.index()).into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            return None; // Not a text node
        }
    };

    // Get CSS property cache and node state
    let css_property_cache = styled_dom.get_css_property_cache();
    let styled_node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

    // Calculate padding and margins that affect text layout
    let padding = calculate_text_padding(node_id, styled_dom, available_rect);
    let margin = calculate_text_margin(node_id, styled_dom, available_rect);

    // Log padding and margin values
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Padding: {:?}, Margin: {:?}", padding, margin).into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Adjust available rect for padding and margin
    let content_rect = LogicalRect::new(
        LogicalPosition::new(
            available_rect.origin.x + margin.left + padding.left,
            available_rect.origin.y + margin.top + padding.top,
        ),
        LogicalSize::new(
            available_rect.size.width - padding.left - padding.right - margin.left - margin.right,
            available_rect.size.height - padding.top - padding.bottom - margin.top - margin.bottom,
        ),
    );

    // Log adjusted content rect
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Content rect: {:?}", content_rect).into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Extract text styling properties
    let font_families =
        css_property_cache.get_font_id_or_default(node_data, &node_id, styled_node_state);

    let font_size =
        css_property_cache.get_font_size_or_default(node_data, &node_id, styled_node_state);

    let text_align = css_property_cache
        .get_text_align(node_data, &node_id, styled_node_state)
        .and_then(|ta| ta.get_property().copied())
        .unwrap_or(StyleTextAlign::Left);

    // Get the font from the renderer resources
    let css_font_families_hash = StyleFontFamiliesHash::new(font_families.as_ref());

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Getting font family with hash: {:?}",
                css_font_families_hash
            )
            .into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    let css_font_family = match renderer_resources.get_font_family(&css_font_families_hash) {
        Some(f) => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: "Font family found".to_string().into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            f
        }
        None => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: "Font family not found".to_string().into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            return None;
        }
    };

    let font_key = match renderer_resources.get_font_key(css_font_family) {
        Some(k) => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: format!("Font key found: {:?}", k).into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            k
        }
        None => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: "Font key not found".to_string().into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            return None;
        }
    };

    let (font_ref, _) = match renderer_resources.get_registered_font(font_key) {
        Some(fr) => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: "Font reference found".to_string().into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            fr
        }
        None => {
            if let Some(messages) = debug_messages {
                messages.push(LayoutDebugMessage {
                    message: "Font reference not found".to_string().into(),
                    location: "layout_text_node".to_string().into(),
                });
            }
            return None;
        }
    };

    // Get the parsed font
    let font_data = font_ref.get_data();
    let parsed_font = unsafe { &*(font_data.parsed as *const ParsedFont) };

    // Create resolved text layout options from CSS properties
    let text_layout_options =
        create_text_layout_options(node_id, node_data, styled_dom, content_rect, font_size);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Created text layout options with font size: {}",
                text_layout_options.font_size_px
            )
            .into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Process text with hyphenation
    // Using lazy_static pattern for the hyphenation cache
    static mut HYPHENATION_CACHE: Option<HyphenationCache> = None;
    let hyphenation_cache = unsafe {
        if HYPHENATION_CACHE.is_none() {
            HYPHENATION_CACHE = Some(HyphenationCache::new());
        }
        HYPHENATION_CACHE.as_ref().unwrap()
    };

    // Split text into words with possible hyphenation
    let words = split_text_into_words_with_hyphenation(
        node_text,
        &text_layout_options,
        hyphenation_cache,
        debug_messages,
    );

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Split text into {} words", words.items.len()).into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Shape the words using the font
    let shaped_words = shape_words(&words, parsed_font);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!("Shaped words contains {} items", shaped_words.items.len()).into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Position the words based on the layout options, considering floats
    let word_positions =
        position_words(&words, &shaped_words, &text_layout_options, debug_messages);

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: format!(
                "Positioned {} words in {} lines",
                word_positions.word_positions.len(),
                word_positions.line_breaks.len()
            )
            .into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    // Convert word positions to line layout
    let mut inline_text_layout = word_positions_to_inline_text_layout(&word_positions);

    // Apply text alignment
    if text_align != StyleTextAlign::Left {
        inline_text_layout.align_children_horizontal(&content_rect.size, text_align);

        if let Some(messages) = debug_messages {
            messages.push(LayoutDebugMessage {
                message: format!("Applied text alignment: {:?}", text_align).into(),
                location: "layout_text_node".to_string().into(),
            });
        }
    }

    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: "Text layout completed successfully".to_string().into(),
            location: "layout_text_node".to_string().into(),
        });
    }

    Some(inline_text_layout)
}
