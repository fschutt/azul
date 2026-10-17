//! An inline formatting context's content: text, images, shapes, spaces and breaks, and their glyphs.

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

// Enhanced content model supporting mixed inline content
// [g117 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)): the web lift MIS-READS a repr(Rust)
// niche/compiler-placed discriminant — `<InlineContent as Clone>::clone` and create_logical_items'
// match both mis-route a Text(disc 0) to a Vec-bearing variant → clone reads a heap ptr as a Vec
// len → ~789MB alloc → OOB (g111/g115/g116 named stack = InlineContent::clone ←
// create_logical_items; content is CLEAN: len=1, ptr ok, disc-at-0=0). An explicit u8 tag at offset
// 0 (no niche) lowers to a simple load the lift handles correctly — the layout other (repr(C,u8))
// enums use. Not FFI-exposed (internal to text3; only native shell code matches it), so the repr
// change is layout-safe.
#[derive(Debug, Clone, Hash, PartialEq)]
#[repr(C, u8)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
    /// Tab character - rendered with width based on tab-size CSS property
    Tab {
        style: Arc<StyleProperties>,
    },
    /// List marker (`::marker` pseudo-element)
    /// Markers with list-style-position: outside are positioned
    /// in the padding gutter of the list container
    Marker {
        run: StyledRun,
        /// Whether marker is positioned outside (in padding) or inside (inline)
        position_outside: bool,
    },
    // Ruby annotation
    Ruby {
        base: Vec<InlineContent>,
        text: Vec<InlineContent>,
        // Style for the ruby text itself
        style: Arc<StyleProperties>,
    },
}

#[derive(Debug, Clone)]
pub struct InlineImage {
    pub source: ImageSource,
    pub intrinsic_size: Size,
    pub display_size: Option<Size>,
    // How much to shift baseline
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub object_fit: ObjectFit,
}

impl PartialEq for InlineImage {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.source == other.source
            && self.intrinsic_size == other.intrinsic_size
            && self.display_size == other.display_size
            && self.alignment == other.alignment
            && self.object_fit == other.object_fit
    }
}

impl Eq for InlineImage {}

impl Hash for InlineImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.intrinsic_size.hash(state);
        self.display_size.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.alignment.hash(state);
        self.object_fit.hash(state);
    }
}

impl PartialOrd for InlineImage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineImage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source
            .cmp(&other.source)
            .then_with(|| self.intrinsic_size.cmp(&other.intrinsic_size))
            .then_with(|| self.display_size.cmp(&other.display_size))
            .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
            .then_with(|| self.alignment.cmp(&other.alignment))
            .then_with(|| self.object_fit.cmp(&other.object_fit))
    }
}

/// Enhanced glyph with all features
#[derive(Debug, Clone)]
pub struct Glyph {
    // Core glyph data
    pub glyph_id: u16,
    pub codepoint: char,
    /// Hash of the font - use `LoadedFonts` to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
    pub style: Arc<StyleProperties>,
    pub source: GlyphSource,

    // Text mapping
    pub logical_byte_index: usize,
    pub logical_byte_len: usize,
    pub content_index: usize,
    pub cluster: u32,

    // Metrics
    pub advance: f32,
    pub kerning: f32,
    pub offset: Point,

    // Vertical text support
    pub vertical_advance: f32,
    pub vertical_origin_y: f32, // from VORG
    pub vertical_bearing: Point,
    pub orientation: GlyphOrientation,

    // Layout properties
    pub script: Script,
    pub bidi_level: BidiLevel,
}

impl Glyph {
    #[inline]
    pub(super) fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.advance,
            height: self
                .style
                .line_height
                .resolve_with_metrics(self.style.font_size_px, &self.font_metrics),
        }
    }

    #[inline]
    pub(super) const fn character_class(&self) -> CharacterClass {
        classify_character(self.codepoint as u32)
    }

    #[inline]
    pub(super) fn is_whitespace(&self) -> bool {
        self.character_class() == CharacterClass::Space
    }

    #[inline]
    pub(super) fn can_justify(&self) -> bool {
        !self.codepoint.is_whitespace() && self.character_class() != CharacterClass::Combining
    }

    #[inline]
    pub(super) const fn justification_priority(&self) -> u8 {
        get_justification_priority(self.character_class())
    }

    #[inline]
    pub(super) const fn break_opportunity_after(&self) -> bool {
        let is_whitespace = self.codepoint.is_whitespace();
        let is_soft_hyphen = self.codepoint == '\u{00AD}';
        let is_hyphen_minus = self.codepoint == '\u{002D}';
        let is_hyphen = self.codepoint == '\u{2010}';
        is_whitespace || is_soft_hyphen || is_hyphen_minus || is_hyphen
    }
}

