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

// Font and styling types

/// A selector for loading fonts from the font cache.
/// Used by `FontManager` to query fontconfig and load font files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontSelector {
    pub family: String,
    /// What the style asks of the family's face: the exact weight, stretch, style and slant, the
    /// optical size and the explicit `font-variation-settings` ([`FontRequest`]). Every selector
    /// of a stack carries the request of its node (the generic families a stack ends in the
    /// default one); [`FontChainKey::from_selectors`] reads the first.
    pub request: FontRequest,
    pub unicode_ranges: Vec<UnicodeRange>,
}

impl Default for FontSelector {
    fn default() -> Self {
        Self {
            family: "serif".to_string(),
            request: FontRequest::default(),
            unicode_ranges: Vec::new(),
        }
    }
}

/// The optical size ([`FontRequest::optical_size`]) for text of
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
// in shape_bidi_runs (`match &style.font_stack { Ref => shape, Stack => resolve }`). repr(Rust)
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
    /// Draw the face thickened (synthetic bold): the text asks for a bold
    /// weight and the face it matched has no bold cut. Read by both
    /// rasterizers (`crate::cpurender::embolden`); lookups by face use
    /// `font_hash` alone, so a reader that cannot thicken draws the face as
    /// it is. Part of the run's identity, so a run that turns bold repaints.
    pub synthetic_bold: bool,
}

impl FontHash {
    #[must_use]
    pub const fn invalid() -> Self {
        Self {
            font_hash: 0,
            synthetic_bold: false,
        }
    }

    #[must_use]
    pub const fn from_hash(font_hash: u64) -> Self {
        Self {
            font_hash,
            synthetic_bold: false,
        }
    }

    /// This face, drawn thickened when `on` ([`Self::synthetic_bold`]).
    #[must_use]
    pub const fn with_synthetic_bold(self, on: bool) -> Self {
        Self {
            font_hash: self.font_hash,
            synthetic_bold: on,
        }
    }
}

/// Whether text is drawn thickened (synthetic bold) from a face whose OS/2
/// weight class is `face_weight` (`None` when the face does not say): the
/// text asks for a bold weight (600 or more, [`StyleProperties::asks_for_bold`];
/// CSS Fonts 4 `font-synthesis-weight: auto`) and the face is lighter than
/// that. A face that does not tell its weight is never thickened.
#[must_use]
pub fn needs_synthetic_bold(asks_for_bold: bool, face_weight: Option<u16>) -> bool {
    asks_for_bold && face_weight.is_some_and(|w| w > 0 && w < 600)
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
    /// The inline boxes the text sits in whose margin, border or padding
    /// move the pen, the outermost first (its own span last). Two clusters
    /// side by side are in the same box when both chains name its node, so
    /// a span's start edge is added where its first cluster begins and its
    /// end edge after its last one, also across the nested spans inside it
    /// (`measure::inline_box_edge_advances`). Empty outside such boxes.
    pub inline_boxes: Arc<[InlineBoxEdge]>,
    // +spec:text-alignment-spacing:b39a04 - word-spacing and letter-spacing control text spacing
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,

    pub line_height: LineHeight,
    pub text_decoration: TextDecoration,

    /// The OpenType features `font-feature-settings` asks for, each `tag=value`
    /// (`liga=0`, `tnum=1`, `salt=2`), read by the shaper's
    /// `parse_font_feature`: 0 turns a feature off (also one the shaper turns
    /// on by default), 1 on, more picks an alternate.
    pub font_features: Vec<String>,

    // Multiplier of the space width
    pub tab_size: f32,
    // text-transform
    pub text_transform: TextTransform,
    /// `writing-mode`: how the text's lines run (`vertical-rl` and `vertical-lr` are vertical). Read
    /// by the PDF glyph runs, which say it, and by the runs a paint merges.
    pub writing_mode: WritingMode,
    /// Tate-chu-yoko (`text-combine-upright`): the digits or the whole run set side by side in
    /// one em of a vertical line. The only trigger of combined text in the logical items.
    pub text_combine_upright: Option<TextCombineUpright>,

    // Variant handling
    pub font_variant_caps: FontVariantCaps,
    pub font_variant_numeric: FontVariantNumeric,
    pub font_variant_ligatures: FontVariantLigatures,
    pub font_variant_east_asian: FontVariantEastAsian,

    /// `font-synthesis`: which faces the engine may fake where the font has none (the bold, the
    /// italic, the small capitals, the super- and subscripts).
    pub font_synthesis: FontSynthesis,

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
            inline_boxes: Arc::from(Vec::new()),
            letter_spacing: Spacing::default(), // Px(0)
            word_spacing: Spacing::default(),   // Px(0)
            line_height: LineHeight::Normal,
            text_decoration: TextDecoration::default(),
            font_features: Vec::new(),
            tab_size: TAB_SIZE, // CSS default
            text_transform: TextTransform::default(),
            writing_mode: WritingMode::default(),
            text_combine_upright: None,
            font_variant_caps: FontVariantCaps::default(),
            font_variant_numeric: FontVariantNumeric::default(),
            font_variant_ligatures: FontVariantLigatures::default(),
            font_variant_east_asian: FontVariantEastAsian::default(),
            font_synthesis: FontSynthesis::default(),
            vertical_align: VerticalAlign::Baseline,
        }
    }
}

