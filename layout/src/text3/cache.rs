//! Core types and layout pipeline for the text/inline formatting context.
//!
//! This module defines the central data structures (`UnifiedConstraints`,
//! `LayoutCache`, `FontManager`, `UnifiedLayout`, etc.) and implements the
//! 5-stage inline layout pipeline:
//!
//! 1. **Logical Analysis** — `InlineContent` → `LogicalItem`
//! 2. **`BiDi` Reordering** — `LogicalItem` → `VisualItem`
//! 3. **Shaping** — `VisualItem` → `ShapedItem`
//! 4. **Text Orientation** — vertical writing-mode transforms
//! 5. **Flow / Positioning** — line breaking + final `PositionedItem` placement
//!
//! The module also contains cursor movement helpers, caching infrastructure
//! (per-item and monolithic), and font management (`FontContext`, `FontManager`,
//! `LoadedFonts`).  Integration with the box layout solver lives in
//! `solver3/fc.rs`.

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

pub use azul_core::selection::{ContentIndex, GraphemeClusterId};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage, props::basic::ColorU, props::style::StyleBackgroundContent,
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, FontId, PatternMatch, UnicodeRange};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;

// --- Named constants for layout heuristics ---

/// Fraction of line-height used as ascent when no font metrics are available.
/// Matches the typical 80/20 ascent/descent ratio found in Latin fonts.
const FALLBACK_ASCENT_RATIO: f32 = 0.8;
const FALLBACK_DESCENT_RATIO: f32 = 1.0 - FALLBACK_ASCENT_RATIO;

// Strut/metric fallbacks below assume the CSS-initial 16px font size when no
// explicit size is set.

/// Default strut ascent: `FALLBACK_ASCENT_RATIO` * (16px * `DEFAULT_LINE_HEIGHT_FACTOR`)
const DEFAULT_STRUT_ASCENT: f32 = 12.8;
/// Default strut descent: `FALLBACK_DESCENT_RATIO` * (16px * `DEFAULT_LINE_HEIGHT_FACTOR`)
const DEFAULT_STRUT_DESCENT: f32 = 3.2;

/// Default x-height approximation: 0.5 * 16px (CSS spec fallback).
const DEFAULT_X_HEIGHT: f32 = 8.0;
/// Default ch-width (advance of '0'): 0.5 * 16px.
const DEFAULT_CH_WIDTH: f32 = 8.0;

/// Approximate space character width as a fraction of `font_size`.
const SPACE_WIDTH_RATIO: f32 = 0.5;

/// CSS subscript baseline offset as fraction of line ascent (CSS Inline §3).
const SUBSCRIPT_OFFSET_RATIO: f32 = 0.3;
/// CSS superscript baseline offset as fraction of line ascent (CSS Inline §3).
const SUPERSCRIPT_OFFSET_RATIO: f32 = 0.4;

/// Ruby annotation font size relative to the base, per the CSS UA stylesheet
/// (`rt { font-size: 50% }`). Used to reserve placeholder width for the
/// annotation so a long annotation is not clipped by a short base.
const RUBY_ANNOTATION_FONT_SCALE: f32 = 0.5;

/// Computes the reserved box size for a ruby pair (CSS Ruby Layout §3): the inline-size is
/// the wider of the base and annotation runs (the narrower is centered over the wider), and
/// the block-size stacks the annotation line above the base line so the base reserves
/// vertical space for the annotation. Both inputs are REAL shaped advances / resolved line
/// heights — no magic per-character ratio.
fn ruby_reserved_box(
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

/// Glyph storage for a single shaped cluster.
///
/// Inline one glyph (the
/// common case for Latin text), spill to heap for ligatures / combining
/// marks / multi-glyph clusters. The `union` feature of smallvec packs
/// the inline buffer and the heap pointer into the same bytes, so sizeof
/// stays `sizeof(ShapedGlyph) + 2*usize` regardless of inline/heap state.
pub type ShapedGlyphVec = SmallVec<[ShapedGlyph; 1]>;

/// CSS `line-height` value.
///
/// `Normal` defers resolution to the point where font metrics are available,
/// computing `(ascent + |descent| + lineGap) / upem * fontSize`.
/// `Px` is an already-resolved pixel value from an explicit CSS declaration
/// (e.g. `line-height: 1.5` → `Px(fontSize * 1.5)`).
#[derive(Debug, Clone, Copy)]
#[derive(Default)]
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
    /// `font_size_px` and `units_per_em` are used to scale.
    #[must_use] pub fn resolve(&self, font_size_px: f32, ascent: f32, descent: f32, line_gap: f32, units_per_em: u16) -> f32 {
        match self {
            Self::Px(px) => *px,
            Self::Normal => {
                if units_per_em == 0 {
                    return font_size_px * 1.2; // fallback
                }
                let scale = font_size_px / f32::from(units_per_em);
                (ascent - descent + line_gap) * scale
            }
        }
    }

    /// Resolve using a `LayoutFontMetrics` struct for convenience.
    #[must_use] pub fn resolve_with_metrics(&self, font_size_px: f32, metrics: &LayoutFontMetrics) -> f32 {
        self.resolve(font_size_px, metrics.ascent, metrics.descent, metrics.line_gap, metrics.units_per_em)
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

// Stub type when hyphenation is disabled
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct Standard;

#[cfg(not(feature = "text_layout_hyphenation"))]
impl Standard {
    /// Stub hyphenate method that returns no breaks
    pub fn hyphenate<'a>(&'a self, _word: &'a str) -> StubHyphenationBreaks {
        StubHyphenationBreaks { breaks: Vec::new() }
    }
}

/// Result of hyphenation (stub when feature is disabled)
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct StubHyphenationBreaks {
    pub breaks: Vec<usize>,
}

// Always import Language from script module
use crate::text3::script::{script_to_language, Language, Script};

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
    #[must_use] pub const fn is_definite(&self) -> bool {
        matches!(self, Self::Definite(_))
    }

    /// Returns true if this is an indefinite (min-content or max-content) constraint
    #[must_use] pub const fn is_indefinite(&self) -> bool {
        !self.is_definite()
    }

    /// Returns the definite value if available, or a fallback for indefinite constraints
    #[must_use] pub const fn unwrap_or(self, fallback: f32) -> f32 {
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
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[must_use] pub fn to_f32_for_layout(self) -> f32 {
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
    #[must_use] pub fn from_f32(value: f32) -> Self {
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

// Re-export traits for backwards compatibility
pub use crate::font_traits::{ParsedFontTrait, ShallowClone};

// --- Core Data Structures for the New Architecture ---

/// Key for caching font chains - based only on CSS properties, not text content
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontChainKey {
    pub font_families: Vec<String>,
    pub weight: FcWeight,
    pub italic: bool,
    pub oblique: bool,
}

/// Either a `FontChainKey` (resolved via fontconfig) or a direct `FontRef` hash.
/// 
/// This enum cleanly separates:
/// - `Chain`: Fonts resolved through fontconfig with fallback support
/// - `Ref`: Direct `FontRef` that bypasses fontconfig entirely (e.g., embedded icon fonts)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontChainKeyOrRef {
    /// Regular font chain resolved via fontconfig
    Chain(FontChainKey),
    /// Direct `FontRef` identified by pointer address (covers entire Unicode range, no fallbacks)
    Ref(usize),
}

impl FontChainKeyOrRef {
    /// Create from a `FontStack` enum
    #[must_use] pub fn from_font_stack(font_stack: &FontStack) -> Self {
        match font_stack {
            FontStack::Stack(selectors) => Self::Chain(FontChainKey::from_selectors(selectors)),
            FontStack::Ref(font_ref) => Self::Ref(font_ref.parsed as usize),
        }
    }
    
    /// Returns true if this is a direct `FontRef`
    #[must_use] pub const fn is_ref(&self) -> bool {
        matches!(self, Self::Ref(_))
    }
    
    /// Returns the `FontRef` pointer if this is a Ref variant
    #[must_use] pub const fn as_ref_ptr(&self) -> Option<usize> {
        match self {
            Self::Ref(ptr) => Some(*ptr),
            Self::Chain(_) => None,
        }
    }
    
    /// Returns the `FontChainKey` if this is a Chain variant
    #[must_use] pub const fn as_chain(&self) -> Option<&FontChainKey> {
        match self {
            Self::Chain(key) => Some(key),
            Self::Ref(_) => None,
        }
    }
}

impl FontChainKey {
    /// Create a `FontChainKey` from a slice of font selectors
    #[must_use] pub fn from_selectors(font_stack: &[FontSelector]) -> Self {
        // (2026-06-10) FIRST-WINS DEDUP: cascaded font stacks can carry duplicate
        // families (e.g. [serif, sans-serif, serif, monospace] when the UA fallback
        // list is appended to a stack already naming serif). The pre-resolve
        // collector dedupes its stacks, so without deduping HERE the shaping-time
        // key never matched the stored key (the g121/g122 chain-lookup misses).
        // This is THE canonical FontChainKey constructor — every key-build site
        // must go through it so lookups match by construction.
        let mut font_families: Vec<String> = Vec::new();
        for sel in font_stack {
            if sel.family.is_empty() || font_families.contains(&sel.family) {
                continue;
            }
            font_families.push(sel.family.clone());
        }

        let font_families = if font_families.is_empty() {
            vec!["serif".to_string()]
        } else {
            font_families
        };

        let weight = font_stack
            .first()
            .map_or(FcWeight::Normal, |s| s.weight);
        let is_italic = font_stack
            .first()
            .is_some_and(|s| s.style == FontStyle::Italic);
        let is_oblique = font_stack
            .first()
            .is_some_and(|s| s.style == FontStyle::Oblique);

        Self {
            font_families,
            weight,
            italic: is_italic,
            oblique: is_oblique,
        }
    }
}

/// A map of pre-loaded fonts, keyed by `FontId` (from rust-fontconfig)
///
/// This is passed to the shaper - no font loading happens during shaping
/// The fonts are loaded BEFORE layout based on the font chains and text content.
///
/// Provides both `FontId` and hash-based lookup for efficient glyph operations.
#[derive(Debug, Clone)]
pub struct LoadedFonts<T> {
    /// Primary storage: `FontId` -> Font
    pub fonts: HashMap<FontId, T>,
    /// Reverse index: `font_hash` -> `FontId` for fast hash-based lookups
    hash_to_id: HashMap<u64, FontId>,
}

impl<T: ParsedFontTrait> LoadedFonts<T> {
    #[must_use] pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            hash_to_id: HashMap::new(),
        }
    }

    /// Insert a font with its `FontId`
    pub fn insert(&mut self, font_id: FontId, font: T) {
        let hash = font.get_hash();
        self.hash_to_id.insert(hash, font_id);
        self.fonts.insert(font_id, font);
    }

    /// Get a font by `FontId`
    #[must_use] pub fn get(&self, font_id: &FontId) -> Option<&T> {
        self.fonts.get(font_id)
    }

    /// Get a font by its hash
    #[must_use] pub fn get_by_hash(&self, hash: u64) -> Option<&T> {
        self.hash_to_id.get(&hash).and_then(|id| self.fonts.get(id))
    }

    /// Get the `FontId` for a hash
    #[must_use] pub fn get_font_id_by_hash(&self, hash: u64) -> Option<&FontId> {
        self.hash_to_id.get(&hash)
    }

    /// Check if a `FontId` is present
    #[must_use] pub fn contains_key(&self, font_id: &FontId) -> bool {
        self.fonts.contains_key(font_id)
    }

    /// Check if a hash is present
    #[must_use] pub fn contains_hash(&self, hash: u64) -> bool {
        self.hash_to_id.contains_key(&hash)
    }

    /// Iterate over all fonts
    pub fn iter(&self) -> impl Iterator<Item = (&FontId, &T)> {
        self.fonts.iter()
    }

    /// Get the number of loaded fonts
    #[must_use] pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if empty
    #[must_use] pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for LoadedFonts<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ParsedFontTrait> FromIterator<(FontId, T)> for LoadedFonts<T> {
    fn from_iter<I: IntoIterator<Item = (FontId, T)>>(iter: I) -> Self {
        let mut loaded = Self::new();
        for (id, font) in iter {
            loaded.insert(id, font);
        }
        loaded
    }
}

/// Enum that wraps either a fontconfig-resolved font (T) or a direct `FontRef`.
///
/// This allows the shaping code to handle both fontconfig-resolved fonts
/// and embedded fonts (`FontRef`) uniformly through the `ParsedFontTrait` interface.
#[derive(Debug, Clone)]
pub enum FontOrRef<T> {
    /// A font loaded via fontconfig
    Font(T),
    /// A direct `FontRef` (embedded font, bypasses fontconfig)
    Ref(azul_css::props::basic::FontRef),
}

impl<T: ParsedFontTrait> ShallowClone for FontOrRef<T> {
    fn shallow_clone(&self) -> Self {
        match self {
            Self::Font(f) => Self::Font(f.shallow_clone()),
            Self::Ref(r) => Self::Ref(r.clone()),
        }
    }
}

impl<T: ParsedFontTrait> ParsedFontTrait for FontOrRef<T> {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        match self {
            Self::Font(f) => f.shape_text(text, script, language, direction, style),
            Self::Ref(r) => r.shape_text(text, script, language, direction, style),
        }
    }

    fn get_hash(&self) -> u64 {
        match self {
            Self::Font(f) => f.get_hash(),
            Self::Ref(r) => r.get_hash(),
        }
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        match self {
            Self::Font(f) => f.get_glyph_size(glyph_id, font_size),
            Self::Ref(r) => r.get_glyph_size(glyph_id, font_size),
        }
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            Self::Font(f) => f.get_hyphen_glyph_and_advance(font_size),
            Self::Ref(r) => r.get_hyphen_glyph_and_advance(font_size),
        }
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            Self::Font(f) => f.get_kashida_glyph_and_advance(font_size),
            Self::Ref(r) => r.get_kashida_glyph_and_advance(font_size),
        }
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        match self {
            Self::Font(f) => f.has_glyph(codepoint),
            Self::Ref(r) => r.has_glyph(codepoint),
        }
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        match self {
            Self::Font(f) => f.get_vertical_metrics(glyph_id),
            Self::Ref(r) => r.get_vertical_metrics(glyph_id),
        }
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        match self {
            Self::Font(f) => f.get_font_metrics(),
            Self::Ref(r) => r.get_font_metrics(),
        }
    }

    fn num_glyphs(&self) -> u16 {
        match self {
            Self::Font(f) => f.num_glyphs(),
            Self::Ref(r) => r.num_glyphs(),
        }
    }

    fn get_space_width(&self) -> Option<usize> {
        match self {
            Self::Font(f) => f.get_space_width(),
            Self::Ref(r) => r.get_space_width(),
        }
    }
}

/// Bundles all font-related state that can be shared across layout passes.
///
/// Separates font concerns from layout/rendering state (`LayoutWindow`).
/// Each test/render creates a fresh `LayoutWindow` from a shared `FontContext`,
/// avoiding stale layout cache reuse while keeping parsed fonts warm.
///
/// Usage:
/// ```ignore
/// let ctx = FontContext::from_fc_cache(fc_cache);
/// ctx.pre_resolve_chains(&styled_dom, &platform);
/// ctx.load_fonts_for_chains();
///
/// // Per-test: create fresh LayoutWindow from context
/// let mut window = LayoutWindow::from_font_context(&ctx)?;
/// window.layout_and_generate_display_list(styled_dom, ...)?;
/// ```
#[derive(Debug, Clone)]
pub struct FontContext {
    /// The shared font cache. As of rust-fontconfig 4.1 this type is
    /// itself backed by `Arc<RwLock<_>>`, so cloning is cheap and all
    /// clones see builder-thread writes immediately — no more `Arc<T>`
    /// wrapping is needed and no more stale-snapshot refresh dance.
    pub fc_cache: FcFontCache,
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, azul_css::props::basic::FontRef>>>,
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    pub embedded_fonts: HashMap<u64, azul_css::props::basic::FontRef>,
    /// Reverse map: `font_family_hash` → actual `StyleFontFamilyVec`.
    /// Accumulated across DOMs for persistence. Copied to `FontManager` on `LayoutWindow` creation.
    pub font_hash_to_families: HashMap<u64, azul_css::props::basic::font::StyleFontFamilyVec>,
    /// Optional link back to the live `FcFontRegistry`. Present iff the
    /// caller wants the scout-on-demand path
    /// ([`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]),
    /// which priority-bumps the builder for not-yet-parsed families
    /// rather than falling back to the empty-snapshot response.
    pub registry: Option<Arc<rust_fontconfig::registry::FcFontRegistry>>,
}

impl FontContext {
    /// Create from an `FcFontCache`. Parsed fonts, font chains, and
    /// embedded fonts start empty.
    ///
    /// The resulting `FontContext` has `registry = None`, so font
    /// chain resolution only sees what's already in the cache. For
    /// the scout-on-demand path, use [`FontContext::from_registry`]
    /// instead, which keeps a handle to the registry so that chain
    /// resolution can lazy-parse families the DOM needs.
    #[must_use] pub fn from_fc_cache(fc_cache: FcFontCache) -> Self {
        Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: HashMap::new(),
            font_hash_to_families: HashMap::new(),
            registry: None,
        }
    }

    /// Create from a live `FcFontRegistry`. The `fc_cache` field gets
    /// a *shared* handle to the registry's cache (cheap `Arc::clone`
    /// on the v4.1 shared-state cache) — writes by builder threads
    /// show up immediately in every reader. Chain resolution goes
    /// through
    /// [`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]
    /// which priority-bumps the builder for unparsed families and
    /// waits for them. This is the "scout-on-demand" path: a
    /// headless renderer can skip the eager common-stack parse and
    /// pay only the per-family cost on first use, dropping peak RSS
    /// by the common-stack metadata size (~15 MiB on macOS).
    pub fn from_registry(
        registry: Arc<rust_fontconfig::registry::FcFontRegistry>,
    ) -> Self {
        let fc_cache = registry.shared_cache();
        Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: HashMap::new(),
            font_hash_to_families: HashMap::new(),
            registry: Some(registry),
        }
    }

    /// Pre-resolve font chains for a `StyledDom`'s CSS font stacks.
    /// Call this before layout so text rendering doesn't skip glyphs.
    ///
    /// Unicode-fallback fonts are limited to the scripts actually
    /// present in the document's text content — for an ASCII-only
    /// page, this skips the ~300 MiB Arial-Unicode / CJK / Arabic
    /// pull-in entirely. See
    /// [`crate::solver3::getters::scripts_present_in_styled_dom`].
    pub fn pre_resolve_chains_for_dom(
        &mut self,
        styled_dom: &azul_core::styled_dom::StyledDom,
        platform: &azul_css::system::Platform,
    ) {
        use crate::solver3::getters::{
            collect_font_stacks_from_styled_dom, collect_used_codepoints,
            prune_chain_to_used_chars, resolve_font_chains, scripts_present_in_styled_dom,
        };
        let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);
        let scripts = scripts_present_in_styled_dom(styled_dom);
        let mut chains = resolve_font_chains(&collected, &self.fc_cache, Some(&scripts));
        // Coverage-based prune (matches `collect_and_resolve_font_chains_with_registration`).
        let used_chars = collect_used_codepoints(styled_dom);
        for chain in chains.chains.values_mut() {
            prune_chain_to_used_chars(chain, &used_chars);
        }
        // WEB-LIFT last resort (after prune, so it survives — prune drops the registered
        // fallback because its cmap isn't parsed yet): if a chain ended up with no fonts,
        // append the first registered font so load_missing_for_chains finds it and text
        // shapes instead of measuring 0. (Done in azul-layout, NOT rust-fontconfig, so the
        // lift-fragile with_memory_fonts isn't re-codegen'd into a trapping shape.)
        for chain in chains.chains.values_mut() {
            let total = chain.css_fallbacks.iter().map(|g| g.fonts.len()).sum::<usize>()
                + chain.unicode_fallbacks.len();
            if total == 0 {
                if let Some((pattern, id)) = self.fc_cache.list().first() {
                    chain.unicode_fallbacks.push(rust_fontconfig::FontMatch {
                        id: *id,
                        unicode_ranges: pattern.unicode_ranges.clone(),
                        fallbacks: Vec::new(),
                    });
                }
            }
        }
        self.font_chain_cache = chains.into_fontconfig_chains();
    }

    /// Load parsed font bytes from disk for all fonts referenced in `font_chain_cache`.
    ///
    /// Thin wrapper that materialises a `ResolvedFontChains` from the
    /// cached chain map and delegates the actual disk-load to the
    /// shared `FontManager::load_missing_for_chains` helper, so the
    /// "collect → diff → load → insert" sequence lives in exactly
    /// one place. Failures are silently dropped here (the caller is
    /// the warmup path which has no good place to log them); use
    /// `FontManager::load_missing_for_chains` directly for diagnostics.
    pub fn load_fonts_for_chains(&self) {
        use crate::solver3::getters::ResolvedFontChains;
        use crate::text3::default::PathLoader;

        let chains_map: HashMap<FontChainKeyOrRef, _> = self
            .font_chain_cache
            .iter()
            .map(|(k, v)| (FontChainKeyOrRef::Chain(k.clone()), v.clone()))
            .collect();
        let resolved = ResolvedFontChains { chains: chains_map };

        // Borrow our shared `parsed_fonts` Arc as a transient
        // FontManager so we can use the helper. `from_arc_shared`
        // returns a manager that mutates the same underlying pool.
        let Ok(manager) = FontManager::<azul_css::props::basic::FontRef>::from_arc_shared(
            self.fc_cache.clone(),
            self.parsed_fonts.clone(),
        ) else {
            return;
        };
        let loader = PathLoader::new();
        let _failed = manager
            .load_missing_for_chains(&resolved, |bytes, idx| loader.load_font_shared(bytes, idx));
    }

    /// Convert into a `FontManager` with all data populated.
    /// Carries the `registry` forward so the resulting manager also
    /// has the scout-on-demand path available.
    #[must_use] pub fn to_font_manager(&self) -> FontManager<azul_css::props::basic::FontRef> {
        FontManager {
            fc_cache: self.fc_cache.clone(),
            parsed_fonts: self.parsed_fonts.clone(),
            font_chain_cache: self.font_chain_cache.clone(),
            embedded_fonts: Mutex::new(self.embedded_fonts.clone()),
            font_hash_to_families: self.font_hash_to_families.clone(),
            registry: self.registry.clone(),
            last_resolved_font_stacks_sig: None,
        }
    }
}

#[derive(Debug)]
pub struct FontManager<T> {
    /// The font-path cache. `FcFontCache` in rust-fontconfig 4.1 is
    /// already a shared handle internally (`Arc<RwLock<_>>`), so no
    /// further `Arc<...>` wrapping is needed — clones are cheap and
    /// all clones see builder writes instantly.
    pub fc_cache: FcFontCache,
    /// Holds the actual parsed font (usually with the font bytes attached).
    /// Wrapped in Arc so multiple `FontManager` instances can share the same
    /// pool of already-parsed fonts (avoids re-reading from disk).
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>,
    // Cache for font chains - populated by resolve_all_font_chains() before layout
    // This is read-only during layout - no locking needed for reads
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    /// Cache for direct `FontRefs` (embedded fonts like Material Icons)
    /// These are fonts referenced via `FontStack::Ref` that bypass fontconfig
    pub embedded_fonts: Mutex<HashMap<u64, azul_css::props::basic::FontRef>>,
    /// Reverse map: `font_family_hash` → actual `StyleFontFamilyVec`.
    /// Accumulated across DOMs. Used by font collection and text shaping to
    /// resolve compact cache hashes without `get_property_slow`.
    pub font_hash_to_families: HashMap<u64, azul_css::props::basic::font::StyleFontFamilyVec>,
    /// Optional link back to the live `FcFontRegistry`. When present,
    /// chain resolution uses
    /// [`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]
    /// which lazy-parses system fonts as the DOM requests them
    /// (scout-on-demand). `None` falls back to querying whatever is
    /// already in the shared cache.
    pub registry: Option<Arc<rust_fontconfig::registry::FcFontRegistry>>,
    /// `FxHash` of the `prev_font_hashes` slice at the moment the last
    /// successful `collect_and_resolve_font_chains_with_registration`
    /// call populated `font_chain_cache`. Lets repeated layouts of the
    /// same DOM skip the ~1.5 ms (cold) / ~0.9 ms (warm) chain resolver
    /// when the set of font-family hashes has not changed. Cleared
    /// whenever `font_chain_cache` is explicitly emptied.
    pub last_resolved_font_stacks_sig: Option<u64>,
}

impl<T: ParsedFontTrait> FontManager<T> {
    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cache cannot be initialized.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: Mutex::new(HashMap::new()),
            font_hash_to_families: HashMap::new(),
            registry: None,
            last_resolved_font_stacks_sig: None,
        })
    }

    /// Create a `FontManager` sharing the font-path cache handle.
    ///
    /// The `parsed_fonts` pool starts empty. Fonts loaded during the first
    /// layout pass are cached and will be available on subsequent calls
    /// if you clone the `parsed_fonts` Arc before creating the next instance.
    /// For full sharing, prefer `from_arc_shared()`.
    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cache cannot be initialized.
    pub fn from_shared(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: Mutex::new(HashMap::new()),
            font_hash_to_families: HashMap::new(),
            registry: None,
            last_resolved_font_stacks_sig: None,
        })
    }

    /// Create a `FontManager` sharing both the font-path cache and the
    /// already-parsed font data with another `FontManager`.
    ///
    /// This avoids re-reading and re-parsing font files from disk when
    /// rendering multiple documents that use the same fonts.
    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cache cannot be initialized.
    pub fn from_arc_shared(
        fc_cache: FcFontCache,
        parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>,
    ) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts,
            font_chain_cache: HashMap::new(),
            embedded_fonts: Mutex::new(HashMap::new()),
            font_hash_to_families: HashMap::new(),
            registry: None,
            last_resolved_font_stacks_sig: None,
        })
    }

    /// Attach a `FcFontRegistry` to this `FontManager` so subsequent
    /// chain-resolution calls use the on-demand path
    /// ([`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]).
    #[must_use]
    pub fn with_registry(
        mut self,
        registry: Arc<rust_fontconfig::registry::FcFontRegistry>,
    ) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Get a shareable handle to the parsed-font pool.
    ///
    /// Pass this to `from_arc_shared()` to create a new `FontManager` that
    /// reuses already-parsed fonts.
    pub fn shared_parsed_fonts(&self) -> Arc<Mutex<HashMap<FontId, T>>> {
        Arc::clone(&self.parsed_fonts)
    }

    /// Set the font chain cache from externally resolved chains
    ///
    /// This should be called with the result of `resolve_font_chains()` or
    /// `collect_and_resolve_font_chains()` from `solver3::getters`.
    pub fn set_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache = chains;
        self.last_resolved_font_stacks_sig = None;
    }

    /// Set the font chain cache and record the input signature so
    /// subsequent layouts with the same `prev_font_hashes` skip the
    /// resolver. Pass `sig = None` if the caller cannot compute a
    /// reliable signature — equivalent to the single-arg
    /// `set_font_chain_cache`.
    pub fn set_font_chain_cache_with_sig(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        sig: Option<u64>,
    ) {
        // (2026-06-10: reverted to HashMap — the empty-map RawIter hang behind the 2026-06-05
        // BTreeMap migration was the un-mirrored hashbrown EMPTY_GROUP static, fixed
        // transpiler-side.)
        self.font_chain_cache = chains;
        self.last_resolved_font_stacks_sig = sig;
    }

    /// Merge additional font chains into the existing cache
    ///
    /// Useful when processing multiple DOMs that may have different font requirements.
    pub fn merge_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache.extend(chains);
    }

    /// Get a reference to the font chain cache
    pub const fn get_font_chain_cache(
        &self,
    ) -> &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain> {
        &self.font_chain_cache
    }

    /// Get an embedded font by its hash (used for `WebRender` registration)
    /// Returns the `FontRef` if it exists in the `embedded_fonts` cache.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn get_embedded_font_by_hash(&self, font_hash: u64) -> Option<azul_css::props::basic::FontRef> {
        let embedded = self.embedded_fonts.lock().unwrap();
        embedded.get(&font_hash).cloned()
    }

    /// Get a parsed font by its hash (used for `WebRender` registration)
    /// Returns the parsed font if it exists in the `parsed_fonts` cache.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn get_font_by_hash(&self, font_hash: u64) -> Option<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // Linear search through all cached fonts to find one with matching hash
        let found = parsed
            .iter()
            .find(|(_, font)| font.get_hash() == font_hash)
            .map(|(_, font)| font.clone());
        drop(parsed);
        found
    }

    /// Register an embedded `FontRef` for later lookup by hash
    /// This is called when using `FontStack::Ref` during shaping
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn register_embedded_font(&self, font_ref: &azul_css::props::basic::FontRef) {
        let hash = font_ref.get_hash();
        let mut embedded = self.embedded_fonts.lock().unwrap();
        embedded.insert(hash, font_ref.clone());
    }

    /// Get a snapshot of all currently loaded fonts
    ///
    /// This returns a copy of all parsed fonts, which can be passed to the shaper.
    /// No locking is required after this call - the returned `HashMap` is independent.
    ///
    /// NOTE: This should be called AFTER loading all required fonts for a layout pass.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn get_loaded_fonts(&self) -> LoadedFonts<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed
            .iter()
            .map(|(id, font)| (*id, font.shallow_clone()))
            .collect()
    }

    /// Get the set of `FontIds` that are currently loaded
    ///
    /// This is useful for computing which fonts need to be loaded
    /// (diff with required fonts).
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn get_loaded_font_ids(&self) -> HashSet<FontId> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // M12.7: skip hashbrown's RawIterRange on an empty map — its NEON
        // control-byte group-scan mis-lifts to wasm and iterates forever
        // (the headless web layout uses an empty font cache → parsed is
        // empty here). is_empty() is len-based (no iteration), so it is safe.
        if parsed.is_empty() {
            return HashSet::new();
        }
        unsafe { crate::az_mark(0x60788, 0xA1) };
        let out = parsed.keys().copied().collect();
        drop(parsed);
        unsafe { crate::az_mark(0x6078C, 0xA2) };
        out
    }

    /// Insert a loaded font into the cache
    ///
    /// Returns the old font if one was already present for this `FontId`.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn insert_font(&self, font_id: FontId, font: T) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.insert(font_id, font)
    }

    /// Insert multiple loaded fonts into the cache
    ///
    /// This is more efficient than calling `insert_font` multiple times
    /// because it only acquires the lock once.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn insert_fonts(&self, fonts: impl IntoIterator<Item = (FontId, T)>) {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        for (font_id, font) in fonts {
            parsed.insert(font_id, font);
        }
    }

    /// One-shot helper that resolves "what fonts does `chains` need
    /// that this manager hasn't loaded yet" and loads them via the
    /// supplied `load_fn` closure (typically
    /// `PathLoader::load_font_shared` for the production lazy-decode
    /// path). Updates `parsed_fonts` in place and returns any failures
    /// for the caller to log.
    ///
    /// Replaces the same four-step `collect → compute_diff →
    /// load_from_disk → insert_fonts` dance previously inlined in
    /// `LayoutWindow::layout_document`, the CPU rasterizer pre-fill
    /// in `cpurender.rs`, and `FontContext::load_fonts_for_chains`.
    pub fn load_missing_for_chains<F>(
        &self,
        chains: &crate::solver3::getters::ResolvedFontChains,
        load_fn: F,
    ) -> Vec<(FontId, String)>
    where
        F: Fn(Arc<rust_fontconfig::FontBytes>, usize) -> Result<T, LayoutError>,
    {
        use crate::solver3::getters::{
            collect_font_ids_from_chains, compute_fonts_to_load, load_fonts_from_disk,
        };
        let required = collect_font_ids_from_chains(chains);
        let already = self.get_loaded_font_ids();
        let to_load = compute_fonts_to_load(&required, &already);
        if to_load.is_empty() {
            return Vec::new();
        }
        let result = load_fonts_from_disk(&to_load, &self.fc_cache, load_fn);
        self.insert_fonts(result.loaded);
        result.failed
    }

    /// Remove a font from the cache
    ///
    /// Returns the removed font if it was present.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn remove_font(&self, font_id: &FontId) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.remove(font_id)
    }
}

// Error handling
// [g119 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)): the String/FontSelector payloads give
// `Result<T, LayoutError>` (e.g. measure_intrinsic_widths' return + reorder/shape/orientation `?`)
// a POINTER-niche disc the web lift mis-reads → Ok→Err. Explicit u8 tag = simple-compare niche the
// lift handles. Also nested in solver3::LayoutError::Text (so both must be repr(C,u8)). Not FFI-exposed.
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
///   +spec:box-model:51342f - inline box margins/borders/padding do not affect line box height (default leading mode)
///   +spec:font-metrics:618776 - line-fit-edge (cap, ex, ideographic, alphabetic edge selection) not yet implemented
///
/// ## \u00a7 6 Trimming Leading (text-box-trim)
/// - \u274c NOT IMPLEMENTED: text-box-trim property
/// - \u274c NOT IMPLEMENTED: text-box-edge property
///   +spec:box-model:c09331 - text-box-trim trims block container first/last line to font metrics
///   // +spec:overflow:dc2196 - text-box-trim overflow handled as normal overflow (no special handling needed)
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
// +spec:box-model:415ef3 - initial letters use standard margin/padding/border box model; exclusion area = margin box
// +spec:box-model:d53ea3 - when block-start padding+border are zero, content edge coincides with over alignment point
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
    // +spec:writing-modes:6c5ab9 - blocks inherit base direction from parent via CSS direction property
    // Base direction from CSS, overrides auto-detection
    pub direction: Option<BidiDirection>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    // +spec:display-property:3bcac8 - inline boxes sized in block axis based on font metrics (ascent/descent)
    pub line_height: LineHeight,
    pub vertical_align: VerticalAlign,
    // block container's first available font, used for minimum line box height
    pub strut_ascent: f32,
    pub strut_descent: f32,
    // x-height of the strut font (scaled to font_size), for vertical-align: middle
    pub strut_x_height: f32,

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
            ch_width: DEFAULT_CH_WIDTH,
            overflow: OverflowBehavior::default(),
            segment_alignment: SegmentAlignment::default(),
            text_combine_upright: None,
            exclusion_margin: 0.0,
            hyphenation: Hyphens::default(),
            hyphenation_language: None,
            columns: 1,
            column_gap: 0.0,
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
    #[must_use] pub fn resolved_line_height(&self) -> f32 {
        let font_size_approx = self.strut_ascent + self.strut_descent;
        self.line_height.resolve(font_size_approx, 0.0, 0.0, 0.0, 0)
    }
    fn direction(&self, fallback: BidiDirection) -> BidiDirection {
        self.writing_mode.map_or(fallback, |s| s.get_direction().unwrap_or(fallback))
    }
    const fn is_vertical(&self) -> bool {
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
}

impl WritingMode {
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    const fn get_direction(&self) -> Option<BidiDirection> {
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

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone, Hash)]
pub struct StyledRun {
    pub text: String,
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
}

impl Default for FontSelector {
    fn default() -> Self {
        Self {
            family: "serif".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        }
    }
}

/// Font stack that can be either a list of font selectors (resolved via fontconfig)
/// or a direct `FontRef` (bypasses fontconfig entirely).
///
/// When a `FontRef` is used, it bypasses fontconfig resolution entirely
/// and uses the pre-parsed font data directly. This is used for embedded
/// fonts like Material Icons.
// [g121 az-web-lift] `#[repr(C, u8)]` — same disc-mis-lift guard as the other text3 enums; matched in
// shape_visual_items (`match &style.font_stack { Ref => shape, Stack => resolve }`). repr(Rust) niche
// (from the Vec/FontRef payloads) could mis-route. Explicit u8 tag = simple load. Internal to text3.
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
    #[must_use] pub const fn is_ref(&self) -> bool {
        matches!(self, Self::Ref(_))
    }

    /// Returns the `FontRef` if this is a Ref variant
    #[must_use] pub const fn as_ref(&self) -> Option<&azul_css::props::basic::font::FontRef> {
        match self {
            Self::Ref(r) => Some(r),
            Self::Stack(_) => None,
        }
    }

    /// Returns the font selectors if this is a Stack variant
    #[must_use] pub fn as_stack(&self) -> Option<&[FontSelector]> {
        match self {
            Self::Stack(s) => Some(s),
            Self::Ref(_) => None,
        }
    }

    /// Returns the first `FontSelector` if this is a Stack variant, None if Ref
    #[must_use] pub fn first_selector(&self) -> Option<&FontSelector> {
        match self {
            Self::Stack(s) => s.first(),
            Self::Ref(_) => None,
        }
    }

    /// Returns the first font family name (for Stack) or a placeholder (for Ref)
    #[must_use] pub fn first_family(&self) -> &str {
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
    #[must_use] pub const fn invalid() -> Self {
        Self { font_hash: 0 }
    }

    #[must_use] pub const fn from_hash(font_hash: u64) -> Self {
        Self { font_hash }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
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
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

