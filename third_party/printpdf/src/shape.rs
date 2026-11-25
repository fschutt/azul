//! Turns a text layout into PDF operations.
//!
//! A PDF content stream has no line breaking, kerning rules or bidirectional text: it only says
//! "show these glyphs at these positions". Working that out is the job of a text layout engine.
//! printpdf uses the one of azul-layout, which turns text and fonts into a [`UnifiedLayout`]
//! (lines of positioned glyph ids). [`layout_to_ops`] and [`layout_to_ops_with_offset`] turn
//! such a layout into [`Op`]s that draw it. They are also available as
//! [`crate::text_shaping`], and the HTML renderer ([`crate::PdfDocument::from_html`]) uses them
//! for all of its text.
//!
//! ```text
//! text + fonts  ->  UnifiedLayout (azul-layout)  ->  Vec<Op>
//! ```
//!
//! Both functions require the `text_layout` feature. The option and result types of this
//! module ([`TextShapingOptions`], [`ShapedText`], ...) are plain data that no function of
//! printpdf reads or produces.
//!
//! To lay out text yourself without azul-layout, see
//! [`show_placed_glyphs`](crate::show_placed_glyphs), which draws glyphs at positions you
//! supply.

use crate::{Color, FontId, Pt, Rect, Rgb};

#[cfg(feature = "text_layout")]
use crate::Op;

#[cfg(feature = "text_layout")]
use azul_layout::text3::{
    cache::{LoadedFonts, ParsedFontTrait, UnifiedLayout},
    glyphs::get_glyph_runs_pdf,
};

#[cfg(feature = "text_layout")]
use azul_css::props::basic::ColorU;

/// A rectangular area that text should flow around, such as an inline image.
#[derive(Debug, Clone)]
pub struct TextHole {
    /// The rectangular area of the hole in points
    pub rect: Rect,
}

/// Horizontal text alignment options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Left align text (default for LTR scripts)
    #[default]
    Left,
    /// Center align text
    Center,
    /// Right align text (default for RTL scripts)
    Right,
    /// Justified text (both edges aligned)
    Justify,
}

/// Text direction for bidirectional text support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    /// Left-to-right (default for Latin, Cyrillic, etc.)
    #[default]
    Ltr,
    /// Right-to-left (for Arabic, Hebrew, etc.)
    Rtl,
}

/// Options for laying out a block of text.
///
/// Plain data; no function of printpdf reads it (see the [module docs](crate::shape)).
#[derive(Debug, Clone)]
pub struct TextShapingOptions {
    /// Font size in points (1 pt = 1/72 inch)
    pub font_size: Pt,
    /// Line height in points (optional, defaults to font's recommended line height)
    pub line_height: Option<Pt>,
    /// Letter spacing in points (0.0 = default spacing)
    pub letter_spacing: Option<f32>,
    /// Word spacing in points (0.0 = default spacing)
    pub word_spacing: Option<f32>,
    /// Width of tab character in points (default: 4 spaces)
    pub tab_width: Option<f32>,
    /// Maximum width of text block for line wrapping (None = no wrapping)
    pub max_width: Option<Pt>,
    /// Maximum height of text block (None = unlimited)
    pub max_height: Option<Pt>,
    /// Horizontal text alignment
    pub align: TextAlign,
    /// Text direction (LTR or RTL)
    pub direction: TextDirection,
    /// Text color
    pub color: Color,
    /// Rectangular "holes" where text won't flow
    pub holes: Vec<TextHole>,
}

impl TextShapingOptions {
    /// Create new options with default values and the specified font size
    pub fn new(font_size: Pt) -> Self {
        Self {
            font_size,
            ..Default::default()
        }
    }

    /// Set the maximum width for line wrapping
    pub fn with_max_width(mut self, max_width: Pt) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Set the maximum height for text clipping
    pub fn with_max_height(mut self, max_height: Pt) -> Self {
        self.max_height = Some(max_height);
        self
    }

