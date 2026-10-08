//! Justification: distributing a line's free space, kashida, and the trailing white space it leaves out.

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, HashMap},
        BTreeSet, HashSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage,
    props::{basic::ColorU, style::StyleBackgroundContent},
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{
    FcFontCache, FcPattern, FcStretch, FcWeight, FontId, PatternMatch, UnicodeRange,
};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;
use crate::text3::script::{script_to_language, Language, Script};
#[allow(clippy::wildcard_imports)]
// the text layout cache's items, re-exported from the sibling modules by mod.rs
use super::*;

/// Calculates the extra spacing needed for justification without modifying the items.
///
/// This function is pure and does not mutate any state, making it safe to use
/// with cached `ShapedItem` data.
///
/// # Arguments
/// * `items` - A slice of items on the line.
/// * `line_constraints` - The geometric constraints for the line.
/// * `text_justify` - The type of justification to calculate.
/// * `is_vertical` - Whether the layout is vertical.
///
/// # Returns
/// A tuple `(extra_per_word, extra_per_char)` containing the extra space in pixels
/// to add at each word or character justification opportunity.
// +spec:display-contents:654278 - distributes remaining space to fill line box when justifying
// +spec:text-alignment-spacing:56c7f4 - equal distribution of justification space within priority
// level +spec:text-alignment-spacing:f17bbc - justification opportunities controlled by
// text-justify value (inter-word = word separators, inter-character = character juxtaposition)
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
pub(super) fn calculate_justification_spacing(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    text_justify: JustifyContent,
    is_vertical: bool,
) -> (f32, f32) {
    // (extra_per_word, extra_per_char)
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;

    if total_width >= available_width || available_width <= 0.0 {
        return (0.0, 0.0);
    }

    let extra_space = available_width - total_width;

    // +spec:text-alignment-spacing:71314a - script categories for justification: inter-word for
    // clustered, kashida for cursive (Arabic), inter-character for block (CJK)
    match text_justify {
        JustifyContent::InterWord => {
            // Count justification opportunities (spaces).
            let space_count = items.iter().filter(|item| is_word_separator(item)).count();
            if space_count > 0 {
                (extra_space / space_count as f32, 0.0)
            } else {
                (0.0, 0.0) // No spaces to expand, do nothing.
            }
        }
        JustifyContent::InterCharacter | JustifyContent::Distribute => {
            // Count justification opportunities (between non-combining characters).
            let gap_count = items
                .iter()
                .enumerate()
                .filter(|(i, item)| *i < items.len() - 1 && can_justify_after(item))
                .count();
            if gap_count > 0 {
                (0.0, extra_space / gap_count as f32)
            } else {
                (0.0, 0.0) // No gaps to expand, do nothing.
            }
        }
        // Kashida justification modifies the item list and is handled by a separate function.
        _ => (0.0, 0.0),
    }
}