// +spec:font-metrics:df51b1 - font metrics (ascent, descent, line_gap) used as baselines for inline layout alignment and box sizing
/// Layout-specific font metrics extracted from `FontMetrics`
/// Contains only the metrics needed for text layout and rendering
// +spec:box-model:a2f1c1 - inline box content area sized from first available font metrics (ascent/descent)
// +spec:font-metrics:9c2ca5 - ascent and descent metrics per font for inline layout
// +spec:font-metrics:797593 - font metrics (ascent, descent, line-gap) used for baseline calculations
// +spec:font-metrics:842d6a - font metrics (ascent, descent) used for precise spacing control
// +spec:font-metrics:eb97e0 - Font baseline metrics (ascent/descent) from font tables used for baseline alignment
// +spec:font-metrics:f2cd75 - em-over/em-under baselines intentionally not included (not used by CSS per spec)
// +spec:inline-formatting-context:76cd57 - ascent/descent font metrics for inline formatting context layout
// +spec:font-metrics:207e6b - ascent/descent metrics used for baseline calculations
#[derive(Copy, Debug, Clone)]
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
}

impl LayoutFontMetrics {
    // +spec:font-metrics:006bd8 - baseline position from font design coordinates, scaled with font size
    // +spec:font-metrics:910c0a - dominant-baseline: auto resolves to alphabetic for horizontal text
    // +spec:writing-modes:098958 - baseline is along the inline axis, used to align glyphs
    #[must_use] pub fn baseline_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / f32::from(self.units_per_em);
        self.ascent * scale
    }

    /// Returns the x-height scaled to the given font size in px.
    /// Falls back to 0.5em when the font doesn't provide sxHeight.
    #[must_use] pub fn x_height_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / f32::from(self.units_per_em);
        self.x_height.map_or(font_size * 0.5, |xh| xh * scale)
    }

    /// Returns the cap height scaled to the given font size in px.
    /// Falls back to ascent when the font doesn't provide sCapHeight.
    #[must_use] pub fn cap_height_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / f32::from(self.units_per_em);
        self.cap_height.unwrap_or(self.ascent) * scale
    }

    // +spec:line-height:471816 - line gap metric extracted from font for optional use when line-height is normal
    /// Convert from full `FontMetrics` to layout-specific metrics.
    ///
    // +spec:font-metrics:05193a - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:font-metrics:17a71c - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:font-metrics:62c659 - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:writing-modes:451a3e - ascent/descent/line-gap metrics: prefer OS/2, fallback HHEA, floor line_gap at 0
    /// Per CSS 2.2 §10.8.1: prefer OS/2 sTypoAscender/sTypoDescender,
    /// fall back to HHEA Ascent/Descent if OS/2 metrics are absent.
    // +spec:font-metrics:3dc8c1 - text-over/text-under baselines from font ascent/descent metrics
    // +spec:font-metrics:332c16 - text-over/text-under baseline metrics derived from font ascent/descent
    // +spec:font-metrics:9895e2 - baseline table is a font-level property; metrics apply uniformly to all glyphs
    // +spec:font-metrics:e05c40 - font ascent/descent metric extraction (text edge metrics)
    // +spec:font-metrics:21a3de - ascent/descent used as basis for em-over/em-under normalization
    // +spec:font-metrics:1257b7 - font ascent/descent ensure text fits within line box
    // +spec:table-layout:6bbd10 - use sTypoAscender/sTypoDescender as ascent/descent metrics per spec recommendation
    // +spec:font-metrics:5346d2 - prefer OS/2 sTypoAscender/sTypoDescender, fall back to HHEA
    // +spec:font-metrics:e16941 - line gap metric floored at zero per spec
    // +spec:font-metrics:a55c05 - metrics taken from font, synthesized if missing (prefers OS/2, falls back to HHEA)
    #[must_use] pub fn from_font_metrics(metrics: &azul_css::props::basic::FontMetrics) -> Self {
        let ascent = metrics.s_typo_ascender
            .as_option()
            .map_or_else(|| f32::from(metrics.ascender), |v| f32::from(*v));
        let descent = metrics.s_typo_descender
            .as_option()
            .map_or_else(|| f32::from(metrics.descender), |v| f32::from(*v));
        // UAs must floor the line gap metric at zero (css-inline-3 §3.2.2)
        // Spec: "UAs must floor the line gap metric at zero."
        let line_gap = metrics.s_typo_line_gap
            .as_option()
            .map_or_else(|| f32::from(metrics.line_gap), |v| f32::from(*v))
            .max(0.0);
        let x_height = metrics.sx_height
            .as_option()
            .map(|v| f32::from(*v));
        let cap_height = metrics.s_cap_height
            .as_option()
            .map(|v| f32::from(*v));
        Self {
            ascent,
            descent,
            line_gap,
            units_per_em: metrics.units_per_em,
            x_height,
            cap_height,
        }
    }

    // +spec:font-metrics:1eda6b - em-over is 0.5em over central baseline, em-under is 0.5em under
    /// Synthesize em-over baseline offset (in font units).
    /// Per CSS Inline 3 Appendix A.1: em-over = central baseline + 0.5em.
    /// Central baseline is synthesized as midpoint of ascent and descent.
    #[must_use] pub fn em_over(&self) -> f32 {
        let central = self.central_baseline();
        central + (f32::from(self.units_per_em) / 2.0)
    }

    /// Synthesize em-under baseline offset (in font units).
    /// Per CSS Inline 3 Appendix A.1: em-under = central baseline - 0.5em.
    #[must_use] pub fn em_under(&self) -> f32 {
        let central = self.central_baseline();
        central - (f32::from(self.units_per_em) / 2.0)
    }

    /// Synthesize central baseline (in font units).
    /// Midpoint between ascent and descent when not provided by the font.
    #[must_use] pub const fn central_baseline(&self) -> f32 {
        f32::midpoint(self.ascent, self.descent)
    }
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

// +spec:line-breaking:841a87 - hyphens property: manual (U+00AD/U+2010 only) and auto (language-aware automatic hyphenation)
// +spec:line-breaking:68c6ad - hyphens property controls hyphenation opportunities (none/manual/auto)
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

// +spec:line-breaking:ce5258 - white-space property controls collapsing, wrapping, and forced breaks
// +spec:line-breaking:35817b - normal/pre/nowrap/pre-wrap/break-spaces/pre-line behaviors
// +spec:white-space-processing:dec7aa - White space not removed/collapsed is "preserved white space"
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

// +spec:display-property:162c99 - Initial letter box: in-flow inline-level box with special layout behavior
// +spec:display-property:72a797 - Initial letter handled like inline-level content in originating line box
// initial-letter
// +spec:containing-block:46a499 - subsequent block must clear previous block's initial letter if it starts with its own initial letter, establishes independent FC, or specifies clear in initial letter's CB start direction
// +spec:font-metrics:1e5325 - drop initial cap-height = (N-1)*line_height + surrounding cap-height
// +spec:font-metrics:3aa518 - initial-letter-align: cap-height/ideographic/hanging/leading/border-box baseline alignment
// +spec:writing-modes:9698b0 - Han-derived scripts: initial letter extends from block-start to block-end of Nth line
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

// Path and shape definitions
#[derive(Copy, Debug, Clone, PartialOrd)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CurveTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    QuadTo {
        control: Point,
        end: Point,
    },
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
    Close,
}

// PathSegment
impl Hash for PathSegment {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the enum variant's discriminant first to distinguish them
        discriminant(self).hash(state);

        match self {
            Self::MoveTo(p) => p.hash(state),
            Self::LineTo(p) => p.hash(state),
            Self::CurveTo {
                control1,
                control2,
                end,
            } => {
                control1.hash(state);
                control2.hash(state);
                end.hash(state);
            }
            Self::QuadTo { control, end } => {
                control.hash(state);
                end.hash(state);
            }
            Self::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                center.hash(state);
                (radius.round() as isize).hash(state);
                (start_angle.round() as isize).hash(state);
                (end_angle.round() as isize).hash(state);
            }
            Self::Close => {} // No data to hash
        }
    }
}

impl PartialEq for PathSegment {
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MoveTo(a), Self::MoveTo(b)) => a == b,
            (Self::LineTo(a), Self::LineTo(b)) => a == b,
            (
                Self::CurveTo {
                    control1: c1a,
                    control2: c2a,
                    end: ea,
                },
                Self::CurveTo {
                    control1: c1b,
                    control2: c2b,
                    end: eb,
                },
            ) => c1a == c1b && c2a == c2b && ea == eb,
            (
                Self::QuadTo {
                    control: ca,
                    end: ea,
                },
                Self::QuadTo {
                    control: cb,
                    end: eb,
                },
            ) => ca == cb && ea == eb,
            (
                Self::Arc {
                    center: ca,
                    radius: ra,
                    start_angle: sa_a,
                    end_angle: ea_a,
                },
                Self::Arc {
                    center: cb,
                    radius: rb,
                    start_angle: sa_b,
                    end_angle: ea_b,
                },
            ) => ca == cb && round_eq(*ra, *rb) && round_eq(*sa_a, *sa_b) && round_eq(*ea_a, *ea_b),
            (Self::Close, Self::Close) => true,
            _ => false, // Variants are different
        }
    }
}

impl Eq for PathSegment {}

// Enhanced content model supporting mixed inline content
// [g117 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)): the web lift MIS-READS a repr(Rust)
// niche/compiler-placed discriminant — `<InlineContent as Clone>::clone` and create_logical_items'
// match both mis-route a Text(disc 0) to a Vec-bearing variant → clone reads a heap ptr as a Vec len
// → ~789MB alloc → OOB (g111/g115/g116 named stack = InlineContent::clone ← create_logical_items;
// content is CLEAN: len=1, ptr ok, disc-at-0=0). An explicit u8 tag at offset 0 (no niche) lowers to
// a simple load the lift handles correctly — the layout other (repr(C,u8)) enums use. Not FFI-exposed
// (internal to text3; only native shell code matches it), so the repr change is layout-safe.
#[derive(Debug, Clone, Hash)]
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
    fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.advance,
            height: self.style.line_height.resolve_with_metrics(self.style.font_size_px, &self.font_metrics),
        }
    }

    #[inline]
    const fn character_class(&self) -> CharacterClass {
        classify_character(self.codepoint as u32)
    }

    #[inline]
    fn is_whitespace(&self) -> bool {
        self.character_class() == CharacterClass::Space
    }

    #[inline]
    fn can_justify(&self) -> bool {
        !self.codepoint.is_whitespace() && self.character_class() != CharacterClass::Combining
    }

    #[inline]
    const fn justification_priority(&self) -> u8 {
        get_justification_priority(self.character_class())
    }

    #[inline]
    const fn break_opportunity_after(&self) -> bool {
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
                ImageSource::Url(_) => 1,
                ImageSource::Data(_) => 2,
                ImageSource::Svg(_) => 3,
                ImageSource::Placeholder(_) => 4,
            }
        }
        match (self, other) {
            (Self::Ref(a), Self::Ref(b)) => a.get_hash().cmp(&b.get_hash()),
            (Self::Url(a), Self::Url(b)) => a.cmp(b),
            (Self::Data(a), Self::Data(b)) => {
                (Arc::as_ptr(a).cast::<u8>() as usize).cmp(&(Arc::as_ptr(b).cast::<u8>() as usize))
            }
            (Self::Svg(a), Self::Svg(b)) => {
                (Arc::as_ptr(a).cast::<u8>() as usize).cmp(&(Arc::as_ptr(b).cast::<u8>() as usize))
            }
            (Self::Placeholder(a), Self::Placeholder(b)) => {
                (a.width.to_bits(), a.height.to_bits())
                    .cmp(&(b.width.to_bits(), b.height.to_bits()))
            }
            // Different variants: compare by variant index
            _ => variant_index(self).cmp(&variant_index(other)),
        }
    }
}

// +spec:font-metrics:fa104e - vertical-align values; baseline-source defaults to auto (first baseline)
// +spec:inline-formatting-context:340729 - alignment-baseline values for IFC baseline alignment (only baseline/top/bottom/middle implemented)
// CSS 2.2 §10.8.1 vertical-align property values
// +spec:display-property:0b1deb - inline boxes use dominant baseline to align text and inline-level children
// +spec:inline-formatting-context:3996a6 - dominant-baseline defaults to alphabetic in horizontal mode; vertical-align handles baseline alignment and super/sub shifting
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
    // +spec:font-metrics:152df3 - Raise (positive) or lower (negative) by this distance; 0 = baseline
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
        }
    }
}

impl InlineBorderInfo {
    /// Returns true if any border has a non-zero width
    #[must_use] pub fn has_border(&self) -> bool {
        self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0
    }

    /// Returns true if any border or padding is present
    #[must_use] pub fn has_chrome(&self) -> bool {
        self.has_border()
            || self.padding_top > 0.0
            || self.padding_right > 0.0
            || self.padding_bottom > 0.0
            || self.padding_left > 0.0
    }

    // +spec:box-model:da0ba2 - RTL bidi inline box split: left/right edges assigned to correct fragments
    // +spec:box-model:e9144f - visual-order margin/border/padding for inline boxes in bidi context
    // +spec:box-model:fac66f - Assigns margins/borders/padding in visual order for bidi inline fragments
    // +spec:box-model:720688 - LTR: left on first, right on last; RTL: right on first, left on last
    // +spec:positioning:1fcad6 - bidi-aware margin/border/padding on inline box fragments per visual order
    /// Total left inset (border + padding), suppressed at split points per §8.6.
    /// In LTR: left edge drawn on first fragment. In RTL: left edge drawn on last fragment.
    // +spec:box-model:bae97f - visual-order margin/border/padding assignment for bidi inline fragments
    #[must_use] pub fn left_inset(&self) -> f32 {
        let show = if self.is_rtl { self.is_last_fragment } else { self.is_first_fragment };
        if show { self.left + self.padding_left } else { 0.0 }
    }
    /// Total right inset (border + padding), suppressed at split points per §8.6.
    /// In LTR: right edge drawn on last fragment. In RTL: right edge drawn on first fragment.
    #[must_use] pub fn right_inset(&self) -> f32 {
        let show = if self.is_rtl { self.is_first_fragment } else { self.is_last_fragment };
        if show { self.right + self.padding_right } else { 0.0 }
    }
    /// Total top inset (border + padding)
    #[must_use] pub fn top_inset(&self) -> f32 { self.top + self.padding_top }
    /// Total bottom inset (border + padding)
    #[must_use] pub fn bottom_inset(&self) -> f32 { self.bottom + self.padding_bottom }
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

#[derive(Debug, Default, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PartialEq for Rect {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x)
            && round_eq(self.y, other.y)
            && round_eq(self.width, other.width)
            && round_eq(self.height, other.height)
    }
}
impl Eq for Rect {}

impl Hash for Rect {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The order in which you hash the fields matters.
        // A consistent order is crucial.
        (self.x.round() as isize).hash(state);
        (self.y.round() as isize).hash(state);
        (self.width.round() as isize).hash(state);
        (self.height.round() as isize).hash(state);
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl PartialOrd for Size {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Size {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn cmp(&self, other: &Self) -> Ordering {
        (self.width.round() as isize)
            .cmp(&(other.width.round() as isize))
            .then_with(|| (self.height.round() as isize).cmp(&(other.height.round() as isize)))
    }
}

// Size
impl Hash for Size {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width.round() as isize).hash(state);
        (self.height.round() as isize).hash(state);
    }
}
impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.width, other.width) && round_eq(self.height, other.height)
    }
}
impl Eq for Size {}

impl Size {
    #[must_use] pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[must_use] pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Point
impl Hash for Point {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.x.round() as isize).hash(state);
        (self.y.round() as isize).hash(state);
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x) && round_eq(self.y, other.y)
    }
}

impl Eq for Point {}

#[derive(Debug, Clone, PartialOrd)]
pub enum ShapeDefinition {
    Rectangle {
        size: Size,
        corner_radius: Option<f32>,
    },
    Circle {
        radius: f32,
    },
    Ellipse {
        radii: Size,
    },
    Polygon {
        points: Vec<Point>,
    },
    Path {
        segments: Vec<PathSegment>,
    },
}

// ShapeDefinition
impl Hash for ShapeDefinition {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Rectangle {
                size,
                corner_radius,
            } => {
                size.hash(state);
                corner_radius.map(|r| r.round() as isize).hash(state);
            }
            Self::Circle { radius } => {
                (radius.round() as isize).hash(state);
            }
            Self::Ellipse { radii } => {
                radii.hash(state);
            }
            Self::Polygon { points } => {
                // Since Point implements Hash, we can hash the Vec directly.
                points.hash(state);
            }
            Self::Path { segments } => {
                // Same for Vec<PathSegment>
                segments.hash(state);
            }
        }
    }
}

impl PartialEq for ShapeDefinition {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Rectangle {
                    size: s1,
                    corner_radius: r1,
                },
                Self::Rectangle {
                    size: s2,
                    corner_radius: r2,
                },
            ) => {
                s1 == s2
                    && match (r1, r2) {
                        (None, None) => true,
                        (Some(v1), Some(v2)) => round_eq(*v1, *v2),
                        _ => false,
                    }
            }
            (Self::Circle { radius: r1 }, Self::Circle { radius: r2 }) => {
                round_eq(*r1, *r2)
            }
            (Self::Ellipse { radii: r1 }, Self::Ellipse { radii: r2 }) => {
                r1 == r2
            }
            (Self::Polygon { points: p1 }, Self::Polygon { points: p2 }) => {
                p1 == p2
            }
            (Self::Path { segments: s1 }, Self::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeDefinition {}

impl ShapeDefinition {
    /// Calculates the bounding box size for the shape.
    #[must_use] pub fn get_size(&self) -> Size {
        match self {
            // The size is explicitly defined.
            Self::Rectangle { size, .. } => *size,

            // The bounding box of a circle is a square with sides equal to the diameter.
            Self::Circle { radius } => {
                let diameter = radius * 2.0;
                Size::new(diameter, diameter)
            }

            // The bounding box of an ellipse has width and height equal to twice its radii.
            Self::Ellipse { radii } => Size::new(radii.width * 2.0, radii.height * 2.0),

            // For a polygon, we must find the min/max coordinates to get the bounds.
            Self::Polygon { points } => calculate_bounding_box_size(points),

            // For a path, we find the bounding box of all its anchor and control points.
            //
            // NOTE: This is a common and fast approximation. The true bounding box of
            // bezier curves can be slightly smaller than the box containing their control
            // points. For pixel-perfect results, one would need to calculate the
            // curve's extrema.
            Self::Path { segments } => {
                let mut points = Vec::new();
                let mut current_pos = Point { x: 0.0, y: 0.0 };

                for segment in segments {
                    match segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                            points.push(*p);
                            current_pos = *p;
                        }
                        PathSegment::QuadTo { control, end } => {
                            points.push(current_pos);
                            points.push(*control);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::CurveTo {
                            control1,
                            control2,
                            end,
                        } => {
                            points.push(current_pos);
                            points.push(*control1);
                            points.push(*control2);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::Arc {
                            center,
                            radius,
                            start_angle,
                            end_angle,
                        } => {
                            // 1. Calculate and add the arc's start and end points to the list.
                            let start_point = Point {
                                x: center.x + radius * start_angle.cos(),
                                y: center.y + radius * start_angle.sin(),
                            };
                            let end_point = Point {
                                x: center.x + radius * end_angle.cos(),
                                y: center.y + radius * end_angle.sin(),
                            };
                            points.push(start_point);
                            points.push(end_point);

                            // 2. Normalize the angles to handle cases where the arc crosses the
                            //    0-radian line.
                            // This ensures we can iterate forward from a start to an end angle.
                            let mut normalized_end = *end_angle;
                            #[allow(clippy::while_float)] // intentional bounded float loop (angle-wrap / pixel-step); an integer counter would be artificial
                            while normalized_end < *start_angle {
                                normalized_end += 2.0 * std::f32::consts::PI;
                            }

                            // 3. Find the first cardinal point (multiples of PI/2) at or after the
                            //    start angle.
                            let mut check_angle = (*start_angle / std::f32::consts::FRAC_PI_2)
                                .ceil()
                                * std::f32::consts::FRAC_PI_2;

                            // 4. Iterate through all cardinal points that fall within the arc's
                            //    sweep and add them.
                            // These points define the maximum extent of the arc's bounding box.
                            #[allow(clippy::while_float)] // intentional bounded float loop (angle-wrap / pixel-step); an integer counter would be artificial
                            while check_angle < normalized_end {
                                points.push(Point {
                                    x: center.x + radius * check_angle.cos(),
                                    y: center.y + radius * check_angle.sin(),
                                });
                                check_angle += std::f32::consts::FRAC_PI_2;
                            }

                            // 5. The end of the arc is the new current position for subsequent path
                            //    segments.
                            current_pos = end_point;
                        }
                        PathSegment::Close => {
                            // No new points are added for closing the path
                        }
                    }
                }
                calculate_bounding_box_size(&points)
            }
        }
    }
}

// +spec:text-alignment-spacing:25e82a - text-align shorthand resolves text-align-all / text-align-last
/// Resolve effective text alignment for a line, handling text-align-last per CSS Text §6.3.
/// For the last line (or lines before forced breaks), text-align-last overrides text-align.
/// When text-align-last is auto (default), justify falls back to start; others use text-align.
// +spec:text-alignment-spacing:bca77d - text-align-last auto falls back to text-align-all, justify→start
// +spec:line-breaking:9b10d2 - text-align-last applies to last line and lines before forced breaks
/// +spec:text-alignment-spacing:8d88ce - text-align-last overrides justify on last line/forced break
pub(crate) fn resolve_effective_alignment(
    text_align: TextAlign,
    text_align_last: TextAlign,
    is_last_or_forced: bool,
) -> TextAlign {
    if is_last_or_forced {
        if text_align_last == TextAlign::default() {
            if text_align == TextAlign::Justify { TextAlign::Start } else { text_align }
        } else {
            text_align_last
        }
    } else {
        text_align
    }
}

/// Helper function to calculate the size of the bounding box enclosing a set of points.
fn calculate_bounding_box_size(points: &[Point]) -> Size {
    if points.is_empty() {
        return Size::zero();
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }

    // Handle case where points might be collinear or a single point
    if min_x > max_x || min_y > max_y {
        return Size::zero();
    }

    Size::new(max_x - min_x, max_y - min_y)
}

#[derive(Debug, Clone, PartialOrd)]
pub struct Stroke {
    pub color: ColorU,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Stroke
impl Hash for Stroke {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        (self.width.round() as isize).hash(state);

        // Manual hashing for Option<Vec<f32>>
        match &self.dash_pattern {
            None => 0u8.hash(state), // Hash a discriminant for None
            Some(pattern) => {
                1u8.hash(state); // Hash a discriminant for Some
                pattern.len().hash(state); // Hash the length
                for &val in pattern {
                    (val.round() as isize).hash(state); // Hash each rounded value
                }
            }
        }
    }
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        if self.color != other.color || !round_eq(self.width, other.width) {
            return false;
        }
        match (&self.dash_pattern, &other.dash_pattern) {
            (None, None) => true,
            (Some(p1), Some(p2)) => {
                p1.len() == p2.len() && p1.iter().zip(p2.iter()).all(|(a, b)| round_eq(*a, *b))
            }
            _ => false,
        }
    }
}

impl Eq for Stroke {}

// Helper function to round f32 for comparison
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
fn round_eq(a: f32, b: f32) -> bool {
    (a.round() as isize) == (b.round() as isize)
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

impl ShapeBoundary {
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn inflate(&self, margin: f32) -> Self {
        if margin == 0.0 {
            return self.clone();
        }
        match self {
            Self::Rectangle(rect) => Self::Rectangle(Rect {
                x: rect.x - margin,
                y: rect.y - margin,
                width: (rect.width + margin * 2.0).max(0.0),
                height: (rect.height + margin * 2.0).max(0.0),
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: *center,
                radius: radius + margin,
            },
            // For simplicity, Polygon and Path inflation is not implemented here.
            // A full implementation would require a geometry library to offset the path.
            _ => self.clone(),
        }
    }
}

// ShapeBoundary
impl Hash for ShapeBoundary {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Rectangle(rect) => rect.hash(state),
            Self::Circle { center, radius } => {
                center.hash(state);
                (radius.round() as isize).hash(state);
            }
            Self::Ellipse { center, radii } => {
                center.hash(state);
                radii.hash(state);
            }
            Self::Polygon { points } => points.hash(state),
            Self::Path { segments } => segments.hash(state),
        }
    }
}
impl PartialEq for ShapeBoundary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Rectangle(r1), Self::Rectangle(r2)) => r1 == r2,
            (
                Self::Circle {
                    center: c1,
                    radius: r1,
                },
                Self::Circle {
                    center: c2,
                    radius: r2,
                },
            ) => c1 == c2 && round_eq(*r1, *r2),
            (
                Self::Ellipse {
                    center: c1,
                    radii: r1,
                },
                Self::Ellipse {
                    center: c2,
                    radii: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (Self::Polygon { points: p1 }, Self::Polygon { points: p2 }) => {
                p1 == p2
            }
            (Self::Path { segments: s1 }, Self::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

impl ShapeBoundary {
    /// Converts a CSS shape (from azul-css) to a layout engine `ShapeBoundary`
    ///
    /// # Arguments
    /// * `css_shape` - The parsed CSS shape from azul-css
    /// * `reference_box` - The containing box for resolving coordinates (from layout solver)
    ///
    /// # Returns
    /// A `ShapeBoundary` ready for use in the text layout engine
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    pub fn from_css_shape(
        css_shape: &azul_css::shape::CssShape,
        reference_box: Rect,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self {
        use azul_css::shape::CssShape;

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Input CSS shape: {css_shape:?}"
            )));
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Reference box: {reference_box:?}"
            )));
        }

        let result = match css_shape {
            CssShape::Circle(circle) => {
                let center = Point {
                    x: reference_box.x + circle.center.x,
                    y: reference_box.y + circle.center.y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - CSS center: ({}, {}), radius: {}",
                        circle.center.x, circle.center.y, circle.radius
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - Absolute center: ({}, {}), \
                         radius: {}",
                        center.x, center.y, circle.radius
                    )));
                }
                Self::Circle {
                    center,
                    radius: circle.radius,
                }
            }

            CssShape::Ellipse(ellipse) => {
                let center = Point {
                    x: reference_box.x + ellipse.center.x,
                    y: reference_box.y + ellipse.center.y,
                };
                let radii = Size {
                    width: ellipse.radius_x,
                    height: ellipse.radius_y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Ellipse - center: ({}, {}), radii: ({}, \
                         {})",
                        center.x, center.y, radii.width, radii.height
                    )));
                }
                Self::Ellipse { center, radii }
            }

            CssShape::Polygon(polygon) => {
                let points = polygon
                    .points
                    .as_ref()
                    .iter()
                    .map(|pt| Point {
                        x: reference_box.x + pt.x,
                        y: reference_box.y + pt.y,
                    })
                    .collect();
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Polygon - {} points",
                        polygon.points.as_ref().len()
                    )));
                }
                Self::Polygon { points }
            }

            CssShape::Inset(inset) => {
                // Inset defines distances from reference box edges
                let x = reference_box.x + inset.inset_left;
                let y = reference_box.y + inset.inset_top;
                let width = reference_box.width - inset.inset_left - inset.inset_right;
                let height = reference_box.height - inset.inset_top - inset.inset_bottom;

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - insets: ({}, {}, {}, {})",
                        inset.inset_top, inset.inset_right, inset.inset_bottom, inset.inset_left
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - resulting rect: x={x}, y={y}, \
                         w={width}, h={height}"
                    )));
                }

                Self::Rectangle(Rect {
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                })
            }

            CssShape::Path(path) => {
                // CSS `path()` value: `path.data` is a raw SVG path `d=""` string in the
                // reference-box coordinate system (origin at the reference box's top-left).
                // Parse + flatten it into `Vec<PathSegment>` (curves sampled to line
                // segments) so the scanline code in `get_shape_horizontal_spans` can
                // intersect it per line, exactly like `polygon`.
                let segments = match azul_core::path_parser::parse_svg_path_d(path.data.as_str()) {
                    Ok(multipolygon) => {
                        flatten_svg_to_path_segments(&multipolygon, reference_box)
                    }
                    Err(_) => Vec::new(),
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Path - parsed {} flattened segments",
                        segments.len()
                    )));
                }
                if segments.is_empty() {
                    // Unparseable / empty path: fall back to the reference rectangle so a
                    // shape-inside container does not collapse to zero usable space.
                    ShapeBoundary::Rectangle(reference_box)
                } else {
                    ShapeBoundary::Path { segments }
                }
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Result: {result:?}"
            )));
        }
        result
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
    VerticalRl, // +spec:writing-modes:6e22a7 - vertical-rl (vertical right-to-left, commonly used in East Asia)
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

impl WritingMode {
    /// Necessary to determine if the glyphs are advancing in a horizontal direction
    #[must_use] pub const fn is_advance_horizontal(&self) -> bool {
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

// +spec:block-formatting-context:458d31 - vertical text orientation: upright for horizontal scripts, intrinsic for vertical scripts
// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, PartialOrd, Ord, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}


impl TextDecoration {
    /// Convert from CSS `StyleTextDecoration` enum to our internal representation.
    /// 
    /// Note: CSS text-decoration can have multiple values (underline line-through),
    /// but the current azul-css parser only supports single values. This can be
    /// extended in the future if CSS parsing is updated.
    #[must_use] pub fn from_css(css: azul_css::props::style::text::StyleTextDecoration) -> Self {
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
    Px(i32), // Use integer pixels to simplify hashing and equality
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
            Self::Em(val) => val.to_bits().hash(state),
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Self::Px(0)
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
        }
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
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // For f32 fields, round and cast to usize before hashing.
        (self.font_size_px.round() as isize).hash(state);
        self.line_height.hash(state);
    }
}

impl StyleProperties {
    /// Returns a hash that only includes properties that affect text layout.
    /// 
    /// Properties that DON'T affect layout (only rendering):
    /// - color, `background_color`, `background_content`
    /// - `text_decoration` (underline, etc.)
    /// - border (for inline elements)
    ///
    /// Properties that DO affect layout:
    /// - `font_stack`, `font_size_px`, `font_features`, `font_variations`
    /// - `letter_spacing`, `word_spacing`, `line_height`, `tab_size`
    /// - `writing_mode`, `text_orientation`, `text_combine_upright`
    /// - `text_transform`
    /// - `font_variant`_* (affects glyph selection)
    ///
    /// This allows the layout cache to reuse layouts when only rendering
    /// properties change (e.g., color changes on hover).
    // (family, weight, style) so that shaping runs break at element boundaries where font
    // properties differ, preventing impossible cross-boundary ligatures (e.g. "and" → "&").
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    #[must_use] pub fn layout_hash(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = DefaultHasher::new();

        // Font selection (affects shaping and metrics)
        self.font_stack.hash(&mut hasher);
        (self.font_size_px.round() as isize).hash(&mut hasher);
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
    #[must_use] pub fn layout_eq(&self, other: &Self) -> bool {
        self.layout_hash() == other.layout_hash()
    }
}

#[derive(Copy, Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub enum TextCombineUpright {
    None,
    All,        // Combine all characters in horizontal layout
    Digits(u8), // Combine up to N digits
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

// Bidi and script detection
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BidiDirection {
    Ltr,
    Rtl,
}

impl BidiDirection {
    #[must_use] pub const fn is_rtl(&self) -> bool {
        matches!(self, Self::Rtl)
    }
}

/// CSS `unicode-bidi` property values relevant to layout.
///
/// When `Plaintext`, the bidi algorithm uses P2/P3 heuristics to auto-detect
/// paragraph direction from text content, instead of the HL1 override from
/// the CSS `direction` property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Default)]
pub enum UnicodeBidi {
    #[default]
    Normal,
    Embed,
    Isolate,
    BidiOverride,
    IsolateOverride,
    Plaintext,
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

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantNumeric {
    #[default]
    Normal,
    LiningNums,
    OldstyleNums,
    ProportionalNums,
    TabularNums,
    DiagonalFractions,
    StackedFractions,
    Ordinal,
    SlashedZero,
}

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
    #[must_use] pub const fn new(level: u8) -> Self {
        Self(level)
    }
    #[must_use] pub const fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    #[must_use] pub const fn level(&self) -> u8 {
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
    fn apply_override(&self, partial: &PartialStyleProperties) -> Self {
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

/// The kind of a glyph, used to distinguish characters from layout-inserted items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphKind {
    /// A standard glyph representing one or more characters from the source text.
    Character,
    /// A hyphen glyph inserted by the line breaking algorithm.
    Hyphen,
    /// A `.notdef` glyph, indicating a character that could not be found in any font.
    NotDef,
    /// A Kashida justification glyph, inserted to stretch Arabic text.
    Kashida {
        /// The target width of the kashida.
        width: f32,
    },
}

// --- Stage 1: Logical Representation ---

// [g117 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)) — same disc-mis-lift class as InlineContent
// above. LogicalItem is matched in measure Stage-2 (`if let LogicalItem::Text`) + reorder_logical_items;
// a repr(Rust) niche disc mis-lifts on the web. Explicit u8 tag at offset 0 = a simple load the lift
// reads correctly. Internal to text3 (not FFI-exposed). LogicalItem::Object embeds InlineContent inline.
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item (often a single grapheme cluster).
        text: String,
        style: Arc<StyleProperties>,
        /// If this text is a list marker: whether it should be positioned outside
        /// (in the padding gutter) or inside (inline with content).
        /// None for non-marker content.
        marker_position_outside: Option<bool>,
        /// The DOM `NodeId` of the Text node this item originated from.
        /// None for generated content (list markers, `::before/::after`, etc.)
        source_node_id: Option<NodeId>,
    },
    // +spec:display-property:b1533f - text-combine-upright tate-chu-yoko horizontal-in-vertical composition
    /// Tate-chu-yoko: Run of text to be laid out horizontally within a vertical context.
    CombinedText {
        source: ContentIndex,
        text: String,
        style: Arc<StyleProperties>,
    },
    Ruby {
        source: ContentIndex,
        // For the stub, we simplify to strings. A full implementation
        // would need to handle Vec<LogicalItem> for both.
        base_text: String,
        ruby_text: String,
        style: Arc<StyleProperties>,
    },
    Object {
        /// A stable ID pointing back to the original source object.
        source: ContentIndex,
        /// The original non-text object.
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        style: Arc<StyleProperties>,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl Hash for LogicalItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Text {
                source,
                text,
                style,
                marker_position_outside,
                source_node_id,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
                marker_position_outside.hash(state);
                source_node_id.hash(state);
            }
            Self::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            Self::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                source.hash(state);
                base_text.hash(state);
                ruby_text.hash(state);
                style.as_ref().hash(state);
            }
            Self::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            Self::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            Self::Break { source, break_info } => {
                source.hash(state);
                break_info.hash(state);
            }
        }
    }
}

// --- Stage 2: Visual Representation ---

#[derive(Debug, Clone)]
pub struct VisualItem {
    /// A reference to the logical item this visual item originated from.
    /// A single `LogicalItem` can be split into multiple `VisualItems`.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
}

// --- Stage 3: Shaped Representation ---

