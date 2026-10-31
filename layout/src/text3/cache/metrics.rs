//! Font metrics: the fallback ratios and strut defaults, line heights and half-leading.

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

// --- Named constants for layout heuristics ---

/// Fraction of line-height used as ascent when no font metrics are available.
/// Matches the typical 80/20 ascent/descent ratio found in Latin fonts.
pub(super) const FALLBACK_ASCENT_RATIO: f32 = 0.8;
pub(super) const FALLBACK_DESCENT_RATIO: f32 = 1.0 - FALLBACK_ASCENT_RATIO;

// Strut/metric fallbacks below assume the CSS-initial 16px font size when no
// explicit size is set.

/// Default strut ascent: `FALLBACK_ASCENT_RATIO` * (16px * `DEFAULT_LINE_HEIGHT_FACTOR`)
pub(super) const DEFAULT_STRUT_ASCENT: f32 = 12.8;
/// Default strut descent: `FALLBACK_DESCENT_RATIO` * (16px * `DEFAULT_LINE_HEIGHT_FACTOR`)
pub(super) const DEFAULT_STRUT_DESCENT: f32 = 3.2;

/// Default x-height approximation: 0.5 * 16px (CSS spec fallback).
pub(super) const DEFAULT_X_HEIGHT: f32 = 8.0;
/// Cap height of the default strut (0.7 x the 16px default font size), the
/// same typical-Latin-ratio approximation the rest of the strut block uses.
pub(super) const DEFAULT_CAP_HEIGHT: f32 = 11.2;
/// Default ch-width (advance of '0'): 0.5 * 16px.
pub(super) const DEFAULT_CH_WIDTH: f32 = 8.0;

/// Approximate space character width as a fraction of `font_size`.
pub(super) const SPACE_WIDTH_RATIO: f32 = 0.5;

/// The CSS-initial font size (16px) as the default strut's: the parent font
/// size `vertical-align: sub` / `super` shift by when no container set one.
pub(super) const DEFAULT_STRUT_FONT_SIZE: f32 = 16.0;

/// Ruby annotation font size relative to the base, per the CSS UA stylesheet
/// (`rt { font-size: 50% }`). Used to reserve placeholder width for the
/// annotation so a long annotation is not clipped by a short base.
pub(super) const RUBY_ANNOTATION_FONT_SCALE: f32 = 0.5;

/// Computes the reserved box size for a ruby pair (CSS Ruby Layout §3): the inline-size is
/// the wider of the base and annotation runs (the narrower is centered over the wider), and
/// the block-size stacks the annotation line above the base line so the base reserves
/// vertical space for the annotation. Both inputs are REAL shaped advances / resolved line
/// heights — no magic per-character ratio.
pub(super) fn ruby_reserved_box(
    base_width: f32,
    annotation_width: f32,
    base_line_height: f32,
    annotation_line_height: f32,
) -> (f32, f32) {
    (
        base_width.max(annotation_width),
        base_line_height + annotation_line_height,
    )
}

/// CSS `line-height` value.
///
/// `Normal` defers resolution to the point where font metrics are available,
/// computing `(ascent + |descent| + lineGap) / upem * fontSize`.
/// `Px` is an already-resolved pixel value from an explicit CSS declaration
/// (e.g. `line-height: 1.5` → `Px(fontSize * 1.5)`).
#[derive(Debug, Clone, Copy, Default)]
pub enum LineHeight {
    /// `line-height: normal` — resolve from font metrics at layout time
    #[default]
    Normal,
    /// Pre-resolved pixel value (from CSS `line-height: <number|length|percentage>`)
    Px(f32),
}

