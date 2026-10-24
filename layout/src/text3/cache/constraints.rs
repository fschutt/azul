//! What a text layout is asked for: the constraints, available space, wrapping and breaking rules, alignment and writing modes.

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

/// Available space for layout, similar to Taffy's `AvailableSpace`.
///
/// This type explicitly represents the three possible states for available space:
///
/// - `Definite(f32)`: A specific pixel width is available
/// - `MinContent`: Layout should use minimum content width (shrink-wrap)
/// - `MaxContent`: Layout should use maximum content width (no line breaks unless necessary)
///
/// This is critical for proper handling of intrinsic sizing in Flexbox/Grid
/// where the available space may be indefinite during the measure phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSpace {
    /// A specific amount of space is available (in pixels).
    /// Must be >= 0.  A value of 0.0 means "genuinely zero-width container"
    /// (e.g. `width: 0px`), NOT "unresolved".
    Definite(f32),
    /// The node should be laid out under a min-content constraint
    MinContent,
    /// The node should be laid out under a max-content constraint.
    /// This is the correct default: "lay out to natural width, no constraint".
    MaxContent,
}

impl Default for AvailableSpace {
    /// Default is `MaxContent` — the absence of a width constraint.
    /// Never `Definite(0.0)`, which would make every word overflow.
    fn default() -> Self {
        Self::MaxContent
    }
}

impl AvailableSpace {
    /// Returns true if this is a definite (finite, known) amount of space
    #[must_use]
    pub const fn is_definite(&self) -> bool {
        matches!(self, Self::Definite(_))
    }

    /// Returns true if this is an indefinite (min-content or max-content) constraint
    #[must_use]
    pub const fn is_indefinite(&self) -> bool {
        !self.is_definite()
    }

    /// The definite length, or `None` when this is a measurement constraint.
    ///
    /// The accessor sizing code should reach for: it forces the caller to
    /// decide what an indefinite axis means at that call site instead of
    /// inheriting an `f32::INFINITY` it will do arithmetic on.
    #[must_use]
    pub const fn definite(self) -> Option<f32> {
        match self {
            Self::Definite(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the definite value if available, or a fallback for indefinite constraints
    #[must_use]
    pub const fn unwrap_or(self, fallback: f32) -> f32 {
        match self {
            Self::Definite(v) => v,
            _ => fallback,
        }
    }

    /// Returns the definite value, or a large value for both min-content and max-content.
    ///
    /// For intrinsic sizing, we use a large value to let text lay out fully,
    /// then measure the result. The distinction between min/max-content is handled
    /// by the line breaking algorithm, not by constraining the available width.
    #[allow(clippy::match_same_arms)]
    // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that
    // can't merge)
    #[must_use]
    pub fn to_f32_for_layout(self) -> f32 {
        match self {
            Self::Definite(v) => v,
            Self::MinContent => f32::MAX / 2.0,
            Self::MaxContent => f32::MAX / 2.0,
        }
    }

    /// Create from an f32 value, recognizing special sentinel values.
    ///
    /// This function provides backwards compatibility with code that uses f32 for constraints:
    /// - `f32::INFINITY` or `f32::MAX` → `MaxContent` (no line wrapping)
    /// - `0.0` → `MinContent` (maximum line wrapping, return longest word width)
    /// - Other values → `Definite(value)`
    ///
    /// Note: Using sentinel values like 0.0 for `MinContent` is fragile. Prefer using
    /// `AvailableSpace::MinContent` directly when possible.
    #[must_use]
    pub fn from_f32(value: f32) -> Self {
        if value.is_infinite() || value >= f32::MAX / 2.0 {
            // Treat very large values (including f32::MAX) as MaxContent
            Self::MaxContent
        } else if value <= 0.0 {
            // Treat zero or negative as MinContent (shrink-wrap)
            Self::MinContent
        } else {
            Self::Definite(value)
        }
    }
}

impl Hash for AvailableSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        if let Self::Definite(v) = self {
            // Hash the full f32 bit pattern, NOT the integer-rounded value. The
            // derived `PartialEq` compares `Definite` widths exactly, so rounding
            // here both (a) broke sub-pixel precision — a 100.1px vs 100.4px
            // constraint can wrap lines differently yet collided in the same hash
            // bucket — and (b) was inconsistent with the exact equality used as the
            // cache key. `-0.0` is normalized to `+0.0` so the `+0.0 == -0.0`
            // PartialEq pair still hashes identically (Hash/Eq contract).
            let normalized = if *v == 0.0 { 0.0f32 } else { *v };
            normalized.to_bits().hash(state);
        }
    }
}

// Error handling
// [g119 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)): the String/FontSelector payloads give
// `Result<T, LayoutError>` (e.g. measure_intrinsic_widths' return + reorder/shape/orientation `?`)
// a POINTER-niche disc the web lift mis-reads → Ok→Err. Explicit u8 tag = simple-compare niche the
// lift handles. Also nested in solver3::LayoutError::Text (so both must be repr(C,u8)). Not
// FFI-exposed.
#[derive(Debug, thiserror::Error)]
#[repr(C, u8)]
pub enum LayoutError {
    #[error("Bidi analysis failed: {0}")]
    BidiError(String),
    #[error("Shaping failed: {0}")]
    ShapingError(String),
    #[error("Font not found: {0:?}")]
    FontNotFound(FontSelector),
    #[error("Invalid text input: {0}")]
    InvalidText(String),
    #[error("Hyphenation failed: {0}")]
    HyphenationError(String),
}

/// Text boundary types for cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoundary {
    /// Reached top of text (first line)
    Top,
    /// Reached bottom of text (last line)
    Bottom,
    /// Reached start of text (first character)
    Start,
    /// Reached end of text (last character)
    End,
}

