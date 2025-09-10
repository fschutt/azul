use std::{
    collections::{BTreeSet, HashMap},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use hyphenation::{Hyphenator as _, Language, Load as _, Standard};
use lru::LruCache;
use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, FontId, FontMatch, PatternMatch};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

use crate::text3::script::Script;

pub trait ParsedFontTrait: Send + Sync {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: Direction,
    ) -> Result<Vec<ShapedGlyph>, LayoutError>;

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> (u16, f32);
    fn has_glyph(&self, codepoint: u32) -> bool;
    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics>;
    fn get_font_metrics(&self) -> FontMetrics;
}

pub trait FontLoaderTrait: Send + Sync + core::fmt::Debug {
    fn load_font(
        &self,
        font_bytes: &[u8],
        font_index: usize,
    ) -> Result<Box<dyn ParsedFontTrait>, LayoutError>;
}

// Font loading and management
pub trait FontProviderTrait {
    fn load_font<T: ParsedFontTrait>(&self, font_ref: &FontRef) -> Result<Arc<T>, LayoutError>;
    fn get_fallback_chain(&self, font_ref: &FontRef, script: Script) -> Vec<FontRef>;
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontRef {
    pub family: String,
    pub weight: u16,
    pub style: FontStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy)]
pub enum VerticalAlign {
    Baseline,    // Align image baseline with text baseline
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

#[derive(Debug, Clone, Copy)]
pub enum OverflowBehavior {
    Visible, // Content extends outside shape
    Hidden,  // Content is clipped to shape
    Scroll,  // Scrollable overflow
    Auto,    // Browser/system decides
    Break,   // Break into next shape/page
}

// Content representation for shaped layout
#[derive(Debug, Clone)]
pub enum ShapedInlineItem {
    Glyph(ShapedGlyph),
    Image(MeasuredImage),
    Shape(MeasuredShape),
    Space(InlineSpace),
    Break(InlineBreak),
    Custom(InlineSize),
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
pub struct PositionedInlineItem {
    pub content: ShapedInlineItem,
    pub position: Point,
    pub bounds: Rect,
}

#[derive(Debug, Clone)]
pub struct ShapedLine {
    pub y: f32,
    pub content: Vec<PositionedInlineItem>,
    pub constraints: LineShapeConstraints,
    pub baseline_y: f32,
}

#[derive(Debug, Clone)]
pub struct ShapedLayout {
    pub content: Vec<ShapedLine>,
    pub bounds: Rect,
    pub overflow: OverflowInfo,
}

#[derive(Debug, Clone, Default)]
pub struct OverflowInfo {
    pub has_overflow: bool,
    pub overflow_bounds: Option<Rect>,
    pub clipped_content: Vec<ShapedInlineItem>,
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
pub struct FontManager {
    fc_cache: FcFontCache,
    parsed_fonts: HashMap<FontId, Arc<ParsedFont>>,
    fallback_chains: HashMap<FontRef, FontFallbackChain>,
    // Default: System font loader (loads fonts from file - can be intercepted for mocking in
    // tests)
    font_loader: Arc<dyn FontLoaderTrait>,
}

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone)]
pub struct StyledRun {
    pub text: String,
    pub style: StyleProperties,
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

#[derive(Debug)]
struct SystemFontLoader;

impl SystemFontLoader {
    fn new() -> Self {
        Self
    }
}

impl FontLoaderTrait for SystemFontLoader {
    fn load_font(
        &self,
        font_bytes: &[u8],
        font_index: usize,
    ) -> Result<Box<dyn ParsedFontTrait>, LayoutError> {
        // Implementation would load the actual font
        unimplemented!("System font loading")
    }
}

// Enhanced layout constraints supporting arbitrary shapes
#[derive(Debug, Clone)]
pub struct LayoutConstraints {
    pub shape: ShapeConstraints,
    pub justify_content: JustifyContent,
    pub vertical_align: VerticalAlign,
    pub overflow_behavior: OverflowBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WritingMode {
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl,   // vertical-rl (vertical right-to-left)
    VerticalLr,   // vertical-lr (vertical left-to-right)
    SidewaysRl,   // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr,   // sideways-lr (rotated horizontal in vertical context)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JustifyContent {
    None,
    InterWord,      // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,     // Distribute space evenly including start/end
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,        // Logical start/end
    JustifyAll, // Justify including last line
}

// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextOrientation {
    Mixed,    // Default: upright for scripts, rotated for others
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

// Glyph with vertical metrics and justification info
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    pub style: StyleProperties,

    // Horizontal metrics
    pub advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,

    // Vertical metrics (for vertical text)
    pub vertical_advance: f32,
    pub vertical_x_offset: f32,
    pub vertical_y_offset: f32,
    pub vertical_origin_y: f32, // From VORG table

    // Source mapping
    pub logical_byte_start: usize,
    pub logical_byte_len: u8,
    pub cluster: u32,

    // Layout properties
    pub source: GlyphSource,
    pub is_whitespace: bool,
    pub break_opportunity_after: bool,
    pub can_justify: bool, // Can this glyph be expanded for justification?
    pub justification_priority: u8, // 0 = highest priority (spaces), 255 = lowest
    pub character_class: CharacterClass, // For justification rules
    pub text_orientation: GlyphOrientation, // How this glyph should be oriented
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone)]
pub struct BidiLevel(u8);

// Content representation after bidirectional analysis
#[derive(Debug, Clone)]
pub struct BidiAnalyzedContent<'a> {
    pub visual_runs: Vec<VisualRun<'a>>, // Using 'static lifetime for simplicity
    pub non_text_items: Vec<(usize, InlineContent)>,
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
pub struct TextRunInfo {
    pub text: String,
    pub style: StyleProperties,
    pub logical_start: usize,
    pub content_index: usize,
}

// Content representation after initial analysis
#[derive(Debug, Clone)]
pub struct AnalyzedContent {
    pub text_runs: Vec<TextRunInfo>,
    pub non_text_items: Vec<(usize, InlineContent)>,
    pub full_text: String,
    pub base_direction: Direction,
}

#[derive(Debug)]
struct LineMetrics {
    ascent: f32,
    descent: f32,
    line_gap: f32,
    total_height: f32,
}

#[derive(Debug, Clone)]
pub struct LineLayout {
    pub bounds: Rect,
    pub baseline_y: f32,
    pub glyph_start: usize,
    pub glyph_count: usize,
    pub logical_start_byte: usize,
    pub logical_end_byte: usize,
}

// Final layout result
#[derive(Debug, Clone)]
pub struct ParagraphLayout {
    pub glyphs: Vec<PositionedGlyph>,
    pub lines: Vec<LineLayout>,
    pub content_size: Size,
    pub source_text: String,
    pub base_direction: Direction,
}

// Stage 4: Positioning - Final positioned glyphs ready for rendering
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    // Rendering information
    pub glyph_id: u16,
    pub style: StyleProperties,
    pub x: f32,
    pub y: f32,
    pub bounds: Rect,

    // Layout information
    pub advance: f32,
    pub line_index: usize,

    // Source mapping for editing/selection
    pub logical_char_byte_index: usize,
    pub logical_char_byte_count: u8,
    pub visual_index: usize,

    // Bidi information
    pub bidi_level: BidiLevel,
}

/// Unified constraints combining all layout features
#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeExclusion>,

    // Basic layout
    pub available_width: f32, // For simple rectangular layouts
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: WritingMode,
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
pub enum ShapedItem {
    Glyph(EnhancedGlyph),
    Image(MeasuredImage),
    Shape(MeasuredShape),
    LineBreak(InlineBreak),
    Space(MeasuredSpace),
}

