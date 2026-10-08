//! Text styles: runs, font selection, decorations, spacing and the resolved style properties.

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

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone, Hash, PartialEq)]
pub struct StyledRun {
    /// The run's source text. `Arc<str>` since the §3.2 campaign step 2:
    /// this is THE single copy of a style run's text — logical items
    /// fragment it, shaping consumes it, and the dense model's
    /// `DenseRun.text` shares it, replacing every per-cluster `String`
    /// once the compact model takes over. (printpdf constructs zero
    /// `StyledRuns` — verified — so the type change is boundary-safe.)
    pub text: Arc<str>,
    pub style: Arc<StyleProperties>,
    /// Byte index in the original logical paragraph text
    pub logical_start_byte: usize,
    /// The DOM `NodeId` of the Text node this run came from.
    /// None for generated content (e.g., list markers, `::before/::after`).
    pub source_node_id: Option<NodeId>,
}

// Stage 2: Bidi Analysis - Visual runs in display order
#[derive(Debug, Clone)]
pub struct VisualRun<'a> {
    pub text_slice: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
    pub bidi_level: BidiLevel,
    pub script: Script,
    pub language: Language,
}

// Font and styling types

/// A selector for loading fonts from the font cache.
/// Used by `FontManager` to query fontconfig and load font files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontSelector {
    pub family: String,
    pub weight: FcWeight,
    pub style: FontStyle,
    pub unicode_ranges: Vec<UnicodeRange>,
    /// The optical size to draw a variable face with an `opsz` axis at: the
    /// used font size rounded to whole CSS px (CSS Fonts 4
    /// `font-optical-sizing: auto`; Chrome and CoreText set `opsz` to the
    /// font size). 0 = the face's default instance. Every selector of a
    /// stack carries the same value; [`FontChainKey::from_selectors`] reads
    /// the first. macOS draws its UI in ONE such face (`SFNS.ttf`, opsz
    /// 17-96, default 28): 13px text is "SF Pro Text" (opsz 17), not the
    /// default's wider-set "Display" design (SYSUI8).
    pub optical_size: u16,
}

impl Default for FontSelector {
    fn default() -> Self {
        Self {
            family: "serif".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
            optical_size: 0,
        }
    }
}

/// The optical size (`FontSelector::optical_size`) for text of
/// `font_size_px`: the size in whole CSS px, 0 for a size that is not a
/// positive finite number.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to u16 first
pub fn optical_size_for(font_size_px: f32) -> u16 {
    if font_size_px.is_finite() && font_size_px > 0.0 {
        font_size_px.round().clamp(1.0, f32::from(u16::MAX)) as u16
    } else {
        0
    }
}

/// Font stack that can be either a list of font selectors (resolved via fontconfig)
/// or a direct `FontRef` (bypasses fontconfig entirely).
///
/// When a `FontRef` is used, it bypasses fontconfig resolution entirely
/// and uses the pre-parsed font data directly. This is used for embedded
/// fonts like Material Icons.
// [g121 az-web-lift] `#[repr(C, u8)]` — same disc-mis-lift guard as the other text3 enums; matched
// in shape_visual_items (`match &style.font_stack { Ref => shape, Stack => resolve }`). repr(Rust)
// niche (from the Vec/FontRef payloads) could mis-route. Explicit u8 tag = simple load. Internal to
// text3.
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum FontStack {
    /// A stack of font selectors to be resolved via fontconfig
    /// First font is primary, rest are fallbacks
    Stack(Vec<FontSelector>),
    /// A direct reference to a pre-parsed font (e.g., embedded icon fonts)
    /// This font covers the entire Unicode range and has no fallbacks.
    Ref(azul_css::props::basic::font::FontRef),
}

impl Default for FontStack {
    fn default() -> Self {
        Self::Stack(vec![FontSelector::default()])
    }
}

impl FontStack {
    /// Returns true if this is a direct `FontRef`
    #[must_use]
    pub const fn is_ref(&self) -> bool {
        matches!(self, Self::Ref(_))
    }

    /// Returns the `FontRef` if this is a Ref variant
    #[must_use]
    pub const fn as_ref(&self) -> Option<&azul_css::props::basic::font::FontRef> {
        match self {
            Self::Ref(r) => Some(r),
            Self::Stack(_) => None,
        }
    }