    /// Set the text alignment
    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Set the text direction
    pub fn with_direction(mut self, direction: TextDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Set the text color
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the line height
    pub fn with_line_height(mut self, line_height: Pt) -> Self {
        self.line_height = Some(line_height);
        self
    }

    /// Set the letter spacing
    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = Some(letter_spacing);
        self
    }
}

impl Default for TextShapingOptions {
    fn default() -> Self {
        Self {
            font_size: Pt(12.0),
            line_height: None,
            letter_spacing: None,
            word_spacing: None,
            tab_width: None,
            max_width: None,
            max_height: None,
            align: TextAlign::default(),
            direction: TextDirection::default(),
            color: Color::Rgb(Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }),
            holes: Vec::new(),
        }
    }
}

/// A shaped word with positioning information.
///
/// Represents a single word after text shaping, with its computed
/// position and dimensions relative to the text block origin.
///
/// All measurements are in points (1/72 inch).
#[derive(Debug, Clone)]
pub struct ShapedWord {
    /// The text content of the word
    pub text: String,
    /// X position relative to the text origin (in points)
    pub x: f32,
    /// Y position relative to the text origin (in points)
    pub y: f32,
    /// Width of the word in points
    pub width: f32,
    /// Height of the word in points
    pub height: f32,
    /// Index of the word within its line (0-based)
    pub index: usize,
}

/// A line of shaped text.
///
/// Represents a single line of text after shaping and line-breaking,
/// containing the individual words and the line's position/dimensions.
///
/// All measurements are in points (1/72 inch).
#[derive(Debug, Clone)]
pub struct ShapedLine {
    /// Words making up the line
    pub words: Vec<ShapedWord>,
    /// X position relative to the text origin (in points)
    pub x: f32,
    /// Y position relative to the text origin (in points)
    pub y: f32,
    /// Width of the line in points
    pub width: f32,
    /// Height of the line in points
    pub height: f32,
    /// Line number (0-based)
    pub index: usize,
}

/// A block of shaped text with its layout.
///
/// Plain data; no function of printpdf produces it (see the [module docs](crate::shape)).
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Font ID that the ShapedText used
    pub font_id: FontId,
    /// Options that this text was laid out with
    pub options: TextShapingOptions,
    /// Lines making up the text block
    pub lines: Vec<ShapedLine>,
    /// Total width of the text block in points
    pub width: f32,
    /// Total height of the text block in points
    pub height: f32,
}

impl ShapedText {
    /// Get the bounding box of the shaped text
    pub fn bounds(&self) -> Rect {
        Rect {
            x: Pt(0.0),
            y: Pt(0.0),
            width: Pt(self.width),
            height: Pt(self.height),
            mode: None,
            winding_order: None,
        }
    }

    /// Check if the shaped text is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Get the number of lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Converts a [`UnifiedLayout`] to PDF operations.
///
/// For each glyph run (consecutive glyphs of one font, size and color) it emits
/// `SetFillColor` when the color changes, `SetFont`, and a text section with one
/// `SetTextMatrix` and `ShowText` per glyph. A run that asks for bold from a face without a
/// bold cut is drawn filled and stroked, inside its own graphics state.
///
/// Layout space has its origin at the top left with y down, PDF space at the bottom left, so
/// every y is flipped with `page_height`. Layout lengths are used as pt.
///
/// * `layout` - the layout from azul's text3 engine.
/// * `page_height` - height of the page in pt.
/// * `font_id` - ignored. Each run's font is named `F{font_hash}`, so the document's font
///   resources must be registered under those ids, as the HTML renderer does.
/// * `loaded_fonts` - the fonts the glyphs are looked up in.
/// * `default_color` - ignored; colors come from the runs.
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub fn layout_to_ops<T: ParsedFontTrait + 'static>(
    layout: &UnifiedLayout,
    page_height: Pt,
    font_id: &FontId,
    loaded_fonts: &LoadedFonts<T>,
    default_color: Color,
) -> Vec<Op> {
    layout_to_ops_with_offset(layout, page_height, font_id, loaded_fonts, default_color, Pt(0.0), Pt(0.0))
}