// [g118 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)) — same disc-mis-lift class as InlineContent
// + LogicalItem (g117). ShapedItem is matched in measure Stage-5 (`match item { ShapedItem::Cluster ..}`)
// + cloned/matched throughout shaping; a repr(Rust) niche disc mis-lifts on the web. Explicit u8 tag at
// offset 0 = a simple load the lift reads correctly. Internal to text3 (not FFI-exposed).
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum ShapedItem {
    Cluster(ShapedCluster),
    /// A block of combined text (tate-chu-yoko) that is laid out
    // as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: ShapedGlyphVec,
        bounds: Rect,
        baseline_offset: f32,
    },
    Object {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
        // Store original object for rendering
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        bounds: Rect,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl ShapedItem {
    #[must_use] pub const fn as_cluster(&self) -> Option<&ShapedCluster> {
        match self {
            Self::Cluster(c) => Some(c),
            _ => None,
        }
    }
    /// Returns the bounding box of the item, relative to its own origin.
    ///
    /// The origin of the returned `Rect` is `(0,0)`, representing the top-left corner
    /// of the item's layout space before final positioning. The size represents the
    /// item's total advance (width in horizontal mode) and its line height (ascent + descent).
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[must_use] pub fn bounds(&self) -> Rect {
        match self {
            Self::Cluster(cluster) => {
                // The width of a text cluster is its total advance.
                let width = cluster.advance;

                // The height is the sum of its ascent and descent, which defines its line box.
                // We use the existing helper function which correctly calculates this from font
                // metrics.
                let (ascent, descent) = get_item_vertical_metrics_approx(self);
                let height = ascent + descent;

                Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                }
            }
            // For atomic inline items like objects, combined blocks, and tabs,
            // their bounds have already been calculated during the shaping or measurement phase.
            Self::CombinedBlock { bounds, .. } => *bounds,
            Self::Object { bounds, .. } => *bounds,
            Self::Tab { bounds, .. } => *bounds,

            // Breaks are control characters and have no visual geometry.
            Self::Break { .. } => Rect::default(), // A zero-sized rectangle.
        }
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone)]
pub struct ShapedCluster {
    /// The original text that this cluster was shaped from.
    /// This is crucial for correct hyphenation.
    pub text: String,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The DOM `NodeId` of the Text node this cluster originated from.
    /// None for generated content (list markers, `::before/::after`, etc.)
    pub source_node_id: Option<NodeId>,
    /// The glyphs that make up this cluster. `SmallVec<[T; 1]>` — inline
    /// single-glyph clusters (the common case for Latin text), spill to
    /// heap only for ligatures / combining marks.
    pub glyphs: ShapedGlyphVec,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: BidiDirection,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
    /// If this cluster is a list marker: whether it should be positioned outside
    /// (in the padding gutter) or inside (inline with content).
    /// None for non-marker content.
    pub marker_position_outside: Option<bool>,
    /// True if this is the first visual fragment of its inline box.
    /// Used for `box-decoration-break` and split inline border/padding.
    /// When an inline element wraps across lines, only the first fragment
    /// gets the start-edge border/padding.
    pub is_first_fragment: bool,
    /// True if this is the last visual fragment of its inline box.
    /// Only the last fragment gets the end-edge border/padding.
    pub is_last_fragment: bool,
}

/// A single, shaped glyph with its essential metrics.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text) - this is the BASE advance
    /// from the font metrics, WITHOUT kerning applied
    pub advance: f32,
    /// The kerning adjustment for this glyph (positive = more space, negative = less space)
    /// This is separate from advance so we can position glyphs absolutely
    pub kerning: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    pub style: Arc<StyleProperties>,
    /// Hash of the font - use `LoadedFonts` to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
}

impl ShapedGlyph {
    #[must_use] pub fn into_glyph_instance<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        let position = if writing_mode.is_advance_horizontal() {
            LogicalPosition {
                x: self.offset.x,
                y: self.offset.y,
            }
        } else {
            LogicalPosition {
                x: self.vertical_offset.x,
                y: self.vertical_offset.y,
            }
        };

        GlyphInstance {
            index: u32::from(self.glyph_id),
            point: position,
            size,
        }
    }

    /// Convert this `ShapedGlyph` into a `GlyphInstance` with an absolute position.
    /// This is used for display list generation where glyphs need their final page coordinates.
    #[must_use] pub fn into_glyph_instance_at<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        absolute_position: LogicalPosition,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        GlyphInstance {
            index: u32::from(self.glyph_id),
            point: absolute_position,
            size,
        }
    }

    /// Convert this `ShapedGlyph` into a `GlyphInstance` with an absolute position.
    /// This version doesn't require fonts - it uses a default size.
    /// Use this when you don't need precise glyph bounds (e.g., display list generation).
    #[must_use] pub fn into_glyph_instance_at_simple(
        &self,
        _writing_mode: WritingMode,
        absolute_position: LogicalPosition,
    ) -> GlyphInstance {
        // Use font metrics to estimate size, or default to zero
        // The actual rendering will use the font directly
        GlyphInstance {
            index: u32::from(self.glyph_id),
            point: absolute_position,
            size: LogicalSize::default(),
        }
    }
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone)]
pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: Point,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo,
}

impl UnifiedLayout {
    /// Calculate the bounding box of all positioned items.
    /// This is computed on-demand rather than cached.
    #[must_use] pub fn bounds(&self) -> Rect {
        if self.items.is_empty() {
            return Rect::default();
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for item in &self.items {
            let item_x = item.position.x;
            let item_y = item.position.y;

            // Get item dimensions
            let item_bounds = item.item.bounds();
            let item_width = item_bounds.width;
            let item_height = item_bounds.height;

            min_x = min_x.min(item_x);
            min_y = min_y.min(item_y);
            max_x = max_x.max(item_x + item_width);
            max_y = max_y.max(item_y + item_height);
        }

        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    #[must_use] pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    #[must_use] pub fn first_baseline(&self) -> Option<f32> {
        self.items
            .iter()
            .find_map(|item| get_baseline_for_item(&item.item))
    }

    #[must_use] pub fn last_baseline(&self) -> Option<f32> {
        self.items
            .iter()
            .rev()
            .find_map(|item| get_baseline_for_item(&item.item))
    }

    /// Takes a point relative to the layout's origin and returns the closest
    /// logical cursor position.
    ///
    /// This is the unified hit-testing implementation. The old `hit_test_to_cursor`
    /// method is deprecated in favor of this one.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn hittest_cursor(&self, point: LogicalPosition) -> Option<TextCursor> {
        if self.items.is_empty() {
            return None;
        }

        // Find the closest cluster vertically and horizontally
        let mut closest_item_idx = 0;
        let mut closest_distance = f32::MAX;

        for (idx, item) in self.items.iter().enumerate() {
            // Only consider cluster items for cursor placement
            if !matches!(item.item, ShapedItem::Cluster(_)) {
                continue;
            }

            let item_bounds = item.item.bounds();
            let item_center_y = item.position.y + item_bounds.height / 2.0;

            // Distance from click position to item center
            let vertical_distance = (point.y - item_center_y).abs();

            // For horizontal distance, check if we're within the cluster bounds
            let horizontal_distance = if point.x < item.position.x {
                item.position.x - point.x
            } else if point.x > item.position.x + item_bounds.width {
                point.x - (item.position.x + item_bounds.width)
            } else {
                0.0 // Inside the cluster horizontally
            };

            // Combined distance (prioritize vertical proximity)
            let distance = vertical_distance * 2.0 + horizontal_distance;

            if distance < closest_distance {
                closest_distance = distance;
                closest_item_idx = idx;
            }
        }

        // Get the closest cluster
        let closest_item = &self.items[closest_item_idx];
        let cluster = match &closest_item.item {
            ShapedItem::Cluster(c) => c,
            // Objects are treated as a single cluster for selection
            ShapedItem::Object { source, .. } | ShapedItem::CombinedBlock { source, .. } => {
                return Some(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: source.run_index,
                        start_byte_in_run: source.item_index,
                    },
                    affinity: if point.x
                        < closest_item.position.x + (closest_item.item.bounds().width / 2.0)
                    {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                });
            }
            _ => return None,
        };

        // Determine affinity based on which half of the cluster was clicked
        let cluster_mid_x = closest_item.position.x + cluster.advance / 2.0;
        let affinity = if point.x < cluster_mid_x {
            CursorAffinity::Leading
        } else {
            CursorAffinity::Trailing
        };

        Some(TextCursor {
            cluster_id: cluster.source_cluster_id,
            affinity,
        })
    }

    /// Given a logical selection range, returns a vector of visual rectangles
    /// that cover the selected text, in the layout's coordinate space.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use] pub fn get_selection_rects(&self, range: &SelectionRange) -> Vec<LogicalRect> {
        // 1. Build a map from the logical cluster ID to the visual PositionedItem for fast lookups.
        let mut cluster_map: HashMap<GraphemeClusterId, &PositionedItem> = HashMap::new();
        for item in &self.items {
            if let Some(cluster) = item.item.as_cluster() {
                cluster_map.insert(cluster.source_cluster_id, item);
            }
        }

        // 2. Normalize the range to ensure start always logically precedes end.
        let (start_cursor, end_cursor) = if range.start.cluster_id > range.end.cluster_id
            || (range.start.cluster_id == range.end.cluster_id
                && range.start.affinity > range.end.affinity)
        {
            (range.end, range.start)
        } else {
            (range.start, range.end)
        };

        // 3. Find the positioned items corresponding to the start and end of the selection.
        let Some(start_item) = cluster_map.get(&start_cursor.cluster_id) else {
            return Vec::new();
        };
        let Some(end_item) = cluster_map.get(&end_cursor.cluster_id) else {
            return Vec::new();
        };

        let mut rects = Vec::new();

        // Helper to get the absolute visual X coordinate of a cursor.
        let get_cursor_x = |item: &PositionedItem, affinity: CursorAffinity| -> f32 {
            match affinity {
                CursorAffinity::Leading => item.position.x,
                CursorAffinity::Trailing => item.position.x + get_item_measure(&item.item, false),
            }
        };

        // Helper to get the visual bounding box of all content on a specific line index.
        let get_line_bounds = |line_index: usize| -> Option<LogicalRect> {
            let items_on_line = self.items.iter().filter(|i| i.line_index == line_index);

            let mut min_x: Option<f32> = None;
            let mut max_x: Option<f32> = None;
            let mut min_y: Option<f32> = None;
            let mut max_y: Option<f32> = None;

            for item in items_on_line {
                // Skip items that don't take up space (like hard breaks)
                let item_bounds = item.item.bounds();
                if item_bounds.width <= 0.0 && item_bounds.height <= 0.0 {
                    continue;
                }

                let item_x_end = item.position.x + item_bounds.width;
                let item_y_end = item.position.y + item_bounds.height;

                min_x = Some(min_x.map_or(item.position.x, |mx| mx.min(item.position.x)));
                max_x = Some(max_x.map_or(item_x_end, |mx| mx.max(item_x_end)));
                min_y = Some(min_y.map_or(item.position.y, |my| my.min(item.position.y)));
                max_y = Some(max_y.map_or(item_y_end, |my| my.max(item_y_end)));
            }

            if let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) =
                (min_x, max_x, min_y, max_y)
            {
                Some(LogicalRect {
                    origin: LogicalPosition { x: min_x, y: min_y },
                    size: LogicalSize {
                        width: max_x - min_x,
                        height: max_y - min_y,
                    },
                })
            } else {
                None
            }
        };

        // 4. Handle single-line selection.
        if start_item.line_index == end_item.line_index {
            if let Some(line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let end_x = get_cursor_x(end_item, end_cursor.affinity);

                // Use min/max and abs to correctly handle selections made from right-to-left.
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: start_x.min(end_x),
                        y: line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: (end_x - start_x).abs(),
                        height: line_bounds.size.height,
                    },
                });
            }
        }
        // 5. Handle multi-line selection.
        else {
            // Rectangle for the start line (from cursor to end of line).
            if let Some(start_line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let line_end_x = start_line_bounds.origin.x + start_line_bounds.size.width;
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: start_x,
                        y: start_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: line_end_x - start_x,
                        height: start_line_bounds.size.height,
                    },
                });
            }

            // Rectangles for all full lines in between.
            for line_idx in (start_item.line_index + 1)..end_item.line_index {
                if let Some(line_bounds) = get_line_bounds(line_idx) {
                    rects.push(line_bounds);
                }
            }

            // Rectangle for the end line (from start of line to cursor).
            if let Some(end_line_bounds) = get_line_bounds(end_item.line_index) {
                let line_start_x = end_line_bounds.origin.x;
                let end_x = get_cursor_x(end_item, end_cursor.affinity);
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: line_start_x,
                        y: end_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: end_x - line_start_x,
                        height: end_line_bounds.size.height,
                    },
                });
            }
        }

        rects
    }

    /// Calculates the visual rectangle for a cursor at a given logical position.
    #[must_use] pub fn get_cursor_rect(&self, cursor: &TextCursor) -> Option<LogicalRect> {
        // Find the item and glyph corresponding to the cursor's cluster ID.
        let mut last_cluster: Option<(&PositionedItem, &ShapedCluster)> = None;
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                if cluster.source_cluster_id == cursor.cluster_id {
                    // Exact match
                    let line_height = item.item.bounds().height;
                    let cursor_x = match cursor.affinity {
                        CursorAffinity::Leading => item.position.x,
                        CursorAffinity::Trailing => item.position.x + cluster.advance,
                    };
                    return Some(LogicalRect {
                        origin: LogicalPosition {
                            x: cursor_x,
                            y: item.position.y,
                        },
                        size: LogicalSize {
                            width: 1.0,
                            height: line_height,
                        },
                    });
                }
                last_cluster = Some((item, cluster));
            }
        }
        // Cursor past end of text: position after the last cluster
        if let Some((item, cluster)) = last_cluster {
            if cursor.cluster_id.source_run == cluster.source_cluster_id.source_run
                && cursor.cluster_id.start_byte_in_run >= cluster.source_cluster_id.start_byte_in_run
            {
                let line_height = item.item.bounds().height;
                return Some(LogicalRect {
                    origin: LogicalPosition {
                        x: item.position.x + cluster.advance,
                        y: item.position.y,
                    },
                    size: LogicalSize {
                        width: 1.0,
                        height: line_height,
                    },
                });
            }
        }
        None
    }

    /// Get a cursor at the first cluster (leading edge) in the layout.
    #[must_use] pub fn get_first_cluster_cursor(&self) -> Option<TextCursor> {
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                });
            }
        }
        None
    }

    /// Get a cursor at the last cluster (trailing edge) in the layout.
    #[must_use] pub fn get_last_cluster_cursor(&self) -> Option<TextCursor> {
        for item in self.items.iter().rev() {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                });
            }
        }
        None
    }

    /// Moves a cursor one visual unit to the left, handling line wrapping and Bidi text.
    pub fn move_cursor_left(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        // Find current item
        let current_item_pos = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        });

        let Some(current_pos) = current_item_pos else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        // Skip the Trailing→Leading affinity flip for simple cursor movement.
        // Each left arrow press should move to the previous visible character position.

        // Move to previous cluster's trailing edge
        // Search backwards for a cluster on the same line, or any cluster if at line start
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at leading edge, current line {current_line}"
            ));
        }

        // First, try to find previous item on same line
        for i in (0..current_pos).rev() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == current_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_left: found previous cluster on same line, byte \
                             {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Trailing,
                    };
                }
            }
        }

        // If no previous item on same line, try to move to end of previous line
        if current_line > 0 {
            let prev_line = current_line - 1;
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: trying previous line {prev_line}"
                ));
            }
            for i in (0..current_pos).rev() {
                if let Some(cluster) = self.items[i].item.as_cluster() {
                    if self.items[i].line_index == prev_line {
                        if let Some(d) = debug {
                            d.push(format!(
                                "[Cursor] move_cursor_left: found cluster on previous line, byte \
                                 {}",
                                cluster.source_cluster_id.start_byte_in_run
                            ));
                        }
                        return TextCursor {
                            cluster_id: cluster.source_cluster_id,
                            affinity: CursorAffinity::Trailing,
                        };
                    }
                }
            }
        }

        // At start of text, can't move further
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at start of text, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor one visual unit to the right.
    pub fn move_cursor_right(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        // Find current item
        let current_item_pos = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        });

        let Some(current_pos) = current_item_pos else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_right: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        // Skip the Leading→Trailing affinity flip for simple cursor movement.
        // The affinity distinction matters for selection extension and bidi text,
        // but for basic left/right navigation, the user expects each press to move
        // the cursor to the next/previous visible character position.
        // If at Leading, go directly to the next cluster's Leading.

        // We're at leading or trailing edge, move to next cluster's leading edge
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at trailing edge, current line {current_line}"
            ));
        }

        // First, try to find next item on same line
        for i in (current_pos + 1)..self.items.len() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == current_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_right: found next cluster on same line, byte {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
        }

        // If no next item on same line, try to move to start of next line
        let next_line = current_line + 1;
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: trying next line {next_line}"
            ));
        }
        for i in (current_pos + 1)..self.items.len() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == next_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_right: found cluster on next line, byte {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
        }

        // At end of text, can't move further
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at end of text, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor up one line, attempting to preserve the horizontal column.
    pub fn move_cursor_up(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let target_line_idx = current_item.line_index.saturating_sub(1);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: already at top line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        // Find the Y coordinate of the middle of the target line
        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map_or(current_item.position.y, |i| i.position.y + (i.item.bounds().height / 2.0));

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: target line {target_line_idx}, hittesting at ({current_x}, {target_y})"
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: result byte {} (affinity {:?})",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor down one line, attempting to preserve the horizontal column.
    pub fn move_cursor_down(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let max_line = self.items.iter().map(|i| i.line_index).max().unwrap_or(0);
        let target_line_idx = (current_item.line_index + 1).min(max_line);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: already at bottom line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map_or(current_item.position.y, |i| i.position.y + (i.item.bounds().height / 2.0));

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: hit testing at ({current_x}, {target_y})"
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: result byte {}, affinity {:?}",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor to the visual start of its current line.
    pub fn move_cursor_to_line_start(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_start: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let first_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .min_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = first_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_start: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: no first item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor to the visual end of its current line.
    pub fn move_cursor_to_line_end(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_end: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let last_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .max_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = last_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_end: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: no last item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor one word to the left (Ctrl+Left / Option+Left).
    ///
    /// Word boundaries use the shared [`is_word_char`] predicate (alphanumeric or
    /// underscore are word characters; whitespace AND punctuation are boundaries),
    /// so this agrees with double-click word selection. The cursor moves past any
    /// boundary clusters to the left, then past word clusters until the next
    /// boundary or start of text.
    pub fn move_cursor_to_prev_word(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_prev_word: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_pos) = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            return cursor;
        };

        // Phase 1: Skip whitespace going left
        let mut pos = if cursor.affinity == CursorAffinity::Leading {
            // Already at leading edge, start from previous item
            current_pos.checked_sub(1)
        } else {
            // At trailing edge, start from current item
            Some(current_pos)
        };

        // Skip boundary clusters (whitespace + punctuation)
        while let Some(p) = pos {
            if let Some(cluster) = self.items[p].item.as_cluster() {
                if !cluster_is_word_boundary(cluster) {
                    break;
                }
            }
            pos = p.checked_sub(1);
        }

        // Phase 2: Skip word clusters going left (the word itself)
        while let Some(p) = pos {
            if let Some(cluster) = self.items[p].item.as_cluster() {
                if cluster_is_word_boundary(cluster) {
                    // We've reached a boundary before the word — stop at next cluster
                    if p + 1 < self.items.len() {
                        if let Some(c) = self.items[p + 1].item.as_cluster() {
                            return TextCursor {
                                cluster_id: c.source_cluster_id,
                                affinity: CursorAffinity::Leading,
                            };
                        }
                    }
                    break;
                }
            }
            if p == 0 {
                // Reached start of text — return first cluster
                if let Some(c) = self.items[0].item.as_cluster() {
                    return TextCursor {
                        cluster_id: c.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
                break;
            }
            pos = p.checked_sub(1);
        }

        // If we exhausted the search, go to first cluster
        if pos.is_none() {
            if let Some(first) = self.get_first_cluster_cursor() {
                return first;
            }
        }

        cursor
    }

    /// Moves a cursor one word to the right (Ctrl+Right / Option+Right).
    ///
    /// Word boundaries use the shared [`is_word_char`] predicate (alphanumeric or
    /// underscore are word characters; whitespace AND punctuation are boundaries),
    /// so this agrees with double-click word selection. The cursor moves past any
    /// word clusters, then past boundary clusters until the next word or end of text.
    pub fn move_cursor_to_next_word(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_next_word: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_pos) = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .is_some_and(|c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            return cursor;
        };

        let len = self.items.len();

        // Start position: if at leading edge, start from current; if trailing, start from next
        let start = if cursor.affinity == CursorAffinity::Trailing {
            current_pos + 1
        } else {
            current_pos
        };

        if start >= len {
            return cursor;
        }

        let mut pos = start;

        // Phase 1: Skip word clusters (current word)
        while pos < len {
            if let Some(cluster) = self.items[pos].item.as_cluster() {
                if cluster_is_word_boundary(cluster) {
                    break;
                }
            }
            pos += 1;
        }

        // Phase 2: Skip boundary clusters (whitespace + punctuation) after word
        while pos < len {
            if let Some(cluster) = self.items[pos].item.as_cluster() {
                if !cluster_is_word_boundary(cluster) {
                    // Found start of next word
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
            pos += 1;
        }

        // Reached end of text
        if let Some(last) = self.get_last_cluster_cursor() {
            return last;
        }

        cursor
    }
}

#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn get_baseline_for_item(item: &ShapedItem) -> Option<f32> {
    match item {
        ShapedItem::CombinedBlock {
            baseline_offset, ..
        } => Some(*baseline_offset),
        ShapedItem::Object {
            baseline_offset, ..
        } => Some(*baseline_offset),
        // We have to get the clusters font from the last glyph
        ShapedItem::Cluster(ref cluster) => {
            cluster.glyphs.last().map(|last_glyph| last_glyph
                        .font_metrics
                        .baseline_scaled(last_glyph.style.font_size_px))
        }
        ShapedItem::Break { source, break_info } => {
            // Breaks do not contribute to baseline
            None
        }
        ShapedItem::Tab { source, bounds } => {
            // Tabs do not contribute to baseline
            None
        }
    }
}

/// Stores information about content that exceeded the available layout space.
#[derive(Debug, Clone, Default)]
pub struct OverflowInfo {
    /// The items that did not fit within the constraints.
    ///
    /// Currently always empty: the positioners place every item (visual overflow
    /// is clipped at paint time) rather than dropping content, so nothing is ever
    /// recorded here. The `window.rs` incremental-patch guard reads
    /// `overflow_items.is_empty()` to stay future-proof against a positioning path
    /// that *does* drop items. TODO(superplan): populate this if such a path lands.
    pub overflow_items: Vec<ShapedItem>,
    /// The total bounds of all positioned content, including any that overflows
    /// the constraints. Populated by both positioners (greedy + Knuth-Plass) from
    /// [`UnifiedLayout::bounds`]; useful for `OverflowBehavior::Visible`/`Scroll`.
    pub unclipped_bounds: Rect,
}

impl OverflowInfo {
    #[must_use] pub const fn has_overflow(&self) -> bool {
        !self.overflow_items.is_empty()
    }
}

/// Intermediate structure carrying information from the line breaker to the positioner.
#[derive(Debug, Clone)]
pub struct UnifiedLine {
    pub items: Vec<ShapedItem>,
    /// The y-position (for horizontal) or x-position (for vertical) of the line's baseline.
    pub cross_axis_position: f32,
    /// The geometric segments this line must fit into.
    pub constraints: LineConstraints,
    pub is_last: bool,
}

// --- Caching Infrastructure ---

pub type CacheId = u64;

/// Defines a single area for layout, with its own shape and properties.
#[derive(Debug, Clone)]
pub struct LayoutFragment {
    /// A unique identifier for this fragment (e.g., "main-content", "sidebar").
    pub id: String,
    /// The geometric and style constraints for this specific fragment.
    pub constraints: UnifiedConstraints,
}

/// Represents the final layout distributed across multiple fragments.
#[derive(Debug, Clone)]
pub(crate) struct FlowLayout {
    /// A map from a fragment's unique ID to the layout it contains.
    pub(crate) fragment_layouts: HashMap<String, Arc<UnifiedLayout>>,
    /// Any items that did not fit into the last fragment in the flow chain.
    /// This is useful for pagination or determining if more layout space is needed.
    pub(crate) remaining_items: Vec<ShapedItem>,
}

/// Inline-axis intrinsic contributions derived from shaped text, without running
/// the line-breaking stage of the pipeline.
///
/// Callers that only need min/max-content widths for sizing (see
/// `calculate_ifc_root_intrinsic_sizes`) should prefer this over invoking
/// `layout_flow` twice with `AvailableSpace::MinContent`/`MaxContent`. The
/// latter runs the full flow loop — including `BreakCursor::peek_next_unit`,
/// which clones every `ShapedCluster` it inspects — even though no constraint
/// actually limits the line width.
#[derive(Copy, Debug, Clone, Default)]
pub struct IntrinsicTextSizes {
    /// CSS min-content = widest unbreakable unit (word) along the inline axis.
    pub min_content_width: f32,
    /// CSS max-content = sum of all advances along the inline axis (single line).
    pub max_content_width: f32,
    /// Height of a single line box: max(ascent + descent) across all items.
    pub max_content_height: f32,
}

/// Cached line break boundaries from a previous layout pass.
///
/// Enables incremental relayout: when a word changes width,
/// we can check if it still fits on the same line without
/// re-running the full line-breaking algorithm.
#[derive(Clone, Debug)]
pub struct CachedLineBreaks {
    /// Per-line: (`first_item_idx`, `last_item_idx_exclusive`) into positioned items.
    pub line_ranges: Vec<(usize, usize)>,
    /// Per-line total width (sum of item advances on that line).
    pub line_widths: Vec<f32>,
    /// The available width constraint used when these breaks were computed.
    pub available_width: f32,
}

/// Result of an incremental relayout attempt.
#[derive(Copy, Clone, Debug)]
pub enum IncrementalRelayoutResult {
    /// Glyphs changed but advance widths identical — swap in place, no repositioning.
    GlyphSwap,
    /// Width changed but still fits on same line — shift `x_offsets` of subsequent items.
    LineShift {
        /// Index of the first affected item.
        affected_item: usize,
        /// Width delta (`new_advance` - `old_advance`).
        delta: f32,
    },
    /// Line breaks changed — need to reflow from this line onward.
    PartialReflow {
        /// The line index from which to start reflowing.
        reflow_from_line: usize,
    },
    /// Cannot do incremental — fall back to full relayout.
    FullRelayout,
}

/// Extract line break boundaries from a positioned items list.
#[must_use] pub fn extract_line_breaks(
    items: &[PositionedItem],
    available_width: f32,
) -> CachedLineBreaks {
    let mut line_ranges = Vec::new();
    let mut line_widths = Vec::new();

    if items.is_empty() {
        return CachedLineBreaks { line_ranges, line_widths, available_width };
    }

    let mut line_start = 0usize;
    let mut current_line = items[0].line_index;
    let mut line_width = 0.0f32;

    for (i, item) in items.iter().enumerate() {
        if item.line_index != current_line {
            line_ranges.push((line_start, i));
            line_widths.push(line_width);
            line_start = i;
            current_line = item.line_index;
            line_width = 0.0;
        }
        line_width += get_item_measure(&item.item, false);
    }

    // Final line
    line_ranges.push((line_start, items.len()));
    line_widths.push(line_width);

    CachedLineBreaks { line_ranges, line_widths, available_width }
}

/// Attempt incremental relayout given old metrics and new per-item advance widths.
///
/// `dirty_item_indices`: which items in the shaped list changed.
/// `old_advances`: per-item advance widths from the previous layout.
/// `new_advances`: per-item advance widths after reshaping.
/// `line_breaks`: cached line boundaries from previous layout.
#[must_use] pub fn try_incremental_relayout(
    dirty_item_indices: &[usize],
    old_advances: &[f32],
    new_advances: &[f32],
    line_breaks: &CachedLineBreaks,
) -> IncrementalRelayoutResult {
    if dirty_item_indices.is_empty() {
        return IncrementalRelayoutResult::GlyphSwap;
    }

    // Check each dirty item
    for &dirty_idx in dirty_item_indices {
        if dirty_idx >= old_advances.len() || dirty_idx >= new_advances.len() {
            return IncrementalRelayoutResult::FullRelayout;
        }

        let old_adv = old_advances[dirty_idx];
        let new_adv = new_advances[dirty_idx];
        let delta = new_adv - old_adv;

        if delta.abs() < 0.001 {
            // Same width — just swap glyphs (GlyphSwap for this item)
            continue;
        }

        // Width changed — find which line this item is on
        let line_idx = line_breaks.line_ranges.iter()
            .position(|&(start, end)| dirty_idx >= start && dirty_idx < end);

        let Some(line_idx) = line_idx else {
            return IncrementalRelayoutResult::FullRelayout;
        };

        let old_line_width = line_breaks.line_widths[line_idx];
        let new_line_width = old_line_width + delta;

        if new_line_width <= line_breaks.available_width {
            // Still fits on same line — shift subsequent items
            return IncrementalRelayoutResult::LineShift {
                affected_item: dirty_idx,
                delta,
            };
        }
        // Overflows line — need to reflow from this line
        return IncrementalRelayoutResult::PartialReflow {
            reflow_from_line: line_idx,
        };
    }

    // All dirty items had same width
    IncrementalRelayoutResult::GlyphSwap
}

/// Cached shaped result for a single visual item (or coalesced group).
/// Enables per-item cache hits when only one word changes in a paragraph.
#[derive(Debug)]
pub(crate) struct PerItemShapedEntry {
    /// The shaped clusters for this single item/group.
    pub(crate) clusters: Vec<ShapedItem>,
    /// Sum of advance widths — for fast same-width detection during incremental relayout.
    pub(crate) total_advance: f32,
}

#[derive(Debug)]
pub struct TextShapingCache {
    // Stage 1 Cache: InlineContent -> LogicalItems
    logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,
    // Stage 2 Cache: LogicalItems -> VisualItems
    visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,
    // Stage 3 Cache: VisualItems -> ShapedItems (monolithic, for backward compat)
    shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem>>>,
    // Stage 3b Cache: Per-item/coalesce-group shaped results
    // Key: hash(text, bidi_level, script, style.layout_hash())
    per_item_shaped: HashMap<u64, Arc<PerItemShapedEntry>>,
    /// Tracks which `per_item_shaped` keys were accessed in the current generation.
    per_item_accessed: HashSet<u64>,
    /// Current generation counter, incremented each layout pass.
    generation: u64,
}

/// Approximate heap bytes retained by a [`TextShapingCache`].
#[derive(Copy, Debug, Clone, Default)]
pub struct TextCacheMemoryReport {
    pub logical_items_entries: usize,
    pub logical_items_bytes: usize,
    pub visual_items_entries: usize,
    pub visual_items_bytes: usize,
    pub shaped_items_entries: usize,
    pub shaped_items_bytes: usize,
    pub shaped_glyph_bytes: usize,
    pub shaped_cluster_text_bytes: usize,
    pub per_item_shaped_entries: usize,
    pub per_item_shaped_bytes: usize,
}

impl TextCacheMemoryReport {
    #[must_use] pub const fn total_bytes(&self) -> usize {
        self.logical_items_bytes
            + self.visual_items_bytes
            + self.shaped_items_bytes
            + self.shaped_glyph_bytes
            + self.shaped_cluster_text_bytes
            + self.per_item_shaped_bytes
    }
}

impl TextShapingCache {
    #[must_use] pub fn new() -> Self {
        Self {
            logical_items: HashMap::new(),
            visual_items: HashMap::new(),
            shaped_items: HashMap::new(),
            per_item_shaped: HashMap::new(),
            per_item_accessed: HashSet::new(),
            generation: 0,
        }
    }

    /// Approximate per-stage heap-byte breakdown.
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    #[must_use] pub fn memory_report(&self) -> TextCacheMemoryReport {
        let mut r = TextCacheMemoryReport::default();
        r.logical_items_entries = self.logical_items.len();
        for arc in self.logical_items.values() {
            r.logical_items_bytes += arc.capacity() * size_of::<LogicalItem>();
        }
        r.visual_items_entries = self.visual_items.len();
        for arc in self.visual_items.values() {
            r.visual_items_bytes += arc.capacity() * size_of::<VisualItem>();
        }
        r.shaped_items_entries = self.shaped_items.len();
        for arc in self.shaped_items.values() {
            r.shaped_items_bytes += arc.capacity() * size_of::<ShapedItem>();
            for item in arc.iter() {
                if let ShapedItem::Cluster(c) = item {
                    r.shaped_glyph_bytes += c.glyphs.capacity() * size_of::<ShapedGlyph>();
                    r.shaped_cluster_text_bytes += c.text.capacity();
                }
            }
        }
        r.per_item_shaped_entries = self.per_item_shaped.len();
        for arc in self.per_item_shaped.values() {
            r.per_item_shaped_bytes += arc.clusters.capacity() * size_of::<ShapedItem>();
            for item in &arc.clusters {
                if let ShapedItem::Cluster(c) = item {
                    r.per_item_shaped_bytes += c.glyphs.capacity() * size_of::<ShapedGlyph>();
                    r.per_item_shaped_bytes += c.text.capacity();
                }
            }
        }
        r
    }

    /// Call at the start of each layout pass. Evicts per-item shaped entries
    /// not accessed in the previous generation to prevent unbounded growth.
    pub fn begin_generation(&mut self) {
        if self.generation > 0 && !self.per_item_accessed.is_empty() {
            // Evict entries not accessed in this generation
            let accessed = &self.per_item_accessed;
            self.per_item_shaped.retain(|k, _| accessed.contains(k));
        }
        self.per_item_accessed.clear();
        self.generation += 1;
    }

    /// Check if we can reuse an old layout based on layout-affecting parameters.
    /// 
    /// This function compares only the parameters that affect glyph positions,
    /// not rendering-only parameters like color or text-decoration.
    /// 
    /// # Parameters
    /// - `old_constraints`: The constraints used for the cached layout
    /// - `new_constraints`: The constraints for the new layout request
    /// - `old_content`: The content used for the cached layout
    /// - `new_content`: The new content to layout
    /// 
    /// # Returns
    /// - `true` if the old layout can be reused (only rendering changed)
    /// - `false` if a new layout is needed (layout-affecting params changed)
    #[must_use] pub fn use_old_layout(
        old_constraints: &UnifiedConstraints,
        new_constraints: &UnifiedConstraints,
        old_content: &[InlineContent],
        new_content: &[InlineContent],
    ) -> bool {
        // First check: constraints must match exactly for layout purposes
        if old_constraints != new_constraints {
            return false;
        }
        
        // Second check: content length must match
        if old_content.len() != new_content.len() {
            return false;
        }
        
        // Third check: each content item must have same layout properties
        for (old, new) in old_content.iter().zip(new_content.iter()) {
            if !Self::inline_content_layout_eq(old, new) {
                return false;
            }
        }
        
        true
    }
    
    /// Compare two `InlineContent` items for layout equality.
    /// 
    /// Returns true if the layouts would be identical (only rendering differs).
    fn inline_content_layout_eq(old: &InlineContent, new: &InlineContent) -> bool {
        use InlineContent::{Text, Image, Space, LineBreak, Tab, Marker, Shape, Ruby};
        match (old, new) {
            (Text(old_run), Text(new_run)) => {
                // Text must match exactly, but style only needs layout_eq
                old_run.text == new_run.text 
                    && old_run.style.layout_eq(&new_run.style)
            }
            (Image(old_img), Image(new_img)) => {
                // Images: size affects layout, but not visual properties
                old_img.intrinsic_size == new_img.intrinsic_size
                    && old_img.display_size == new_img.display_size
                    && old_img.baseline_offset == new_img.baseline_offset
                    && old_img.alignment == new_img.alignment
            }
            (Space(old_sp), Space(new_sp)) => old_sp == new_sp,
            (LineBreak(old_br), LineBreak(new_br)) => old_br == new_br,
            (Tab { style: old_style }, Tab { style: new_style }) => old_style.layout_eq(new_style),
            (Marker { run: old_run, position_outside: old_pos },
             Marker { run: new_run, position_outside: new_pos }) => {
                old_pos == new_pos
                    && old_run.text == new_run.text
                    && old_run.style.layout_eq(&new_run.style)
            }
            (Shape(old_shape), Shape(new_shape)) => {
                // Shapes: shape_def affects layout, not fill/stroke
                old_shape.shape_def == new_shape.shape_def
                    && old_shape.baseline_offset == new_shape.baseline_offset
            }
            (Ruby { base: old_base, text: old_text, style: old_style },
             Ruby { base: new_base, text: new_text, style: new_style }) => {
                old_style.layout_eq(new_style)
                    && old_base.len() == new_base.len()
                    && old_text.len() == new_text.len()
                    && old_base.iter().zip(new_base.iter())
                        .all(|(o, n)| Self::inline_content_layout_eq(o, n))
                    && old_text.iter().zip(new_text.iter())
                        .all(|(o, n)| Self::inline_content_layout_eq(o, n))
            }
            // Different variants cannot have same layout
            _ => false,
        }
    }
}

