//! Font traits that are always available, regardless of text_layout feature.
//!
//! These traits define the interface between the layout solver and the font system.
//! The actual implementations live in text3/cache.rs when text_layout feature is enabled.
//!
//! When `text_layout` + `font_loading` features are disabled, a stub module provides
//! minimal placeholder types so that downstream code can still reference these names.

use azul_core::geom::LogicalSize;

#[cfg(all(feature = "text_layout", feature = "font_loading"))]
pub use crate::text3::script::Language;
#[cfg(all(feature = "text_layout", feature = "font_loading"))]
pub use crate::text3::{
    cache::{
        AvailableSpace, BidiDirection, ContentIndex, FontHash, FontManager, FontSelector,
        FontStyle, Glyph, ImageSource, InlineContent, InlineImage, InlineShape, TextShapingCache,
        LayoutError, LayoutFontMetrics, LayoutFragment, ObjectFit, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, UnifiedConstraints,
        UnifiedLayout, VerticalMetrics,
    },
    script::Script,
};

/// Backwards-compat alias for the inner `TextShapingCache` type.
/// The real struct was renamed to disambiguate it from
/// [`crate::solver3::cache::LayoutCache`] (the per-node 9+1-slot
/// layout cache). Internal callers that read this name continue to
/// resolve via the alias; new code should use `TextShapingCache`.
#[cfg(all(feature = "text_layout", feature = "font_loading"))]
pub use crate::text3::cache::TextShapingCache as LayoutCache;

#[cfg(all(feature = "text_layout", feature = "font_loading"))]
pub type TextLayoutCache = TextShapingCache;

#[cfg(not(all(feature = "text_layout", feature = "font_loading")))]
pub use stub::TextLayoutCache;

/// Trait for types that support cheap, shallow cloning (e.g., reference-counted types).
pub trait ShallowClone {
    /// Create a shallow clone (increment reference count, don't copy data)
    #[must_use]
    fn shallow_clone(&self) -> Self;
}

/// Core trait for parsed fonts that can be used for text shaping and layout.
///
/// This trait abstracts over the actual font parsing implementation, allowing
/// the layout solver to work with different font backends.
pub trait ParsedFontTrait: Send + Clone + ShallowClone {
    /// Shape the given text into a sequence of glyphs using the font's shaping tables.
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError>;

    /// Hash of the font, necessary for breaking layouted glyphs into glyph runs
    fn get_hash(&self) -> u64;

    /// Returns the size of a glyph at the given font size, or `None` if the glyph is missing.
    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize>;

    /// Returns the glyph ID and horizontal advance of the hyphen character at the given font size.
    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)>;

    /// Returns the glyph ID and horizontal advance of the kashida (tatweel) character.
    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)>;

    /// Returns whether the font contains a glyph for the given Unicode codepoint.
    fn has_glyph(&self, codepoint: u32) -> bool;

    /// Returns vertical metrics (ascent, descent, line gap) for a specific glyph.
    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics>;

    /// Returns the global font metrics (ascent, descent, units per em, etc.).
    fn get_font_metrics(&self) -> LayoutFontMetrics;

    /// Returns the total number of glyphs in the font.
    fn num_glyphs(&self) -> u16;

    /// Returns the advance width of the space character (U+0020) in font units,
    /// or None if the font doesn't have a space glyph.
    fn get_space_width(&self) -> Option<usize>;
}

/// Trait for loading fonts from raw bytes.
///
/// This allows different font loading strategies (e.g., allsorts, freetype, mock)
/// to be used with the layout engine.
pub trait FontLoaderTrait<T>: Send + core::fmt::Debug {
    fn load_font(&self, font_bytes: &[u8], font_index: usize) -> Result<T, LayoutError>;
}

// When text_layout or font_loading is disabled, provide minimal stub types
#[cfg(not(all(feature = "text_layout", feature = "font_loading")))]
pub use stub::*;

#[cfg(not(all(feature = "text_layout", feature = "font_loading")))]
mod stub {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Script;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Language;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum FontStyle {
        Normal,
        Italic,
        Oblique,
    }

    /// Stub for BidiDirection when text_layout is disabled
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BidiDirection {
        Ltr,
        Rtl,
    }

    #[derive(Debug, Clone)]
    pub struct StyleProperties;

    #[derive(Debug, Clone)]
    pub struct Glyph;

    #[derive(Debug, Clone, Copy)]
    pub struct VerticalMetrics {
        pub ascent: f32,
        pub descent: f32,
        pub line_gap: f32,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct LayoutFontMetrics {
        pub ascent: f32,
        pub descent: f32,
        pub line_gap: f32,
        pub units_per_em: u16,
        pub x_height: Option<f32>,
        pub cap_height: Option<f32>,
    }

    #[derive(Debug, Clone)]
    pub struct LayoutError;

    #[derive(Debug, Clone)]
    pub struct FontSelector;

    #[derive(Debug)]
    pub struct FontManager;

    #[derive(Debug)]
    pub struct LayoutCache;

    // Additional stub types needed by solver3
    pub type ContentIndex = usize;
    pub type FontHash = u64;

    #[derive(Debug, Clone)]
    pub struct InlineContent;

    #[derive(Debug, Clone)]
    pub struct StyledRun;

    #[derive(Debug, Clone)]
    pub struct LayoutFragment;

    #[derive(Debug, Clone)]
    pub struct UnifiedConstraints;

    #[derive(Debug, Clone)]
    pub struct InlineImage;

    #[derive(Debug, Clone)]
    pub struct InlineShape;

    #[derive(Debug, Clone)]
    pub struct ShapeDefinition;

    #[derive(Debug, Clone)]
    pub struct ShapeBoundary;

    #[derive(Debug, Clone)]
    pub struct ShapedItem;

    #[derive(Debug, Clone)]
    pub enum ImageSource {
        Ref(azul_core::resources::ImageRef),
        Url(String),
        Data(std::sync::Arc<[u8]>),
        Svg(std::sync::Arc<str>),
        Placeholder(Size),
    }

    #[derive(Debug, Clone, Copy)]
    pub enum ObjectFit {
        Contain,
        Cover,
        Fill,
        None,
        ScaleDown,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum SegmentAlignment {
        Start,
        Center,
        End,
    }

    #[derive(Debug, Clone)]
    pub struct UnifiedLayout;

    pub type TextLayoutCache = LayoutCache;

    pub type Size = azul_core::geom::LogicalSize;
}
