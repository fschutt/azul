//! Line break opportunities (UAX 14 with CSS word-break and line-break) and the cursor that walks them.

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

// +spec:inline-block:6e7dd9 - Non-tailorable Unicode line breaking controls take precedence over
// atomic inline rules (CSS-TEXT-3 recent changes, issue 8972)

pub(super) const fn is_break_suppressing_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{200D}' | // ZERO WIDTH JOINER
        '\u{2060}' | // WORD JOINER
        '\u{FEFF}' // ZERO WIDTH NO-BREAK SPACE
    )
}

pub(super) const fn is_break_forcing_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}' | // ZERO WIDTH SPACE (already handled but included for completeness)
        '\u{2028}' | // LINE SEPARATOR
        '\u{2029}' // PARAGRAPH SEPARATOR
    )
}

// +spec:line-breaking:495247 - CJK/syllabic writing systems allow breaks between typographic letter
// units with varying strictness §5.2 word-break: determines if a character is CJK ideograph/kana
pub(super) const fn is_cjk_character(ch: char) -> bool {
    let cp = ch as u32;
    matches!(cp,
        // CJK Unified Ideographs
        0x4E00..=0x9FFF |
        // CJK Unified Ideographs Extension A
        0x3400..=0x4DBF |
        // CJK Unified Ideographs Extension B
        0x20000..=0x2A6DF |
        // CJK Compatibility Ideographs
        0xF900..=0xFAFF |
        // Hiragana
        0x3040..=0x309F |
        // Katakana
        0x30A0..=0x30FF |
        // Katakana Phonetic Extensions
        0x31F0..=0x31FF |
        // CJK Symbols and Punctuation
        0x3000..=0x303F |
        // Halfwidth and Fullwidth Forms
        0xFF00..=0xFFEF |
        // Hangul Syllables
        0xAC00..=0xD7AF
    )
}

// §5.2 word-break: checks if a cluster contains CJK characters
pub(super) const fn is_cjk_cluster(cluster: &ShapedCluster) -> bool {
    cluster.flags.has(ClusterFlags::HAS_CJK)
}

/// Whether `cluster` is a soft hyphen (U+00AD).
///
/// A soft hyphen is a place its word may break, and a word that holds one
/// breaks only at its soft hyphens: the dictionary's breaks of it are
/// ignored.
pub(super) const fn is_soft_hyphen(cluster: &ShapedCluster) -> bool {
    cluster.flags.has(ClusterFlags::SOFT_HYPHEN_START)
}