/// Enhanced glyph with all features
#[derive(Debug, Clone)]
pub struct EnhancedGlyph<T: ParsedFontTrait> {
    // Core glyph data
    pub glyph_id: u16,
    pub codepoint: u32,
    pub font: Arc<T>,
    pub style: Arc<StyleProperties>,

    // Metrics
    pub advance: f32,
    pub offset: Point,
    pub bounds: Rect,

    // Vertical text support
    pub vertical_advance: f32,
    pub vertical_origin: Point,
    pub orientation: GlyphOrientation,

    // Text mapping
    pub logical_byte_index: usize,
    pub logical_byte_len: usize,
    pub content_index: usize,

    // Layout properties
    pub script: Script,
    pub bidi_level: BidiLevel,
    pub character_class: CharacterClass,
    pub can_justify: bool,
    pub justification_priority: u8,
    pub break_opportunity_after: bool,
}

/// Unified line representation
#[derive(Debug, Clone)]
pub struct UnifiedLine {
    pub items: Vec<ShapedItem>,
    pub position: f32,
    pub constraints: LineConstraints,
    pub is_last: bool,
}

/// Final unified layout
#[derive(Debug, Clone)]
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    pub bounds: Rect,
    pub overflow: OverflowInfo,
}

#[derive(Debug, Clone)]
pub struct PositionedItem {
    pub item: ShapedItem,
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

#[derive(Debug, PartialEq)]
enum BidiClass {
    L,   // Left-to-Right
    R,   // Right-to-Left
    AL,  // Arabic Letter (RTL)
    EN,  // European Number
    ES,  // European Separator
    ET,  // European Terminator
    AN,  // Arabic Number
    CS,  // Common Separator
    NSM, // Non-spacing Mark
    BN,  // Boundary Neutral
    B,   // Paragraph Separator
    S,   // Segment Separator
    WS,  // Whitespace
    ON,  // Other Neutral
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
            WritingMode::VerticalRl | WritingMode::VerticalLr
        )
    }

    pub fn should_justify(&self, line: &UnifiedLine) -> bool {
        // Don't justify last line unless JustifyAll
        self.justify_content != JustifyContent::None
            && (line.is_last == false || self.text_align == TextAlign::JustifyAll)
    }
}

impl ShapedItem {
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

impl BidiClass {
    fn is_strong(&self) -> bool {
        matches!(self, BidiClass::L | BidiClass::R | BidiClass::AL)
    }

    fn is_rtl(&self) -> bool {
        matches!(self, BidiClass::R | BidiClass::AL)
    }

    fn is_ltr(&self) -> bool {
        matches!(self, BidiClass::L)
    }
}

/// Determines the bidirectional character type of a Unicode code point
fn get_bidi_class(c: char) -> BidiClass {
    let code = c as u32;

    // Arabic Letter (AL) range
    if (0x0600..=0x06FF).contains(&code) ||
       (0x0750..=0x077F).contains(&code) ||  // Arabic Supplement
       (0x08A0..=0x08FF).contains(&code) ||  // Arabic Extended-A
       (0xFB50..=0xFDFF).contains(&code) ||  // Arabic Presentation Forms-A
       (0xFE70..=0xFEFF).contains(&code) ||  // Arabic Presentation Forms-B
       (0x1EE00..=0x1EEFF).contains(&code)
    {
        // Arabic Mathematical Alphabetic Symbols
        return BidiClass::AL;
    }

    // Right-to-Left (R) ranges
    if (0x0591..=0x07FF).contains(&code) ||  // Hebrew, Arabic
       (0xFB1D..=0xFB4F).contains(&code) ||  // Hebrew Presentation Forms
       (0x10800..=0x10FFF).contains(&code) || // Ancient scripts
       (0x1E800..=0x1EFFF).contains(&code)
    {
        // Mende Kikakui, etc.
        return BidiClass::R;
    }

    // Left-to-Right (L) ranges (simplified)
    if (0x0041..=0x007A).contains(&code) ||  // Basic Latin letters
       (0x00C0..=0x02AF).contains(&code) ||  // Latin-1 Supplement, Latin Extended-A/B
       (0x0300..=0x036F).contains(&code) ||  // Combining Diacritical Marks
       (0x0370..=0x03FF).contains(&code) ||  // Greek
       (0x0400..=0x04FF).contains(&code) ||  // Cyrillic
       (0x2000..=0x206F).contains(&code) ||  // General Punctuation (mostly LTR)
       (0x3000..=0x30FF).contains(&code)
    {
        // CJK Symbols and Punctuation
        return BidiClass::L;
    }

    // European Number (EN)
    if (0x0030..=0x0039).contains(&code) {
        // ASCII digits
        return BidiClass::EN;
    }

    // Whitespace (WS)
    if matches!(
        code,
        0x0020 | 0x00A0 | 0x2000..=0x200B | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000
    ) {
        return BidiClass::WS;
    }

    // European Separator (ES)
    if matches!(
        code,
        0x002B
            | 0x002D
            | 0x002F
            | 0x003A
            | 0x003B
            | 0x003C
            | 0x003D
            | 0x003E
            | 0x003F
            | 0x0040
            | 0x005C
            | 0x005E
            | 0x005F
            | 0x0060
            | 0x007B
            | 0x007C
            | 0x007D
            | 0x007E
    ) {
        return BidiClass::ES;
    }

    // Other Neutral (ON) is the default if we don't match above
    BidiClass::ON
}

/// Detects the base direction of a text string according to the Unicode Bidirectional Algorithm
///
/// Returns:
///
/// - `Direction::LTR` if the text is predominantly left-to-right
/// - `Direction::RTL` if the text is predominantly right-to-left
/// - `Direction::Neutral` if there are no strong directional characters or if counts are equal with
///   no strong first character
fn detect_base_direction(text: &str) -> Direction {
    let bidi_info = BidiInfo::new(text, None);
    let para = &bidi_info.paragraphs[0];
    if para.level.is_rtl() {
        Direction::Rtl
    } else {
        Direction::Ltr
    }
}

fn perform_bidi_analysis<'a>(
    styled_runs: &'a [StyledRun],
    full_text: &'a str,
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
        let start = run.logical_start_byte;
        let end = start + run.text.len();
        for i in start..end {
            byte_to_run_index[i] = run_idx;
        }
    }

    // TODO: bidi_info.paragraphs.len() - correct?
    let mut final_visual_runs = Vec::new();
    let (levels, visual_run_ranges) = bidi_info.visual_runs(para, (0..bidi_info.paragraphs.len()));

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
                    style: &styled_runs[original_run_idx].style,
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
            style: &styled_runs[original_run_idx].style,
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

fn shape_visual_runs(
    visual_runs: &[VisualRun],
    font_provider: &impl FontProvider,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
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
            let source_char = run.text_slice[shaped_in_run.logical_byte_start..]
                .chars()
                .next()
                .unwrap_or('\0');

            let is_whitespace = source_char.is_whitespace();
            // A soft hyphen is a break opportunity but not whitespace.
            let is_soft_hyphen = source_char == '\u{00AD}';

            all_shaped_glyphs.push(ShapedGlyph {
                glyph_id: shaped_in_run.glyph_id,
                style: run.style.clone(),
                advance: shaped_in_run.advance,
                x_offset: shaped_in_run.x_offset,
                y_offset: shaped_in_run.y_offset,
                // Make cluster and byte start absolute to the entire paragraph.
                logical_byte_start: shaped_in_run.logical_byte_start + run.logical_start_byte,
                logical_byte_len: shaped_in_run.logical_byte_len,
                cluster: shaped_in_run.cluster + run.logical_start_byte as u32,
                source: GlyphSource::Char,
                is_whitespace,
                break_opportunity_after: is_whitespace || is_soft_hyphen,
            });
        }
    }

    Ok(all_shaped_glyphs)
}