/// Rebuilds a line of items, inserting Kashida glyphs for justification.
///
/// This function is non-mutating with respect to its inputs. It takes ownership of the
/// original items and returns a completely new `Vec`. This is necessary because Kashida
/// justification changes the number of items on the line, and must not modify cached data.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
pub fn justify_kashida_and_rebuild<T: ParsedFontTrait>(
    items: Vec<ShapedItem>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Vec<ShapedItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering justify_kashida_and_rebuild ---".to_string(),
        ));
    }
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Total item width: {total_width}, Available width: {available_width}"
        )));
    }

    if total_width >= available_width || available_width <= 0.0 {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No justification needed (line is full or invalid).".to_string(),
            ));
        }
        return items;
    }

    let extra_space = available_width - total_width;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Extra space to fill: {extra_space}"
        )));
    }

    let font_info = items.iter().find_map(|item| {
        if let ShapedItem::Cluster(c) = item {
            if let Some(glyph) = c.glyphs.first() {
                if glyph.script == Script::Arabic {
                    // Look up font from hash
                    if let Some(font) = fonts.get_by_hash(glyph.font_hash) {
                        return Some((
                            font.clone(),
                            glyph.font_hash,
                            glyph.font_metrics,
                            c.style.clone(),
                        ));
                    }
                }
            }
        }
        None
    });

    let (font, font_hash, font_metrics, style) = if let Some(info) = font_info {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Found Arabic font for kashida.".to_string(),
            ));
        }
        info
    } else {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No Arabic font found on line. Cannot insert kashidas.".to_string(),
            ));
        }
        return items;
    };

    let (kashida_glyph_id, kashida_advance) =
        match font.get_kashida_glyph_and_advance(style.font_size_px) {
            Some((id, adv)) if adv > 0.0 => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "Font provides kashida glyph with advance {adv}"
                    )));
                }
                (id, adv)
            }
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "Font does not support kashida justification.".to_string(),
                    ));
                }
                return items;
            }
        };

    let opportunity_indices: Vec<usize> = items
        .windows(2)
        .enumerate()
        .filter_map(|(i, window)| {
            if let (ShapedItem::Cluster(cur), ShapedItem::Cluster(next)) = (&window[0], &window[1])
            {
                if is_arabic_cluster(cur)
                    && is_arabic_cluster(next)
                    && !is_word_separator(&window[1])
                {
                    return Some(i + 1);
                }
            }
            None
        })
        .collect();

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Found {} kashida insertion opportunities at indices: {:?}",
            opportunity_indices.len(),
            opportunity_indices
        )));
    }

    if opportunity_indices.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No opportunities found. Exiting.".to_string(),
            ));
        }
        return items;
    }

    let num_kashidas_to_insert = (extra_space / kashida_advance).floor() as usize;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Calculated number of kashidas to insert: {num_kashidas_to_insert}"
        )));
    }

    if num_kashidas_to_insert == 0 {
        return items;
    }

    let kashidas_per_point = num_kashidas_to_insert / opportunity_indices.len();
    let mut remainder = num_kashidas_to_insert % opportunity_indices.len();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Distributing kashidas: {kashidas_per_point} per point, with {remainder} remainder."
        )));
    }

    let kashida_item = {
        /* ... as before ... */
        let kashida_glyph = ShapedGlyph {
            kind: GlyphKind::Kashida {
                width: kashida_advance,
            },
            glyph_id: kashida_glyph_id,
            font_hash,
            font_metrics,
            script: Script::Arabic,
            advance: kashida_advance,
            kerning: 0.0,
            cluster_offset: 0,
            offset: Point::default(),
            vertical_advance: 0.0,
            vertical_offset: Point::default(),
        };
        ShapedItem::Cluster(ShapedCluster {
            flags: ClusterFlags::classify("\u{0640}"),
            source_text: Arc::from("\u{0640}"),
            source_byte_len: 2,
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Kashida is generated, not from DOM
            glyphs: smallvec![kashida_glyph],
            advance: kashida_advance,
            direction: BidiDirection::Ltr,
            style,
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        })
    };

    let mut new_items = Vec::with_capacity(items.len() + num_kashidas_to_insert);
    let mut last_copy_idx = 0;
    for &point in &opportunity_indices {
        new_items.extend_from_slice(&items[last_copy_idx..point]);
        let mut num_to_insert = kashidas_per_point;
        if remainder > 0 {
            num_to_insert += 1;
            remainder -= 1;
        }
        for _ in 0..num_to_insert {
            new_items.push(kashida_item.clone());
        }
        last_copy_idx = point;
    }
    new_items.extend_from_slice(&items[last_copy_idx..]);

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting justify_kashida_and_rebuild, new item count: {} ---",
            new_items.len()
        )));
    }
    new_items
}

/// Helper to determine if a cluster belongs to the Arabic script.
pub(super) fn is_arabic_cluster(cluster: &ShapedCluster) -> bool {
    // A cluster is considered Arabic if its first non-NotDef glyph is from the Arabic script.
    // This is a robust heuristic for mixed-script lines.
    cluster.glyphs.iter().any(|g| g.script == Script::Arabic)
}

/// Helper to identify if an item is a word separator (like a space).
pub(super) fn measure_trailing_whitespace(items: &[ShapedItem], is_vertical: bool) -> f32 {
    let mut trailing_ws = 0.0;
    for item in items.iter().rev() {
        // A list marker's space belongs to the marker (`break_one_line`).
        let is_marker = matches!(
            item,
            ShapedItem::Cluster(c) if c.marker_position_outside.is_some()
        );
        if is_collapsible_whitespace(item) && !is_marker {
            trailing_ws += get_item_measure(item, is_vertical);
        } else {
            break;
        }
    }
    trailing_ws
}

/// Helper to identify if space can be added after an item.
pub(super) fn can_justify_after(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text().chars().last().is_some_and(|g| {
            !g.is_whitespace() && classify_character(g as u32) != CharacterClass::Combining
        })
    } else {
        // Per CSS 2.2 §9.4.2, justification must NOT stretch inline-table and
        // inline-block boxes. Object items represent these atomic inline-level
        // boxes, so we return false to prevent adding justification space after them.
        false
    }
}

// +spec:font-metrics:b8eb97 - Script group classification for justification/letter-spacing behavior
/// Classifies a character for layout purposes (e.g., justification behavior).
/// Copied from `mod.rs`.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant
                                  // (or cross-type bindings that can't merge)
pub(super) const fn classify_character(codepoint: u32) -> CharacterClass {
    match codepoint {
        0x0020 | 0x00A0 | 0x3000 => CharacterClass::Space,
        0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => {
            CharacterClass::Punctuation
        }
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => CharacterClass::Ideograph,
        0x0300..=0x036F | 0x1AB0..=0x1AFF => CharacterClass::Combining,
        // Mongolian script range
        0x1800..=0x18AF => CharacterClass::Letter,
        _ => CharacterClass::Letter,
    }
}

pub(super) const fn get_justification_priority(class: CharacterClass) -> u8 {
    match class {
        CharacterClass::Space => 0,
        CharacterClass::Punctuation => 64,
        CharacterClass::Ideograph => 128,
        CharacterClass::Letter => 192,
        CharacterClass::Symbol => 224,
        CharacterClass::Combining => 255,
    }
}