// Information about text runs after initial analysis
#[derive(Debug, Clone)]
pub(crate) struct TextRunInfo<'a> {
    pub(crate) text: &'a str,
    pub(crate) style: Arc<StyleProperties>,
    pub(crate) logical_start: usize,
    pub(crate) content_index: usize,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Direct reference to decoded image (from DOM `NodeType::Image`)
    Ref(ImageRef),
    /// The image content of a DOM node, resolved LIVE at paint time through
    /// the content overlay (overlay→DOM). Identity is the NODE, not the
    /// pixels — swapping the node's image repaints without invalidating the
    /// IFC this item is cached in. This is what `fc.rs` snapshots for
    /// `NodeType::Image` inline items (the old `Ref` snapshot froze the
    /// `ImageRef` at IFC-build time, so inline image swaps were invisible
    /// until a full relayout).
    Node(NodeId),
    /// CSS url reference (from background-image, needs `ImageCache` lookup)
    Url(String),
    /// Raw image data
    Data(Arc<[u8]>),
    /// SVG source
    Svg(Arc<str>),
    /// Placeholder for layout without actual image
    Placeholder(Size),
}

impl PartialEq for ImageSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ref(a), Self::Ref(b)) => a.get_hash() == b.get_hash(),
            (Self::Node(a), Self::Node(b)) => a == b,
            (Self::Url(a), Self::Url(b)) => a == b,
            (Self::Data(a), Self::Data(b)) => Arc::ptr_eq(a, b),
            (Self::Svg(a), Self::Svg(b)) => Arc::ptr_eq(a, b),
            (Self::Placeholder(a), Self::Placeholder(b)) => {
                a.width.to_bits() == b.width.to_bits() && a.height.to_bits() == b.height.to_bits()
            }
            _ => false,
        }
    }
}

impl Eq for ImageSource {}

impl Hash for ImageSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Ref(r) => r.get_hash().hash(state),
            Self::Node(n) => n.hash(state),
            Self::Url(s) => s.hash(state),
            Self::Data(d) => (Arc::as_ptr(d).cast::<u8>() as usize).hash(state),
            Self::Svg(s) => (Arc::as_ptr(s).cast::<u8>() as usize).hash(state),
            Self::Placeholder(sz) => {
                sz.width.to_bits().hash(state);
                sz.height.to_bits().hash(state);
            }
        }
    }
}