/// Error returned when cursor movement hits a boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorBoundsError {
    pub(crate) boundary: TextBoundary,
    pub(crate) cursor: TextCursor,
}

/// Unified constraints combining all layout features
///
/// # CSS Inline Layout Module Level 3: Constraint Mapping
///
/// This structure maps CSS properties to layout constraints:
///
/// ## \u00a7 2.1 Layout of Line Boxes
/// - `available_width`: \u26a0\ufe0f CRITICAL - Should equal containing block's inner width
///   * Currently defaults to 0.0 which causes immediate line breaking
///   * Per spec: "logical width of a line box is equal to the inner logical width of its containing
///     block"
/// - `available_height`: For block-axis constraints (max-height)
///
/// ## \u00a7 2.2 Layout Within Line Boxes
/// - `text_align`: \u2705 Horizontal alignment (start, end, center, justify)
/// - `vertical_align`: \u26a0\ufe0f PARTIAL - Only baseline supported, missing:
///   * top, bottom, middle, text-top, text-bottom
///   * <length>, <percentage> values
///   * sub, super positions
/// - `line_height`: \u2705 Distance between baselines
///
/// ## \u00a7 3 Baselines and Alignment Metrics
/// - `text_orientation`: \u2705 For vertical writing (sideways, upright)
/// - `writing_mode`: \u2705 horizontal-tb, vertical-rl, vertical-lr
/// - `direction`: \u2705 ltr, rtl for `BiDi`
///
/// ## \u00a7 4 Baseline Alignment (vertical-align property)
/// \u26a0\ufe0f INCOMPLETE: Only basic baseline alignment implemented
///
/// ## \u00a7 5 Line Spacing (line-height property)
/// - `line_height`: \u2705 Implemented
/// - \u274c MISSING: line-fit-edge for controlling which edges contribute to line height
///   +spec:box-model:51342f - inline box margins/borders/padding do not affect line box height
///   (default leading mode) +spec:font-metrics:618776 - line-fit-edge (cap, ex, ideographic,
///   alphabetic edge selection) not yet implemented
///
/// ## \u00a7 6 Trimming Leading (text-box-trim)
/// - \u274c NOT IMPLEMENTED: text-box-trim property
/// - \u274c NOT IMPLEMENTED: text-box-edge property +spec:box-model:c09331 - text-box-trim trims
///   block container first/last line to font metrics // +spec:overflow:dc2196 - text-box-trim
///   overflow handled as normal overflow (no special handling needed)
///
/// ## CSS Text Module Level 3
/// - `text_indent`: \u2705 First line indentation
/// - `text_justify`: \u2705 Justification algorithm (auto, inter-word, inter-character)
/// - `hyphenation`: \u2705 Hyphens property (none / manual / auto)
/// - `hanging_punctuation`: \u2705 Hanging punctuation at line edges
///
/// ## CSS Text Level 4
/// - `text_wrap`: \u2705 balance, pretty, stable
/// - `line_clamp`: \u2705 Max number of lines
///
/// ## CSS Writing Modes Level 4
/// - `text_combine_upright`: \u2705 Tate-chu-yoko for vertical text
///
/// ## CSS Shapes Module
/// - `shape_boundaries`: \u2705 Custom line box shapes
/// - `shape_exclusions`: \u2705 Exclusion areas (float-like behavior)
/// - `exclusion_margin`: \u2705 Margin around exclusions
///
/// ## Multi-column Layout
/// - `columns`: \u2705 Number of columns
/// - `column_gap`: \u2705 Gap between columns
///
/// # Known Issues:
/// 1. [ISSUE] `available_width` defaults to Definite(0.0) instead of containing block width
/// 2. [ISSUE] `vertical_align` only supports baseline
/// 3. [TODO] initial-letter (drop caps) not implemented
// +spec:box-model:415ef3 - initial letters use standard margin/padding/border box model; exclusion
// area = margin box +spec:box-model:d53ea3 - when block-start padding+border are zero, content edge
// coincides with over alignment point
///    +spec:positioning:fb233a - initial letter block-axis: if size < sink, use over alignment
#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeBoundary>,

    // Basic layout - using AvailableSpace for proper indefinite handling
    pub available_width: AvailableSpace,
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: Option<WritingMode>,
    // +spec:writing-modes:6c5ab9 - blocks inherit base direction from parent via CSS direction
    // property Base direction from CSS, overrides auto-detection
    pub direction: Option<BidiDirection>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    // +spec:display-property:3bcac8 - inline boxes sized in block axis based on font metrics
    // (ascent/descent)
    pub line_height: LineHeight,
    pub vertical_align: VerticalAlign,
    // block container's first available font, used for minimum line box height
    pub strut_ascent: f32,
    pub strut_descent: f32,
    // x-height of the strut font (scaled to font_size), for vertical-align: middle
    pub strut_x_height: f32,
    // cap-height of the strut font (scaled to font_size), for
    // text-box-edge: cap trimming (CSS Inline 3 §6.1).
    pub strut_cap_height: f32,
    // The block container's (the strut's) computed font size in px: the
    // parent font size `vertical-align: sub` / `super` shift a box by
    // (`baseline_shift`; Chrome: / 5 + 1 down, / 3 + 1 up).
    pub strut_font_size: f32,

    // Width of '0' (zero) character in px, used for ch unit and tab-size.
    // Approximated as space_width from the first available font, or 0.5 * font_size fallback.
    pub ch_width: f32,

    // Overflow handling
    pub overflow: OverflowBehavior,
    pub segment_alignment: SegmentAlignment,

    // Advanced features
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: Hyphens,
    pub hyphenation_language: Option<Language>,
    pub text_indent: f32,
    pub text_indent_each_line: bool,
    pub text_indent_hanging: bool,
    pub initial_letter: Option<InitialLetter>,
    pub line_clamp: Option<NonZeroUsize>,

    // text-wrap: balance
    pub text_wrap: TextWrap,
    pub columns: u32,
    pub column_gap: f32,
    pub hanging_punctuation: bool,
    pub overflow_wrap: OverflowWrap,
    pub text_align_last: TextAlign,
    // §5.2 word-break property on constraints
    pub word_break: WordBreak,
    pub white_space_mode: WhiteSpaceMode,
    pub line_break: LineBreakStrictness,
    // CSS unicode-bidi property; Plaintext causes per-paragraph auto-detection
    pub unicode_bidi: UnicodeBidi,
    /// This inline formatting context's share of a multi-column BLOCK
    /// container's flow (`solver3::multicol`); `None` everywhere else.
    /// Honoured only when `columns == 1` (a context with columns of its own
    /// splits its lines itself).
    pub column_flow: Option<ColumnFlow>,
}