/// Like [`layout_to_ops`], and moves the layout by an offset on the page.
///
/// `offset_x` is the distance in pt from the left edge of the page and `offset_y` the distance
/// from the top edge (downward); both are added to the glyph positions of the layout. The other
/// parameters are those of [`layout_to_ops`], including that `font_id` and `_default_color` are
/// ignored.
///
/// Requires the `text_layout` feature.
#[cfg(feature = "text_layout")]
pub fn layout_to_ops_with_offset<T: ParsedFontTrait + 'static>(
    layout: &UnifiedLayout,
    page_height: Pt,
    _font_id: &FontId,
    loaded_fonts: &LoadedFonts<T>,
    _default_color: Color,
    offset_x: Pt,
    offset_y: Pt,
) -> Vec<Op> {
    let mut ops = Vec::new();

    // Get PDF-optimized glyph runs from the layout
    let glyph_runs = get_glyph_runs_pdf(layout, loaded_fonts);

    if glyph_runs.is_empty() {
        return ops;
    }

    // Track current color to avoid redundant SetFillColor operations
    let mut current_color: Option<ColorU> = None;

    // Process each glyph run
    for run in glyph_runs.iter() {
        if run.glyphs.is_empty() {
            continue;
        }

        // Set color if changed
        if current_color != Some(run.color) {
            ops.push(Op::SetFillColor {
                col: coloru_to_color(&run.color),
            });
            current_color = Some(run.color);
        }

        // Set font (SetFont must be outside the text section in PDF)
        let run_font_id = FontId(format!("F{}", run.font_hash));
        ops.push(Op::SetFont {
            font: crate::ops::PdfFontHandle::External(run_font_id),
            size: Pt(run.font_size_px),
        });

        // Bold asked of a face with no bold cut is drawn thickened, as the
        // screen draws it: filled and stroked in the text's colour, in a
        // graphics state of its own so the stroke settings end with the run.
        if run.synthetic_bold {
            ops.push(Op::SaveGraphicsState);
            ops.push(Op::SetOutlineColor {
                col: coloru_to_color(&run.color),
            });
            ops.push(Op::SetOutlineThickness {
                pt: Pt(run.font_size_px * azul_layout::text3::glyphs::SYNTHETIC_BOLD_STROKE_EM),
            });
            ops.push(Op::SetTextRenderingMode {
                mode: crate::graphics::TextRenderingMode::FillStroke,
            });
        }

        // Start text section
        ops.push(Op::StartTextSection);

        // Position and render each glyph
        for glyph in &run.glyphs {
            // Calculate PDF coordinates
            // Layout uses top-left origin, PDF uses bottom-left
            let pdf_x = glyph.position.x + offset_x.0;
            let pdf_y = page_height.0 - glyph.position.y - offset_y.0;

            // Set text matrix to position this glyph
            ops.push(Op::SetTextMatrix {
                matrix: crate::matrix::TextMatrix::Raw([
                    1.0,    // a: horizontal scaling
                    0.0,    // b: horizontal skewing
                    0.0,    // c: vertical skewing
                    1.0,    // d: vertical scaling
                    pdf_x,  // e: x translation
                    pdf_y,  // f: y translation
                ]),
            });

            // Render the glyph
            ops.push(Op::ShowText {
                items: vec![crate::text::TextItem::GlyphIds(vec![
                    crate::text::Codepoint {
                        gid: glyph.glyph_id,
                        offset: 0.0,
                        cid: Some(glyph.unicode_codepoint.clone()),
                    }
                ])],
            });
        }

        // End text section
        ops.push(Op::EndTextSection);

        if run.synthetic_bold {
            ops.push(Op::RestoreGraphicsState);
        }
    }

    ops
}

/// Converts an azul `ColorU` (0 to 255 per channel) to an RGB [`Color`].
#[cfg(feature = "text_layout")]
fn coloru_to_color(color: &ColorU) -> Color {
    Color::Rgb(Rgb {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        icc_profile: None,
    })
}
