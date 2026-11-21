//! Measuring items along a line: advances with letter and word spacing, inline box edges, line widths.

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

/// Helper to get the primary measure (width or height) of a shaped item.
#[must_use]
pub fn get_item_measure(item: &ShapedItem, is_vertical: bool) -> f32 {
    match item {
        ShapedItem::Cluster(c) => {
            // Total width = base advance + kerning adjustments
            // Kerning is stored separately in glyphs for inspection, but the total
            // cluster width must include it for correct layout positioning
            let total_kerning: f32 = c.glyphs.iter().map(|g| g.kerning).sum();
            c.advance + total_kerning
        }
        ShapedItem::Object { bounds, .. }
        | ShapedItem::CombinedBlock { bounds, .. }
        | ShapedItem::Tab { bounds, .. } => {
            if is_vertical {
                bounds.height
            } else {
                bounds.width
            }
        }
        ShapedItem::Break { .. } => 0.0,
    }
}

/// Like [`get_item_measure`] but ALSO includes the per-cluster letter-spacing and
/// per-separator word-spacing that `position_one_line` adds after each cluster.
///
/// Line breaking and center/right alignment must measure the SAME width the text is
/// finally positioned at; `get_item_measure` alone omits letter/word-spacing, so a run
/// that "just fits" without spacing overflows its box (or mis-aligns) once the spacing
/// is applied. Selection/caret geometry must NOT include the trailing spacing, so those
/// callers keep using the bare `get_item_measure`.
#[must_use]
pub fn get_item_measure_with_spacing(item: &ShapedItem, is_vertical: bool) -> f32 {
    let base = get_item_measure(item, is_vertical);
    if let ShapedItem::Cluster(c) = item {
        let mut extra = 0.0;
        if !is_cursive_script_cluster(c) {
            extra += c.style.letter_spacing.resolve_px(c.style.font_size_px);
        }
        if is_word_separator(item) {
            extra += c.style.word_spacing.resolve_px(c.style.font_size_px);
        }
        base + extra
    } else {
        base
    }
}

/// The single fold that BOTH the intrinsic-size scan and the line breaker use
/// to accumulate a line's width.
///
/// f32 addition is not associative, so "measure a box at max-content, then lay
/// its text out at exactly that width" only guarantees a single line when both
/// passes fold the SAME per-item values in the SAME order onto the SAME running
/// total. Any other grouping - per-word subtotals, or a subtract-based fit test
/// like `unit <= available - current` - rounds differently by a few ULP, and a
/// box sized from its own measurement then wraps its last word (the ribbon's
/// "Format Painter" truncating to "Format").
///
/// Negative advances (pathological kerning) are clamped here so every consumer
/// agrees that a cluster cannot rewind the caret.
#[inline]
#[must_use]
pub fn fold_line_width(current: f32, item: &ShapedItem, is_vertical: bool) -> f32 {
    if is_hanging_marker(item) {
        return current;
    }
    current + get_item_measure_with_spacing(item, is_vertical).max(0.0)
}

/// A cluster of an OUTSIDE list marker: it hangs in the gutter before the
/// line (`position_one_line` places it at a negative offset and never
/// advances the pen for it), so it takes no room on the line - neither in
/// the line breaker's fit nor in the intrinsic widths. Counted there, a
/// list item as wide as its text (`width: fit-content`, shrink-to-fit) no
/// longer fitted its text beside its marker and wrapped it below.
pub(super) fn is_hanging_marker(item: &ShapedItem) -> bool {
    matches!(item, ShapedItem::Cluster(c) if c.marker_position_outside == Some(true))
}

/// How far the inline boxes (`<span>`s) around the cluster `items[idx]`
/// move the pen before and after it: the start margin + border + padding of
/// every box that begins at it, the end ones of every box that ends after
/// it, `(0, 0)` for every other item. A box begins where the cluster before
/// it is not inside it ([`StyleProperties::inline_boxes`] names the boxes by
/// node), so a span's edges sit around all of its text, the text of the
/// spans nested in it included, and the edges of the outer span count as
/// well as the inner one's.
///
/// The ONE rule the placement (`position_one_line`) and the intrinsic scan
/// (`measure_intrinsic_widths`) share: an inline box's padding widens its
/// line, so it widens the max-content a float or an inline-block shrinks to
/// (a float of `<span style="padding-left: 40px">Y</span>` is 40px wider
/// than one of `<span>Y</span>`, as in Chrome).
pub(super) fn inline_box_edge_advances(items: &[ShapedItem], idx: usize) -> (f32, f32) {
    inline_box_edge_advances_between(items, idx, None, None)
}

/// [`inline_box_edge_advances`] for `items` that are one stretch of a longer
/// list (a line): `before` / `after` are the boxes of the nearest clusters
/// beyond its ends, `None` where nothing comes. A box that goes on past the
/// end of a line is split there and has no edge at the split (CSS 2.2
/// s9.4.2).
pub(super) fn inline_box_edge_advances_between(
    items: &[ShapedItem],
    idx: usize,
    before: Option<&[InlineBoxEdge]>,
    after: Option<&[InlineBoxEdge]>,
) -> (f32, f32) {
    let Some(ShapedItem::Cluster(c)) = items.get(idx) else {
        return (0.0, 0.0);
    };
    let chain = &c.style.inline_boxes;
    if !chain.is_empty() {
        // The nearest clusters on either side: an atomic inline or a break
        // between two pieces of a span's text does not end the span.
        let prev = items[..idx]
            .iter()
            .rev()
            .find_map(ShapedItem::as_cluster)
            .map(|p| &*p.style.inline_boxes)
            .or(before);
        let next = items[idx + 1..]
            .iter()
            .find_map(ShapedItem::as_cluster)
            .map(|n| &*n.style.inline_boxes)
            .or(after);
        let inside = |boxes: Option<&[InlineBoxEdge]>, edge: &InlineBoxEdge| {
            boxes.is_some_and(|b| b.iter().any(|e| e.node == edge.node))
        };
        let left: f32 = chain
            .iter()
            .filter(|&e| !inside(prev, e))
            .map(|e| e.left)
            .sum();
        let right: f32 = chain
            .iter()
            .filter(|&e| !inside(next, e))
            .map(|e| e.right)
            .sum();
        return (left, right);
    }
    // A style built without its boxes (a test, a synthesized run): its own
    // box, told apart from its neighbours by the `Arc` its runs share.
    let Some(border) = c.style.border.as_ref().filter(|b| b.moves_the_pen()) else {
        return (0.0, 0.0);
    };
    let style_ptr = Arc::as_ptr(&c.style);
    let same_span = |other: Option<&ShapedItem>| {
        other
            .and_then(ShapedItem::as_cluster)
            .is_some_and(|o| Arc::as_ptr(&o.style) == style_ptr)
    };
    let prev = idx.checked_sub(1).and_then(|i| items.get(i));
    let left = if same_span(prev) {
        0.0
    } else {
        border.left_advance()
    };
    let right = if same_span(items.get(idx + 1)) {
        0.0
    } else {
        border.right_advance()
    };
    (left, right)
}
