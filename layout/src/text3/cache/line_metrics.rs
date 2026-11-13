//! The vertical metrics of items and lines: vertical alignment, baseline shifts and line heights.

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

// --- Stage 5 & 6 Implementation: Combined Layout Pass ---
// This section replaces the previous simple line breaking and positioning logic.

/// Extracts the per-item vertical-align from a `ShapedItem`.
///
/// For `Object` items (inline-blocks, images), this returns the alignment stored
/// in the original `InlineContent`. For text clusters and other items, returns `None`
/// to indicate the global `constraints.vertical_align` should be used.
pub(super) fn get_item_vertical_align(item: &ShapedItem) -> Option<VerticalAlign> {
    match item {
        ShapedItem::Object { content, .. } => match content {
            InlineContent::Image(img) => Some(img.alignment),
            InlineContent::Shape(shape) => Some(shape.alignment),
            _ => None,
        },
        // A text cluster carries its span's vertical-align on its style. A non-baseline
        // value (sub / super / length / percentage on an inline <span>) overrides the
        // line's default alignment so the cluster is shifted; baseline yields None so the
        // cluster keeps the line/IFC default.
        ShapedItem::Cluster(c) => match c.style.vertical_align {
            VerticalAlign::Baseline => None,
            va => Some(va),
        },
        _ => None,
    }
}

/// Approximate version of `get_item_vertical_metrics` for use without constraints (e.g.
/// `bounds()`). Uses 80/20 ascent/descent ratio as fallback for empty-glyph strut case.
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't
// merge)
#[must_use]
pub fn get_item_vertical_metrics_approx(item: &ShapedItem) -> (f32, f32) {
    // For non-empty clusters, delegate to the font-metrics-based calculation
    if let ShapedItem::Cluster(c) = item {
        if !c.glyphs.is_empty() {
            // Reuse the glyph-based calculation (same as get_item_vertical_metrics)
            let (asc, desc) =
                c.glyphs
                    .iter()
                    .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                        match glyph
                            .font_metrics
                            .inline_box_px(c.style.font_size_px, &c.style.line_height)
                        {
                            Some((item_asc, item_desc)) => {
                                (max_asc.max(item_asc), max_desc.max(item_desc))
                            }
                            None => (max_asc, max_desc),
                        }
                    });
            return (asc, desc);
        }
    }
    // Fallback for empty glyphs or non-cluster items
    match item {
        ShapedItem::Cluster(c) => {
            let lh = c
                .style
                .line_height
                .resolve(c.style.font_size_px, 0.0, 0.0, 0.0, 0);
            (lh * FALLBACK_ASCENT_RATIO, lh * FALLBACK_DESCENT_RATIO)
        }
        ShapedItem::CombinedBlock { bounds, .. } => (
            bounds.height * FALLBACK_ASCENT_RATIO,
            bounds.height * FALLBACK_DESCENT_RATIO,
        ),
        ShapedItem::Object { bounds, .. } => (bounds.height, 0.0),
        ShapedItem::Tab { bounds, .. } => (
            bounds.height * FALLBACK_ASCENT_RATIO,
            bounds.height * FALLBACK_DESCENT_RATIO,
        ),
        ShapedItem::Break { .. } => (0.0, 0.0),
    }
}