fn position_glyphs(
    mut shaped_glyphs: Vec<ShapedGlyph>,
    constraints: LayoutConstraints,
    source_text: &str,
    base_direction: Direction,
    font_provider: &impl FontProvider,
) -> Result<ParagraphLayout, LayoutError> {
    let hyphenator = get_hyphenator()?;
    let mut positioned_glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut line_y = 0.0;
    let mut glyph_cursor = 0;

    // The main line breaking loop.
    while glyph_cursor < shaped_glyphs.len() {
        let (line_end_idx, needs_hyphen) = find_line_break(
            &shaped_glyphs,
            glyph_cursor,
            line_y,
            &constraints,
            &hyphenator,
            source_text,
        );

        let mut glyphs_for_this_line = shaped_glyphs[glyph_cursor..line_end_idx].to_vec();

        // Handle hyphen insertion.
        let hyphen_glyph = if needs_hyphen {
            // Get the style from the last character of the word being broken.
            let style = glyphs_for_this_line.last().unwrap().style.clone();
            let font = font_provider.load_font(&style.font_ref)?;
            let (glyph_id, advance) = font.get_hyphen_glyph_and_advance(style.font_size_px);

            Some(ShapedGlyph {
                glyph_id,
                style,
                advance,
                x_offset: 0.0,
                y_offset: 0.0,
                logical_byte_start: 0, // No direct mapping to source
                logical_byte_len: 0,
                cluster: 0,
                source: GlyphSource::Hyphen,
                is_whitespace: false,
                break_opportunity_after: false,
            })
        } else {
            None
        };

        if let Some(hyphen) = hyphen_glyph.as_ref() {
            glyphs_for_this_line.push(hyphen.clone());
        }

        // Finalize the line's geometry, applying alignment and justification.
        let (mut finalized_line_glyphs, line_layout) = finalize_line(
            &glyphs_for_this_line,
            glyph_cursor,
            line_y,
            constraints.available_width, // TODO: Use per-line width with floats
            TextAlign::Justify,          // TODO: Get from style
            base_direction,
            needs_hyphen,
        );

        positioned_glyphs.append(&mut finalized_line_glyphs);
        lines.push(line_layout.clone());

        line_y += line_layout.bounds.height;
        glyph_cursor = line_end_idx;
    }

    let content_size = calculate_content_size(&lines);

    Ok(ParagraphLayout {
        glyphs: positioned_glyphs,
        lines,
        content_size,
        source_text: source_text.to_string(),
        base_direction,
    })
}

/// Finds the index in `shaped_glyphs` where the current line should break.
/// Returns `(break_index, needs_hyphen)`.
fn find_line_break(
    glyphs: &[ShapedGlyph],
    start_idx: usize,
    line_y: f32,
    constraints: &LayoutConstraints,
    hyphenator: &Standard,
    source_text: &str,
) -> (usize, bool) {
    let available_width = constraints.available_width; // TODO: handle floats
    let mut current_width = 0.0;
    let mut last_opportunity = start_idx;

    for i in start_idx..glyphs.len() {
        let glyph = &glyphs[i];

        // Don't start a line with whitespace.
        if i == start_idx && glyph.is_whitespace {
            continue;
        }

        if current_width + glyph.advance > available_width {
            // We have overflowed. Break at the last known opportunity.
            if last_opportunity > start_idx {
                return (last_opportunity, false);
            }

            // No break opportunity found (long word). We must hyphenate or force break.
            let (word_start, word_end) = find_word_boundaries(glyphs, i);
            let word_text = &source_text[glyphs[word_start].logical_byte_start
                ..glyphs[word_end - 1].logical_byte_start
                    + glyphs[word_end - 1].logical_byte_len as usize];

            let breaks = hyphenator.opportunities(word_text);

            let mut best_hyphen_break = start_idx;
            let mut hyphen_width = 0.0;

            // Find the last possible hyphenation point that fits.
            for &break_byte_offset in breaks.iter().rev() {
                let mut width_at_hyphen = 0.0;
                let mut glyph_idx_at_hyphen = word_start;

                for j in word_start..word_end {
                    width_at_hyphen += glyphs[j].advance;
                    // Check if the glyph's logical end corresponds to the hyphen break point.
                    let logical_break_pos =
                        glyphs[word_start].logical_byte_start + break_byte_offset;
                    if glyphs[j].logical_byte_start + glyphs[j].logical_byte_len as usize
                        == logical_break_pos
                    {
                        glyph_idx_at_hyphen = j + 1;
                        break;
                    }
                }

                if width_at_hyphen < available_width {
                    best_hyphen_break = glyph_idx_at_hyphen;
                    break;
                }
            }

            if best_hyphen_break > start_idx {
                return (best_hyphen_break, true);
            }

            // Cannot hyphenate, force break before the overflowing glyph.
            return (i.max(start_idx + 1), false);
        }

        current_width += glyph.advance;

        if glyph.break_opportunity_after {
            last_opportunity = i + 1;
        }
    }

    // Reached the end of the text.
    (glyphs.len(), false)
}

// --- HELPER FUNCTIONS FOR POSITIONING ---

fn find_word_boundaries(glyphs: &[ShapedGlyph], current_idx: usize) -> (usize, usize) {
    let mut start = current_idx;
    while start > 0 && !glyphs[start - 1].is_whitespace {
        start -= 1;
    }
    let mut end = current_idx;
    while end < glyphs.len() && !glyphs[end].is_whitespace {
        end += 1;
    }
    (start, end)
}

fn get_available_width_for_line(line_y: f32, constraints: &LayoutConstraints) -> f32 {
    // TODO: This is a simplified check. A full implementation would handle multiple floats.
    let mut available = constraints.available_width;
    for exclusion in &constraints.exclusion_areas {
        if line_y >= exclusion.rect.y && line_y < exclusion.rect.y + exclusion.rect.height {
            available -= exclusion.rect.width;
        }
    }
    available.max(0.0)
}

// Font fallback using unicode ranges
fn shape_run_with_smart_fallback<T: ParsedFontTrait>(
    run: &VisualRun,
    font_manager: &mut FontManager,
    direction: Direction,
) -> Result<Vec<EnhancedGlyph<T>>, LayoutError> {
    let mut result = Vec::new();

    // Query fontconfig for fonts that support this text
    let pattern = FcPattern {
        name: Some(run.style.font_ref.family.clone()),
        weight: weight_to_fc_weight(run.style.font_ref.weight),
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
        ..Default::default()
    };

    let mut trace = Vec::new();
    let font_matches = font_manager
        .fc_cache
        .query_for_text(&pattern, run.text_slice, &mut trace);

    if font_matches.is_empty() {
        return Err(LayoutError::FontNotFound(run.style.font_ref.clone()));
    }

    // Group text by font coverage
    let segments =
        segment_text_by_font_coverage(run.text_slice, &font_matches, &font_manager.fc_cache)?;

    for segment in segments {
        let font = font_manager.load_font_by_id(&segment.font_id)?;
        let shaped = font.shape_text(segment.text, run.script, run.language, direction)?;

        for glyph in shaped {
            result.push(enhance_glyph(
                glyph,
                run,
                segment.byte_offset,
                font.clone(),
                run.style.clone(),
            )?);
        }
    }

    Ok(result)
}