/// One inline formatting context's share of a multi-column container's
/// flow (CSS Multi-column Layout 1): its lines fill the rest of the column
/// it starts in, and from each line index in `breaks` on they continue at
/// the top of the next column.
///
/// The multi-column block layout (`solver3::multicol::plan_columns`)
/// picks the breaks on the context's unsplit layout. Laying the context out
/// again with them only MOVES lines - every column is as wide as the
/// unsplit layout was, so the lines break exactly as before.
#[derive(Debug, Clone, Default)]
pub struct ColumnFlow {
    /// The flow-wide index of the first line of every further column,
    /// ascending: `[2, 7]` keeps lines 0-1 in the first column, 2-6 in
    /// the second, the rest in the third.
    pub breaks: Vec<usize>,
    /// The inline distance from one column to the next: the column width
    /// plus the gap, negative when the columns run right to left.
    pub advance: f32,
    /// Where the first line box of every further column starts on the block
    /// axis, relative to this context's content-box top - minus how far
    /// below the top of its first column the context starts.
    pub column_top: f32,
}

impl PartialEq for ColumnFlow {
    fn eq(&self, other: &Self) -> bool {
        self.breaks == other.breaks
            && round_eq(self.advance, other.advance)
            && round_eq(self.column_top, other.column_top)
    }
}