impl LineHeight {
    /// Resolve to a pixel value, using font metrics when `Normal`.
    ///
    /// `ascent`, `descent` (negative in OpenType convention), `line_gap` are in font units.
    /// `font_size_px` and `units_per_em` are used to scale. `normal` is the
    /// browsers' rounded `A + D + G` ([`LayoutFontMetrics::line_metrics_px`],
    /// for a face without the macOS ascent boost), 1.2em without units.
    #[must_use]
    pub fn resolve(
        &self,
        font_size_px: f32,
        ascent: f32,
        descent: f32,
        line_gap: f32,
        units_per_em: u16,
    ) -> f32 {
        self.resolve_with_metrics(
            font_size_px,
            &LayoutFontMetrics {
                ascent,
                descent,
                line_gap,
                units_per_em,
                x_height: None,
                cap_height: None,
                browser_ascent_boost: false,
            },
        )
    }

    /// Resolve against a face's metrics: `normal` is the line height a
    /// browser gives a line of that face, `A + D + G` each rounded to whole
    /// pixels at the font size ([`LayoutFontMetrics::line_metrics_px`]),
    /// 1.2em for a face without units.
    #[must_use]
    pub fn resolve_with_metrics(&self, font_size_px: f32, metrics: &LayoutFontMetrics) -> f32 {
        match self {
            Self::Px(px) => *px,
            Self::Normal => metrics
                .line_metrics_px(font_size_px)
                .map_or(font_size_px * 1.2, |(a, d, g)| a + d + g),
        }
    }
}

impl PartialEq for LineHeight {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Normal, Self::Normal) => true,
            (Self::Px(a), Self::Px(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Eq for LineHeight {}

impl Hash for LineHeight {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        if let Self::Px(v) = self {
            v.to_bits().hash(state);
        }
    }
}

/// `(above, below)` the baseline of a box of `ascent` and `descent` (px) in a
/// line `line_height` px tall: CSS 2.2 §10.8.1's leading `L = line-height -
/// (A + D)` shared between the two sides, the share ABOVE floored to a whole
/// pixel and the rest below, as `LayoutNG` does (`InlineBoxState::
/// CalculateLeadingSpace`). The two always add up to `line_height`; with
/// whole-pixel metrics a glyph's box and the strut of the same face split
/// alike, so one line's boxes coincide.
#[must_use]
pub fn split_leading(line_height: f32, ascent: f32, descent: f32) -> (f32, f32) {
    let leading = line_height - (ascent + descent);
    let above = (leading / 2.0).floor();
    (ascent + above, descent + (leading - above))
}

#[derive(Copy, Debug, Clone)]
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

// +spec:font-metrics:df51b1 - font metrics (ascent, descent, line_gap) used as baselines for inline
// layout alignment and box sizing
/// Layout-specific font metrics extracted from `FontMetrics`
/// Contains only the metrics needed for text layout and rendering
// +spec:box-model:a2f1c1 - inline box content area sized from first available font metrics
// (ascent/descent) +spec:font-metrics:9c2ca5 - ascent and descent metrics per font for inline
// layout +spec:font-metrics:797593 - font metrics (ascent, descent, line-gap) used for baseline
// calculations +spec:font-metrics:842d6a - font metrics (ascent, descent) used for precise spacing
// control +spec:font-metrics:eb97e0 - Font baseline metrics (ascent/descent) from font tables used
// for baseline alignment +spec:font-metrics:f2cd75 - em-over/em-under baselines intentionally not
// included (not used by CSS per spec) +spec:inline-formatting-context:76cd57 - ascent/descent font
// metrics for inline formatting context layout +spec:font-metrics:207e6b - ascent/descent metrics
// used for baseline calculations
#[derive(Copy, Debug, Clone, PartialEq)]
pub struct LayoutFontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
    /// OS/2 sxHeight: distance from baseline to top of lowercase 'x' (in font units).
    /// Used for `vertical-align: middle` per CSS Inline 3 §4.1.
    pub x_height: Option<f32>,
    /// OS/2 sCapHeight: height of capital letters from baseline (in font units).
    /// Used for drop cap / initial-letter alignment per CSS Inline 3 §7.1.1.
    pub cap_height: Option<f32>,
    /// The face is Apple's Times, Helvetica or Courier on macOS: browsers
    /// there grow its rounded ascent by 15% of ascent + descent
    /// ([`Self::line_metrics_px`]). Set where the face is parsed
    /// (`crate::font::parsed::browser_ascent_boost`).
    pub browser_ascent_boost: bool,
}

