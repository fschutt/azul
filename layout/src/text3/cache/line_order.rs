//! The two orders the items of a line come in.
//!
//! A paragraph is split into bidi runs, shaped and broken into lines in
//! logical (reading) order, and a line's breaking decisions - where it ends,
//! which white space at its end is trimmed - are made in that order too. Only
//! a finished line is put in visual order, left to right, for positioning and
//! drawing: the bidi algorithm's rules L1 and L2 apply per line (UAX 9,
//! section 3.4). The two orders are two types, [`LogicalLine`] and
//! [`VisualLine`], and the only way from one to the other is
//! [`LogicalLine::into_visual`], so no code can position a line nobody put in
//! visual order, nor break or trim one that already is.

use core::ops::Deref;

use super::{is_collapsible_whitespace, BidiDirection, BidiLevel, ShapedItem, WhiteSpaceMode};

/// A line's items in logical (reading) order, as the line breaker takes them
/// from the paragraph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogicalLine(Vec<ShapedItem>);

/// A line's items in visual order: left to right, or top to bottom in a
/// vertical line. Made only by [`LogicalLine::into_visual`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualLine(Vec<ShapedItem>);

impl LogicalLine {
    /// The line of `items`, which are in logical order.
    #[must_use]
    pub const fn new(items: Vec<ShapedItem>) -> Self {
        Self(items)
    }

    /// Drops the items at the line's end while `drop` holds for them.
    pub fn trim_end_while(&mut self, mut drop: impl FnMut(&ShapedItem) -> bool) {
        while self.0.last().is_some_and(&mut drop) {
            self.0.pop();
        }
    }

    /// Drops the white space a line ends in where `white_space_mode`
    /// collapses it (CSS Text 3, 4.1.2: normal, nowrap, pre-line); pre keeps
    /// it, pre-wrap hangs it and break-spaces wraps it. A list marker's own
    /// space ("1. ") belongs to the marker (Chrome's UA sheet: `::marker {
    /// white-space: pre }`), never to the line's end.
    pub fn trim_trailing_white_space(&mut self, white_space_mode: WhiteSpaceMode) {
        if matches!(
            white_space_mode,
            WhiteSpaceMode::Normal | WhiteSpaceMode::Nowrap | WhiteSpaceMode::PreLine
        ) {
            self.trim_end_while(|item| {
                let is_marker = matches!(
                    item,
                    ShapedItem::Cluster(c) if c.marker_position_outside.is_some()
                );
                is_collapsible_whitespace(item) && !is_marker
            });
        }
    }

    /// The items, still in logical order.
    #[must_use]
    pub fn into_items(self) -> Vec<ShapedItem> {
        self.0
    }

    /// The line in visual order, in a paragraph that runs in `base`.
    ///
    /// Rule L1 first puts tabs, breaks, the white space before them and the
    /// white space at the line's end at the paragraph's level, so they stay
    /// at the end of the line the paragraph reads towards. Rule L2 then
    /// reverses, from the highest level down to the lowest odd one, every
    /// run of items at that level or higher: an Arabic word in an English
    /// line reads right to left, the number inside it left to right again.
    #[must_use]
    pub fn into_visual(self, base: BidiDirection) -> VisualLine {
        let paragraph = BidiLevel::paragraph(base).level();
        // Rule L2 reverses runs from the highest level down to the lowest
        // odd one: when the highest level is below the lowest odd one (every
        // line of a left-to-right paragraph without right-to-left text, and
        // any line whose items are all at one even level) it reverses
        // nothing, and the visual order is the logical one. The levels' range
        // is found in one pass, by the L1 rule below, without the four
        // vectors the reordering takes - once per line of every paragraph.
        let mut range: Option<(u8, u8)> = None;
        let mut ends_a_segment = true;
        for item in self.0.iter().rev() {
            let level = if matches!(item, ShapedItem::Tab { .. } | ShapedItem::Break { .. }) {
                ends_a_segment = true;
                paragraph
            } else if ends_a_segment && is_bidi_white_space(item) {
                paragraph
            } else {
                ends_a_segment = false;
                item.bidi_level().map_or(paragraph, |level| level.level())
            };
            range = Some(range.map_or((level, level), |(low, high)| {
                (low.min(level), high.max(level))
            }));
        }
        if range.is_none_or(|(lowest, highest)| highest < (lowest | 1)) {
            return VisualLine(self.0);
        }
        let mut levels: Vec<u8> = self
            .0
            .iter()
            .map(|item| item.bidi_level().map_or(paragraph, |level| level.level()))
            .collect();
        let mut ends_a_segment = true;
        for (item, level) in self.0.iter().zip(levels.iter_mut()).rev() {
            if matches!(item, ShapedItem::Tab { .. } | ShapedItem::Break { .. }) {
                *level = paragraph;
                ends_a_segment = true;
            } else if ends_a_segment && is_bidi_white_space(item) {
                *level = paragraph;
            } else {
                ends_a_segment = false;
            }
        }
        let mut items: Vec<Option<ShapedItem>> = self.0.into_iter().map(Some).collect();
        VisualLine(
            visual_order(&levels)
                .into_iter()
                .filter_map(|index| items[index].take())
                .collect(),
        )
    }
}