impl Default for TextShapingCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Key for caching the conversion from `InlineContent` to `LogicalItem`s.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct LogicalItemsKey<'a> {
    pub(crate) inline_content_hash: u64,
    pub(crate) default_font_size: u32,
    pub(crate) _marker: std::marker::PhantomData<&'a ()>,
}

/// Key for caching the Bidi reordering stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct VisualItemsKey {
    pub(crate) logical_items_id: CacheId,
    pub(crate) base_direction: BidiDirection,
}

/// Key for caching the shaping stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct ShapedItemsKey {
    pub(crate) visual_items_id: CacheId,
    pub(crate) style_hash: u64,
}

impl ShapedItemsKey {
    pub(crate) fn new(visual_items_id: CacheId, visual_items: &[VisualItem]) -> Self {
        let style_hash = {
            let mut hasher = DefaultHasher::new();
            for item in visual_items {
                // Hash the style from the logical source, as this is what determines the font.
                match &item.logical_source {
                    LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                        style.as_ref().hash(&mut hasher);
                    }
                    _ => {}
                }
            }
            hasher.finish()
        };

        Self {
            visual_items_id,
            style_hash,
        }
    }
}

/// Key for the final layout stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct LayoutKey {
    pub(crate) shaped_items_id: CacheId,
    pub(crate) constraints: UnifiedConstraints,
}

/// Helper to create a `CacheId` from any `Hash`able type.
fn calculate_id<T: Hash>(item: &T) -> CacheId {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

// --- Main Layout Pipeline Implementation ---

impl TextShapingCache {
    /// New top-level entry point for flowing layout across multiple regions.
    ///
    /// This function orchestrates the entire layout pipeline, but instead of fitting
    /// content into a single set of constraints, it flows the content through an
    /// ordered sequence of `LayoutFragment`s.
    ///
    /// # CSS Inline Layout Module Level 3: Pipeline Implementation
    ///
    /// This implements the inline formatting context with 5 stages:
    ///
    /// ## Stage 1: Logical Analysis (`InlineContent` -> `LogicalItem`)
    /// \u2705 IMPLEMENTED: Parses raw content into logical units
    /// - Handles text runs, inline-blocks, replaced elements
    /// - Applies style overrides at character level
    /// - Implements \u00a7 2.2: Content size contribution calculation
    ///
    /// ## Stage 2: `BiDi` Reordering (`LogicalItem` -> `VisualItem`)
    /// \u2705 IMPLEMENTED: Uses CSS 'direction' property per CSS Writing Modes
    /// - Reorders items for right-to-left text (Arabic, Hebrew)
    /// - Respects containing block direction (not auto-detection)
    /// - Conforms to Unicode `BiDi` Algorithm (UAX #9)
    ///
    /// ## Stage 3: Shaping (`VisualItem` -> `ShapedItem`)
    /// \u2705 IMPLEMENTED: Converts text to glyphs
    /// - Uses `HarfBuzz` for OpenType shaping
    /// - Handles ligatures, kerning, contextual forms
    /// - Caches shaped results for performance
    ///
    /// ## Stage 4: Text Orientation Transformations
    /// \u26a0\ufe0f PARTIAL: Applies text-orientation for vertical text
    /// - Uses constraints from *first* fragment only
    /// - \u274c TODO: Should re-orient if fragments have different writing modes
    ///
    /// ## Stage 5: Flow Loop (`ShapedItem` -> `PositionedItem`)
    /// \u2705 IMPLEMENTED: Breaks lines and positions content
    /// - Calls `perform_fragment_layout` for each fragment
    /// - Uses `BreakCursor` to flow content across fragments
    /// - Implements \u00a7 5: Line breaking and hyphenation
    ///
    /// # Missing Features from CSS Inline-3:
    /// - \u00a7 3.3: initial-letter (drop caps)
    /// - \u00a7 4: vertical-align (only baseline supported)
    /// - \u00a7 6: text-box-trim (leading trim)
    /// - \u00a7 7: inline-sizing (aspect-ratio for inline-blocks)
    ///
    /// # Arguments
    /// * `content` - The raw `InlineContent` to be laid out.
    /// * `style_overrides` - Character-level style changes.
    /// * `flow_chain` - An ordered slice of `LayoutFragment` defining the regions (e.g., columns,
    ///   pages) that the content should flow through.
    /// * `font_chain_cache` - Pre-resolved font chains (from `FontManager.font_chain_cache`)
    /// * `fc_cache` - The fontconfig cache for font lookups
    /// * `loaded_fonts` - Pre-loaded fonts, keyed by `FontId`
    ///
    /// # Returns
    /// A `FlowLayout` struct containing the positioned items for each fragment that
    /// was filled, and any content that did not fit in the final fragment.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    /// # Panics
    ///
    /// Panics if bidi reordering of the logical items fails (an internal invariant).
    /// # Errors
    ///
    /// Returns a `LayoutError` if text flow layout fails.
    pub fn layout_flow<T: ParsedFontTrait>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        flow_chain: &[LayoutFragment],
        font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        fc_cache: &FcFontCache,
        loaded_fonts: &LoadedFonts<T>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<FlowLayout, LayoutError> {
        // [g150 az-web-lift DIAG] content data ptr (0x60BD0) + len (0x60BD4) at layout_flow ENTRY.
        #[cfg(feature = "web_lift")]
        unsafe {
            crate::az_mark((0x60BD0) as u32, (content.as_ptr() as usize as u32) as u32);
            crate::az_mark((0x60BD4) as u32, (content.len() as u32 | 0xC0DE0000) as u32);
        }
        // [g218 2026-06-09] The g158 `content.len()` force-materialize (a volatile read of content+16) is
        // DELETED: the within-fn SROA-to-0 of content.len() it worked around is now fixed (NEON-decoder +
        // volatile-guest-load transpiler work). VERIFIED: hello-world lays out without it — counter "5"
        // (label_wrapper 8,16,784,40) + button shape correctly, same rects as before. (The cross-FN Vec-*return*-
        // len mis-lift is a separate, still-present issue handled by the g127/g129/g130 out-param hacks — see
        // g134 marker: callee content.len=1 but the caller's return-read sees 0.)
        // --- Stages 1-3: Preparation ---
        // These stages are independent of the final geometry. We perform them once
        // on the entire content block before flowing. Caching is used at each stage.

        // Cap per-item shaped cache to prevent unbounded growth.
        // When threshold is exceeded, evict entries not accessed this generation.
        const PER_ITEM_CACHE_MAX: usize = 4096;
        if self.per_item_shaped.len() > PER_ITEM_CACHE_MAX {
            self.begin_generation();
        }

        // Stage 1: Logical Analysis (InlineContent -> LogicalItem)
        // [g213 2026-06-09] The web lift uses the real `self.logical_items` HashMap cache (NO bypass).
        // This entry() find-probe USED to spin forever on the lift (g178-g210 mis-diagnosed it many ways).
        // TRUE root cause: hashbrown's portable WIDTH=8 `Group::static_empty()` — `[0xFF; 8]` in libazul's
        // `__TEXT.__const` — was not mirrored into the wasm, so the empty-map ctrl-scan read 0x00, looked
        // ALL-FULL (EMPTY=0xFF), and the probe never terminated. FIXED entirely transpiler-side in
        // `dll/src/web/symbol_table.rs::compute_hashbrown_empty_group_ranges` (signature-scans `__const`
        // for >=8-byte 8-aligned 0xFF runs and mirrors them). Verified: web-nested-text lays out
        // ("Hello" at 8,16,800,20), __remill_error=0. No azul-source workaround needed here.
        let logical_items_id = calculate_id(&content);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| {
                Arc::new(create_logical_items(content, style_overrides, debug_messages))
            })
            .clone();

        // Get the first fragment's constraints to extract the CSS direction property.
        // This is used for BiDi reordering in Stage 2.
        let default_constraints = UnifiedConstraints::default();
        let first_constraints = flow_chain
            .first()
            .map_or(&default_constraints, |f| &f.constraints);

        // +spec:containing-block:e7a271 - paragraph embedding level set from containing block's 'direction' property
        // +spec:display-property:7665cb - inline boxes split into multiple visual runs due to bidi text processing
        // +spec:display-property:929d6b - applies Unicode bidi algorithm to inline-level box sequences
        // +spec:display-property:e8584a - Apply Unicode bidi algorithm to inline-level box sequences per CSS Writing Modes §2.4
        // Stage 2: Bidi Reordering (LogicalItem -> VisualItem)
        // +spec:containing-block:961e3c - bidi paragraph level from containing block direction, not UAX9 heuristic
        // +spec:writing-modes:0a5368 - unicode-bidi: plaintext auto-detects direction from text content
        // Per CSS Writing Modes §8.3: when unicode-bidi is plaintext, the paragraph's
        // base direction is determined from text content (first strong character), ignoring
        // the containing block's direction property. Empty paragraphs fall back to
        // the containing block's direction.
        let unicode_bidi_val = first_constraints.unicode_bidi;
        let base_direction = if unicode_bidi_val == UnicodeBidi::Plaintext {
            // Auto-detect from text content; fall back to containing block direction
            let has_strong = logical_items.iter().any(|item| {
                if let LogicalItem::Text { text, .. } = item {
                    matches!(unicode_bidi::get_base_direction(text.as_str()),
                        unicode_bidi::Direction::Ltr | unicode_bidi::Direction::Rtl)
                } else {
                    false
                }
            });
            if has_strong {
                get_base_direction_from_logical(&logical_items)
            } else {
                // Empty paragraph: use containing block's direction
                first_constraints.direction.unwrap_or(BidiDirection::Ltr)
            }
        } else {
            // Normal case: use CSS direction property
            first_constraints.direction.unwrap_or(BidiDirection::Ltr)
        };
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        // [g213] web lift uses the real visual_items HashMap cache (g180 bypass deleted; WIDTH=8
        // EMPTY_GROUP now mirrored — see Stage-1 note + symbol_table.rs).
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(
                    reorder_logical_items(&logical_items, base_direction, unicode_bidi_val, debug_messages).unwrap(),
                )
            })
            .clone();

        // Stage 3: Shaping (VisualItem -> ShapedItem)
        // Two-level cache: monolithic (fast path) + per-item (incremental path).
        let shaped_key = ShapedItemsKey::new(visual_items_id, &visual_items);
        let shaped_items_id = calculate_id(&shaped_key);
        // [g213] web lift uses the real shaped_items HashMap cache (g180 bypass deleted).
        let shaped_items = if let Some(cached) = self.shaped_items.get(&shaped_items_id) {
            // Monolithic cache hit — all visual items unchanged
            cached.clone()
        } else {
            // Monolithic miss — use per-item cache for incremental reshaping.
            // Items not in per-item cache are shaped; cached items are reused.
            let items = Arc::new(shape_visual_items_with_per_item_cache(
                &visual_items,
                &mut self.per_item_shaped,
                &mut self.per_item_accessed,
                font_chain_cache,
                fc_cache,
                loaded_fonts,
                debug_messages,
            )?);
            self.shaped_items.insert(shaped_items_id, items.clone());
            items
        };

        // --- Stage 4: Apply Vertical Text Transformations ---

        // Note: first_constraints was already extracted above for BiDi reordering (Stage 2).
        // This orients all text based on the constraints of the *first* fragment.
        // A more advanced system could defer orientation until inside the loop if
        // fragments can have different writing modes.
        let oriented_items = apply_text_orientation(shaped_items, first_constraints);

        // --- Stage 5: The Flow Loop ---
        let mut fragment_layouts = HashMap::new();
        // The cursor now manages the stream of items for the entire flow.
        // §5.2 word-break: pass word_break from constraints to cursor
        let mut cursor = BreakCursor::with_word_break(&oriented_items, first_constraints.word_break);
        cursor.hyphens = first_constraints.hyphenation;
        cursor.line_break = first_constraints.line_break;

        // [g147 az-web-lift] Hard safety bound on the Stage-5 flow loop. On the remill lift this
        // `for fragment in flow_chain` (or the `cursor.is_done()` break) mis-lifts for the NESTED IFC
        // and iterates without terminating → solveLayoutReal HANGS (fuel trap in layout_flow). The text
        // is fully laid out on the first iteration(s); cap the iterations so the loop always converges.
        // (native is unaffected — the cap is far above any real fragment count.)
        #[allow(clippy::no_effect_underscore_binding)] // web_lift-gated debug iteration counter
        let mut _az_flow_iters: usize = 0;
        for fragment in flow_chain {
            #[cfg(feature = "web_lift")]
            {
                _az_flow_iters += 1;
                unsafe { crate::az_mark((0x60BC0) as u32, (_az_flow_iters as u32 | 0xC0DE0000) as u32); }
                if _az_flow_iters > 256 {
                    break;
                }
            }
            // Perform layout for this single fragment, consuming items from the cursor.
            let fragment_layout = perform_fragment_layout(
                &mut cursor,
                &logical_items,
                &fragment.constraints,
                debug_messages,
                loaded_fonts,
            )?;

            fragment_layouts.insert(fragment.id.clone(), Arc::new(fragment_layout));
            if cursor.is_done() {
                break; // All content has been laid out.
            }
        }

        Ok(FlowLayout {
            fragment_layouts,
            remaining_items: cursor.drain_remaining(),
        })
    }

    /// Runs stages 1–4 of the layout pipeline (logical analysis, `BiDi`, shaping,
    /// text orientation) and derives min/max-content widths by scanning the
    /// resulting `ShapedItem`s directly — without running stage 5's line-breaking
    /// `BreakCursor` loop.
    ///
    /// Used by `calculate_ifc_root_intrinsic_sizes` to avoid the 24% CPU spent
    /// cloning `ShapedCluster`s inside `BreakCursor::peek_next_unit` on every
    /// sizing pass. Since stages 1–3 hit the same `per_item_shaped` cache as
    /// `layout_flow`, a subsequent `layout_flow` call for the same content at
    /// a real container width is a pure cache hit for the shaping work.
    ///
    /// The item walk uses the same break-opportunity predicate that the
    /// `BreakCursor` would — min-content accumulates advances between break
    /// opportunities and tracks the maximum; max-content is the sum of all
    /// advances (as if the flow were laid out on a single infinitely-wide line).
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    /// # Panics
    ///
    /// Panics if bidi reordering of the logical items fails (an internal invariant).
    /// # Errors
    ///
    /// Returns a `LayoutError` if measuring intrinsic widths fails.
    pub fn measure_intrinsic_widths<T: ParsedFontTrait>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        constraints: &UnifiedConstraints,
        font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        fc_cache: &FcFontCache,
        loaded_fonts: &LoadedFonts<T>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<IntrinsicTextSizes, LayoutError> {
        const PER_ITEM_CACHE_MAX: usize = 4096;
        if self.per_item_shaped.len() > PER_ITEM_CACHE_MAX {
            self.begin_generation();
        }

        // Stage 1: Logical Analysis (cached, same as layout_flow — the historic web-lift
        // bypass here was rooted in the un-mirrored hashbrown EMPTY_GROUP, fixed transpiler-side
        // in symbol_table.rs::compute_hashbrown_empty_group_ranges).
        let logical_items_id = calculate_id(&content);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| {
                Arc::new(create_logical_items(content, style_overrides, debug_messages))
            })
            .clone();

        // Stage 2: BiDi (same derivation as layout_flow)
        let unicode_bidi_val = constraints.unicode_bidi;
        let base_direction = if unicode_bidi_val == UnicodeBidi::Plaintext {
            let has_strong = logical_items.iter().any(|item| {
                if let LogicalItem::Text { text, .. } = item {
                    matches!(unicode_bidi::get_base_direction(text.as_str()),
                        unicode_bidi::Direction::Ltr | unicode_bidi::Direction::Rtl)
                } else {
                    false
                }
            });
            if has_strong {
                get_base_direction_from_logical(&logical_items)
            } else {
                constraints.direction.unwrap_or(BidiDirection::Ltr)
            }
        } else {
            constraints.direction.unwrap_or(BidiDirection::Ltr)
        };
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(
                    reorder_logical_items(&logical_items, base_direction, unicode_bidi_val, debug_messages).unwrap(),
                )
            })
            .clone();

        // Stage 3: Shaping (two-level cache, same as layout_flow)
        let shaped_key = ShapedItemsKey::new(visual_items_id, &visual_items);
        let shaped_items_id = calculate_id(&shaped_key);
        let shaped_items = if let Some(cached) = self.shaped_items.get(&shaped_items_id) { cached.clone() } else {
            let items = Arc::new(shape_visual_items_with_per_item_cache(
                &visual_items,
                &mut self.per_item_shaped,
                &mut self.per_item_accessed,
                font_chain_cache,
                fc_cache,
                loaded_fonts,
                debug_messages,
            )?);
            self.shaped_items.insert(shaped_items_id, items.clone());
            items
        };

        // Stage 4: Text orientation
        let oriented_items = apply_text_orientation(shaped_items, constraints);

        // Stage 5 bypass: scan items for min/max contributions.
        let word_break = constraints.word_break;
        let hyphens = constraints.hyphenation;

        let mut total = 0.0f32;
        let mut max_word = 0.0f32;
        let mut cur_word = 0.0f32;
        let mut max_line_height = 0.0f32;

        for item in oriented_items.iter() {
            // Must match get_item_measure() exactly: a cluster's inline advance
            // INCLUDES per-glyph kerning. Omitting kerning here under-measures
            // max-content, so a shrink-to-fit box (e.g. a flex item sized to its
            // text's max-content) ends up narrower than the kerned text the line
            // breaker lays out — the word then "overflows" its own box and, with
            // overflow-wrap:normal, gets force-broken to its first cluster
            // (the menubar "View" → "V" clip). Summing (advance + kerning) here,
            // in the same order as the breaker, makes the box exactly fit.
            let advance = match item {
                ShapedItem::Cluster(c) => {
                    let total_kerning: f32 = c.glyphs.iter().map(|g| g.kerning).sum();
                    c.advance + total_kerning
                }
                ShapedItem::CombinedBlock { bounds, .. }
                | ShapedItem::Object { bounds, .. }
                | ShapedItem::Tab { bounds, .. } => bounds.width,
                ShapedItem::Break { .. } => 0.0,
            };
            let adv = advance.max(0.0);
            total += adv;

            let (asc, desc) = get_item_vertical_metrics_approx(item);
            let h = (asc + desc).max(item.bounds().height);
            if h > max_line_height {
                max_line_height = h;
            }

            if is_break_opportunity_with_word_break(item, word_break, hyphens) {
                if cur_word > max_word {
                    max_word = cur_word;
                }
                cur_word = 0.0;
            } else {
                cur_word += adv;
            }
        }
        if cur_word > max_word {
            max_word = cur_word;
        }

        Ok(IntrinsicTextSizes {
            min_content_width: max_word,
            max_content_width: total,
            max_content_height: max_line_height,
        })
    }
}

// --- Stage 1 Implementation ---
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if the scan cursor advances past the end of `text` (an internal invariant).
pub fn create_logical_items(
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<LogicalItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering create_logical_items (Refactored) ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input content length: {}",
            content.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input overrides length: {}",
            style_overrides.len()
        )));
    }

    let mut items: Vec<LogicalItem> = Vec::new();
    let mut style_cache: HashMap<u64, Arc<StyleProperties>> = HashMap::new();

    // 1. Organize overrides for fast lookup per run.
    let mut run_overrides: HashMap<u32, HashMap<u32, &PartialStyleProperties>> = HashMap::new();
    for override_item in style_overrides {
        run_overrides
            .entry(override_item.target.run_index)
            .or_default()
            .insert(override_item.target.item_index, &override_item.style);
    }

    for (run_idx, inline_item) in content.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Processing content run #{run_idx}"
            )));
        }

        // Extract marker information if this is a marker
        let marker_position_outside = match inline_item {
            InlineContent::Marker {
                position_outside, ..
            } => Some(*position_outside),
            _ => None,
        };

        // [az-web-lift FIX 2026-06-06] Handle the common Text/Marker case via a STANDALONE `if let`
        // (a simple discriminant compare) instead of the first arm of the multi-way `match` below.
        // The remill lift mis-routes that multi-way InlineContent switch (LLVM's `subs/csel`-clamp
        // lowering): a Text(disc 0) variant lands in the `_`/Object arm → `inline_item.clone()` →
        // `<InlineContent as Clone>::clone` ALSO mis-routes to its Vec-clone arm → reads a heap ptr
        // as a Vec len → ×8 → ~789 MB alloc → BumpAlloc memset OOB. A standalone if-let lowers to a
        // single cmp/beq the lift handles correctly, so Text reaches its real body. Native unaffected.
        if let InlineContent::Text(run) | InlineContent::Marker { run, .. } = inline_item {
                let text = &run.text;
                if text.is_empty() {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(
                            "  Run is empty, skipping.".to_string(),
                        ));
                    }
                    continue;
                }
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!("  Run text: '{text}'")));
                }

                let current_run_overrides = run_overrides.get(&(run_idx as u32));
                let mut boundaries = BTreeSet::new();
                boundaries.insert(0);
                boundaries.insert(text.len());

                // --- Stateful Boundary Generation ---
                // web-lift FIX + perf: this scan_cursor walk ONLY inserts boundaries for
                // per-char style overrides (Rule 2) or text-combine-upright digit runs (Rule 1).
                // For plain text (no overrides AND no combine-upright) it inserts NOTHING and just
                // walks char-by-char via `scan_cursor += current_char.len_utf8()` — which the web
                // lift mis-advances (overshoot → slice_start_index_len_fail OOB; stall → infinite
                // loop). Skip the whole walk in that common case so `boundaries` stays {0, len}.
                let needs_scan = current_run_overrides.is_some()
                    || run.style.text_combine_upright.is_some();
                let mut scan_cursor = 0;
                while needs_scan && scan_cursor < text.len() {
                    let style_at_cursor = current_run_overrides.and_then(|o| o.get(&(scan_cursor as u32))).map_or_else(|| (*run.style).clone(), |partial| run.style.apply_override(partial));

                    let current_char = text[scan_cursor..].chars().next().unwrap();

                    // +spec:containing-block:e4d9de - text-combine-upright digit run rules: digits sharing an ancestor with same value form one sequence across box boundaries
                    // +spec:inline-formatting-context:f65029 - text-combine-upright text run rules: combine consecutive digits not interrupted by box boundary
                    // Rule 1: Multi-character features take precedence.
                    // +spec:containing-block:9a26bd - text-combine-upright digit runs scoped by ancestor style boundaries
                    if let Some(TextCombineUpright::Digits(max_digits)) =
                        style_at_cursor.text_combine_upright
                    {
                        if max_digits > 0 && current_char.is_ascii_digit() {
                            let digit_chunk: String = text[scan_cursor..]
                                .chars()
                                .take(max_digits as usize)
                                .take_while(char::is_ascii_digit)
                                .collect();

                            let end_of_chunk = scan_cursor + digit_chunk.len();
                            boundaries.insert(scan_cursor);
                            boundaries.insert(end_of_chunk);
                            scan_cursor = end_of_chunk; // Jump past the entire sequence
                            continue;
                        }
                    }

                    // Rule 2: If no multi-char feature, check for a normal single-grapheme
                    // override.
                    if current_run_overrides
                        .and_then(|o| o.get(&(scan_cursor as u32)))
                        .is_some()
                    {
                        let grapheme_len = text[scan_cursor..]
                            .graphemes(true)
                            .next()
                            .unwrap_or("")
                            .len();
                        boundaries.insert(scan_cursor);
                        boundaries.insert(scan_cursor + grapheme_len);
                        scan_cursor += grapheme_len;
                        continue;
                    }

                    // Rule 3: No special features or overrides at this point, just advance one
                    // char.
                    scan_cursor += current_char.len_utf8();
                }

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  Boundaries: {boundaries:?}"
                    )));
                }

                // --- Chunk Processing ---
                for (start, end) in boundaries.iter().zip(boundaries.iter().skip(1)) {
                    let (start, end) = (*start, *end);
                    if start >= end {
                        continue;
                    }

                    let text_slice = &text[start..end];
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Processing chunk from {start} to {end}: '{text_slice}'"
                        )));
                    }

                    let style_to_use = current_run_overrides.and_then(|o| o.get(&(start as u32))).map_or_else(|| run.style.clone(), |partial_style| {
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "  -> Applying override at byte {start}"
                            )));
                        }
                        let mut hasher = DefaultHasher::new();
                        Arc::as_ptr(&run.style).hash(&mut hasher);
                        partial_style.hash(&mut hasher);
                        style_cache
                            .entry(hasher.finish())
                            .or_insert_with(|| Arc::new(run.style.apply_override(partial_style)))
                            .clone()
                    });

                    // +spec:block-formatting-context:9e7c79 - text-combine-upright combines multiple characters into 1em in vertical writing
                    // +spec:containing-block:2b399b - text-combine-upright digits: combine ASCII digit sequences within max_digits limit; box boundaries implicitly prevent cross-box combination
                    // +spec:display-contents:644c78 - text-combine-upright run boundary check:
                    // if a combinable run boundary is due only to inline box boundaries,
                    // and adjacent chars would form a longer combinable sequence, do not combine
                    // +spec:white-space-processing:409d90 - text-combine-upright combined text: white space at start/end processed as in inline-block
                    let is_combinable_chunk = match &style_to_use.text_combine_upright {
                        Some(TextCombineUpright::All) => !text_slice.is_empty(),
                        Some(TextCombineUpright::Digits(max_digits)) => {
                            *max_digits > 0
                                && !text_slice.is_empty()
                                && text_slice.chars().all(|c| c.is_ascii_digit())
                                && text_slice.chars().count() <= *max_digits as usize
                        }
                        _ => false,
                    };

                    if is_combinable_chunk {
                        // Trim leading/trailing white space like an inline-block
                        let trimmed = text_slice.trim();
                        let combined_text = if trimmed.is_empty() {
                            text_slice.to_string()
                        } else {
                            trimmed.to_string()
                        };
                        items.push(LogicalItem::CombinedText {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: combined_text,
                            style: style_to_use,
                        });
                    } else {
                        items.push(LogicalItem::Text {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: text_slice.to_string(),
                            style: style_to_use,
                            marker_position_outside,
                            source_node_id: run.source_node_id,
                        });
                    }
                }
        } else {
            match inline_item {
            // line breaking class characters must be treated as forced line breaks
            InlineContent::LineBreak(break_info) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  LineBreak: {break_info:?}"
                    )));
                }
                items.push(LogicalItem::Break {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    break_info: *break_info,
                });
            }
            // Handle tab characters
            InlineContent::Tab { style } => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info("  Tab character".to_string()));
                }
                items.push(LogicalItem::Tab {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    style: style.clone(),
                });
            }
            // Other cases (Image, Shape, Space, Ruby). Text/Marker are handled by the `if let`
            // above (so they never reach here at runtime); `_` keeps this inner match exhaustive.
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Run is not text, creating generic LogicalItem.".to_string(),
                    ));
                }
                items.push(LogicalItem::Object {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    content: inline_item.clone(),
                });
            }
            }
        }
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting create_logical_items, created {} items ---",
            items.len()
        )));
    }
    items
}

// --- Stage 2 Implementation ---

// +spec:inline-block:d47971 - unicode-bidi:plaintext uses P2/P3 heuristic for base direction (implemented via get_base_direction)
// +spec:writing-modes:287491 - BiDi reordering and base direction detection (Appendix A text processing order)
// when determining base direction, consistent with their neutral bidi treatment
#[must_use] pub fn get_base_direction_from_logical(logical_items: &[LogicalItem]) -> BidiDirection {
    let first_strong = logical_items.iter().find_map(|item| {
        if let LogicalItem::Text { text, .. } = item {
            Some(unicode_bidi::get_base_direction(text.as_str()))
        } else {
            None
        }
    });

    match first_strong {
        Some(unicode_bidi::Direction::Rtl) => BidiDirection::Rtl,
        _ => BidiDirection::Ltr,
    }
}

// +spec:containing-block:149255 - bidi reordering produces inline box fragments that may separate in wide containing blocks
// +spec:containing-block:c7c08f - bidi reordering produces inline box fragments that may be adjacent in narrow containing blocks
// +spec:containing-block:2936ae - bidi reordering splits inline boxes into visual fragments (CSS Writing Modes 4 §2.4.5)
// +spec:display-property:0cdbd3 - bidi reordering splits inline boxes into visual runs; each run is shaped/formatted independently
// +spec:display-property:0d62a2 - bidi reordering of inline content respects block direction and unicode-bidi embedding
// +spec:display-property:10f9cd - bidi reordering splits and reorders inline box fragments
// +spec:display-property:58b30a - bidi paragraph breaks within inline boxes: each IFC does independent bidi analysis, so splitting an inline box at a paragraph boundary naturally closes/reopens bidi embeddings
// +spec:display-property:ecd935 - inline boxes split and reordered for uniform bidi flow
// +spec:writing-modes:330b8f - text ordered according to Unicode bidi algorithm after white-space processing
// +spec:writing-modes:7a9e7d - bidi control translation: text passed to unicode_bidi for reordering
// +spec:writing-modes:8e7281 - unicode-bidi property: bidi control codes inserted via BidiInfo
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if bidi reordering fails.
pub fn reorder_logical_items(
    logical_items: &[LogicalItem],
    base_direction: BidiDirection,
    unicode_bidi: UnicodeBidi,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<VisualItem>, LayoutError> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering reorder_logical_items ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input logical items count: {}",
            logical_items.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Base direction: {base_direction:?}"
        )));
    }

    // +spec:writing-modes:809513 - bidi string built across inline element boundaries; unicode-bidi:normal adds no extra embedding levels
    let mut bidi_str = String::new();
    let mut item_map = Vec::new();
    for (idx, item) in logical_items.iter().enumerate() {
        // +spec:containing-block:1fdc31 - inline boxes with unicode-bidi:normal are transparent to bidi algorithm
        // +spec:display-property:074abf - inline boxes transparent to bidi when unicode-bidi:normal
        // +spec:display-property:354966 - unicode-bidi control code injection for inline boxes
        // +spec:display-property:8409d3 - inline-level elements with unicode-bidi:normal have no effect on bidi ordering; embed creates an embedding
        // +spec:display-property:89464a - inline boxes with unicode-bidi:normal don't open embedding levels, so direction has no effect on bidi reordering
        // +spec:display-property:d47971 - bidi control codes should be injected at inline box boundaries based on unicode-bidi + direction
        // +spec:display-property:de657b - bidi control codes injected for display:inline boxes per unicode-bidi value
        // +spec:display-property:f01a81 - bidi-override should prepend LRO/RLO and append PDF per unicode-bidi CSS property (not yet implemented)
        // are treated as neutral characters in the bidi algorithm. Replaced elements with
        // +spec:display-property:fcb011 - unicode-bidi values on inline boxes insert bidi control codes
        // +spec:display-property:89095f - isolate/bidi-override/isolate-override/plaintext semantics
        // +spec:writing-modes:d490bf - direction only affects reordering when unicode-bidi is embed/override (not yet enforced for inline elements)
        // display:inline are also neutral unless unicode-bidi != normal (not yet implemented).
        // +spec:display-property:b4756e - replaced inline elements treated as neutral bidi chars;
        // embed/bidi-override exception not yet implemented (would make them strong chars).
        // U+FFFC (OBJECT REPLACEMENT CHARACTER) is a neutral bidi character.
        // +spec:display-property:df11ef - atomic inlines treated as neutral bidi characters (U+FFFC)
        // Replaced elements with display:inline are also neutral unless unicode-bidi != normal.
        let text = match item {
            LogicalItem::Text { text, .. } => text.as_str(),
            LogicalItem::CombinedText { text, .. } => text.as_str(),
            _ => "\u{FFFC}",
        };
        let start_byte = bidi_str.len();
        bidi_str.push_str(text);
        for _ in start_byte..bidi_str.len() {
            item_map.push(idx);
        }
    }

    if bidi_str.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Bidi string is empty, returning.".to_string(),
            ));
        }
        return Ok(Vec::new());
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Constructed bidi string: '{bidi_str}'"
        )));
    }

    // +spec:display-property:1a6075 - paragraph embedding level set from direction property per UAX9 HL1
    // +spec:containing-block:0d4914 - unicode-bidi: plaintext exception
    // When the containing block has unicode-bidi: plaintext, use None so the
    // Unicode bidi algorithm applies P2/P3 heuristics instead of the HL1 override
    let bidi_level = if unicode_bidi == UnicodeBidi::Plaintext {
        None
    } else if base_direction == BidiDirection::Rtl {
        Some(Level::rtl())
    } else {
        Some(Level::ltr())
    };
    // +spec:writing-modes:15bf17 - bidi isolation handled by unicode_bidi UAX #9 implementation
    let bidi_info = BidiInfo::new(&bidi_str, bidi_level);
    let para = &bidi_info.paragraphs[0];
    let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Bidi visual runs generated:".to_string(),
        ));
        for (i, run_range) in visual_runs.iter().enumerate() {
            let level = levels[run_range.start].number();
            let slice = &bidi_str[run_range.start..run_range.end];
            msgs.push(LayoutDebugMessage::info(format!(
                "  Run {i}: range={run_range:?}, level={level}, text='{slice}'"
            )));
        }
    }

    let mut visual_items = Vec::new();
    for run_range in visual_runs {
        let bidi_level = BidiLevel::new(levels[run_range.start].number());
        let mut sub_run_start = run_range.start;

        for i in (run_range.start + 1)..run_range.end {
            if item_map[i] != item_map[sub_run_start] {
                let logical_idx = item_map[sub_run_start];
                let logical_item = &logical_items[logical_idx];
                let text_slice = &bidi_str[sub_run_start..i];
                visual_items.push(VisualItem {
                    logical_source: logical_item.clone(),
                    bidi_level,
                    script: crate::text3::script::detect_script(text_slice)
                        .unwrap_or(Script::Latin),
                    text: text_slice.to_string(),
                });
                sub_run_start = i;
            }
        }

        let logical_idx = item_map[sub_run_start];
        let logical_item = &logical_items[logical_idx];
        let text_slice = &bidi_str[sub_run_start..run_range.end];
        visual_items.push(VisualItem {
            logical_source: logical_item.clone(),
            bidi_level,
            script: crate::text3::script::detect_script(text_slice).unwrap_or(Script::Latin),
            text: text_slice.to_string(),
        });
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Final visual items produced:".to_string(),
        ));
        for (i, item) in visual_items.iter().enumerate() {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Item {}: level={}, text='{}'",
                i,
                item.bidi_level.level(),
                item.text
            )));
        }
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting reorder_logical_items ---".to_string(),
        ));
    }
    Ok(visual_items)
}

// --- Stage 3 Implementation ---

