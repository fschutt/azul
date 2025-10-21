use std::{
    any::{Any, TypeId},
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, Entry, HashMap},
        BTreeSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

pub use azul_core::selection::{ContentIndex, GraphemeClusterId};
use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::ColorU;
use hyphenation::{Hyphenator, Language, Load, Standard};
use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, FontId, PatternMatch, UnicodeRange};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;

use crate::text3::script::{script_to_language, Script};

// --- Core Data Structures for the New Architecture ---

pub trait ParsedFontTrait: Send + Clone {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: Direction,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph<Self>>, LayoutError>;
    /// Hash of the font, necessary for breaking layouted glyphs into glyph runs
    fn get_hash(&self) -> u64;
    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize>;
    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)>;
    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)>;
    fn has_glyph(&self, codepoint: u32) -> bool;
    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics>;
    fn get_font_metrics(&self) -> LayoutFontMetrics;
    fn num_glyphs(&self) -> u16;
}

pub trait FontLoaderTrait<T: ParsedFontTrait>: Send + core::fmt::Debug {
    fn load_font(&self, font_bytes: &[u8], font_index: usize) -> Result<Arc<T>, LayoutError>;
}

// Font loading and management
pub trait FontProviderTrait<T: ParsedFontTrait> {
    fn load_font(&self, font_selector: &FontSelector) -> Result<Arc<T>, LayoutError>;
}

#[derive(Debug)]
pub struct FontManager<T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    pub fc_cache: FcFontCache,
    pub parsed_fonts: Mutex<HashMap<FontId, Arc<T>>>,
    pub font_selector_to_id_cache: Mutex<HashMap<FontSelector, FontId>>,
    // Default: System font loader
    // (loads fonts from file - can be intercepted for mocking in tests)
    pub font_loader: Arc<Q>,
}

impl<T: ParsedFontTrait, Q: FontLoaderTrait<T>> FontManager<T, Q> {
    pub fn with_loader(fc_cache: FcFontCache, loader: Arc<Q>) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts: Mutex::new(HashMap::new()),
            font_loader: loader,
            font_selector_to_id_cache: Mutex::new(HashMap::new()),
        })
    }
}

// FontManager with proper rust-fontconfig fallback
impl<T: ParsedFontTrait, Q: FontLoaderTrait<T>> FontProviderTrait<T> for FontManager<T, Q> {
    fn load_font(&self, font_selector: &FontSelector) -> Result<Arc<T>, LayoutError> {
        // Check cache first
        if let Ok(c) = self.font_selector_to_id_cache.lock() {
            if let Some(cached_id) = c.get(font_selector) {
                let fonts = self.parsed_fonts.lock().unwrap();
                if let Some(font) = fonts.get(cached_id) {
                    return Ok(font.clone());
                }
            }
        }

        // Query fontconfig
        let pattern = FcPattern {
            name: Some(font_selector.family.clone()),
            weight: font_selector.weight,
            italic: if font_selector.style == FontStyle::Italic {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            oblique: if font_selector.style == FontStyle::Oblique {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            ..Default::default()
        };

        let mut trace = Vec::new();
        let fc_match = self.fc_cache.query(&pattern, &mut trace).ok_or_else(|| {
            eprintln!(
                "[FontManager] Font not found: '{}' (weight: {:?}, style: {:?})",
                font_selector.family, font_selector.weight, font_selector.style
            );
            eprintln!("[FontManager] FontConfig trace: {:?}", trace);
            LayoutError::FontNotFound(font_selector.clone())
        })?;

        // Load font if not cached
        {
            let mut fonts = self.parsed_fonts.lock().unwrap();
            if !fonts.contains_key(&fc_match.id) {
                let font_bytes = self
                    .fc_cache
                    .get_font_bytes(&fc_match.id)
                    .ok_or_else(|| LayoutError::FontNotFound(font_selector.clone()))?;

                let font_index = 0; // Default
                let parsed = self.font_loader.load_font(&font_bytes, font_index)?;

                fonts.insert(fc_match.id.clone(), parsed);
            }
        }

        // Update ref cache
        {
            let mut ref_cache = self.font_selector_to_id_cache.lock().unwrap();
            ref_cache.insert(font_selector.clone(), fc_match.id.clone());
        }

        let fonts = self.parsed_fonts.lock().unwrap();
        Ok(fonts.get(&fc_match.id).unwrap().clone())
    }
}

// Error handling
#[derive(Debug, thiserror::Error)]
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
pub struct CursorBoundsError {
    /// The boundary that was hit
    pub boundary: TextBoundary,
    /// The cursor position (unchanged from input)
    pub cursor: TextCursor,
}

/// Unified constraints combining all layout features
#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeBoundary>,

    // Basic layout
    pub available_width: f32, // For simple rectangular layouts
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    pub line_height: f32,
    pub vertical_align: VerticalAlign,

    // Overflow handling
    pub overflow: OverflowBehavior,
    pub segment_alignment: SegmentAlignment,

    // Advanced features
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: bool,
    pub hyphenation_language: Option<Language>,
    pub text_indent: f32,
    pub initial_letter: Option<InitialLetter>,
    pub line_clamp: Option<NonZeroUsize>,

    // text-wrap: balance
    pub text_wrap: TextWrap,
    pub columns: u32,
    pub column_gap: f32,
    pub hanging_punctuation: bool,
}

impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            shape_boundaries: Vec::new(),
            shape_exclusions: Vec::new(),
            available_width: 0.0,
            available_height: None,
            writing_mode: None,
            text_orientation: TextOrientation::default(),
            text_align: TextAlign::default(),
            text_justify: JustifyContent::default(),
            line_height: 16.0, // A more sensible default
            vertical_align: VerticalAlign::default(),
            overflow: OverflowBehavior::default(),
            segment_alignment: SegmentAlignment::default(),
            text_combine_upright: None,
            exclusion_margin: 0.0,
            hyphenation: false,
            hyphenation_language: None,
            columns: 1,
            column_gap: 0.0,
            hanging_punctuation: false,
            text_indent: 0.0,
            initial_letter: None,
            line_clamp: None,
            text_wrap: TextWrap::default(),
        }
    }
}

// UnifiedConstraints
impl Hash for UnifiedConstraints {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_boundaries.hash(state);
        self.shape_exclusions.hash(state);
        (self.available_width.round() as usize).hash(state);
        self.available_height
            .map(|h| h.round() as usize)
            .hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_align.hash(state);
        self.text_justify.hash(state);
        (self.line_height.round() as usize).hash(state);
        self.vertical_align.hash(state);
        self.overflow.hash(state);
        self.text_combine_upright.hash(state);
        (self.exclusion_margin.round() as usize).hash(state);
        self.hyphenation.hash(state);
        self.hyphenation_language.hash(state);
        self.columns.hash(state);
        (self.column_gap.round() as usize).hash(state);
        self.hanging_punctuation.hash(state);
    }
}

impl PartialEq for UnifiedConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.shape_boundaries == other.shape_boundaries
            && self.shape_exclusions == other.shape_exclusions
            && round_eq(self.available_width, other.available_width)
            && match (self.available_height, other.available_height) {
                (None, None) => true,
                (Some(h1), Some(h2)) => round_eq(h1, h2),
                _ => false,
            }
            && self.writing_mode == other.writing_mode
            && self.text_orientation == other.text_orientation
            && self.text_align == other.text_align
            && self.text_justify == other.text_justify
            && round_eq(self.line_height, other.line_height)
            && self.vertical_align == other.vertical_align
            && self.overflow == other.overflow
            && self.text_combine_upright == other.text_combine_upright
            && round_eq(self.exclusion_margin, other.exclusion_margin)
            && self.hyphenation == other.hyphenation
            && self.hyphenation_language == other.hyphenation_language
            && self.columns == other.columns
            && round_eq(self.column_gap, other.column_gap)
            && self.hanging_punctuation == other.hanging_punctuation
    }
}

impl Eq for UnifiedConstraints {}

impl UnifiedConstraints {
    fn direction(&self, fallback: Direction) -> Direction {
        match self.writing_mode {
            Some(s) => s.get_direction().unwrap_or(fallback),
            None => fallback,
        }
    }
    fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            Some(WritingMode::VerticalRl) | Some(WritingMode::VerticalLr)
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
    fn get_direction(&self) -> Option<Direction> {
        match self {
            WritingMode::HorizontalTb => None, // determined by text content
            WritingMode::VerticalRl => Some(Direction::Rtl),
            WritingMode::VerticalLr => Some(Direction::Ltr),
            WritingMode::SidewaysRl => Some(Direction::Rtl),
            WritingMode::SidewaysLr => Some(Direction::Ltr),
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
/// Used by FontManager to query fontconfig and load font files.
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

/// A reference to a font for rendering, identified by its hash.
/// This hash corresponds to ParsedFont::hash and is used to look up
/// the actual font data in the renderer's font cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontRef {
    /// The hash of the ParsedFont. 0 means invalid/unknown font.
    pub font_hash: u64,
}

impl FontRef {
    pub fn invalid() -> Self {
        Self { font_hash: 0 }
    }

    pub fn from_hash(font_hash: u64) -> Self {
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
    /// Align text relative to the total available width of all segments on the line combined.
    Total,
}

#[derive(Debug, Clone)]
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

/// Layout-specific font metrics extracted from FontMetrics
/// Contains only the metrics needed for text layout and rendering
#[derive(Debug, Clone)]
pub struct LayoutFontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
}

impl LayoutFontMetrics {
    pub fn baseline_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em as f32;
        self.ascent * scale
    }

    /// Convert from full FontMetrics to layout-specific metrics
    pub fn from_font_metrics(metrics: &azul_css::props::basic::FontMetrics) -> Self {
        Self {
            ascent: metrics.ascender as f32,
            descent: metrics.descender as f32,
            line_gap: metrics.line_gap as f32,
            units_per_em: metrics.units_per_em,
        }
    }
}

#[derive(Debug, Clone)]
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

// initial-letter
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct InitialLetter {
    /// How many lines tall the initial letter should be.
    pub size: f32,
    /// How many lines the letter should sink into.
    pub sink: u32,
    /// How many characters to apply this styling to.
    pub count: NonZeroUsize,
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// This is a marker trait, indicating that `a == b` is a true equivalence
// relation. The derived `PartialEq` already satisfies this.
impl Eq for InitialLetter {}

impl Hash for InitialLetter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Per the request, round the f32 to a usize for hashing.
        // This is a lossy conversion; values like 2.3 and 2.4 will produce
        // the same hash value for this field. This is acceptable as long as
        // the `PartialEq` implementation correctly distinguishes them.
        (self.size.round() as usize).hash(state);
        self.sink.hash(state);
        self.count.hash(state);
    }
}