// +spec:line-breaking:e1fc9d - word-break normal/break-all/keep-all break opportunity rules
// +spec:line-breaking:73d5fe - word-break break-point determination for CJK and Latin text
// +spec:line-breaking:31ef1a - word-break property controls soft wrap opportunities between letters
// (NU/AL/AI/ID classes as letter units) +spec:line-breaking:798252 - word-break property affects
// break opportunities (normal/break-all/keep-all) +spec:line-breaking:8fed57 - word-break:
// break-all treats all clusters as break opportunities, keep-all suppresses CJK breaks
// +spec:line-breaking:e2b374 - word-break: normal (only at separators) vs break-all (between all
// letters incl. Ethiopic) +spec:overflow:53a97f - word-break (normal/break-all/keep-all) and
// line-break strictness rules +spec:line-breaking:1c830a - word-break: normal/break-all/keep-all
// break opportunity rules §5.2 word-break property: break opportunity logic
// +spec:line-breaking:a75147 - word-break property: normal (CJK breaks), break-all (every cluster),
// keep-all (suppress CJK breaks) +spec:line-breaking:65ab41 - word-break: normal/break-all/keep-all
// break opportunity rules +spec:line-breaking:7eca16 - U+200B ZERO WIDTH SPACE is always a break
// opportunity, even with keep-all
pub(crate) fn is_break_opportunity_with_word_break(
    item: &ShapedItem,
    word_break: WordBreak,
    hyphens: Hyphens,
) -> bool {
    // No-break spaces (UAX#14 class GL/WJ) are word separators for word-spacing
    // purposes but must NOT offer a soft-wrap opportunity. This is the segmentation
    // path used by BreakCursor::peek_next_unit, so it must suppress them the same way
    // the dedicated is_break_opportunity() does; otherwise "10\u{00A0}km" wrongly wraps.
    if let ShapedItem::Cluster(c) = item {
        if c.flags.has(ClusterFlags::NO_BREAK_SPACE) {
            return false;
        }
    }
    // Break after spaces or explicit break items (always, regardless of word-break).
    if is_word_separator(item) {
        return true;
    }
    if let ShapedItem::Break { .. } = item {
        return true;
    }
    // +spec:line-breaking:432d5b - hyphens property controls soft wrap opportunities via
    // hyphenation +spec:line-breaking:5a32a1 - soft hyphen (U+00AD) creates break opportunity;
    // glyph styled per surrounding text properties U+200B ZERO WIDTH SPACE is always a soft
    // wrap opportunity regardless of word-break. This allows authors to mark explicit wrap
    // points (e.g. with <wbr> or &#x200B;) even when using word-break: keep-all to suppress
    // other breaks.
    if is_zero_width_space(item) {
        return true;
    }
    // only when hyphens != none. With hyphens:none, soft hyphens do not create break points.
    if hyphens != Hyphens::None && item.as_cluster().is_some_and(is_soft_hyphen) {
        return true;
    }

    // +spec:line-breaking:05e09a - U+002D HYPHEN-MINUS / U+2010 HYPHEN always create a
    // soft-wrap opportunity AFTER them (UAX#14 class HY/BA), independent of the hyphens
    // property (they are NOT hyphenation opportunities — no extra glyph is inserted).
    // U+002F SOLIDUS (UAX#14 class SY) may offer one after it too. Whether the
    // line breaks after them, and never before, is `is_soft_wrap_between`'s.
    if let ShapedItem::Cluster(c) = item {
        if c.flags.has(ClusterFlags::ENDS_BREAKABLE) {
            return true;
        }
    }

    // +spec:line-breaking:2bbda0 - word-break does not affect soft wrap opportunities around
    // punctuation
    match word_break {
        WordBreak::Normal => {
            // CJK characters are implicit break opportunities in normal mode.
            if let ShapedItem::Cluster(c) = item {
                if is_cjk_cluster(c) {
                    return true;
                }
            }
            false
        }
        WordBreak::BreakAll => {
            // Every typographic letter unit is a break opportunity.
            if let ShapedItem::Cluster(_) = item {
                return true;
            }
            false
        }
        WordBreak::KeepAll => {
            // +spec:line-breaking:aa3044 - keep-all suppresses CJK (incl. Korean) inter-character
            // breaks Only break at spaces/hyphens (already handled above).
            false
        }
    }
}

/// Whether a line may break between `prev` and `next`, two items next to
/// each other in a paragraph; `before` is the item before `prev`, if any.
///
/// The one rule of soft wrap opportunities both line breakers and the
/// intrinsic sizes keep: the greedy breaker ends its units there, the
/// balanced one puts a penalty there. An item that breaks on both its sides
/// (`breaks_around`) offers a break after it, and one before it unless a
/// joiner or the `line-break` strictness keeps it with the item before. A
/// hyphen or a solidus offers a break after it only, by the rules of
/// `breaks_after_hyphen_or_solidus`. A soft hyphen is none here: it is a
/// break inside its word, where a hyphen ends the line, and the word's
/// hyphenation breaks hold it (`find_all_hyphenation_breaks`).
pub(crate) fn is_soft_wrap_between(
    before: Option<&ShapedItem>,
    prev: &ShapedItem,
    next: &ShapedItem,
    word_break: WordBreak,
    line_break: LineBreakStrictness,
) -> bool {
    if breaks_around(prev, word_break)
        || (word_break == WordBreak::BreakAll && matches!(prev, ShapedItem::Cluster(_)))
    {
        return true;
    }
    let prev_text = prev.as_cluster().map_or("", ShapedCluster::text);
    let first = next.as_cluster().and_then(|c| c.text().chars().next());
    if prev_text
        .chars()
        .next_back()
        .is_some_and(is_break_suppressing_control)
        || first.is_some_and(is_break_suppressing_control)
    {
        return false;
    }
    if breaks_around(next, word_break)
        && first.is_none_or(|ch| is_cjk_break_allowed_by_strictness(ch, None, line_break))
    {
        return true;
    }
    prev.as_cluster()
        .is_some_and(|c| c.flags.has(ClusterFlags::ENDS_BREAKABLE))
        && breaks_after_hyphen_or_solidus(before, prev_text, first)
}

