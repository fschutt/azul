use std::{
    collections::{BTreeSet, HashMap},
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use hyphenation::{Hyphenator as _, Language, Load as _, Standard};
use lru::LruCache;
use rust_fontconfig::{
    FcFontCache, FcPattern, FcWeight, FontId, FontMatch, PatternMatch, UnicodeRange,
};
use unicode_bidi::{get_base_direction, BidiInfo};
use unicode_segmentation::UnicodeSegmentation;

use crate::text3::script::Script;

pub mod default;
pub mod script;

pub trait ParsedFontTrait: Send + Clone {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: Direction,
    ) -> Result<Vec<Glyph<Self>>, LayoutError>;

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> (u16, f32);
    fn has_glyph(&self, codepoint: u32) -> bool;
    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics>;
    fn get_font_metrics(&self) -> FontMetrics;
}

pub trait FontLoaderTrait<T: ParsedFontTrait>: Send + core::fmt::Debug {
    fn load_font(&self, font_bytes: &[u8], font_index: usize) -> Result<Arc<T>, LayoutError>;
}

// Font loading and management
pub trait FontProviderTrait<T: ParsedFontTrait> {
    fn load_font(&self, font_ref: &FontRef) -> Result<Arc<T>, LayoutError>;
}

#[derive(Debug, Clone)]
pub struct ExclusionRect {
    pub rect: Rect,
    pub side: ExclusionSide,
}

#[derive(Debug, Clone)]
pub enum ExclusionSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Font and styling types
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontRef {
    pub family: String,
    pub weight: FcWeight,
    pub style: FontStyle,
    pub unicode_ranges: Vec<UnicodeRange>,
}

impl FontRef {
    fn invalid() -> Self {
        Self {
            family: "unknown".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        }
    }
}

impl Hash for FontRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the String field directly.
        self.family.hash(state);

        // Hash the FcWeight by casting the enum to its integer representation.
        (self.weight as u32).hash(state);

        // Hash the FontStyle by casting the enum to its integer representation.
        (self.style as u8).hash(state);

        // It is important to hash the length of the Vec to avoid collisions.
        self.unicode_ranges.len().hash(state);

        // Manually hash each element in the Vec since UnicodeRange doesn't implement Hash.
        for range in &self.unicode_ranges {
            range.start.hash(state);
            range.end.hash(state);
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone)]
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

#[derive(Debug, Clone)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
}

// Enhanced content model supporting mixed inline content
#[derive(Debug, Clone)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
}

#[derive(Debug, Clone)]
pub struct InlineImage {
    pub source: ImageSource,
    pub intrinsic_size: Size,
    pub display_size: Option<Size>,
    pub baseline_offset: f32, // How much to shift baseline
    pub alignment: VerticalAlign,
    pub object_fit: ObjectFit,
    pub alt_text: String, // Fallback text if image fails
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    Url(String),
    Data(Arc<[u8]>),
    Svg(Arc<str>),
    Placeholder(Size), // For layout without actual image
}

#[derive(Debug, Clone, Copy, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline, // Align image baseline with text baseline
    Bottom,      // Align image bottom with line bottom
    Top,         // Align image top with line top
    Middle,      // Align image middle with text middle
    TextTop,     // Align with tallest text in line
    TextBottom,  // Align with lowest text in line
    Sub,         // Subscript alignment
    Super,       // Superscript alignment
    Offset(f32), // Custom offset from baseline
}

#[derive(Debug, Clone, Copy)]
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
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    pub size: Size,
    pub baseline_offset: f32,
}

#[derive(Debug, Clone)]
pub struct InlineSpace {
    pub width: f32,
    pub is_breaking: bool, // Can line break here
    pub is_stretchy: bool, // Can be expanded for justification
}

#[derive(Debug, Clone)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
    pub content_index: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum BreakType {
    Soft,   // Preferred break (like <wbr>)
    Hard,   // Forced break (like <br>)
    Page,   // Page break
    Column, // Column break
}

#[derive(Debug, Clone, Copy)]
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
    pub exclusions: Vec<ShapeExclusion>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

#[derive(Debug, Clone)]
pub enum ShapeExclusion {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
    Image { bounds: Rect, shape: ImageShape },
}

#[derive(Debug, Clone)]
pub enum ImageShape {
    Rectangle,                    // Normal rectangular image
    AlphaMask(Arc<[u8]>),         // Use alpha channel as exclusion mask
    VectorMask(Vec<PathSegment>), // Vector clipping path
}

#[derive(Debug, Clone)]
pub struct LineShapeConstraints {
    pub segments: Vec<LineSegment>,
    pub total_width: f32,
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub start_x: f32,
    pub width: f32,
    pub priority: u8, // For choosing best segment when multiple available
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
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
pub struct InlineSize {
    pub width: f32,
    pub height: f32,
    pub baseline_offset: f32,
}

#[derive(Debug, Clone)]
pub struct OverflowInfo<T: ParsedFontTrait> {
    pub has_overflow: bool,
    pub overflow_bounds: Option<Rect>,
    pub clipped_content: Vec<ShapedItem<T>>,
}

impl<T: ParsedFontTrait> Default for OverflowInfo<T> {
    fn default() -> Self {
        Self {
            has_overflow: false,
            overflow_bounds: None,
            clipped_content: Vec::new(),
        }
    }
}

// Path and shape definitions
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Enhanced font management with fallback chains
#[derive(Debug, Clone)]
pub struct FontFallbackChain {
    pub primary: FontRef,
    pub fallbacks: Vec<FontRef>,
    pub script_specific: HashMap<Script, Vec<FontRef>>,
}

#[derive(Debug)]
pub struct FontManager<T: ParsedFontTrait, Q: FontLoaderTrait<T>> {
    fc_cache: FcFontCache,
    parsed_fonts: Mutex<HashMap<FontId, Arc<T>>>,
    font_ref_to_id_cache: Mutex<HashMap<FontRef, FontId>>,
    // Default: System font loader
    // (loads fonts from file - can be intercepted for mocking in tests)
    font_loader: Arc<Q>,
}

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone)]
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

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Bidi analysis failed: {0}")]
    BidiError(String),
    #[error("Shaping failed: {0}")]
    ShapingError(String),
    #[error("Font not found: {0:?}")]
    FontNotFound(FontRef),
    #[error("Invalid text input: {0}")]
    InvalidText(String),
    #[error("Hyphenation failed: {0}")]
    HyphenationError(String),
}

// Enhanced layout constraints supporting arbitrary shapes
#[derive(Debug, Clone)]
pub struct LayoutConstraints {
    pub shape: ShapeConstraints,
    pub justify_content: JustifyContent,
    pub vertical_align: VerticalAlign,
    pub overflow_behavior: OverflowBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl, // vertical-rl (vertical right-to-left)
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum JustifyContent {
    #[default]
    None,
    InterWord,      // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,     // Distribute space evenly including start/end
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq)]
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

// Enhanced style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    pub font_ref: FontRef,
    pub font_size_px: f32,
    pub color: Color,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub line_height: f32,
    pub text_decoration: TextDecoration,
    pub font_features: Vec<String>,

    // Vertical text properties
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    pub text_combine_upright: Option<TextCombineUpright>, // tate-chu-yoko
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextCombineUpright {
    None,
    All,        // Combine all characters in horizontal layout
    Digits(u8), // Combine up to N digits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BidiLevel(u8);

// Content representation after bidirectional analysis
#[derive(Debug, Clone)]
pub struct BidiAnalyzedContent<'a, 'b> {
    pub visual_runs: Vec<VisualRun<'a>>,
    pub non_text_items: &'b [(usize, InlineContent)],
    pub base_direction: Direction,
}

// Measured space representation
#[derive(Debug, Clone)]
pub struct MeasuredSpace {
    pub width: f32,
    pub content_index: usize,
}

// Information about text runs after initial analysis
#[derive(Debug, Clone)]
pub struct TextRunInfo<'a> {
    pub text: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start: usize,
    pub content_index: usize,
}