impl StyleProperties {
    /// Whether the text reads as bold to a format that has only a boolean for
    /// it (a clipboard flavour, an editor's B button): its first font asks for
    /// a weight of 700 or more, exactly (a weight of 699 does not).
    #[must_use]
    pub fn is_bold(&self) -> bool {
        self.font_stack
            .first_selector()
            .is_some_and(|s| s.request.weight_at_least(700))
    }

    /// The weight the text asks its faces for, `None` for a direct font
    /// reference (an embedded icon font), which has no weight to ask for.
    /// The display list compares it with the matched face's weight to
    /// decide on synthetic bold ([`needs_synthetic_bold`]).
    #[must_use]
    pub fn requested_weight(&self) -> Option<FontCoord> {
        self.font_stack.first_selector().map(|s| s.request.weight)
    }

    /// Whether the text asks for a bold weight (600 or more), the weights
    /// a face without a bold cut is thickened for.
    #[must_use]
    pub fn asks_for_bold(&self) -> bool {
        self.font_stack
            .first_selector()
            .is_some_and(|s| s.request.weight_at_least(600))
    }

    /// Whether a face without a bold cut is drawn thickened for this text: it asks for a bold
    /// weight ([`Self::asks_for_bold`]) and `font-synthesis-weight` is not `none`. The one
    /// question every synthetic bold - the display list's, the PDF run's - is decided by.
    #[must_use]
    pub fn asks_for_synthetic_bold(&self) -> bool {
        self.font_synthesis.weight && self.asks_for_bold()
    }

    /// Whether the text reads as italic: oblique is a slanted rendering of an
    /// upright face, and every format this feeds collapses it into italic.
    #[must_use]
    pub fn is_italic(&self) -> bool {
        self.font_stack
            .first_selector()
            .is_some_and(|s| matches!(s.request.style, FontStyle::Italic | FontStyle::Oblique))
    }