struct TextSegment<'a> {
    text: &'a str,
    font_id: FontId,
    byte_offset: usize,
}

fn segment_text_by_font_coverage<'a>(
    text: &'a str,
    font_matches: &[FontMatch],
    fc_cache: &FcFontCache,
) -> Result<Vec<TextSegment<'a>>, LayoutError> {
    let mut segments = Vec::new();
    let mut char_indices = text.char_indices().peekable();

    while let Some((byte_idx, ch)) = char_indices.next() {
        // Find best font for this character
        let codepoint = ch as u32;
        let font_id = find_font_for_codepoint(codepoint, font_matches)?;

        // Collect consecutive chars that use the same font
        let mut segment_end = byte_idx + ch.len_utf8();
        while let Some(&(next_idx, next_ch)) = char_indices.peek() {
            let next_codepoint = next_ch as u32;
            let next_font = find_font_for_codepoint(next_codepoint, font_matches)?;

            if next_font == font_id {
                char_indices.next();
                segment_end = next_idx + next_ch.len_utf8();
            } else {
                break;
            }
        }

        segments.push(TextSegment {
            text: &text[byte_idx..segment_end],
            font_id,
            byte_offset: byte_idx,
        });
    }

    Ok(segments)
}

fn find_font_for_codepoint(
    codepoint: u32,
    font_matches: &[FontMatch],
) -> Result<FontId, LayoutError> {
    // Check primary font first
    if let Some(primary) = font_matches.first() {
        for range in &primary.unicode_ranges {
            if codepoint >= range.start && codepoint <= range.end {
                return Ok(primary.id.clone());
            }
        }
    }

    // Check fallbacks
    for font_match in font_matches {
        for range in &font_match.unicode_ranges {
            if codepoint >= range.start && codepoint <= range.end {
                return Ok(font_match.id.clone());
            }
        }
    }

    // Use first font as last resort
    font_matches.first().map(|m| m.id.clone()).ok_or_else(|| {
        LayoutError::FontNotFound(FontRef {
            family: "fallback".to_string(),
            weight: 400,
            style: FontStyle::Normal,
        })
    })
}

fn calculate_line_metrics(glyphs: &[ShapedGlyph]) -> LineMetrics {
    let font_size = glyphs.first().map_or(16.0, |g| g.style.font_size_px);
    let line_height = font_size * 1.4; // Common default
    let ascent = font_size;
    LineMetrics {
        ascent,
        descent: line_height - ascent,
        line_gap: 0.0,
        total_height: line_height,
    }
}

fn finalize_line(
    glyphs_on_line: &[ShapedGlyph],
    line_start_visual_index: usize,
    line_y: f32,
    available_width: f32,
    align: TextAlign,
    base_direction: Direction,
    is_hyphenated: bool,
) -> (Vec<PositionedGlyph>, LineLayout) {
    if glyphs_on_line.is_empty() {
        // Handle empty lines (e.g., from double newlines).
        let height = 16.0 * 1.2; // A default line height.
        return (
            vec![],
            LineLayout {
                bounds: Rect {
                    x: 0.0,
                    y: line_y,
                    width: 0.0,
                    height,
                },
                baseline_y: line_y + 16.0,
                glyph_start: 0,
                glyph_count: 0,
                logical_start_byte: 0,
                logical_end_byte: 0,
            },
        );
    }

    let metrics = calculate_line_metrics(glyphs_on_line);
    let line_width: f32 = glyphs_on_line.iter().map(|g| g.advance).sum();

    let mut start_x = 0.0;
    let mut space_expansion = 0.0;
    let logical_align = resolve_logical_align(align, base_direction);

    // Don't justify the last line of a paragraph or a hyphenated line.
    let is_last_line = false; // TODO: This requires lookahead, omitted for now.
    if logical_align == TextAlign::Justify && !is_last_line && !is_hyphenated {
        let space_count = glyphs_on_line.iter().filter(|g| g.is_whitespace).count();
        if space_count > 0 {
            space_expansion = (available_width - line_width) / space_count as f32;
        }
    } else {
        match logical_align {
            TextAlign::Center => start_x = (available_width - line_width) / 2.0,
            TextAlign::Right => start_x = available_width - line_width,
            _ => {} // Left is default.
        }
    }

    let mut current_x = start_x;
    let baseline_y = line_y + metrics.ascent;
    let mut positioned_glyphs = Vec::new();

    for (i, glyph) in glyphs_on_line.iter().enumerate() {
        positioned_glyphs.push(PositionedGlyph {
            glyph_id: glyph.glyph_id,
            style: glyph.style.clone(),
            x: current_x + glyph.x_offset,
            y: baseline_y - glyph.y_offset,
            bounds: Rect {
                x: current_x,
                y: line_y,
                width: glyph.advance,
                height: metrics.total_height,
            },
            advance: glyph.advance,
            line_index: 0, // TODO: This can be set in a final pass.
            logical_char_byte_index: glyph.logical_byte_start,
            logical_char_byte_count: glyph.logical_byte_len,
            visual_index: line_start_visual_index + i,
            bidi_level: BidiLevel::new(0), // TODO: Propagate from VisualRun.
        });
        current_x += glyph.advance;
        if glyph.is_whitespace {
            current_x += space_expansion;
        }
    }

    let (logical_start, logical_end) = glyphs_on_line
        .iter()
        .filter(|g| g.source == GlyphSource::Char)
        .fold((usize::MAX, 0), |(min, max), g| {
            (
                min.min(g.logical_byte_start),
                max.max(g.logical_byte_start + g.logical_byte_len as usize),
            )
        });

    let line_layout = LineLayout {
        bounds: Rect {
            x: start_x,
            y: line_y,
            width: current_x - start_x,
            height: metrics.total_height,
        },
        baseline_y,
        glyph_start: line_start_visual_index,
        glyph_count: glyphs_on_line.len(),
        logical_start_byte: if logical_start == usize::MAX {
            0
        } else {
            logical_start
        },
        logical_end_byte: logical_end,
    };

    (positioned_glyphs, line_layout)
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

fn calculate_content_size(lines: &[LineLayout]) -> Size {
    let max_width = lines
        .iter()
        .map(|line| line.bounds.x + line.bounds.width)
        .fold(0.0f32, f32::max);

    let total_height = lines
        .last()
        .map(|line| line.bounds.y + line.bounds.height)
        .unwrap_or(0.0);

    Size {
        width: max_width,
        height: total_height,
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
impl FontProviderTrait for FontManager {
    fn load_font<T: ParsedFontTrait>(&self, font_ref: &FontRef) -> Result<Arc<T>, LayoutError> {
        // Check cache first
        if let Some(cached_id) = self.font_ref_to_id_cache.get(font_ref) {
            if let Some(font) = self.parsed_fonts.get(cached_id) {
                return Ok(font.clone());
            }
        }

        // Query fontconfig
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            weight: weight_to_fc_weight(font_ref.weight),
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
        if !self.parsed_fonts.contains_key(&fc_match.id) {
            let font_bytes = self
                .fc_cache
                .get_font_bytes(&fc_match.id)
                .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

            let parsed = self
                .font_loader
                .load_font(&font_bytes.data, font_bytes.font_index)?;

            self.parsed_fonts
                .insert(fc_match.id.clone(), Arc::from(parsed));
            self.font_ref_to_id_cache
                .insert(font_ref.clone(), fc_match.id.clone());
        }

        Ok(self.parsed_fonts.get(&fc_match.id).unwrap().clone())
    }

    fn get_fallback_chain(&self, font_ref: &FontRef, script: Script) -> Vec<FontRef> {
        // This is handled by query_for_text now
        Vec::new()
    }
}

impl FontManager {
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts: HashMap::new(),
            fallback_chains: HashMap::new(),
            font_loader: Arc::new(SystemFontLoader::new()),
        })
    }

    pub fn with_loader(
        fc_cache: FcFontCache,
        loader: Arc<dyn FontLoaderTrait>,
    ) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache,
            parsed_fonts: HashMap::new(),
            fallback_chains: HashMap::new(),
            font_loader: loader,
        })
    }

    // Build fallback chain for a given font request and text content
    pub fn build_fallback_chain(
        &mut self,
        font_ref: &FontRef,
        text: &str,
    ) -> Result<FontFallbackChain, LayoutError> {
        if let Some(cached) = self.fallback_chains.get(font_ref) {
            return Ok(cached.clone());
        }

        let mut trace = Vec::new();

        // First try exact match
        let pattern = FcPattern {
            name: Some(font_ref.family.clone()),
            weight: weight_to_fc_weight(font_ref.weight),
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

        let primary_match = self
            .fc_cache
            .query(&pattern, &mut trace)
            .ok_or_else(|| LayoutError::FontNotFound(font_ref.clone()))?;

        // Then find fallbacks for the specific text
        let fallback_matches = self.fc_cache.query_for_text(&pattern, text, &mut trace);

        // Convert to our FontRef format and filter out duplicates
        let mut fallbacks = Vec::new();
        let mut script_specific = HashMap::new();

        for fc_match in fallback_matches {
            let fallback_ref = self.fc_match_to_font_ref(&fc_match)?;
            if fallback_ref != *font_ref && !fallbacks.contains(&fallback_ref) {
                fallbacks.push(fallback_ref.clone());

                // Group by script for efficient lookup
                for &script in &fc_match.scripts {
                    script_specific
                        .entry(Script(script))
                        .or_insert_with(Vec::new)
                        .push(fallback_ref.clone());
                }
            }
        }

        let chain = FontFallbackChain {
            primary: font_ref.clone(),
            fallbacks,
            script_specific,
        };

        self.fallback_chains.insert(font_ref.clone(), chain.clone());
        Ok(chain)
    }

    pub fn get_font_for_text(
        &mut self,
        font_ref: &FontRef,
        text: &str,
        script: Script,
    ) -> Result<Arc<ParsedFont>, LayoutError> {
        // Try primary font first
        if let Ok(font) = self.load_font(font_ref) {
            if self.font_supports_text(&font, text) {
                return Ok(font);
            }
        }

        // Build fallback chain if needed
        let chain = self.build_fallback_chain(font_ref, text)?;

        // Try script-specific fallbacks first
        if let Some(script_fonts) = chain.script_specific.get(&script) {
            for fallback_ref in script_fonts {
                if let Ok(font) = self.load_font(fallback_ref) {
                    if self.font_supports_text(&font, text) {
                        return Ok(font);
                    }
                }
            }
        }

        // Try general fallbacks
        for fallback_ref in &chain.fallbacks {
            if let Ok(font) = self.load_font(fallback_ref) {
                if self.font_supports_text(&font, text) {
                    return Ok(font);
                }
            }
        }

        Err(LayoutError::FontNotFound(font_ref.clone()))
    }

    fn font_supports_text(&self, font: &ParsedFont, text: &str) -> bool {
        // Quick check using cmap table
        // TODO: rust-fontconfig has functions for this, i.e. unicode-range check, not
        // necessary to check whether the char actually exists on the font
        text.chars().all(|c| font.has_glyph(c as u32))
    }

    fn fc_match_to_font_ref(&self, fc_match: &FontMatch) -> Result<FontRef, LayoutError> {
        // Convert rust-fontconfig match to our FontRef
        let font_path = self.fc_cache.get_font_path(&fc_match.id).ok_or_else(|| {
            LayoutError::FontNotFound(FontRef {
                family: "unknown".to_string(),
                weight: 400,
                style: FontStyle::Normal,
            })
        })?;

        // Extract family name from the font file
        let family = self.extract_family_name(&font_path.path)?;

        Ok(FontRef {
            family,
            weight: fc_match.weight.unwrap_or(400),
            style: fc_slant_to_style(fc_match.slant.unwrap_or(0)),
        })
    }

    fn extract_family_name(&self, font_path: &str) -> Result<String, LayoutError> {
        // TODO: This would parse the font file to get the actual family name
        // For now, use a simplified approach
        std::path::Path::new(font_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| LayoutError::InvalidText("Cannot extract font family".to_string()))
    }
}

