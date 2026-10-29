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
#[must_use]
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
}