    /// This style with every font of its stack asking for the weight of the
    /// bucket `weight` - a direct font reference (an embedded icon font) has
    /// no weight to ask for and stays as it is.
    #[must_use]
    pub fn with_font_weight(&self, weight: FcWeight) -> Self {
        let mut style = self.clone();
        if let FontStack::Stack(selectors) = &mut style.font_stack {
            for selector in selectors {
                selector.request = selector.request.clone().with_weight_bucket(weight);
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
                selector.request = selector.request.clone().with_style(font_style);
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
        self.text_combine_upright.hash(state);
        self.vertical_align.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // The size by its BITS, and every field that shapes or lays text out
        // (those of `layout_hash`). This hash is a cache KEY with no equality
        // check behind it: the text cache's first stage keeps a paragraph's
        // logical items - each holding this style's `Arc`, which shaping and
        // the display list read - under `calculate_id(&content)`. The size was
        // rounded to whole px and `text-transform`, the font variants and
        // variations (now the request of the font stack, hashed with it) and `tab-size` were left out, so the same text at 21.3 px
        // was served the logical items (and the style) of its 21.0 px layout,
        // and a node whose `text-transform` changed kept the old one.
        self.font_size_px.to_bits().hash(state);
        self.line_height.hash(state);
        self.tab_size.to_bits().hash(state);
        self.text_transform.hash(state);
        self.font_variant_caps.hash(state);
        self.font_variant_numeric.hash(state);
        self.font_variant_ligatures.hash(state);
        self.font_variant_east_asian.hash(state);
        self.font_synthesis.hash(state);

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
        for edge in self.inline_boxes.iter() {
            edge.hash_bits(state);
        }
    }
}

/// One inline box (a `<span>`) whose margin, border and padding move the pen
/// ([`StyleProperties::inline_boxes`]): the box's identity and how far its
/// left and right edges push the text, in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineBoxEdge {
    /// The inline element. Clusters compare it to tell whether they sit in
    /// the same box; the display list paints the box's fragments by it.
    pub node: NodeId,
    /// Left margin + border + padding: the pen moves this far before the
    /// box's first cluster.
    pub left: f32,
    /// Right padding + border + margin: the pen moves this far after the
    /// box's last cluster.
    pub right: f32,
}

impl InlineBoxEdge {
    /// The edge of the span `node` styled `style` (its own decoration in
    /// `style.border`), `None` when it moves the pen nowhere.
    #[must_use]
    pub fn of(node: NodeId, style: &StyleProperties) -> Option<Self> {
        let border = style.border.as_ref()?;
        let edge = Self {
            node,
            left: border.left_advance(),
            right: border.right_advance(),
        };
        (edge.left.abs() > f32::EPSILON || edge.right.abs() > f32::EPSILON).then_some(edge)
    }