impl Eq for ColumnFlow {}

impl Hash for ColumnFlow {
    #[allow(clippy::cast_possible_truncation)] // the rounded-pixel key `round_eq` compares
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.breaks.hash(state);
        (self.advance.round() as isize).hash(state);
        (self.column_top.round() as isize).hash(state);
    }
}

impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            shape_boundaries: Vec::new(),
            shape_exclusions: Vec::new(),

            // Use MaxContent as default to avoid premature line breaking.
            // MaxContent means "use intrinsic width" which is appropriate when
            // the containing block's width is not yet known.
            // Previously this was Definite(0.0) which caused each character to
            // wrap to its own line. The actual width should be passed from the
            // box layout solver (fc.rs) when creating UnifiedConstraints.
            available_width: AvailableSpace::MaxContent,
            available_height: None,
            writing_mode: None,
            direction: None, // Will default to LTR if not specified
            text_orientation: TextOrientation::default(),
            text_align: TextAlign::default(),
            text_justify: JustifyContent::default(),
            line_height: LineHeight::Normal,
            vertical_align: VerticalAlign::default(),
            strut_ascent: DEFAULT_STRUT_ASCENT,
            strut_descent: DEFAULT_STRUT_DESCENT,
            strut_x_height: DEFAULT_X_HEIGHT,
            strut_cap_height: DEFAULT_CAP_HEIGHT,
            strut_font_size: DEFAULT_STRUT_FONT_SIZE,
            ch_width: DEFAULT_CH_WIDTH,
            overflow: OverflowBehavior::default(),
            segment_alignment: SegmentAlignment::default(),
            text_combine_upright: None,
            exclusion_margin: 0.0,
            hyphenation: Hyphens::default(),
            hyphenation_language: None,
            columns: 1,
            column_gap: 0.0,
            column_flow: None,
            hanging_punctuation: false,
            text_indent: 0.0,
            text_indent_each_line: false,
            text_indent_hanging: false,
            initial_letter: None,
            line_clamp: None,
            text_wrap: TextWrap::default(),
            overflow_wrap: OverflowWrap::default(),
            text_align_last: TextAlign::default(),
            word_break: WordBreak::default(),
            white_space_mode: WhiteSpaceMode::default(),
            line_break: LineBreakStrictness::default(),
            unicode_bidi: UnicodeBidi::default(),
        }
    }
}

// UnifiedConstraints
impl Hash for UnifiedConstraints {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_boundaries.hash(state);
        self.shape_exclusions.hash(state);
        self.available_width.hash(state);
        self.available_height
            .map(|h| h.round() as isize)
            .hash(state);
        self.writing_mode.hash(state);
        self.direction.hash(state);
        self.text_orientation.hash(state);
        self.text_align.hash(state);
        self.text_justify.hash(state);
        self.line_height.hash(state);
        self.vertical_align.hash(state);
        (self.strut_ascent.round() as isize).hash(state);
        (self.strut_descent.round() as isize).hash(state);
        (self.strut_x_height.round() as isize).hash(state);
        (self.strut_font_size.round() as isize).hash(state);
        (self.ch_width.round() as isize).hash(state);
        self.overflow.hash(state);
        self.segment_alignment.hash(state);
        self.text_combine_upright.hash(state);
        (self.exclusion_margin.round() as isize).hash(state);
        self.hyphenation.hash(state);
        self.hyphenation_language.hash(state);
        (self.text_indent.round() as isize).hash(state);
        self.text_indent_each_line.hash(state);
        self.text_indent_hanging.hash(state);
        self.initial_letter.hash(state);
        self.line_clamp.hash(state);
        self.columns.hash(state);
        (self.column_gap.round() as isize).hash(state);
        self.column_flow.hash(state);
        self.hanging_punctuation.hash(state);
        self.overflow_wrap.hash(state);
        self.text_align_last.hash(state);
        self.word_break.hash(state);
        self.white_space_mode.hash(state);
        self.line_break.hash(state);
        self.unicode_bidi.hash(state);
    }
}