/// Shape visual items into `ShapedItems` using pre-loaded fonts.
///
/// This function does NOT load any fonts - all fonts must be pre-loaded and passed in.
/// If a required font is not in `loaded_fonts`, the text will be skipped with a warning.
///
/// **Optimization: Inline Run Coalescing**
///
/// // +spec:display-property:9c6d59 - text shaping not broken across inline box boundaries when no effective formatting change
/// // +spec:display-property:cf8917 - text shaping not broken across inline box boundaries
/// When consecutive text `VisualItem`s share the same layout-affecting properties
/// (font, size, spacing, etc.) but differ only in rendering properties (color,
/// background), they are coalesced into a single shaping call. This dramatically
/// reduces the number of `font.shape_text()` invocations for syntax-highlighted
/// code where hundreds of `<span>` elements use the same monospace font but
/// different colors. After shaping, the original per-span styles are restored
/// to each `ShapedCluster` based on byte-range mapping.
/// Shape visual items with per-item caching. For each item (or coalesced group),
/// compute a cache key from (text, `bidi_level`, script, `style_layout_hash`). On cache
/// hit, reuse the previously shaped clusters. On miss, shape and store.
///
/// This is the incremental shaping path: when one word changes in a paragraph,
/// only that word's item misses the per-item cache; all other items hit.
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
/// # Errors
///
/// Returns a `LayoutError` if shaping the visual items fails.
pub fn shape_visual_items_with_per_item_cache<T: ParsedFontTrait>(
    visual_items: &[VisualItem],
    per_item_cache: &mut HashMap<u64, Arc<PerItemShapedEntry>>,
    per_item_accessed: &mut HashSet<u64>,
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<ShapedItem>, LayoutError> {
    use std::hash::{Hash, Hasher};
    // Delegate to the existing shaping logic, but for each coalesce group,
    // check the per-item cache first.
    //
    // Strategy: Identify coalesce groups (adjacent items with same layout_hash,
    // bidi_level, script). For each group, compute a key from the concatenated
    // text + shared properties. Check cache. On miss, shape the group and cache it.
    let mut shaped = Vec::new();
    let mut idx = 0;

    while idx < visual_items.len() {
        let item = &visual_items[idx];

        // Determine coalesce group boundaries (same logic as shape_visual_items)
        let (layout_hash, bidi_level, script) = match &item.logical_source {
            LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                (style.layout_hash(), item.bidi_level, item.script)
            }
            _ => {
                // Non-text items: shape individually (no coalescing)
                let single = shape_visual_items(
                    &visual_items[idx..=idx],
                    font_chain_cache, fc_cache, loaded_fonts, debug_messages,
                )?;
                shaped.extend(single);
                idx += 1;
                continue;
            }
        };

        let mut coalesce_end = idx + 1;
        while coalesce_end < visual_items.len() {
            let next = &visual_items[coalesce_end];
            let next_layout_hash = match &next.logical_source {
                LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                    Some(style.layout_hash())
                }
                _ => None,
            };
            if let Some(nlh) = next_layout_hash {
                if nlh == layout_hash
                    && next.bidi_level == bidi_level
                    && next.script == script
                {
                    coalesce_end += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Compute per-group cache key
        let mut hasher = DefaultHasher::new();
        for item in &visual_items[idx..coalesce_end] {
            item.text.hash(&mut hasher);
        }
        layout_hash.hash(&mut hasher);
        bidi_level.hash(&mut hasher);
        (script as u32).hash(&mut hasher);
        let group_key = hasher.finish();

        // Check per-item cache
        per_item_accessed.insert(group_key);
        if let Some(cached) = per_item_cache.get(&group_key) {
            shaped.extend(cached.clusters.iter().cloned());
        } else {
            // Cache miss — shape this group
            let group_items = shape_visual_items(
                &visual_items[idx..coalesce_end],
                font_chain_cache, fc_cache, loaded_fonts, debug_messages,
            )?;
            let total_advance: f32 = group_items.iter().map(|item| {
                match item {
                    ShapedItem::Cluster(c) => c.advance,
                    _ => 0.0,
                }
            }).sum();
            per_item_cache.insert(group_key, Arc::new(PerItemShapedEntry {
                clusters: group_items.clone(),
                total_advance,
            }));
            shaped.extend(group_items);
        }

        idx = coalesce_end;
    }

    Ok(shaped)
}

/// Split text into segments where consecutive characters resolve to the same font
/// in the fallback chain. Returns Vec<(`byte_start`, `byte_end`, `FontId`)>.
///
/// Characters that can't be resolved to any font are skipped (gap in coverage).
fn split_text_by_font_coverage<T: ParsedFontTrait>(
    text: &str,
    font_chain: &rust_fontconfig::FontFallbackChain,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Vec<(usize, usize, FontId)> {
    let mut segments: Vec<(usize, usize, FontId)> = Vec::new();

    for (byte_idx, ch) in text.char_indices() {
        let char_end = byte_idx + ch.len_utf8();
        // Primary: the resolved fallback chain. Its coverage comes from
        // rust-fontconfig's OS/2-derived `unicode_ranges`, which can MISS
        // codepoints a font actually has in its cmap — e.g. Noto Sans CJK's
        // JP face does not advertise the Hangul OS/2 block, so 한국어 resolves
        // to None here even though that face's cmap covers it.
        let font_id = font_chain
            .resolve_char(fc_cache, ch)
            .map(|(id, _)| id)
            // Fallback: probe the actually-loaded fonts by REAL glyph coverage
            // so OS/2-vs-cmap gaps render instead of being silently dropped.
            // The covering CJK face is already loaded (Han/Kana resolved to it),
            // so this reuses it for Hangul rather than mixing in another font.
            .or_else(|| {
                loaded_fonts
                    .iter()
                    .find(|(_, font)| font.has_glyph(ch as u32))
                    .map(|(id, _)| *id)
            });
        if let Some(font_id) = font_id {
            match segments.last_mut() {
                Some(last) if last.2 == font_id && last.1 == byte_idx => {
                    // Extend current segment (same font, contiguous)
                    last.1 = char_end;
                }
                _ => {
                    // New segment (different font or gap)
                    segments.push((byte_idx, char_end, font_id));
                }
            }
        }
    }

    segments
}

/// Measures the total inline advance (width in horizontal mode) of `text` shaped at
/// `style`, using the same font-resolution path as the main shaper. Returns `None` if the
/// font chain is not resolved / shaping fails, so callers can fall back to an estimate.
///
/// Used by ruby layout to size the base and annotation runs from REAL shaped advances
/// (instead of a `chars * font_size * magic_ratio` fudge).
fn measure_run_advance<T: ParsedFontTrait>(
    text: &str,
    style: &Arc<StyleProperties>,
    script: Script,
    source: ContentIndex,
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    let language = script_to_language(script, text);
    match &style.font_stack {
        FontStack::Ref(font_ref) => {
            let glyphs = font_ref
                .shape_text(text, script, language, BidiDirection::Ltr, style.as_ref())
                .ok()?;
            Some(glyphs.iter().map(|g| g.advance + g.kerning).sum())
        }
        FontStack::Stack(selectors) => {
            let cache_key = FontChainKey::from_selectors(selectors);
            let font_chain = font_chain_cache.get(&cache_key)?;
            let clusters = shape_with_font_fallback(
                text, script, language, BidiDirection::Ltr, style, source, None, font_chain,
                fc_cache, loaded_fonts,
            )
            .ok()?;
            Some(clusters.iter().map(|c| c.advance).sum())
        }
    }
}

/// Shape text with per-character font fallback.
///
/// Splits the text into segments by font coverage, shapes each segment with
/// its resolved font, and fixes byte offsets so they're relative to the
/// original `text` (not the segment substring).
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
fn shape_with_font_fallback<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: Language,
    direction: BidiDirection,
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
    source_node_id: Option<NodeId>,
    font_chain: &rust_fontconfig::FontFallbackChain,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    // Cache the debug flag in a `OnceLock<bool>` — reading it per-shape
    // (this function fires once per text segment, ~hundreds of times
    // per render of a real DOM) costs ~100 ns per `std::env::var_os`
    // call on macOS (env-lock + hashmap lookup), and even before the
    // lookup finishes the `eprintln!` machinery takes a stderr lock
    // and allocates the formatted string. Both are invisible in
    // release unless `AZ_FONT_FALLBACK_DEBUG=1` is set.
    static FONT_FB_DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let dbg = *FONT_FB_DEBUG.get_or_init(|| {
        std::env::var_os("AZ_FONT_FALLBACK_DEBUG").is_some()
    });

    let segments = split_text_by_font_coverage(text, font_chain, fc_cache, loaded_fonts);

    if dbg && segments.len() > 1 {
        eprintln!(
            "[FONT FALLBACK] text needs {} font segments for '{}' ({}..{} bytes)",
            segments.len(),
            text.chars().take(40).collect::<String>(),
            0, text.len()
        );
    }

    unsafe { crate::az_mark(0x60850_u32, segments.len() as u32); } // [g123] segments count (split_text_by_font_coverage)
    if segments.len() <= 1 {
        // Fast path: all characters use the same font (common case)
        let (seg_start, seg_end, font_id) = if let Some(s) = segments.first() { unsafe { crate::az_mark(0x60854_u32, 0x0000_0001_u32); } s } else {
            unsafe { crate::az_mark(0x60854_u32, 0x0000_00EE_u32); } // [g123] split→0 segments (resolve_char failed all)
            if dbg {
                eprintln!("[FONT FALLBACK] no font could render any char in '{}'", text.chars().take(20).collect::<String>());
            }
            return Ok(Vec::new());
        };
        let font = if let Some(f) = loaded_fonts.get(font_id) { unsafe { crate::az_mark(0x60858_u32, 0x0000_0001_u32); } f } else {
            unsafe { crate::az_mark(0x60858_u32, 0x0000_00EE_u32); } // [g123] loaded_fonts.get MISS
            if dbg {
                eprintln!("[FONT FALLBACK] font {:?} not in loaded_fonts for '{}'", font_id, text.chars().take(20).collect::<String>());
            }
            return Ok(Vec::new());
        };
        // If segment covers the full text (overwhelmingly common), skip substr+fixup
        if *seg_start == 0 && *seg_end == text.len() {
            unsafe { crate::az_mark(0x60860_u32, 0xC0DE_0860_u32); } // [g123] reached shape_text_correctly (full-text)
            return shape_text_correctly(
                text, script, language, direction,
                font, style, source_index, source_node_id,
            );
        }
        let mut clusters = shape_text_correctly(
            &text[*seg_start..*seg_end], script, language, direction,
            font, style, source_index, source_node_id,
        )?;
        if *seg_start > 0 {
            for cluster in &mut clusters {
                cluster.source_cluster_id.start_byte_in_run += *seg_start as u32;
            }
        }
        return Ok(clusters);
    }

    // Multiple fonts needed — shape each segment separately
    let mut all_clusters = Vec::new();
    for (seg_start, seg_end, font_id) in &segments {
        let Some(font) = loaded_fonts.get(font_id) else {
            if dbg {
                eprintln!("[FONT FALLBACK] font {font_id:?} NOT loaded, skipping segment bytes {seg_start}..{seg_end}");
            }
            continue;
        };
        let segment_text = &text[*seg_start..*seg_end];
        if dbg {
            eprintln!(
                "[FONT FALLBACK] text='{segment_text}' uses font {font_id:?} (bytes {seg_start}..{seg_end})"
            );
        }
        let mut seg_clusters = shape_text_correctly(
            segment_text, script, language, direction,
            font, style, source_index, source_node_id,
        )?;
        // Fix byte offsets: shape_text_correctly produces offsets relative to
        // segment_text, but callers expect offsets relative to the full text.
        if *seg_start > 0 {
            for cluster in &mut seg_clusters {
                cluster.source_cluster_id.start_byte_in_run += *seg_start as u32;
            }
        }
        all_clusters.extend(seg_clusters);
    }
    Ok(all_clusters)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if shaping the visual items fails.
pub fn shape_visual_items<T: ParsedFontTrait>(
    visual_items: &[VisualItem],
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<ShapedItem>, LayoutError> {
    let mut shaped = Vec::new();
    let mut idx = 0;
    let mut _coalesced_runs = 0usize;
    let mut _total_runs = 0usize;
    let mut _shape_calls = 0usize;

    // Log count of visual items for debugging coalescing

    while idx < visual_items.len() {
        let item = &visual_items[idx];
        match &item.logical_source {
            LogicalItem::Text {
                style,
                source,
                marker_position_outside,
                source_node_id,
                ..
            } => {
                let layout_hash = style.layout_hash();
                let bidi_level = item.bidi_level;
                let script = item.script;

                // +spec:display-property:ca95f6 - text shaping breaks at inline box boundaries when layout-affecting properties differ
                // when layout-affecting properties (font weight, family, size, etc.) change
                // across element boundaries, preventing ligatures from forming across such changes.
                // Look ahead: find consecutive text items with the same layout-affecting
                // properties (font, size, spacing) that can be shaped as one merged run.
                let mut coalesce_end = idx + 1;
                while coalesce_end < visual_items.len() {
                    let next = &visual_items[coalesce_end];
                    if let LogicalItem::Text { style: next_style, .. } = &next.logical_source {
                        if next_style.layout_hash() == layout_hash
                            && next.bidi_level == bidi_level
                            && next.script == script
                        {
                            coalesce_end += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                let coalesce_count = coalesce_end - idx;

                if coalesce_count > 1 {
                    _coalesced_runs += coalesce_count;
                    _shape_calls += 1;
                    // ── COALESCED PATH ──
                    // Merge N text items into one shaping call, then split results
                    // back per original run to preserve per-span rendering styles.

                    // Build merged text and record byte ranges → original style
                    let total_text_len: usize = visual_items[idx..coalesce_end]
                        .iter()
                        .map(|v| v.text.len())
                        .sum();
                    let mut merged_text = String::with_capacity(total_text_len);
                    // (byte_start, byte_end, style, source, source_node_id, marker_outside)
                    let mut byte_ranges: Vec<(
                        usize, usize,
                        Arc<StyleProperties>,
                        ContentIndex,
                        Option<NodeId>,
                        Option<bool>,
                    )> = Vec::with_capacity(coalesce_count);

                    for item in &visual_items[idx..coalesce_end] {
                        let start = merged_text.len();
                        merged_text.push_str(&item.text);
                        let end = merged_text.len();
                        if let LogicalItem::Text {
                            style: s, source: src, source_node_id: nid,
                            marker_position_outside: mpo, ..
                        } = &item.logical_source {
                            byte_ranges.push((start, end, s.clone(), *src, *nid, *mpo));
                        }
                    }

                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "[TextLayout] Coalescing {} text runs ({} bytes) into single shaping call",
                            coalesce_count, merged_text.len()
                        )));
                    }

                    let direction = if bidi_level.is_rtl() {
                        BidiDirection::Rtl
                    } else {
                        BidiDirection::Ltr
                    };
                    let language = script_to_language(script, &merged_text);

                    // Shape the merged text using the first item's font (layout is identical
                    // for all coalesced items since layout_hash matches).
                    let shaped_clusters_result: Result<Vec<ShapedCluster>, LayoutError> = match &style.font_stack {
                        FontStack::Ref(font_ref) => {
                            shape_text_correctly(
                                &merged_text, script, language, direction,
                                font_ref, style, *source, *source_node_id,
                            )
                        }
                        FontStack::Stack(selectors) => {
                            let cache_key = FontChainKey::from_selectors(selectors);
                            let Some(font_chain) = font_chain_cache.get(&cache_key) else { idx = coalesce_end; continue; };
                            // Per-character font fallback: split text by font coverage
                            shape_with_font_fallback(
                                &merged_text, script, language, direction,
                                style, *source, *source_node_id,
                                font_chain, fc_cache, loaded_fonts,
                            )
                        }
                    };

                    let shaped_clusters = shaped_clusters_result?;

                    // Restore original per-span styles to each cluster based on byte position.
                    // Each ShapedCluster's source_cluster_id.start_byte_in_run is the byte
                    // offset within the merged text — we use byte_ranges to find which
                    // original run it belongs to and reassign its style, source info, etc.
                    for cluster in shaped_clusters {
                        let byte_pos = cluster.source_cluster_id.start_byte_in_run as usize;
                        // Find the original run this cluster's first byte falls into
                        let orig = byte_ranges.iter().find(|(start, end, ..)| {
                            byte_pos >= *start && byte_pos < *end
                        });
                        let mut cluster = cluster;
                        if let Some((range_start, _, orig_style, orig_source, orig_nid, orig_mpo)) = orig {
                            // Reassign rendering-affecting style (color, background, etc.)
                            cluster.style = orig_style.clone();
                            cluster.source_content_index = *orig_source;
                            cluster.source_node_id = *orig_nid;
                            // Fix the byte offset to be relative to the original run
                            cluster.source_cluster_id.source_run = orig_source.run_index;
                            cluster.source_cluster_id.start_byte_in_run = (byte_pos - range_start) as u32;
                            // Update glyph styles
                            for glyph in &mut cluster.glyphs {
                                glyph.style = orig_style.clone();
                            }
                            if let Some(is_outside) = orig_mpo {
                                cluster.marker_position_outside = Some(*is_outside);
                            }
                        }
                        shaped.push(ShapedItem::Cluster(cluster));
                    }

                    idx = coalesce_end;
                    continue;
                }

                // ── SINGLE ITEM PATH (no coalescing) ──
                _total_runs += 1;
                _shape_calls += 1;
                let direction = if item.bidi_level.is_rtl() {
                    BidiDirection::Rtl
                } else {
                    BidiDirection::Ltr
                };

                let language = script_to_language(item.script, &item.text);

                // Shape text using either FontRef directly or fontconfig-resolved font
                let shaped_clusters_result: Result<Vec<ShapedCluster>, LayoutError> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        unsafe { crate::az_mark(0x60820_u32, 0x0000_0001_u32); } // [g121] Ref arm
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for text: '{}'",
                                item.text.chars().take(30).collect::<String>()
                            )));
                        }
                        shape_text_correctly(
                            &item.text,
                            item.script,
                            language,
                            direction,
                            font_ref,
                            style,
                            *source,
                            *source_node_id,
                        )
                    }
                    FontStack::Stack(selectors) => {
                        unsafe { crate::az_mark(0x60820_u32, 0x0000_0002_u32); } // [g121] Stack arm
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);
                        unsafe { crate::az_mark(0x60824_u32, font_chain_cache.len() as u32); } // [g121] chain map len

                        // Look up the pre-resolved font chain. (2026-06-10: the g122
                        // by_find/by_only fallback chain is GONE — the historic miss was a
                        // KEY-CONSTRUCTION divergence (duplicated families on the query side,
                        // deduped on the store side), fixed by routing every key build through
                        // FontChainKey::from_selectors. Verified lifted: lookup path = get.)
                        let Some(font_chain) = font_chain_cache.get(&cache_key) else {
                            if let Some(msgs) = debug_messages {
                                msgs.push(LayoutDebugMessage::warning(format!(
                                    "[TextLayout] Font chain not pre-resolved for {:?} - text will \
                                     not be rendered",
                                    cache_key.font_families
                                )));
                            }
                            idx += 1;
                            continue;
                        };

                        // Per-character font fallback: split text by font coverage
                        shape_with_font_fallback(
                            &item.text, item.script, language, direction,
                            style, *source, *source_node_id,
                            font_chain, fc_cache, loaded_fonts,
                        )
                    }
                };

                let mut shaped_clusters = shaped_clusters_result?;

                // Set marker flag on all clusters if this is a marker
                if let Some(is_outside) = marker_position_outside {
                    for cluster in &mut shaped_clusters {
                        cluster.marker_position_outside = Some(*is_outside);
                    }
                }

                shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));
            }
            // +spec:display-property:df076b - tab-size rendering and inline-level line breaking
            // "If the tab size is zero, preserved tabs are not rendered."
            // "Otherwise, each preserved tab is rendered as a horizontal shift that lines up
            //  the start edge of the next glyph with the next tab stop."
            // "Tab stops occur at points that are multiples of the tab size from the starting
            //  content edge of the preserved tab's nearest block container ancestor."
            LogicalItem::Tab { source, style } => {
                if style.tab_size == 0.0 {
                    // Tab size zero: tab is not rendered (zero width)
                    shaped.push(ShapedItem::Tab {
                        source: *source,
                        bounds: Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 0.0,
                            height: 0.0,
                        },
                    });
                } else {
                    // TODO: use actual font's space_width via ParsedFontTrait::get_space_width()
                    // once we thread font resolution into the shaping phase for tab stops.
                    // For now, approximate space advance as 0.5 * font_size (typical for Latin fonts).
                    let space_advance_approx = style.font_size_px * SPACE_WIDTH_RATIO;
                    // +spec:text-alignment-spacing:5a5efd - tab-size includes letter-spacing and word-spacing
                    let ls = match style.letter_spacing {
                        Spacing::Px(px) => px as f32,
                        Spacing::Em(em) => em * style.font_size_px,
                    };
                    let ws = match style.word_spacing {
                        Spacing::Px(px) => px as f32,
                        Spacing::Em(em) => em * style.font_size_px,
                    };
                    // Tab stop interval: tab_size * (space advance + letter-spacing + word-spacing)
                    let tab_interval = style.tab_size * (space_advance_approx + ls + ws);
                    // Calculate current advance to find next tab stop
                    let current_advance: f32 = shaped.iter().map(|item| {
                        match item {
                            ShapedItem::Cluster(c) => c.advance,
                            ShapedItem::Tab { bounds, .. } => bounds.width,
                            ShapedItem::Object { bounds, .. } => bounds.width,
                            _ => 0.0,
                        }
                    }).sum();
                    // Next tab stop = next multiple of tab_interval from content edge
                    let next_tab_stop = ((current_advance / tab_interval).floor() + 1.0) * tab_interval;
                    let mut tab_width = next_tab_stop - current_advance;
                    // "If this distance is less than 0.5ch, then the subsequent tab stop is used instead."
                    let half_ch = space_advance_approx * 0.5;
                    if tab_width < half_ch {
                        tab_width += tab_interval;
                    }
                    shaped.push(ShapedItem::Tab {
                        source: *source,
                        bounds: Rect {
                            x: 0.0,
                            y: 0.0,
                            width: tab_width,
                            height: 0.0,
                        },
                    });
                }
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                // CSS Ruby Layout (§3): the annotation (ruby-text) is laid out at its used
                // `font-size` — the UA default is `RUBY_ANNOTATION_FONT_SCALE` of the base —
                // and centered over the base, with the ruby box reserving the WIDER of the
                // two inline-sizes and stacking the annotation line above the base line.
                //
                // Both the base and the annotation are shaped to obtain their REAL inline
                // advances (no `chars * font_size * 0.6` fudge). The annotation is shaped at
                // the scaled style so its width reflects the smaller glyphs.
                let base_font_size = style.font_size_px;
                let annotation_font_size = base_font_size * RUBY_ANNOTATION_FONT_SCALE;

                let mut annotation_props = (**style).clone();
                annotation_props.font_size_px = annotation_font_size;
                let annotation_style = Arc::new(annotation_props);

                // Fallback estimate (only when shaping fails / no font chain): 1em per char
                // is a closer CJK approximation than the old 0.6 ratio.
                let base_width = measure_run_advance(
                    base_text, style, item.script, *source, font_chain_cache, fc_cache,
                    loaded_fonts,
                )
                .unwrap_or_else(|| base_text.chars().count() as f32 * base_font_size);
                let annotation_width = measure_run_advance(
                    ruby_text, &annotation_style, item.script, *source, font_chain_cache,
                    fc_cache, loaded_fonts,
                )
                .unwrap_or_else(|| ruby_text.chars().count() as f32 * annotation_font_size);

                let base_line_height =
                    style.line_height.resolve(base_font_size, 0.0, 0.0, 0.0, 0);
                let annotation_line_height = annotation_style.line_height.resolve(
                    annotation_font_size, 0.0, 0.0, 0.0, 0,
                );
                // The ruby box reserves the wider inline-size, and stacks the annotation
                // line (at its smaller font-size) above the base line.
                let (reserved_width, reserved_height) = ruby_reserved_box(
                    base_width,
                    annotation_width,
                    base_line_height,
                    annotation_line_height,
                );

                // TODO2: the annotation glyphs are now correctly sized + reserve vertical
                // space above the base, but are not yet emitted as a separately positioned
                // (centered) run — `ShapedItem::Object` carries only the base `StyledRun`.
                // Rendering the centered annotation needs a ruby-aware `ShapedItem` variant
                // (rendering-structural change); deferred to keep this change layout-safe.
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: reserved_width,
                        height: reserved_height,
                    },
                    baseline_offset: 0.0,
                    content: InlineContent::Text(StyledRun {
                        text: base_text.clone(),
                        style: style.clone(),
                        logical_start_byte: 0,
                        source_node_id: None,
                    }),
                });
            }
            LogicalItem::CombinedText {
                style,
                source,
                text,
            } => {
                let language = script_to_language(item.script, &item.text);

                // +spec:width-calculation:657f75 - convert full-width chars to non-full-width before compression
                // +spec:width-calculation:d0a295 - full-width digit conversion example (e.g. "23" stays narrow)
                // When combined text has more than one typographic character unit,
                // full-width characters (U+FF01..U+FF5E) are converted to their
                // ASCII equivalents (U+0021..U+007E) before compression.
                let text = if text.chars().count() > 1 {
                    let converted: String = text.chars().map(|c| {
                        let cp = c as u32;
                        if (0xFF01..=0xFF5E).contains(&cp) {
                            // Reverse of text-transform: full-width
                            char::from_u32(cp - 0xFF01 + 0x0021).unwrap_or(c)
                        } else {
                            c
                        }
                    }).collect();
                    converted
                } else {
                    text.clone()
                };

                // +spec:width-calculation:1ed84d - OpenType compression (half-width/third-width substitution)
                // is delegated to the font shaping layer via shape_text()

                // Shape CombinedText using either FontRef directly or fontconfig-resolved font
                let glyphs: Vec<Glyph> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for CombinedText: '{}'",
                                text.chars().take(30).collect::<String>()
                            )));
                        }
                        font_ref.shape_text(
                            &text,
                            item.script,
                            language,
                            BidiDirection::Ltr,
                            style.as_ref(),
                        )?
                    }
                    FontStack::Stack(selectors) => {
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);

                        let Some(font_chain) = font_chain_cache.get(&cache_key) else {
                            if let Some(msgs) = debug_messages {
                                msgs.push(LayoutDebugMessage::warning(format!(
                                    "[TextLayout] Font chain not pre-resolved for CombinedText {:?}",
                                    cache_key.font_families
                                )));
                            }
                            idx += 1;
                            continue;
                        };

                        // Per-character font fallback for CombinedText
                        let segments = split_text_by_font_coverage(&text, font_chain, fc_cache, loaded_fonts);
                        let mut all_glyphs = Vec::new();
                        for (seg_start, seg_end, font_id) in &segments {
                            let Some(font) = loaded_fonts.get(font_id) else { continue; };
                            let segment_text = &text[*seg_start..*seg_end];
                            let mut seg_glyphs = font.shape_text(
                                segment_text,
                                item.script,
                                language,
                                BidiDirection::Ltr,
                                style.as_ref(),
                            )?;
                            // Fix byte offsets for glyphs
                            if *seg_start > 0 {
                                for g in &mut seg_glyphs {
                                    g.logical_byte_index += *seg_start;
                                    g.cluster += *seg_start as u32;
                                }
                            }
                            all_glyphs.extend(seg_glyphs);
                        }
                        if all_glyphs.is_empty() {
                            idx += 1;
                            continue;
                        }
                        all_glyphs
                    }
                };

                let shaped_glyphs: ShapedGlyphVec = glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        kind: GlyphKind::Character,
                        glyph_id: g.glyph_id,
                        script: g.script,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics,
                        style: g.style,
                        cluster_offset: 0,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                    })
                    .collect();

                // +spec:block-formatting-context:dc4549 - text-combine-upright compression: UA may scale composition to match 水 advance height
                let total_width: f32 = shaped_glyphs.iter().map(|g| g.advance + g.kerning).sum();
                // +spec:inline-formatting-context:8c5969 - text-combine-upright baseline centering
                // The composition forms a 1em square. Per spec, its baseline must be
                // chosen so the square is centered between the text-over and text-under
                // baselines of the parent inline box. We approximate by using font_size
                // as the square height and centering it (baseline_offset = em_size / 2).
                let em_size = shaped_glyphs.first()
                    .map_or(style.font_size_px, |g| g.style.font_size_px);
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: em_size,
                };

                shaped.push(ShapedItem::CombinedBlock {
                    source: *source,
                    glyphs: shaped_glyphs,
                    bounds,
                    baseline_offset: em_size / 2.0,
                });
            }
            LogicalItem::Object {
                content, source, ..
            } => {
                let (bounds, baseline) = measure_inline_object(content)?;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds,
                    baseline_offset: baseline,
                    content: content.clone(),
                });
            }
            LogicalItem::Break { source, break_info } => {
                shaped.push(ShapedItem::Break {
                    source: *source,
                    break_info: *break_info,
                });
            }
        }
        idx += 1;
    }

    Ok(shaped)
}

/// Returns true if `c` is a hanging punctuation stop or comma per CSS Text 3 §8.2.1.
// +spec:hanging-punctuation - full stop/comma character list per CSS Text 3 §8.2.1
const fn is_hanging_punctuation_char(c: char) -> bool {
    matches!(c,
        ','      | // U+002C COMMA
        '.'      | // U+002E FULL STOP
        '\u{060C}' | // ARABIC COMMA
        '\u{06D4}' | // ARABIC FULL STOP
        '\u{3001}' | // IDEOGRAPHIC COMMA
        '\u{3002}' | // IDEOGRAPHIC FULL STOP
        '\u{FF0C}' | // FULLWIDTH COMMA
        '\u{FF0E}' | // FULLWIDTH FULL STOP
        '\u{FE50}' | // SMALL COMMA
        '\u{FE51}' | // SMALL IDEOGRAPHIC COMMA
        '\u{FE52}' | // SMALL FULL STOP
        '\u{FF61}' | // HALFWIDTH IDEOGRAPHIC FULL STOP
        '\u{FF64}'   // HALFWIDTH IDEOGRAPHIC COMMA
    )
}

/// Helper to check if a cluster contains only hanging punctuation.
// +spec:box-model:8bbcd1 - non-zero inline-axis borders/padding between hangable glyph and line edge prevent hanging
/// +spec:inline-formatting-context:135be2 - hanging punctuation placed outside the line box
/// +spec:intrinsic-sizing:407d8b - hanging glyphs not counted in intrinsic size computation
fn is_hanging_punctuation(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        if c.glyphs.len() == 1 {
            c.text.chars().next().is_some_and(is_hanging_punctuation_char)
        } else {
            false
        }
    } else {
        false
    }
}

#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn shape_text_correctly<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: Language,
    direction: BidiDirection,
    font: &T, // Changed from &Arc<T>
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
    source_node_id: Option<NodeId>,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    unsafe { crate::az_mark(0x60864_u32, 0xC0DE_0864_u32); } // [g123] shape_text_correctly ENTERED
    let glyphs = font.shape_text(text, script, language, direction, style.as_ref())?;
    unsafe { crate::az_mark(0x60868_u32, (glyphs.len() as u32) | 0x8000_0000_u32); } // [g123] font.shape_text returned (high bit set); low bits = glyph count

    if glyphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();

    // Group glyphs by cluster ID from the shaper.
    let mut current_cluster_glyphs = Vec::new();
    let mut cluster_id = glyphs[0].cluster;
    let mut cluster_start_byte_in_text = glyphs[0].logical_byte_index;

    for glyph in glyphs {
        if glyph.cluster != cluster_id {
            // Finalize previous cluster
            let advance = current_cluster_glyphs
                .iter()
                .map(|g: &Glyph| g.advance)
                .sum();

            // Safely extract cluster text - handle cases where byte indices may be out of order
            // (can happen with RTL text or complex GSUB reordering)
            let (start, end) = if cluster_start_byte_in_text <= glyph.logical_byte_index {
                (cluster_start_byte_in_text, glyph.logical_byte_index)
            } else {
                (glyph.logical_byte_index, cluster_start_byte_in_text)
            };
            let cluster_text = text.get(start..end).unwrap_or("");

            clusters.push(ShapedCluster {
                text: cluster_text.to_string(), // Store original text for hyphenation
                source_cluster_id: GraphemeClusterId {
                    source_run: source_index.run_index,
                    start_byte_in_run: cluster_id,
                },
                source_content_index: source_index,
                source_node_id,
                glyphs: current_cluster_glyphs
                    .iter()
                    .map(|g| {
                        // Calculate cluster_offset safely
                        let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                            (g.logical_byte_index - cluster_start_byte_in_text) as u32
                        } else {
                            0
                        };
                        ShapedGlyph {
                            kind: if g.glyph_id == 0 {
                                GlyphKind::NotDef
                            } else {
                                GlyphKind::Character
                            },
                            glyph_id: g.glyph_id,
                            script: g.script,
                            font_hash: g.font_hash,
                            font_metrics: g.font_metrics,
                            style: g.style.clone(),
                            cluster_offset,
                            advance: g.advance,
                            kerning: g.kerning,
                            vertical_advance: g.vertical_advance,
                            vertical_offset: g.vertical_bearing,
                            offset: g.offset,
                        }
                    })
                    .collect(),
                advance,
                direction,
                style: style.clone(),
                marker_position_outside: None,
                is_first_fragment: true,
                is_last_fragment: true,
            });
            current_cluster_glyphs.clear();
            cluster_id = glyph.cluster;
            cluster_start_byte_in_text = glyph.logical_byte_index;
        }
        current_cluster_glyphs.push(glyph);
    }

    // Finalize the last cluster
    if !current_cluster_glyphs.is_empty() {
        let advance = current_cluster_glyphs
            .iter()
            .map(|g: &Glyph| g.advance)
            .sum();
        let cluster_text = text.get(cluster_start_byte_in_text..).unwrap_or("");
        clusters.push(ShapedCluster {
            text: cluster_text.to_string(), // Store original text
            source_cluster_id: GraphemeClusterId {
                source_run: source_index.run_index,
                start_byte_in_run: cluster_id,
            },
            source_content_index: source_index,
            source_node_id,
            glyphs: current_cluster_glyphs
                .iter()
                .map(|g| {
                    // Calculate cluster_offset safely
                    let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                        (g.logical_byte_index - cluster_start_byte_in_text) as u32
                    } else {
                        0
                    };
                    ShapedGlyph {
                        kind: if g.glyph_id == 0 {
                            GlyphKind::NotDef
                        } else {
                            GlyphKind::Character
                        },
                        glyph_id: g.glyph_id,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics,
                        style: g.style.clone(),
                        script: g.script,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                        cluster_offset,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                    }
                })
                .collect(),
            advance,
            direction,
            style: style.clone(),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        });
    }

    Ok(clusters)
}

/// Measures a non-text object, returning its bounds and baseline offset.
fn measure_inline_object(item: &InlineContent) -> Result<(Rect, f32), LayoutError> {
    match item {
        InlineContent::Image(img) => {
            let size = img.display_size.unwrap_or(img.intrinsic_size);
            Ok((
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                img.baseline_offset,
            ))
        }
        InlineContent::Shape(shape) => Ok({
            let size = shape.shape_def.get_size();
            (
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                shape.baseline_offset,
            )
        }),
        InlineContent::Space(space) => Ok((
            Rect {
                x: 0.0,
                y: 0.0,
                width: space.width,
                height: 0.0,
            },
            0.0,
        )),
        InlineContent::Marker { .. } => {
            // Markers are treated as text content, not measurable objects
            Err(LayoutError::InvalidText(
                "Marker is text content, not a measurable object".into(),
            ))
        }
        _ => Err(LayoutError::InvalidText("Not a measurable object".into())),
    }
}

// --- Stage 4 Implementation: Vertical Text ---

/// Applies orientation and vertical metrics to glyphs if the writing mode is vertical.
// +spec:block-formatting-context:227171 - vertical glyph orientation with fallback vertical metrics
// +spec:block-formatting-context:df20a5 - mixed vertical orientation dispatch (TextOrientation::Mixed)
fn apply_text_orientation(
    items: Arc<Vec<ShapedItem>>,
    constraints: &UnifiedConstraints,
) -> Arc<Vec<ShapedItem>> {
    if !constraints.is_vertical() {
        return items;
    }

    let mut oriented_items = Vec::with_capacity(items.len());
    let writing_mode = constraints.writing_mode.unwrap_or_default();

    for item in items.iter() {
        match item {
            ShapedItem::Cluster(cluster) => {
                let mut new_cluster = cluster.clone();
                let mut total_vertical_advance = 0.0;

                for glyph in &mut new_cluster.glyphs {
                    // Use the vertical metrics already computed during shaping
                    // If they're zero, use fallback values
                    if glyph.vertical_advance > 0.0 {
                        total_vertical_advance += glyph.vertical_advance;
                    } else {
                        // Fallback: use line height for vertical advance
                        let fallback_advance = cluster.style.line_height.resolve_with_metrics(cluster.style.font_size_px, &glyph.font_metrics);
                        glyph.vertical_advance = fallback_advance;
                        // Center the glyph horizontally as a fallback
                        glyph.vertical_offset = Point {
                            x: -glyph.advance / 2.0,
                            y: 0.0,
                        };
                        total_vertical_advance += fallback_advance;
                    }
                }
                // The cluster's `advance` now represents vertical advance.
                new_cluster.advance = total_vertical_advance;
                oriented_items.push(ShapedItem::Cluster(new_cluster));
            }
            // Non-text objects also need their advance axis swapped.
            ShapedItem::Object {
                source,
                bounds,
                baseline_offset,
                content,
            } => {
                let mut new_bounds = *bounds;
                std::mem::swap(&mut new_bounds.width, &mut new_bounds.height);
                oriented_items.push(ShapedItem::Object {
                    source: *source,
                    bounds: new_bounds,
                    baseline_offset: *baseline_offset,
                    content: content.clone(),
                });
            }
            _ => oriented_items.push(item.clone()),
        }
    }

    Arc::new(oriented_items)
}

