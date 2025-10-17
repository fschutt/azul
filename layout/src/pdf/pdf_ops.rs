//! Shim definitions for PDF operations, decoupled from the `printpdf` crate.
//!
//! This module provides an intermediate representation for PDF rendering operations
//! that is independent of any specific PDF library. This allows azul-layout to
//! generate PDF operations without depending on printpdf, avoiding circular dependencies.

use azul_core::geom::LogicalRect;
use azul_css::props::basic::ColorU;

/// A point in PDF coordinate space (typically in points/pt units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPoint {
    pub x: f32,
    pub y: f32,
}

impl PdfPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A line segment or curve in PDF space.
#[derive(Debug, Clone)]
pub struct PdfLine {
    /// Points defining the line, with a boolean indicating if it's a curve control point
    pub points: Vec<(PdfPoint, bool)>,
    /// Whether this is a closed path
    pub is_closed: bool,
}

/// A color representation for PDF rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PdfColor {
    Rgb(ColorU),
    Rgba(ColorU),
    Cmyk { c: f32, m: f32, y: f32, k: f32 },
    Gray(f32),
}

impl From<ColorU> for PdfColor {
    fn from(c: ColorU) -> Self {
        if c.a == 255 {
            PdfColor::Rgb(c)
        } else {
            PdfColor::Rgba(c)
        }
    }
}

/// A transformation matrix for text and graphics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfTextMatrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl PdfTextMatrix {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x,
            f: y,
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }
}

/// A unique identifier for a font within a PDF document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub String);

impl FontId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// A unique identifier for an XObject (like an image) within a PDF document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct XObjectId(pub String);

impl XObjectId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Represents a single PDF rendering operation.
#[derive(Debug, Clone)]
pub enum PdfOp {
    /// Begin a new path
    BeginPath,

    /// Move to a point without drawing
    MoveTo { point: PdfPoint },

    /// Draw a line to a point
    LineTo { point: PdfPoint },

    /// Draw a cubic Bezier curve
    CurveTo {
        control1: PdfPoint,
        control2: PdfPoint,
        end: PdfPoint,
    },

    /// Close the current path
    ClosePath,

    /// Stroke the current path
    Stroke,

    /// Fill the current path
    Fill,

    /// Fill and stroke the current path
    FillAndStroke,

    /// Set the stroke color
    SetStrokeColor { color: PdfColor },

    /// Set the fill color
    SetFillColor { color: PdfColor },

    /// Set the line width
    SetLineWidth { width: f32 },

    /// Set the line dash pattern
    SetLineDash { pattern: Vec<f32>, phase: f32 },

    /// Begin a text object
    BeginText,

    /// End a text object
    EndText,

    /// Set the text font and size
    SetTextFont { font_id: FontId, size: f32 },

    /// Set the text matrix (position and transformation)
    SetTextMatrix { matrix: PdfTextMatrix },

    /// Show text (draw glyphs)
    ShowText { text: String },

    /// Show positioned text (with individual glyph positioning)
    ShowPositionedText { items: Vec<TextItem> },

    /// Draw an image XObject
    DrawImage {
        xobject_id: XObjectId,
        rect: LogicalRect,
    },

    /// Save graphics state
    SaveState,

    /// Restore graphics state
    RestoreState,

    /// Apply a transformation matrix to the current graphics state
    Transform { matrix: PdfTextMatrix },

    /// Set clipping rectangle
    ClipRect { rect: LogicalRect },
}

/// A text item with optional kerning adjustment.
#[derive(Debug, Clone)]
pub struct TextItem {
    /// The text to display (usually a single glyph)
    pub text: String,
    /// Horizontal adjustment in thousandths of a unit of text space
    pub adjustment: f32,
}

impl TextItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            adjustment: 0.0,
        }
    }

    pub fn with_adjustment(text: impl Into<String>, adjustment: f32) -> Self {
        Self {
            text: text.into(),
            adjustment,
        }
    }
}