impl PartialEq for UnifiedConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.shape_boundaries == other.shape_boundaries
            && self.shape_exclusions == other.shape_exclusions
            && self.available_width == other.available_width
            && match (self.available_height, other.available_height) {
                (None, None) => true,
                (Some(h1), Some(h2)) => round_eq(h1, h2),
                _ => false,
            }
            && self.writing_mode == other.writing_mode
            && self.direction == other.direction
            && self.text_orientation == other.text_orientation
            && self.text_align == other.text_align
            && self.text_justify == other.text_justify
            && self.line_height == other.line_height
            && self.vertical_align == other.vertical_align
            && round_eq(self.strut_ascent, other.strut_ascent)
            && round_eq(self.strut_descent, other.strut_descent)
            && round_eq(self.strut_x_height, other.strut_x_height)
            && round_eq(self.strut_font_size, other.strut_font_size)
            && round_eq(self.ch_width, other.ch_width)
            && self.overflow == other.overflow
            && self.segment_alignment == other.segment_alignment
            && self.text_combine_upright == other.text_combine_upright
            && round_eq(self.exclusion_margin, other.exclusion_margin)
            && self.hyphenation == other.hyphenation
            && self.hyphenation_language == other.hyphenation_language
            && round_eq(self.text_indent, other.text_indent)
            && self.text_indent_each_line == other.text_indent_each_line
            && self.text_indent_hanging == other.text_indent_hanging
            && self.initial_letter == other.initial_letter
            && self.line_clamp == other.line_clamp
            && self.columns == other.columns
            && round_eq(self.column_gap, other.column_gap)
            && self.column_flow == other.column_flow
            && self.hanging_punctuation == other.hanging_punctuation
            && self.overflow_wrap == other.overflow_wrap
            && self.text_align_last == other.text_align_last
            && self.word_break == other.word_break
            && self.white_space_mode == other.white_space_mode
            && self.line_break == other.line_break
            && self.unicode_bidi == other.unicode_bidi
    }
}

impl Eq for UnifiedConstraints {}

impl UnifiedConstraints {
    /// Resolve `line_height` to a pixel value using the strut metrics as a font-size proxy.
    /// `strut_ascent + strut_descent` approximates `font_size` (the block container's font).
    #[must_use]
    pub fn resolved_line_height(&self) -> f32 {
        match self.line_height {
            // `line-height: normal` — the minimum line-box height is the block's
            // first-available-font metrics, approximated here by the strut's
            // ascent + descent. Resolving `Normal` with no real metrics fell back
            // to `font_size * 1.2`, which inflated every non-last line's advance
            // ~20% (block auto-heights came out too tall). The real per-line box
            // height (from each run's actual glyph metrics) is folded in via
            // `.max()` at the call sites, so this strut value is the correct floor.
            LineHeight::Normal => self.strut_ascent + self.strut_descent,
            LineHeight::Px(px) => px,
        }
    }
    pub(super) fn direction(&self, fallback: BidiDirection) -> BidiDirection {
        self.writing_mode
            .map_or(fallback, |s| s.get_direction().unwrap_or(fallback))
    }
    pub(super) const fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            Some(WritingMode::VerticalRl | WritingMode::VerticalLr)
        )
    }
}

/// Line constraints with multi-segment support
#[derive(Debug, Clone)]
pub struct LineConstraints {
    pub segments: Vec<LineSegment>,
    pub total_available: f32,
    /// True when measuring min-content: the breaker must break at EVERY soft-wrap
    /// opportunity (each word on its own line) rather than filling `total_available`
    /// (which is a sentinel `f32::MAX / 2` for intrinsic sizing and never overflows).
    pub is_min_content: bool,
}