// Content representation after initial analysis
#[derive(Debug, Clone)]
pub struct AnalyzedContent<'a> {
    pub text_runs: Vec<TextRunInfo<'a>>,
    pub non_text_items: Vec<(usize, InlineContent)>,
    pub full_text: String,
    pub grapheme_boundaries: BTreeSet<usize>,
}

#[derive(Debug)]
struct LineMetrics {
    ascent: f32,
    descent: f32,
    line_gap: f32,
    total_height: f32,
}

/// Unified constraints combining all layout features
#[derive(Debug, Clone, Default)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeExclusion>,

    // Basic layout
    pub available_width: f32, // For simple rectangular layouts
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub justify_content: JustifyContent,
    pub line_height: f32,
    pub vertical_align: VerticalAlign,

    // Overflow handling
    pub overflow: OverflowBehavior,

    // Advanced features
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: bool,
    pub hyphenation_language: Option<Language>,
}

impl UnifiedConstraints {
    fn direction(&self, fallback: Direction) -> Direction {
        match self.writing_mode {
            Some(s) => s.get_direction().unwrap_or(fallback),
            None => fallback,
        }
    }
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

// Cache key structure
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct CacheKey {
    pub content_hash: u64,
    pub constraints_hash: u64,
}

// RenderCommand enum for representing rendering operations
#[derive(Debug, Clone)]
pub enum RenderCommand {
    DrawGlyph {
        glyph_id: u16,
        position: Point,
        color: Color,
    },
    DrawImage {
        source: ImageSource,
        position: Point,
        size: Size,
    },
    // Add other rendering commands as needed by your application
}

// Constraints for inline content layout
#[derive(Debug, Clone)]
pub struct InlineConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

/// Enhanced shaped item that unifies glyphs and inline content
#[derive(Debug, Clone)]
pub enum ShapedItem<T: ParsedFontTrait> {
    Glyph(Glyph<T>),
    Image(MeasuredImage),
    Shape(MeasuredShape),
    LineBreak(InlineBreak),
    Space(MeasuredSpace),
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

/// Unified line representation
#[derive(Debug, Clone)]
pub struct UnifiedLine<T: ParsedFontTrait> {
    pub items: Vec<ShapedItem<T>>,
    pub position: f32,
    pub constraints: LineConstraints,
    pub is_last: bool,
}

/// Final unified layout
#[derive(Debug, Clone)]
pub struct UnifiedLayout<T: ParsedFontTrait> {
    pub items: Vec<PositionedItem<T>>,
    pub bounds: Rect,
    pub overflow: OverflowInfo<T>,
}

#[derive(Debug, Clone)]
pub struct PositionedItem<T: ParsedFontTrait> {
    pub item: ShapedItem<T>,
    pub position: Point,
    pub bounds: Rect,
    pub line_index: usize,
}

/// Line constraints with multi-segment support
#[derive(Debug, Clone)]
pub struct LineConstraints {
    pub segments: Vec<LineSegment>,
    pub total_available: f32,
}

// --- BASIC --- //

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

impl UnifiedConstraints {
    pub fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            Some(WritingMode::VerticalRl) | Some(WritingMode::VerticalLr)
        )
    }

    pub fn should_justify<T: ParsedFontTrait>(&self, line: &UnifiedLine<T>) -> bool {
        // Don't justify last line unless JustifyAll
        self.justify_content != JustifyContent::None
            && (line.is_last == false || self.text_align == TextAlign::JustifyAll)
    }
}

impl<T: ParsedFontTrait> ShapedItem<T> {
    fn content_index(&self) -> usize {
        match self {
            ShapedItem::Glyph(g) => g.content_index,
            ShapedItem::Image(i) => i.content_index,
            ShapedItem::Shape(s) => s.content_index,
            ShapedItem::Space(s) => s.content_index,
            ShapedItem::LineBreak(b) => b.content_index,
        }
    }
}