impl Deref for LogicalLine {
    type Target = [ShapedItem];
    fn deref(&self) -> &[ShapedItem] {
        &self.0
    }
}

impl VisualLine {
    /// The items, in visual order.
    #[must_use]
    pub fn into_items(self) -> Vec<ShapedItem> {
        self.0
    }
}

impl Deref for VisualLine {
    type Target = [ShapedItem];
    fn deref(&self) -> &[ShapedItem] {
        &self.0
    }
}

/// The items of a visual line, or of a piece of one, from the end a paragraph
/// running in `base` reads towards: from the right end leftwards in a
/// left-to-right paragraph, from the left end rightwards in a right-to-left
/// one. Rule L1 keeps a line's trailing white space and its break there.
pub fn from_line_end(
    items: &[ShapedItem],
    base: BidiDirection,
) -> impl Iterator<Item = &ShapedItem> {
    let (forward, backward) = if base.is_rtl() {
        (Some(items.iter()), None)
    } else {
        (None, Some(items.iter().rev()))
    };
    forward
        .into_iter()
        .flatten()
        .chain(backward.into_iter().flatten())
}

/// Whether `item` is white space to the bidi algorithm (class WS, or an
/// isolate control), which rule L1 moves to the paragraph's level at the end
/// of a line and before a tab or a break.
fn is_bidi_white_space(item: &ShapedItem) -> bool {
    use unicode_bidi::{bidi_class, BidiClass};
    let ShapedItem::Cluster(cluster) = item else {
        return false;
    };
    let text = cluster.text();
    !text.is_empty()
        && text.chars().all(|c| {
            matches!(
                bidi_class(c),
                BidiClass::WS | BidiClass::LRI | BidiClass::RLI | BidiClass::FSI | BidiClass::PDI
            )
        })
}

/// The order in which the pieces of a one-line text are seen from left to
/// right: the bidi algorithm over the whole of `text` (in `direction`, else in
/// the direction of its first strong letter, as `unicode-bidi: plaintext` /
/// `dir="auto"`), each piece at the level of the character it starts with,
/// and rule L2 over the pieces. `starts` are the pieces' byte offsets into
/// `text`, ascending; the result indexes into `starts`. A piece that starts
/// at the end of `text`, or at no character boundary, is at the paragraph's
/// level.
///
/// A text set piece by piece - the words of a label along a curve, each
/// placed on its own - lies in this order from the curve's start: the words
/// of an Arabic name from the last to the first, the Arabic part of a Latin
/// name reversed in its place, a number in an Arabic name where it is read.
/// The same levels as the layout's ([`paragraph_bidi`],
/// [`split_into_bidi_runs`]): each word lies where [`LogicalLine::into_visual`]
/// puts its glyphs when the text is laid out as one line.
#[must_use]
pub fn visual_order_of_pieces(
    text: &str,
    starts: &[usize],
    direction: Option<BidiDirection>,
) -> Vec<usize> {
    use std::sync::Arc;

    use azul_core::selection::ContentIndex;

    use super::{
        paragraph_bidi, split_into_bidi_runs, LogicalItem, StyleProperties, UnicodeBidi,
        UnifiedConstraints,
    };

    let as_written: Vec<usize> = (0..starts.len()).collect();
    if text.is_empty() || starts.len() < 2 {
        return as_written;
    }
    // Without a right-to-left letter, an Arabic number or a right-to-left
    // embedding every level is even (outside a right-to-left paragraph): the
    // pieces keep their order, without the bidi stages (most texts).
    let right_to_left = |c: char| {
        use unicode_bidi::{bidi_class, BidiClass};
        matches!(
            bidi_class(c),
            BidiClass::R | BidiClass::AL | BidiClass::AN | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI
        )
    };
    if direction != Some(BidiDirection::Rtl) && !text.chars().any(right_to_left) {
        return as_written;
    }
    // One logical item for the whole text: the bidi runs' byte offsets are
    // then offsets into `text`.
    let items = [LogicalItem::Text {
        source: ContentIndex {
            run_index: 0,
            item_index: 0,
        },
        text: Arc::from(text),
        style: Arc::new(StyleProperties::default()),
        marker_position_outside: None,
        source_node_id: None,
    }];
    let constraints = UnifiedConstraints {
        direction,
        unicode_bidi: if direction.is_some() {
            UnicodeBidi::Normal
        } else {
            UnicodeBidi::Plaintext
        },
        ..UnifiedConstraints::default()
    };
    let (base, mode) = paragraph_bidi(&items, &constraints);
    let Ok(runs) = split_into_bidi_runs(&items, base, mode, &mut None) else {
        return as_written;
    };
    let paragraph = BidiLevel::paragraph(base).level();
    let levels: Vec<u8> = starts
        .iter()
        .map(|&start| {
            runs.iter()
                .find(|run| {
                    start >= run.run_byte_offset && start < run.run_byte_offset + run.text.len()
                })
                .map_or(paragraph, |run| run.bidi_level.level())
        })
        .collect();
    visual_order(&levels)
}

