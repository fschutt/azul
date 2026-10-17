//! Character classes the line breaker and the justifier ask about: white space, word separators, cursive scripts.

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

/// Returns true if the item is collapsible whitespace per CSS Text 3 §4.1.2 Phase II.
///
/// This is used for stripping leading/trailing whitespace at line edges —
/// distinct from `is_word_separator` which is for word-spacing per §7.1.
#[must_use]
pub fn is_collapsible_whitespace(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text().chars().all(|ch| {
            matches!(
                ch,
                ' ' | '\t' | '\u{1680}' // Ogham space mark (collapsible per spec)
            )
        })
    } else {
        false
    }
}

// +spec:text-alignment-spacing:456643 - cursive scripts do not admit letter-spacing gaps
/// Returns true if the cluster's first character belongs to a cursive script
/// (Arabic, Syriac, Mongolian, N'Ko, Mandaic, Phags Pa, Hanifi Rohingya)
/// per CSS Text 3 Appendix D.
///
/// These scripts should not have letter-spacing applied.
pub fn is_cursive_script_cluster(c: &ShapedCluster) -> bool {
    c.text().chars().next().is_some_and(is_cursive_script_char)
}

pub(super) fn is_cursive_script_char(ch: char) -> bool {
    let cp = ch as u32;
    // Arabic (U+0600–U+06FF, U+0750–U+077F, U+08A0–U+08FF, U+FB50–U+FDFF, U+FE70–U+FEFF)
    if (0x0600..=0x06FF).contains(&cp) {
        return true;
    }
    if (0x0750..=0x077F).contains(&cp) {
        return true;
    }
    if (0x08A0..=0x08FF).contains(&cp) {
        return true;
    }
    if (0xFB50..=0xFDFF).contains(&cp) {
        return true;
    }
    if (0xFE70..=0xFEFF).contains(&cp) {
        return true;
    }
    // Syriac (U+0700–U+074F)
    if (0x0700..=0x074F).contains(&cp) {
        return true;
    }
    // Mongolian (U+1800–U+18AF)
    if (0x1800..=0x18AF).contains(&cp) {
        return true;
    }
    // N'Ko (U+07C0–U+07FF)
    if (0x07C0..=0x07FF).contains(&cp) {
        return true;
    }
    // Mandaic (U+0840–U+085F)
    if (0x0840..=0x085F).contains(&cp) {
        return true;
    }
    // Phags Pa (U+A840–U+A87F)
    if (0xA840..=0xA87F).contains(&cp) {
        return true;
    }
    // Hanifi Rohingya (U+10D00–U+10D3F)
    if (0x10D00..=0x10D3F).contains(&cp) {
        return true;
    }
    false
}

/// Word-segmentation predicate shared by word selection (double-click) and word
/// cursor motion (Ctrl/Alt+Arrow) so they agree on what a "word" is.
///
/// A word character is alphanumeric or underscore; everything else — whitespace
/// AND punctuation — is a word boundary. This is deliberately distinct from
/// [`is_word_separator`] (which classifies *spacing* characters for word-spacing
/// justification per CSS Text §7.1, and treats punctuation as non-separator).
/// Used by `selection::find_word_boundaries` and `UnifiedLayout::move_cursor_to_*_word`.
pub(crate) fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// True when a shaped cluster is a word-segmentation boundary (whitespace or
/// punctuation), i.e. it contains no word characters. Keeps cursor word-motion
/// consistent with `selection::find_word_boundaries`.
pub(super) fn cluster_is_word_boundary(cluster: &ShapedCluster) -> bool {
    !cluster.text().chars().any(is_word_char)
}

// exclude punctuation and fixed-width spaces (U+3000, U+2000..U+200A)
#[must_use]
pub const fn is_word_separator(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        // Precomputed at shaping — see ClusterFlags.
        c.flags.has(ClusterFlags::WORD_SEPARATOR)
    } else {
        false
    }
}

/// True for separators that add word-spacing but must NOT offer a soft-wrap opportunity.
///
/// (UAX#14 class GL/WJ): NBSP, NARROW NO-BREAK SPACE, WORD JOINER, ZWNBSP. These are a
/// subset of `is_word_separator` — they still contribute Glue, but no break Penalty.
#[must_use]
pub const fn is_no_break_space(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.flags.has(ClusterFlags::NO_BREAK_SPACE)
    } else {
        false
    }
}

// +spec:margin-collapsing:6706c1 - fixed-width spaces (U+2000–U+200A, U+3000) excluded from word
// separators
/// Returns true if the character is a word-separator character per CSS Text §7.1.
/// Punctuation and fixed-width spaces (U+3000, U+2000 through U+200A) are NOT
/// word-separator characters even though they may visually separate words.
// +spec:text-alignment-spacing:3e0655 - word-separator characters for word-spacing
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant
                                  // (or cross-type bindings that can't merge)
pub(super) const fn is_word_separator_char(c: char) -> bool {
    match c {
        // Standard ASCII space
        '\u{0020}' => true,
        // NO-BREAK SPACE
        '\u{00A0}' => true,
        // OGHAM SPACE MARK
        '\u{1680}' => true,
        // ETHIOPIC WORDSPACE (spec §7.1)
        '\u{1361}' => true,
        // Fixed-width spaces: NOT word separators per spec
        '\u{2000}'..='\u{200A}' => false,
        // NARROW NO-BREAK SPACE
        '\u{202F}' => true,
        // MEDIUM MATHEMATICAL SPACE
        '\u{205F}' => true,
        // IDEOGRAPHIC SPACE: NOT a word separator per spec
        '\u{3000}' => false,
        // AEGEAN WORD SEPARATOR LINE (spec §7.1)
        '\u{10100}' => true,
        // AEGEAN WORD SEPARATOR DOT (spec §7.1)
        '\u{10101}' => true,
        // UGARITIC WORD DIVIDER (spec §7.1)
        '\u{1039F}' => true,
        // PHOENICIAN WORD SEPARATOR (spec §7.1)
        '\u{1091F}' => true,
        // Other Unicode whitespace not listed above
        _ => false,
    }
}

/// Helper to identify if an item is a zero-width space (U+200B),
/// which provides a soft wrap opportunity with no visible width.
///
/// Used in scripts like Thai, Lao, and Khmer that don't use spaces between words.
// +spec:line-breaking:fd3164 - U+200B as explicit word delimiter for scripts without
// space-separated words
#[must_use]
pub const fn is_zero_width_space(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.flags.has(ClusterFlags::ZERO_WIDTH_SPACE)
    } else {
        false
    }
}