// Path and shape definitions
#[derive(Debug, Clone, PartialOrd)]
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the enum variant's discriminant first to distinguish them
        discriminant(self).hash(state);

        match self {
            PathSegment::MoveTo(p) => p.hash(state),
            PathSegment::LineTo(p) => p.hash(state),
            PathSegment::CurveTo {
                control1,
                control2,
                end,
            } => {
                control1.hash(state);
                control2.hash(state);
                end.hash(state);
            }
            PathSegment::QuadTo { control, end } => {
                control.hash(state);
                end.hash(state);
            }
            PathSegment::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
                (start_angle.round() as usize).hash(state);
                (end_angle.round() as usize).hash(state);
            }
            PathSegment::Close => {} // No data to hash
        }
    }
}

impl PartialEq for PathSegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PathSegment::MoveTo(a), PathSegment::MoveTo(b)) => a == b,
            (PathSegment::LineTo(a), PathSegment::LineTo(b)) => a == b,
            (
                PathSegment::CurveTo {
                    control1: c1a,
                    control2: c2a,
                    end: ea,
                },
                PathSegment::CurveTo {
                    control1: c1b,
                    control2: c2b,
                    end: eb,
                },
            ) => c1a == c1b && c2a == c2b && ea == eb,
            (
                PathSegment::QuadTo {
                    control: ca,
                    end: ea,
                },
                PathSegment::QuadTo {
                    control: cb,
                    end: eb,
                },
            ) => ca == cb && ea == eb,
            (
                PathSegment::Arc {
                    center: ca,
                    radius: ra,
                    start_angle: sa_a,
                    end_angle: ea_a,
                },
                PathSegment::Arc {
                    center: cb,
                    radius: rb,
                    start_angle: sa_b,
                    end_angle: ea_b,
                },
            ) => ca == cb && round_eq(*ra, *rb) && round_eq(*sa_a, *sa_b) && round_eq(*ea_a, *ea_b),
            (PathSegment::Close, PathSegment::Close) => true,
            _ => false, // Variants are different
        }
    }
}

impl Eq for PathSegment {}

// Enhanced content model supporting mixed inline content
#[derive(Debug, Clone, Hash)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
    Tab,
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
    pub baseline_offset: f32, // How much to shift baseline
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
pub struct Glyph<T: ParsedFontTrait> {
    // Core glyph data
    pub glyph_id: u16,
    pub codepoint: char,
    pub font: Arc<T>,
    pub style: Arc<StyleProperties>,
    pub source: GlyphSource,

    // Text mapping
    pub logical_byte_index: usize,
    pub logical_byte_len: usize,
    pub content_index: usize,
    pub cluster: u32,

    // Metrics
    pub advance: f32,
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

impl<T: ParsedFontTrait> Glyph<T> {
    #[inline]
    fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.advance,
            height: self.style.line_height,
        }
    }

    #[inline]
    fn character_class(&self) -> CharacterClass {
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
    fn justification_priority(&self) -> u8 {
        get_justification_priority(self.character_class())
    }

    #[inline]
    fn break_opportunity_after(&self) -> bool {
        let is_whitespace = self.codepoint.is_whitespace();
        let is_soft_hyphen = self.codepoint == '\u{00AD}';
        is_whitespace || is_soft_hyphen
    }
}

// Information about text runs after initial analysis
#[derive(Debug, Clone)]
pub struct TextRunInfo<'a> {
    pub text: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start: usize,
    pub content_index: usize,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageSource {
    Url(String),
    Data(Arc<[u8]>),
    Svg(Arc<str>),
    Placeholder(Size), // For layout without actual image
}

#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerticalAlign {
    #[default]
    Baseline, // Align image baseline with text baseline
    Bottom,     // Align image bottom with line bottom
    Top,        // Align image top with line top
    Middle,     // Align image middle with text middle
    TextTop,    // Align with tallest text in line
    TextBottom, // Align with lowest text in line
    Sub,        // Subscript alignment
    Super,      /* Superscript alignment
                 * Offset(f32), // Custom offset from baseline */
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFit {
    Fill,      // Stretch to fit display size
    Contain,   // Scale to fit within display size
    Cover,     // Scale to cover display size
    None,      // Use intrinsic size
    ScaleDown, // Like contain but never scale up
}

#[derive(Debug, Clone)]
pub struct InlineShape {
    pub shape_def: ShapeDefinition,
    pub fill: Option<ColorU>,
    pub stroke: Option<Stroke>,
    pub baseline_offset: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverflowBehavior {
    Visible, // Content extends outside shape
    Hidden,  // Content is clipped to shape
    Scroll,  // Scrollable overflow
    #[default]
    Auto, // Browser/system decides
    Break,   // Break into next shape/page
}

#[derive(Debug, Clone)]
pub struct MeasuredImage {
    pub source: ImageSource,
    pub size: Size,
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct MeasuredShape {
    pub shape_def: ShapeDefinition,
    pub size: Size,
    pub baseline_offset: f32,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
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
    }
}

impl Eq for InlineShape {}

impl Hash for InlineShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_def.hash(state);
        self.fill.hash(state);
        self.stroke.hash(state);
        self.baseline_offset.to_bits().hash(state);
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
                .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset)),
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The order in which you hash the fields matters.
        // A consistent order is crucial.
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Ord for Size {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.width.round() as usize)
            .cmp(&(other.width.round() as usize))
            .then_with(|| (self.height.round() as usize).cmp(&(other.height.round() as usize)))
    }
}

// Size
impl Hash for Size {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}
impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.width, other.width) && round_eq(self.height, other.height)
    }
}
impl Eq for Size {}