// Enhanced shaping that handles font fallback within runs
fn shape_visual_runs_with_fallback(
    visual_runs: &[VisualRun],
    font_manager: &mut FontManager,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
    let mut all_shaped_glyphs = Vec::new();

    for run in visual_runs {
        let direction = if run.bidi_level.is_rtl() {
            Direction::Rtl
        } else {
            Direction::Ltr
        };

        // Shape with fallback - this is the key enhancement
        let shaped_glyphs = shape_run_with_fallback(run, font_manager, direction)?;

        if direction == Direction::Rtl {
            // Note: Only reverse the glyphs, not the entire vec structure
            let mut reversed_glyphs = shaped_glyphs;
            reversed_glyphs.reverse();
            all_shaped_glyphs.extend(reversed_glyphs);
        } else {
            all_shaped_glyphs.extend(shaped_glyphs);
        }
    }

    Ok(all_shaped_glyphs)
}

fn shape_run_with_fallback(
    run: &VisualRun,
    font_manager: &mut FontManager,
    direction: Direction,
) -> Result<Vec<ShapedGlyph>, LayoutError> {
    let mut result = Vec::new();
    let mut char_indices = run.text_slice.char_indices().peekable();

    while let Some((byte_offset, ch)) = char_indices.next() {
        // Collect a sequence of characters that can be shaped together
        let mut segment_end = byte_offset + ch.len_utf8();
        let mut segment_chars = vec![ch];

        // Look ahead to group characters that likely use the same font
        while let Some(&(next_byte_offset, next_ch)) = char_indices.peek() {
            if should_group_chars(ch, next_ch, run.script) {
                segment_chars.push(next_ch);
                char_indices.next();
                segment_end = next_byte_offset + next_ch.len_utf8();
            } else {
                break;
            }
        }

        let segment_text = &run.text_slice[byte_offset..segment_end];

        // Find appropriate font for this segment
        let font = font_manager.get_font_for_text(&run.style.font_ref, segment_text, run.script)?;
        // Shape the segment
        let mut shaped_segment =
            font.shape_text(segment_text, run.script, run.language, direction)?;

        // Adjust byte indices to be relative to the full run
        for glyph in &mut shaped_segment {
            glyph.logical_byte_start += run.logical_start_byte + byte_offset;
            glyph.cluster += (run.logical_start_byte + byte_offset) as u32;
        }

        result.extend(shaped_segment);
    }

    Ok(result)
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

// Helper conversion functions
fn weight_to_fc_weight(weight: u16) -> FcWeight {
    if weight < 150 {
        FcWeight::Thin
    } else if weight < 250 {
        FcWeight::ExtraLight
    } else if weight < 350 {
        FcWeight::Light
    } else if weight < 450 {
        FcWeight::Normal
    } else if weight < 550 {
        FcWeight::Medium
    } else if weight < 650 {
        FcWeight::SemiBold
    } else if weight < 750 {
        FcWeight::Bold
    } else if weight < 850 {
        FcWeight::ExtraBold
    } else {
        FcWeight::Black
    }
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

fn justify_line(
    glyphs: &mut [ShapedGlyph],
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
fn find_line_break_with_graphemes(
    glyphs: &[ShapedGlyph],
    start_idx: usize,
    line_y: f32,
    constraints: &UnifiedConstraints,
    hyphenator: &Standard,
    source_text: &str,
) -> (usize, bool) {
    let available_width = get_available_width_for_line(line_y, constraints);
    let mut current_width = 0.0;
    let mut last_opportunity = start_idx;
    let mut last_grapheme_boundary = start_idx;

    // Build grapheme cluster map
    let grapheme_boundaries = get_grapheme_boundaries(source_text, glyphs, start_idx);

    for i in start_idx..glyphs.len() {
        let glyph = &glyphs[i];

        // Skip leading whitespace
        if i == start_idx && glyph.is_whitespace {
            continue;
        }

        // Check if this is a grapheme boundary
        if grapheme_boundaries.contains(&i) {
            last_grapheme_boundary = i;
        }

        if current_width + glyph.advance > available_width {
            // Must break at grapheme boundary
            if last_opportunity > start_idx && grapheme_boundaries.contains(&last_opportunity) {
                return (last_opportunity, false);
            }

            // Try hyphenation at grapheme boundaries
            let (word_start, word_end) =
                find_word_boundaries_grapheme_aware(glyphs, i, &grapheme_boundaries);

            if let Some(hyphen_break) = try_hyphenate_word(
                glyphs,
                word_start,
                word_end,
                source_text,
                hyphenator,
                available_width,
                &grapheme_boundaries,
            ) {
                return (hyphen_break, true);
            }

            // Force break at last grapheme boundary
            return (last_grapheme_boundary.max(start_idx + 1), false);
        }

        current_width += glyph.advance;

        if glyph.break_opportunity_after && grapheme_boundaries.contains(&(i + 1)) {
            last_opportunity = i + 1;
        }
    }

    (glyphs.len(), false)
}

fn get_grapheme_boundaries(
    text: &str,
    glyphs: &[ShapedGlyph],
    start_idx: usize,
) -> BTreeSet<usize> {
    let mut boundaries = BTreeSet::new();
    boundaries.insert(start_idx);

    let graphemes = text.graphemes(true);
    let mut byte_offset = 0;

    for grapheme in graphemes {
        byte_offset += grapheme.len();

        // Find corresponding glyph index
        for (idx, glyph) in glyphs.iter().enumerate() {
            if glyph.logical_byte_start == byte_offset {
                boundaries.insert(idx);
                break;
            }
        }
    }

    boundaries.insert(glyphs.len());
    boundaries
}

// Enhanced bidi line reordering
fn position_content_with_bidi_reordering(
    lines: Vec<UnifiedLine>,
    constraints: &UnifiedConstraints,
    base_direction: Direction,
) -> Result<UnifiedLayout, LayoutError> {
    let mut positioned_items = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        // Group items by bidi level for proper reordering
        let reordered_items = reorder_line_bidi(&line.items, base_direction)?;

        // Apply justification after reordering
        let justified_items = if constraints.should_justify(line) {
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

        // Position items
        let mut inline_position = calculate_alignment_offset(
            &justified_items,
            &line.constraints,
            constraints.text_align,
            base_direction,
        );

        for item in justified_items {
            let positioned = position_item_with_bidi(
                item,
                Point {
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
                constraints,
                base_direction,
            )?;

            inline_position += get_item_advance(&positioned, constraints);
            positioned_items.push(positioned);
        }
    }

    Ok(UnifiedLayout {
        items: positioned_items,
        bounds: calculate_bounds(&positioned_items),
        overflow: OverflowInfo::default(),
    })
}

fn reorder_line_bidi(
    items: &[ShapedItem],
    base_direction: Direction,
) -> Result<Vec<ShapedItem>, LayoutError> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    // Group consecutive items by bidi level
    let mut runs = Vec::new();
    let mut current_run = Vec::new();
    let mut current_level = None;

    for item in items {
        let item_level = get_item_bidi_level(item);

        if current_level != Some(item_level) {
            if !current_run.is_empty() {
                runs.push((current_level.unwrap(), current_run));
                current_run = Vec::new();
            }
            current_level = Some(item_level);
        }
        current_run.push(item.clone());
    }

    if !current_run.is_empty() {
        runs.push((current_level.unwrap(), current_run));
    }

    // Reorder runs according to bidi algorithm
    reorder_bidi_runs(&mut runs, base_direction);

    // Flatten reordered runs
    Ok(runs.into_iter().flat_map(|(_, items)| items).collect())
}

fn reorder_bidi_runs(runs: &mut Vec<(BidiLevel, Vec<ShapedItem>)>, base_direction: Direction) {
    if runs.is_empty() {
        return;
    }

    // Find max level
    let max_level = runs
        .iter()
        .map(|(level, _)| level.level())
        .max()
        .unwrap_or(0);

    // Reverse runs at each level from max to 1
    for level in (1..=max_level).rev() {
        let mut i = 0;
        while i < runs.len() {
            // Find sequence of runs at or above current level
            if runs[i].0.level() >= level {
                let start = i;
                while i < runs.len() && runs[i].0.level() >= level {
                    i += 1;
                }
                // Reverse the sequence
                runs[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
}

fn calculate_line_width(glyphs: &[ShapedGlyph], writing_mode: WritingMode) -> f32 {
    glyphs
        .iter()
        .map(|g| get_glyph_advance(g, writing_mode))
        .sum()
}

fn get_glyph_advance(glyph: &ShapedGlyph, writing_mode: WritingMode) -> f32 {
    match writing_mode {
        WritingMode::HorizontalTb => glyph.advance,
        WritingMode::VerticalRl | WritingMode::VerticalLr => glyph.vertical_advance,
        WritingMode::SidewaysRl | WritingMode::SidewaysLr => glyph.advance,
    }
}

fn justify_inter_word(glyphs: &mut [ShapedGlyph], available_space: f32) -> Result<(), LayoutError> {
    // Find all word boundaries (spaces and break opportunities)
    let space_indices: Vec<usize> = glyphs
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            if g.character_class == CharacterClass::Space
                || (g.break_opportunity_after && g.character_class != CharacterClass::Combining)
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

fn justify_inter_character(
    glyphs: &mut [ShapedGlyph],
    available_space: f32,
) -> Result<(), LayoutError> {
    // For CJK text - expand space between all characters
    let justifiable_gaps: Vec<usize> = glyphs
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            if g.can_justify
                && g.character_class != CharacterClass::Combining
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

fn justify_distribute(glyphs: &mut [ShapedGlyph], available_space: f32) -> Result<(), LayoutError> {
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

fn apply_vertical_metrics<T: ParsedFontTrait>(glyph: &mut ShapedGlyph, font: &T) {
    // Get vertical metrics from VMTX, VORG tables
    if let Some(v_metrics) = font.get_vertical_metrics(glyph.glyph_id) {
        glyph.vertical_advance = v_metrics.advance;
        glyph.vertical_x_offset = v_metrics.bearing_x;
        glyph.vertical_y_offset = v_metrics.bearing_y;
        glyph.vertical_origin_y = v_metrics.origin_y;
    } else {
        // Fallback: derive from horizontal metrics
        glyph.vertical_advance = glyph.style.line_height;
        glyph.vertical_x_offset = -glyph.advance / 2.0;
        glyph.vertical_y_offset = 0.0;
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
    pub fn layout(
        content: Vec<InlineContent>,
        constraints: UnifiedConstraints,
        font_manager: &mut FontManager,
        cache: &mut LayoutCache,
    ) -> Result<Arc<UnifiedLayout>, LayoutError> {
        // Check cache first
        let cache_key = CacheKey::new(&content, &constraints);

        if let Some(cached) = cache.get(&cache_key) {
            return Ok(cached);
        }

        // Stage 1: Content analysis and preparation
        let analyzed_content = Self::analyze_content(&content, &constraints)?;

        // Stage 2: Bidi analysis if text content exists
        let bidi_analyzed = Self::apply_bidi_analysis(analyzed_content, &constraints)?;

        // Stage 3: Shape all content with font fallback
        let shaped_content = Self::shape_content(bidi_analyzed, font_manager, &constraints)?;

        // Stage 4: Apply vertical text transformations if needed
        let oriented_content = Self::apply_text_orientation(shaped_content, &constraints)?;

        // Stage 5: Line breaking with shape awareness
        let lines = Self::break_lines(oriented_content, &constraints, font_manager)?;

        // Stage 6: Position content with justification
        let positioned = Self::position_content(lines, &constraints)?;

        // Stage 7: Apply overflow handling
        let final_layout = Self::handle_overflow(positioned, &constraints)?;

        let layout = Arc::new(final_layout);

        // Cache the result
        cache.put(cache_key, layout.clone());

        Ok(layout)
    }

    fn analyze_content(
        content: &[InlineContent],
        constraints: &UnifiedConstraints,
    ) -> Result<AnalyzedContent, LayoutError> {
        let mut text_runs = Vec::new();
        let mut non_text_items = Vec::new();
        let mut full_text = String::new();
        let mut byte_offset = 0;

        for (idx, item) in content.iter().enumerate() {
            match item {
                InlineContent::Text(run) => {
                    text_runs.push(TextRunInfo {
                        text: run.text.clone(),
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

        let base_direction = detect_base_direction(&full_text);

        Ok(AnalyzedContent {
            text_runs,
            non_text_items,
            full_text,
            base_direction,
        })
    }

    fn apply_bidi_analysis(
        content: AnalyzedContent,
        constraints: &UnifiedConstraints,
    ) -> Result<BidiAnalyzedContent, LayoutError> {
        if content.full_text.is_empty() {
            return Ok(BidiAnalyzedContent {
                visual_runs: Vec::new(),
                non_text_items: content.non_text_items,
                base_direction: content.base_direction,
            });
        }

        let (visual_runs, unified_direction) =
            perform_bidi_analysis(&content.text_runs, &content.full_text)?;

        Ok(BidiAnalyzedContent {
            visual_runs,
            non_text_items: content.non_text_items,
            base_direction: unified_direction,
        })
    }

    fn shape_content(
        content: BidiAnalyzedContent,
        font_manager: &mut FontManager,
        constraints: &UnifiedConstraints,
    ) -> Result<Vec<ShapedItem>, LayoutError> {
        let mut shaped_items = Vec::new();

        // Shape text runs with font fallback
        for run in content.visual_runs {
            let shaped_glyphs = shape_run_with_fallback(
                &run,
                font_manager,
                if run.bidi_level.is_rtl() {
                    Direction::Rtl
                } else {
                    Direction::Ltr
                },
            )?;

            for glyph in shaped_glyphs {
                shaped_items.push(ShapedItem::Glyph(enhance_glyph(glyph, &run, constraints)?));
            }
        }

        // Measure non-text items
        for (idx, item) in content.non_text_items {
            shaped_items.push(Self::measure_inline_item(item, constraints)?);
        }

        // Sort by content index to maintain order
        shaped_items.sort_by_key(|item| item.content_index());

        Ok(shaped_items)
    }

    fn apply_text_orientation(
        mut items: Vec<ShapedItem>,
        constraints: &UnifiedConstraints,
    ) -> Result<Vec<ShapedItem>, LayoutError> {
        if !constraints.is_vertical() {
            return Ok(items);
        }

        for item in &mut items {
            if let ShapedItem::Glyph(ref mut glyph) = item {
                // Determine orientation for this glyph
                let orientation = determine_glyph_orientation(
                    glyph.codepoint,
                    glyph.script,
                    constraints.text_orientation,
                    constraints.writing_mode,
                );

                glyph.orientation = orientation;

                // Apply vertical metrics
                if let Some(metrics) = glyph.font.get_vertical_metrics(glyph.glyph_id) {
                    glyph.vertical_advance = metrics.advance;
                    glyph.vertical_origin = Point {
                        x: metrics.bearing_x,
                        y: metrics.origin_y,
                    };
                } else {
                    // Synthesize vertical metrics
                    glyph.vertical_advance = glyph.style.line_height;
                    glyph.vertical_origin = Point {
                        x: -glyph.advance / 2.0,
                        y: glyph.style.font_size_px * 0.88,
                    };
                }
            }
        }

        Ok(items)
    }

    fn break_lines(
        items: Vec<ShapedItem>,
        constraints: &UnifiedConstraints,
        font_manager: &mut FontManager,
    ) -> Result<Vec<UnifiedLine>, LayoutError> {
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
            let (line_end, line_items) = Self::find_line_break(
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
            });

            current_position += constraints.line_height;
            item_cursor += line_end;
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
                constraints.writing_mode,
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

        Ok(LineConstraints {
            segments,
            total_available: segments.iter().map(|s| s.width).sum(),
        })
    }

    fn find_line_break(
        items: &[ShapedItem],
        line_constraints: &LineConstraints,
        constraints: &UnifiedConstraints,
        font_manager: &mut FontManager,
    ) -> Result<(usize, Vec<ShapedItem>), LayoutError> {
        // Advanced line breaking that handles:
        //
        // - Multiple segments per line
        // - Hyphenation
        // - Inline objects
        // - Vertical text

        let mut fitted_items = Vec::new();
        let mut current_segment = 0;
        let mut segment_position = 0.0;
        let mut item_cursor = 0;

        while item_cursor < items.len() && current_segment < line_constraints.segments.len() {
            let segment = &line_constraints.segments[current_segment];
            let remaining_width = segment.width - segment_position;

            let item_width = Self::get_item_measure(&items[item_cursor], constraints);

            if item_width <= remaining_width {
                // Item fits in current segment
                fitted_items.push(items[item_cursor].clone());
                segment_position += item_width;
                item_cursor += 1;
            } else if segment_position > 0.0 {
                // Try next segment
                current_segment += 1;
                segment_position = 0.0;
            } else {
                // Item doesn't fit, try hyphenation or break
                if let Some(hyphenated) =
                    Self::try_hyphenate(&items[item_cursor], remaining_width, font_manager)
                {
                    fitted_items.push(hyphenated);
                    item_cursor += 1;
                }
                break;
            }
        }

        Ok((item_cursor, fitted_items))
    }

    fn position_content(
        lines: Vec<UnifiedLine>,
        constraints: &UnifiedConstraints,
    ) -> Result<UnifiedLayout, LayoutError> {
        let mut positioned_items = Vec::new();

        for line in lines {
            // Apply justification if needed
            let justified_items = if constraints.should_justify(&line) {
                justify_line(
                    line.items,
                    &line.constraints,
                    constraints.justify_content,
                    constraints.writing_mode,
                )?
            } else {
                line.items
            };

            // Position items in line
            let mut inline_position = Self::calculate_alignment_offset(
                &justified_items,
                &line.constraints,
                constraints.text_align,
                constraints.writing_mode,
            );

            for item in justified_items {
                let positioned = Self::position_item(
                    item,
                    Point {
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
                    constraints,
                )?;

                inline_position += Self::get_item_advance(&positioned, constraints);
                positioned_items.push(positioned);
            }
        }

        Ok(UnifiedLayout {
            items: positioned_items,
            bounds: Self::calculate_bounds(&positioned_items),
            overflow: OverflowInfo::default(),
        })
    }

    fn handle_overflow(
        mut layout: UnifiedLayout,
        constraints: &UnifiedConstraints,
    ) -> Result<UnifiedLayout, LayoutError> {
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

    fn measure_inline_item(
        item: InlineContent,
        constraints: &UnifiedConstraints,
    ) -> Result<ShapedItem, LayoutError> {
        match item {
            InlineContent::Image(img) => {
                let size = img.display_size.unwrap_or(img.intrinsic_size);
                Ok(ShapedItem::Image(MeasuredImage {
                    source: img.source,
                    size,
                    baseline_offset: img.baseline_offset,
                    alignment: img.alignment,
                    content_index: 0,
                }))
            }
            InlineContent::Shape(shape) => Ok(ShapedItem::Shape(MeasuredShape {
                shape_def: shape.shape_def,
                size: shape.size,
                baseline_offset: shape.baseline_offset,
                content_index: 0,
            })),
            InlineContent::Space(space) => Ok(ShapedItem::Space(MeasuredSpace {
                width: space.width,
                content_index: 0,
            })),
            InlineContent::LineBreak(br) => Ok(ShapedItem::LineBreak(br)),
            _ => Err(LayoutError::InvalidText(
                "Unexpected inline content".to_string(),
            )),
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

    fn get_item_measure(item: &ShapedItem, constraints: &UnifiedConstraints) -> f32 {
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

    fn get_item_advance(item: &PositionedItem, constraints: &UnifiedConstraints) -> f32 {
        Self::get_item_measure(&item.item, constraints)
    }

    fn try_hyphenate(
        item: &ShapedItem,
        available_width: f32,
        font_manager: &mut FontManager,
    ) -> Option<ShapedItem> {
        // TODO: Simplified hyphenation - would use hyphenation library
        None
    }

    fn calculate_alignment_offset(
        items: &[ShapedItem],
        constraints: &LineConstraints,
        align: TextAlign,
        writing_mode: WritingMode,
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

    fn position_item(
        item: ShapedItem,
        position: Point,
        constraints: &UnifiedConstraints,
    ) -> Result<PositionedItem, LayoutError> {
        let bounds = match &item {
            ShapedItem::Glyph(g) => g.bounds.clone(),
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

    fn calculate_bounds(items: &[PositionedItem]) -> Rect {
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

    fn item_intersects_bounds(item: &PositionedItem, boundaries: &[ShapeBoundary]) -> bool {
        // TODO: Simplified - check if item center is within any boundary
        true
    }

    fn calculate_overflow(items: &[PositionedItem], boundaries: &[ShapeBoundary]) -> OverflowInfo {
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

// Helper function to enhance a basic shaped glyph
fn enhance_glyph<T: ParsedFontTrait>(
    glyph: ShapedGlyph,
    run: &VisualRun,
    constraints: &UnifiedConstraints,
    font: Arc<T>,
    style: Arc<StyleProperties>,
) -> Result<EnhancedGlyph<T>, LayoutError> {
    let codepoint = run.text_slice[glyph.logical_byte_start - run.logical_start_byte..]
        .chars()
        .next()
        .unwrap_or('\0') as u32;

    Ok(EnhancedGlyph {
        glyph_id: glyph.glyph_id,
        codepoint,
        font: font.clone(),
        style: style.clone(),
        advance: glyph.advance,
        offset: Point {
            x: glyph.x_offset,
            y: glyph.y_offset,
        },
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: glyph.advance,
            height: run.style.line_height,
        },
        vertical_advance: 0.0, // Set in apply_text_orientation
        vertical_origin: Point { x: 0.0, y: 0.0 },
        orientation: GlyphOrientation::Horizontal,
        logical_byte_index: glyph.logical_byte_start,
        logical_byte_len: glyph.logical_byte_len as usize,
        content_index: 0, // Would be set from content analysis
        script: run.script,
        bidi_level: run.bidi_level.clone(),
        character_class: classify_character(codepoint),
        can_justify: !glyph.is_whitespace,
        justification_priority: get_justification_priority(classify_character(codepoint)),
        break_opportunity_after: glyph.break_opportunity_after,
    })
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
pub struct LayoutCache {
    cache: Arc<Mutex<LruCache<CacheKey, Arc<UnifiedLayout>>>>,
}

impl LayoutCache {
    pub fn new(capacity: usize) -> Self {
        LayoutCache {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(if capacity == 0 { 1 } else { capacity }).unwrap(),
            ))),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<Arc<UnifiedLayout>> {
        self.cache.lock().unwrap().get(key).cloned()
    }

    pub fn put(&self, key: CacheKey, layout: Arc<UnifiedLayout>) {
        self.cache.lock().unwrap().put(key, layout);
    }
}

// Example: Render Mongolian text in a circle with fallback
pub fn render_mongolian_in_circle() -> Result<Arc<UnifiedLayout>, LayoutError> {
    let mut cache = LayoutCache::new(100);
    let mongolian_text = "ᠮᠣᠩ᠋ᠭᠤᠯ ᠤᠯᠤᠰ ᠤᠨ ᠶᠡᠷᠦᠩᠬᠡᠢᠢᠯᠡᠭᠴᠢ";

    let content = vec![InlineContent::Text(StyledRun {
        text: mongolian_text.to_string(),
        style: StyleProperties {
            font_ref: FontRef {
                family: "Mongolian Baiti".to_string(),
                weight: 400,
                style: FontStyle::Normal,
            },
            font_size_px: 16.0,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            letter_spacing: 0.0,
            word_spacing: 0.0,
            line_height: 20.0,
            text_decoration: TextDecoration::default(),
            font_features: vec![],
            writing_mode: WritingMode::VerticalLr, // Mongolian is vertical
            text_orientation: TextOrientation::Upright,
            text_combine_upright: None,
        },
        logical_start_byte: 0,
    })];

    let constraints = UnifiedConstraints {
        shape_boundaries: vec![ShapeBoundary::Circle {
            center: Point { x: 200.0, y: 200.0 },
            radius: 150.0,
        }],
        shape_exclusions: vec![
            // Inner circle to create a ring
            ShapeExclusion::Circle {
                center: Point { x: 200.0, y: 200.0 },
                radius: 50.0,
            },
        ],
        writing_mode: WritingMode::VerticalLr,
        text_orientation: TextOrientation::Upright,
        text_align: TextAlign::Justify,
        justify_content: JustifyContent::InterCharacter,
        line_height: 24.0,
        overflow: OverflowBehavior::Hidden,
        text_combine_upright: None,
        exclusion_margin: 2.0,
    };

    let fc_cache = FcFontCache::build(); // loads from system cache
    let mut font_manager = FontManager::new(fc_cache)?;

    // Build fallback chain for Mongolian script
    font_manager.build_fallback_chain(
        &FontRef {
            family: "Mongolian Baiti".to_string(),
            weight: 400,
            style: FontStyle::Normal,
        },
        mongolian_text,
    )?;

    UnifiedLayoutEngine::layout(content, constraints, &mut font_manager, &mut cache)
}