impl PartialOrd for ImageSource {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImageSource {
    fn cmp(&self, other: &Self) -> Ordering {
        const fn variant_index(s: &ImageSource) -> u8 {
            match s {
                ImageSource::Ref(_) => 0,
                ImageSource::Node(_) => 5,
                ImageSource::Url(_) => 1,
                ImageSource::Data(_) => 2,
                ImageSource::Svg(_) => 3,
                ImageSource::Placeholder(_) => 4,
            }
        }
        match (self, other) {
            (Self::Ref(a), Self::Ref(b)) => a.get_hash().cmp(&b.get_hash()),
            (Self::Node(a), Self::Node(b)) => a.cmp(b),
            (Self::Url(a), Self::Url(b)) => a.cmp(b),
            (Self::Data(a), Self::Data(b)) => {
                (Arc::as_ptr(a).cast::<u8>() as usize).cmp(&(Arc::as_ptr(b).cast::<u8>() as usize))
            }
            (Self::Svg(a), Self::Svg(b)) => {
                (Arc::as_ptr(a).cast::<u8>() as usize).cmp(&(Arc::as_ptr(b).cast::<u8>() as usize))
            }
            (Self::Placeholder(a), Self::Placeholder(b)) => (a.width.to_bits(), a.height.to_bits())
                .cmp(&(b.width.to_bits(), b.height.to_bits())),
            // Different variants: compare by variant index
            _ => variant_index(self).cmp(&variant_index(other)),
        }
    }
}

// +spec:font-metrics:fa104e - vertical-align values; baseline-source defaults to auto (first
// baseline) +spec:inline-formatting-context:340729 - alignment-baseline values for IFC baseline
// alignment (only baseline/top/bottom/middle implemented) CSS 2.2 §10.8.1 vertical-align property
// values +spec:display-property:0b1deb - inline boxes use dominant baseline to align text and
// inline-level children +spec:inline-formatting-context:3996a6 - dominant-baseline defaults to
// alphabetic in horizontal mode; vertical-align handles baseline alignment and super/sub shifting
#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum VerticalAlign {
    // Align baseline of box with baseline of parent box
    #[default]
    Baseline,
    // Align bottom of aligned subtree with bottom of line box
    Bottom,
    // Align top of aligned subtree with top of line box
    Top,
    // Align vertical midpoint of box with baseline of parent plus half x-height
    Middle,
    // Align top of box with top of parent's content area (§10.6.1)
    TextTop,
    // Align bottom of box with bottom of parent's content area (§10.6.1)
    TextBottom,
    // Lower baseline to proper subscript position
    Sub,
    // Raise baseline to proper superscript position
    Super,
    // +spec:font-metrics:152df3 - Raise (positive) or lower (negative) by this distance; 0 =
    // baseline
    Offset(f32),
}

impl Hash for VerticalAlign {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        if let Self::Offset(f) = self {
            f.to_bits().hash(state);
        }
    }
}

impl Eq for VerticalAlign {}

// cmp delegates to the derived PartialOrd (unwrap_or(Equal)), so Ord and PartialOrd are
// consistent; Ord can't be derived because of the f32 `Offset` variant.
#[allow(clippy::derive_ord_xor_partial_ord)]
impl Ord for VerticalAlign {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFit {
    // Stretch to fit display size
    Fill,
    // Scale to fit within display size
    Contain,
    // Scale to cover display size
    Cover,
    // Use intrinsic size
    None,
    // Like contain but never scale up
    ScaleDown,
}

/// Border information for inline elements (display: inline, inline-block)
///
/// This stores the resolved border properties needed for rendering inline element borders.
/// Unlike block elements which render borders via `paint_node_background_and_border()`,
/// inline element borders must be rendered per glyph-run to handle line breaks correctly.
#[derive(Copy, Debug, Clone, PartialEq)]
pub struct InlineBorderInfo {
    /// Border widths in pixels for each side
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    /// Border colors for each side
    pub top_color: ColorU,
    pub right_color: ColorU,
    pub bottom_color: ColorU,
    pub left_color: ColorU,
    /// Border radius (if any)
    pub radius: Option<f32>,
    /// Padding widths in pixels for each side (needed to expand background rect)
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    // +spec:box-model:c5723b - inline box split: suppress margin/border/padding at split points
    /// CSS 2.2 §9.4.2 / §8.6: when an inline box is split across line boxes,
    /// margins, borders, and padding have no visible effect at the split points.
    /// True if this is the first fragment of the inline box.
    pub is_first_fragment: bool,
    /// True if this is the last fragment of the inline box.
    pub is_last_fragment: bool,
    /// CSS 2.2 §8.6: direction flag for visual-order rendering in bidi context.
    /// LTR: first fragment gets left edge, last gets right edge.
    /// RTL: first fragment gets right edge, last gets left edge.
    pub is_rtl: bool,
    /// The left / right margins in pixels (CSS 2.2 s10.3.1: an inline box's
    /// horizontal margins apply; vertical ones do not). They move the pen
    /// like the border and padding but are not painted - the background
    /// covers the border box only ([`Self::left_inset`]).
    pub margin_left: f32,
    pub margin_right: f32,
}

impl Default for InlineBorderInfo {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
            top_color: ColorU::TRANSPARENT,
            right_color: ColorU::TRANSPARENT,
            bottom_color: ColorU::TRANSPARENT,
            left_color: ColorU::TRANSPARENT,
            radius: None,
            padding_top: 0.0,
            padding_right: 0.0,
            padding_bottom: 0.0,
            padding_left: 0.0,
            is_first_fragment: true,
            is_last_fragment: true,
            is_rtl: false,
            margin_left: 0.0,
            margin_right: 0.0,
        }
    }
}