/// Gets the ascent (distance from baseline to top) and descent (distance from baseline to bottom)
/// for a single item, incorporating half-leading from line-height.
// +spec:box-model:37aeb2 - inline box margins/borders/padding do not affect line box height
// (leading model) +spec:display-property:184f0d - Inline box baseline derives from first available
// font metrics +spec:display-property:238bf5 - Inline box layout bounds from own text metrics, not
// child boxes +spec:display-property:29b194 - baseline determination for inline boxes (CSS Box
// Alignment 3 §9.1) +spec:display-property:2987db - per-glyph font metrics impact inline box layout
// bounds (line-height: normal caveat not yet distinguished)
/// +spec:display-property:fd42a9 - line-height affects line box contribution, not inline box size
// +spec:font-metrics:506abb - A/D from font metrics with half-leading: L = line-height - (A+D), A'
// = A + L/2, D' = D + L/2 +spec:font-metrics:773029 - ascent/descent font metrics used for baseline
// calculations (visual centering depends on these) +spec:font-metrics:f42870 - half-leading model:
// leading = line-height - (ascent + descent), distributed equally above/below +spec:writing-modes:
// 531c2e - UAs should use vertical baseline tables in vertical typographic modes
#[must_use]
pub fn get_item_vertical_metrics(
    item: &ShapedItem,
    constraints: &UnifiedConstraints,
) -> (f32, f32) {
    // (ascent, descent)
    match item {
        ShapedItem::Cluster(c) => {
            if c.glyphs.is_empty() {
                // +spec:display-property:626c86 - strut for inline box with no glyphs uses first
                // available font metrics +spec:line-height:0078fa - strut:
                // zero-width inline box with element's font/line-height
                // §10.8.1 strut: if inline box contains no glyphs, it is considered to
                // contain a strut with A and D of the element's first available font.
                // Half-leading: L = line-height - (A + D), shared by `split_leading`.
                // `normal` is the strut's own (A + D + gap of the container's
                // first available font, `UnifiedConstraints::resolved_line_height`),
                // not a 1.2em guess.
                let resolved_lh = match c.style.line_height {
                    LineHeight::Px(px) => px,
                    LineHeight::Normal => constraints.resolved_line_height(),
                };
                return split_leading(
                    resolved_lh,
                    constraints.strut_ascent,
                    constraints.strut_descent,
                );
            }
            // +spec:box-model:0b3e1f - inline non-replaced box height uses only line-height, not
            // vertical padding/border/margin +spec:display-property:80b900 - fallback
            // glyphs affect line box size via per-glyph metrics +spec:display-property:
            // d52f26 - layout bounds enclose all glyphs from highest A to deepest D
            // +spec:font-metrics:387751 - content area uses max ascenders/descenders across all
            // fonts +spec:font-metrics:790fd2 - half-leading: L = line-height - (A+D),
            // A' = A + L/2, D' = D + L/2 +spec:line-height:1ae6f5 - line-height on
            // non-replaced inline: half-leading model +spec:line-height:0078fa -
            // half-leading: L = line-height - (A+D), distributed equally above/below
            // +spec:line-height:32b3da - half-leading: L = line-height - AD, A' = A + L/2, D' = D +
            // L/2 §10.8.1: for each glyph determine A, D from font metrics,
            // then L = line-height - (A + D), and adjust: A' = A + L/2, D' = D + L/2.
            // Note: L may be negative.
            // +spec:height-calculation:eb98b5 - multi-font normal line-height uses max across glyph
            // metrics
            // A and D are the face's ROUNDED pixel metrics, as in Chrome
            // (`LayoutFontMetrics::inline_box_px` / `line_metrics_px`).
            c.glyphs
                .iter()
                .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                    match glyph
                        .font_metrics
                        .inline_box_px(c.style.font_size_px, &c.style.line_height)
                    {
                        Some((item_asc, item_desc)) => {
                            (max_asc.max(item_asc), max_desc.max(item_desc))
                        }
                        None => (max_asc, max_desc),
                    }
                })
        }
        ShapedItem::Object {
            bounds,
            baseline_offset,
            ..
        } => {
            // Per analysis, `baseline_offset` is the distance from the bottom.
            // bounds.height already includes margins (set from margin_box_height in fc.rs)
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent.max(0.0), descent.max(0.0))
        }
        ShapedItem::CombinedBlock {
            bounds,
            baseline_offset,
            ..
        } => {
            // CORRECTED: Treat baseline_offset consistently as distance from the bottom (descent).
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent.max(0.0), descent.max(0.0))
        }
        _ => (0.0, 0.0), // Breaks and other non-visible items don't affect line height.
    }
}

/// How far `vertical-align` moves a box's baseline DOWN from its line's
/// baseline (negative = raised), for the alignments CSS 2.1 s10.8.1 measures
/// from the parent's baseline; `None` for the line-relative `top` / `bottom`
/// (aligned once the line box is known). `ascent` and `descent` are the box's
/// own ([`get_item_vertical_metrics`]).
///
/// The ONE rule the line box (`calculate_line_metrics`) and the placement
/// (`position_one_line`) share, so a box always sits inside the line box it
/// was counted in. `middle` RAISES the box's midpoint to "the baseline of the
/// parent box plus half the x-height of the parent": the shift used to add the
/// half x-height, moving it down (a 24px `middle` icon in 16px text sat 16px
/// low, below its own line).
pub(super) fn baseline_shift(
    align: VerticalAlign,
    ascent: f32,
    descent: f32,
    constraints: &UnifiedConstraints,
) -> Option<f32> {
    match align {
        VerticalAlign::Baseline => Some(0.0),
        // midpoint (baseline + shift - ascent + (ascent + descent) / 2) at
        // baseline - x-height / 2
        VerticalAlign::Middle => Some((ascent - descent) / 2.0 - constraints.strut_x_height / 2.0),
        // top (baseline + shift - ascent) at the parent's content-area top,
        // baseline - strut ascent (s10.6.1)
        VerticalAlign::TextTop => Some(ascent - constraints.strut_ascent),
        // bottom (baseline + shift + descent) at the content-area bottom,
        // baseline + strut descent
        VerticalAlign::TextBottom => Some(constraints.strut_descent - descent),
        // <length> / <percentage>: raise (positive) or lower (negative)
        VerticalAlign::Offset(offset) => Some(-offset),
        // +spec:font-metrics:aa21f7 - sub / super: "a proper position" for
        // the parent's subscripts / superscripts - Chrome's (LayoutNG): the
        // parent font size / 5 + 1px down, / 3 + 1px up. The parent is the
        // block container here (its strut's font size). They were left to
        // the placement as 0.3 / 0.4 of the LINE's ascent and never counted
        // in the line box: a `<sup>` reached above its line.
        VerticalAlign::Sub => Some(constraints.strut_font_size / 5.0 + 1.0),
        VerticalAlign::Super => Some(-(constraints.strut_font_size / 3.0 + 1.0)),
        VerticalAlign::Top | VerticalAlign::Bottom => None,
    }
}