/// Whether `item` is a soft wrap opportunity on both its sides: a space, a
/// forced break, a zero width space, a CJK ideograph, any letter with
/// `word-break: break-all`.
///
/// A hyphen and a solidus are one after them only, and a soft hyphen is
/// none: `Hyphens::None` keeps it out, the one item that setting decides.
pub(super) fn breaks_around(item: &ShapedItem, word_break: WordBreak) -> bool {
    !item
        .as_cluster()
        .is_some_and(|c| c.flags.has(ClusterFlags::ENDS_BREAKABLE))
        && is_break_opportunity_with_word_break(item, word_break, Hyphens::None)
}

/// Whether a line may break after the hyphen or solidus that ends
/// `prev_text`, before the character `next_first`; `before` is the item
/// before it.
///
/// A hyphen breaks after it (UAX 14), except a minus sign: a hyphen-minus
/// before a digit with no letter or digit in front of it keeps its number
/// whole ("-12"); between two words it breaks, "ab-12" as Chrome does. A
/// solidus breaks after it only before a character outside ASCII that is
/// no digit, as in Chrome: its line breaking table for ASCII text keeps
/// "and/or", a path and a URL whole, and its ICU rules keep "1/2" whole and
/// break "Ελλάδα/Κύπρος" after the solidus.
fn breaks_after_hyphen_or_solidus(
    before: Option<&ShapedItem>,
    prev_text: &str,
    next_first: Option<char>,
) -> bool {
    let mut chars = prev_text.chars();
    match chars.next_back() {
        Some('/') => next_first.is_some_and(|ch| !ch.is_ascii() && !ch.is_numeric()),
        Some('-') if next_first.is_some_and(char::is_numeric) => chars
            .next_back()
            .or_else(|| {
                before
                    .and_then(ShapedItem::as_cluster)
                    .and_then(|c| c.text().chars().next_back())
            })
            .is_some_and(char::is_alphanumeric),
        _ => true,
    }
}

// +spec:line-breaking:db0289 - line-break strictness: anywhere allows soft wrap around every
// typographic character unit +spec:line-breaking:7d242b - line-break strictness levels:
// loose/normal/strict/anywhere with CJK punctuation rules +spec:line-breaking:67bfe8 - line-break
// strictness (auto/loose/normal/strict/anywhere) controls CSS Text Level 3 §5.3: Determines whether
// a break opportunity before a character is allowed based on the line-break strictness level. The
// spec defines:
// - strict: forbids breaks before small kana (class CJ), CJK hyphens, and certain punctuation
// - normal: allows breaks before small kana (CJ); allows CJK hyphen breaks for CJK writing systems
// - loose: additionally allows breaks before hyphens U+2010/U+2013 after ID-class chars
// - anywhere: allows soft wrap around every typographic character unit
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant
                                  // (or cross-type bindings that can't merge)