impl LayoutFontMetrics {
    /// The `(ascent, descent, line gap)` a browser lays a line of this face
    /// out with at `font_size_px`, in px, `None` for a face without units.
    ///
    /// Each is rounded to a whole pixel AT THE FONT SIZE (Blink
    /// `FontMetrics::AscentDescentWithHacks`: `SkScalarRoundToScalar` of the
    /// ascent and descent; `SimpleFontData::PlatformInit`: `lroundf` of the
    /// line gap), so `line-height: normal` is `A + D + G` in whole pixels: a
    /// 16px Arial line is 14 + 3 + 1 = 18px, as in Chrome (the unrounded sum
    /// is 18.4px, and every line of a mail drifted 0.4px against Chrome).
    /// Then, for [`Self::browser_ascent_boost`] faces only, the rounded
    /// ascent grows by `floor((A + D) * 0.15 + 0.5)` (Blink, macOS; `WebKit`
    /// `SimpleFontData::platformInit`): 16px Helvetica is 14 + 4 + 0 = 18px.
    /// The descent is the hhea descender's distance below the baseline
    /// (stored negative), the line gap is floored at zero (CSS Inline 3
    /// §3.2.2). Measured against Chrome 154 for 11 faces x 13 sizes
    /// (`layout/tests/a_normal_line_is_as_tall_as_chromes.rs`).
    #[must_use]
    pub fn line_metrics_px(&self, font_size_px: f32) -> Option<(f32, f32, f32)> {
        if self.units_per_em == 0 {
            return None;
        }
        let scale = font_size_px / f32::from(self.units_per_em);
        let round = |v: f32| (v + 0.5).floor();
        let mut ascent = round(self.ascent * scale);
        let descent = round((-self.descent * scale).max(0.0));
        let line_gap = round((self.line_gap * scale).max(0.0));
        if self.browser_ascent_boost {
            ascent += round((ascent + descent) * 0.15);
        }
        Some((ascent, descent, line_gap))
    }

    /// `(above, below)` the baseline of a glyph of this face in a line of
    /// `line_height`: the rounded ascent and descent
    /// ([`Self::line_metrics_px`]) with the leading shared out by
    /// [`split_leading`]. `None` for a face without units.
    #[must_use]
    pub fn inline_box_px(&self, font_size_px: f32, line_height: &LineHeight) -> Option<(f32, f32)> {
        let (ascent, descent, _) = self.line_metrics_px(font_size_px)?;
        let line_height = line_height.resolve_with_metrics(font_size_px, self);
        Some(split_leading(line_height, ascent, descent))
    }