/// The visual order rule L2 puts items at `levels` in, as indices into
/// `levels`: from the highest level down to the lowest odd one, every run of
/// items at that level or higher is reversed.
fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let (Some(&highest), Some(&lowest)) = (levels.iter().max(), levels.iter().min()) else {
        return order;
    };
    let lowest_odd = lowest | 1;
    let mut level = highest;
    while level >= lowest_odd {
        let mut start = 0;
        while start < order.len() {
            if levels[order[start]] < level {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < order.len() && levels[order[end]] >= level {
                end += 1;
            }
            order[start..end].reverse();
            start = end;
        }
        level -= 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::visual_order;

    #[test]
    fn a_right_to_left_line_is_read_backwards() {
        assert_eq!(visual_order(&[1, 1, 1]), [2, 1, 0]);
    }

    #[test]
    fn a_left_to_right_word_in_a_right_to_left_line_keeps_its_own_order() {
        // "مرحبا Hello" at levels 1 1 2 2: the line reads right to left, the
        // Latin word inside it left to right.
        assert_eq!(visual_order(&[1, 1, 2, 2]), [2, 3, 1, 0]);
    }

    #[test]
    fn a_number_in_an_arabic_phrase_of_an_english_line_stays_left_to_right() {
        // "ab مر 12 cd": levels 0 0 1 1 2 2 0 0.
        assert_eq!(
            visual_order(&[0, 0, 1, 1, 2, 2, 0, 0]),
            [0, 1, 4, 5, 3, 2, 6, 7]
        );
    }

    #[test]
    fn a_left_to_right_line_keeps_its_order() {
        assert_eq!(visual_order(&[0, 0, 0]), [0, 1, 2]);
        assert_eq!(visual_order(&[]), Vec::<usize>::new());
    }

    /// The words of `words`, a space between two, in the order they are seen
    /// from left to right on one line
    fn words_left_to_right(words: &[&str], direction: Option<super::BidiDirection>) -> Vec<usize> {
        let mut text = String::new();
        let mut starts = Vec::new();
        for (i, w) in words.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            starts.push(text.len());
            text.push_str(w);
        }
        super::visual_order_of_pieces(&text, &starts, direction)
    }

    #[test]
    fn the_words_of_a_name_written_right_to_left_are_seen_from_the_last() {
        // an Arabic name: its words from the last to the first
        assert_eq!(words_left_to_right(&["الطريق", "المداري", "الشمالي"], None), [2, 1, 0]);
        // Hebrew
        assert_eq!(words_left_to_right(&["תל", "אביב"], None), [1, 0]);
        // a Latin name: as written
        assert_eq!(words_left_to_right(&["Max-", "Eyth-", "Straße"], None), [0, 1, 2]);
    }

    #[test]
    fn the_arabic_part_of_a_latin_name_is_reversed_in_its_place() {
        assert_eq!(
            words_left_to_right(&["Avenue", "Hassan", "II", "شارع", "الحسن", "الثاني"], None),
            [0, 1, 2, 5, 4, 3]
        );
        // the pieces of "Casablanca-Agadir ... - أكادير" with the empty piece a
        // hyphen leaves: it takes the level of the space at its place
        assert_eq!(
            words_left_to_right(
                &["Autoroute", "Casablanca-", "Agadir", "طرق", "السيارات", "-", "", "أكادير"],
                None
            ),
            [0, 1, 2, 7, 6, 5, 4, 3]
        );
    }

    #[test]
    fn a_name_that_starts_in_arabic_reads_right_to_left() {
        // its Latin part on the left
        assert_eq!(words_left_to_right(&["أكادير", "Agadir"], None), [1, 0]);
        // a number in an Arabic name stays where it is read ("20 August Street")
        assert_eq!(words_left_to_right(&["شارع", "20", "غشت"], None), [2, 1, 0]);
        // a direction given: in a right-to-left paragraph a Latin part keeps
        // its order and stands at the right, where the paragraph starts
        assert_eq!(
            words_left_to_right(&["Oued", "Sebou"], Some(super::BidiDirection::Rtl)),
            [0, 1]
        );
        assert_eq!(
            words_left_to_right(&["Oued", "Sebou", "وادي", "سبو"], Some(super::BidiDirection::Rtl)),
            [3, 2, 0, 1]
        );
        assert_eq!(
            words_left_to_right(&["أكادير", "Agadir"], Some(super::BidiDirection::Ltr)),
            [0, 1]
        );
    }

    #[test]
    fn pieces_without_a_letter_keep_their_order() {
        assert_eq!(super::visual_order_of_pieces("", &[0, 0], None), [0, 1]);
        assert_eq!(super::visual_order_of_pieces("abc", &[0], None), [0]);
        assert_eq!(super::visual_order_of_pieces("12 34", &[0, 3], None), [0, 1]);
        // a piece past the end of the text is at the paragraph's level
        assert_eq!(super::visual_order_of_pieces("ab", &[0, 5], None), [0, 1]);
    }
}