// +spec:block-formatting-context:861155 - vertical-align affects vertical positioning inside line
// box for inline-level elements
/// Calculates the maximum ascent and descent for an entire line of items.
/// This determines the "line box" used for vertical alignment.
/// // +spec:display-contents:66d910 - line box height fitted to contents, controlled by line-height
// +spec:inline-formatting-context:c3fc54 - line box tall enough for all boxes, vertical-align
// determines alignment within line box
///
/// Per CSS 2.2 §10.8: Inline-level boxes aligned 'top' or 'bottom' must be aligned
/// so as to minimize the line box height. The algorithm is:
/// 1. First pass: compute line box height from baseline-aligned items only (baseline, sub, super,
///    middle, text-top, text-bottom, offset).
/// 2. Second pass: check if any top/bottom-aligned items are taller than the line box from pass 1,
///    and expand if necessary.
// +spec:box-model:c9bcd7 - when line-fit-edge is not leading, layout bounds inflated by
// margin+border+padding (not yet implemented; default leading behavior is correct)
pub(super) fn calculate_line_metrics(
    items: &[ShapedItem],
    default_vertical_align: VerticalAlign,
    constraints: &UnifiedConstraints,
    (strut_above, strut_below): (f32, f32),
) -> (f32, f32) {
    // +spec:font-metrics:95152b - baseline alignment: items with different font sizes aligned by
    // matching alphabetic baselines Pass 1: Compute ascent/descent from baseline-aligned items
    // only (i.e., items that are NOT vertical-align: top or bottom) - the STRUT included (CSS 2.1
    // s10.8: the line box holds it like any other baseline-aligned box, and the top / bottom pass
    // below aligns against the line box it makes: a 24px `vertical-align: bottom` box in an 18px
    // strut line makes a 24px line, not 24 + the strut's descent). Each box counts where its
    // `vertical-align` PUTS it ([`baseline_shift`]): a `middle` icon taller than the strut
    // reaches below the baseline, and the line box must hold it there.
    let (mut max_asc, mut max_desc) =
        items
            .iter()
            .fold((strut_above, strut_below), |(max_asc, max_desc), item| {
                let effective_align =
                    get_item_vertical_align(item).unwrap_or(default_vertical_align);
                match effective_align {
                    VerticalAlign::Top | VerticalAlign::Bottom => {
                        // Skip top/bottom items in first pass
                        (max_asc, max_desc)
                    }
                    _ => {
                        let (item_asc, item_desc) = get_item_vertical_metrics(item, constraints);
                        let shift =
                            baseline_shift(effective_align, item_asc, item_desc, constraints)
                                .unwrap_or(0.0);
                        (
                            max_asc.max(item_asc - shift),
                            max_desc.max(item_desc + shift),
                        )
                    }
                }
            });

    let baseline_line_height = max_asc + max_desc;

    // Pass 2: Check top/bottom aligned items. If any of them is taller
    // than the current line box, expand the line box to fit.
    for item in items {
        let effective_align = get_item_vertical_align(item).unwrap_or(default_vertical_align);
        match effective_align {
            VerticalAlign::Top | VerticalAlign::Bottom => {
                let (item_asc, item_desc) = get_item_vertical_metrics(item, constraints);
                let item_height = item_asc + item_desc;
                if item_height > baseline_line_height {
                    // To minimize height, expand in the direction the item is aligned to
                    if effective_align == VerticalAlign::Top {
                        // Top-aligned item extends downward from line top
                        max_desc = max_desc.max(item_height - max_asc);
                    } else {
                        // Bottom-aligned item extends upward from line bottom
                        max_asc = max_asc.max(item_height - max_desc);
                    }
                }
            }
            _ => {} // Already handled in first pass
        }
    }

    (max_asc, max_desc)
}