// --- Stage 5 & 6 Implementation: Combined Layout Pass ---
// This section replaces the previous simple line breaking and positioning logic.

/// Extracts the per-item vertical-align from a `ShapedItem`.
///
/// For `Object` items (inline-blocks, images), this returns the alignment stored
/// in the original `InlineContent`. For text clusters and other items, returns `None`
/// to indicate the global `constraints.vertical_align` should be used.
const fn get_item_vertical_align(item: &ShapedItem) -> Option<VerticalAlign> {
    match item {
        ShapedItem::Object { content, .. } => match content {
            InlineContent::Image(img) => Some(img.alignment),
            InlineContent::Shape(shape) => Some(shape.alignment),
            _ => None,
        },
        _ => None,
    }
}

/// Approximate version of `get_item_vertical_metrics` for use without constraints (e.g. `bounds()`).
/// Uses 80/20 ascent/descent ratio as fallback for empty-glyph strut case.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[must_use] pub fn get_item_vertical_metrics_approx(item: &ShapedItem) -> (f32, f32) {
    // For non-empty clusters, delegate to the font-metrics-based calculation
    if let ShapedItem::Cluster(c) = item {
        if !c.glyphs.is_empty() {
            // Reuse the glyph-based calculation (same as get_item_vertical_metrics)
            let (asc, desc) = c.glyphs
                .iter()
                .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                    let metrics = &glyph.font_metrics;
                    if metrics.units_per_em == 0 {
                        return (max_asc, max_desc);
                    }
                    let scale = glyph.style.font_size_px / f32::from(metrics.units_per_em);
                    let font_ascent = metrics.ascent * scale;
                    let font_descent = (-metrics.descent * scale).max(0.0);
                    let ad = font_ascent + font_descent;
                    let resolved_lh = c.style.line_height.resolve_with_metrics(glyph.style.font_size_px, &glyph.font_metrics);
                    let half_leading = (resolved_lh - ad) / 2.0;
                    (max_asc.max(font_ascent + half_leading), max_desc.max(font_descent + half_leading))
                });
            return (asc, desc);
        }
    }
    // Fallback for empty glyphs or non-cluster items
    match item {
        ShapedItem::Cluster(c) => {
            let lh = c.style.line_height.resolve(c.style.font_size_px, 0.0, 0.0, 0.0, 0);
            (lh * FALLBACK_ASCENT_RATIO, lh * FALLBACK_DESCENT_RATIO)
        }
        ShapedItem::CombinedBlock { bounds, .. } => {
            (bounds.height * FALLBACK_ASCENT_RATIO, bounds.height * FALLBACK_DESCENT_RATIO)
        }
        ShapedItem::Object { bounds, .. } => (bounds.height, 0.0),
        ShapedItem::Tab { bounds, .. } => {
            (bounds.height * FALLBACK_ASCENT_RATIO, bounds.height * FALLBACK_DESCENT_RATIO)
        }
        ShapedItem::Break { .. } => (0.0, 0.0),
    }
}

/// Gets the ascent (distance from baseline to top) and descent (distance from baseline to bottom)
/// for a single item, incorporating half-leading from line-height.
///
// +spec:box-model:37aeb2 - inline box margins/borders/padding do not affect line box height (leading model)
// +spec:display-property:184f0d - Inline box baseline derives from first available font metrics
// +spec:display-property:238bf5 - Inline box layout bounds from own text metrics, not child boxes
// +spec:display-property:29b194 - baseline determination for inline boxes (CSS Box Alignment 3 §9.1)
// +spec:display-property:2987db - per-glyph font metrics impact inline box layout bounds (line-height: normal caveat not yet distinguished)
/// +spec:display-property:fd42a9 - line-height affects line box contribution, not inline box size
// +spec:font-metrics:506abb - A/D from font metrics with half-leading: L = line-height - (A+D), A' = A + L/2, D' = D + L/2
// +spec:font-metrics:773029 - ascent/descent font metrics used for baseline calculations (visual centering depends on these)
// +spec:font-metrics:f42870 - half-leading model: leading = line-height - (ascent + descent), distributed equally above/below
// +spec:writing-modes:531c2e - UAs should use vertical baseline tables in vertical typographic modes
#[must_use] pub fn get_item_vertical_metrics(item: &ShapedItem, constraints: &UnifiedConstraints) -> (f32, f32) {
    // (ascent, descent)
    match item {
        ShapedItem::Cluster(c) => {
            if c.glyphs.is_empty() {
                // +spec:display-property:626c86 - strut for inline box with no glyphs uses first available font metrics
                // +spec:line-height:0078fa - strut: zero-width inline box with element's font/line-height
                // §10.8.1 strut: if inline box contains no glyphs, it is considered to
                // contain a strut with A and D of the element's first available font.
                // Half-leading: L = line-height - (A + D), A' = A + L/2, D' = D + L/2
                let ad = constraints.strut_ascent + constraints.strut_descent;
                let resolved_lh = c.style.line_height.resolve(c.style.font_size_px, 0.0, 0.0, 0.0, 0);
                let half_leading = (resolved_lh - ad) / 2.0;
                return (constraints.strut_ascent + half_leading, constraints.strut_descent + half_leading);
            }
            // +spec:box-model:0b3e1f - inline non-replaced box height uses only line-height, not vertical padding/border/margin
            // +spec:display-property:80b900 - fallback glyphs affect line box size via per-glyph metrics
            // +spec:display-property:d52f26 - layout bounds enclose all glyphs from highest A to deepest D
            // +spec:font-metrics:387751 - content area uses max ascenders/descenders across all fonts
            // +spec:font-metrics:790fd2 - half-leading: L = line-height - (A+D), A' = A + L/2, D' = D + L/2
            // +spec:line-height:1ae6f5 - line-height on non-replaced inline: half-leading model
            // +spec:line-height:0078fa - half-leading: L = line-height - (A+D), distributed equally above/below
            // +spec:line-height:32b3da - half-leading: L = line-height - AD, A' = A + L/2, D' = D + L/2
            // §10.8.1: for each glyph determine A, D from font metrics,
            // then L = line-height - (A + D), and adjust: A' = A + L/2, D' = D + L/2.
            // Note: L may be negative.
            // +spec:height-calculation:eb98b5 - multi-font normal line-height uses max across glyph metrics
            c.glyphs
                .iter()
                .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                    let metrics = &glyph.font_metrics;
                    if metrics.units_per_em == 0 {
                        return (max_asc, max_desc);
                    }
                    let scale = glyph.style.font_size_px / f32::from(metrics.units_per_em);
                    let a = metrics.ascent * scale;
                    // Descent in OpenType is typically negative, so we negate it to get a positive
                    // distance.
                    let d = (-metrics.descent * scale).max(0.0);
                    let ad = a + d;
                    let resolved_lh = glyph.style.line_height.resolve_with_metrics(glyph.style.font_size_px, &glyph.font_metrics);
                    let leading = resolved_lh - ad;
                    let half_leading = leading / 2.0;
                    let item_asc = a + half_leading;
                    let item_desc = d + half_leading;
                    (max_asc.max(item_asc), max_desc.max(item_desc))
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

// +spec:block-formatting-context:861155 - vertical-align affects vertical positioning inside line box for inline-level elements
/// Calculates the maximum ascent and descent for an entire line of items.
/// This determines the "line box" used for vertical alignment.
/// // +spec:display-contents:66d910 - line box height fitted to contents, controlled by line-height
// +spec:inline-formatting-context:c3fc54 - line box tall enough for all boxes, vertical-align determines alignment within line box
///
/// Per CSS 2.2 §10.8: Inline-level boxes aligned 'top' or 'bottom' must be aligned
/// so as to minimize the line box height. The algorithm is:
/// 1. First pass: compute line box height from baseline-aligned items only
///    (baseline, sub, super, middle, text-top, text-bottom, offset).
/// 2. Second pass: check if any top/bottom-aligned items are taller than the
///    line box from pass 1, and expand if necessary.
// +spec:box-model:c9bcd7 - when line-fit-edge is not leading, layout bounds inflated by margin+border+padding (not yet implemented; default leading behavior is correct)
fn calculate_line_metrics(
    items: &[ShapedItem],
    default_vertical_align: VerticalAlign,
    constraints: &UnifiedConstraints,
) -> (f32, f32) {
    // +spec:font-metrics:95152b - baseline alignment: items with different font sizes aligned by matching alphabetic baselines
    // Pass 1: Compute ascent/descent from baseline-aligned items only
    // (i.e., items that are NOT vertical-align: top or bottom).
    let (mut max_asc, mut max_desc) = items
        .iter()
        .fold((0.0f32, 0.0f32), |(max_asc, max_desc), item| {
            let effective_align = get_item_vertical_align(item)
                .unwrap_or(default_vertical_align);
            match effective_align {
                VerticalAlign::Top | VerticalAlign::Bottom => {
                    // Skip top/bottom items in first pass
                    (max_asc, max_desc)
                }
                _ => {
                    let (item_asc, item_desc) = get_item_vertical_metrics(item, constraints);
                    (max_asc.max(item_asc), max_desc.max(item_desc))
                }
            }
        });

    let baseline_line_height = max_asc + max_desc;

    // Pass 2: Check top/bottom aligned items. If any of them is taller
    // than the current line box, expand the line box to fit.
    for item in items {
        let effective_align = get_item_vertical_align(item)
            .unwrap_or(default_vertical_align);
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

/// Performs layout for a single fragment, consuming items from a `BreakCursor`.
///
/// This function contains the core line-breaking and positioning logic, but is
/// designed to operate on a portion of a larger content stream and within the
/// constraints of a single geometric area (a fragment).
///
/// The loop terminates when either the fragment is filled (e.g., runs out of
/// vertical space) or the content stream managed by the `cursor` is exhausted.
///
/// # CSS Inline Layout Module Level 3 Implementation
///
/// This function implements the inline formatting context as described in:
/// <https://www.w3.org/TR/css-inline-3/#inline-formatting-context>
///
/// ## § 2.1 Layout of Line Boxes
/// "In general, the line-left edge of a line box touches the line-left edge of its
/// containing block and the line-right edge touches the line-right edge of its
/// containing block, and thus the logical width of a line box is equal to the inner
/// logical width of its containing block."
///
/// [ISSUE] `available_width` should be set to the containing block's inner width,
/// but is currently defaulting to 0.0 in `UnifiedConstraints::default()`.
/// This causes premature line breaking.
///
/// ## § 2.2 Layout Within Line Boxes
/// The layout process follows these steps:
/// 1. Baseline Alignment: All inline-level boxes are aligned by their baselines
/// 2. Content Size Contribution: Calculate layout bounds for each box
/// 3. Line Box Sizing: Size line box to fit aligned layout bounds
/// 4. Content Positioning: Position boxes within the line box
///
/// ## Missing Features:
/// - § 3 Baselines and Alignment Metrics: Only basic baseline alignment implemented
/// - § 4 Baseline Alignment: vertical-align property not fully supported
/// - § 5 Line Spacing: line-height implemented, but line-fit-edge missing
/// - § 6 Trimming Leading: text-box-trim not implemented
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if fragment layout fails.
pub fn perform_fragment_layout<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<'_>,
    logical_items: &[LogicalItem],
    fragment_constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Result<UnifiedLayout, LayoutError> {
    const MAX_EMPTY_SEGMENTS: usize = 1000; // Maximum allowed consecutive empty segments
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering perform_fragment_layout ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Constraints: available_width={:?}, available_height={:?}, columns={}, text_wrap={:?}",
            fragment_constraints.available_width,
            fragment_constraints.available_height,
            fragment_constraints.columns,
            fragment_constraints.text_wrap
        )));
    }

    // For TextWrap::Balance, use Knuth-Plass algorithm for optimal line breaking
    // This produces more visually balanced lines at the cost of more computation
    if fragment_constraints.text_wrap == TextWrap::Balance {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Using Knuth-Plass algorithm for text-wrap: balance".to_string(),
            ));
        }

        // Get the shaped items from the cursor
        let shaped_items: Vec<ShapedItem> = cursor.drain_remaining();

        // +spec:line-breaking:90c1bd - only auto-hyphenate when language is known and hyphenation resource available
        let hyphenator = if fragment_constraints.hyphenation == Hyphens::Auto {
            fragment_constraints
                .hyphenation_language
                .and_then(|lang| get_hyphenator(lang).ok())
        } else {
            None
        };

        // Use the Knuth-Plass algorithm for optimal line breaking
        return Ok(crate::text3::knuth_plass::kp_layout(
            &shaped_items,
            logical_items,
            fragment_constraints,
            hyphenator.as_ref(),
            fonts,
        ));
    }

    // +spec:intrinsic-sizing:57e02d - hyphenation opportunities considered in min-content sizing
    let hyphenator = if fragment_constraints.hyphenation == Hyphens::Auto {
        fragment_constraints
            .hyphenation_language
            .and_then(|lang| get_hyphenator(lang).ok())
    } else {
        None
    };

    let mut positioned_items = Vec::new();
    let mut layout_bounds = Rect::default();

    let num_columns = fragment_constraints.columns.max(1);
    let total_column_gap = fragment_constraints.column_gap * (num_columns - 1) as f32;

    // CSS Inline Layout § 2.1: "the logical width of a line box is equal to the inner
    // logical width of its containing block"
    //
    // Handle the different available space modes:
    // - Definite(width): Use the specified width for column calculation
    // - MinContent: Force line breaks at word boundaries, return widest word width
    // - MaxContent: Use a large value to allow content to expand naturally
    //
    // IMPORTANT: For MinContent, we do NOT use 0.0 (which would break after every character).
    // Instead, we use a large width but track the is_min_content flag to force word-level
    // line breaks in the line breaker. The actual min-content width is the width of the
    // widest resulting line (typically the widest word).
    let is_min_content = matches!(fragment_constraints.available_width, AvailableSpace::MinContent);
    let is_max_content = matches!(fragment_constraints.available_width, AvailableSpace::MaxContent);
    
    let column_width = match fragment_constraints.available_width {
        AvailableSpace::Definite(width) => (width - total_column_gap) / num_columns as f32,
        AvailableSpace::MinContent | AvailableSpace::MaxContent => {
            // For intrinsic sizing, use a large width to measure actual content width.
            // The line breaker will handle MinContent specially by breaking after each word.
            f32::MAX / 2.0
        }
    };
    let mut current_column = 0;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Column width calculated: {column_width}"
        )));
    }

    // Use the CSS direction from constraints instead of auto-detecting from text
    // This ensures that mixed-direction text (e.g., "مرحبا - Hello") uses the
    // correct paragraph-level direction for alignment purposes.
    // With unicode-bidi: plaintext, direction is auto-detected from text content
    // per CSS Writing Modes §8.3.
    let base_direction = if fragment_constraints.unicode_bidi == UnicodeBidi::Plaintext {
        // Auto-detect from remaining shaped items' text content
        let remaining = &cursor.items[cursor.next_item_index..];
        let text: String = remaining.iter()
            .filter_map(|i| i.as_cluster())
            .map(|c| c.text.as_str())
            .collect();
        match unicode_bidi::get_base_direction(text.as_str()) {
            unicode_bidi::Direction::Ltr => BidiDirection::Ltr,
            unicode_bidi::Direction::Rtl => BidiDirection::Rtl,
            // No strong character: fall back to containing block direction
            unicode_bidi::Direction::Mixed => fragment_constraints.direction.unwrap_or(BidiDirection::Ltr),
        }
    } else {
        fragment_constraints.direction.unwrap_or(BidiDirection::Ltr)
    };

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PFLayout] Base direction: {:?} (from CSS), Text align: {:?}",
            base_direction, fragment_constraints.text_align
        )));
    }

    'column_loop: while current_column < num_columns {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "\n-- Starting Column {current_column} --"
            )));
        }
        let column_start_x =
            (column_width + fragment_constraints.column_gap) * current_column as f32;
        let mut line_top_y = 0.0;
        let mut line_index = 0;
        let mut empty_segment_count = 0; // Failsafe counter for infinite loops
        let mut is_after_forced_break = false;

        // [g147 az-web-lift] Hard total-iteration cap on the line-build loop. On the remill lift,
        // `cursor.is_done()` (or the empty-segment failsafe) mis-lifts for the NESTED IFC (content.len
        // reads 0 → the cursor is starved but never reports done) → this `while !cursor.is_done()` spins
        // forever → solveLayoutReal HANGS inside perform_fragment_layout. Cap total iterations so the loop
        // always converges (the harness can then read the markers). native is unaffected (far above real
        // line counts). The 0x60BC4 marker exposes the iteration count.
        #[allow(clippy::no_effect_underscore_binding)] // web_lift-gated debug iteration counter
        let mut _az_line_iters: usize = 0;
        while !cursor.is_done() {
            #[cfg(feature = "web_lift")]
            {
                _az_line_iters += 1;
                unsafe { crate::az_mark((0x60BC4) as u32, (_az_line_iters as u32 | 0xC0DE0000) as u32); }
                if _az_line_iters > 4096 {
                    break;
                }
            }
            if let Some(max_height) = fragment_constraints.available_height {
                if line_top_y >= max_height {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Column full (pen {line_top_y} >= height {max_height}), breaking to next column."
                        )));
                    }
                    break;
                }
            }

            if let Some(clamp) = fragment_constraints.line_clamp {
                if line_index >= clamp.get() {
                    break;
                }
            }

            // Create constraints specific to the current column for the line breaker.
            let mut column_constraints = fragment_constraints.clone();
            // For MinContent/MaxContent, preserve the semantic type so the line breaker
            // can handle word-level breaking correctly. Only use Definite for actual widths.
            if is_min_content {
                column_constraints.available_width = AvailableSpace::MinContent;
            } else if is_max_content {
                column_constraints.available_width = AvailableSpace::MaxContent;
            } else {
                column_constraints.available_width = AvailableSpace::Definite(column_width);
            }
            let line_constraints = get_line_constraints(
                line_top_y,
                fragment_constraints.resolved_line_height(),
                &column_constraints,
                debug_messages,
            );

            if line_constraints.segments.is_empty() {
                empty_segment_count += 1;
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  No available segments at y={line_top_y}, skipping to next line. (empty count: \
                         {empty_segment_count}/{MAX_EMPTY_SEGMENTS})"
                    )));
                }

                // Failsafe: If we've skipped too many lines without content, break out
                if empty_segment_count >= MAX_EMPTY_SEGMENTS {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  [WARN] Reached maximum empty segment count ({MAX_EMPTY_SEGMENTS}). Breaking to \
                             prevent infinite loop."
                        )));
                        msgs.push(LayoutDebugMessage::warning(
                            "  This likely means the shape constraints are too restrictive or \
                             positioned incorrectly."
                                .to_string(),
                        ));
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  Current y={line_top_y}, shape boundaries might be outside this range."
                        )));
                    }
                    break;
                }

                // Additional check: If we have shapes and are far beyond the expected height,
                // also break to avoid infinite loops
                if !fragment_constraints.shape_boundaries.is_empty() && empty_segment_count > 50 {
                    // Calculate maximum shape height
                    let max_shape_y: f32 = fragment_constraints
                        .shape_boundaries
                        .iter()
                        .map(|shape| {
                            match shape {
                                ShapeBoundary::Circle { center, radius } => center.y + radius,
                                ShapeBoundary::Ellipse { center, radii } => center.y + radii.height,
                                ShapeBoundary::Polygon { points } => {
                                    points.iter().map(|p| p.y).fold(0.0, f32::max)
                                }
                                ShapeBoundary::Rectangle(rect) => rect.y + rect.height,
                                ShapeBoundary::Path { segments } => segments
                                    .iter()
                                    .filter_map(|s| match s {
                                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => Some(p.y),
                                        PathSegment::CurveTo { end, .. }
                                        | PathSegment::QuadTo { end, .. } => Some(end.y),
                                        PathSegment::Arc { center, radius, .. } => {
                                            Some(center.y + radius)
                                        }
                                        PathSegment::Close => None,
                                    })
                                    .fold(0.0, f32::max),
                            }
                        })
                        .fold(0.0, f32::max);

                    if line_top_y > max_shape_y + 100.0 {
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "  [INFO] Current y={line_top_y} is far beyond maximum shape extent y={max_shape_y}. \
                                 Breaking layout."
                            )));
                            msgs.push(LayoutDebugMessage::info(
                                "  Shape boundaries exist but no segments available - text cannot \
                                 fit in shape."
                                    .to_string(),
                            ));
                        }
                        break;
                    }
                }

                line_top_y += fragment_constraints.resolved_line_height();
                continue;
            }

            // Reset counter when we find valid segments
            empty_segment_count = 0;

            // +spec:line-breaking:3bb032 - break-word not considered for min-content intrinsic sizes
            // +spec:overflow:b932c4 - overflow-wrap/word-wrap (normal/break-word/anywhere) and hyphens interaction
            // `anywhere` introduces soft wrap opportunities (min-content = widest cluster),
            // but `break-word` does NOT (min-content = widest unbreakable word).
            let effective_overflow_wrap = if is_min_content && fragment_constraints.overflow_wrap == OverflowWrap::Anywhere {
                OverflowWrap::Anywhere
            } else if is_min_content && fragment_constraints.overflow_wrap == OverflowWrap::BreakWord {
                OverflowWrap::Normal
            } else {
                fragment_constraints.overflow_wrap
            };

            // CSS Text Module Level 3 § 5 Line Breaking and Word Boundaries
            // https://www.w3.org/TR/css-text-3/#line-breaking
            // +spec:display-property:2608cc - inline box splitting across line boxes, overflow for unsplittable boxes
            // +spec:display-property:ea615c - inline boxes split and distributed across line boxes
            // "When an inline box exceeds the logical width of a line box, it is split
            // into several fragments, which are partitioned across multiple line boxes."
            let (mut line_items, was_hyphenated) =
                break_one_line(cursor, &line_constraints, false, hyphenator.as_ref(), fonts, fragment_constraints.line_break, fragment_constraints.white_space_mode, effective_overflow_wrap);
            if line_items.is_empty() {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Break returned no items. Ending column.".to_string(),
                    ));
                }
                break;
            }

            let line_text_before_rev: String = line_items
                .iter()
                .filter_map(|i| i.as_cluster())
                .map(|c| c.text.as_str())
                .collect();
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    // FIX: The log message was misleading. Items are in visual order.
                    "[PFLayout] Line items from breaker (visual order): [{line_text_before_rev}]"
                )));
            }

            // +spec:line-breaking:c59944 - forced line breaks detected for bidi-aware alignment
            let line_ends_with_forced_break = line_items.iter().any(|item| matches!(item, ShapedItem::Break { .. }));

            // uses text-align-last (last line of block, or line right before forced break)
            let is_last_line = cursor.is_done() && !was_hyphenated;
            let effective_align = resolve_effective_alignment(
                fragment_constraints.text_align,
                fragment_constraints.text_align_last,
                is_last_line || line_ends_with_forced_break,
            );

            let (mut line_pos_items, line_height) = position_one_line(
                &line_items,
                &line_constraints,
                line_top_y,
                line_index,
                effective_align,
                base_direction,
                is_last_line,
                fragment_constraints,
                debug_messages,
                fonts,
                is_after_forced_break,
            );

            // Track whether the next line follows a forced break
            is_after_forced_break = line_ends_with_forced_break;

            for item in &mut line_pos_items {
                item.position.x += column_start_x;
            }

            // +spec:display-property:6c4978 - line-height on block container establishes minimum line box height
            line_top_y += line_height.max(fragment_constraints.resolved_line_height());
            line_index += 1;
            positioned_items.extend(line_pos_items);
        }
        current_column += 1;
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting perform_fragment_layout, positioned {} items ---",
            positioned_items.len()
        )));
    }

    let mut layout = UnifiedLayout {
        items: positioned_items,
        overflow: OverflowInfo::default(),
    };

    // Calculate bounds on demand via the bounds() method
    let calculated_bounds = layout.bounds();

    // Record the unclipped content bounds. `overflow_items` stays empty by
    // design: this positioner places *every* item, so visual overflow is handled
    // at paint time via clipping rather than by dropping items here.
    // TODO(superplan): only populate `overflow_items` if a future positioning
    // path actually discards content that does not fit.
    layout.overflow.unclipped_bounds = calculated_bounds;

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Calculated bounds: width={}, height={} ---",
            calculated_bounds.width, calculated_bounds.height
        )));
    }

    Ok(layout)
}

/// Breaks a single line of items to fit within the given geometric constraints,
/// handling multi-segment lines and hyphenation.
/// Break a single line from the current cursor position.
///
/// # CSS Text Module Level 3 \u00a7 5 Line Breaking and Word Boundaries
/// <https://www.w3.org/TR/css-text-3/#line-breaking>
///
/// Implements the line breaking algorithm:
/// 1. "When an inline box exceeds the logical width of a line box, it is split into several
///    fragments, which are partitioned across multiple line boxes."
///
/// ## \u2705 Implemented Features:
/// - **Break Opportunities**: Identifies word boundaries and break points
/// - **Soft Wraps**: Wraps at spaces between words
/// - **Hard Breaks**: Handles explicit line breaks (\\n)
/// - **Overflow**: If a word is too long, places it anyway to avoid infinite loop
/// - **Hyphenation**: Tries to break long words at hyphenation points (\u00a7 5.4)
///
/// ## \u26a0\ufe0f Known Issues:
/// - If `line_constraints.total_available` is 0.0 (from `available_width: 0.0` bug), every word
///   will overflow, causing single-word lines
/// - This is the symptom visible in the PDF: "List items break extremely early"
///
/// ## \u00a7 5.2 Breaking Rules for Letters
/// \u2705 IMPLEMENTED: Uses Unicode line breaking algorithm
/// - Relies on UAX #14 for break opportunities
/// - Respects non-breaking spaces and zero-width joiners
///
/// ## \u00a7 5.3 Breaking Rules for Punctuation
/// \u26a0\ufe0f PARTIAL: Basic punctuation handling
/// - \u274c TODO: hanging-punctuation is declared in `UnifiedConstraints` but not used here
/// - \u274c TODO: Should implement punctuation trimming at line edges
///   // +spec:intrinsic-sizing:6085cf - hanging glyphs must be excluded from intrinsic size computation
///
/// ## \u00a7 5.4 Hyphenation
/// \u2705 IMPLEMENTED: Automatic hyphenation with hyphenator library
/// - Tries to hyphenate words that overflow
/// - Inserts hyphen glyph at break point
/// - Carries remainder to next line
///
/// ## \u00a7 5.5 Overflow Wrapping
/// \u2705 IMPLEMENTED: Emergency breaking
/// - If line is empty and word doesn't fit, forces at least one item
/// - Prevents infinite loop
/// - This is "overflow-wrap: break-word" behavior
///
/// # Missing Features:
/// - word-break property (normal, break-all, keep-all) - IMPLEMENTED via `BreakCursor.word_break`
/// - \u26a0\ufe0f line-break property: anywhere implemented; loose/normal/strict CJK strictness
///   filtering added via `is_cjk_break_allowed_by_strictness` (§5.3)
/// - \u274c overflow-wrap: anywhere vs break-word distinction
/// - \u2705 white-space: break-spaces handling
// around every typographic character unit including preserved white spaces; with break-spaces
// it allows breaking before the first space of a sequence
// +spec:line-breaking:722f3b - wrapping only at soft wrap opportunities, minimizing overflow
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if a break unit is unexpectedly empty (an internal invariant).
pub fn break_one_line<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<'_>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
    line_break: LineBreakStrictness,
    white_space_mode: WhiteSpaceMode,
    overflow_wrap: OverflowWrap,
) -> (Vec<ShapedItem>, bool) {
    let mut line_items = Vec::new();
    let mut current_width = 0.0;

    if cursor.is_done() {
        return (Vec::new(), false);
    }

    // +spec:white-space-processing:c83dbd - Phase II: collapsible spaces at line start removed, trailing spaces removed, tab stops
    // CSS Text Module Level 3 § 4.1.2: At the beginning of a line, white space
    // is collapsed away. Skip leading whitespace at line start.
    // https://www.w3.org/TR/css-text-3/#white-space-phase-2
    let break_spaces = white_space_mode == WhiteSpaceMode::BreakSpaces;
    if !break_spaces {
        while !cursor.is_done() {
            let next_unit = cursor.peek_next_unit();
            if next_unit.is_empty() {
                break;
            }
            if next_unit.len() == 1 && is_collapsible_whitespace(&next_unit[0]) {
                cursor.consume(1);
            } else {
                break;
            }
        }
    }

    // +spec:line-breaking:35817b - white-space: nowrap/pre prevent soft wrap opportunities
    // CSS Text Level 3 § 3: For nowrap and pre, wrapping is suppressed. All content
    // stays on a single line, overflowing if necessary.
    let no_wrap = matches!(white_space_mode, WhiteSpaceMode::Nowrap | WhiteSpaceMode::Pre);

    if no_wrap {
        // No soft wrapping — consume everything onto one line.
        // Only explicit <br>/newline breaks are honored.
        loop {
            let next_unit = cursor.peek_next_unit();
            if next_unit.is_empty() {
                break;
            }
            if let Some(ShapedItem::Break { .. }) = next_unit.first() {
                line_items.push(next_unit[0].clone());
                cursor.consume(1);
                return (line_items, false);
            }
            line_items.extend_from_slice(&next_unit);
            cursor.consume(next_unit.len());
        }
    } else {

    loop {
        // typographic character unit as a soft wrap opportunity; hyphenation is not applied
        let next_unit = if line_break == LineBreakStrictness::Anywhere {
            cursor.peek_next_single_item()
        } else {
            cursor.peek_next_unit()
        };
        if next_unit.is_empty() {
            break; // End of content
        }

        if let Some(ShapedItem::Break { .. }) = next_unit.first() {
            line_items.push(next_unit[0].clone());
            cursor.consume(1);
            return (line_items, false);
        }

        let unit_width: f32 = next_unit
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();
        let available_width = line_constraints.total_available - current_width;

        // 2. Can the whole unit fit on the current line?
        if unit_width <= available_width {
            line_items.extend_from_slice(&next_unit);
            current_width += unit_width;
            cursor.consume(next_unit.len());
        } else {
            // 3. The unit overflows. Can we hyphenate it?
            if line_break != LineBreakStrictness::Anywhere {
                if let Some(hyphenator) = hyphenator {
                    if !is_break_opportunity(next_unit.last().unwrap()) {
                        if let Some(hyphenation_result) = try_hyphenate_word_cluster(
                            &next_unit,
                            available_width,
                            is_vertical,
                            hyphenator,
                            fonts,
                        ) {
                            line_items.extend(hyphenation_result.line_part);
                            cursor.consume(next_unit.len());
                            cursor.partial_remainder = hyphenation_result.remainder_part;
                            return (line_items, true);
                        }
                    }
                }
            }

            // an otherwise unbreakable sequence at an arbitrary point when no other
            // break points exist. Grapheme clusters stay together; no hyphen inserted.
            // 4. Cannot hyphenate or fit. The line is finished.
            // If the line is empty, we must force at least one item to avoid an infinite loop.
            // With overflow-wrap: anywhere or break-word, we break the unbreakable
            // unit at an arbitrary cluster boundary. With normal, we only force one
            // item to prevent infinite loops (content will overflow).
            if line_items.is_empty() {
                match overflow_wrap {
                    OverflowWrap::Anywhere | OverflowWrap::BreakWord => {
                        // Emergency break: fit as many clusters as possible on
                        // this line.  Grapheme clusters stay together.
                        //
                        // Per CSS Text 3 §5.5: "an otherwise unbreakable sequence
                        // of characters may be broken at an arbitrary point" when
                        // overflow-wrap is anywhere/break-word.
                        let avail = line_constraints.total_available;
                        for item in &next_unit {
                            let item_w = get_item_measure(item, is_vertical);
                            // Break BEFORE this item if adding it would overflow,
                            // but only if we already have at least one item on the
                            // line (must always make progress).
                            if !line_items.is_empty() && avail > 0.0 && current_width + item_w > avail {
                                break;
                            }
                            line_items.push(item.clone());
                            current_width += item_w;
                            // When the container is zero-width (avail <= 0), the
                            // break-before check above is skipped (it requires
                            // avail > 0), so every item lands on this one line —
                            // there's nowhere to break TO, content just overflows.
                            // This matches browser behavior for `width: 0`
                            // containers.
                        }
                        let consumed = line_items.len().max(1);
                        if line_items.is_empty() {
                            line_items.push(next_unit[0].clone());
                        }
                        cursor.consume(consumed);
                    }
                    OverflowWrap::Normal => {
                        // No emergency breaking — just force one item to prevent infinite loop
                        line_items.push(next_unit[0].clone());
                        cursor.consume(1);
                    }
                }
            }
            break;
        }
    }

    } // end !no_wrap

    // +spec:white-space-processing:fef250 - Phase II: trailing collapsible spaces and U+1680 removed at line end
    // as well as any trailing U+1680 OGHAM SPACE MARK whose white-space is normal/nowrap/pre-line.
    // Note: pre-wrap and break-spaces have different handling (hanging/preserving)
    // which is not yet implemented here.
    while let Some(last) = line_items.last() {
        if is_collapsible_whitespace(last) {
            line_items.pop();
        } else {
            break;
        }
    }

    (line_items, false)
}

/// Represents a single valid hyphenation point within a word.
#[derive(Debug, Clone)]
pub struct HyphenationBreak {
    /// The number of characters from the original word string included on the line.
    pub char_len_on_line: usize,
    /// The total advance width of the line part + the hyphen.
    pub width_on_line: f32,
    /// The cluster(s) that will remain on the current line.
    pub line_part: Vec<ShapedItem>,
    /// The cluster that represents the hyphen character itself.
    pub hyphen_item: ShapedItem,
    /// The cluster(s) that will be carried over to the next line.
    /// CRITICAL FIX: Changed from `ShapedItem` to Vec<ShapedItem>
    pub remainder_part: Vec<ShapedItem>,
}