impl Size {
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    pub const fn new(width: f32, height: f32) -> Self {
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeDefinition::Rectangle {
                size,
                corner_radius,
            } => {
                size.hash(state);
                corner_radius.map(|r| r.round() as usize).hash(state);
            }
            ShapeDefinition::Circle { radius } => {
                (radius.round() as usize).hash(state);
            }
            ShapeDefinition::Ellipse { radii } => {
                radii.hash(state);
            }
            ShapeDefinition::Polygon { points } => {
                // Since Point implements Hash, we can hash the Vec directly.
                points.hash(state);
            }
            ShapeDefinition::Path { segments } => {
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
                ShapeDefinition::Rectangle {
                    size: s1,
                    corner_radius: r1,
                },
                ShapeDefinition::Rectangle {
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
            (ShapeDefinition::Circle { radius: r1 }, ShapeDefinition::Circle { radius: r2 }) => {
                round_eq(*r1, *r2)
            }
            (ShapeDefinition::Ellipse { radii: r1 }, ShapeDefinition::Ellipse { radii: r2 }) => {
                r1 == r2
            }
            (ShapeDefinition::Polygon { points: p1 }, ShapeDefinition::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeDefinition::Path { segments: s1 }, ShapeDefinition::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeDefinition {}

impl ShapeDefinition {
    /// Calculates the bounding box size for the shape.
    pub fn get_size(&self) -> Size {
        match self {
            // The size is explicitly defined.
            ShapeDefinition::Rectangle { size, .. } => *size,

            // The bounding box of a circle is a square with sides equal to the diameter.
            ShapeDefinition::Circle { radius } => {
                let diameter = radius * 2.0;
                Size::new(diameter, diameter)
            }

            // The bounding box of an ellipse has width and height equal to twice its radii.
            ShapeDefinition::Ellipse { radii } => Size::new(radii.width * 2.0, radii.height * 2.0),

            // For a polygon, we must find the min/max coordinates to get the bounds.
            ShapeDefinition::Polygon { points } => calculate_bounding_box_size(points),

            // For a path, we find the bounding box of all its anchor and control points.
            //
            // NOTE: This is a common and fast approximation. The true bounding box of
            // bezier curves can be slightly smaller than the box containing their control
            // points. For pixel-perfect results, one would need to calculate the
            // curve's extrema.
            ShapeDefinition::Path { segments } => {
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        (self.width.round() as usize).hash(state);

        // Manual hashing for Option<Vec<f32>>
        match &self.dash_pattern {
            None => 0u8.hash(state), // Hash a discriminant for None
            Some(pattern) => {
                1u8.hash(state); // Hash a discriminant for Some
                pattern.len().hash(state); // Hash the length
                for &val in pattern {
                    (val.round() as usize).hash(state); // Hash each rounded value
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
    pub fn inflate(&self, margin: f32) -> Self {
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeBoundary::Rectangle(rect) => rect.hash(state),
            ShapeBoundary::Circle { center, radius } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
            }
            ShapeBoundary::Ellipse { center, radii } => {
                center.hash(state);
                radii.hash(state);
            }
            ShapeBoundary::Polygon { points } => points.hash(state),
            ShapeBoundary::Path { segments } => segments.hash(state),
        }
    }
}
impl PartialEq for ShapeBoundary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ShapeBoundary::Rectangle(r1), ShapeBoundary::Rectangle(r2)) => r1 == r2,
            (
                ShapeBoundary::Circle {
                    center: c1,
                    radius: r1,
                },
                ShapeBoundary::Circle {
                    center: c2,
                    radius: r2,
                },
            ) => c1 == c2 && round_eq(*r1, *r2),
            (
                ShapeBoundary::Ellipse {
                    center: c1,
                    radii: r1,
                },
                ShapeBoundary::Ellipse {
                    center: c2,
                    radii: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (ShapeBoundary::Polygon { points: p1 }, ShapeBoundary::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeBoundary::Path { segments: s1 }, ShapeBoundary::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
    pub content_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BreakType {
    Soft,   // Preferred break (like <wbr>)
    Hard,   // Forced break (like <br>)
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
pub struct ShapeConstraints {
    pub boundaries: Vec<ShapeBoundary>,
    pub exclusions: Vec<ShapeBoundary>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum WritingMode {
    #[default]
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl, // vertical-rl (vertical right-to-left)
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

impl WritingMode {
    /// Necessary to determine if the glyphs are advancing in a horizontal direction
    pub fn is_advance_horizontal(&self) -> bool {
        matches!(
            self,
            WritingMode::HorizontalTb | WritingMode::SidewaysRl | WritingMode::SidewaysLr
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

// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, PartialOrd, Ord, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

impl Default for TextDecoration {
    fn default() -> Self {
        TextDecoration {
            underline: false,
            overline: false,
            strikethrough: false,
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
            Spacing::Px(val) => val.hash(state),
            // For hashing floats, convert them to their raw bit representation.
            // This ensures that identical float values produce identical hashes.
            Spacing::Em(val) => val.to_bits().hash(state),
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing::Px(0)
    }
}

impl Default for FontRef {
    fn default() -> Self {
        Self::invalid()
    }
}

/// Style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    pub font_selector: FontSelector,
    pub font_size_px: f32,
    pub color: ColorU,
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,

    pub line_height: f32,
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
            font_selector: FontSelector::default(),
            font_size_px: FONT_SIZE,
            color: ColorU::default(),
            letter_spacing: Spacing::default(), // Px(0)
            word_spacing: Spacing::default(),   // Px(0)
            line_height: FONT_SIZE * 1.2,
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
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_selector.hash(state);
        self.color.hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // For f32 fields, round and cast to usize before hashing.
        (self.font_size_px.round() as usize).hash(state);
        (self.line_height.round() as usize).hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharacterClass {
    Space,       // Regular spaces - highest justification priority
    Punctuation, // Can sometimes be adjusted
    Letter,      // Normal letters
    Ideograph,   // CJK characters - can be justified between
    Symbol,      // Symbols, emojis
    Combining,   // Combining marks - never justified
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphOrientation {
    Horizontal, // Keep horizontal (normal in horizontal text)
    Vertical,   // Rotate to vertical (normal in vertical text)
    Upright,    // Keep upright regardless of writing mode
    Mixed,      // Use script-specific default orientation
}

// Bidi and script detection
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Ltr,
    Rtl,
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
    pub fn new(level: u8) -> Self {
        Self(level)
    }
    pub fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    pub fn level(&self) -> u8 {
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
    pub font_selector: Option<FontSelector>,
    pub font_size_px: Option<f32>,
    pub color: Option<ColorU>,
    pub letter_spacing: Option<Spacing>,
    pub word_spacing: Option<Spacing>,
    pub line_height: Option<f32>,
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
        self.font_selector.hash(state);
        self.font_size_px.map(|f| f.to_bits()).hash(state);
        self.color.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);
        self.line_height.map(|f| f.to_bits()).hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);

        // Manual hashing for Vec<(FourCc, f32)>
        self.font_variations.as_ref().map(|v| {
            for (tag, val) in v {
                tag.hash(state);
                val.to_bits().hash(state);
            }
        });

        self.tab_size.map(|f| f.to_bits()).hash(state);
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
        self.font_selector == other.font_selector &&
        self.font_size_px.map(|f| f.to_bits()) == other.font_size_px.map(|f| f.to_bits()) &&
        self.color == other.color &&
        self.letter_spacing == other.letter_spacing &&
        self.word_spacing == other.word_spacing &&
        self.line_height.map(|f| f.to_bits()) == other.line_height.map(|f| f.to_bits()) &&
        self.text_decoration == other.text_decoration &&
        self.font_features == other.font_features &&
        self.font_variations == other.font_variations && // Vec<(FourCc, f32)> is PartialEq
        self.tab_size.map(|f| f.to_bits()) == other.tab_size.map(|f| f.to_bits()) &&
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
        if let Some(val) = &partial.font_selector {
            new_style.font_selector = val.clone();
        }
        if let Some(val) = partial.font_size_px {
            new_style.font_size_px = val;
        }
        if let Some(val) = &partial.color {
            new_style.color = val.clone();
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
            new_style.text_decoration = val.clone();
        }
        if let Some(val) = &partial.font_features {
            new_style.font_features = val.clone();
        }
        if let Some(val) = &partial.font_variations {
            new_style.font_variations = val.clone();
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
            new_style.text_combine_upright = val.clone();
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

#[derive(Debug, Clone)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item (often a single grapheme cluster).
        text: String,
        style: Arc<StyleProperties>,
    },
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
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            LogicalItem::Text {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
            }
            LogicalItem::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Ruby {
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
            LogicalItem::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            LogicalItem::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Break { source, break_info } => {
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
    /// A single LogicalItem can be split into multiple VisualItems.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
}

// --- Stage 3: Shaped Representation ---

#[derive(Debug, Clone)]
pub enum ShapedItem<T: ParsedFontTrait> {
    Cluster(ShapedCluster<T>),
    /// A block of combined text (tate-chu-yoko) that is laid out as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: Vec<ShapedGlyph<T>>,
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

impl<T: ParsedFontTrait> ShapedItem<T> {
    fn as_cluster(&self) -> Option<&ShapedCluster<T>> {
        match self {
            ShapedItem::Cluster(c) => Some(c),
            _ => None,
        }
    }
    /// Returns the bounding box of the item, relative to its own origin.
    ///
    /// The origin of the returned `Rect` is `(0,0)`, representing the top-left corner
    /// of the item's layout space before final positioning. The size represents the
    /// item's total advance (width in horizontal mode) and its line height (ascent + descent).
    pub fn bounds(&self) -> Rect {
        match self {
            ShapedItem::Cluster(cluster) => {
                // The width of a text cluster is its total advance.
                let width = cluster.advance;

                // The height is the sum of its ascent and descent, which defines its line box.
                // We use the existing helper function which correctly calculates this from font
                // metrics.
                let (ascent, descent) = get_item_vertical_metrics(self);
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
            ShapedItem::CombinedBlock { bounds, .. } => *bounds,
            ShapedItem::Object { bounds, .. } => *bounds,
            ShapedItem::Tab { bounds, .. } => *bounds,

            // Breaks are control characters and have no visual geometry.
            ShapedItem::Break { .. } => Rect::default(), // A zero-sized rectangle.
        }
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone)]
pub struct ShapedCluster<T: ParsedFontTrait> {
    /// The original text that this cluster was shaped from.
    /// This is crucial for correct hyphenation.
    pub text: String,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The glyphs that make up this cluster.
    pub glyphs: Vec<ShapedGlyph<T>>,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: Direction,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
}

/// A single, shaped glyph with its essential metrics.
#[derive(Debug, Clone)]
pub struct ShapedGlyph<T: ParsedFontTrait> {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text)
    pub advance: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    pub style: Arc<StyleProperties>,
    pub font: Arc<T>,
}

impl<T: ParsedFontTrait> ShapedGlyph<T> {
    pub fn into_glyph_instance(&self, writing_mode: WritingMode) -> GlyphInstance {
        let size = self
            .font
            .get_glyph_size(self.glyph_id, self.style.font_size_px)
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
            index: self.glyph_id as u32,
            point: position,
            size,
        }
    }
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone)]
pub struct PositionedItem<T: ParsedFontTrait> {
    pub item: ShapedItem<T>,
    pub position: Point,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedLayout<T: ParsedFontTrait> {
    pub items: Vec<PositionedItem<T>>,
    pub bounds: Rect,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo<T>,
    /// Map of font hashes to the actual parsed fonts used in this layout.
    /// This allows the renderer to register all fonts needed for this layout
    /// after the layout is complete, avoiding the need to pre-register fonts.
    pub used_fonts: std::collections::BTreeMap<u64, Arc<T>>,
}

impl<T: ParsedFontTrait> UnifiedLayout<T> {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn last_baseline(&self) -> Option<f32> {
        self.items
            .iter()
            .rev()
            .find_map(|item| get_baseline_for_item(&item.item))
    }

    /// Takes a point relative to the layout's origin and returns the closest
    /// logical cursor position.
    pub fn hittest_cursor(&self, point: LogicalPosition) -> Option<TextCursor> {
        // Find the line that contains the point's Y coordinate.
        let target_line = self
            .items
            .iter()
            .filter(|item| {
                item.position.y <= point.y && point.y < item.position.y + item.item.bounds().height
            })
            .min_by(|a, b| {
                (a.position.y - point.y)
                    .abs()
                    .partial_cmp(&(b.position.y - point.y).abs())
                    .unwrap_or(Ordering::Equal)
            });

        let target_line_idx = match target_line {
            Some(item) => item.line_index,
            None => return None, // No line at this y-coordinate
        };

        let items_on_line: Vec<_> = self
            .items
            .iter()
            .filter(|i| i.line_index == target_line_idx)
            .collect();
        if items_on_line.is_empty() {
            return None;
        }

        // Find the item on the line that contains the point's X coordinate
        let target_item = items_on_line.iter().min_by(|a, b| {
            (a.position.x - point.x)
                .abs()
                .partial_cmp(&(b.position.x - point.x).abs())
                .unwrap_or(Ordering::Equal)
        })?;

        let cluster = match &target_item.item {
            ShapedItem::Cluster(c) => c,
            // Objects are treated as a single cluster for selection
            ShapedItem::Object { source, .. } | ShapedItem::CombinedBlock { source, .. } => {
                return Some(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: source.run_index,
                        start_byte_in_run: source.item_index,
                    },
                    affinity: if point.x
                        < target_item.position.x + (target_item.item.bounds().width / 2.0)
                    {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                });
            }
            _ => return None,
        };

        // Find the specific glyph within the cluster
        let mut pen_x = target_item.position.x;
        for glyph in &cluster.glyphs {
            let glyph_end_x = pen_x + glyph.advance;
            if point.x >= pen_x && point.x <= glyph_end_x {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: if point.x < pen_x + (glyph.advance / 2.0) {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                });
            }
            pen_x = glyph_end_x;
        }

        // If not inside any glyph, snap to the start or end of the cluster
        Some(TextCursor {
            cluster_id: cluster.source_cluster_id,
            affinity: if point.x < target_item.position.x {
                CursorAffinity::Leading
            } else {
                CursorAffinity::Trailing
            },
        })
    }

    /// Given a logical selection range, returns a vector of visual rectangles
    /// that cover the selected text, in the layout's coordinate space.
    pub fn get_selection_rects(&self, range: &SelectionRange) -> Vec<LogicalRect> {
        // 1. Build a map from the logical cluster ID to the visual PositionedItem for fast lookups.
        let mut cluster_map: HashMap<GraphemeClusterId, &PositionedItem<T>> = HashMap::new();
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
        let get_cursor_x = |item: &PositionedItem<T>, affinity: CursorAffinity| -> f32 {
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
    pub fn get_cursor_rect(&self, cursor: &TextCursor) -> Option<LogicalRect> {
        // Find the item and glyph corresponding to the cursor's cluster ID.
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                if cluster.source_cluster_id == cursor.cluster_id {
                    // This is the correct cluster. Now find the position.
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
                        }, // 1px wide cursor
                    });
                }
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
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
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

        // If we're at trailing edge, move to leading edge of same cluster
        if cursor.affinity == CursorAffinity::Trailing {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: moving from trailing to leading edge of byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return TextCursor {
                cluster_id: cursor.cluster_id,
                affinity: CursorAffinity::Leading,
            };
        }

        // We're at leading edge, move to previous cluster's trailing edge
        // Search backwards for a cluster on the same line, or any cluster if at line start
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at leading edge, current line {}",
                current_line
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
                    "[Cursor] move_cursor_left: trying previous line {}",
                    prev_line
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
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
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

        // If we're at leading edge, move to trailing edge of same cluster
        if cursor.affinity == CursorAffinity::Leading {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_right: moving from leading to trailing edge of byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return TextCursor {
                cluster_id: cursor.cluster_id,
                affinity: CursorAffinity::Trailing,
            };
        }

        // We're at trailing edge, move to next cluster's leading edge
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at trailing edge, current line {}",
                current_line
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
                "[Cursor] move_cursor_right: trying next line {}",
                next_line
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
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
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
            .map(|i| i.position.y + (i.item.bounds().height / 2.0))
            .unwrap_or(current_item.position.y);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: target line {}, hittesting at ({}, {})",
                target_line_idx, current_x, target_y
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
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
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
            .map(|i| i.position.y + (i.item.bounds().height / 2.0))
            .unwrap_or(current_item.position.y);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: hit testing at ({}, {})",
                current_x, target_y
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
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
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
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
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

    /// Collects all unique fonts used in this layout into the used_fonts map.
    /// This should be called after the layout is complete to populate the font cache.
    pub fn collect_used_fonts(&mut self) {
        use std::collections::BTreeMap;

        let mut fonts: BTreeMap<u64, Arc<T>> = BTreeMap::new();

        for item in &self.items {
            match &item.item {
                ShapedItem::Cluster(cluster) => {
                    for glyph in &cluster.glyphs {
                        let hash = glyph.font.get_hash();
                        fonts.entry(hash).or_insert_with(|| glyph.font.clone());
                    }
                }
                ShapedItem::CombinedBlock { glyphs, .. } => {
                    for glyph in glyphs {
                        let hash = glyph.font.get_hash();
                        fonts.entry(hash).or_insert_with(|| glyph.font.clone());
                    }
                }
                _ => {}
            }
        }

        self.used_fonts = fonts;
    }
}

fn get_baseline_for_item<T: ParsedFontTrait>(item: &ShapedItem<T>) -> Option<f32> {
    match item {
        ShapedItem::CombinedBlock {
            baseline_offset, ..
        } => Some(*baseline_offset),
        ShapedItem::Object {
            baseline_offset, ..
        } => Some(*baseline_offset),
        // We have to get the clusters font from the last glyph
        ShapedItem::Cluster(ref cluster) => {
            if let Some(last_glyph) = cluster.glyphs.last() {
                Some(
                    last_glyph
                        .font
                        .get_font_metrics()
                        .baseline_scaled(last_glyph.style.font_size_px),
                )
            } else {
                None
            }
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
#[derive(Debug, Clone)]
pub struct OverflowInfo<T: ParsedFontTrait> {
    /// The items that did not fit within the constraints.
    pub overflow_items: Vec<ShapedItem<T>>,
    /// The total bounds of all content, including overflowing items.
    /// This is useful for `OverflowBehavior::Visible` or `Scroll`.
    pub unclipped_bounds: Rect,
}

impl<T: ParsedFontTrait> OverflowInfo<T> {
    pub fn has_overflow(&self) -> bool {
        !self.overflow_items.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for OverflowInfo<T> {
    fn default() -> Self {
        Self {
            overflow_items: Vec::new(),
            unclipped_bounds: Rect::default(),
        }
    }
}

/// Intermediate structure carrying information from the line breaker to the positioner.
#[derive(Debug, Clone)]
pub struct UnifiedLine<T: ParsedFontTrait> {
    pub items: Vec<ShapedItem<T>>,
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
pub struct FlowLayout<T: ParsedFontTrait> {
    /// A map from a fragment's unique ID to the layout it contains.
    pub fragment_layouts: HashMap<String, Arc<UnifiedLayout<T>>>,
    /// Any items that did not fit into the last fragment in the flow chain.
    /// This is useful for pagination or determining if more layout space is needed.
    pub remaining_items: Vec<ShapedItem<T>>,
}

pub struct LayoutCache<T: ParsedFontTrait> {
    // Stage 1 Cache: InlineContent -> LogicalItems
    logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,
    // Stage 2 Cache: LogicalItems -> VisualItems
    visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,
    // Stage 3 Cache: VisualItems -> ShapedItems (now strongly typed)
    shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem<T>>>>,
    // Stage 4 Cache: ShapedItems + Constraints -> Final Layout (now strongly typed)
    layouts: HashMap<CacheId, Arc<UnifiedLayout<T>>>,
}

impl<T: ParsedFontTrait> LayoutCache<T> {
    pub fn new() -> Self {
        Self {
            logical_items: HashMap::new(),
            visual_items: HashMap::new(),
            shaped_items: HashMap::new(),
            layouts: HashMap::new(),
        }
    }
}

impl<T: ParsedFontTrait> Default for LayoutCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Key for caching the conversion from `InlineContent` to `LogicalItem`s.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LogicalItemsKey<'a> {
    pub inline_content_hash: u64, // Pre-hash the content for efficiency
    pub default_font_size: u32,   // Affects space widths
    // Add other relevant properties from constraints if they affect this stage
    pub _marker: std::marker::PhantomData<&'a ()>,
}

/// Key for caching the Bidi reordering stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct VisualItemsKey {
    pub logical_items_id: CacheId,
    pub base_direction: Direction,
}

/// Key for caching the shaping stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ShapedItemsKey {
    pub visual_items_id: CacheId,
    pub style_hash: u64, // Represents a hash of all font/style properties
}

impl ShapedItemsKey {
    pub fn new(visual_items_id: CacheId, visual_items: &[VisualItem]) -> Self {
        let style_hash = {
            let mut hasher = DefaultHasher::new();
            for item in visual_items.iter() {
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
pub struct LayoutKey {
    pub shaped_items_id: CacheId,
    pub constraints: UnifiedConstraints,
}

/// Helper to create a `CacheId` from any `Hash`able type.
fn calculate_id<T: Hash>(item: &T) -> CacheId {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

// --- Main Layout Pipeline Implementation ---

impl<T: ParsedFontTrait> LayoutCache<T> {
    /// New top-level entry point for flowing layout across multiple regions.
    ///
    /// This function orchestrates the entire layout pipeline, but instead of fitting
    /// content into a single set of constraints, it flows the content through an
    /// ordered sequence of `LayoutFragment`s.
    ///
    /// # Arguments
    /// * `content` - The raw `InlineContent` to be laid out.
    /// * `style_overrides` - Character-level style changes.
    /// * `flow_chain` - An ordered slice of `LayoutFragment` defining the regions (e.g., columns,
    ///   pages) that the content should flow through.
    /// * `font_manager` - The font provider.
    ///
    /// # Returns
    /// A `FlowLayout` struct containing the positioned items for each fragment that
    /// was filled, and any content that did not fit in the final fragment.
    pub fn layout_flow<P: FontProviderTrait<T>>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        flow_chain: &[LayoutFragment],
        font_manager: &P,
    ) -> Result<FlowLayout<T>, LayoutError> {
        // --- Stages 1-3: Preparation ---
        // These stages are independent of the final geometry. We perform them once
        // on the entire content block before flowing. Caching is used at each stage.

        // Stage 1: Logical Analysis (InlineContent -> LogicalItem)
        let logical_items_id = calculate_id(&content);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| Arc::new(create_logical_items(content, style_overrides)))
            .clone();

        // Stage 2: Bidi Reordering (LogicalItem -> VisualItem)
        let base_direction = get_base_direction_from_logical(&logical_items);
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(reorder_logical_items(&logical_items, base_direction).unwrap())
            })
            .clone();

        // Stage 3: Shaping (VisualItem -> ShapedItem)
        let shaped_key = ShapedItemsKey::new(visual_items_id, &visual_items);
        let shaped_items_id = calculate_id(&shaped_key);
        let shaped_items = match self.shaped_items.get(&shaped_items_id) {
            Some(cached) => cached.clone(),
            None => {
                let items = Arc::new(shape_visual_items(&visual_items, font_manager)?);
                self.shaped_items.insert(shaped_items_id, items.clone());
                items
            }
        };

        // --- Stage 4: Apply Vertical Text Transformations ---

        // TODO: This orients all text based on the constraints of the *first* fragment.
        // A more advanced system could defer orientation until inside the loop if
        // fragments can have different writing modes.
        let default_constraints = UnifiedConstraints::default();
        let first_constraints = flow_chain
            .first()
            .map(|f| &f.constraints)
            .unwrap_or(&default_constraints);
        let oriented_items = apply_text_orientation(shaped_items, first_constraints)?;

        // --- Stage 5: The Flow Loop ---

        let mut fragment_layouts = HashMap::new();
        // The cursor now manages the stream of items for the entire flow.
        let mut cursor = BreakCursor::new(&oriented_items);

        for fragment in flow_chain {
            // Perform layout for this single fragment, consuming items from the cursor.
            let fragment_layout =
                perform_fragment_layout(&mut cursor, &logical_items, &fragment.constraints)?;

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
}

// --- Stage 1 Implementation ---
pub fn create_logical_items(
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
) -> Vec<LogicalItem> {
    println!("\n--- [DEBUG] Entering create_logical_items (Refactored) ---");
    println!("Input content length: {}", content.len());
    println!("Input overrides length: {}", style_overrides.len());

    let mut items = Vec::new();
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
        println!("Processing content run #{}", run_idx);
        match inline_item {
            InlineContent::Text(run) => {
                let text = &run.text;
                if text.is_empty() {
                    println!("  Run is empty, skipping.");
                    continue;
                }
                println!("  Run text: '{}'", text);

                let current_run_overrides = run_overrides.get(&(run_idx as u32));
                let mut boundaries = BTreeSet::new();
                boundaries.insert(0);
                boundaries.insert(text.len());

                // --- Stateful Boundary Generation ---
                let mut scan_cursor = 0;
                while scan_cursor < text.len() {
                    let style_at_cursor = if let Some(partial) =
                        current_run_overrides.and_then(|o| o.get(&(scan_cursor as u32)))
                    {
                        // Create a temporary, full style to check its properties
                        run.style.apply_override(partial)
                    } else {
                        (*run.style).clone()
                    };

                    let current_char = text[scan_cursor..].chars().next().unwrap();

                    // Rule 1: Multi-character features take precedence.
                    if let Some(TextCombineUpright::Digits(max_digits)) =
                        style_at_cursor.text_combine_upright
                    {
                        if max_digits > 0 && current_char.is_ascii_digit() {
                            let digit_chunk: String = text[scan_cursor..]
                                .chars()
                                .take(max_digits as usize)
                                .take_while(|c| c.is_ascii_digit())
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

                println!("  Boundaries: {:?}", boundaries);

                // --- Chunk Processing ---
                for (start, end) in boundaries.iter().zip(boundaries.iter().skip(1)) {
                    let (start, end) = (*start, *end);
                    if start >= end {
                        continue;
                    }

                    let text_slice = &text[start..end];
                    println!(
                        "  Processing chunk from {} to {}: '{}'",
                        start, end, text_slice
                    );

                    let style_to_use = if let Some(partial_style) =
                        current_run_overrides.and_then(|o| o.get(&(start as u32)))
                    {
                        println!("  -> Applying override at byte {}", start);
                        let mut hasher = DefaultHasher::new();
                        Arc::as_ptr(&run.style).hash(&mut hasher);
                        partial_style.hash(&mut hasher);
                        style_cache
                            .entry(hasher.finish())
                            .or_insert_with(|| Arc::new(run.style.apply_override(partial_style)))
                            .clone()
                    } else {
                        run.style.clone()
                    };

                    let is_combinable_chunk = if let Some(TextCombineUpright::Digits(max_digits)) =
                        &style_to_use.text_combine_upright
                    {
                        *max_digits > 0
                            && !text_slice.is_empty()
                            && text_slice.chars().all(|c| c.is_ascii_digit())
                            && text_slice.chars().count() <= *max_digits as usize
                    } else {
                        false
                    };

                    if is_combinable_chunk {
                        items.push(LogicalItem::CombinedText {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: text_slice.to_string(),
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
                        });
                    }
                }
            }
            // Other cases...
            _ => {
                println!("  Run is not text, creating generic LogicalItem.");
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
    println!(
        "--- [DEBUG] Exiting create_logical_items, created {} items ---",
        items.len()
    );
    items
}

// --- Stage 2 Implementation ---

pub fn get_base_direction_from_logical(logical_items: &[LogicalItem]) -> Direction {
    let first_strong = logical_items.iter().find_map(|item| {
        if let LogicalItem::Text { text, .. } = item {
            Some(unicode_bidi::get_base_direction(text.as_str()))
        } else {
            None
        }
    });

    match first_strong {
        Some(unicode_bidi::Direction::Rtl) => Direction::Rtl,
        _ => Direction::Ltr,
    }
}

pub fn reorder_logical_items(
    logical_items: &[LogicalItem],
    base_direction: Direction,
) -> Result<Vec<VisualItem>, LayoutError> {
    println!("\n--- [DEBUG] Entering reorder_logical_items ---");
    println!("Input logical items count: {}", logical_items.len());
    println!("Base direction: {:?}", base_direction);

    let mut bidi_str = String::new();
    let mut item_map = Vec::new();
    for (idx, item) in logical_items.iter().enumerate() {
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
        println!("Bidi string is empty, returning.");
        return Ok(Vec::new());
    }
    println!("Constructed bidi string: '{}'", bidi_str);

    let bidi_level = if base_direction == Direction::Rtl {
        Some(Level::rtl())
    } else {
        None
    };
    let bidi_info = BidiInfo::new(&bidi_str, bidi_level);
    let para = &bidi_info.paragraphs[0];
    let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());

    println!("Bidi visual runs generated:");
    for (i, run_range) in visual_runs.iter().enumerate() {
        let level = levels[run_range.start].number();
        let slice = &bidi_str[run_range.start..run_range.end];
        println!(
            "  Run {}: range={:?}, level={}, text='{}'",
            i, run_range, level, slice
        );
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

    println!("Final visual items produced:");
    for (i, item) in visual_items.iter().enumerate() {
        println!(
            "  Item {}: level={}, text='{}'",
            i,
            item.bidi_level.level(),
            item.text
        );
    }
    println!("--- [DEBUG] Exiting reorder_logical_items ---");
    Ok(visual_items)
}

// --- Stage 3 Implementation ---

pub fn shape_visual_items<T: ParsedFontTrait, P: FontProviderTrait<T>>(
    visual_items: &[VisualItem],
    font_provider: &P,
) -> Result<Vec<ShapedItem<T>>, LayoutError> {
    let mut shaped = Vec::new();

    for item in visual_items {
        match &item.logical_source {
            LogicalItem::Text { style, source, .. } => {
                let direction = if item.bidi_level.is_rtl() {
                    Direction::Rtl
                } else {
                    Direction::Ltr
                };

                // Try to load the requested font, fall back to default if not found
                let font = match font_provider.load_font(&style.font_selector) {
                    Ok(f) => f,
                    Err(LayoutError::FontNotFound(_)) => {
                        // Try generic fallbacks
                        let fallback_fonts = ["sans-serif", "serif", "monospace", "system-ui"];
                        let mut loaded_font = None;
                        for fallback in &fallback_fonts {
                            let fallback_selector = FontSelector {
                                family: fallback.to_string(),
                                weight: rust_fontconfig::FcWeight::Normal,
                                style: FontStyle::Normal,
                                unicode_ranges: vec![],
                            };
                            if let Ok(f) = font_provider.load_font(&fallback_selector) {
                                eprintln!(
                                    "[TextLayout] Using fallback font '{}' for '{}'",
                                    fallback, style.font_selector.family
                                );
                                loaded_font = Some(f);
                                break;
                            }
                        }

                        // If no fallback available, skip this text item with warning
                        if loaded_font.is_none() {
                            eprintln!(
                                "[TextLayout] No font available for '{}', skipping text",
                                style.font_selector.family
                            );
                            continue;
                        }
                        loaded_font.unwrap()
                    }
                    Err(e) => return Err(e),
                };
                let language = script_to_language(item.script, &item.text);

                let shaped_clusters = shape_text_correctly(
                    &item.text,
                    item.script,
                    language,
                    direction,
                    &font,
                    style,
                    *source,
                )?;
                shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));
            }
            LogicalItem::Tab { source, style } => {
                // TODO: To get the space width accurately, we would need to shape
                // a space character with the current font.
                // For now, we approximate it as a fraction of the font size.
                let space_advance = style.font_size_px * 0.33;
                let tab_width = style.tab_size * space_advance;
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
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                // TODO: Implement Ruby layout. This is a major feature.
                // 1. Recursively call layout for the `base_text` to get its size.
                // 2. Recursively call layout for the `ruby_text` (with a smaller font from
                //    `style`).
                // 3. Position the ruby text bounds above/beside the base text bounds.
                // 4. Create a single `ShapedItem::Object` or `ShapedItem::CombinedBlock` that
                //    represents the combined metric bounds of the group, which will be used for
                //    line breaking and positioning on the main line.
                // For now, create a placeholder object.
                let placeholder_width = base_text.chars().count() as f32 * style.font_size_px * 0.6;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: placeholder_width,
                        height: style.line_height * 1.5,
                    },
                    baseline_offset: 0.0,
                    content: InlineContent::Text(StyledRun {
                        text: base_text.clone(),
                        style: style.clone(),
                        logical_start_byte: 0,
                    }),
                });
            }
            LogicalItem::CombinedText {
                style,
                source,
                text,
            } => {
                let font: Arc<T> = font_provider.load_font(&style.font_selector)?;
                let language = script_to_language(item.script, &item.text);

                // Force LTR horizontal shaping for the combined block.
                let glyphs =
                    font.shape_text(text, item.script, language, Direction::Ltr, style.as_ref())?;

                let shaped_glyphs = glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        kind: GlyphKind::Character,
                        glyph_id: g.glyph_id,
                        script: g.script,
                        font: g.font,
                        style: g.style,
                        cluster_offset: 0,
                        advance: g.advance,
                        offset: g.offset,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                    })
                    .collect::<Vec<_>>();

                let total_width: f32 = shaped_glyphs.iter().map(|g| g.advance).sum();
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: style.line_height,
                };

                shaped.push(ShapedItem::CombinedBlock {
                    source: *source,
                    glyphs: shaped_glyphs,
                    bounds,
                    baseline_offset: 0.0,
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
                    break_info: break_info.clone(),
                });
            }
        }
    }
    Ok(shaped)
}

/// Helper to check if a cluster contains only hanging punctuation.
fn is_hanging_punctuation<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    if let ShapedItem::Cluster(c) = item {
        if c.glyphs.len() == 1 {
            match c.text.as_str() {
                "." | "," | ":" | ";" => true,
                _ => false,
            }
        } else {
            false
        }
    } else {
        false
    }
}

fn shape_text_correctly<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: hyphenation::Language,
    direction: Direction,
    font: &Arc<T>,
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
) -> Result<Vec<ShapedCluster<T>>, LayoutError> {
    let glyphs = font.shape_text(text, script, language, direction, style.as_ref())?;

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
                .map(|g: &Glyph<T>| g.advance)
                .sum();
            let cluster_text = &text[cluster_start_byte_in_text..glyph.logical_byte_index];

            clusters.push(ShapedCluster {
                text: cluster_text.to_string(), // Store original text for hyphenation
                source_cluster_id: GraphemeClusterId {
                    source_run: source_index.run_index,
                    start_byte_in_run: cluster_id,
                },
                source_content_index: source_index,
                glyphs: current_cluster_glyphs
                    .iter()
                    .map(|g| {
                        let source_char = text[g.logical_byte_index..]
                            .chars()
                            .next()
                            .unwrap_or('\u{FFFD}');
                        ShapedGlyph {
                            kind: if g.glyph_id == 0 {
                                GlyphKind::NotDef
                            } else {
                                GlyphKind::Character
                            },
                            glyph_id: g.glyph_id,
                            script: g.script,
                            font: g.font.clone(),
                            style: g.style.clone(),
                            cluster_offset: (g.logical_byte_index - cluster_start_byte_in_text)
                                as u32,
                            advance: g.advance,
                            vertical_advance: g.vertical_advance,
                            vertical_offset: g.vertical_bearing,
                            offset: g.offset,
                        }
                    })
                    .collect(),
                advance,
                direction,
                style: style.clone(),
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
            .map(|g: &Glyph<T>| g.advance)
            .sum();
        let cluster_text = &text[cluster_start_byte_in_text..];
        clusters.push(ShapedCluster {
            text: cluster_text.to_string(), // Store original text
            source_cluster_id: GraphemeClusterId {
                source_run: source_index.run_index,
                start_byte_in_run: cluster_id,
            },
            source_content_index: source_index,
            glyphs: current_cluster_glyphs
                .iter()
                .map(|g| {
                    let source_char = text[g.logical_byte_index..]
                        .chars()
                        .next()
                        .unwrap_or('\u{FFFD}');
                    ShapedGlyph {
                        kind: if g.glyph_id == 0 {
                            GlyphKind::NotDef
                        } else {
                            GlyphKind::Character
                        },
                        glyph_id: g.glyph_id,
                        font: g.font.clone(),
                        style: g.style.clone(),
                        script: g.script,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                        cluster_offset: (g.logical_byte_index - cluster_start_byte_in_text) as u32,
                        advance: g.advance,
                        offset: g.offset,
                    }
                })
                .collect(),
            advance,
            direction,
            style: style.clone(),
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
        _ => Err(LayoutError::InvalidText("Not a measurable object".into())),
    }
}

// --- Stage 4 Implementation: Vertical Text ---

/// Applies orientation and vertical metrics to glyphs if the writing mode is vertical.
fn apply_text_orientation<T: ParsedFontTrait>(
    items: Arc<Vec<ShapedItem<T>>>,
    constraints: &UnifiedConstraints,
) -> Result<Arc<Vec<ShapedItem<T>>>, LayoutError> {
    if !constraints.is_vertical() {
        return Ok(items);
    }

    let mut oriented_items = Vec::with_capacity(items.len());
    let writing_mode = constraints.writing_mode.unwrap_or_default();

    for item in items.iter() {
        match item {
            ShapedItem::Cluster(cluster) => {
                let mut new_cluster = cluster.clone();
                let mut total_vertical_advance = 0.0;

                for glyph in &mut new_cluster.glyphs {
                    if let Some(v_metrics) = glyph.font.get_vertical_metrics(glyph.glyph_id) {
                        glyph.vertical_advance = v_metrics.advance;
                        glyph.vertical_offset = Point {
                            x: v_metrics.bearing_x,
                            y: v_metrics.bearing_y,
                        };
                        total_vertical_advance += v_metrics.advance;
                    } else {
                        // Fallback: use line height for vertical advance.
                        let fallback_advance = cluster.style.line_height;
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

    Ok(Arc::new(oriented_items))
}

// --- Stage 5 & 6 Implementation: Combined Layout Pass ---
// This section replaces the previous simple line breaking and positioning logic.

/// Gets the ascent (distance from baseline to top) and descent (distance from baseline to bottom)
/// for a single item.
pub fn get_item_vertical_metrics<T: ParsedFontTrait>(item: &ShapedItem<T>) -> (f32, f32) {
    // (ascent, descent)
    match item {
        ShapedItem::Cluster(c) => {
            if c.glyphs.is_empty() {
                // For an empty text cluster, use the line height from its style as a fallback.
                return (c.style.line_height, 0.0);
            }
            // CORRECTED: Iterate through ALL glyphs in the cluster to find the true max
            // ascent/descent.
            c.glyphs
                .iter()
                .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                    let metrics = glyph.font.get_font_metrics();
                    if metrics.units_per_em == 0 {
                        return (max_asc, max_desc);
                    }
                    let scale = glyph.style.font_size_px / metrics.units_per_em as f32;
                    let item_asc = metrics.ascent * scale;
                    // Descent in OpenType is typically negative, so we negate it to get a positive
                    // distance.
                    let item_desc = (-metrics.descent * scale).max(0.0);
                    (max_asc.max(item_asc), max_desc.max(item_desc))
                })
        }
        ShapedItem::Object {
            bounds,
            baseline_offset,
            ..
        } => {
            // Per analysis, `baseline_offset` is the distance from the bottom.
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

/// Calculates the maximum ascent and descent for an entire line of items.
/// This determines the "line box" used for vertical alignment.
fn calculate_line_metrics<T: ParsedFontTrait>(items: &[ShapedItem<T>]) -> (f32, f32) {
    // (max_ascent, max_descent)
    items
        .iter()
        .fold((0.0f32, 0.0f32), |(max_asc, max_desc), item| {
            let (item_asc, item_desc) = get_item_vertical_metrics(item);
            (max_asc.max(item_asc), max_desc.max(item_desc))
        })
}

/// Performs layout for a single fragment, consuming items from a `BreakCursor`.
///
/// This function contains the core line-breaking and positioning logic, but is
/// designed to operate on a portion of a larger content stream and within the
/// constraints of a single geometric area (a fragment).
///
/// The loop terminates when either the fragment is filled (e.g., runs out of
/// vertical space) or the content stream managed by the `cursor` is exhausted.
pub fn perform_fragment_layout<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<T>,
    logical_items: &[LogicalItem],
    fragment_constraints: &UnifiedConstraints,
) -> Result<UnifiedLayout<T>, LayoutError> {
    println!("\n--- [DEBUG] Entering perform_fragment_layout ---");
    println!(
        "Constraints: available_width={}, available_height={:?}, columns={}",
        fragment_constraints.available_width,
        fragment_constraints.available_height,
        fragment_constraints.columns
    );

    let hyphenator = if fragment_constraints.hyphenation {
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
    let column_width =
        (fragment_constraints.available_width - total_column_gap) / num_columns as f32;
    let mut current_column = 0;
    println!("Column width calculated: {}", column_width);

    let base_direction = get_base_direction_from_logical(logical_items);
    // REMOVED: No longer pre-resolving alignment.
    // let physical_align = resolve_logical_align(fragment_constraints.text_align, base_direction);
    println!(
        "[PFLayout] Base direction: {:?}, Text align: {:?}",
        base_direction, fragment_constraints.text_align
    );

    'column_loop: while current_column < num_columns {
        println!("\n-- Starting Column {} --", current_column);
        let column_start_x =
            (column_width + fragment_constraints.column_gap) * current_column as f32;
        let mut line_top_y = 0.0;
        let mut line_index = 0;

        while !cursor.is_done() {
            if let Some(max_height) = fragment_constraints.available_height {
                if line_top_y >= max_height {
                    println!(
                        "  Column full (pen {} >= height {}), breaking to next column.",
                        line_top_y, max_height
                    );
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
            column_constraints.available_width = column_width;
            let line_constraints = get_line_constraints(
                line_top_y,
                fragment_constraints.line_height,
                &column_constraints,
            );

            if line_constraints.segments.is_empty() {
                println!(
                    "  No available segments at y={}, skipping to next line.",
                    line_top_y
                );
                line_top_y += fragment_constraints.line_height;
                continue;
            }

            let (mut line_items, was_hyphenated) =
                break_one_line(cursor, &line_constraints, false, hyphenator.as_ref());
            if line_items.is_empty() {
                println!("  Break returned no items. Ending column.");
                break;
            }

            let line_text_before_rev: String = line_items
                .iter()
                .filter_map(|i| i.as_cluster())
                .map(|c| c.text.as_str())
                .collect();
            println!(
                // FIX: The log message was misleading. Items are in visual order.
                "[PFLayout] Line items from breaker (visual order): [{}]",
                line_text_before_rev
            );

            let (mut line_pos_items, line_height) = position_one_line(
                line_items,
                &line_constraints,
                line_top_y,
                line_index,
                fragment_constraints.text_align,
                base_direction,
                cursor.is_done() && !was_hyphenated,
                fragment_constraints,
            );

            for item in &mut line_pos_items {
                item.position.x += column_start_x;
            }

            line_top_y += line_height.max(fragment_constraints.line_height);
            line_index += 1;
            positioned_items.extend(line_pos_items);
        }
        current_column += 1;
    }

    println!(
        "--- [DEBUG] Exiting perform_fragment_layout, positioned {} items ---",
        positioned_items.len()
    );
    let mut layout = UnifiedLayout {
        items: positioned_items,
        bounds: layout_bounds,
        overflow: OverflowInfo::default(),
        used_fonts: std::collections::BTreeMap::new(),
    };

    // Collect all fonts used in this layout
    layout.collect_used_fonts();

    Ok(layout)
}

/// Breaks a single line of items to fit within the given geometric constraints,
/// handling multi-segment lines and hyphenation.
pub fn break_one_line<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<T>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&Standard>,
) -> (Vec<ShapedItem<T>>, bool) {
    let mut line_items = Vec::new();
    let mut current_width = 0.0;

    if cursor.is_done() {
        return (Vec::new(), false);
    }

    loop {
        // 1. Identify the next unbreakable unit (word) or break opportunity.
        let next_unit = cursor.peek_next_unit();
        if next_unit.is_empty() {
            break; // End of content
        }

        // Handle hard breaks immediately.
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
            if let Some(hyphenator) = hyphenator {
                // We only try to hyphenate if the unit is a word (not a space).
                if !is_break_opportunity(next_unit.last().unwrap()) {
                    if let Some(hyphenation_result) = try_hyphenate_word_cluster(
                        &next_unit,
                        available_width,
                        is_vertical,
                        hyphenator,
                    ) {
                        line_items.extend(hyphenation_result.line_part);
                        // Consume the original full word from the cursor.
                        cursor.consume(next_unit.len());
                        // Put the remainder back for the next line.
                        cursor.partial_remainder = hyphenation_result.remainder_part;
                        return (line_items, true);
                    }
                }
            }

            // 4. Cannot hyphenate or fit. The line is finished.
            // If the line is empty, we must force at least one item to avoid an infinite loop.
            if line_items.is_empty() {
                line_items.push(next_unit[0].clone());
                cursor.consume(1);
            }
            break;
        }
    }

    (line_items, false)
}

/// Represents a single valid hyphenation point within a word.
#[derive(Clone)]
pub struct HyphenationBreak<T: ParsedFontTrait> {
    /// The number of characters from the original word string included on the line.
    pub char_len_on_line: usize,
    /// The total advance width of the line part + the hyphen.
    pub width_on_line: f32,
    /// The cluster(s) that will remain on the current line.
    pub line_part: Vec<ShapedItem<T>>,
    /// The cluster that represents the hyphen character itself.
    pub hyphen_item: ShapedItem<T>,
    /// The cluster(s) that will be carried over to the next line.
    /// CRITICAL FIX: Changed from ShapedItem<T> to Vec<ShapedItem<T>>
    pub remainder_part: Vec<ShapedItem<T>>,
}

/// A "word" is defined as a sequence of one or more adjacent ShapedClusters.
pub fn find_all_hyphenation_breaks<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster<T>],
    hyphenator: &Standard,
    is_vertical: bool, // Pass this in to use correct metrics
) -> Option<Vec<HyphenationBreak<T>>> {
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

    // --- 2. Get hyphenation opportunities ---
    let opportunities = hyphenator.hyphenate(&word_string);
    if opportunities.breaks.is_empty() {
        return None;
    }

    let last_cluster = word_clusters.last().unwrap();
    let last_glyph = last_cluster.glyphs.last().unwrap();
    let (font, style) = (last_glyph.font.clone(), last_cluster.style.clone());
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
        let line_part: Vec<ShapedItem<T>> = word_clusters[..break_char_idx]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        // The remainder is all clusters *from* the break index onward.
        let remainder_part: Vec<ShapedItem<T>> = word_clusters[break_char_idx..]
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
            glyphs: vec![ShapedGlyph {
                kind: GlyphKind::Hyphen,
                glyph_id: hyphen_glyph_id,
                font: font.clone(),
                cluster_offset: 0,
                script: Script::Latin,
                advance: hyphen_advance,
                offset: Point::default(),
                style: style.clone(),
                vertical_advance: hyphen_advance,
                vertical_offset: Point::default(),
            }],
            advance: hyphen_advance,
            direction: Direction::Ltr,
            style: style.clone(),
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
    word_items: &[ShapedItem<T>],
    remaining_width: f32,
    is_vertical: bool,
    hyphenator: &Standard,
) -> Option<HyphenationResult<T>> {
    let word_clusters: Vec<ShapedCluster<T>> = word_items
        .iter()
        .filter_map(|item| item.as_cluster().cloned())
        .collect();

    if word_clusters.is_empty() {
        return None;
    }

    let all_breaks = find_all_hyphenation_breaks(&word_clusters, hyphenator, is_vertical)?;

    if let Some(best_break) = all_breaks
        .into_iter()
        .rfind(|b| b.width_on_line <= remaining_width)
    {
        let mut line_part = best_break.line_part;
        line_part.push(best_break.hyphen_item);

        return Some(HyphenationResult {
            line_part,
            // The remainder is now a Vec, passed through directly.
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
pub fn position_one_line<T: ParsedFontTrait>(
    line_items: Vec<ShapedItem<T>>,
    line_constraints: &LineConstraints,
    line_top_y: f32,
    line_index: usize,
    text_align: TextAlign,
    base_direction: Direction,
    is_last_line: bool,
    constraints: &UnifiedConstraints,
) -> (Vec<PositionedItem<T>>, f32) {
    let line_text: String = line_items
        .iter()
        .filter_map(|i| i.as_cluster())
        .map(|c| c.text.as_str())
        .collect();
    println!(
        "\n--- [DEBUG] Entering position_one_line for line: [{}] ---",
        line_text
    );
    // NEW: Resolve the final physical alignment here, inside the function.
    let physical_align = match (text_align, base_direction) {
        (TextAlign::Start, Direction::Ltr) => TextAlign::Left,
        (TextAlign::Start, Direction::Rtl) => TextAlign::Right,
        (TextAlign::End, Direction::Ltr) => TextAlign::Right,
        (TextAlign::End, Direction::Rtl) => TextAlign::Left,
        // Physical alignments are returned as-is, regardless of direction.
        (other, _) => other,
    };
    println!("[Pos1Line] Physical align: {:?}", physical_align);

    if line_items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut positioned = Vec::new();
    let is_vertical = constraints.is_vertical();

    // The line box is calculated once for all items on the line, regardless of segment.
    let (line_ascent, line_descent) = calculate_line_metrics(&line_items);
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

        // 2. Calculate justification spacing *for this segment only*.
        let (extra_word_spacing, extra_char_spacing) = if constraints.text_justify
            != JustifyContent::None
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::Kashida
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![segment.clone()],
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
                segments: vec![segment.clone()],
                total_available: segment.width,
            };
            justify_kashida_and_rebuild(segment_items, &segment_line_constraints, is_vertical)
        } else {
            segment_items
        };

        // Recalculate width in case kashida changed the item list
        let final_segment_width: f32 = justified_segment_items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        // 3. Calculate alignment offset *within this segment*.
        let remaining_space = segment.width - final_segment_width;
        let mut main_axis_pen = segment.start_x
            + match physical_align {
                TextAlign::Center => remaining_space / 2.0,
                TextAlign::Right => remaining_space,
                _ => 0.0, // Left, Justify
            };
        println!(
            "[Pos1Line] Segment width: {}, Item width: {}, Remaining space: {}, Initial pen: {}",
            segment.width, final_segment_width, remaining_space, main_axis_pen
        );

        // Apply text-indent only to the very first segment of the first line.
        if is_first_line_of_para && segment_idx == 0 {
            main_axis_pen += constraints.text_indent;
        }

        // 4. Position the items belonging to this segment.
        for item in justified_segment_items {
            let (item_ascent, item_descent) = get_item_vertical_metrics(&item);
            let item_baseline_pos = match constraints.vertical_align {
                VerticalAlign::Top => line_top_y + item_ascent,
                VerticalAlign::Middle => {
                    line_top_y + (line_box_height / 2.0) - ((item_ascent + item_descent) / 2.0)
                        + item_ascent
                }
                VerticalAlign::Bottom => line_top_y + line_box_height - item_descent,
                _ => line_baseline_y, // Baseline
            };

            let position = if is_vertical {
                Point {
                    x: item_baseline_pos - item_ascent,
                    y: main_axis_pen,
                }
            } else {
                Point {
                    y: item_baseline_pos - item_ascent,
                    x: main_axis_pen,
                }
            };

            let item_measure = get_item_measure(&item, is_vertical);
            let item_text = item
                .as_cluster()
                .map(|c| c.text.as_str())
                .unwrap_or("[OBJ]");
            println!(
                "[Pos1Line] Positioning item '{}' at pen_x={}",
                item_text, main_axis_pen
            );
            positioned.push(PositionedItem {
                item: item.clone(),
                position,
                line_index,
            });
            main_axis_pen += item_measure;

            // Apply calculated spacing to the pen
            if extra_char_spacing > 0.0 && can_justify_after(&item) {
                main_axis_pen += extra_char_spacing;
            }
            if let ShapedItem::Cluster(c) = &item {
                let letter_spacing_px = match c.style.letter_spacing {
                    Spacing::Px(px) => px as f32,
                    Spacing::Em(em) => em * c.style.font_size_px,
                };
                main_axis_pen += letter_spacing_px;
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

    (positioned, line_box_height)
}

/// Calculates the starting pen offset to achieve the desired text alignment.
fn calculate_alignment_offset<T: ParsedFontTrait>(
    items: &[ShapedItem<T>],
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
fn calculate_justification_spacing<T: ParsedFontTrait>(
    items: &[ShapedItem<T>],
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
pub fn justify_kashida_and_rebuild<T: ParsedFontTrait>(
    items: Vec<ShapedItem<T>>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
) -> Vec<ShapedItem<T>> {
    println!("\n--- [DEBUG] Entering justify_kashida_and_rebuild ---");
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;
    println!(
        "Total item width: {}, Available width: {}",
        total_width, available_width
    );

    if total_width >= available_width || available_width <= 0.0 {
        println!("No justification needed (line is full or invalid).");
        return items;
    }

    let extra_space = available_width - total_width;
    println!("Extra space to fill: {}", extra_space);

    let font_info = items.iter().find_map(|item| {
        if let ShapedItem::Cluster(c) = item {
            if let Some(glyph) = c.glyphs.first() {
                if glyph.script == Script::Arabic {
                    return Some((glyph.font.clone(), glyph.style.clone()));
                }
            }
        }
        None
    });

    let (font, style) = match font_info {
        Some(info) => {
            println!("Found Arabic font for kashida.");
            info
        }
        None => {
            println!("No Arabic font found on line. Cannot insert kashidas.");
            return items;
        }
    };

    let (kashida_glyph_id, kashida_advance) =
        match font.get_kashida_glyph_and_advance(style.font_size_px) {
            Some((id, adv)) if adv > 0.0 => {
                println!("Font provides kashida glyph with advance {}", adv);
                (id, adv)
            }
            _ => {
                println!("Font does not support kashida justification.");
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

    println!(
        "Found {} kashida insertion opportunities at indices: {:?}",
        opportunity_indices.len(),
        opportunity_indices
    );

    if opportunity_indices.is_empty() {
        println!("No opportunities found. Exiting.");
        return items;
    }

    let num_kashidas_to_insert = (extra_space / kashida_advance).floor() as usize;
    println!(
        "Calculated number of kashidas to insert: {}",
        num_kashidas_to_insert
    );

    if num_kashidas_to_insert == 0 {
        return items;
    }

    let kashidas_per_point = num_kashidas_to_insert / opportunity_indices.len();
    let mut remainder = num_kashidas_to_insert % opportunity_indices.len();
    println!(
        "Distributing kashidas: {} per point, with {} remainder.",
        kashidas_per_point, remainder
    );

    let kashida_item = {
        /* ... as before ... */
        let kashida_glyph = ShapedGlyph {
            kind: GlyphKind::Kashida {
                width: kashida_advance,
            },
            glyph_id: kashida_glyph_id,
            font,
            style: style.clone(),
            script: Script::Arabic,
            advance: kashida_advance,
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
            glyphs: vec![kashida_glyph],
            advance: kashida_advance,
            direction: Direction::Ltr,
            style,
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

    println!(
        "--- [DEBUG] Exiting justify_kashida_and_rebuild, new item count: {} ---",
        new_items.len()
    );
    new_items
}

/// Helper to determine if a cluster belongs to the Arabic script.
fn is_arabic_cluster<T: ParsedFontTrait>(cluster: &ShapedCluster<T>) -> bool {
    // A cluster is considered Arabic if its first non-NotDef glyph is from the Arabic script.
    // This is a robust heuristic for mixed-script lines.
    cluster.glyphs.iter().any(|g| g.script == Script::Arabic)
}

/// Helper to identify if an item is a word separator (like a space).
pub fn is_word_separator<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    if let ShapedItem::Cluster(c) = item {
        // A cluster is a word separator if its text is whitespace.
        // This is a simplification; a single glyph might be whitespace.
        c.text.chars().any(|g| g.is_whitespace())
    } else {
        false
    }
}

/// Helper to identify if space can be added after an item.
fn can_justify_after<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().last().map_or(false, |g| {
            !g.is_whitespace() && classify_character(g as u32) != CharacterClass::Combining
        })
    } else {
        // Can generally justify after inline objects unless they are followed by a break.
        !matches!(item, ShapedItem::Break { .. })
    }
}

/// Classifies a character for layout purposes (e.g., justification behavior).
/// Copied from `mod.rs`.
fn classify_character(codepoint: u32) -> CharacterClass {
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
pub fn get_item_measure<T: ParsedFontTrait>(item: &ShapedItem<T>, is_vertical: bool) -> f32 {
    match item {
        ShapedItem::Cluster(c) => c.advance,
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

/// Helper to get the final positioned bounds of an item.
fn get_item_bounds<T: ParsedFontTrait>(item: &PositionedItem<T>) -> Rect {
    let measure = get_item_measure(&item.item, false); // for simplicity, use horizontal
    let cross_measure = match &item.item {
        ShapedItem::Object { bounds, .. } => bounds.height,
        _ => 20.0, // placeholder line height
    };
    Rect {
        x: item.position.x,
        y: item.position.y,
        width: measure,
        height: cross_measure,
    }
}

/// Calculates the available horizontal segments for a line at a given vertical position,
/// considering both shape boundaries and exclusions.
fn get_line_constraints(
    line_y: f32,
    line_height: f32,
    constraints: &UnifiedConstraints,
) -> LineConstraints {
    println!(
        "\n--- [DEBUG] Entering get_line_constraints for y={} ---",
        line_y
    );

    let mut available_segments = Vec::new();
    if constraints.shape_boundaries.is_empty() {
        available_segments.push(LineSegment {
            start_x: 0.0,
            width: constraints.available_width,
            priority: 0,
        });
    } else {
        // ... complex boundary logic ...
    }

    println!("Initial available segments: {:?}", available_segments);

    for (idx, exclusion) in constraints.shape_exclusions.iter().enumerate() {
        println!("Applying exclusion #{}: {:?}", idx, exclusion);
        let exclusion_spans =
            get_shape_horizontal_spans(exclusion, line_y, line_height).unwrap_or_default();
        println!("  Exclusion spans at y={}: {:?}", line_y, exclusion_spans);

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
                    next_segments.push(segment.clone()); // No overlap
                }
            }
            available_segments = merge_segments(next_segments);
            next_segments = Vec::new();
        }
        println!(
            "  Segments after exclusion #{}: {:?}",
            idx, available_segments
        );
    }

    let total_width = available_segments.iter().map(|s| s.width).sum();
    println!(
        "Final segments: {:?}, total available width: {}",
        available_segments, total_width
    );
    println!("--- [DEBUG] Exiting get_line_constraints ---");

    LineConstraints {
        segments: available_segments,
        total_available: total_width,
    }
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (start_x, end_x) tuples.
fn get_shape_horizontal_spans(
    shape: &ShapeBoundary,
    y: f32,
    line_height: f32,
) -> Result<Vec<(f32, f32)>, LayoutError> {
    match shape {
        ShapeBoundary::Rectangle(rect) => {
            // Check for any overlap between the line box [y, y + line_height]
            // and the rectangle's vertical span [rect.y, rect.y + rect.height].
            let line_start = y;
            let line_end = y + line_height;
            let rect_start = rect.y;
            let rect_end = rect.y + rect.height;

            if line_start < rect_end && line_end > rect_start {
                Ok(vec![(rect.x, rect.x + rect.width)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Circle { center, radius } => {
            let line_center_y = y + line_height / 2.0;
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                Ok(vec![(center.x - dx, center.x + dx)])
            } else {
                Ok(vec![])
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
                    Ok(vec![(center.x - dx, center.x + dx)])
                } else {
                    Ok(vec![])
                }
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Polygon { points } => {
            let segments = polygon_line_intersection(points, y, line_height)?;
            Ok(segments
                .iter()
                .map(|s| (s.start_x, s.start_x + s.width))
                .collect())
        }
        ShapeBoundary::Path { .. } => Ok(vec![]), // TODO!
    }
}

/// Merges overlapping or adjacent line segments into larger ones.
fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());
    let mut merged = vec![segments[0].clone()];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(next_seg.clone());
        }
    }
    merged
}

// TODO: Dummy polygon function to make it compile
fn polygon_line_intersection(
    points: &[Point],
    y: f32,
    line_height: f32,
) -> Result<Vec<LineSegment>, LayoutError> {
    if points.len() < 3 {
        return Ok(vec![]);
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
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

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

    Ok(segments)
}

// ADDITION: A helper function to get a hyphenator.
/// Helper to get a hyphenator for a given language.
/// TODO: In a real app, this would be cached.
fn get_hyphenator(language: Language) -> Result<Standard, LayoutError> {
    Standard::from_embedded(language).map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

fn is_break_opportunity<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    // Break after spaces or explicit break items.
    if is_word_separator(item) {
        return true;
    }
    if let ShapedItem::Break { .. } = item {
        return true;
    }
    // Also consider soft hyphens as opportunities.
    if let ShapedItem::Cluster(c) = item {
        if c.text.starts_with('\u{00AD}') {
            return true;
        }
    }
    false
}

// A cursor to manage the state of the line breaking process.
// This allows us to handle items that are partially consumed by hyphenation.
pub struct BreakCursor<'a, T: ParsedFontTrait> {
    /// A reference to the complete list of shaped items.
    pub items: &'a [ShapedItem<T>],
    /// The index of the next *full* item to be processed from the `items` slice.
    pub next_item_index: usize,
    /// The remainder of an item that was split by hyphenation on the previous line.
    /// This will be the very first piece of content considered for the next line.
    pub partial_remainder: Vec<ShapedItem<T>>,
}

impl<'a, T: ParsedFontTrait> BreakCursor<'a, T> {
    pub fn new(items: &'a [ShapedItem<T>]) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
        }
    }

    /// Checks if the cursor is at the very beginning of the content stream.
    pub fn is_at_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_empty()
    }

    /// Consumes the cursor and returns all remaining items as a `Vec`.
    pub fn drain_remaining(&mut self) -> Vec<ShapedItem<T>> {
        let mut remaining = std::mem::take(&mut self.partial_remainder);
        if self.next_item_index < self.items.len() {
            remaining.extend_from_slice(&self.items[self.next_item_index..]);
        }
        self.next_item_index = self.items.len();
        remaining
    }

    /// Checks if all content, including any partial remainders, has been processed.
    pub fn is_done(&self) -> bool {
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
    pub fn peek_next_unit(&self) -> Vec<ShapedItem<T>> {
        let mut unit = Vec::new();
        let mut source_items = self.partial_remainder.clone();
        source_items.extend_from_slice(&self.items[self.next_item_index..]);

        if source_items.is_empty() {
            return unit;
        }

        // If the first item is a break opportunity (like a space), it's a unit on its own.
        if is_break_opportunity(&source_items[0]) {
            unit.push(source_items[0].clone());
            return unit;
        }

        // Otherwise, collect all items until the next break opportunity.
        for item in source_items {
            if is_break_opportunity(&item) {
                break;
            }
            unit.push(item.clone());
        }
        unit
    }
}

// A structured result from a hyphenation attempt.
struct HyphenationResult<T: ParsedFontTrait> {
    /// The items that fit on the current line, including the new hyphen.
    line_part: Vec<ShapedItem<T>>,
    /// The remainder of the split item to be carried over to the next line.
    remainder_part: Vec<ShapedItem<T>>,
}

fn perform_bidi_analysis<'a, 'b: 'a>(
    styled_runs: &'a [TextRunInfo],
    full_text: &'b str,
    force_lang: Option<Language>,
) -> Result<(Vec<VisualRun<'a>>, Direction), LayoutError> {
    if full_text.is_empty() {
        return Ok((Vec::new(), Direction::Ltr));
    }

    let bidi_info = BidiInfo::new(full_text, None);
    let para = &bidi_info.paragraphs[0];
    let base_direction = if para.level.is_rtl() {
        Direction::Rtl
    } else {
        Direction::Ltr
    };

    // Create a map from each byte index to its original styled run.
    let mut byte_to_run_index: Vec<usize> = vec![0; full_text.len()];
    for (run_idx, run) in styled_runs.iter().enumerate() {
        let start = run.logical_start;
        let end = start + run.text.len();
        for i in start..end {
            byte_to_run_index[i] = run_idx;
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
                        crate::text3::script::script_to_language(
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
                crate::text3::script::script_to_language(
                    script,
                    &full_text[sub_run_start..range.end],
                )
            }),
        });
    }

    Ok((final_visual_runs, base_direction))
}

fn shape_visual_runs<Q: ParsedFontTrait, T: FontProviderTrait<Q>>(
    visual_runs: &[VisualRun],
    font_provider: &T,
) -> Result<Vec<Glyph<Q>>, LayoutError> {
    let mut all_shaped_glyphs = Vec::new();

    for run in visual_runs {
        let font = font_provider.load_font(&run.style.font_selector)?;

        let direction = if run.bidi_level.is_rtl() {
            Direction::Rtl
        } else {
            Direction::Ltr
        };

        let mut shaped_output = font.shape_text(
            run.text_slice,
            run.script,
            run.language,
            direction,
            &run.style,
        )?;

        if direction == Direction::Rtl {
            shaped_output.reverse();
        }

        for shaped_in_run in shaped_output {
            let source_char = run.text_slice[shaped_in_run.logical_byte_index..]
                .chars()
                .next()
                .unwrap_or('\0');

            let is_whitespace = source_char.is_whitespace();
            let is_soft_hyphen = source_char == '\u{00AD}';

            // Determine character class for justification
            let character_class = classify_character(source_char as u32);
            let can_justify = !is_whitespace && character_class != CharacterClass::Combining;
            let justification_priority = get_justification_priority(character_class);

            all_shaped_glyphs.push(Glyph {
                glyph_id: shaped_in_run.glyph_id,
                codepoint: source_char,
                style: run.style.clone(),
                font: font.clone(),
                advance: shaped_in_run.advance,
                source: GlyphSource::Char,
                script: run.script,
                bidi_level: run.bidi_level,
                offset: shaped_in_run.offset,
                cluster: shaped_in_run.cluster + run.logical_start_byte as u32,
                content_index: 0, // Would be set from content analysis

                // Add missing vertical metrics - will be set later, if vertical
                vertical_advance: 0.0,
                vertical_origin_y: 0.0,
                vertical_bearing: Point { x: 0.0, y: 0.0 },

                // Complete byte mappings
                logical_byte_index: shaped_in_run.logical_byte_index + run.logical_start_byte,
                logical_byte_len: shaped_in_run.logical_byte_len,

                // Add justification fields
                orientation: GlyphOrientation::Horizontal, // Will be set based on script
            });
        }
    }

    Ok(all_shaped_glyphs)
}

fn get_justification_priority(class: CharacterClass) -> u8 {
    match class {
        CharacterClass::Space => 0,
        CharacterClass::Punctuation => 64,
        CharacterClass::Ideograph => 128,
        CharacterClass::Letter => 192,
        CharacterClass::Symbol => 224,
        CharacterClass::Combining => 255,
    }
}