impl InlineBorderInfo {
    /// Returns true if any border has a non-zero width
    #[must_use]
    pub fn has_border(&self) -> bool {
        self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0
    }

    /// Returns true if any border or padding is present
    #[must_use]
    pub fn has_chrome(&self) -> bool {
        self.has_border()
            || self.padding_top > 0.0
            || self.padding_right > 0.0
            || self.padding_bottom > 0.0
            || self.padding_left > 0.0
    }

    // +spec:box-model:da0ba2 - RTL bidi inline box split: left/right edges assigned to correct
    // fragments +spec:box-model:e9144f - visual-order margin/border/padding for inline boxes in
    // bidi context +spec:box-model:fac66f - Assigns margins/borders/padding in visual order for
    // bidi inline fragments +spec:box-model:720688 - LTR: left on first, right on last; RTL:
    // right on first, left on last +spec:positioning:1fcad6 - bidi-aware margin/border/padding
    // on inline box fragments per visual order
    /// Total left inset (border + padding), suppressed at split points per §8.6.
    /// In LTR: left edge drawn on first fragment. In RTL: left edge drawn on last fragment.
    // +spec:box-model:bae97f - visual-order margin/border/padding assignment for bidi inline
    // fragments
    #[must_use]
    pub fn left_inset(&self) -> f32 {
        let show = if self.is_rtl {
            self.is_last_fragment
        } else {
            self.is_first_fragment
        };
        if show {
            self.left + self.padding_left
        } else {
            0.0
        }
    }
    /// Total right inset (border + padding), suppressed at split points per §8.6.
    /// In LTR: right edge drawn on last fragment. In RTL: right edge drawn on first fragment.
    #[must_use]
    pub fn right_inset(&self) -> f32 {
        let show = if self.is_rtl {
            self.is_first_fragment
        } else {
            self.is_last_fragment
        };
        if show {
            self.right + self.padding_right
        } else {
            0.0
        }
    }
    /// Total top inset (border + padding)
    #[must_use]
    pub fn top_inset(&self) -> f32 {
        self.top + self.padding_top
    }
    /// Total bottom inset (border + padding)
    #[must_use]
    pub fn bottom_inset(&self) -> f32 {
        self.bottom + self.padding_bottom
    }

    /// How far the box moves the pen before its content: the left margin
    /// plus [`Self::left_inset`], suppressed at a split like the inset
    /// (CSS 2.2 s9.4.2: margins, borders and padding have no effect where
    /// an inline box is split).
    #[must_use]
    pub fn left_advance(&self) -> f32 {
        let inset = self.left_inset();
        let show = if self.is_rtl {
            self.is_last_fragment
        } else {
            self.is_first_fragment
        };
        if show {
            self.margin_left + inset
        } else {
            inset
        }
    }

    /// How far the box moves the pen after its content: [`Self::right_inset`]
    /// plus the right margin, suppressed at a split like the inset.
    #[must_use]
    pub fn right_advance(&self) -> f32 {
        let inset = self.right_inset();
        let show = if self.is_rtl {
            self.is_first_fragment
        } else {
            self.is_last_fragment
        };
        if show {
            inset + self.margin_right
        } else {
            inset
        }
    }

    /// Whether the box moves the pen at all: a border, a padding or a
    /// horizontal margin (a negative margin moves it too).
    #[must_use]
    pub fn moves_the_pen(&self) -> bool {
        self.has_chrome() || self.margin_left != 0.0 || self.margin_right != 0.0
    }

    /// Every field into `state`, the floats by their bits. `StyleProperties`'
    /// `Hash` is a cache KEY (the text cache's first stage), so two
    /// decorations must hash apart.
    pub fn hash_bits<H: Hasher>(&self, state: &mut H) {
        for v in [
            self.top,
            self.right,
            self.bottom,
            self.left,
            self.padding_top,
            self.padding_right,
            self.padding_bottom,
            self.padding_left,
            self.margin_left,
            self.margin_right,
        ] {
            v.to_bits().hash(state);
        }
        self.radius.map(f32::to_bits).hash(state);
        self.top_color.hash(state);
        self.right_color.hash(state);
        self.bottom_color.hash(state);
        self.left_color.hash(state);
        self.is_first_fragment.hash(state);
        self.is_last_fragment.hash(state);
        self.is_rtl.hash(state);
    }
}