/// A "word" is defined as a sequence of one or more adjacent `ShapedClusters`.
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
/// # Panics
///
/// Panics if a word's cluster or glyph list is unexpectedly empty (an internal invariant).
#[must_use] pub fn find_all_hyphenation_breaks<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster],
    hyphenator: &Standard,
    is_vertical: bool, // Pass this in to use correct metrics
    fonts: &LoadedFonts<T>,
) -> Option<Vec<HyphenationBreak>> {
    if word_clusters.is_empty() {
        return None;
    }

    // --- 1. Concatenate the TRUE text and build a robust map ---
    let mut word_string = String::new();
    let mut char_map = Vec::new();
    let mut current_width = 0.0;

    for (cluster_idx, cluster) in word_clusters.iter().enumerate() {
        for (char_byte_offset, _ch) in cluster.text.char_indices() {
            let glyph_idx = cluster
                .glyphs
                .iter()
                .rposition(|g| g.cluster_offset as usize <= char_byte_offset)
                .unwrap_or(0);
            let glyph = &cluster.glyphs[glyph_idx];

            let num_chars_in_glyph = cluster.text[glyph.cluster_offset as usize..]
                .chars()
                .count();
            let advance_per_char = if is_vertical {
                glyph.vertical_advance
            } else {
                glyph.advance
            } / (num_chars_in_glyph as f32).max(1.0);

            current_width += advance_per_char;
            char_map.push((cluster_idx, glyph_idx, current_width));
        }
        word_string.push_str(&cluster.text);
    }

    // +spec:line-breaking:d7ed93 - language-specific hyphenation rules apply to both auto and explicit (soft hyphen) opportunities
    // --- 2. Get hyphenation opportunities ---
    let opportunities = hyphenator.hyphenate(&word_string);
    if opportunities.breaks.is_empty() {
        return None;
    }

    let last_cluster = word_clusters.last().unwrap();
    let last_glyph = last_cluster.glyphs.last().unwrap();
    let style = last_cluster.style.clone();

    // Look up font from hash
    let font = fonts.get_by_hash(last_glyph.font_hash)?;
    let (hyphen_glyph_id, hyphen_advance) =
        font.get_hyphen_glyph_and_advance(style.font_size_px)?;

    let mut possible_breaks = Vec::new();

    // --- 3. Generate a HyphenationBreak for each valid opportunity ---
    for &break_char_idx in &opportunities.breaks {
        // The break is *before* the character at this index.
        // So the last character on the line is at `break_char_idx - 1`.
        if break_char_idx == 0 || break_char_idx > char_map.len() {
            continue;
        }

        let (_, _, width_at_break) = char_map[break_char_idx - 1];

        // The line part is all clusters *before* the break index.
        let line_part: Vec<ShapedItem> = word_clusters[..break_char_idx]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        // The remainder is all clusters *from* the break index onward.
        let remainder_part: Vec<ShapedItem> = word_clusters[break_char_idx..]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        let hyphen_item = ShapedItem::Cluster(ShapedCluster {
            text: "-".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Hyphen is generated, not from DOM
            glyphs: smallvec![ShapedGlyph {
                kind: GlyphKind::Hyphen,
                glyph_id: hyphen_glyph_id,
                font_hash: last_glyph.font_hash,
                font_metrics: last_glyph.font_metrics,
                cluster_offset: 0,
                script: Script::Latin,
                advance: hyphen_advance,
                kerning: 0.0,
                offset: Point::default(),
                style: style.clone(),
                vertical_advance: hyphen_advance,
                vertical_offset: Point::default(),
            }],
            advance: hyphen_advance,
            direction: BidiDirection::Ltr,
            style: style.clone(),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        });

        possible_breaks.push(HyphenationBreak {
            char_len_on_line: break_char_idx,
            width_on_line: width_at_break + hyphen_advance,
            line_part,
            hyphen_item,
            remainder_part,
        });
    }

    Some(possible_breaks)
}

/// Tries to find a hyphenation point within a word, returning the line part and remainder.
fn try_hyphenate_word_cluster<T: ParsedFontTrait>(
    word_items: &[ShapedItem],
    remaining_width: f32,
    is_vertical: bool,
    hyphenator: &Standard,
    fonts: &LoadedFonts<T>,
) -> Option<HyphenationResult> {
    let word_clusters: Vec<ShapedCluster> = word_items
        .iter()
        .filter_map(|item| item.as_cluster().cloned())
        .collect();

    if word_clusters.is_empty() {
        return None;
    }

    let all_breaks = find_all_hyphenation_breaks(&word_clusters, hyphenator, is_vertical, fonts)?;

    if let Some(best_break) = all_breaks
        .into_iter()
        .rfind(|b| b.width_on_line <= remaining_width)
    {
        let mut line_part = best_break.line_part;
        line_part.push(best_break.hyphen_item);

        return Some(HyphenationResult {
            line_part,
            remainder_part: best_break.remainder_part,
        });
    }

    None
}

/// Positions a single line of items, handling alignment and justification within segments.
///
/// This function is architecturally critical for cache safety. It does not mutate the
/// `advance` or `bounds` of the input `ShapedItem`s. Instead, it applies justification
/// spacing by adjusting the drawing pen's position (`main_axis_pen`).
///
/// # Returns
/// A tuple containing the `Vec` of positioned items and the calculated height of the line box.
/// Position items on a single line after breaking.
///
/// # CSS Inline Layout Module Level 3 \u00a7 2.2 Layout Within Line Boxes
/// <https://www.w3.org/TR/css-inline-3/#layout-within-line-boxes>
///
/// Implements the positioning algorithm:
/// 1. "All inline-level boxes are aligned by their baselines"
/// 2. "Calculate layout bounds for each inline box"
/// 3. "Size the line box to fit the aligned layout bounds"
/// 4. "Position all inline boxes within the line box"
///
/// ## \u2705 Implemented Features:
///
/// ### \u00a7 4 Baseline Alignment (vertical-align)
/// \u26a0\ufe0f PARTIAL IMPLEMENTATION:
/// - \u2705 `baseline`: Aligns box baseline with parent baseline (default)
/// - \u2705 `top`: Aligns top of box with top of line box
/// - \u2705 `middle`: Centers box within line box
/// - \u2705 `bottom`: Aligns bottom of box with bottom of line box
/// - \u274c MISSING: `text-top`, `text-bottom`, `sub`, `super`
/// - \u274c MISSING: `<length>`, `<percentage>` values for custom offset
///
/// ### \u00a7 2.2.1 Text Alignment (text-align)
/// +spec:containing-block:8d5146 - text-align aligns within line box, not viewport/containing block
/// \u2705 IMPLEMENTED:
/// - `left`, `right`, `center`: Physical alignment
/// - `start`, `end`: Logical alignment (respects direction: ltr/rtl)
/// - `justify`: Distributes space between words/characters
/// - `justify-all`: Justifies last line too
///
/// ### \u00a7 7.3 Text Justification (text-justify)
/// \u2705 IMPLEMENTED:
/// - `inter-word`: Adds space between words
/// - `inter-character`: Adds space between characters
/// - `kashida`: Arabic kashida elongation
/// - \u274c MISSING: `distribute` (CJK justification)
///
/// ### CSS Text \u00a7 8.1 Text Indentation (text-indent)
/// \u2705 IMPLEMENTED: First line indentation
///
/// ### CSS Text \u00a7 4.1 Word Spacing (word-spacing)
/// \u2705 IMPLEMENTED: Additional space between words
///
/// ### CSS Text \u00a7 4.2 Letter Spacing (letter-spacing)
/// \u2705 IMPLEMENTED: Additional space between characters
///
/// ## Segment-Aware Layout:
/// \u2705 Handles CSS Shapes and multi-column layouts
/// - Breaks line into segments (for shape boundaries)
/// - Calculates justification per segment
/// - Applies alignment within each segment's bounds
///
/// ## Known Issues:
/// - \u26a0\ufe0f If segment.width is infinite (from intrinsic sizing), sets `alignment_offset=0` to
///   avoid infinite positioning. This is correct for measurement but documented for clarity.
/// - The function assumes `line_index == 0` means first line for text-indent. A more robust system
///   would track paragraph boundaries.
///
/// # Missing Features:
/// - \u274c \u00a7 6 Trimming Leading (text-box-trim, text-box-edge)
/// - \u274c \u00a7 3.3 Initial Letters (drop caps)
///   // +spec:display-property:265c04 - initial letter exclusion area must continue into subsequent blocks when paragraph is shorter than drop cap
/// - \u274c Full vertical-align support (sub, super, lengths, percentages)
/// - \u274c white-space: break-spaces alignment behavior
// +spec:text-alignment-spacing:c8a926 - order of operations: shaping → letter/word-spacing → justification → alignment
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn position_one_line<T: ParsedFontTrait>(
    line_items: &[ShapedItem],
    line_constraints: &LineConstraints,
    line_top_y: f32,
    line_index: usize,
    text_align: TextAlign,
    base_direction: BidiDirection,
    is_last_line: bool,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
    is_after_forced_break: bool,
) -> (Vec<PositionedItem>, f32) {
    let line_text: String = line_items
        .iter()
        .filter_map(|i| i.as_cluster())
        .map(|c| c.text.as_str())
        .collect();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering position_one_line for line: [{line_text}] ---"
        )));
    }
    // +spec:text-alignment-spacing:13b72d - line box start/end determined by inline base direction
    // +spec:text-alignment-spacing:d497af - line box inline base direction affects text-align resolution
    // +spec:text-alignment-spacing:68332e - bidi direction determines start/end to left/right mapping
    let physical_align = match (text_align, base_direction) {
        (TextAlign::Start, BidiDirection::Ltr) => TextAlign::Left,
        (TextAlign::Start, BidiDirection::Rtl) => TextAlign::Right,
        (TextAlign::End, BidiDirection::Ltr) => TextAlign::Right,
        (TextAlign::End, BidiDirection::Rtl) => TextAlign::Left,
        // Physical alignments are returned as-is, regardless of direction.
        (other, _) => other,
    };
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[Pos1Line] Physical align: {physical_align:?}"
        )));
    }

    // +spec:box-model:847003 - Phantom line boxes: empty lines treated as zero-height
    // +spec:box-model:d781f3 - empty line boxes (no text, no preserved whitespace, no inline elements with non-zero margins/padding/borders, no in-flow content) are treated as zero-height
    // +spec:display-property:90d782 - Phantom line boxes (containing only empty inline boxes, out-of-flow items, or collapsed whitespace) are ignored
    if line_items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut positioned = Vec::new();
    let is_vertical = constraints.is_vertical();

    // +spec:line-height:9ca9d9 - line box height = distance from uppermost box top to lowermost box bottom, including strut
    // The line box is calculated once for all items on the line, regardless of segment.
    // Per CSS 2.2 §10.8, top/bottom aligned items are handled in a second pass to
    // minimize line box height; baseline-aligned items determine the initial height.
    let (content_ascent, content_descent) = calculate_line_metrics(line_items, constraints.vertical_align, constraints);

    // +spec:box-model:e99f7d - strut: each line box starts with zero-width inline box with block container's font/line-height
    // +spec:line-height:29c478 - strut: zero-width inline box with block container's font/line-height
    // inline box with the block container's font and line-height. The strut has A (ascent) and
    // D (descent) from the block container's first available font. Half-leading L/2 is applied:
    // L = line-height - (A + D), strut_above = A + L/2, strut_below = D + L/2.
    // +spec:height-calculation:8e91b2 - specified line-height used in line box height calculation
    let strut_ad = constraints.strut_ascent + constraints.strut_descent;
    let strut_leading_half = (constraints.resolved_line_height() - strut_ad) / 2.0;
    let strut_above = constraints.strut_ascent + strut_leading_half;
    let strut_below = constraints.strut_descent + strut_leading_half;
    let line_ascent = content_ascent.max(strut_above);
    let line_descent = content_descent.max(strut_below);
    let line_box_height = line_ascent + line_descent;

    // The baseline for the entire line is determined by its tallest item.
    let line_baseline_y = line_top_y + line_ascent;

    // --- Segment-Aware Positioning ---
    let mut item_cursor = 0;
    let is_first_line_of_para = line_index == 0; // Simplified assumption

    for (segment_idx, segment) in line_constraints.segments.iter().enumerate() {
        if item_cursor >= line_items.len() {
            break;
        }

        // 1. Collect all items that fit into the current segment.
        let mut segment_items = Vec::new();
        let mut current_segment_width = 0.0;
        while item_cursor < line_items.len() {
            let item = &line_items[item_cursor];
            let item_measure = get_item_measure(item, is_vertical);
            // Put at least one item in the segment to avoid getting stuck.
            if current_segment_width + item_measure > segment.width && !segment_items.is_empty() {
                break;
            }
            segment_items.push(item.clone());
            current_segment_width += item_measure;
            item_cursor += 1;
        }

        if segment_items.is_empty() {
            continue;
        }

        // +spec:text-alignment-spacing:b9d88e - justify stretches inline boxes via text-justify; non-collapsible WS may skip justification
        // 2. Calculate justification spacing *for this segment only*.
        // +spec:text-alignment-spacing:30d322 - justify lines with justification opportunities when text-align is justify
        // CSS Text 3 §6: text-justify controls HOW to justify, but only applies
        // when text-align is justify/justify-all. Without this check, ALL text
        // gets justified because text-justify defaults to auto (→ InterWord).
        let (extra_word_spacing, extra_char_spacing) = if (constraints.text_align == TextAlign::Justify
            || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::None
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::Kashida
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![*segment],
                total_available: segment.width,
            };
            calculate_justification_spacing(
                &segment_items,
                &segment_line_constraints,
                constraints.text_justify,
                is_vertical,
            )
        } else {
            (0.0, 0.0)
        };

        // Kashida justification needs to be segment-aware if used.
        let justified_segment_items = if constraints.text_justify == JustifyContent::Kashida
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![*segment],
                total_available: segment.width,
            };
            justify_kashida_and_rebuild(
                segment_items,
                &segment_line_constraints,
                is_vertical,
                debug_messages,
                fonts,
            )
        } else {
            segment_items
        };

        // Recalculate width in case kashida changed the item list
        let final_segment_width: f32 = justified_segment_items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        // +spec:line-breaking:155a96 - pre-wrap hanging spaces: unconditionally hang without forced break, conditionally hang with forced break
        // +spec:white-space-processing:68af09 - Phase II: trailing whitespace hanging/conditional hanging per white-space mode
        // +spec:white-space-processing:75d91e - preserved white space hangs at line end, affecting intrinsic sizing
        // +spec:overflow:a68394 - Hanging trailing whitespace: unconditionally hang (not considered
        // during alignment, may overflow) for lines without forced break; conditionally hang for
        // lines ending with forced break (only hang if would overflow).
        // For normal/nowrap/pre-line: unconditionally hang trailing WS.
        // For pre-wrap: unconditionally hang, unless before forced break (then conditionally hang).
        // For break-spaces: trailing spaces cannot hang.
        // For pre: no hanging (whitespace preserved as-is).
        // +spec:intrinsic-sizing:1db683 - conditionally hanging glyphs excluded from min-content, included in max-content
        let trailing_ws_width = match constraints.white_space_mode {
            WhiteSpaceMode::BreakSpaces | WhiteSpaceMode::Pre => 0.0,
            WhiteSpaceMode::Normal | WhiteSpaceMode::Nowrap | WhiteSpaceMode::PreLine => {
                measure_trailing_whitespace(&justified_segment_items, is_vertical)
            }
            // +spec:line-breaking:8aa426 - space before forced break does not hang if it doesn't overflow
            WhiteSpaceMode::PreWrap => {
                let has_forced_break = justified_segment_items.last()
                    .is_some_and(|item| matches!(item, ShapedItem::Break { .. }));
                let ws_width = measure_trailing_whitespace(&justified_segment_items, is_vertical);
                if has_forced_break {
                    // +spec:display-contents:2704a2 - conditionally hanging chars not considered when measuring line fit
                    // Conditionally hang: only hang if it would overflow
                    let content_width = final_segment_width - ws_width;
                    if content_width + ws_width > segment.width {
                        ws_width
                    } else {
                        0.0
                    }
                } else {
                    ws_width // unconditionally hang
                }
            }
        };
        let effective_segment_width = final_segment_width - trailing_ws_width;

        // +spec:text-alignment-spacing:287316 - overflow content is start-aligned; alignment offset within line box
        // 3. Calculate alignment offset *within this segment*.
        let remaining_space = segment.width - effective_segment_width;

        // Handle MaxContent/indefinite width: when available_width is MaxContent (for intrinsic
        // sizing), segment.width will be f32::MAX / 2.0. Alignment calculations would
        // produce huge offsets. In this case, treat as left-aligned (offset = 0) since
        // we're measuring natural content width. We check for both infinite AND very large
        // values (> 1e30) to catch the MaxContent case.
        let is_indefinite_width = segment.width.is_infinite() || segment.width > 1e30;
        // +spec:text-alignment-spacing:ab1d4f - unexpandable justify text aligns as center
        let alignment_offset = if is_indefinite_width {
            0.0 // No alignment offset for indefinite width
        } else {
            match physical_align {
                TextAlign::Center => remaining_space / 2.0,
                TextAlign::Right => remaining_space,
                TextAlign::Justify | TextAlign::JustifyAll
                    if remaining_space > 0.0
                        && extra_word_spacing == 0.0
                        && extra_char_spacing == 0.0 =>
                {
                    // CSS Text §6.4.3: If text cannot be stretched to full width
                    // and text-align-last is justify, align as center.
                    remaining_space / 2.0
                }
                _ => 0.0, // Left, Justify (when justification succeeded)
            }
        };

        let mut main_axis_pen = segment.start_x + alignment_offset;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[Pos1Line] Segment width: {}, Item width: {}, Remaining space: {}, Initial pen: \
                 {}",
                segment.width, final_segment_width, remaining_space, main_axis_pen
            )));
        }

        // Default: indent first line only. each-line: also indent after forced breaks.
        // hanging: invert which lines get the indent.
        if segment_idx == 0 {
            let is_indent_target = if constraints.text_indent_each_line {
                // each-line: first line AND each line after a forced break
                is_first_line_of_para || is_after_forced_break
            } else {
                // Default: only the first line of the block
                is_first_line_of_para
            };
            // hanging: inverts which lines are affected
            let should_indent = if constraints.text_indent_hanging {
                !is_indent_target
            } else {
                is_indent_target
            };
            if should_indent {
                main_axis_pen += constraints.text_indent;
            }
        }

        // Calculate total marker width for proper outside marker positioning
        // We need to position all marker clusters together in the padding gutter
        let total_marker_width: f32 = justified_segment_items
            .iter()
            .filter_map(|item| {
                if let ShapedItem::Cluster(c) = item {
                    if c.marker_position_outside == Some(true) {
                        return Some(get_item_measure(item, is_vertical));
                    }
                }
                None
            })
            .sum();

        // Track marker pen separately - starts at negative position for outside markers
        let marker_spacing = 4.0; // Small gap between marker and content
        let mut marker_pen = if total_marker_width > 0.0 {
            -(total_marker_width + marker_spacing)
        } else {
            0.0
        };

        // 4. Position the items belonging to this segment.
        //
        // +spec:inline-formatting-context:267438 - Content positioning: position aligned subtree and baseline-shift values within line box
        //
        // Vertical alignment positioning (CSS vertical-align)
        //
        // +spec:font-metrics:cae541 - dominant baseline used for inline alignment
        // Per CSS Inline Layout Level 3 § 4 (Baseline Alignment), each inline
        // element can specify its own `vertical-align`. For Object items
        // (inline-blocks, images), we use their per-item alignment stored in
        // `InlineContent::Shape.alignment` or `InlineContent::Image.alignment`.
        // For text clusters or items without a per-item override, we fall back
        // to the global `constraints.vertical_align` from the containing block.
        //
        // +spec:font-metrics:f29b61 - baseline alignment matches corresponding baseline types (only alphabetic implemented)
        // Reference: https://www.w3.org/TR/css-inline-3/#baseline-alignment
        // +spec:block-formatting-context:26b535 - In vertical typographic mode, central baseline is dominant when text-orientation is mixed/upright; otherwise alphabetic
        // +spec:inline-formatting-context:eb735b - alignment-baseline: inline-level boxes aligned to parent's baseline via vertical-align
        // +spec:inline-formatting-context:da3f34 - baseline alignment of in-flow inline-level boxes in block axis per dominant-baseline/vertical-align
        // +spec:line-height:e2253a - vertical-align positioning within line boxes

        // Pre-compute inline border/padding offsets at span boundaries.
        // Only the FIRST cluster of each inline span gets left_inset, and only
        // the LAST cluster gets right_inset. We detect span boundaries by comparing
        // Arc<StyleProperties> pointers between consecutive clusters.
        let inline_offsets: Vec<(f32, f32)> = {
            let items_slice: &[ShapedItem] = &justified_segment_items;
            items_slice.iter().enumerate().map(|(idx, item)| {
                if let ShapedItem::Cluster(c) = item {
                    if let Some(border) = c.style.border.as_ref() {
                        if border.has_chrome() {
                            let style_ptr = Arc::as_ptr(&c.style);
                            let prev_same_span = idx > 0 && items_slice[idx - 1]
                                .as_cluster()
                                .is_some_and(|pc| Arc::as_ptr(&pc.style) == style_ptr);
                            let next_same_span = idx + 1 < items_slice.len() && items_slice[idx + 1]
                                .as_cluster()
                                .is_some_and(|nc| Arc::as_ptr(&nc.style) == style_ptr);
                            let left = if prev_same_span { 0.0 } else { border.left_inset() };
                            let right = if next_same_span { 0.0 } else { border.right_inset() };
                            return (left, right);
                        }
                    }
                }
                (0.0, 0.0)
            }).collect()
        };
        for (inline_offset_idx, item) in justified_segment_items.into_iter().enumerate() {
            let (item_ascent, item_descent) = get_item_vertical_metrics(&item, constraints);
            // Use per-item alignment if available, otherwise fall back to global
            let effective_align = get_item_vertical_align(&item)
                .unwrap_or(constraints.vertical_align);
            // +spec:display-property:328cfc - baseline-shift / aligned subtree vertical alignment (sub, super, top, bottom, center)
            // §10.8.1 vertical-align positioning
            // +spec:line-height:0fcfab - vertical-align property values (baseline, top, middle, bottom, sub, super, text-top, text-bottom, percentage, length)
            let item_baseline_pos = match effective_align {
                // +spec:display-property:8e018d - aligned subtree edges used for top/bottom line box alignment
                // +spec:inline-formatting-context:495672 - line-relative vertical-align (top/center/bottom) and aligned subtree positioning
                // top: align top of aligned subtree with top of line box
                VerticalAlign::Top => line_top_y + item_ascent,
                // +spec:font-metrics:70000d - align vertical midpoint of box with baseline + half x-height of parent
                VerticalAlign::Middle => {
                    let half_x_height = constraints.strut_x_height / 2.0;
                    line_baseline_y + half_x_height - f32::midpoint(item_ascent, item_descent) + item_ascent
                }
                // bottom: align bottom of aligned subtree with bottom of line box
                VerticalAlign::Bottom => line_top_y + line_box_height - item_descent,
                // +spec:font-metrics:aa21f7 - sub: lower baseline to proper subscript position
                VerticalAlign::Sub => line_baseline_y + line_ascent * SUBSCRIPT_OFFSET_RATIO,
                // +spec:display-property:3b0e76 - baseline-shift super raises by ~1/3 font-size; top/bottom align to line box edges
                // super: raise baseline to proper superscript position (~0.4em)
                VerticalAlign::Super => line_baseline_y - line_ascent * SUPERSCRIPT_OFFSET_RATIO,
                // text-top: align top of box with top of parent's content area (§10.6.1)
                // Parent's content area top = baseline - strut_ascent
                VerticalAlign::TextTop => (line_baseline_y - constraints.strut_ascent) + item_ascent,
                // text-bottom: align bottom of box with bottom of parent's content area (§10.6.1)
                // Parent's content area bottom = baseline + strut_descent
                VerticalAlign::TextBottom => (line_baseline_y + constraints.strut_descent) - item_descent,
                // <length>/<percentage>: raise (positive) or lower (negative); 0 = baseline
                VerticalAlign::Offset(offset) => line_baseline_y - offset,
                // +spec:display-property:8bf37e - dominant-baseline defaults to alphabetic; baseline alignment matches parent
                // baseline: align baseline of box with baseline of parent box
                // +spec:font-metrics:96bbd3 - baseline: align alphabetic baseline of box with parent's alphabetic baseline
                VerticalAlign::Baseline => line_baseline_y,
            };

            // Calculate item measure (needed for both positioning and pen advance)
            let item_measure = get_item_measure(&item, is_vertical);

            // Advance pen by inline left_inset at span entry (before positioning glyphs)
            let (left_inset, right_inset) = if inline_offset_idx < inline_offsets.len() {
                inline_offsets[inline_offset_idx]
            } else {
                (0.0, 0.0)
            };
            main_axis_pen += left_inset;

            let position = if is_vertical {
                Point {
                    x: item_baseline_pos - item_ascent,
                    y: main_axis_pen,
                }
            } else {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[Pos1Line] is_vertical=false, main_axis_pen={main_axis_pen}, item_baseline_pos={item_baseline_pos}, \
                         item_ascent={item_ascent}"
                    )));
                }

                // Check if this is an outside marker - if so, position it in the padding gutter
                let x_position = if let ShapedItem::Cluster(cluster) = &item {
                    if cluster.marker_position_outside == Some(true) {
                        // Use marker_pen for sequential marker positioning
                        let marker_width = item_measure;
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[Pos1Line] Outside marker detected! width={marker_width}, positioning at \
                                 marker_pen={marker_pen}"
                            )));
                        }
                        let pos = marker_pen;
                        marker_pen += marker_width; // Advance marker pen for next marker cluster
                        pos
                    } else {
                        main_axis_pen
                    }
                } else {
                    main_axis_pen
                };

                Point {
                    y: item_baseline_pos - item_ascent,
                    x: x_position,
                }
            };

            // item_measure is calculated above for marker positioning
            let item_text = item
                .as_cluster()
                .map_or("[OBJ]", |c| c.text.as_str());
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[Pos1Line] Positioning item '{item_text}' at pen_x={main_axis_pen}"
                )));
            }
            positioned.push(PositionedItem {
                item: item.clone(),
                position,
                line_index,
            });

            // Outside markers don't advance the pen - they're positioned in the padding gutter
            let is_outside_marker = if let ShapedItem::Cluster(c) = &item {
                c.marker_position_outside == Some(true)
            } else {
                false
            };

            if !is_outside_marker {
                main_axis_pen += item_measure;
                // Advance pen by inline right_inset at span exit (after glyph advance)
                main_axis_pen += right_inset;
            }

            // +spec:text-alignment-spacing:e09bd1 - justification space added on top of letter-spacing/word-spacing
            // +spec:text-alignment-spacing:456643 - cursive scripts don't admit inter-character gaps
            let is_cursive = if let ShapedItem::Cluster(c) = &item { is_cursive_script_cluster(c) } else { false };
            if !is_outside_marker && extra_char_spacing > 0.0 && can_justify_after(&item) && !is_cursive {
                main_axis_pen += extra_char_spacing;
            }
            // +spec:display-property:3a833c - consecutive atomic inlines treated as single unit for letter-spacing
            // +spec:display-property:49f04f - letter-spacing applied per innermost inline element
            // +spec:text-alignment-spacing:22bea4 - letter-spacing applied after bidi reordering, additive with kerning and word-spacing; justification may further adjust
            if let ShapedItem::Cluster(c) = &item {
                if !is_outside_marker {
                    // +spec:display-property:756454 - letter-spacing applied between typographic character units
                    // +spec:overflow:e63bc0 - letter-spacing ignores zero-width formatting chars (Cf); handled by shaper merging them into clusters
                    // +spec:text-alignment-spacing:80f9ec - letter-spacing applied per-cluster using innermost element's style (UA-allowed attachment)
                    // +spec:text-alignment-spacing:bdd704 - letter-spacing applied after each cluster, not at line start
                    // +spec:text-alignment-spacing:d3ef6e - single-char element: only trailing space, no inter-char effect
                    // +spec:text-alignment-spacing:d668fc - letter-spacing only affects characters within the element (per-cluster style)
                    // +spec:text-alignment-spacing:8dbb78 - zero letter-spacing behaves as normal (Px(0) adds no spacing)
                    // +spec:text-alignment-spacing:456643 - skip letter-spacing for cursive scripts
                    if !is_cursive_script_cluster(c) {
                    let letter_spacing_px = match c.style.letter_spacing {
                        Spacing::Px(px) => px as f32,
                        Spacing::Em(em) => em * c.style.font_size_px,
                    };
                    main_axis_pen += letter_spacing_px;
                    }
                    // +spec:width-calculation:9447d1 - word-spacing only applied to word separators; zero-width chars like U+200B are excluded
                    if is_word_separator(&item) {
                        let word_spacing_px = match c.style.word_spacing {
                            Spacing::Px(px) => px as f32,
                            Spacing::Em(em) => em * c.style.font_size_px,
                        };
                        main_axis_pen += word_spacing_px;
                        main_axis_pen += extra_word_spacing;
                    }
                }
            }
        }
    }

    (positioned, line_box_height)
}

/// Calculates the starting pen offset to achieve the desired text alignment.
fn calculate_alignment_offset(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    align: TextAlign,
    is_vertical: bool,
    constraints: &UnifiedConstraints,
) -> f32 {
    // Simplified to use the first segment for alignment.
    if let Some(segment) = line_constraints.segments.first() {
        let total_width: f32 = items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        let available_width = if constraints.segment_alignment == SegmentAlignment::Total {
            line_constraints.total_available
        } else {
            segment.width
        };

        if total_width >= available_width {
            return 0.0; // No alignment needed if line is full or overflows
        }

        let remaining_space = available_width - total_width;

        match align {
            TextAlign::Center => remaining_space / 2.0,
            TextAlign::Right => remaining_space,
            _ => 0.0, // Left, Justify, Start, End
        }
    } else {
        0.0
    }
}

/// Calculates the extra spacing needed for justification without modifying the items.
///
/// This function is pure and does not mutate any state, making it safe to use
/// with cached `ShapedItem` data.
///
/// # Arguments
/// * `items` - A slice of items on the line.
/// * `line_constraints` - The geometric constraints for the line.
/// * `text_justify` - The type of justification to calculate.
/// * `is_vertical` - Whether the layout is vertical.
///
/// # Returns
/// A tuple `(extra_per_word, extra_per_char)` containing the extra space in pixels
/// to add at each word or character justification opportunity.
// +spec:display-contents:654278 - distributes remaining space to fill line box when justifying
// +spec:text-alignment-spacing:56c7f4 - equal distribution of justification space within priority level
// +spec:text-alignment-spacing:f17bbc - justification opportunities controlled by text-justify value (inter-word = word separators, inter-character = character juxtaposition)
#[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
fn calculate_justification_spacing(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    text_justify: JustifyContent,
    is_vertical: bool,
) -> (f32, f32) {
    // (extra_per_word, extra_per_char)
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;

    if total_width >= available_width || available_width <= 0.0 {
        return (0.0, 0.0);
    }

    let extra_space = available_width - total_width;

    // +spec:text-alignment-spacing:71314a - script categories for justification: inter-word for clustered, kashida for cursive (Arabic), inter-character for block (CJK)
    match text_justify {
        JustifyContent::InterWord => {
            // Count justification opportunities (spaces).
            let space_count = items.iter().filter(|item| is_word_separator(item)).count();
            if space_count > 0 {
                (extra_space / space_count as f32, 0.0)
            } else {
                (0.0, 0.0) // No spaces to expand, do nothing.
            }
        }
        JustifyContent::InterCharacter | JustifyContent::Distribute => {
            // Count justification opportunities (between non-combining characters).
            let gap_count = items
                .iter()
                .enumerate()
                .filter(|(i, item)| *i < items.len() - 1 && can_justify_after(item))
                .count();
            if gap_count > 0 {
                (0.0, extra_space / gap_count as f32)
            } else {
                (0.0, 0.0) // No gaps to expand, do nothing.
            }
        }
        // Kashida justification modifies the item list and is handled by a separate function.
        _ => (0.0, 0.0),
    }
}

/// Rebuilds a line of items, inserting Kashida glyphs for justification.
///
/// This function is non-mutating with respect to its inputs. It takes ownership of the
/// original items and returns a completely new `Vec`. This is necessary because Kashida
/// justification changes the number of items on the line, and must not modify cached data.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn justify_kashida_and_rebuild<T: ParsedFontTrait>(
    items: Vec<ShapedItem>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Vec<ShapedItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering justify_kashida_and_rebuild ---".to_string(),
        ));
    }
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Total item width: {total_width}, Available width: {available_width}"
        )));
    }

    if total_width >= available_width || available_width <= 0.0 {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No justification needed (line is full or invalid).".to_string(),
            ));
        }
        return items;
    }

    let extra_space = available_width - total_width;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Extra space to fill: {extra_space}"
        )));
    }

    let font_info = items.iter().find_map(|item| {
        if let ShapedItem::Cluster(c) = item {
            if let Some(glyph) = c.glyphs.first() {
                if glyph.script == Script::Arabic {
                    // Look up font from hash
                    if let Some(font) = fonts.get_by_hash(glyph.font_hash) {
                        return Some((
                            font.clone(),
                            glyph.font_hash,
                            glyph.font_metrics,
                            glyph.style.clone(),
                        ));
                    }
                }
            }
        }
        None
    });

    let (font, font_hash, font_metrics, style) = if let Some(info) = font_info {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Found Arabic font for kashida.".to_string(),
            ));
        }
        info
    } else {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No Arabic font found on line. Cannot insert kashidas.".to_string(),
            ));
        }
        return items;
    };

    let (kashida_glyph_id, kashida_advance) =
        match font.get_kashida_glyph_and_advance(style.font_size_px) {
            Some((id, adv)) if adv > 0.0 => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "Font provides kashida glyph with advance {adv}"
                    )));
                }
                (id, adv)
            }
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "Font does not support kashida justification.".to_string(),
                    ));
                }
                return items;
            }
        };

    let opportunity_indices: Vec<usize> = items
        .windows(2)
        .enumerate()
        .filter_map(|(i, window)| {
            if let (ShapedItem::Cluster(cur), ShapedItem::Cluster(next)) = (&window[0], &window[1])
            {
                if is_arabic_cluster(cur)
                    && is_arabic_cluster(next)
                    && !is_word_separator(&window[1])
                {
                    return Some(i + 1);
                }
            }
            None
        })
        .collect();

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Found {} kashida insertion opportunities at indices: {:?}",
            opportunity_indices.len(),
            opportunity_indices
        )));
    }

    if opportunity_indices.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No opportunities found. Exiting.".to_string(),
            ));
        }
        return items;
    }

    let num_kashidas_to_insert = (extra_space / kashida_advance).floor() as usize;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Calculated number of kashidas to insert: {num_kashidas_to_insert}"
        )));
    }

    if num_kashidas_to_insert == 0 {
        return items;
    }

    let kashidas_per_point = num_kashidas_to_insert / opportunity_indices.len();
    let mut remainder = num_kashidas_to_insert % opportunity_indices.len();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Distributing kashidas: {kashidas_per_point} per point, with {remainder} remainder."
        )));
    }

    let kashida_item = {
        /* ... as before ... */
        let kashida_glyph = ShapedGlyph {
            kind: GlyphKind::Kashida {
                width: kashida_advance,
            },
            glyph_id: kashida_glyph_id,
            font_hash,
            font_metrics,
            style: style.clone(),
            script: Script::Arabic,
            advance: kashida_advance,
            kerning: 0.0,
            cluster_offset: 0,
            offset: Point::default(),
            vertical_advance: 0.0,
            vertical_offset: Point::default(),
        };
        ShapedItem::Cluster(ShapedCluster {
            text: "\u{0640}".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Kashida is generated, not from DOM
            glyphs: smallvec![kashida_glyph],
            advance: kashida_advance,
            direction: BidiDirection::Ltr,
            style,
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        })
    };

    let mut new_items = Vec::with_capacity(items.len() + num_kashidas_to_insert);
    let mut last_copy_idx = 0;
    for &point in &opportunity_indices {
        new_items.extend_from_slice(&items[last_copy_idx..point]);
        let mut num_to_insert = kashidas_per_point;
        if remainder > 0 {
            num_to_insert += 1;
            remainder -= 1;
        }
        for _ in 0..num_to_insert {
            new_items.push(kashida_item.clone());
        }
        last_copy_idx = point;
    }
    new_items.extend_from_slice(&items[last_copy_idx..]);

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting justify_kashida_and_rebuild, new item count: {} ---",
            new_items.len()
        )));
    }
    new_items
}

/// Helper to determine if a cluster belongs to the Arabic script.
fn is_arabic_cluster(cluster: &ShapedCluster) -> bool {
    // A cluster is considered Arabic if its first non-NotDef glyph is from the Arabic script.
    // This is a robust heuristic for mixed-script lines.
    cluster.glyphs.iter().any(|g| g.script == Script::Arabic)
}

/// Helper to identify if an item is a word separator (like a space).
fn measure_trailing_whitespace(items: &[ShapedItem], is_vertical: bool) -> f32 {
    let mut trailing_ws = 0.0;
    for item in items.iter().rev() {
        if is_collapsible_whitespace(item) {
            trailing_ws += get_item_measure(item, is_vertical);
        } else {
            break;
        }
    }
    trailing_ws
}