pub(super) const fn is_cjk_break_allowed_by_strictness(
    ch: char,
    _prev_ch: Option<char>,
    strictness: LineBreakStrictness,
) -> bool {
    match strictness {
        LineBreakStrictness::Anywhere => true,
        LineBreakStrictness::Loose => {
            // Loose allows breaks before hyphens U+2010, U+2013 when preceded by ID-class chars
            // Also allows breaks before small kana (CJ class) and CJK hyphens
            true
        }
        LineBreakStrictness::Normal | LineBreakStrictness::Auto => {
            // Normal forbids breaks before hyphens U+2010/U+2013 for non-CJK text
            // but allows breaks before small kana (CJ) and CJK hyphen-like chars
            // (〜 U+301C, ゠ U+30A0) for CJK writing systems
            match ch {
                '\u{2010}' | '\u{2013}' => false, // hyphens forbidden in normal
                _ => true,
            }
        }
        LineBreakStrictness::Strict => {
            // Strict forbids breaks before:
            // - Small kana and prolonged sound mark (Unicode line break class CJ)
            // - CJK hyphen-like characters: 〜 U+301C, ゠ U+30A0
            // - Hyphens: ‐ U+2010, – U+2013
            match ch {
                '\u{301C}' | '\u{30A0}' => false, // CJK hyphen-like
                '\u{2010}' | '\u{2013}' => false, // hyphens
                c if is_small_kana(c) => false,
                _ => true,
            }
        }
    }
}

/// Returns true if the character is a Japanese small kana or Katakana-Hiragana prolonged sound mark
/// (Unicode line break class CJ). These are forbidden break points in strict line breaking.
pub(super) const fn is_small_kana(ch: char) -> bool {
    matches!(
        ch,
        '\u{3041}' | // ぁ HIRAGANA LETTER SMALL A
        '\u{3043}' | // ぃ HIRAGANA LETTER SMALL I
        '\u{3045}' | // ぅ HIRAGANA LETTER SMALL U
        '\u{3047}' | // ぇ HIRAGANA LETTER SMALL E
        '\u{3049}' | // ぉ HIRAGANA LETTER SMALL O
        '\u{3063}' | // っ HIRAGANA LETTER SMALL TU
        '\u{3083}' | // ゃ HIRAGANA LETTER SMALL YA
        '\u{3085}' | // ゅ HIRAGANA LETTER SMALL YU
        '\u{3087}' | // ょ HIRAGANA LETTER SMALL YO
        '\u{308E}' | // ゎ HIRAGANA LETTER SMALL WA
        '\u{3095}' | // ゕ HIRAGANA LETTER SMALL KA
        '\u{3096}' | // ゖ HIRAGANA LETTER SMALL KE
        '\u{30A1}' | // ァ KATAKANA LETTER SMALL A
        '\u{30A3}' | // ィ KATAKANA LETTER SMALL I
        '\u{30A5}' | // ゥ KATAKANA LETTER SMALL U
        '\u{30A7}' | // ェ KATAKANA LETTER SMALL E
        '\u{30A9}' | // ォ KATAKANA LETTER SMALL O
        '\u{30C3}' | // ッ KATAKANA LETTER SMALL TU
        '\u{30E3}' | // ャ KATAKANA LETTER SMALL YA
        '\u{30E5}' | // ュ KATAKANA LETTER SMALL YU
        '\u{30E7}' | // ョ KATAKANA LETTER SMALL YO
        '\u{30EE}' | // ヮ KATAKANA LETTER SMALL WA
        '\u{30F5}' | // ヵ KATAKANA LETTER SMALL KA
        '\u{30F6}' | // ヶ KATAKANA LETTER SMALL KE
        '\u{30FC}' // ー KATAKANA-HIRAGANA PROLONGED SOUND MARK
    )
}