/// Guards of the 2026-11-21 audit, area A8 (`audit/A8.md`).
#[cfg(test)]
mod audit_a8_tests {
    use super::*;

    /// `LogicalLine::into_visual` before A8-19, kept as the reference.
    fn old_into_visual(line: LogicalLine, base: BidiDirection) -> VisualLine {
        let paragraph = BidiLevel::paragraph(base).level();
        let mut levels: Vec<u8> = line
            .0
            .iter()
            .map(|item| item.bidi_level().map_or(paragraph, |level| level.level()))
            .collect();
        let mut ends_a_segment = true;
        for (item, level) in line.0.iter().zip(levels.iter_mut()).rev() {
            if matches!(item, ShapedItem::Tab { .. } | ShapedItem::Break { .. }) {
                *level = paragraph;
                ends_a_segment = true;
            } else if ends_a_segment && is_bidi_white_space(item) {
                *level = paragraph;
            } else {
                ends_a_segment = false;
            }
        }
        let mut items: Vec<Option<ShapedItem>> = line.0.into_iter().map(Some).collect();
        VisualLine(
            visual_order(&levels)
                .into_iter()
                .filter_map(|index| items[index].take())
                .collect(),
        )
    }

    /// A8-19: every line is put in the visual order it was put in before -
    /// lines that reorder nothing (all at level 0, all at one even level, a
    /// right-to-left paragraph's left-to-right text) through the new early
    /// return, the others through the reordering - for both paragraph
    /// directions, with trailing and inner white space.
    /// Mutation: `highest < (lowest | 1)` -> `highest <= (lowest | 1)` (a line
    /// at levels 0 and 1 kept in logical order) fails it.
    #[test]
    fn a_line_is_put_in_the_visual_order_it_was_put_in_before() {
        let texts = ["Hauptstrasse", "Am Neckar 12 ", " a b  c ", "one-two three", "x"];
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % n) as u8
        };
        for text in texts {
            let shaped = super::super::break_opportunities::audit_a8_tests::shaped(text);
            for base in [BidiDirection::Ltr, BidiDirection::Rtl] {
                for pattern in 0..8 {
                    let mut items = shaped.clone();
                    for (i, item) in items.iter_mut().enumerate() {
                        if let ShapedItem::Cluster(cluster) = item {
                            let level = match pattern {
                                0 => 0,
                                1 => 1,
                                2 => 2,
                                3 => if i % 2 == 0 { 0 } else { 2 },
                                4 => if i < 3 { 1 } else { 2 },
                                _ => next(4),
                            };
                            cluster.bidi_level = BidiLevel::new(level);
                        }
                    }
                    let line = LogicalLine::new(items);
                    assert_eq!(
                        line.clone().into_visual(base),
                        old_into_visual(line, base),
                        "{text:?} {base:?} pattern {pattern}"
                    );
                }
            }
        }
        assert_eq!(LogicalLine::default().into_visual(BidiDirection::Ltr), VisualLine::default());
    }
}