impl WritingMode {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call
    // sites for a perf-neutral change)
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input
                                      // variant (or cross-type bindings that can't merge)
    pub(super) const fn get_direction(&self) -> Option<BidiDirection> {
        match self {
            // determined by text content
            Self::HorizontalTb => None,
            Self::VerticalRl => Some(BidiDirection::Rtl),
            Self::VerticalLr => Some(BidiDirection::Ltr),
            Self::SidewaysRl => Some(BidiDirection::Rtl),
            Self::SidewaysLr => Some(BidiDirection::Ltr),
        }
    }
}

/// Defines how text should be aligned when a line contains multiple disjoint segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SegmentAlignment {
    /// Align text within the first available segment on the line.
    #[default]
    First,
    /// Align text relative to the total available width of all
    /// segments on the line combined.
    Total,
}

#[derive(Copy, Debug, Clone)]
pub struct LineSegment {
    pub start_x: f32,
    pub width: f32,
    // For choosing best segment when multiple available
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextWrap {
    #[default]
    Wrap,
    Balance,
    NoWrap,
}

/// CSS `overflow-wrap` (aka `word-wrap`) property.
///
/// Controls whether an otherwise unbreakable sequence of characters
/// may be broken at an arbitrary point to prevent overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OverflowWrap {
    /// No special break opportunities are introduced.
    #[default]
    Normal,
    /// Break at arbitrary points if no other break points exist.
    /// Soft wrap opportunities from `anywhere` ARE considered
    /// when calculating min-content intrinsic sizes.
    Anywhere,
    /// Same as `anywhere` except soft wrap opportunities introduced
    /// by `break-word` are NOT considered when calculating
    /// min-content intrinsic sizes.
    BreakWord,
}

// +spec:line-breaking:841a87 - hyphens property: manual (U+00AD/U+2010 only) and auto
// (language-aware automatic hyphenation) +spec:line-breaking:68c6ad - hyphens property controls
// hyphenation opportunities (none/manual/auto)
/// Controls whether hyphenation is allowed to create soft wrap opportunities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Hyphens {
    /// No hyphenation: U+00AD soft hyphens are not treated as break points.
    None,
    /// Only break at manually-inserted soft hyphens (U+00AD) or explicit hyphens.
    #[default]
    Manual,
    /// The UA may automatically hyphenate words in addition to manual opportunities.
    Auto,
}

// +spec:line-breaking:ce5258 - white-space property controls collapsing, wrapping, and forced
// breaks +spec:line-breaking:35817b - normal/pre/nowrap/pre-wrap/break-spaces/pre-line behaviors
// +spec:white-space-processing:dec7aa - White space not removed/collapsed is "preserved white
// space"
#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum WhiteSpaceMode {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

// CSS Text Level 3 §5.3: The line-break property controls strictness of line breaking rules.
// - Auto: UA-dependent, typically normal for CJK, loose for non-CJK
// - Loose: least restrictive, allows breaks before small kana, CJK hyphens, etc.
// - Normal: default CJK rules, allows breaks before CJK hyphen-like chars for CJK text
// - Strict: most restrictive, forbids breaks before small kana and CJK punctuation
// - Anywhere: allows soft wrap opportunities around every typographic character unit
#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum LineBreakStrictness {
    #[default]
    Auto,
    Loose,
    Normal,
    Strict,
    /// Soft wrap opportunity around every typographic character unit.
    /// Hyphenation is not applied.
    Anywhere,
}

// §5.2 word-break property: normal, break-all, keep-all
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum WordBreak {
    /// Normal break rules: CJK characters break between each other,
    /// non-CJK text only breaks at spaces/hyphens.
    #[default]
    Normal,
    /// Allow breaks between any two characters, including within Latin words.
    BreakAll,
    /// Suppress breaks between CJK characters (treat them like Latin words,
    /// only breaking at spaces). Sequences of CJK characters do not break.
    KeepAll,
}