// Assume a global or passed-in hyphenator cache for performance.
// For this example, we'll initialize it inside the function.
fn get_hyphenator() -> Result<Standard, LayoutError> {
    Standard::from_embedded(Language::EnglishUS)
        .map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

fn concatenate_runs_text(runs: &[StyledRun]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// Detects the base direction of a text string according to the Unicode Bidirectional Algorithm
///
/// Returns:
///
/// - `Direction::LTR` if the text is predominantly left-to-right
/// - `Direction::RTL` if the text is predominantly right-to-left
/// - `Direction::Neutral` if there are no strong directional characters or if counts are equal with
///   no strong first character
fn detect_base_direction<'a>(text: &'a str) -> (Direction, BidiInfo<'a>) {
    let bidi_info = BidiInfo::new(text, None);
    let para = &bidi_info.paragraphs[0];
    if para.level.is_rtl() {
        (Direction::Rtl, bidi_info)
    } else {
        (Direction::Ltr, bidi_info)
    }
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

fn find_word_boundaries_grapheme_aware<T: ParsedFontTrait>(
    glyphs: &[Glyph<T>],
    current_idx: usize,
    grapheme_boundaries: &BTreeSet<usize>,
) -> (usize, usize) {
    let mut start = current_idx;
    while start > 0
        && !glyphs[start - 1].is_whitespace()
        && grapheme_boundaries.contains(&(start - 1))
    {
        start -= 1;
    }

    let mut end = current_idx;
    while end < glyphs.len() && !glyphs[end].is_whitespace() && grapheme_boundaries.contains(&end) {
        end += 1;
    }

    (start, end)
}

fn try_hyphenate_word<T: ParsedFontTrait>(
    glyphs: &[Glyph<T>],
    word_start: usize,
    word_end: usize,
    source_text: &str,
    hyphenator: &Standard,
    available_width: f32,
    grapheme_boundaries: &BTreeSet<usize>,
) -> Option<usize> {
    // Extract word text
    if word_start >= word_end {
        return None;
    }

    let first_glyph = &glyphs[word_start];
    let last_glyph = &glyphs[word_end - 1];

    let word_slice = &source_text[first_glyph.logical_byte_index
        ..(last_glyph.logical_byte_index + last_glyph.logical_byte_len as usize)];

    // Get hyphenation points
    let hyphenated = hyphenator.hyphenate(word_slice);
    let break_indices = hyphenated.breaks;

    if break_indices.is_empty() {
        return None;
    }

    // Find best break point that fits
    let mut current_width = 0.0;
    for i in word_start..word_end {
        current_width += glyphs[i].advance;

        // Check if this is a valid hyphenation point
        for &break_idx in &break_indices {
            let byte_offset = glyphs[i].logical_byte_index - first_glyph.logical_byte_index;
            if byte_offset == break_idx && grapheme_boundaries.contains(&i) {
                if current_width <= available_width {
                    return Some(i);
                }
            }
        }
    }

    None
}

fn shape_visual_runs<Q: ParsedFontTrait, T: FontProviderTrait<Q>>(
    visual_runs: &[VisualRun],
    font_provider: &T,
) -> Result<Vec<Glyph<Q>>, LayoutError> {
    let mut all_shaped_glyphs = Vec::new();

    for run in visual_runs {
        let font = font_provider.load_font(&run.style.font_ref)?;

        let direction = if run.bidi_level.is_rtl() {
            Direction::Rtl
        } else {
            Direction::Ltr
        };

        let mut shaped_output =
            font.shape_text(run.text_slice, run.script, run.language, direction)?;

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

// --- HELPER FUNCTIONS FOR POSITIONING ---

fn find_word_boundaries<T: ParsedFontTrait>(
    glyphs: &[Glyph<T>],
    current_idx: usize,
) -> (usize, usize) {
    let mut start = current_idx;
    while start > 0 && !glyphs[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = current_idx;
    while end < glyphs.len() && !glyphs[end].is_whitespace() {
        end += 1;
    }
    (start, end)
}

fn get_available_width_for_line(line_y: f32, constraints: &UnifiedConstraints) -> f32 {
    let mut intervals: Vec<(f32, f32)> = Vec::new();

    for exclusion in &constraints.shape_exclusions {
        match exclusion {
            ShapeExclusion::Rectangle(rect) => {
                if line_y >= rect.y && line_y < rect.y + rect.height {
                    intervals.push((rect.x, rect.x + rect.width));
                }
            }
            ShapeExclusion::Circle { center, radius } => {
                let dy = line_y + constraints.line_height / 2.0 - center.y;
                if dy.abs() <= *radius {
                    let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                    intervals.push((center.x - dx, center.x + dx));
                }
            }
            ShapeExclusion::Ellipse { center, radii } => {
                let normalized_dy =
                    (line_y + constraints.line_height / 2.0 - center.y) / radii.height;
                if normalized_dy.abs() <= 1.0 {
                    let dx = radii.width * (1.0 - normalized_dy.powi(2)).sqrt();
                    intervals.push((center.x - dx, center.x + dx));
                }
            }
            ShapeExclusion::Polygon { points } => {
                if let Ok(segs) = UnifiedLayoutEngine::polygon_line_intersection(
                    points,
                    line_y,
                    constraints.line_height,
                ) {
                    for seg in segs {
                        intervals.push((seg.start_x, seg.start_x + seg.width));
                    }
                }
            }
            ShapeExclusion::Path { segments: _ } => {
                // TODO: Implement path intersection to compute excluded intervals
            }
            ShapeExclusion::Image { bounds, shape } => {
                match shape {
                    ImageShape::Rectangle => {
                        if line_y >= bounds.y && line_y < bounds.y + bounds.height {
                            intervals.push((bounds.x, bounds.x + bounds.width));
                        }
                    }
                    _ => {
                        // Approximate with bounds for complex shapes
                        if line_y >= bounds.y && line_y < bounds.y + bounds.height {
                            intervals.push((bounds.x, bounds.x + bounds.width));
                        }
                    }
                }
            }
        }
    }

    // Merge overlapping intervals and calculate total excluded width
    if intervals.is_empty() {
        return constraints.available_width;
    }
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged = vec![intervals[0]];
    for int in intervals.into_iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if int.0 <= last.1 {
            last.1 = last.1.max(int.1);
        } else {
            merged.push(int);
        }
    }
    let excluded: f32 = merged.iter().map(|(s, e)| e - s).sum();

    (constraints.available_width - excluded).max(0.0)
}

fn resolve_logical_align(align: TextAlign, direction: Direction) -> TextAlign {
    match (align, direction) {
        (TextAlign::Start, Direction::Ltr) => TextAlign::Left,
        (TextAlign::Start, Direction::Rtl) => TextAlign::Right,
        (TextAlign::End, Direction::Ltr) => TextAlign::Right,
        (TextAlign::End, Direction::Rtl) => TextAlign::Left,
        (other, _) => other,
    }
}

fn point_in_rect(point: Point, rect: Rect) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

// --- UTILS --- //

// FontManager with proper rust-fontconfig fallback
impl<T: ParsedFontTrait, Q: FontLoaderTrait<T>> FontProviderTrait<T> for FontManager<T, Q> {
    fn load_font(&self, font_ref: &FontRef) -> Result<Arc<T>, LayoutError> {
        // Check cache first
        if let Ok(c) = self.font_ref_to_id_cache.lock() {
            if let Some(cached_id) = c.get(font_ref) {
                let fonts = self.parsed_fonts.lock().unwrap();
                if let Some(font) = fonts.get(cached_id) {
                    return Ok(font.clone());
                }
            }
        }

        // Query fontconfig
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            weight: font_ref.weight,
            italic: if font_ref.style == FontStyle::Italic {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            oblique: if font_ref.style == FontStyle::Oblique {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            ..Default::default()
        };

        let mut trace = Vec::new();
        let fc_match = self
            .fc_cache
            .query(&pattern, &mut trace)
            .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

        // Load font if not cached
        {
            let mut fonts = self.parsed_fonts.lock().unwrap();
            if !fonts.contains_key(&fc_match.id) {
                let font_bytes = self
                    .fc_cache
                    .get_font_bytes(&fc_match.id)
                    .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

                let font_index = 0; // Default
                let parsed = self.font_loader.load_font(&font_bytes, font_index)?;

                fonts.insert(fc_match.id.clone(), parsed);
            }
        }

        // Update ref cache
        {
            let mut ref_cache = self.font_ref_to_id_cache.lock().unwrap();
            ref_cache.insert(font_ref.clone(), fc_match.id.clone());
        }

        let fonts = self.parsed_fonts.lock().unwrap();
        Ok(fonts.get(&fc_match.id).unwrap().clone())
    }
}

impl<T: ParsedFontTrait, Q: FontLoaderTrait<T>> FontManager<T, Q> {
    pub fn with_loader(fc_cache: FcFontCache, loader: Arc<Q>) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts: Mutex::new(HashMap::new()),
            font_loader: loader,
            font_ref_to_id_cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn load_font_by_id(&self, font_id: &FontId) -> Result<Arc<T>, LayoutError> {
        let mut cache = self.parsed_fonts.lock().unwrap();

        if let Some(font) = cache.get(font_id) {
            return Ok(font.clone());
        }

        let font_bytes = self
            .fc_cache
            .get_font_bytes(font_id)
            .ok_or_else(|| LayoutError::FontNotFound(FontRef::invalid()))?;

        let font_index = 0; // Default to first font in collection
        let parsed = self.font_loader.load_font(&font_bytes, font_index)?;

        cache.insert(font_id.clone(), parsed.clone());

        Ok(parsed)
    }

    pub fn get_font_for_text(
        &mut self,
        font_ref: &FontRef,
        text: &str,
        script: Script,
    ) -> Result<Arc<T>, LayoutError> {
        // Try primary font first
        if let Ok(font) = self.load_font(font_ref) {
            return Ok(font);
        }

        // Get fallback fonts that are close to the current main font
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            family: Some(font_ref.family.clone()),
            italic: if font_ref.style == FontStyle::Italic {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            oblique: if font_ref.style == FontStyle::Oblique {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            weight: font_ref.weight,
            unicode_ranges: script.get_unicode_ranges(),
            ..Default::default()
        };

        /// rust-config: find fonts that can render the given text, considering Unicode ranges
        ///
        /// Note: The result will match the entire code range, however, the text itself could have
        /// multiple unicode font ranges
        for font_match in self
            .fc_cache
            .query_for_text(&pattern, text, &mut Vec::new())
        {}

        Err(LayoutError::FontNotFound(font_ref.clone()))
    }

    fn fc_match_to_font_ref(&self, fc_match: &FontMatch) -> Result<FontRef, LayoutError> {
        // Convert rust-fontconfig match to our FontRef
        let fc_pattern = self
            .fc_cache
            .get_metadata_by_id(&fc_match.id)
            .ok_or_else(|| LayoutError::FontNotFound(FontRef::invalid()))?;

        Ok(FontRef {
            weight: fc_pattern.weight,
            style: if fc_pattern.oblique == PatternMatch::True {
                FontStyle::Oblique
            } else if fc_pattern.italic == PatternMatch::True {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            family: fc_pattern.family.clone().unwrap_or_default(),
            unicode_ranges: fc_pattern.unicode_ranges.clone(),
        })
    }
}

fn should_group_chars(ch1: char, ch2: char, script: Script) -> bool {
    // Group characters that are likely to use the same font
    // This is a simplified heuristic
    let script1 = crate::text3::script::detect_char_script(ch1);
    let script2 = crate::text3::script::detect_char_script(ch2);

    script1 == script2
        || (ch1.is_ascii() && ch2.is_ascii())
        || (ch1.is_whitespace() || ch2.is_whitespace())
}

fn fc_slant_to_style(slant: i32) -> FontStyle {
    match slant {
        0 => FontStyle::Normal,
        100 => FontStyle::Italic,
        _ => FontStyle::Oblique,
    }
}

fn determine_glyph_orientation(
    codepoint: u32,
    script: Script,
    text_orientation: TextOrientation,
    writing_mode: WritingMode,
) -> GlyphOrientation {
    match text_orientation {
        TextOrientation::Upright => GlyphOrientation::Upright,
        TextOrientation::Sideways => GlyphOrientation::Horizontal,
        TextOrientation::Mixed => get_default_orientation(codepoint, script, writing_mode),
    }
}

fn get_default_orientation(
    codepoint: u32,
    script: Script,
    writing_mode: WritingMode,
) -> GlyphOrientation {
    // Based on Unicode Vertical Orientation property
    match codepoint {
        // CJK ideographs, symbols - upright in vertical text
        0x4E00..=0x9FFF | // CJK Unified Ideographs
        0x3400..=0x4DBF | // CJK Extension A
        0x20000..=0x2A6DF => GlyphOrientation::Upright,

        // Latin, Arabic, etc. - rotated in vertical text
        0x0020..=0x007F => GlyphOrientation::Horizontal,

        // Punctuation - context dependent
        0x3000..=0x303F => get_punctuation_orientation(codepoint, writing_mode),

        // Default: use script-based heuristic
        _ => get_script_default_orientation(script, writing_mode)
    }
}

fn get_punctuation_orientation(codepoint: u32, writing_mode: WritingMode) -> GlyphOrientation {
    match codepoint {
        // Vertical forms of punctuation
        0x3001 | 0x3002 | // Ideographic comma, full stop
        0x300C | 0x300D | // Corner brackets
        0x300E | 0x300F |
        0x3010 | 0x3011 => GlyphOrientation::Upright,
        _ => GlyphOrientation::Horizontal,
    }
}

fn get_script_default_orientation(script: Script, writing_mode: WritingMode) -> GlyphOrientation {
    // If the writing mode is horizontal, use horizontal orientation
    match writing_mode {
        WritingMode::HorizontalTb => GlyphOrientation::Horizontal,
        WritingMode::SidewaysRl | WritingMode::SidewaysLr => GlyphOrientation::Horizontal,
        WritingMode::VerticalRl | WritingMode::VerticalLr => {
            // For vertical writing modes, check the script
            match script {
                Script::Hangul | Script::Hiragana | Script::Katakana | Script::Mandarin => {
                    GlyphOrientation::Upright
                }
                _ => GlyphOrientation::Horizontal,
            }
        }
    }
}

fn justify_line<T: ParsedFontTrait>(
    glyphs: &mut [Glyph<T>],
    target_width: f32,
    justify_content: JustifyContent,
    writing_mode: WritingMode,
    is_last_line: bool,
) -> Result<(), LayoutError> {
    if is_last_line && justify_content != JustifyContent::Distribute {
        return Ok(()); // Don't justify last line unless explicitly requested
    }

    let current_width = calculate_line_width(glyphs, writing_mode);
    if current_width >= target_width {
        return Ok(()); // Already fits or overflows
    }

    let available_space = target_width - current_width;

    match justify_content {
        JustifyContent::None => Ok(()),
        JustifyContent::InterWord => justify_inter_word(glyphs, available_space),
        JustifyContent::InterCharacter => justify_inter_character(glyphs, available_space),
        JustifyContent::Distribute => justify_distribute(glyphs, available_space),
    }
}

// Enhanced line breaking with grapheme cluster awareness
fn find_line_break_with_graphemes<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    items: &[ShapedItem<T>],
    line_constraints: &LineConstraints,
    constraints: &UnifiedConstraints,
    font_manager: &FontManager<T, Q>,
) -> Result<(usize, Vec<ShapedItem<T>>), LayoutError> {
    if line_constraints.segments.is_empty() {
        return Ok((0, Vec::new()));
    }

    let mut current_width = 0.0;
    let mut line_items = Vec::new();
    let available_width = line_constraints.total_available;

    for (i, item) in items.iter().enumerate() {
        let item_width = UnifiedLayoutEngine::get_item_measure(item, constraints);

        if current_width + item_width > available_width && i > 0 {
            // Try hyphenation for text items
            if constraints.hyphenation {
                if let ShapedItem::Glyph(g) = item {
                    // Simplified: just break at current position
                    return Ok((i, line_items));
                }
            }
            return Ok((i, line_items));
        }

        current_width += item_width;
        line_items.push(item.clone());
    }

    Ok((items.len(), line_items))
}

fn justify_line_items<T: ParsedFontTrait>(
    mut items: Vec<ShapedItem<T>>,
    constraints: &LineConstraints,
    justify_content: JustifyContent,
    writing_mode: Option<WritingMode>,
    is_last_line: bool,
) -> Result<Vec<ShapedItem<T>>, LayoutError> {
    if is_last_line && justify_content != JustifyContent::Distribute {
        return Ok(items);
    }

    let total_width: f32 = items
        .iter()
        .map(|item| {
            UnifiedLayoutEngine::get_item_measure(
                item,
                &UnifiedConstraints {
                    writing_mode,
                    ..Default::default()
                },
            )
        })
        .sum();

    let available = constraints.total_available;
    if total_width >= available {
        return Ok(items);
    }

    let extra_space = available - total_width;

    match justify_content {
        JustifyContent::InterWord => {
            let spaces: Vec<usize> = items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    if let ShapedItem::Glyph(g) = item {
                        if g.character_class() == CharacterClass::Space {
                            return Some(i);
                        }
                    }
                    None
                })
                .collect();

            if !spaces.is_empty() {
                let space_per_gap = extra_space / spaces.len() as f32;
                for &idx in &spaces {
                    if let ShapedItem::Glyph(ref mut g) = items[idx] {
                        g.advance += space_per_gap;
                    }
                }
            }
        }
        _ => {} // Other justification modes
    }

    Ok(items)
}

fn calculate_line_width<T: ParsedFontTrait>(glyphs: &[Glyph<T>], writing_mode: WritingMode) -> f32 {
    glyphs
        .iter()
        .map(|g| get_glyph_advance(g, writing_mode))
        .sum()
}

fn get_glyph_advance<T: ParsedFontTrait>(glyph: &Glyph<T>, writing_mode: WritingMode) -> f32 {
    match writing_mode {
        WritingMode::HorizontalTb => glyph.advance,
        WritingMode::VerticalRl | WritingMode::VerticalLr => glyph.vertical_advance,
        WritingMode::SidewaysRl | WritingMode::SidewaysLr => glyph.advance,
    }
}

fn justify_inter_word<T: ParsedFontTrait>(
    glyphs: &mut [Glyph<T>],
    available_space: f32,
) -> Result<(), LayoutError> {
    // Find all word boundaries (spaces and break opportunities)
    let space_indices: Vec<usize> = glyphs
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            if g.character_class() == CharacterClass::Space
                || (g.break_opportunity_after() && g.character_class() != CharacterClass::Combining)
            {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if space_indices.is_empty() {
        return Ok(());
    }

    let space_per_gap = available_space / space_indices.len() as f32;

    // Distribute space by expanding advances
    for &idx in &space_indices {
        glyphs[idx].advance += space_per_gap;
    }

    Ok(())
}

fn compute_grapheme_boundaries(text: &str) -> BTreeSet<usize> {
    text.grapheme_indices(true).map(|(idx, _)| idx).collect()
}

fn create_pattern_from_run(run: &VisualRun) -> FcPattern {
    FcPattern {
        name: Some(run.style.font_ref.family.clone()),
        weight: run.style.font_ref.weight,
        italic: if run.style.font_ref.style == FontStyle::Italic {
            PatternMatch::True
        } else {
            PatternMatch::DontCare
        },
        oblique: if run.style.font_ref.style == FontStyle::Oblique {
            PatternMatch::True
        } else {
            PatternMatch::DontCare
        },
        // Adding the language is important for selecting correct glyphs (e.g., CJK variants)
        unicode_ranges: run.script.get_unicode_ranges(),
        ..Default::default()
    }
}

fn create_fallback_glyphs<T: ParsedFontTrait>(
    run: &VisualRun,
    font: Arc<T>, // A last-resort font to get metrics from
) -> Result<Vec<Glyph<T>>, LayoutError> {
    let mut glyphs = Vec::new();
    let missing_glyph_id = 0; // .notdef glyph

    // Use a reasonable advance for missing glyphs, like 0.5em
    let advance = run.style.font_size_px * 0.5;

    for (byte_index, ch) in run.text_slice.char_indices() {
        glyphs.push(Glyph {
            glyph_id: missing_glyph_id,
            codepoint: ch,
            font: font.clone(),
            style: run.style.clone(),
            source: GlyphSource::Char,
            logical_byte_index: run.logical_start_byte + byte_index,
            logical_byte_len: ch.len_utf8(),
            content_index: 0, // This will be set later
            cluster: (run.logical_start_byte + byte_index) as u32,
            advance,
            offset: Point { x: 0.0, y: 0.0 },
            vertical_advance: run.style.line_height,
            vertical_origin_y: 0.0,
            vertical_bearing: Point {
                x: -advance / 2.0,
                y: 0.0,
            },
            orientation: GlyphOrientation::Horizontal,
            script: run.script,
            bidi_level: run.bidi_level,
        });
    }
    Ok(glyphs)
}

fn justify_inter_character<T: ParsedFontTrait>(
    glyphs: &mut [Glyph<T>],
    available_space: f32,
) -> Result<(), LayoutError> {
    // For CJK text - expand space between all characters
    let justifiable_gaps: Vec<usize> = glyphs
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            if g.can_justify()
                && g.character_class() != CharacterClass::Combining
                && i < glyphs.len() - 1
            {
                // Don't justify after last glyph
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if justifiable_gaps.is_empty() {
        return justify_inter_word(glyphs, available_space);
    }

    let space_per_gap = available_space / justifiable_gaps.len() as f32;

    for &idx in &justifiable_gaps {
        glyphs[idx].advance += space_per_gap;
    }

    Ok(())
}

fn justify_distribute<T: ParsedFontTrait>(
    glyphs: &mut [Glyph<T>],
    available_space: f32,
) -> Result<(), LayoutError> {
    // CSS text-align: justify - distribute space including at edges
    if glyphs.is_empty() {
        return Ok(());
    }

    // Add space at start, between characters, and at end
    let gaps = glyphs.len() + 1;
    let space_per_gap = available_space / gaps as f32;

    // Add space to each glyph's advance (except maybe the last)
    for glyph in glyphs.iter_mut() {
        glyph.advance += space_per_gap;
    }

    // The remaining space goes before the first character (handled in positioning)
    Ok(())
}

fn apply_vertical_metrics<T: ParsedFontTrait>(glyph: &mut Glyph<T>, font: &T) {
    // Get vertical metrics from VMTX, VORG tables
    if let Some(v_metrics) = font.get_vertical_metrics(glyph.glyph_id) {
        glyph.vertical_advance = v_metrics.advance;
        glyph.vertical_bearing.x = v_metrics.bearing_x;
        glyph.vertical_bearing.y = v_metrics.bearing_y;
        glyph.vertical_origin_y = v_metrics.origin_y;
    } else {
        // Fallback: derive from horizontal metrics
        glyph.vertical_advance = glyph.style.line_height;
        glyph.vertical_bearing.x = -glyph.advance / 2.0;
        glyph.vertical_bearing.y = 0.0;
        glyph.vertical_origin_y = glyph.style.font_size_px * 0.88; // TODO: Approximate
    }
}

// Implement CacheKey constructor
impl CacheKey {
    fn new(content: &[InlineContent], constraints: &UnifiedConstraints) -> Self {
        // TODO: Implement proper hashing logic here
        CacheKey {
            content_hash: 0,     // TODO: Replace with actual hash
            constraints_hash: 0, // TODO: Replace with actual hash
        }
    }
}

// Helper function to compute cache key
fn compute_cache_key(content: &[InlineContent], constraints: &UnifiedConstraints) -> CacheKey {
    CacheKey::new(content, constraints)
}

// --- ENGINE IMPLEMENTATION --- //

/// Unified layout engine combining all features into a single pipeline
pub struct UnifiedLayoutEngine;

impl UnifiedLayoutEngine {
    /// Main entry point for all text layout
    pub fn layout<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
        content: Vec<InlineContent>,
        constraints: UnifiedConstraints,
        font_manager: &FontManager<T, Q>,
        cache: &LayoutCache<T>,
    ) -> Result<Arc<UnifiedLayout<T>>, LayoutError> {
        // Check cache first
        let cache_key = CacheKey::new(&content, &constraints);

        if let Some(cached) = cache.get(&cache_key) {
            return Ok(cached);
        }

        // Stage 1: Content analysis and preparation
        let analyzed_content = Self::analyze_content(&content, &constraints)?;

        let (base_direction, bidi_info) = detect_base_direction(&analyzed_content.full_text);

        // Stage 2: Bidi analysis if text content exists
        let bidi_analyzed =
            Self::apply_bidi_analysis(&analyzed_content, &constraints, base_direction)?;

        // Stage 3: Shape all content with font fallback
        let shaped_content = Self::shape_content(bidi_analyzed, font_manager, &constraints)?;

        // Stage 4: Apply vertical text transformations if needed
        let oriented_content = Self::apply_text_orientation(shaped_content, &constraints)?;

        // Stage 5: Line breaking with shape awareness
        let lines = Self::break_lines(oriented_content, &constraints, font_manager)?;

        // Stage 6: Position content with justification
        let positioned = Self::position_content(lines, &constraints, base_direction, &bidi_info)?;

        // Stage 7: Apply overflow handling
        let final_layout = Self::handle_overflow(positioned, &constraints)?;

        let layout = Arc::new(final_layout);

        // Cache the result
        cache.put(cache_key, layout.clone());

        Ok(layout)
    }

    fn analyze_content<'a>(
        content: &'a [InlineContent],
        constraints: &UnifiedConstraints,
    ) -> Result<AnalyzedContent<'a>, LayoutError> {
        let mut text_runs = Vec::new();
        let mut non_text_items = Vec::new();
        let mut full_text = String::new();
        let mut byte_offset = 0;

        for (idx, item) in content.iter().enumerate() {
            match item {
                InlineContent::Text(run) => {
                    text_runs.push(TextRunInfo {
                        text: &run.text,
                        style: run.style.clone(),
                        logical_start: byte_offset,
                        content_index: idx,
                    });
                    full_text.push_str(&run.text);
                    byte_offset += run.text.len();
                }
                _ => {
                    non_text_items.push((idx, item.clone()));
                }
            }
        }

        let (base_direction, bidi_info) = detect_base_direction(&full_text);
        let grapheme_boundaries = compute_grapheme_boundaries(&full_text);

        Ok(AnalyzedContent {
            text_runs,
            non_text_items,
            full_text,
            grapheme_boundaries,
        })
    }

    fn apply_bidi_analysis<'a, 'b: 'a>(
        content: &'b AnalyzedContent<'a>,
        constraints: &UnifiedConstraints,
        base_direction: Direction,
    ) -> Result<BidiAnalyzedContent<'a, 'b>, LayoutError> {
        if content.full_text.is_empty() {
            return Ok(BidiAnalyzedContent {
                visual_runs: Vec::new(),
                non_text_items: &content.non_text_items,
                base_direction,
            });
        }

        let (visual_runs, unified_direction) = perform_bidi_analysis(
            &content.text_runs,
            &content.full_text,
            constraints.hyphenation_language,
        )?;

        Ok(BidiAnalyzedContent {
            visual_runs,
            non_text_items: &content.non_text_items,
            base_direction: unified_direction,
        })
    }

    fn shape_content<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
        content: BidiAnalyzedContent,
        font_manager: &FontManager<T, Q>,
        constraints: &UnifiedConstraints,
    ) -> Result<Vec<ShapedItem<T>>, LayoutError> {
        let mut shaped_items = Vec::new();

        // Shape text runs with font fallback
        for run in &content.visual_runs {
            let pattern = create_pattern_from_run(run);
            let font_matches =
                font_manager
                    .fc_cache
                    .query_for_text(&pattern, run.text_slice, &mut Vec::new());

            let mut shaped_glyphs: Option<Vec<Glyph<T>>> = None;
            let direction = if run.bidi_level.is_rtl() {
                Direction::Rtl
            } else {
                Direction::Ltr
            };

            // Try each font in the fallback list
            for font_match in &font_matches {
                if let Ok(font) = font_manager.load_font_by_id(&font_match.id) {
                    // Attempt to shape the entire run with this font.
                    // A robust implementation would check for .notdef glyphs in the output.
                    // For simplicity, we assume success if it doesn't error out.
                    if let Ok(mut glyphs) =
                        font.shape_text(run.text_slice, run.script, run.language, direction)
                    {
                        if direction == Direction::Rtl {
                            glyphs.reverse();
                        }
                        shaped_glyphs = Some(glyphs);
                        break; // Success, stop trying other fonts
                    }
                }
            }

            // If all fonts failed, generate fallback glyphs
            let final_glyphs = match shaped_glyphs {
                Some(g) => g,
                None => {
                    // Load the primary font of the run as a "last resort" for metrics
                    let primary_font = font_manager.load_font(&run.style.font_ref)?;
                    create_fallback_glyphs(run, primary_font)?
                }
            };

            for glyph in final_glyphs {
                shaped_items.push(ShapedItem::Glyph(glyph));
            }
        }

        // Measure non-text items
        for (_idx, item) in content.non_text_items.iter() {
            shaped_items.push(Self::measure_inline_item(item, constraints)?);
        }

        // Sort by content index to maintain order
        shaped_items.sort_by_key(|item| item.content_index());

        Ok(shaped_items)
    }

    fn apply_text_orientation<T: ParsedFontTrait>(
        mut items: Vec<ShapedItem<T>>,
        constraints: &UnifiedConstraints,
    ) -> Result<Vec<ShapedItem<T>>, LayoutError> {
        if !constraints.is_vertical() {
            return Ok(items);
        }

        for item in &mut items {
            if let ShapedItem::Glyph(ref mut glyph) = item {
                // Determine orientation for this glyph
                let orientation = determine_glyph_orientation(
                    glyph.codepoint as u32,
                    glyph.script,
                    constraints.text_orientation,
                    constraints.writing_mode.unwrap_or_default(),
                );

                glyph.orientation = orientation;

                // Apply vertical metrics
                if let Some(metrics) = glyph.font.get_vertical_metrics(glyph.glyph_id) {
                    glyph.vertical_advance = metrics.advance;
                    glyph.vertical_origin_y = metrics.origin_y;
                    glyph.vertical_bearing = Point {
                        x: metrics.bearing_x,
                        y: metrics.bearing_y,
                    };
                } else {
                    // Synthesize vertical metrics
                    glyph.vertical_advance = glyph.style.line_height;
                    glyph.vertical_origin_y = 0.0;
                    glyph.vertical_bearing = Point {
                        x: -glyph.advance / 2.0,
                        y: glyph.style.font_size_px * 0.88,
                    };
                }
            }
        }

        Ok(items)
    }

    fn break_lines<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
        items: Vec<ShapedItem<T>>,
        constraints: &UnifiedConstraints,
        font_manager: &FontManager<T, Q>,
    ) -> Result<Vec<UnifiedLine<T>>, LayoutError> {
        let mut lines = Vec::new();
        let mut current_position = 0.0;
        let mut item_cursor = 0;

        while item_cursor < items.len() {
            // Get constraints for current line position
            let line_constraints = Self::get_line_constraints(current_position, constraints)?;

            if line_constraints.segments.is_empty() {
                // No space available, move to next position
                current_position += constraints.line_height;
                continue;
            }

            // Find line break
            let (line_end, line_items) = find_line_break_with_graphemes(
                &items[item_cursor..],
                &line_constraints,
                constraints,
                font_manager,
            )?;

            if line_items.is_empty() {
                // Handle overflow
                if constraints.overflow == OverflowBehavior::Break {
                    break;
                }
                current_position += constraints.line_height;
                continue;
            }

            lines.push(UnifiedLine {
                items: line_items,
                position: current_position,
                constraints: line_constraints,
                is_last: false,
            });

            current_position += constraints.line_height;
            item_cursor += line_end;
        }

        if let Some(l) = lines.last_mut() {
            l.is_last = true;
        }

        Ok(lines)
    }

    fn get_line_constraints(
        position: f32,
        constraints: &UnifiedConstraints,
    ) -> Result<LineConstraints, LayoutError> {
        let mut segments = Vec::new();

        // Get segments from shape boundaries
        for boundary in &constraints.shape_boundaries {
            let boundary_segments = Self::get_boundary_segments(
                boundary,
                position,
                constraints.line_height,
                constraints.writing_mode.unwrap_or_default(),
            )?;
            segments.extend(boundary_segments);
        }

        // Subtract exclusions (holes, floats, etc.)
        for exclusion in &constraints.shape_exclusions {
            segments =
                Self::subtract_exclusion(segments, exclusion, position, constraints.line_height)?;
        }

        // Merge and optimize segments
        segments = Self::merge_segments(segments);

        let total_available = segments.iter().map(|s| s.width).sum();

        Ok(LineConstraints {
            segments,
            total_available,
        })
    }

    // Enhanced bidi line reordering
    // In `impl UnifiedLayoutEngine`

    /// Positions content on each line, performing justification, alignment, and
    /// Unicode Bidirectional Algorithm (UBA) reordering for the line.
    ///
    /// This function replaces the custom Bidi reordering logic with a robust
    /// implementation that delegates to the `unicode_bidi` crate.
    fn position_content<T: ParsedFontTrait>(
        lines: Vec<UnifiedLine<T>>,
        constraints: &UnifiedConstraints,
        base_direction: Direction,
        bidi_info: &BidiInfo, // Takes the pre-computed BidiInfo
    ) -> Result<UnifiedLayout<T>, LayoutError> {
        let mut positioned_items = Vec::new();
        let total_lines = lines.len();

        // For simplicity, we assume the text content is a single paragraph.
        // A multi-paragraph implementation would need to find the correct paragraph
        // for each line's byte range.
        let para_info = if bidi_info.paragraphs.is_empty() {
            None
        } else {
            Some(&bidi_info.paragraphs[0])
        };

        for (line_idx, mut line) in lines.into_iter().enumerate() {
            if line.items.is_empty() {
                continue;
            }
            line.is_last = line_idx == total_lines.saturating_sub(1);

            let should_justify = constraints.should_justify(&line);

            // --- START: BIDI REORDERING LOGIC ---

            let reordered_items = if let Some(para) = para_info {
                // 1. Find the logical byte range of the glyphs on this line.
                let line_start_byte = line.items.iter().find_map(|item| match item {
                    ShapedItem::Glyph(g) => Some(g.logical_byte_index),
                    _ => None,
                });

                if let Some(start_byte) = line_start_byte {
                    let end_byte = line
                        .items
                        .iter()
                        .rev()
                        .find_map(|item| match item {
                            ShapedItem::Glyph(g) => Some(g.logical_byte_index + g.logical_byte_len),
                            _ => None,
                        })
                        .unwrap_or(start_byte);

                    let line_byte_range = start_byte..end_byte;

                    // 2. Get the visually ordered segments (runs) from the unicode_bidi crate.
                    let (_, visual_runs) = bidi_info.visual_runs(para, line_byte_range);

                    // 3. Build the reordered list of items by mapping our logical items to the
                    //    visual runs.
                    let mut reordered: Vec<ShapedItem<T>> = Vec::with_capacity(line.items.len());
                    for run_range in visual_runs {
                        for item in &line.items {
                            // NOTE: This implementation correctly reorders text glyphs.
                            // A complete implementation for non-text items (images, shapes)
                            // would require them to be assigned a logical byte position during
                            // the initial content analysis phase, treating them like an
                            // object replacement character (U+FFFC).
                            if let ShapedItem::Glyph(g) = item {
                                if g.logical_byte_index >= run_range.start
                                    && g.logical_byte_index < run_range.end
                                {
                                    reordered.push(item.clone());
                                }
                            }
                        }
                    }
                    reordered
                } else {
                    // Line contains no text, so no reordering is necessary.
                    line.items
                }
            } else {
                // No paragraph info, likely because text is empty.
                line.items
            };

            // --- END: BIDI REORDERING LOGIC ---

            // The rest of the pipeline continues as before, but with a correctly ordered item list.
            let justified_items = if should_justify {
                justify_line_items(
                    reordered_items,
                    &line.constraints,
                    constraints.justify_content,
                    constraints.writing_mode,
                    line.is_last,
                )?
            } else {
                reordered_items
            };

            // Resolve logical alignment (start/end) to physical alignment (left/right)
            let physical_align = resolve_logical_align(constraints.text_align, base_direction);

            // Position items
            let mut inline_position = Self::calculate_alignment_offset(
                &justified_items,
                &line.constraints,
                physical_align,
                constraints.writing_mode,
            );

            for item in justified_items {
                let (item_advance, item_bounds) = Self::get_item_metrics(&item, constraints);

                let positioned = PositionedItem {
                    item,
                    position: Point {
                        x: if constraints.is_vertical() {
                            line.position
                        } else {
                            inline_position
                        },
                        y: if constraints.is_vertical() {
                            inline_position
                        } else {
                            line.position
                        },
                    },
                    bounds: item_bounds,
                    line_index: line_idx,
                };

                inline_position += item_advance;
                positioned_items.push(positioned);
            }
        }

        let bounds = Self::calculate_bounds(&positioned_items);

        Ok(UnifiedLayout {
            items: positioned_items,
            bounds,
            overflow: OverflowInfo::default(),
        })
    }

    fn handle_overflow<T: ParsedFontTrait>(
        mut layout: UnifiedLayout<T>,
        constraints: &UnifiedConstraints,
    ) -> Result<UnifiedLayout<T>, LayoutError> {
        match constraints.overflow {
            OverflowBehavior::Hidden => {
                // Clip items outside bounds
                layout.items.retain(|item| {
                    Self::item_intersects_bounds(item, &constraints.shape_boundaries)
                });
            }
            OverflowBehavior::Scroll => {
                // Mark overflow areas for scrolling
                layout.overflow =
                    Self::calculate_overflow(&layout.items, &constraints.shape_boundaries);
            }
            _ => {}
        }

        Ok(layout)
    }

    fn measure_inline_item<T: ParsedFontTrait>(
        item: &InlineContent,
        constraints: &UnifiedConstraints,
    ) -> Result<ShapedItem<T>, LayoutError> {
        match item {
            InlineContent::Image(img) => {
                let size = img.display_size.unwrap_or(img.intrinsic_size);
                Ok(ShapedItem::Image(MeasuredImage {
                    source: img.source.clone(),
                    size,
                    baseline_offset: img.baseline_offset,
                    alignment: img.alignment,
                    content_index: 0,
                }))
            }
            InlineContent::Shape(shape) => Ok(ShapedItem::Shape(MeasuredShape {
                shape_def: shape.shape_def.clone(),
                size: shape.size,
                baseline_offset: shape.baseline_offset,
                content_index: 0,
            })),
            InlineContent::Space(space) => Ok(ShapedItem::Space(MeasuredSpace {
                width: space.width,
                content_index: 0,
            })),
            InlineContent::LineBreak(br) => Ok(ShapedItem::LineBreak(br.clone())),
            _ => Err(LayoutError::InvalidText(
                "Unexpected inline content".to_string(),
            )),
        }
    }

    fn get_item_metrics<T: ParsedFontTrait>(
        item: &ShapedItem<T>,
        constraints: &UnifiedConstraints,
    ) -> (f32, Rect) {
        match item {
            ShapedItem::Glyph(g) => {
                let advance = if constraints.is_vertical() {
                    g.vertical_advance
                } else {
                    g.advance
                };
                (advance, g.bounds())
            }
            ShapedItem::Image(img) => {
                let advance = img.size.width;
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: img.size.width,
                    height: img.size.height,
                };
                (advance, bounds)
            }
            ShapedItem::Shape(shape) => {
                let advance = shape.size.width;
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: shape.size.width,
                    height: shape.size.height,
                };
                (advance, bounds)
            }
            ShapedItem::Space(space) => (
                space.width,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: space.width,
                    height: 0.0,
                },
            ),
            ShapedItem::LineBreak(_) => (
                0.0,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
            ),
        }
    }

    fn get_boundary_segments(
        boundary: &ShapeBoundary,
        y: f32,
        line_height: f32,
        writing_mode: WritingMode,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        match boundary {
            ShapeBoundary::Rectangle(rect) => {
                if y >= rect.y && y + line_height <= rect.y + rect.height {
                    Ok(vec![LineSegment {
                        start_x: rect.x,
                        width: rect.width,
                        priority: 0,
                    }])
                } else {
                    Ok(vec![])
                }
            }
            ShapeBoundary::Circle { center, radius } => {
                let dy = (y + line_height / 2.0) - center.y;
                if dy.abs() <= *radius {
                    let dx = (radius * radius - dy * dy).sqrt();
                    Ok(vec![LineSegment {
                        start_x: center.x - dx,
                        width: dx * 2.0,
                        priority: 0,
                    }])
                } else {
                    Ok(vec![])
                }
            }
            ShapeBoundary::Polygon { points } => {
                Self::polygon_line_intersection(points, y, line_height)
            }
            _ => Ok(vec![]),
        }
    }

    fn subtract_exclusion(
        mut segments: Vec<LineSegment>,
        exclusion: &ShapeExclusion,
        y: f32,
        line_height: f32,
    ) -> Result<Vec<LineSegment>, LayoutError> {
        let exclusion_segments = match exclusion {
            ShapeExclusion::Rectangle(rect) => {
                if y >= rect.y && y + line_height <= rect.y + rect.height {
                    vec![(rect.x, rect.x + rect.width)]
                } else {
                    vec![]
                }
            }
            ShapeExclusion::Circle { center, radius } => {
                let dy = (y + line_height / 2.0) - center.y;
                if dy.abs() <= *radius {
                    let dx = (radius * radius - dy * dy).sqrt();
                    vec![(center.x - dx, center.x + dx)]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };

        for (excl_start, excl_end) in exclusion_segments {
            let mut new_segments = Vec::new();
            for seg in segments {
                let seg_end = seg.start_x + seg.width;
                if seg.start_x >= excl_end || seg_end <= excl_start {
                    new_segments.push(seg);
                } else {
                    if seg.start_x < excl_start {
                        new_segments.push(LineSegment {
                            start_x: seg.start_x,
                            width: excl_start - seg.start_x,
                            priority: seg.priority,
                        });
                    }
                    if seg_end > excl_end {
                        new_segments.push(LineSegment {
                            start_x: excl_end,
                            width: seg_end - excl_end,
                            priority: seg.priority,
                        });
                    }
                }
            }
            segments = new_segments;
        }
        Ok(segments)
    }

    fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
        if segments.is_empty() {
            return segments;
        }

        segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());

        let mut merged = Vec::new();
        let mut current = segments[0].clone();

        for seg in segments.into_iter().skip(1) {
            let current_end = current.start_x + current.width;
            if seg.start_x <= current_end {
                current.width = (seg.start_x + seg.width - current.start_x).max(current.width);
            } else {
                merged.push(current);
                current = seg;
            }
        }
        merged.push(current);
        merged
    }

    fn get_item_measure<T: ParsedFontTrait>(
        item: &ShapedItem<T>,
        constraints: &UnifiedConstraints,
    ) -> f32 {
        match item {
            ShapedItem::Glyph(g) => {
                if constraints.is_vertical() {
                    g.vertical_advance
                } else {
                    g.advance
                }
            }
            ShapedItem::Image(i) => i.size.width,
            ShapedItem::Shape(s) => s.size.width,
            ShapedItem::Space(s) => s.width,
            ShapedItem::LineBreak(_) => 0.0,
        }
    }

    fn get_item_advance<T: ParsedFontTrait>(
        item: &PositionedItem<T>,
        constraints: &UnifiedConstraints,
    ) -> f32 {
        Self::get_item_measure(&item.item, constraints)
    }

    fn try_hyphenate<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
        item: &ShapedItem<T>,
        available_width: f32,
        font_manager: &FontManager<T, Q>,
    ) -> Option<ShapedItem<T>> {
        // TODO: Simplified hyphenation - would use hyphenation library
        None
    }

    fn calculate_alignment_offset<T: ParsedFontTrait>(
        items: &[ShapedItem<T>],
        constraints: &LineConstraints,
        align: TextAlign,
        writing_mode: Option<WritingMode>,
    ) -> f32 {
        if constraints.segments.is_empty() {
            return 0.0;
        }

        let total_width: f32 = items
            .iter()
            .map(|item| {
                Self::get_item_measure(
                    item,
                    &UnifiedConstraints {
                        writing_mode,
                        ..Default::default()
                    },
                )
            })
            .sum();

        let available = constraints.segments[0].width;

        match align {
            TextAlign::Center => (available - total_width) / 2.0,
            TextAlign::Right | TextAlign::End => available - total_width,
            _ => 0.0,
        }
    }

    fn position_item<T: ParsedFontTrait>(
        item: ShapedItem<T>,
        position: Point,
        constraints: &UnifiedConstraints,
    ) -> Result<PositionedItem<T>, LayoutError> {
        let bounds = match &item {
            ShapedItem::Glyph(g) => g.bounds(),
            ShapedItem::Image(i) => Rect {
                x: position.x,
                y: position.y,
                width: i.size.width,
                height: i.size.height,
            },
            ShapedItem::Shape(s) => Rect {
                x: position.x,
                y: position.y,
                width: s.size.width,
                height: s.size.height,
            },
            _ => Rect {
                x: position.x,
                y: position.y,
                width: 0.0,
                height: 0.0,
            },
        };

        Ok(PositionedItem {
            item,
            position,
            bounds,
            line_index: 0,
        })
    }

    fn calculate_bounds<T: ParsedFontTrait>(items: &[PositionedItem<T>]) -> Rect {
        if items.is_empty() {
            return Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            };
        }

        let min_x = items
            .iter()
            .map(|i| i.bounds.x)
            .fold(f32::INFINITY, f32::min);
        let min_y = items
            .iter()
            .map(|i| i.bounds.y)
            .fold(f32::INFINITY, f32::min);
        let max_x = items
            .iter()
            .map(|i| i.bounds.x + i.bounds.width)
            .fold(0.0, f32::max);
        let max_y = items
            .iter()
            .map(|i| i.bounds.y + i.bounds.height)
            .fold(0.0, f32::max);

        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    fn item_intersects_bounds<T: ParsedFontTrait>(
        item: &PositionedItem<T>,
        boundaries: &[ShapeBoundary],
    ) -> bool {
        // TODO: Simplified - check if item center is within any boundary
        true
    }

    fn calculate_overflow<T: ParsedFontTrait>(
        items: &[PositionedItem<T>],
        boundaries: &[ShapeBoundary],
    ) -> OverflowInfo<T> {
        // TODO: Simplified
        OverflowInfo::default()
    }

    // Polygon intersection with scanline algorithm
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

        // Use winding number algorithm for robustness
        for i in 0..points.len() {
            let p1 = points[i];
            let p2 = points[(i + 1) % points.len()];

            // Skip horizontal edges
            if (p2.y - p1.y).abs() < f32::EPSILON {
                continue;
            }

            // Check if scanline crosses this edge
            let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
                || (p1.y > line_center_y && p2.y <= line_center_y);

            if crosses {
                // Calculate intersection x
                let t = (line_center_y - p1.y) / (p2.y - p1.y);
                let x = p1.x + t * (p2.x - p1.x);

                // Track whether we're entering or exiting the polygon
                let entering = p1.y < p2.y;
                intersections.push((x, entering));
            }
        }

        // Sort intersections by x coordinate
        intersections.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Build segments from paired intersections
        let mut segments = Vec::new();
        let mut depth = 0;
        let mut start_x = None;

        for (x, entering) in intersections {
            if entering {
                if depth == 0 {
                    start_x = Some(x);
                }
                depth += 1;
            } else {
                depth -= 1;
                if let Some(sx) = start_x {
                    if depth == 0 {
                        segments.push(LineSegment {
                            start_x: sx,
                            width: x - sx,
                            priority: 0,
                        });
                        start_x = None;
                    }
                }
            }
        }

        Ok(segments)
    }
}

// Fix: Proper character classification
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

// LayoutCache struct to encapsulate caching functionality
pub struct LayoutCache<T: ParsedFontTrait> {
    cache: Arc<Mutex<LruCache<CacheKey, Arc<UnifiedLayout<T>>>>>,
}

impl<T: ParsedFontTrait> LayoutCache<T> {
    pub fn new(capacity: usize) -> Self {
        LayoutCache {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(if capacity == 0 { 1 } else { capacity }).unwrap(),
            ))),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<Arc<UnifiedLayout<T>>> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: CacheKey, layout: Arc<UnifiedLayout<T>>) {
        self.cache.lock().unwrap().put(key, layout);
    }
}
