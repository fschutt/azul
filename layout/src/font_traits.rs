//! Font traits that are always available, regardless of text_layout feature.
//!
//! These traits define the interface between the layout solver and the font system.
//! The actual implementations live in text3/cache.rs when text_layout feature is enabled.

use azul_core::geom::LogicalSize;

#[cfg(feature = "text_layout")]
pub use crate::text3::script::Language;
#[cfg(feature = "text_layout")]
pub use crate::text3::{
    cache::{
        AvailableSpace, BidiDirection, ContentIndex, FontHash, FontManager, FontSelector, FontStyle,
        Glyph, ImageSource, InlineContent, InlineImage, InlineShape, LayoutCache, LayoutError,
        LayoutFontMetrics, LayoutFragment, ObjectFit, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, UnifiedConstraints,
        UnifiedLayout, VerticalMetrics,
    },
    script::Script,
};

pub type TextLayoutCache = LayoutCache;

/// Trait for types that support cheap, shallow cloning (e.g., reference-counted types).
pub trait ShallowClone {
    /// Create a shallow clone (increment reference count, don't copy data)
    fn shallow_clone(&self) -> Self;
}

/// Core trait for parsed fonts that can be used for text shaping and layout.
///
/// This trait abstracts over the actual font parsing implementation, allowing
/// the layout solver to work with different font backends.
pub trait ParsedFontTrait: Send + Clone + ShallowClone {
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

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize>;

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)>;

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)>;

    fn has_glyph(&self, codepoint: u32) -> bool;

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics>;

    fn get_font_metrics(&self) -> LayoutFontMetrics;

    fn num_glyphs(&self) -> u16;
}

/// Trait for loading fonts from raw bytes.
///
/// This allows different font loading strategies (e.g., allsorts, freetype, mock)
/// to be used with the layout engine.
pub trait FontLoaderTrait<T>: Send + core::fmt::Debug {
    fn load_font(&self, font_bytes: &[u8], font_index: usize) -> Result<T, LayoutError>;
}

// When text_layout is disabled, provide minimal stub types
#[cfg(not(feature = "text_layout"))]
pub use stub::*;

#[cfg(not(feature = "text_layout"))]
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TextDirection {
        LeftToRight,
        RightToLeft,
    }

    #[derive(Debug, Clone)]
    pub struct StyleProperties;

    #[derive(Debug, Clone)]
    pub struct Glyph<T> {
        _phantom: core::marker::PhantomData<T>,
    }

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
    }

    #[derive(Debug, Clone)]
    pub struct LayoutError;

    #[derive(Debug, Clone)]
    pub struct FontSelector;

    #[derive(Debug)]
    pub struct FontManager<T, Q> {
        _phantom: core::marker::PhantomData<(T, Q)>,
    }

    #[derive(Debug)]
    pub struct LayoutCache<T> {
        _phantom: core::marker::PhantomData<T>,
    }

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
    pub struct ImageSource;

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

    pub type TextLayoutCache = LayoutCache<()>;

    pub type Size = azul_core::geom::LogicalSize;
}