    /// Returns the font selectors if this is a Stack variant
    #[must_use]
    pub fn as_stack(&self) -> Option<&[FontSelector]> {
        match self {
            Self::Stack(s) => Some(s),
            Self::Ref(_) => None,
        }
    }

    /// Returns the first `FontSelector` if this is a Stack variant, None if Ref
    #[must_use]
    pub fn first_selector(&self) -> Option<&FontSelector> {
        match self {
            Self::Stack(s) => s.first(),
            Self::Ref(_) => None,
        }
    }

    /// Returns the first font family name (for Stack) or a placeholder (for Ref)
    #[must_use]
    pub fn first_family(&self) -> &str {
        match self {
            Self::Stack(s) => s.first().map_or("serif", |f| f.family.as_str()),
            Self::Ref(_) => "<embedded-font>",
        }
    }
}

impl PartialEq for FontStack {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Stack(a), Self::Stack(b)) => a == b,
            (Self::Ref(a), Self::Ref(b)) => a.parsed == b.parsed,
            _ => false,
        }
    }
}

impl Eq for FontStack {}

impl Hash for FontStack {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Stack(s) => s.hash(state),
            Self::Ref(r) => (r.parsed as usize).hash(state),
        }
    }
}

/// A reference to a font for rendering, identified by its hash.
/// This hash corresponds to `ParsedFont::hash` and is used to look up
/// the actual font data in the renderer's font cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontHash {
    /// The hash of the `ParsedFont`. 0 means invalid/unknown font.
    pub font_hash: u64,
}

impl FontHash {
    #[must_use]
    pub const fn invalid() -> Self {
        Self { font_hash: 0 }
    }