// for every typographic character unit, disregarding GL/WJ/ZWJ line breaking classes
// replaced element or other atomic inline for web-compat
pub(crate) fn is_break_opportunity(item: &ShapedItem) -> bool {
    // Per CSS Text 3 §5.1: "there is a soft wrap opportunity before and
    // after each replaced element or other atomic inline"
    if matches!(
        item,
        ShapedItem::Object { .. } | ShapedItem::CombinedBlock { .. }
    ) {
        return true;
    }
    // over atomic inline rules: break-forcing controls (ZWSP, LS, PS) create break opportunities
    // even adjacent to atomic inlines, while break-suppressing controls (WJ, ZWJ, ZWNBSP)
    // prevent breaks
    if let ShapedItem::Cluster(c) = item {
        // ZW (zero-width space U+200B) is always a break opportunity
        if c.text().contains('\u{200B}') {
            return true;
        }
        // Break-forcing Unicode controls (LS, PS) create break opportunities
        if c.text().chars().any(is_break_forcing_control) {
            return true;
        }
        // WJ (word joiner U+2060), ZWJ (U+200D), and GL (NBSP U+00A0) suppress breaks
        if c.text()
            .chars()
            .any(|ch| matches!(ch, '\u{2060}' | '\u{200D}' | '\u{00A0}'))
        {
            return false;
        }
        // +spec:line-breaking:05e09a - U+002D/U+2010 always create soft wrap opportunities
        // regardless of hyphens property are always visible and create a soft wrap
        // opportunity after them, but are NOT hyphenation opportunities (no extra glyph is
        // inserted at the break).
        if c.text().ends_with('\u{002D}') || c.text().ends_with('\u{2010}') {
            return true;
        }
    }
    is_break_opportunity_with_word_break(item, WordBreak::Normal, Hyphens::Manual)
}

/// Where a [`BreakCursor`] stands in its items, owned: the next whole item
/// and the rest of a word the line before hyphenated, with that word's
/// later breaks.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorPosition {
    /// The index of the next whole item.
    pub next_item_index: usize,
    /// The rest of the word the line before broke, read before that item.
    pub partial_remainder: Vec<ShapedItem>,
    /// Where that rest may still be hyphenated (see
    /// [`BreakCursor::partial_remainder_breaks`]).
    pub partial_remainder_breaks: Option<Vec<usize>>,
}

// A cursor to manage the state of the line breaking process.
// This allows us to handle items that are partially consumed by hyphenation.
// `Clone` is used to take a cheap snapshot for the multi-column balancing dry run
// (measuring total line count without consuming the real cursor).
#[derive(Debug, Clone)]
pub struct BreakCursor<'a> {
    /// A reference to the complete list of shaped items.
    pub items: &'a [ShapedItem],
    /// The index of the next *full* item to be processed from the `items` slice.
    pub next_item_index: usize,
    /// The remainder of an item that was split by hyphenation on the previous line.
    /// This will be the very first piece of content considered for the next line.
    pub partial_remainder: Vec<ShapedItem>,
    /// Where the word the remainder is the rest of may still be hyphenated:
    /// the index of the remainder's cluster each of its line parts ends
    /// before. These are the breaks the whole word had past the part the
    /// line before took (soft hyphens or the dictionary's), so the rest of a
    /// word breaks where the word does, and not where the dictionary breaks
    /// the rest as a word of its own. `None` when they are not known.
    pub partial_remainder_breaks: Option<Vec<usize>>,
    // §5.2 word-break property stored on cursor
    pub word_break: WordBreak,
    pub hyphens: Hyphens,
    pub line_break: LineBreakStrictness,
}