#[derive(Debug, Clone)]
pub struct InlineShape {
    pub shape_def: ShapeDefinition,
    pub fill: Option<ColorU>,
    pub stroke: Option<Stroke>,
    pub baseline_offset: f32,
    /// Per-item vertical alignment (CSS `vertical-align` on the inline-block element).
    /// This overrides the global `TextStyleOptions::vertical_align` for this shape.
    pub alignment: VerticalAlign,
    /// The `NodeId` of the element that created this shape
    /// (e.g., inline-block) - this allows us to look up
    /// styling information (background, border) when rendering
    pub source_node_id: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) struct MeasuredImage {
    pub(crate) source: ImageSource,
    pub(crate) size: Size,
    pub(crate) baseline_offset: f32,
    pub(crate) alignment: VerticalAlign,
    pub(crate) content_index: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct MeasuredShape {
    pub(crate) shape_def: ShapeDefinition,
    pub(crate) size: Size,
    pub(crate) baseline_offset: f32,
    pub(crate) alignment: VerticalAlign,
    pub(crate) content_index: usize,
}

#[derive(Copy, Debug, Clone)]
pub struct InlineSpace {
    pub width: f32,
    pub is_breaking: bool, // Can line break here
    pub is_stretchy: bool, // Can be expanded for justification
}

impl PartialEq for InlineSpace {
    fn eq(&self, other: &Self) -> bool {
        self.width.to_bits() == other.width.to_bits()
            && self.is_breaking == other.is_breaking
            && self.is_stretchy == other.is_stretchy
    }
}

impl Eq for InlineSpace {}

impl Hash for InlineSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.to_bits().hash(state);
        self.is_breaking.hash(state);
        self.is_stretchy.hash(state);
    }
}

impl PartialOrd for InlineSpace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineSpace {
    fn cmp(&self, other: &Self) -> Ordering {
        self.width
            .total_cmp(&other.width)
            .then_with(|| self.is_breaking.cmp(&other.is_breaking))
            .then_with(|| self.is_stretchy.cmp(&other.is_stretchy))
    }
}

impl PartialEq for InlineShape {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.shape_def == other.shape_def
            && self.fill == other.fill
            && self.stroke == other.stroke
            && self.alignment == other.alignment
            && self.source_node_id == other.source_node_id
    }
}

impl Eq for InlineShape {}

impl Hash for InlineShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_def.hash(state);
        self.fill.hash(state);
        self.stroke.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.alignment.hash(state);
        self.source_node_id.hash(state);
    }
}

impl PartialOrd for InlineShape {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.shape_def
                .partial_cmp(&other.shape_def)?
                .then_with(|| self.fill.cmp(&other.fill))
                .then_with(|| {
                    self.stroke
                        .partial_cmp(&other.stroke)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
                .then_with(|| self.alignment.cmp(&other.alignment))
                .then_with(|| self.source_node_id.cmp(&other.source_node_id)),
        )
    }
}

#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
    pub content_index: usize,
}

// +spec:line-breaking:d70ffd - Defines forced line break (Hard) vs soft wrap break (Soft) types
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BreakType {
    Soft,   // Soft wrap break: UA creates unforced line breaks to fit content within the measure
    Hard,   // Forced line break: explicit line-breaking controls (preserved newline, <br>)
    Page,   // Page break
    Column, // Column break
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClearType {
    None,
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphSource {
    /// Glyph generated from a character in the source text.
    Char,
    /// Glyph inserted dynamically by the layout engine (e.g., a hyphen).
    Hyphen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterClass {
    Space,       // Regular spaces - highest justification priority
    Punctuation, // Can sometimes be adjusted
    Letter,      // Normal letters
    Ideograph,   // CJK characters - can be justified between
    Symbol,      // Symbols, emojis
    Combining,   // Combining marks - never justified
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphOrientation {
    Horizontal, // Keep horizontal (normal in horizontal text)
    Vertical,   // Rotate to vertical (normal in vertical text)
    Upright,    // Keep upright regardless of writing mode
    Mixed,      // Use script-specific default orientation
}