    #[must_use]
    pub const fn from_hash(font_hash: u64) -> Self {
        Self { font_hash }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

impl TextDecoration {
    /// Both sets of lines: a run's own and those it carries from the boxes
    /// around it (`solver3::getters::propagated_text_decoration`).
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self {
            underline: self.underline || other.underline,
            strikethrough: self.strikethrough || other.strikethrough,
            overline: self.overline || other.overline,
        }
    }

    /// Convert from CSS `StyleTextDecoration` enum to our internal representation.
    ///
    /// Note: CSS text-decoration can have multiple values (underline line-through),
    /// but the current azul-css parser only supports single values. This can be
    /// extended in the future if CSS parsing is updated.
    #[must_use]
    pub fn from_css(css: azul_css::props::style::text::StyleTextDecoration) -> Self {
        use azul_css::props::style::text::StyleTextDecoration;
        match css {
            StyleTextDecoration::None => Self::default(),
            StyleTextDecoration::Underline => Self {
                underline: true,
                strikethrough: false,
                overline: false,
            },
            StyleTextDecoration::Overline => Self {
                underline: false,
                strikethrough: false,
                overline: true,
            },
            StyleTextDecoration::LineThrough => Self {
                underline: false,
                strikethrough: true,
                overline: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
    // only within preserved white space (non-preserved spaces already collapsed in Phase I)
    FullWidth,
}

// Type alias for OpenType feature tags
pub type FourCc = [u8; 4];

// Enum for relative or absolute spacing
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Spacing {
    Px(i32), // Whole-pixel spacing (kept for hashing/equality convenience)
    /// Sub-pixel resolved pixel spacing. `letter-spacing`/`word-spacing` accumulate
    /// once per glyph, so quantizing to whole pixels (the `Px(i32)` variant) multiplies
    /// the rounding error across a run. The CSS resolution path emits this variant to
    /// preserve the exact sub-pixel value (e.g. `letter-spacing: 0.4px`).
    PxF(f32),
    Em(f32),
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// The derived `PartialEq` is sufficient for this marker trait.
impl Eq for Spacing {}

impl Hash for Spacing {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // First, hash the enum variant to distinguish between Px and Em.
        discriminant(self).hash(state);
        match self {
            Self::Px(val) => val.hash(state),
            // For hashing floats, convert them to their raw bit representation.
            // This ensures that identical float values produce identical hashes.
            Self::PxF(val) | Self::Em(val) => val.to_bits().hash(state),
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Self::Px(0)
    }
}

impl Spacing {
    /// Resolve this spacing to pixels given the element's font size (for `Em`).
    #[allow(clippy::cast_precision_loss)] // small integer px values; f32 mantissa is ample
    #[must_use]
    pub fn resolve_px(self, font_size_px: f32) -> f32 {
        match self {
            Self::Px(px) => px as f32,
            Self::PxF(px) => px,
            Self::Em(em) => em * font_size_px,
        }
    }
}

impl Default for FontHash {
    fn default() -> Self {
        Self::invalid()
    }
}

/// Style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    /// Font stack for fallback support (priority order)
    /// Can be either a list of `FontSelectors` (resolved via fontconfig)
    /// or a direct `FontRef` (bypasses fontconfig entirely).
    pub font_stack: FontStack,
    pub font_size_px: f32,
    pub color: ColorU,
    /// Background color for inline elements (e.g., `<span style="background-color: yellow">`)
    ///
    /// This is propagated from CSS through the style system and eventually used by
    /// the PDF renderer to draw filled rectangles behind text. The value is `None`
    /// for transparent backgrounds (the default).
    ///
    /// The propagation chain is:
    /// CSS -> `get_style_properties()` -> `StyleProperties` -> `ShapedGlyph` -> `PdfGlyphRun`
    ///
    /// See `PdfGlyphRun::background_color` for how this is used in PDF rendering.
    pub background_color: Option<ColorU>,
    /// Full background content layers (for gradients, images, etc.)
    /// This extends `background_color` to support CSS gradients on inline elements.
    pub background_content: Vec<StyleBackgroundContent>,
    /// Border information for inline elements
    pub border: Option<InlineBorderInfo>,
    // +spec:text-alignment-spacing:b39a04 - word-spacing and letter-spacing control text spacing
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,

    pub line_height: LineHeight,
    pub text_decoration: TextDecoration,

    // Represents CSS font-feature-settings like `"liga"`, `"smcp=1"`.
    pub font_features: Vec<String>,

    // Variable fonts
    pub font_variations: Vec<(FourCc, f32)>,
    // Multiplier of the space width
    pub tab_size: f32,
    // text-transform
    pub text_transform: TextTransform,
    // Vertical text properties
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    // Tate-chu-yoko
    pub text_combine_upright: Option<TextCombineUpright>,

    // Variant handling
    pub font_variant_caps: FontVariantCaps,
    pub font_variant_numeric: FontVariantNumeric,
    pub font_variant_ligatures: FontVariantLigatures,
    pub font_variant_east_asian: FontVariantEastAsian,

    /// The element's own `vertical-align` (baseline / sub / super / length / percentage).
    /// Read per shaped cluster by `get_item_vertical_align` so an inline `<span>` shifts
    /// its text relative to the line baseline. `Baseline` (the default) leaves the cluster
    /// on the line's default alignment.
    pub vertical_align: VerticalAlign,
}

impl Default for StyleProperties {
    fn default() -> Self {
        const FONT_SIZE: f32 = 16.0;
        const TAB_SIZE: f32 = 8.0;
        Self {
            font_stack: FontStack::default(),
            font_size_px: FONT_SIZE,
            color: ColorU::default(),
            background_color: None,
            background_content: Vec::new(),
            border: None,
            letter_spacing: Spacing::default(), // Px(0)
            word_spacing: Spacing::default(),   // Px(0)
            line_height: LineHeight::Normal,
            text_decoration: TextDecoration::default(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            tab_size: TAB_SIZE, // CSS default
            text_transform: TextTransform::default(),
            writing_mode: WritingMode::default(),
            text_orientation: TextOrientation::default(),
            text_combine_upright: None,
            font_variant_caps: FontVariantCaps::default(),
            font_variant_numeric: FontVariantNumeric::default(),
            font_variant_ligatures: FontVariantLigatures::default(),
            font_variant_east_asian: FontVariantEastAsian::default(),
            vertical_align: VerticalAlign::Baseline,
        }
    }
}

impl StyleProperties {
    /// Whether the text reads as bold to a format that has only a boolean for
    /// it (a clipboard flavour, an editor's B button): its first font asks for
    /// a weight of 700 or more. `FcWeight` is ordered by its CSS numeric
    /// value, so this is that comparison.
    #[must_use]
    pub fn is_bold(&self) -> bool {
        self.font_stack
            .first_selector()
            .is_some_and(|s| s.weight >= FcWeight::Bold)
    }

    /// Whether the text reads as italic: oblique is a slanted rendering of an
    /// upright face, and every format this feeds collapses it into italic.
    #[must_use]
    pub fn is_italic(&self) -> bool {
        self.font_stack
            .first_selector()
            .is_some_and(|s| matches!(s.style, FontStyle::Italic | FontStyle::Oblique))
    }

    /// This style with every font of its stack asking for `weight` - a
    /// direct font reference (an embedded icon font) has no weight to ask for
    /// and stays as it is.
    #[must_use]
    pub fn with_font_weight(&self, weight: FcWeight) -> Self {
        let mut style = self.clone();
        if let FontStack::Stack(selectors) = &mut style.font_stack {
            for selector in selectors {
                selector.weight = weight;
            }
        }
        style
    }

    /// This style with every font of its stack asking for `font_style` (see
    /// [`Self::with_font_weight`]).
    #[must_use]
    pub fn with_font_style(&self, font_style: FontStyle) -> Self {
        let mut style = self.clone();
        if let FontStack::Stack(selectors) = &mut style.font_stack {
            for selector in selectors {
                selector.style = font_style;
            }
        }
        style
    }
}

impl Hash for StyleProperties {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.color.hash(state);
        self.background_color.hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.vertical_align.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // For f32 fields, round and cast to usize before hashing.
        (self.font_size_px.round() as isize).hash(state);
        self.line_height.hash(state);

        // The inline box's decoration. This hash is a cache KEY - the text
        // cache's first stage is `calculate_id(&content)` - so a span's text
        // styled with the span's border / padding / background image must not
        // hash like the same text styled with the text node's own style: the
        // intrinsic-sizing pass collects it that way, and its border-less
        // items were served to the final layout, so a span with a border
        // but no background drew no border and moved nothing (WPT
        // inline-formatting-context-004).
        self.background_content.hash(state);
        match &self.border {
            None => 0_u8.hash(state),
            Some(b) => {
                1_u8.hash(state);
                b.hash_bits(state);
            }
        }
    }
}

impl StyleProperties {
    /// Returns a hash that only includes properties that affect text layout.
    ///
    /// Properties that DON'T affect layout (only rendering):
    /// - color, `background_color`, `background_content`
    /// - `text_decoration` (underline, etc.)
    /// - an inline box's border colours and its top / bottom border and padding
    ///
    /// Properties that DO affect layout:
    /// - `font_stack`, `font_size_px`, `font_features`, `font_variations`
    /// - `letter_spacing`, `word_spacing`, `line_height`, `tab_size`
    /// - `writing_mode`, `text_orientation`, `text_combine_upright`
    /// - `text_transform`
    /// - `font_variant`_* (affects glyph selection)
    /// - an inline box's left / right border, padding and margin: they move
    ///   the pen (`inline_offsets` in `position_one_line`)
    ///
    /// This allows the layout cache to reuse layouts when only rendering
    /// properties change (e.g., color changes on hover).
    // (family, weight, style) so that shaping runs break at element boundaries where font
    // properties differ, preventing impossible cross-boundary ligatures (e.g. "and" → "&").
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    #[must_use]
    pub fn layout_hash(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = DefaultHasher::new();

        // Font selection (affects shaping and metrics)
        self.font_stack.hash(&mut hasher);
        // Hash the EXACT font size bits, not a rounded integer: two styles differing
        // by <0.5px must not share a shaping-cache entry / coalesce, or one run gets
        // shaped at the other's size (wrong advances/metrics).
        self.font_size_px.to_bits().hash(&mut hasher);
        self.font_features.hash(&mut hasher);
        // font_variations affects glyph outlines
        for (tag, value) in &self.font_variations {
            tag.hash(&mut hasher);
            (value.round() as i32).hash(&mut hasher);
        }

        // Spacing (affects glyph positions)
        self.letter_spacing.hash(&mut hasher);
        self.word_spacing.hash(&mut hasher);
        self.line_height.hash(&mut hasher);
        (self.tab_size.round() as isize).hash(&mut hasher);

        // Writing mode (affects layout direction)
        self.writing_mode.hash(&mut hasher);
        self.text_orientation.hash(&mut hasher);
        self.text_combine_upright.hash(&mut hasher);

        // Text transform (affects which characters are used)
        self.text_transform.hash(&mut hasher);

        // Font variants (affect glyph selection)
        self.font_variant_caps.hash(&mut hasher);
        self.font_variant_numeric.hash(&mut hasher);
        self.font_variant_ligatures.hash(&mut hasher);
        self.font_variant_east_asian.hash(&mut hasher);

        // An inline box's horizontal margin + border + padding move the pen
        // (`inline_offsets` in `position_one_line`): a span that gains one
        // must not reuse the old positions. No box and a box of zeros hash
        // alike.
        let (start, end) = self.border.as_ref().map_or((0.0_f32, 0.0_f32), |b| {
            (b.left_advance(), b.right_advance())
        });
        start.to_bits().hash(&mut hasher);
        end.to_bits().hash(&mut hasher);

        hasher.finish()
    }

    /// Check if two `StyleProperties` have the same layout-affecting properties.
    ///
    /// Returns true if the layouts would be identical (only rendering differs).
    ///
    /// **Note:** This is a fast-path comparison using 64-bit hashes.  Hash
    /// collisions are theoretically possible, which could cause the cache to
    /// serve a stale layout.  In practice the probability is negligible for
    /// the number of distinct `StyleProperties` values in a single document.
    #[must_use]
    pub fn layout_eq(&self, other: &Self) -> bool {
        self.layout_hash() == other.layout_hash()
    }
}

#[derive(Copy, Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub enum TextCombineUpright {
    None,
    All,        // Combine all characters in horizontal layout
    Digits(u8), // Combine up to N digits
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

/// The text's `font-variant-numeric` (CSS Fonts 4 s6.7) - the CSS value as
/// it is: one choice per group (figures, spacing, fractions) plus `ordinal`
/// and `slashed-zero`, ANY combination of them (`tabular-nums lining-nums
/// slashed-zero`), which a single keyword could not hold. The shaper turns on
/// [`StyleFontVariantNumeric::opentype_features`] (`lnum`, `tnum`, `zero`, ...).
///
/// [`StyleFontVariantNumeric::opentype_features`]: azul_css::props::basic::font::StyleFontVariantNumeric::opentype_features
pub type FontVariantNumeric = azul_css::props::basic::font::StyleFontVariantNumeric;

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantLigatures {
    #[default]
    Normal,
    None,
    Common,
    NoCommon,
    Discretionary,
    NoDiscretionary,
    Historical,
    NoHistorical,
    Contextual,
    NoContextual,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantEastAsian {
    #[default]
    Normal,
    Jis78,
    Jis83,
    Jis90,
    Jis04,
    Simplified,
    Traditional,
    FullWidth,
    ProportionalWidth,
    Ruby,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiLevel(u8);

impl BidiLevel {
    #[must_use]
    pub const fn new(level: u8) -> Self {
        Self(level)
    }
    #[must_use]
    pub const fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.0
    }
}

// Add this new struct for style overrides
#[derive(Debug, Clone)]
pub struct StyleOverride {
    /// The specific character this override applies to.
    pub target: ContentIndex,
    /// The style properties to apply.
    /// Any `None` value means "inherit from the base style".
    pub style: PartialStyleProperties,
}

#[derive(Debug, Clone, Default)]
pub struct PartialStyleProperties {
    pub font_stack: Option<FontStack>,
    pub font_size_px: Option<f32>,
    pub color: Option<ColorU>,
    pub letter_spacing: Option<Spacing>,
    pub word_spacing: Option<Spacing>,
    pub line_height: Option<LineHeight>,
    pub text_decoration: Option<TextDecoration>,
    pub font_features: Option<Vec<String>>,
    pub font_variations: Option<Vec<(FourCc, f32)>>,
    pub tab_size: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: Option<TextOrientation>,
    pub text_combine_upright: Option<Option<TextCombineUpright>>,
    pub font_variant_caps: Option<FontVariantCaps>,
    pub font_variant_numeric: Option<FontVariantNumeric>,
    pub font_variant_ligatures: Option<FontVariantLigatures>,
    pub font_variant_east_asian: Option<FontVariantEastAsian>,
}

impl Hash for PartialStyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.font_size_px.map(f32::to_bits).hash(state);
        self.color.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);
        self.line_height.hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);

        // Manual hashing for Vec<(FourCc, f32)>
        if let Some(v) = self.font_variations.as_ref() {
            for (tag, val) in v {
                tag.hash(state);
                val.to_bits().hash(state);
            }
        }

        self.tab_size.map(f32::to_bits).hash(state);
        self.text_transform.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.font_variant_caps.hash(state);
        self.font_variant_numeric.hash(state);
        self.font_variant_ligatures.hash(state);
        self.font_variant_east_asian.hash(state);
    }
}

impl PartialEq for PartialStyleProperties {
    fn eq(&self, other: &Self) -> bool {
        self.font_stack == other.font_stack &&
        self.font_size_px.map(f32::to_bits) == other.font_size_px.map(f32::to_bits) &&
        self.color == other.color &&
        self.letter_spacing == other.letter_spacing &&
        self.word_spacing == other.word_spacing &&
        self.line_height == other.line_height &&
        self.text_decoration == other.text_decoration &&
        self.font_features == other.font_features &&
        self.font_variations == other.font_variations && // Vec<(FourCc, f32)> is PartialEq
        self.tab_size.map(f32::to_bits) == other.tab_size.map(f32::to_bits) &&
        self.text_transform == other.text_transform &&
        self.writing_mode == other.writing_mode &&
        self.text_orientation == other.text_orientation &&
        self.text_combine_upright == other.text_combine_upright &&
        self.font_variant_caps == other.font_variant_caps &&
        self.font_variant_numeric == other.font_variant_numeric &&
        self.font_variant_ligatures == other.font_variant_ligatures &&
        self.font_variant_east_asian == other.font_variant_east_asian
    }
}

impl Eq for PartialStyleProperties {}

impl StyleProperties {
    pub(super) fn apply_override(&self, partial: &PartialStyleProperties) -> Self {
        let mut new_style = self.clone();
        if let Some(val) = &partial.font_stack {
            new_style.font_stack = val.clone();
        }
        if let Some(val) = partial.font_size_px {
            new_style.font_size_px = val;
        }
        if let Some(val) = &partial.color {
            new_style.color = *val;
        }
        if let Some(val) = partial.letter_spacing {
            new_style.letter_spacing = val;
        }
        if let Some(val) = partial.word_spacing {
            new_style.word_spacing = val;
        }
        if let Some(val) = partial.line_height {
            new_style.line_height = val;
        }
        if let Some(val) = &partial.text_decoration {
            new_style.text_decoration = *val;
        }
        if let Some(val) = &partial.font_features {
            new_style.font_features.clone_from(val);
        }
        if let Some(val) = &partial.font_variations {
            new_style.font_variations.clone_from(val);
        }
        if let Some(val) = partial.tab_size {
            new_style.tab_size = val;
        }
        if let Some(val) = partial.text_transform {
            new_style.text_transform = val;
        }
        if let Some(val) = partial.writing_mode {
            new_style.writing_mode = val;
        }
        if let Some(val) = partial.text_orientation {
            new_style.text_orientation = val;
        }
        if let Some(val) = &partial.text_combine_upright {
            new_style.text_combine_upright.clone_from(val);
        }
        if let Some(val) = partial.font_variant_caps {
            new_style.font_variant_caps = val;
        }
        if let Some(val) = partial.font_variant_numeric {
            new_style.font_variant_numeric = val;
        }
        if let Some(val) = partial.font_variant_ligatures {
            new_style.font_variant_ligatures = val;
        }
        if let Some(val) = partial.font_variant_east_asian {
            new_style.font_variant_east_asian = val;
        }
        new_style
    }
}