impl<'a> BreakCursor<'a> {
    #[must_use]
    pub fn new(items: &'a [ShapedItem]) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
            partial_remainder_breaks: None,
            word_break: WordBreak::Normal,
            hyphens: Hyphens::default(),
            line_break: LineBreakStrictness::default(),
        }
    }

    #[must_use]
    pub fn with_word_break(items: &'a [ShapedItem], word_break: WordBreak) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
            partial_remainder_breaks: None,
            word_break,
            hyphens: Hyphens::default(),
            line_break: LineBreakStrictness::default(),
        }
    }

    /// A cursor over `items` that breaks them as `constraints` say, by their
    /// `word-break`, `hyphens` and `line-break`.
    ///
    /// Every line breaker of a paragraph starts here, its first layout and a
    /// relayout after an edit alike, so the lines of both follow the same
    /// style.
    #[must_use]
    pub const fn for_constraints(items: &'a [ShapedItem], constraints: &UnifiedConstraints) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
            partial_remainder_breaks: None,
            word_break: constraints.word_break,
            hyphens: constraints.hyphenation,
            line_break: constraints.line_break,
        }
    }

    /// Where the cursor stands: what a line breaker restores to take back
    /// a line it broke.
    #[must_use]
    pub fn position(&self) -> CursorPosition {
        CursorPosition {
            next_item_index: self.next_item_index,
            partial_remainder: self.partial_remainder.clone(),
            partial_remainder_breaks: self.partial_remainder_breaks.clone(),
        }
    }

    /// Puts the cursor back where `position` says it stood.
    pub fn restore(&mut self, position: CursorPosition) {
        self.next_item_index = position.next_item_index;
        self.partial_remainder = position.partial_remainder;
        self.partial_remainder_breaks = position.partial_remainder_breaks;
    }

    /// Checks if the cursor is at the very beginning of the content stream.
    #[must_use]
    pub const fn is_at_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_empty()
    }

    /// Consumes the cursor and returns all remaining items as a `Vec`.
    pub fn drain_remaining(&mut self) -> Vec<ShapedItem> {
        let mut remaining = std::mem::take(&mut self.partial_remainder);
        self.partial_remainder_breaks = None;
        if self.next_item_index < self.items.len() {
            remaining.extend_from_slice(&self.items[self.next_item_index..]);
        }
        self.next_item_index = self.items.len();
        remaining
    }

    /// Checks if all content, including any partial remainders, has been processed.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.next_item_index >= self.items.len() && self.partial_remainder.is_empty()
    }

    /// Consumes a number of items from the cursor's stream.
    pub fn consume(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        let remainder_len = self.partial_remainder.len();
        if count <= remainder_len {
            // Consuming only from the remainder; its breaks move with its start.
            self.partial_remainder.drain(..count);
            self.partial_remainder_breaks = self
                .partial_remainder_breaks
                .take()
                .filter(|_| !self.partial_remainder.is_empty())
                .map(|breaks| {
                    breaks
                        .into_iter()
                        .filter(|&split| split > count)
                        .map(|split| split - count)
                        .collect()
                });
        } else {
            // Consuming all of the remainder and some from the main list.
            let from_main_list = count - remainder_len;
            self.partial_remainder.clear();
            self.partial_remainder_breaks = None;
            self.next_item_index += from_main_list;
        }
    }

    /// Looks ahead and returns the next "unbreakable" unit of content.
    /// This is typically a word (a series of non-space clusters) followed by a
    /// space, or just a single space if that's next.
    /// The definition of "unbreakable unit" depends on the word-break property.
    // a single typographic character unit (every character is a soft wrap opportunity), including
    // punctuation and preserved white spaces; currently handled via peek_next_single_item
    #[must_use]
    pub fn peek_next_unit(&self) -> Vec<ShapedItem> {
        let mut unit = Vec::new();
        // The remaining stream, WITHOUT materializing it. This used to be
        // `partial_remainder.clone()` + `extend_from_slice(rest)` — a deep
        // clone of every remaining ShapedItem (each carrying a String and a
        // Vec<Glyph>) on EVERY call. The line breaker calls this once per
        // word, so laying out an N-cluster paragraph performed O(N²) deep
        // clones: ~90% of line-breaking time on an ordinary document, and
        // line breaking was ~40% of a full pagination. Only the items that
        // actually enter `unit` (one word) are cloned now.
        let mut stream = self
            .partial_remainder
            .iter()
            .chain(self.items[self.next_item_index..].iter())
            .peekable();

        // The unit runs up to the next soft wrap opportunity, by the rule the
        // balanced breaker keeps too (`is_soft_wrap_between`).
        let mut before = None;
        while let Some(item) = stream.next() {
            unit.push(item.clone());
            let ends_here = stream.peek().is_none_or(|next| {
                is_soft_wrap_between(before, item, next, self.word_break, self.line_break)
            });
            if ends_here {
                break;
            }
            before = Some(item);
        }
        unit
    }

    #[must_use]
    pub fn peek_next_single_item(&self) -> Vec<ShapedItem> {
        if !self.partial_remainder.is_empty() {
            return vec![self.partial_remainder[0].clone()];
        }
        if self.next_item_index < self.items.len() {
            return vec![self.items[self.next_item_index].clone()];
        }
        Vec::new()
    }
}