    /// The fields into `state`, the floats by their bits (the style's
    /// `Hash` is a cache key).
    pub fn hash_bits<H: Hasher>(&self, state: &mut H) {
        self.node.index().hash(state);
        self.left.to_bits().hash(state);
        self.right.to_bits().hash(state);
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
    /// - `font_stack` (with the request of each font: weight, stretch, style, optical size and
    ///   `font-variation-settings`), `font_size_px`, `font_features`
    /// - `letter_spacing`, `word_spacing`, `line_height`, `tab_size`
    /// - `writing_mode`, `text_combine_upright`
    /// - `text_transform`
    /// - `font_variant`_* (affects glyph selection) and `font_synthesis` (small capitals are
    ///   synthesized or not)
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
        #[cfg(test)]
        audit_a8_tests::LAYOUT_HASHES.with(|n| n.set(n.get() + 1));
        let mut hasher = DefaultHasher::new();

        // Font selection (affects shaping and metrics)
        self.font_stack.hash(&mut hasher);
        // Hash the EXACT font size bits, not a rounded integer: two styles differing
        // by <0.5px must not share a shaping-cache entry / coalesce, or one run gets
        // shaped at the other's size (wrong advances/metrics).
        self.font_size_px.to_bits().hash(&mut hasher);
        self.font_features.hash(&mut hasher);

        // Spacing (affects glyph positions)
        self.letter_spacing.hash(&mut hasher);
        self.word_spacing.hash(&mut hasher);
        self.line_height.hash(&mut hasher);
        self.tab_size.to_bits().hash(&mut hasher);

        // Writing mode (affects layout direction)
        self.writing_mode.hash(&mut hasher);
        self.text_combine_upright.hash(&mut hasher);

        // Text transform (affects which characters are used)
        self.text_transform.hash(&mut hasher);

        // Font variants (affect glyph selection) and what may be faked where a face lacks them
        // (the small capitals are shaped as such)
        self.font_variant_caps.hash(&mut hasher);
        self.font_variant_numeric.hash(&mut hasher);
        self.font_variant_ligatures.hash(&mut hasher);
        self.font_variant_east_asian.hash(&mut hasher);
        self.font_synthesis.hash(&mut hasher);

        // An inline box's horizontal margin + border + padding move the pen
        // (`inline_offsets` in `position_one_line`): a span that gains one
        // must not reuse the old positions. No box and a box of zeros hash
        // alike.
        let (start, end) = self.border.as_ref().map_or((0.0_f32, 0.0_f32), |b| {
            (b.left_advance(), b.right_advance())
        });
        start.to_bits().hash(&mut hasher);
        end.to_bits().hash(&mut hasher);
        // So do the boxes around it, and which boxes they are.
        for edge in self.inline_boxes.iter() {
            edge.hash_bits(&mut hasher);
        }

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

/// The text's `font-variant-caps` (CSS Fonts 4 s6.6), the CSS value as it is.
/// The shaper turns on `smcp` / `c2sc` / `pcap` / `c2pc` / `unic` / `titl`,
/// and synthesizes small capitals for a face that has none
/// (`shaping::synthesized_small_caps`).
pub type FontVariantCaps = azul_css::props::basic::font::StyleFontVariantCaps;

/// The text's `font-variant-numeric` (CSS Fonts 4 s6.7) - the CSS value as
/// it is: one choice per group (figures, spacing, fractions) plus `ordinal`
/// and `slashed-zero`, ANY combination of them (`tabular-nums lining-nums
/// slashed-zero`), which a single keyword could not hold. The shaper turns on
/// [`StyleFontVariantNumeric::opentype_features`] (`lnum`, `tnum`, `zero`, ...).
///
/// [`StyleFontVariantNumeric::opentype_features`]: azul_css::props::basic::font::StyleFontVariantNumeric::opentype_features
pub type FontVariantNumeric = azul_css::props::basic::font::StyleFontVariantNumeric;

/// The text's `font-variant-ligatures` (CSS Fonts 4 s6.4) - the CSS value as it is: one choice
/// per group (common, discretionary, historical, contextual), any combination of them. The
/// shaper turns on [`StyleFontVariantLigatures::opentype_on`] (`dlig`, `hlig`, ...) and takes
/// [`StyleFontVariantLigatures::opentype_off`] (`liga` and `clig`, `calt`, ...) out of the
/// features a script turns on by default; `none` takes out all of them, never `rlig`.
///
/// [`StyleFontVariantLigatures::opentype_on`]: azul_css::props::basic::font_variation::StyleFontVariantLigatures::opentype_on
/// [`StyleFontVariantLigatures::opentype_off`]: azul_css::props::basic::font_variation::StyleFontVariantLigatures::opentype_off
pub type FontVariantLigatures = azul_css::props::basic::font_variation::StyleFontVariantLigatures;

/// The text's `font-variant-east-asian` (CSS Fonts 4 s6.8) - the CSS value as it is: one of the
/// JIS and the simplified / traditional forms, one of the two widths, and `ruby`, in any
/// combination the grammar allows. The shaper turns on
/// [`StyleFontVariantEastAsian::opentype_features`] (`jp78`, `fwid`, `ruby`, ...).
///
/// [`StyleFontVariantEastAsian::opentype_features`]: azul_css::props::basic::font_variation::StyleFontVariantEastAsian::opentype_features
pub type FontVariantEastAsian = azul_css::props::basic::font_variation::StyleFontVariantEastAsian;

/// What `font-synthesis` lets the engine fake where a font has no face or feature for what the text
/// asks (CSS Fonts 4 s2.5): the bold of a weight the family lacks, the slant of an italic it lacks,
/// the small capitals of a font without `smcp`, the super- and subscripts of a font without `sups`
/// and `subs`. Every switch is on (`auto`) unless a `font-synthesis*` property says `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)] // one switch per longhand of `font-synthesis`
pub struct FontSynthesis {
    /// `font-synthesis-weight`: a face without a bold cut is drawn thickened.
    pub weight: bool,
    /// `font-synthesis-style`: a family without an italic or oblique face is drawn slanted.
    pub style: bool,
    /// `font-synthesis-small-caps`: a font without small capitals gets them from scaled capitals.
    pub small_caps: bool,
    /// `font-synthesis-position`: a font without super- and subscript forms gets them from
    /// smaller, raised or lowered glyphs.
    pub position: bool,
}

impl Default for FontSynthesis {
    /// `font-synthesis: auto`, the initial value: everything may be faked.
    fn default() -> Self {
        Self {
            weight: true,
            style: true,
            small_caps: true,
            position: true,
        }
    }
}

/// A bidi embedding level (UAX 9): even levels run left to right, odd ones
/// right to left.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiLevel(u8);

impl BidiLevel {
    #[must_use]
    pub const fn new(level: u8) -> Self {
        Self(level)
    }
    /// The level of a paragraph that runs in `direction`: 0 or 1.
    #[must_use]
    pub const fn paragraph(direction: BidiDirection) -> Self {
        if direction.is_rtl() {
            Self(1)
        } else {
            Self(0)
        }
    }
    #[must_use]
    pub const fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    /// The direction text at this level runs in.
    #[must_use]
    pub const fn direction(&self) -> BidiDirection {
        if self.is_rtl() {
            BidiDirection::Rtl
        } else {
            BidiDirection::Ltr
        }
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
    pub tab_size: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub writing_mode: Option<WritingMode>,
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

        self.tab_size.map(f32::to_bits).hash(state);
        self.text_transform.hash(state);
        self.writing_mode.hash(state);
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
        self.tab_size.map(f32::to_bits) == other.tab_size.map(f32::to_bits) &&
        self.text_transform == other.text_transform &&
        self.writing_mode == other.writing_mode &&
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
        if let Some(val) = partial.tab_size {
            new_style.tab_size = val;
        }
        if let Some(val) = partial.text_transform {
            new_style.text_transform = val;
        }
        if let Some(val) = partial.writing_mode {
            new_style.writing_mode = val;
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

/// Guards of the 2026-11-21 audit, area A8 (`audit/A8.md`).
#[cfg(test)]
pub(super) mod audit_a8_tests {
    use super::*;

    std::thread_local! {
        /// `StyleProperties::layout_hash` calls on this thread.
        pub(crate) static LAYOUT_HASHES: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    }

    fn key(style: &StyleProperties) -> u64 {
        let mut hasher = DefaultHasher::new();
        style.hash(&mut hasher);
        hasher.finish()
    }

    /// `base` set in a font whose request has the variation setting `wght` = `value`.
    fn with_wght(base: &StyleProperties, value: f32) -> StyleProperties {
        StyleProperties {
            font_stack: FontStack::Stack(vec![FontSelector {
                request: FontRequest::default().with_setting(*b"wght", value),
                ..FontSelector::default()
            }]),
            ..base.clone()
        }
    }

    /// A8-16: the style's `Hash` (the text cache's first-stage key) tells
    /// apart every change of a field that shapes or lays text out: a size
    /// 0.3 px apart, `text-transform`, a font variant, a variation value
    /// 0.2 apart (in the request of the font stack) and `tab-size`. It rounded
    /// the size to whole px and left the others out, so their paragraphs were
    /// served each other's layout.
    /// Mutation: `self.font_size_px.round() as isize` again fails the first.
    #[test]
    fn the_style_key_sees_every_layout_field() {
        let base = StyleProperties {
            font_size_px: 21.0,
            ..StyleProperties::default()
        };
        let variants = [
            StyleProperties { font_size_px: 21.3, ..base.clone() },
            StyleProperties { text_transform: TextTransform::Uppercase, ..base.clone() },
            StyleProperties { font_variant_caps: FontVariantCaps::SmallCaps, ..base.clone() },
            with_wght(&base, 400.2),
            StyleProperties { tab_size: 4.0, ..base.clone() },
        ];
        for (i, other) in variants.iter().enumerate() {
            assert_ne!(key(&base), key(other), "variant {i} hashes like the base");
        }
        let a = with_wght(&base, 400.2);
        let b = with_wght(&base, 400.4);
        assert_ne!(key(&a), key(&b), "variation values 0.2 apart");
        assert_ne!(a.layout_hash(), b.layout_hash(), "the shaping key too");
        let c = StyleProperties { tab_size: 4.3, ..base.clone() };
        let d = StyleProperties { tab_size: 4.0, ..base.clone() };
        assert_ne!(c.layout_hash(), d.layout_hash(), "tab sizes 0.3 apart");
        assert_eq!(key(&base), key(&base.clone()), "equal styles hash alike");
    }
}