// +spec:display-property:162c99 - Initial letter box: in-flow inline-level box with special layout
// behavior +spec:display-property:72a797 - Initial letter handled like inline-level content in
// originating line box initial-letter
// +spec:containing-block:46a499 - subsequent block must clear previous block's initial letter if it
// starts with its own initial letter, establishes independent FC, or specifies clear in initial
// letter's CB start direction +spec:font-metrics:1e5325 - drop initial cap-height =
// (N-1)*line_height + surrounding cap-height +spec:font-metrics:3aa518 - initial-letter-align:
// cap-height/ideographic/hanging/leading/border-box baseline alignment +spec:writing-modes:9698b0 -
// Han-derived scripts: initial letter extends from block-start to block-end of Nth line
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct InitialLetter {
    /// How many lines tall the initial letter should be.
    pub size: f32,
    // +spec:font-metrics:dc0632 - raised initial "sinks" to first text baseline (sink=1)
    /// How many lines the letter should sink into.
    pub sink: u32,
    /// How many characters to apply this styling to.
    pub count: NonZeroUsize,
    // +spec:display-property:4c69bf - alignment points for sizing/positioning initial letter
    /// Alignment mode for the initial letter (over/under alignment points
    /// matched to corresponding points of the root inline box).
    pub align: InitialLetterAlign,
}

/// Alignment mode for initial letters, controlling which alignment points
/// are used to size and position the letter relative to the root inline box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InitialLetterAlign {
    /// UA chooses based on script
    Auto,
    /// Alphabetic baseline alignment
    Alphabetic,
    /// Hanging baseline alignment
    Hanging,
    /// Ideographic baseline alignment
    Ideographic,
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// This is a marker trait, indicating that `a == b` is a true equivalence
// relation. The derived `PartialEq` already satisfies this.
impl Eq for InitialLetter {}

impl Hash for InitialLetter {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Per the request, round the f32 to a usize for hashing.
        // This is a lossy conversion; values like 2.3 and 2.4 will produce
        // the same hash value for this field. This is acceptable as long as
        // the `PartialEq` implementation correctly distinguishes them.
        (self.size.round() as isize).hash(state);
        self.sink.hash(state);
        self.count.hash(state);
        self.align.hash(state);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverflowBehavior {
    // Content extends outside shape
    Visible,
    // Content is clipped to shape
    Hidden,
    // Scrollable overflow
    Scroll,
    // Browser/system decides
    #[default]
    Auto,
    // Break into next shape/page
    Break,
}

// Complex shape constraints for non-rectangular text flow
#[derive(Debug, Clone)]
pub(crate) struct ShapeConstraints {
    pub(crate) boundaries: Vec<ShapeBoundary>,
    pub(crate) exclusions: Vec<ShapeBoundary>,
    pub(crate) writing_mode: WritingMode,
    pub(crate) text_align: TextAlign,
    pub(crate) line_height: LineHeight,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum WritingMode {
    #[default]
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl, /* +spec:writing-modes:6e22a7 - vertical-rl (vertical right-to-left, commonly
                 * used in East Asia) */
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

impl WritingMode {
    /// Necessary to determine if the glyphs are advancing in a horizontal direction
    #[must_use]
    pub const fn is_advance_horizontal(&self) -> bool {
        matches!(
            self,
            Self::HorizontalTb | Self::SidewaysRl | Self::SidewaysLr
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum JustifyContent {
    #[default]
    None,
    InterWord,      // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,     // Distribute space evenly including start/end
    Kashida,        // Stretch Arabic text using kashidas
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,        // Logical start/end
    JustifyAll, // Justify including last line
}

// +spec:block-formatting-context:458d31 - vertical text orientation: upright for horizontal
// scripts, intrinsic for vertical scripts Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, PartialOrd, Ord, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

// Bidi and script detection
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BidiDirection {
    Ltr,
    Rtl,
}

impl BidiDirection {
    #[must_use]
    pub const fn is_rtl(&self) -> bool {
        matches!(self, Self::Rtl)
    }
}

/// CSS `unicode-bidi` property values relevant to layout.
///
/// When `Plaintext`, the bidi algorithm uses P2/P3 heuristics to auto-detect
/// paragraph direction from text content, instead of the HL1 override from
/// the CSS `direction` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum UnicodeBidi {
    #[default]
    Normal,
    Embed,
    Isolate,
    BidiOverride,
    IsolateOverride,
    Plaintext,
}