/// Returns true if the item is collapsible whitespace per CSS Text 3 §4.1.2 Phase II.
///
/// This is used for stripping leading/trailing whitespace at line edges —
/// distinct from `is_word_separator` which is for word-spacing per §7.1.
#[must_use] pub fn is_collapsible_whitespace(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().all(|ch| matches!(ch,
            ' ' | '\t' | '\u{1680}' // Ogham space mark (collapsible per spec)
        ))
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
    c.text.chars().next().is_some_and(is_cursive_script_char)
}

fn is_cursive_script_char(ch: char) -> bool {
    let cp = ch as u32;
    // Arabic (U+0600–U+06FF, U+0750–U+077F, U+08A0–U+08FF, U+FB50–U+FDFF, U+FE70–U+FEFF)
    if (0x0600..=0x06FF).contains(&cp) { return true; }
    if (0x0750..=0x077F).contains(&cp) { return true; }
    if (0x08A0..=0x08FF).contains(&cp) { return true; }
    if (0xFB50..=0xFDFF).contains(&cp) { return true; }
    if (0xFE70..=0xFEFF).contains(&cp) { return true; }
    // Syriac (U+0700–U+074F)
    if (0x0700..=0x074F).contains(&cp) { return true; }
    // Mongolian (U+1800–U+18AF)
    if (0x1800..=0x18AF).contains(&cp) { return true; }
    // N'Ko (U+07C0–U+07FF)
    if (0x07C0..=0x07FF).contains(&cp) { return true; }
    // Mandaic (U+0840–U+085F)
    if (0x0840..=0x085F).contains(&cp) { return true; }
    // Phags Pa (U+A840–U+A87F)
    if (0xA840..=0xA87F).contains(&cp) { return true; }
    // Hanifi Rohingya (U+10D00–U+10D3F)
    if (0x10D00..=0x10D3F).contains(&cp) { return true; }
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
fn cluster_is_word_boundary(cluster: &ShapedCluster) -> bool {
    !cluster.text.chars().any(is_word_char)
}

// exclude punctuation and fixed-width spaces (U+3000, U+2000..U+200A)
pub fn is_word_separator(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().any(is_word_separator_char)
    } else {
        false
    }
}

// +spec:margin-collapsing:6706c1 - fixed-width spaces (U+2000–U+200A, U+3000) excluded from word separators
/// Returns true if the character is a word-separator character per CSS Text §7.1.
/// Punctuation and fixed-width spaces (U+3000, U+2000 through U+200A) are NOT
/// word-separator characters even though they may visually separate words.
// +spec:text-alignment-spacing:3e0655 - word-separator characters for word-spacing
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn is_word_separator_char(c: char) -> bool {
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
// +spec:line-breaking:fd3164 - U+200B as explicit word delimiter for scripts without space-separated words
#[must_use] pub fn is_zero_width_space(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.contains('\u{200B}')
    } else {
        false
    }
}

/// Helper to identify if space can be added after an item.
fn can_justify_after(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().last().is_some_and(|g| {
            !g.is_whitespace() && classify_character(g as u32) != CharacterClass::Combining
        })
    } else {
        // Per CSS 2.2 §9.4.2, justification must NOT stretch inline-table and
        // inline-block boxes. Object items represent these atomic inline-level
        // boxes, so we return false to prevent adding justification space after them.
        false
    }
}

// +spec:font-metrics:b8eb97 - Script group classification for justification/letter-spacing behavior
/// Classifies a character for layout purposes (e.g., justification behavior).
/// Copied from `mod.rs`.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn classify_character(codepoint: u32) -> CharacterClass {
    match codepoint {
        0x0020 | 0x00A0 | 0x3000 => CharacterClass::Space,
        0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => {
            CharacterClass::Punctuation
        }
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => CharacterClass::Ideograph,
        0x0300..=0x036F | 0x1AB0..=0x1AFF => CharacterClass::Combining,
        // Mongolian script range
        0x1800..=0x18AF => CharacterClass::Letter,
        _ => CharacterClass::Letter,
    }
}

/// Helper to get the primary measure (width or height) of a shaped item.
#[must_use] pub fn get_item_measure(item: &ShapedItem, is_vertical: bool) -> f32 {
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

/// Calculates the available horizontal segments for a line at a given vertical position,
/// considering both shape boundaries and exclusions.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn get_line_constraints(
    line_y: f32,
    line_height: f32,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LineConstraints {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering get_line_constraints for y={line_y} ---"
        )));
    }

    let mut available_segments = Vec::new();
    if constraints.shape_boundaries.is_empty() {
        // The segment_width is determined by available_width, NOT by TextWrap.
        // TextWrap::NoWrap only affects whether the LineBreaker can insert soft breaks,
        // it should NOT override a definite width constraint from CSS.
        // +spec:overflow:b06c3e - text overflows when wrapping is prevented (e.g. white-space: nowrap)
        // CSS Text Level 3: For 'white-space: pre/nowrap', text overflows horizontally
        // if it doesn't fit, rather than expanding the container.
        //
        // For MinContent/MaxContent intrinsic sizing: use a large value to let text 
        // lay out fully. The line breaker handles min-content by breaking at word 
        // boundaries. The actual content width is measured from the laid-out lines.
        let segment_width = match constraints.available_width {
            AvailableSpace::Definite(w) => w, // Respect definite width from CSS
            AvailableSpace::MaxContent => f32::MAX / 2.0, // For intrinsic max-content sizing
            AvailableSpace::MinContent => f32::MAX / 2.0, // For intrinsic min-content sizing
        };
        // Note: TextWrap::NoWrap is handled by the LineBreaker in break_one_line()
        // to prevent soft wraps. The text will simply overflow if it exceeds segment_width.
        available_segments.push(LineSegment {
            start_x: 0.0,
            width: segment_width,
            priority: 0,
        });
    } else {
        // ... complex boundary logic ...
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Initial available segments: {available_segments:?}"
        )));
    }

    for (idx, exclusion) in constraints.shape_exclusions.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Applying exclusion #{idx}: {exclusion:?}"
            )));
        }
        let exclusion_spans =
            get_shape_horizontal_spans(exclusion, line_y, line_height);
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Exclusion spans at y={line_y}: {exclusion_spans:?}"
            )));
        }

        if exclusion_spans.is_empty() {
            continue;
        }

        let mut next_segments = Vec::new();
        for (excl_start, excl_end) in exclusion_spans {
            for segment in &available_segments {
                let seg_start = segment.start_x;
                let seg_end = segment.start_x + segment.width;

                // Create new segments by subtracting the exclusion
                if seg_end > excl_start && seg_start < excl_end {
                    if seg_start < excl_start {
                        // Left part
                        next_segments.push(LineSegment {
                            start_x: seg_start,
                            width: excl_start - seg_start,
                            priority: segment.priority,
                        });
                    }
                    if seg_end > excl_end {
                        // Right part
                        next_segments.push(LineSegment {
                            start_x: excl_end,
                            width: seg_end - excl_end,
                            priority: segment.priority,
                        });
                    }
                } else {
                    next_segments.push(*segment); // No overlap
                }
            }
            available_segments = merge_segments(next_segments);
            next_segments = Vec::new();
        }
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Segments after exclusion #{idx}: {available_segments:?}"
            )));
        }
    }

    let total_width = available_segments.iter().map(|s| s.width).sum();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Final segments: {available_segments:?}, total available width: {total_width}"
        )));
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting get_line_constraints ---".to_string(),
        ));
    }

    LineConstraints {
        segments: available_segments,
        total_available: total_width,
    }
}

/// Flattens a parsed SVG multipolygon (from a CSS `path()` shape) into a flat list of
/// `PathSegment`s in absolute coordinates (offset by the reference box origin). Each ring
/// becomes a `MoveTo` + a run of `LineTo`s + `Close`; curve elements are sampled into line
/// segments (~one segment per 4px of arc length, capped) so the scanline intersection can
/// treat each subpath as a polygon.
fn flatten_svg_to_path_segments(
    multipolygon: &azul_core::svg::SvgMultiPolygon,
    reference_box: Rect,
) -> Vec<PathSegment> {
    use azul_core::svg::SvgPathElement;

    let mut out: Vec<PathSegment> = Vec::new();

    for ring in multipolygon.rings.as_ref() {
        let elements = ring.items.as_ref();
        if elements.is_empty() {
            continue;
        }
        let start = elements[0].get_start();
        out.push(PathSegment::MoveTo(Point {
            x: reference_box.x + start.x,
            y: reference_box.y + start.y,
        }));
        for el in elements {
            match el {
                SvgPathElement::Line(l) => {
                    out.push(PathSegment::LineTo(Point {
                        x: reference_box.x + l.end.x,
                        y: reference_box.y + l.end.y,
                    }));
                }
                curve => {
                    // Sample the curve by arc length into line segments.
                    let len = curve.get_length();
                    let steps = ((len / 4.0).ceil() as usize).clamp(1, 64);
                    for i in 1..=steps {
                        let offset = len * (i as f64) / (steps as f64);
                        let t = curve.get_t_at_offset(offset);
                        out.push(PathSegment::LineTo(Point {
                            x: reference_box.x + curve.get_x_at_t(t) as f32,
                            y: reference_box.y + curve.get_y_at_t(t) as f32,
                        }));
                    }
                }
            }
        }
        out.push(PathSegment::Close);
    }

    out
}

/// Computes horizontal line segments where a flattened `path()` shape (a set of
/// `MoveTo`/`LineTo`/`Close` subpaths) intersects a scanline at the given y range. Uses an
/// even-odd fill rule over the union of all subpaths so reversed rings (holes) carve out
/// space. Curves are assumed already flattened to `LineTo`s by `flatten_svg_to_path_segments`.
fn path_segments_line_intersection(
    segments: &[PathSegment],
    y: f32,
    line_height: f32,
) -> Vec<(f32, f32)> {
    let line_center_y = y + line_height / 2.0;
    let mut crossings: Vec<f32> = Vec::new();

    // Walk the segments, reconstructing each subpath's vertices and intersecting its
    // (closing) edges with the scanline.
    let mut subpath: Vec<Point> = Vec::new();
    let flush = |subpath: &mut Vec<Point>, crossings: &mut Vec<f32>| {
        if subpath.len() >= 2 {
            for i in 0..subpath.len() {
                let p1 = subpath[i];
                let p2 = subpath[(i + 1) % subpath.len()];
                if (p2.y - p1.y).abs() < f32::EPSILON {
                    continue;
                }
                let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
                    || (p1.y > line_center_y && p2.y <= line_center_y);
                if crosses {
                    let t = (line_center_y - p1.y) / (p2.y - p1.y);
                    crossings.push(p1.x + t * (p2.x - p1.x));
                }
            }
        }
        subpath.clear();
    };

    for seg in segments {
        match seg {
            PathSegment::MoveTo(p) => {
                flush(&mut subpath, &mut crossings);
                subpath.push(*p);
            }
            PathSegment::LineTo(p) => subpath.push(*p),
            PathSegment::Close => flush(&mut subpath, &mut crossings),
            // CurveTo/QuadTo/Arc should have been flattened to LineTo already; sample the
            // end point as a fallback so an unflattened path still produces a polygon.
            PathSegment::CurveTo { end, .. } | PathSegment::QuadTo { end, .. } => {
                subpath.push(*end)
            }
            PathSegment::Arc { center, .. } => subpath.push(*center),
        }
    }
    flush(&mut subpath, &mut crossings);

    crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let mut spans = Vec::new();
    for chunk in crossings.chunks_exact(2) {
        if chunk[1] > chunk[0] {
            spans.push((chunk[0], chunk[1]));
        }
    }
    spans
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (`start_x`, `end_x`) tuples.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
fn get_shape_horizontal_spans(
    shape: &ShapeBoundary,
    y: f32,
    line_height: f32,
) -> Vec<(f32, f32)> {
    match shape {
        ShapeBoundary::Rectangle(rect) => {
            // Check for any overlap between the line box [y, y + line_height]
            // and the rectangle's vertical span [rect.y, rect.y + rect.height].
            let line_start = y;
            let line_end = y + line_height;
            let rect_start = rect.y;
            let rect_end = rect.y + rect.height;

            if line_start < rect_end && line_end > rect_start {
                vec![(rect.x, rect.x + rect.width)]
            } else {
                vec![]
            }
        }
        ShapeBoundary::Circle { center, radius } => {
            let line_center_y = y + line_height / 2.0;
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                vec![(center.x - dx, center.x + dx)]
            } else {
                vec![]
            }
        }
        ShapeBoundary::Ellipse { center, radii } => {
            let line_center_y = y + line_height / 2.0;
            let dy = line_center_y - center.y;
            if dy.abs() <= radii.height {
                // Formula: (x-h)^2/a^2 + (y-k)^2/b^2 = 1
                let y_term = dy / radii.height;
                let x_term_squared = 1.0 - y_term.powi(2);
                if x_term_squared >= 0.0 {
                    let dx = radii.width * x_term_squared.sqrt();
                    vec![(center.x - dx, center.x + dx)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        }
        ShapeBoundary::Polygon { points } => {
            let segments = polygon_line_intersection(points, y, line_height);
            segments
                .iter()
                .map(|s| (s.start_x, s.start_x + s.width))
                .collect()
        }
        // Scanline intersection for `path()` shapes. `segments` is the flattened
        // (Close-terminated, curves pre-sampled) output of `flatten_svg_to_path_segments`;
        // intersect each subpath polygon with this scanline under an even-odd fill rule so
        // reversed rings (holes) carve out space.
        ShapeBoundary::Path { segments } => {
            path_segments_line_intersection(segments, y, line_height)
        }
    }
}

/// Merges overlapping or adjacent line segments into larger ones.
fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());
    let mut merged = vec![segments[0]];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(*next_seg);
        }
    }
    merged
}

/// Computes horizontal line segments where a polygon intersects a scanline at the given y range.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
fn polygon_line_intersection(
    points: &[Point],
    y: f32,
    line_height: f32,
) -> Vec<LineSegment> {
    if points.len() < 3 {
        return vec![];
    }

    let line_center_y = y + line_height / 2.0;
    let mut intersections = Vec::new();

    // Use winding number algorithm for robustness with complex polygons.
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        // Skip horizontal edges as they don't intersect a horizontal scanline in a meaningful way.
        if (p2.y - p1.y).abs() < f32::EPSILON {
            continue;
        }

        // Check if our horizontal scanline at `line_center_y` crosses this polygon edge.
        let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
            || (p1.y > line_center_y && p2.y <= line_center_y);

        if crosses {
            // Calculate intersection x-coordinate using linear interpolation.
            let t = (line_center_y - p1.y) / (p2.y - p1.y);
            let x = p1.x + t * (p2.x - p1.x);
            intersections.push(x);
        }
    }

    // Sort intersections by x-coordinate to form spans.
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    // Build segments from paired intersection points.
    let mut segments = Vec::new();
    for chunk in intersections.chunks_exact(2) {
        let start_x = chunk[0];
        let end_x = chunk[1];
        if end_x > start_x {
            segments.push(LineSegment {
                start_x,
                width: end_x - start_x,
                priority: 0,
            });
        }
    }

    segments
}

// ADDITION: A helper function to get a hyphenator.
/// Helper to get a hyphenator for a given language.
/// TODO: In a real app, this would be cached.
#[cfg(feature = "text_layout_hyphenation")]
fn get_hyphenator(language: HyphenationLanguage) -> Result<Standard, LayoutError> {
    Standard::from_embedded(language).map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

/// Stub when hyphenation is disabled - always returns an error
#[cfg(not(feature = "text_layout_hyphenation"))]
fn get_hyphenator(_language: Language) -> Result<Standard, LayoutError> {
    Err(LayoutError::HyphenationError("Hyphenation feature not enabled".to_string()))
}

// +spec:inline-block:6e7dd9 - Non-tailorable Unicode line breaking controls take precedence over atomic inline rules (CSS-TEXT-3 recent changes, issue 8972)

const fn is_break_suppressing_control(ch: char) -> bool {
    matches!(ch,
        '\u{200D}' | // ZERO WIDTH JOINER
        '\u{2060}' | // WORD JOINER
        '\u{FEFF}'   // ZERO WIDTH NO-BREAK SPACE
    )
}

const fn is_break_forcing_control(ch: char) -> bool {
    matches!(ch,
        '\u{200B}' | // ZERO WIDTH SPACE (already handled but included for completeness)
        '\u{2028}' | // LINE SEPARATOR
        '\u{2029}'   // PARAGRAPH SEPARATOR
    )
}

// +spec:line-breaking:495247 - CJK/syllabic writing systems allow breaks between typographic letter units with varying strictness
// §5.2 word-break: determines if a character is CJK ideograph/kana
const fn is_cjk_character(ch: char) -> bool {
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
fn is_cjk_cluster(cluster: &ShapedCluster) -> bool {
    cluster.text.chars().any(is_cjk_character)
}

// +spec:line-breaking:e1fc9d - word-break normal/break-all/keep-all break opportunity rules
// +spec:line-breaking:73d5fe - word-break break-point determination for CJK and Latin text
// +spec:line-breaking:31ef1a - word-break property controls soft wrap opportunities between letters (NU/AL/AI/ID classes as letter units)
// +spec:line-breaking:798252 - word-break property affects break opportunities (normal/break-all/keep-all)
// +spec:line-breaking:8fed57 - word-break: break-all treats all clusters as break opportunities, keep-all suppresses CJK breaks
// +spec:line-breaking:e2b374 - word-break: normal (only at separators) vs break-all (between all letters incl. Ethiopic)
// +spec:overflow:53a97f - word-break (normal/break-all/keep-all) and line-break strictness rules
// +spec:line-breaking:1c830a - word-break: normal/break-all/keep-all break opportunity rules
// §5.2 word-break property: break opportunity logic
// +spec:line-breaking:a75147 - word-break property: normal (CJK breaks), break-all (every cluster), keep-all (suppress CJK breaks)
// +spec:line-breaking:65ab41 - word-break: normal/break-all/keep-all break opportunity rules
// +spec:line-breaking:7eca16 - U+200B ZERO WIDTH SPACE is always a break opportunity, even with keep-all
fn is_break_opportunity_with_word_break(item: &ShapedItem, word_break: WordBreak, hyphens: Hyphens) -> bool {
    // Break after spaces or explicit break items (always, regardless of word-break).
    if is_word_separator(item) {
        return true;
    }
    if let ShapedItem::Break { .. } = item {
        return true;
    }
    // +spec:line-breaking:432d5b - hyphens property controls soft wrap opportunities via hyphenation
    // +spec:line-breaking:5a32a1 - soft hyphen (U+00AD) creates break opportunity; glyph styled per surrounding text properties
    // U+200B ZERO WIDTH SPACE is always a soft wrap opportunity regardless of word-break.
    // This allows authors to mark explicit wrap points (e.g. with <wbr> or &#x200B;)
    // even when using word-break: keep-all to suppress other breaks.
    if is_zero_width_space(item) {
        return true;
    }
    // only when hyphens != none. With hyphens:none, soft hyphens do not create break points.
    if hyphens != Hyphens::None {
        if let ShapedItem::Cluster(c) = item {
            if c.text.starts_with('\u{00AD}') {
                return true;
            }
        }
    }

    // +spec:line-breaking:2bbda0 - word-break does not affect soft wrap opportunities around punctuation
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
            // +spec:line-breaking:aa3044 - keep-all suppresses CJK (incl. Korean) inter-character breaks
            // Only break at spaces/hyphens (already handled above).
            false
        }
    }
}

// +spec:line-breaking:db0289 - line-break strictness: anywhere allows soft wrap around every typographic character unit
// +spec:line-breaking:7d242b - line-break strictness levels: loose/normal/strict/anywhere with CJK punctuation rules
// +spec:line-breaking:67bfe8 - line-break strictness (auto/loose/normal/strict/anywhere) controls
// CSS Text Level 3 §5.3: Determines whether a break opportunity before a character is
// allowed based on the line-break strictness level. The spec defines:
// - strict: forbids breaks before small kana (class CJ), CJK hyphens, and certain punctuation
// - normal: allows breaks before small kana (CJ); allows CJK hyphen breaks for CJK writing systems
// - loose: additionally allows breaks before hyphens U+2010/U+2013 after ID-class chars
// - anywhere: allows soft wrap around every typographic character unit
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
const fn is_cjk_break_allowed_by_strictness(
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
                '\u{2010}' | '\u{2013}' => false,  // hyphens
                c if is_small_kana(c) => false,
                _ => true,
            }
        }
    }
}

/// Returns true if the character is a Japanese small kana or Katakana-Hiragana prolonged sound mark
/// (Unicode line break class CJ). These are forbidden break points in strict line breaking.
const fn is_small_kana(ch: char) -> bool {
    matches!(ch,
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
        '\u{30FC}'   // ー KATAKANA-HIRAGANA PROLONGED SOUND MARK
    )
}

// for every typographic character unit, disregarding GL/WJ/ZWJ line breaking classes
// replaced element or other atomic inline for web-compat
fn is_break_opportunity(item: &ShapedItem) -> bool {
    // Per CSS Text 3 §5.1: "there is a soft wrap opportunity before and
    // after each replaced element or other atomic inline"
    if matches!(item, ShapedItem::Object { .. } | ShapedItem::CombinedBlock { .. }) {
        return true;
    }
    // over atomic inline rules: break-forcing controls (ZWSP, LS, PS) create break opportunities
    // even adjacent to atomic inlines, while break-suppressing controls (WJ, ZWJ, ZWNBSP)
    // prevent breaks
    if let ShapedItem::Cluster(c) = item {
        // ZW (zero-width space U+200B) is always a break opportunity
        if c.text.contains('\u{200B}') {
            return true;
        }
        // Break-forcing Unicode controls (LS, PS) create break opportunities
        if c.text.chars().any(is_break_forcing_control) {
            return true;
        }
        // WJ (word joiner U+2060), ZWJ (U+200D), and GL (NBSP U+00A0) suppress breaks
        if c.text.chars().any(|ch| matches!(ch, '\u{2060}' | '\u{200D}' | '\u{00A0}')) {
            return false;
        }
        // +spec:line-breaking:05e09a - U+002D/U+2010 always create soft wrap opportunities regardless of hyphens property
        // are always visible and create a soft wrap opportunity after them, but are NOT
        // hyphenation opportunities (no extra glyph is inserted at the break).
        if c.text.ends_with('\u{002D}') || c.text.ends_with('\u{2010}') {
            return true;
        }
    }
    is_break_opportunity_with_word_break(item, WordBreak::Normal, Hyphens::Manual)
}

// A cursor to manage the state of the line breaking process.
// This allows us to handle items that are partially consumed by hyphenation.
#[derive(Debug)]
pub struct BreakCursor<'a> {
    /// A reference to the complete list of shaped items.
    pub items: &'a [ShapedItem],
    /// The index of the next *full* item to be processed from the `items` slice.
    pub next_item_index: usize,
    /// The remainder of an item that was split by hyphenation on the previous line.
    /// This will be the very first piece of content considered for the next line.
    pub partial_remainder: Vec<ShapedItem>,
    // §5.2 word-break property stored on cursor
    pub word_break: WordBreak,
    pub hyphens: Hyphens,
    pub line_break: LineBreakStrictness,
}

impl<'a> BreakCursor<'a> {
    #[must_use] pub fn new(items: &'a [ShapedItem]) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
            word_break: WordBreak::Normal,
            hyphens: Hyphens::default(),
            line_break: LineBreakStrictness::default(),
        }
    }

    #[must_use] pub fn with_word_break(items: &'a [ShapedItem], word_break: WordBreak) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
            word_break,
            hyphens: Hyphens::default(),
            line_break: LineBreakStrictness::default(),
        }
    }

    /// Checks if the cursor is at the very beginning of the content stream.
    #[must_use] pub const fn is_at_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_empty()
    }

    /// Consumes the cursor and returns all remaining items as a `Vec`.
    pub fn drain_remaining(&mut self) -> Vec<ShapedItem> {
        let mut remaining = std::mem::take(&mut self.partial_remainder);
        if self.next_item_index < self.items.len() {
            remaining.extend_from_slice(&self.items[self.next_item_index..]);
        }
        self.next_item_index = self.items.len();
        remaining
    }

    /// Checks if all content, including any partial remainders, has been processed.
    #[must_use] pub const fn is_done(&self) -> bool {
        self.next_item_index >= self.items.len() && self.partial_remainder.is_empty()
    }

    /// Consumes a number of items from the cursor's stream.
    pub fn consume(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        let remainder_len = self.partial_remainder.len();
        if count <= remainder_len {
            // Consuming only from the remainder.
            self.partial_remainder.drain(..count);
        } else {
            // Consuming all of the remainder and some from the main list.
            let from_main_list = count - remainder_len;
            self.partial_remainder.clear();
            self.next_item_index += from_main_list;
        }
    }

    /// Looks ahead and returns the next "unbreakable" unit of content.
    /// This is typically a word (a series of non-space clusters) followed by a
    /// space, or just a single space if that's next.
    /// The definition of "unbreakable unit" depends on the word-break property.
    // a single typographic character unit (every character is a soft wrap opportunity), including
    // punctuation and preserved white spaces; currently handled via peek_next_single_item
    pub fn peek_next_unit(&self) -> Vec<ShapedItem> {
        let mut unit = Vec::new();
        let mut source_items = self.partial_remainder.clone();
        source_items.extend_from_slice(&self.items[self.next_item_index..]);

        if source_items.is_empty() {
            return unit;
        }

        // If the first item is a break opportunity (like a space), it's a unit on its own.
        if is_break_opportunity_with_word_break(&source_items[0], self.word_break, self.hyphens) {
            unit.push(source_items[0].clone());
            return unit;
        }

        // Otherwise, collect all items until the next break opportunity.
        // For break-all: each cluster is its own unit.
        // For keep-all: CJK sequences are NOT break opportunities.
        // For normal: CJK characters are individual break opportunities.
        // glue items together: if the last cluster ends with a break-suppressing control,
        // the next item cannot be separated from it.
        let mut suppress_next_break = false;
        for (i, item) in source_items.iter().enumerate() {
            // Also suppress break if this item starts with a break-suppressing control
            // (WJ/ZWJ/ZWNBSP suppress breaks on both sides per Unicode line breaking)
            let starts_with_suppress = if let ShapedItem::Cluster(c) = item {
                c.text.chars().next().is_some_and(is_break_suppressing_control)
            } else {
                false
            };
            // If the item is a CJK cluster, check if the break is allowed by strictness
            let cjk_strictness_suppressed = if let ShapedItem::Cluster(c) = item {
                c.text.chars().next().is_some_and(|ch| {
                    !is_cjk_break_allowed_by_strictness(ch, None, self.line_break)
                })
            } else {
                false
            };
            if i > 0 && !suppress_next_break && !starts_with_suppress && !cjk_strictness_suppressed && is_break_opportunity_with_word_break(item, self.word_break, self.hyphens) {
                break;
            }
            suppress_next_break = false;
            unit.push(item.clone());

            // Check if this item ends with a break-suppressing control character
            if let ShapedItem::Cluster(c) = item {
                if let Some(last_ch) = c.text.chars().last() {
                    if is_break_suppressing_control(last_ch) {
                        suppress_next_break = true;
                    }
                }
            }

            // For break-all, each non-space cluster is a unit on its own
            if self.word_break == WordBreak::BreakAll {
                if let ShapedItem::Cluster(_) = item {
                    break;
                }
            }
        }
        unit
    }

    #[must_use] pub fn peek_next_single_item(&self) -> Vec<ShapedItem> {
        if !self.partial_remainder.is_empty() {
            return vec![self.partial_remainder[0].clone()];
        }
        if self.next_item_index < self.items.len() {
            return vec![self.items[self.next_item_index].clone()];
        }
        Vec::new()
    }
}

// A structured result from a hyphenation attempt.
struct HyphenationResult {
    /// The items that fit on the current line, including the new hyphen.
    line_part: Vec<ShapedItem>,
    /// The remainder of the split item to be carried over to the next line.
    remainder_part: Vec<ShapedItem>,
}

fn perform_bidi_analysis<'a>(
    styled_runs: &'a [TextRunInfo<'_>],
    full_text: &'a str,
    force_lang: Option<Language>,
) -> (Vec<VisualRun<'a>>, BidiDirection) {
    if full_text.is_empty() {
        return (Vec::new(), BidiDirection::Ltr);
    }

    let bidi_info = BidiInfo::new(full_text, None);
    let para = &bidi_info.paragraphs[0];
    let base_direction = if para.level.is_rtl() {
        BidiDirection::Rtl
    } else {
        BidiDirection::Ltr
    };

    // Create a map from each byte index to its original styled run.
    let mut byte_to_run_index: Vec<usize> = vec![0; full_text.len()];
    for (run_idx, run) in styled_runs.iter().enumerate() {
        let start = run.logical_start;
        let end = start + run.text.len();
        for slot in &mut byte_to_run_index[start..end] {
            *slot = run_idx;
        }
    }

    let mut final_visual_runs = Vec::new();
    let (levels, visual_run_ranges) = bidi_info.visual_runs(para, para.range.clone());

    for range in visual_run_ranges {
        let bidi_level = levels[range.start];
        let mut sub_run_start = range.start;

        // Iterate through the bytes of the visual run to detect style changes.
        for i in (range.start + 1)..range.end {
            if byte_to_run_index[i] != byte_to_run_index[sub_run_start] {
                // Style boundary found. Finalize the previous sub-run.
                let original_run_idx = byte_to_run_index[sub_run_start];
                let script = crate::text3::script::detect_script(&full_text[sub_run_start..i])
                    .unwrap_or(Script::Latin);
                final_visual_runs.push(VisualRun {
                    text_slice: &full_text[sub_run_start..i],
                    style: styled_runs[original_run_idx].style.clone(),
                    logical_start_byte: sub_run_start,
                    bidi_level: BidiLevel::new(bidi_level.number()),
                    language: force_lang.unwrap_or_else(|| {
                        script_to_language(
                            script,
                            &full_text[sub_run_start..i],
                        )
                    }),
                    script,
                });
                // Start a new sub-run.
                sub_run_start = i;
            }
        }

        // Add the last sub-run (or the only one if no style change occurred).
        let original_run_idx = byte_to_run_index[sub_run_start];
        let script = crate::text3::script::detect_script(&full_text[sub_run_start..range.end])
            .unwrap_or(Script::Latin);

        final_visual_runs.push(VisualRun {
            text_slice: &full_text[sub_run_start..range.end],
            style: styled_runs[original_run_idx].style.clone(),
            logical_start_byte: sub_run_start,
            bidi_level: BidiLevel::new(bidi_level.number()),
            script,
            language: force_lang.unwrap_or_else(|| {
                script_to_language(
                    script,
                    &full_text[sub_run_start..range.end],
                )
            }),
        });
    }

    (final_visual_runs, base_direction)
}

const fn get_justification_priority(class: CharacterClass) -> u8 {
    match class {
        CharacterClass::Space => 0,
        CharacterClass::Punctuation => 64,
        CharacterClass::Ideograph => 128,
        CharacterClass::Letter => 192,
        CharacterClass::Symbol => 224,
        CharacterClass::Combining => 255,
    }
}

#[cfg(test)]
mod shape_outside_and_ruby_tests {
    use super::*;
    use azul_css::shape::{CssShape, ShapePath};

    fn path_shape(d: &str) -> CssShape {
        CssShape::Path(ShapePath {
            data: d.into(),
        })
    }

    // --- shape-outside: path() ----------------------------------------------

    #[test]
    fn css_path_shape_builds_path_boundary_not_rect_fallback() {
        // A right triangle (0,0)-(100,0)-(0,100).
        let shape = path_shape("M 0 0 L 100 0 L 0 100 Z");
        let rbox = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let boundary = ShapeBoundary::from_css_shape(&shape, rbox, &mut None);
        match boundary {
            ShapeBoundary::Path { segments } => {
                assert!(!segments.is_empty(), "path() must flatten to real segments");
                assert!(matches!(segments[0], PathSegment::MoveTo(_)));
                assert!(segments.iter().any(|s| matches!(s, PathSegment::Close)));
            }
            other => panic!("expected ShapeBoundary::Path, got {:?}", other),
        }
    }

    #[test]
    fn empty_or_garbage_path_falls_back_to_rectangle() {
        let rbox = Rect { x: 0.0, y: 0.0, width: 50.0, height: 50.0 };
        let boundary = ShapeBoundary::from_css_shape(&path_shape("   "), rbox, &mut None);
        assert!(matches!(boundary, ShapeBoundary::Rectangle(_)),
            "unparseable path() should fall back to the reference rectangle");
    }

    #[test]
    fn path_triangle_narrows_line_box_per_scanline() {
        // Right triangle with the hypotenuse running (100,0) -> (0,100).
        // At scanline y, the shape spans x in [0, 100 - y]. So the available band
        // must NARROW as y increases — the proof that real path geometry (not a
        // full-width rect) drives the per-line exclusion.
        let shape = path_shape("M 0 0 L 100 0 L 0 100 Z");
        let rbox = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let boundary = ShapeBoundary::from_css_shape(&shape, rbox, &mut None);

        let spans_top = get_shape_horizontal_spans(&boundary, 10.0, 1.0).unwrap();
        let spans_bot = get_shape_horizontal_spans(&boundary, 80.0, 1.0).unwrap();

        assert_eq!(spans_top.len(), 1, "single span expected near the top");
        assert_eq!(spans_bot.len(), 1, "single span expected near the bottom");

        let width_top = spans_top[0].1 - spans_top[0].0;
        let width_bot = spans_bot[0].1 - spans_bot[0].0;

        // Geometry check: width ~= 100 - y (line center is y + 0.5).
        assert!((width_top - 89.5).abs() < 1.5, "top width {} != ~89.5", width_top);
        assert!((width_bot - 19.5).abs() < 1.5, "bottom width {} != ~19.5", width_bot);
        assert!(width_top > width_bot,
            "path() exclusion band must narrow with y ({} !> {})", width_top, width_bot);

        // And it must differ from a plain full-width rectangle (which would be 0..100
        // at every scanline) — i.e. this is not the old rect/empty stub.
        assert!(width_bot < 50.0, "rect fallback would give full width here");
    }

    #[test]
    fn path_with_hole_carves_out_interior_via_even_odd() {
        // Outer square 0..100 with an inner reversed square 30..70 (a hole). At a
        // scanline through the hole, even-odd fill yields two spans straddling the hole.
        let shape = path_shape(
            "M 0 0 L 100 0 L 100 100 L 0 100 Z \
             M 30 30 L 30 70 L 70 70 L 70 30 Z",
        );
        let rbox = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let boundary = ShapeBoundary::from_css_shape(&shape, rbox, &mut None);
        let spans = get_shape_horizontal_spans(&boundary, 50.0, 1.0).unwrap();
        assert_eq!(spans.len(), 2, "hole should split the band into two spans: {:?}", spans);
    }

    // --- ruby ----------------------------------------------------------------

    #[test]
    fn ruby_annotation_font_scale_is_real_not_06_fudge() {
        // The annotation is sized at the used font-size of the ruby-text run, which the
        // UA stylesheet sets to 50% of the base — NOT a 0.6 per-character fudge.
        let base_font_size = 20.0_f32;
        let annotation_font_size = base_font_size * RUBY_ANNOTATION_FONT_SCALE;
        assert_eq!(annotation_font_size, 10.0);
        assert!((RUBY_ANNOTATION_FONT_SCALE - 0.6).abs() > f32::EPSILON,
            "annotation scale must not be the old 0.6 magic ratio");
    }

    #[test]
    fn ruby_box_reserves_max_width_and_stacks_annotation_above_base() {
        // Wider base, narrower annotation: reserved inline-size = base width.
        let (w, h) = ruby_reserved_box(80.0, 30.0, 24.0, 12.0);
        assert_eq!(w, 80.0, "reserved width is the wider of base/annotation");
        // Block-size stacks the annotation line above the base line => base reserves
        // vertical space for the annotation.
        assert_eq!(h, 36.0, "block-size = base line + annotation line");
        assert!(h > 24.0, "ruby box must reserve extra vertical space for the annotation");

        // Narrower base, wider annotation: reserved inline-size = annotation width.
        let (w2, _) = ruby_reserved_box(20.0, 50.0, 24.0, 12.0);
        assert_eq!(w2, 50.0, "a long annotation widens the reserved box");
    }
}