    // +spec:font-metrics:006bd8 - baseline position from font design coordinates, scaled with font
    // size +spec:font-metrics:910c0a - dominant-baseline: auto resolves to alphabetic for
    // horizontal text +spec:writing-modes:098958 - baseline is along the inline axis, used to
    // align glyphs
    #[must_use]
    pub fn baseline_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / f32::from(self.units_per_em);
        self.ascent * scale
    }

    /// Returns the x-height scaled to the given font size in px.
    /// Falls back to 0.5em when the font doesn't provide sxHeight.
    #[must_use]
    pub fn x_height_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / f32::from(self.units_per_em);
        self.x_height.map_or(font_size * 0.5, |xh| xh * scale)
    }

    /// Returns the cap height scaled to the given font size in px.
    /// Falls back to ascent when the font doesn't provide sCapHeight.
    #[must_use]
    pub fn cap_height_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / f32::from(self.units_per_em);
        self.cap_height.unwrap_or(self.ascent) * scale
    }

    // +spec:line-height:471816 - line gap metric extracted from font for optional use when
    // line-height is normal
    /// Convert from full `FontMetrics` to layout-specific metrics.
    // +spec:font-metrics:05193a - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:font-metrics:17a71c - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:font-metrics:62c659 - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:writing-modes:451a3e - ascent/descent/line-gap metrics: prefer OS/2, fallback HHEA,
    // floor line_gap at 0
    /// Per CSS 2.2 §10.8.1: prefer OS/2 sTypoAscender/sTypoDescender,
    /// fall back to HHEA Ascent/Descent if OS/2 metrics are absent.
    // +spec:font-metrics:3dc8c1 - text-over/text-under baselines from font ascent/descent metrics
    // +spec:font-metrics:332c16 - text-over/text-under baseline metrics derived from font
    // ascent/descent +spec:font-metrics:9895e2 - baseline table is a font-level property;
    // metrics apply uniformly to all glyphs +spec:font-metrics:e05c40 - font ascent/descent
    // metric extraction (text edge metrics) +spec:font-metrics:21a3de - ascent/descent used as
    // basis for em-over/em-under normalization +spec:font-metrics:1257b7 - font ascent/descent
    // ensure text fits within line box +spec:table-layout:6bbd10 - use
    // sTypoAscender/sTypoDescender as ascent/descent metrics per spec recommendation
    // +spec:font-metrics:5346d2 - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:font-metrics:e16941 - line gap metric floored at zero per spec
    // +spec:font-metrics:a55c05 - metrics taken from font, synthesized if missing (prefers OS/2,
    // falls back to HHEA)
    #[must_use]
    pub fn from_font_metrics(metrics: &azul_css::props::basic::FontMetrics) -> Self {
        let ascent = metrics
            .s_typo_ascender
            .as_option()
            .map_or_else(|| f32::from(metrics.ascender), |v| f32::from(*v));
        let descent = metrics
            .s_typo_descender
            .as_option()
            .map_or_else(|| f32::from(metrics.descender), |v| f32::from(*v));
        // UAs must floor the line gap metric at zero (css-inline-3 §3.2.2)
        // Spec: "UAs must floor the line gap metric at zero."
        let line_gap = metrics
            .s_typo_line_gap
            .as_option()
            .map_or_else(|| f32::from(metrics.line_gap), |v| f32::from(*v))
            .max(0.0);
        let x_height = metrics.sx_height.as_option().map(|v| f32::from(*v));
        let cap_height = metrics.s_cap_height.as_option().map(|v| f32::from(*v));
        Self {
            ascent,
            descent,
            line_gap,
            units_per_em: metrics.units_per_em,
            x_height,
            cap_height,
            // No family name reaches this constructor (an embedder's face
            // metrics): only `ParsedFont` sets the macOS ascent boost.
            browser_ascent_boost: false,
        }
    }

    // +spec:font-metrics:1eda6b - em-over is 0.5em over central baseline, em-under is 0.5em under
    /// Synthesize em-over baseline offset (in font units).
    /// Per CSS Inline 3 Appendix A.1: em-over = central baseline + 0.5em.
    /// Central baseline is synthesized as midpoint of ascent and descent.
    #[must_use]
    pub fn em_over(&self) -> f32 {
        let central = self.central_baseline();
        central + (f32::from(self.units_per_em) / 2.0)
    }

    /// Synthesize em-under baseline offset (in font units).
    /// Per CSS Inline 3 Appendix A.1: em-under = central baseline - 0.5em.
    #[must_use]
    pub fn em_under(&self) -> f32 {
        let central = self.central_baseline();
        central - (f32::from(self.units_per_em) / 2.0)
    }

    /// Synthesize central baseline (in font units).
    /// Midpoint between ascent and descent when not provided by the font.
    #[must_use]
    pub const fn central_baseline(&self) -> f32 {
        f32::midpoint(self.ascent, self.descent)
    }
}
