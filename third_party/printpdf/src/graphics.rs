//! Paths and the graphics state.
//!
//! A PDF page is drawn by a *content stream*, a list of operators. To draw a shape the stream
//! first builds a *path* and then paints it. A path is a list of *subpaths*. A subpath starts
//! with a move to a point (`m`), continues with straight lines (`l`) or cubic Bezier curves (`c`,
//! two control points and an end point), and can be closed (`h`) with a straight line back to its
//! start. Painting then strokes the outline (`S`), fills the inside (`f`), both (`B`), or turns
//! the path into a clip (`W`), so that later drawing only shows inside it.
//!
//! All coordinates are in points (1/72 inch), measured from the bottom left corner of the page,
//! with y pointing up.
//!
//! # Shapes
//!
//! - [`Line`] is an open or closed path that is stroked.
//! - [`Polygon`] is made of [`PolygonRing`]s (subpaths) and a [`PaintMode`]. Several rings make
//!   shapes with holes, together with a [`WindingOrder`], the rule that decides which points are
//!   inside. With `NonZero`, a hole needs a ring that runs in the opposite direction of the
//!   outer ring. With `EvenOdd`, any ring inside another one makes a hole, whatever its direction.
//! - [`Rect`] is an axis aligned rectangle and converts to both of the above.
//! - [`Point`] and [`LinePoint`] are the vertices. A `LinePoint` with `bezier: true` is a curve
//!   control point.
//!
//! Draw them with [`Op::DrawLine`](crate::Op::DrawLine), [`Op::DrawPolygon`](crate::Op::DrawPolygon)
//! and [`Op::DrawRectangle`](crate::Op::DrawRectangle).
//!
//! # Stroke style
//!
//! The width, cap ([`LineCapStyle`], the shape of line ends), join ([`LineJoinStyle`], the shape
//! of corners), miter limit (how long a pointed corner may get before it is cut off) and dash
//! pattern ([`LineDashPattern`]) are part of the *graphics state*. Set them with the `Op::Set...`
//! operations or through an [`ExtendedGraphicsState`].
//!
//! # Extended graphics state and transparency
//!
//! [`ExtendedGraphicsState`] (`ExtGState` in PDF) is a named set of graphics parameters. It also
//! carries what has no operator of its own: opacity of fills and strokes, the [`BlendMode`] that
//! decides how new paint mixes with what is below it, a [`SoftMask`], overprint settings for
//! print, and flatness and smoothness tolerances. Register one with
//! [`PdfDocument::add_graphics_state`](crate::PdfDocument::add_graphics_state) and apply it with
//! [`Op::LoadGraphicsState`](crate::Op::LoadGraphicsState).
//!
//! # Examples
//!
//! A square with a square hole in it. The even-odd rule makes the inner ring a hole.
//!
//! ```
//! use printpdf::*;
//!
//! let ring = |x: f32, size: f32| PolygonRing {
//!     points: [(x, x), (x + size, x), (x + size, x + size), (x, x + size)]
//!         .iter()
//!         .map(|&(x, y)| LinePoint { p: Point::new(Mm(x), Mm(y)), bezier: false })
//!         .collect(),
//! };
//! let polygon = Polygon {
//!     rings: vec![ring(10.0, 40.0), ring(20.0, 20.0)],
//!     mode: PaintMode::Fill,
//!     winding_order: WindingOrder::EvenOdd,
//! };
//! let ops = vec![
//!     Op::SetFillColor { col: Color::Rgb(Rgb::new(0.0, 0.4, 0.8, None)) },
//!     Op::DrawPolygon { polygon },
//! ];
//! # let _ = ops;
//! ```
//!
//! Half transparent paint that darkens what is below it:
//!
//! ```
//! use printpdf::*;
//!
//! let mut doc = PdfDocument::new("alpha");
//! let gs = doc.add_graphics_state(
//!     ExtendedGraphicsState::default()
//!         .with_current_fill_alpha(0.5)
//!         .with_blend_mode(BlendMode::multiply()),
//! );
//! let ops = vec![
//!     Op::SaveGraphicsState,
//!     Op::LoadGraphicsState { gs },
//!     Op::DrawRectangle {
//!         rectangle: Rect {
//!             mode: Some(PaintMode::Fill),
//!             ..Rect::from_xywh(Pt(50.0), Pt(50.0), Pt(100.0), Pt(100.0))
//!         },
//!     },
//!     Op::RestoreGraphicsState,
//! ];
//! # let _ = ops;
//! ```

use core::fmt;
use std::collections::HashSet;

use lopdf::Dictionary as LoDictionary;
use serde_derive::{Deserialize, Serialize};

use crate::{
    units::{Mm, Pt},
    BuiltinFont, FontId,
};

/// `f`: fills the path, nonzero rule.
pub const OP_PATH_PAINT_FILL_NZ: &str = "f";
/// `f*`: fills the path, even-odd rule.
pub const OP_PATH_PAINT_FILL_EO: &str = "f*";
/// `B`: fills and strokes the path, nonzero rule.
pub const OP_PATH_PAINT_FILL_STROKE_NZ: &str = "B";
/// `b`: closes, fills and strokes the path, nonzero rule.
pub const OP_PATH_PAINT_FILL_STROKE_CLOSE_NZ: &str = "b";
/// `B*`: fills and strokes the path, even-odd rule.
pub const OP_PATH_PAINT_FILL_STROKE_EO: &str = "B*";
/// `b*`: closes, fills and strokes the path, even-odd rule.
pub const OP_PATH_PAINT_FILL_STROKE_CLOSE_EO: &str = "b*";
/// `W`: intersects the clipping path with the current path, nonzero rule.
pub const OP_PATH_CONST_CLIP_NZ: &str = "W";
/// `W*`: intersects the clipping path with the current path, even-odd rule.
pub const OP_PATH_CONST_CLIP_EO: &str = "W*";

/// An axis aligned rectangle with the corner closest to the page origin at `(x, y)`.
///
/// The origin of a page is its bottom left corner and y points up. Drawn with
/// [`Op::DrawRectangle`](crate::Op::DrawRectangle), which uses `mode` and `winding_order`.
#[derive(PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    /// Distance of the left edge from the left side of the page.
    pub x: Pt,
    /// Distance of the bottom edge from the bottom of the page.
    pub y: Pt,
    /// Width, in points.
    pub width: Pt,
    /// Height, in points.
    pub height: Pt,
    /// How to paint the rectangle. With `None` it is not painted: the path is ended (`n`), or
    /// used as a clip if `winding_order` is set.
    pub mode: Option<PaintMode>,
    /// Fill and clip rule. `None` means nonzero when `mode` is set.
    pub winding_order: Option<WindingOrder>,
}

impl fmt::Debug for Rect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}x{} @ {} - {}",
            self.width.0, self.height.0, self.x.0, self.y.0
        )
    }
}

impl Rect {
    /// The bottom left corner.
    pub fn lower_left(&self) -> Point {
        Point {
            x: self.x,
            y: self.y,
        }
    }

    /// The top right corner.
    pub fn upper_right(&self) -> Point {
        Point {
            x: self.x + self.width,
            y: self.y + self.height,
        }
    }

    /// A rectangle of the given size with its corner at the origin and no paint mode.
    pub fn from_wh(width: Pt, height: Pt) -> Self {
        Self {
            x: Pt(0.0),
            y: Pt(0.0),
            width,
            height,
            mode: None,
            winding_order: None,
        }
    }

    /// A rectangle at `(x, y)` with the given size and no paint mode.
    pub fn from_xywh(x: Pt, y: Pt, width: Pt, height: Pt) -> Self {
        Self {
            x,
            y,
            width,
            height,
            mode: None,
            winding_order: None,
        }
    }

    /// The rectangle as a one-ring polygon. Uses `mode` (default: fill) and `winding_order`
    /// (default: nonzero).
    pub fn to_polygon(&self) -> Polygon {
        Polygon {
            rings: vec![PolygonRing {
                points: self.gen_points(),
            }],
            mode: self.mode.unwrap_or(PaintMode::Fill),
            winding_order: self.winding_order.unwrap_or(WindingOrder::NonZero),
        }
    }

    /// The outline of the rectangle as a closed [`Line`].
    pub fn to_line(&self) -> Line {
        Line {
            points: self.gen_points(),
            is_closed: true,
        }
    }

    fn gen_points(&self) -> Vec<LinePoint> {
        let top = Pt(self.y.0 + self.height.0);
        let bottom = self.y;
        let left = self.x;
        let right = Pt(self.x.0 + self.width.0);

        let tl = Point { x: left, y: top };
        let tr = Point { x: right, y: top };
        let br = Point {
            x: right,
            y: bottom,
        };
        let bl = Point { x: left, y: bottom };

        vec![
            LinePoint {
                p: tl,
                bezier: false,
            },
            LinePoint {
                p: tr,
                bezier: false,
            },
            LinePoint {
                p: br,
                bezier: false,
            },
            LinePoint {
                p: bl,
                bezier: false,
            },
        ]
    }

    /// The PDF array `[x, y, width, height]` of real numbers.
    pub fn to_array(&self) -> Vec<lopdf::Object> {
        vec![
            self.x.0.into(),
            self.y.0.into(),
            self.width.0.into(),
            self.height.0.into(),
        ]
    }
}

/// The rule that decides which points are inside a path, for filling and clipping.
///
/// The two rules differ only for paths that overlap themselves or have several rings (ISO 32000-1,
/// 8.5.3.3). `NonZero` is the usual choice. Serialized to JSON in kebab-case.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindingOrder {
    /// A point is inside if a ray from it crosses an odd number of path segments, whatever their
    /// direction. Nested rings alternate between filled and empty.
    EvenOdd,

    /// A point is inside if the path winds around it a net number of times that is not zero.
    /// Crossings count +1 in one direction and -1 in the other, so a ring that runs the other
    /// way cuts a hole.
    #[default]
    NonZero,
}

impl WindingOrder {
    /// The clip operator: `W` or `W*`.
    #[must_use]
    pub fn get_clip_op(&self) -> &'static str {
        match self {
            WindingOrder::NonZero => OP_PATH_CONST_CLIP_NZ,
            WindingOrder::EvenOdd => OP_PATH_CONST_CLIP_EO,
        }
    }

    /// The fill operator: `f` or `f*`.
    #[must_use]
    pub fn get_fill_op(&self) -> &'static str {
        match self {
            WindingOrder::NonZero => OP_PATH_PAINT_FILL_NZ,
            WindingOrder::EvenOdd => OP_PATH_PAINT_FILL_EO,
        }
    }

    /// The close, fill and stroke operator: `b` or `b*`.
    #[must_use]
    pub fn get_fill_stroke_close_op(&self) -> &'static str {
        match self {
            WindingOrder::NonZero => OP_PATH_PAINT_FILL_STROKE_CLOSE_NZ,
            WindingOrder::EvenOdd => OP_PATH_PAINT_FILL_STROKE_CLOSE_EO,
        }
    }

    /// The fill and stroke operator: `B` or `B*`.
    #[must_use]
    pub fn get_fill_stroke_op(&self) -> &'static str {
        match self {
            WindingOrder::NonZero => OP_PATH_PAINT_FILL_STROKE_NZ,
            WindingOrder::EvenOdd => OP_PATH_PAINT_FILL_STROKE_EO,
        }
    }
}

/// What to do with a path once it is built. Serialized to JSON in kebab-case.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaintMode {
    /// Draws nothing and limits later painting to the inside of the path (`W`, then `n`). The
    /// [`WindingOrder`] picks the rule. Wrap it in a saved graphics state to undo the clip.
    Clip,

    /// Fills the inside of the path with the fill color (`f`).
    #[default]
    Fill,

    /// Draws a line along the path with the outline color and the current stroke style (`S`).
    Stroke,

    /// Fills the path and then strokes it (`B`).
    FillStroke,
}

/// A position on a page, in points from the bottom left corner of the page.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    /// Distance from the left side of the page, in points.
    pub x: Pt,
    /// Distance from the bottom of the page, in points.
    pub y: Pt,
}

impl Point {
    /// Creates a point from millimeters, measured from the bottom left corner of the page. The
    /// values are converted to points. For other units, fill `x` and `y` directly.
    #[inline]
    pub fn new(x: Mm, y: Mm) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }
}

impl PartialEq for Point {
    // Compares with the tolerance of `Pt`. Coordinates that are exactly 0.0 never compare equal.
    fn eq(&self, other: &Point) -> bool {
        if self.x.0.is_normal()
            && other.x.0.is_normal()
            && self.y.0.is_normal()
            && other.y.0.is_normal()
        {
            let x_eq = self.x == other.x;
            if !x_eq {
                return false;
            }
            let y_eq = self.y == other.y;
            if y_eq {
                return true;
            }
        }

        false
    }
}

/// A vertex of a [`Line`] or [`PolygonRing`]: an on-curve point or a Bezier control point.
///
/// A cubic curve is made of three consecutive entries: two control points with `bezier: true`
/// and the end point after them. A single control point without a partner is drawn as a straight
/// line to it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinePoint {
    /// Location of the point.
    pub p: Point,
    /// `true` if this point is a Bezier control point instead of a point on the path.
    pub bezier: bool,
}

/// A path that is stroked with the current outline color and stroke style, never filled.
///
/// Drawn with [`Op::DrawLine`](crate::Op::DrawLine) (`m`, `l`, `c`, then `S`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// The vertices in drawing order. The first one starts the path.
    pub points: Vec<LinePoint>,
    /// `true` draws a closing segment from the last point back to the first (`h` before `S`).
    pub is_closed: bool,
}

/// A closed shape made of one or more rings, filled, stroked or used as a clip.
///
/// Drawn with [`Op::DrawPolygon`](crate::Op::DrawPolygon). The default is an empty polygon that
/// is filled with the nonzero rule. Collecting `(Point, bool)` pairs builds a one-ring polygon
/// whose `bool` is [`LinePoint::bezier`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Polygon {
    /// The outline and the holes. Each ring is its own subpath.
    pub rings: Vec<PolygonRing>,
    /// Fill, stroke, fill and stroke, or clip.
    pub mode: PaintMode,
    /// Rule for filling and clipping. It decides which rings are holes.
    pub winding_order: WindingOrder,
}

/// One closed outline of a [`Polygon`]: the outer border, or a hole.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonRing {
    /// The vertices in drawing order. The ring is closed from the last point to the first.
    pub points: Vec<LinePoint>,
}

impl FromIterator<(Point, bool)> for Polygon {
    fn from_iter<I: IntoIterator<Item = (Point, bool)>>(iter: I) -> Self {
        let mut points = Vec::new();
        for i in iter {
            points.push(LinePoint {
                p: i.0,
                bezier: i.1,
            });
        }
        Polygon {
            rings: vec![PolygonRing { points }],
            ..Default::default()
        }
    }
}

/// How a stroke is broken into dashes: lengths of dashes and gaps in points.
///
/// Written as the `d` operator, `[pattern] offset`.
#[derive(Debug, Clone, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineDashPattern {
    /// Distance into the pattern at which the line starts, in points (0 starts with a full dash).
    pub offset: f32,
    /// Alternating lengths in points: dash, gap, dash, gap, and so on. The pattern repeats. A
    /// single number is used for both dash and gap. An empty pattern is a solid line.
    pub pattern: smallvec::SmallVec<[f32;8]>, 
}

impl LineDashPattern {
    /// A dash pattern of alternating dash and gap lengths (points), starting `offset` points into
    /// the pattern. Takes a plain slice, so callers need no `smallvec` dependency.
    ///
    /// # Examples
    ///
    /// ```
    /// # use printpdf::LineDashPattern;
    /// let dashed = LineDashPattern::new(0.0, &[5.0, 3.0]);
    /// let solid = LineDashPattern::solid();
    /// ```
    pub fn new(offset: f32, pattern: &[f32]) -> Self {
        Self {
            offset,
            pattern: smallvec::SmallVec::from_slice(pattern),
        }
    }

    /// An empty pattern, a solid line. Use it to switch dashing off again.
    pub fn solid() -> Self {
        Self::default()
    }

    /// The pattern as comma separated numbers, as for the SVG `stroke-dasharray` attribute.
    pub fn get_svg_id(&self) -> String {
        self.pattern
            .iter()
            .map(|num| num.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The shape of the corner where two stroked segments meet (ISO 32000-1, 8.4.3.4).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineJoinStyle {
    /// Pointed corner: the outer edges are extended until they meet. A corner sharper than the
    /// miter limit is cut off like `Bevel`.
    Miter,
    /// Rounded corner.
    Round,
    /// Corner cut off flat.
    Bevel,
}

impl LineJoinStyle {
    /// The PDF code for the `j` operator: 0 miter, 1 round, 2 bevel.
    pub fn id(&self) -> i64 {
        match self {
            LineJoinStyle::Miter => 0,
            LineJoinStyle::Round => 1,
            LineJoinStyle::Bevel => 2,
        }
    }
    /// The SVG `stroke-linejoin` keyword.
    pub fn to_svg_string(&self) -> &'static str {
        match self {
            LineJoinStyle::Miter => "miter",
            LineJoinStyle::Round => "round",
            LineJoinStyle::Bevel => "bevel",
        }
    }
}

/// How the glyphs of text are painted (`Tr` operator, ISO 32000-1, 9.3.6). The default is `Fill`.
///
/// Fill uses the fill color, stroke uses the outline color. The clip modes add the glyph outlines
/// to the clipping path when the text object ends.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextRenderingMode {
    /// Fills the glyphs (mode 0).
    Fill,
    /// Strokes the glyph outlines (mode 1).
    Stroke,
    /// Fills and strokes the glyphs (mode 2).
    FillStroke,
    /// Draws nothing, for example for text that is only meant to be selectable (mode 3).
    Invisible,
    /// Fills the glyphs and adds them to the clip (mode 4).
    FillClip,
    /// Strokes the glyphs and adds them to the clip (mode 5).
    StrokeClip,
    /// Fills and strokes the glyphs and adds them to the clip (mode 6).
    FillStrokeClip,
    /// Only adds the glyphs to the clip (mode 7).
    Clip,
}

impl TextRenderingMode {
    /// The mode for a PDF code from 0 to 7. Any other value gives `Fill`.
    pub fn from_i64(i: i64) -> Self {
        match i {
            0 => TextRenderingMode::Fill,
            1 => TextRenderingMode::Stroke,
            2 => TextRenderingMode::FillStroke,
            3 => TextRenderingMode::Invisible,
            4 => TextRenderingMode::FillClip,
            5 => TextRenderingMode::StrokeClip,
            6 => TextRenderingMode::FillStrokeClip,
            7 => TextRenderingMode::Clip,
            _ => TextRenderingMode::Fill,
        }
    }
    /// The PDF code (0 to 7) for the `Tr` operator.
    pub fn id(&self) -> i64 {
        match self {
            TextRenderingMode::Fill => 0,
            TextRenderingMode::Stroke => 1,
            TextRenderingMode::FillStroke => 2,
            TextRenderingMode::Invisible => 3,
            TextRenderingMode::FillClip => 4,
            TextRenderingMode::StrokeClip => 5,
            TextRenderingMode::FillStrokeClip => 6,
            TextRenderingMode::Clip => 7,
        }
    }
}

/// The shape of the two ends of an open stroked path (ISO 32000-1, 8.4.3.3).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineCapStyle {
    /// Flat end exactly at the end point.
    Butt,
    /// Semicircle around the end point, with the line width as diameter.
    Round,
    /// Flat end that extends past the end point by half the line width.
    ProjectingSquare,
}

impl LineCapStyle {
    /// The PDF code for the `J` operator: 0 butt, 1 round, 2 projecting square.
    pub fn id(&self) -> i64 {
        match self {
            LineCapStyle::Butt => 0,
            LineCapStyle::Round => 1,
            LineCapStyle::ProjectingSquare => 2,
        }
    }

    /// The SVG `stroke-linecap` keyword.
    pub fn get_svg_id(&self) -> &'static str {
        match self {
            LineCapStyle::Butt => "butt",
            LineCapStyle::Round => "round",
            LineCapStyle::ProjectingSquare => "square",
        }
    }
}

/// A field of an [`ExtendedGraphicsState`] that has been set. Only these are written to the PDF.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename = "kebab-case")]
pub enum ChangedField {
    /// `LW`, line width.
    LineWidth,
    /// `LC`, line cap style.
    LineCap,
    /// `LJ`, line join style.
    LineJoin,
    /// `ML`, miter limit.
    MiterLimit,
    /// `D`, dash pattern.
    LineDashPattern,
    /// `RI`, rendering intent.
    RenderingIntent,
    /// Overprint for stroking.
    OverprintStroke,
    /// Overprint for filling.
    OverprintFill,
    /// `OPM`, overprint mode.
    OverprintMode,
    /// `Font`, the font.
    Font,
    /// `BG`, black generation.
    BlackGeneration,
    /// `BG2`, black generation, extended.
    BlackGenerationExtra,
    /// `UCR`, undercolor removal.
    UnderColorRemoval,
    /// `UCR2`, undercolor removal, extended.
    UnderColorRemovalExtra,
    /// `TR`, transfer function.
    TransferFunction,
    /// `TR2`, transfer function, extended.
    TransferFunctionExtra,
    /// `HT`, halftone.
    HalftoneDictionary,
    /// `FL`, flatness tolerance.
    FlatnessTolerance,
    /// `SM`, smoothness tolerance.
    SmoothnessTolerance,
    /// `SA`, automatic stroke adjustment.
    StrokeAdjustment,
    /// `BM`, blend mode.
    BlendMode,
    /// `SMask`, soft mask.
    SoftMask,
    /// `CA`, opacity of strokes.
    CurrentStrokeAlpha,
    /// `ca`, opacity of fills.
    CurrentFillAlpha,
    /// `AIS`, whether the alpha constants are shape values.
    AlphaIsShape,
    /// `TK`, text knockout.
    TextKnockout,
}

/// Names a font of the document: one of the built-in PDF fonts, or a font added to the document.
///
/// Serialized to JSON as `{ "type": "builtin", "data": ... }` or `{ "type": "external", ... }`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinOrExternalFontId {
    /// One of the 14 standard PDF fonts, which every reader has.
    Builtin(BuiltinFont),
    /// A font that was added to the document.
    External(FontId),
}

impl BuiltinOrExternalFontId {
    /// `true` for [`Self::Builtin`].
    pub fn is_builtin(&self) -> bool {
        match self {
            BuiltinOrExternalFontId::Builtin(_) => true,
            BuiltinOrExternalFontId::External(_) => false,
        }
    }

    /// The font name used as resource name in the PDF.
    pub fn get_id(&self) -> &str {
        match self {
            BuiltinOrExternalFontId::Builtin(builtin_font) => builtin_font.get_id(),
            BuiltinOrExternalFontId::External(font_id) => &font_id.0,
        }
    }
    /// A built-in font if `s` is the id of one (see [`BuiltinFont::from_id`]), else an external
    /// font with that id.
    pub fn from_str(s: &str) -> Self {
        if let Some(bf) = BuiltinFont::from_id(s) {
            Self::Builtin(bf)
        } else {
            Self::External(FontId(s.to_string()))
        }
    }
}

/// The extended graphics state (`ExtGState`): a set of graphics parameters that
/// [`Op::LoadGraphicsState`](crate::Op::LoadGraphicsState) applies with the `gs` operator.
///
/// The graphics state holds the parameters a page is drawn with: colors, line style, opacity and
/// more. Most have their own [`Op`](crate::Op), but opacity, blend mode, soft mask, overprint and
/// the tolerances can only be set through an `ExtGState`.
///
/// Start from [`Default`], set what is needed with the `with_*` methods (or `set_*`), register the
/// result with [`PdfDocument::add_graphics_state`](crate::PdfDocument::add_graphics_state) and
/// load the returned id. Only the fields that were set are written. The others keep the value of
/// the current graphics state. Wrap the use in
/// [`Op::SaveGraphicsState`](crate::Op::SaveGraphicsState) and
/// [`Op::RestoreGraphicsState`](crate::Op::RestoreGraphicsState) to undo it. Lengths are in
/// points. Serialized to JSON in camelCase.
///
/// # Examples
///
/// ```
/// use printpdf::*;
///
/// let gs = ExtendedGraphicsState::default()
///     .with_current_fill_alpha(0.5)   // fills are 50 percent opaque
///     .with_line_width(2.0);          // 2 pt wide strokes
/// assert!(gs.has_changed(ChangedField::CurrentFillAlpha));
/// assert!(!gs.has_changed(ChangedField::LineCap));
/// ```
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedGraphicsState {
    /// The fields that were set. Only these are written.
    pub(crate) changed_fields: HashSet<ChangedField>,
    /// LW - Line width
    pub(crate) line_width: f32,
    /// LC - Line cap style
    pub(crate) line_cap: LineCapStyle,
    /// LJ - Line join style
    pub(crate) line_join: LineJoinStyle,
    /// ML - Miter limit
    pub(crate) miter_limit: f32,
    /// D - Line dash pattern
    pub(crate) line_dash_pattern: Option<LineDashPattern>,
    /// RI - Rendering intent name
    pub(crate) rendering_intent: RenderingIntent,
    /// OP - Overprint for stroking
    pub(crate) overprint_stroke: bool,
    /// op - Overprint for nonstroking
    pub(crate) overprint_fill: bool,
    /// OPM - Overprint mode
    pub(crate) overprint_mode: OverprintMode,
    /// Font
    pub(crate) font: Option<BuiltinOrExternalFontId>,
    /// BG - Black-generation function
    pub(crate) black_generation: Option<BlackGenerationFunction>,
    /// BG2 - Black-generation extra function
    pub(crate) black_generation_extra: Option<BlackGenerationExtraFunction>,
    /// UCR - Undercolor-removal function
    pub(crate) under_color_removal: Option<UnderColorRemovalFunction>,
    /// UCR2 - Undercolor-removal extra function
    pub(crate) under_color_removal_extra: Option<UnderColorRemovalExtraFunction>,
    /// TR - Transfer function
    pub(crate) transfer_function: Option<TransferFunction>,
    /// TR2 - Transfer extra function
    pub(crate) transfer_extra_function: Option<TransferExtraFunction>,
    /// HT - Halftone dictionary/stream/name
    pub(crate) halftone_dictionary: Option<HalftoneType>,
    /// FL - Flatness tolerance
    pub(crate) flatness_tolerance: f32,
    /// SM - Smoothness tolerance
    pub(crate) smoothness_tolerance: f32,
    /// SA - Automatic stroke adjustment
    pub(crate) stroke_adjustment: bool,
    /// BM - Blend mode
    pub(crate) blend_mode: BlendMode,
    /// SMask - Soft mask
    pub(crate) soft_mask: Option<SoftMask>,
    /// CA - Stroking alpha constant
    pub(crate) current_stroke_alpha: f32,
    /// ca - Nonstroking alpha constant
    pub(crate) current_fill_alpha: f32,
    /// AIS - Alpha source flag
    pub(crate) alpha_is_shape: bool,
    /// TK - Text knockout flag
    pub(crate) text_knockout: bool,
}

impl ExtendedGraphicsState {
    /// Line width in points.
    pub fn line_width(&self) -> f32 {
        self.line_width
    }

    /// Shape of the ends of strokes.
    pub fn line_cap(&self) -> LineCapStyle {
        self.line_cap
    }

    /// Shape of the corners of strokes.
    pub fn line_join(&self) -> LineJoinStyle {
        self.line_join
    }

    /// Miter limit: how long a miter join may get, as a multiple of the line width, before it is cut off.
    pub fn miter_limit(&self) -> f32 {
        self.miter_limit
    }

    /// Dash pattern in points, or `None` for a solid line.
    pub fn line_dash_pattern(&self) -> &Option<LineDashPattern> {
        &self.line_dash_pattern
    }

    /// Rendering intent: how colors outside the gamut of the output device are mapped.
    pub fn rendering_intent(&self) -> RenderingIntent {
        self.rendering_intent
    }

    /// Overprint flag for strokes: paint without erasing the colorants below.
    pub fn overprint_stroke(&self) -> bool {
        self.overprint_stroke
    }

    /// Overprint flag for fills: paint without erasing the colorants below.
    pub fn overprint_fill(&self) -> bool {
        self.overprint_fill
    }

    /// Overprint mode, which decides how zero components of a CMYK color are treated.
    pub fn overprint_mode(&self) -> OverprintMode {
        self.overprint_mode
    }

    /// Font selected by the state.
    pub fn font(&self) -> &Option<BuiltinOrExternalFontId> {
        &self.font
    }

    /// Black-generation function. Not written to the PDF.
    pub fn black_generation(&self) -> &Option<BlackGenerationFunction> {
        &self.black_generation
    }

    /// Extended black-generation function. Not written to the PDF.
    pub fn black_generation_extra(&self) -> &Option<BlackGenerationExtraFunction> {
        &self.black_generation_extra
    }

    /// Undercolor-removal function. Not written to the PDF.
    pub fn under_color_removal(&self) -> &Option<UnderColorRemovalFunction> {
        &self.under_color_removal
    }

    /// Extended undercolor-removal function. Not written to the PDF.
    pub fn under_color_removal_extra(&self) -> &Option<UnderColorRemovalExtraFunction> {
        &self.under_color_removal_extra
    }

    /// Transfer function. Not written to the PDF.
    pub fn transfer_function(&self) -> &Option<TransferFunction> {
        &self.transfer_function
    }

    /// Extended transfer function. Not written to the PDF.
    pub fn transfer_extra_function(&self) -> &Option<TransferExtraFunction> {
        &self.transfer_extra_function
    }

    /// Halftone. Not written to the PDF.
    pub fn halftone_dictionary(&self) -> &Option<HalftoneType> {
        &self.halftone_dictionary
    }

    /// Flatness tolerance: the largest allowed error when curves are drawn as lines, in device pixels (0 for the device default).
    pub fn flatness_tolerance(&self) -> f32 {
        self.flatness_tolerance
    }

    /// Smoothness tolerance: the largest allowed color error when painting smooth shadings, from 0.0 to 1.0.
    pub fn smoothness_tolerance(&self) -> f32 {
        self.smoothness_tolerance
    }

    /// Whether thin strokes are adjusted so that they are drawn with a consistent width.
    pub fn stroke_adjustment(&self) -> bool {
        self.stroke_adjustment
    }

    /// Blend mode used when painting over existing content.
    pub fn blend_mode(&self) -> &BlendMode {
        &self.blend_mode
    }

    /// Soft mask. Not written to the PDF, except that `None` writes `SM /None`.
    pub fn soft_mask(&self) -> &Option<SoftMask> {
        &self.soft_mask
    }

    /// Opacity of strokes, from 0.0 (transparent) to 1.0 (opaque).
    pub fn current_stroke_alpha(&self) -> f32 {
        self.current_stroke_alpha
    }

    /// Opacity of fills, from 0.0 (transparent) to 1.0 (opaque).
    pub fn current_fill_alpha(&self) -> f32 {
        self.current_fill_alpha
    }

    /// Whether the alpha values are read as shape (coverage) instead of opacity.
    pub fn alpha_is_shape(&self) -> bool {
        self.alpha_is_shape
    }

    /// Whether overlapping glyphs of one text object knock each other out instead of stacking.
    pub fn text_knockout(&self) -> bool {
        self.text_knockout
    }

    /// Sets the line width and marks the field as changed.
    pub fn set_line_width(&mut self, value: f32) {
        self.line_width = value;
        self.changed_fields.insert(ChangedField::LineWidth);
    }

    /// Sets the line cap style and marks the field as changed.
    pub fn set_line_cap(&mut self, value: LineCapStyle) {
        self.line_cap = value;
        self.changed_fields.insert(ChangedField::LineCap);
    }

    /// Sets the line join style and marks the field as changed.
    pub fn set_line_join(&mut self, value: LineJoinStyle) {
        self.line_join = value;
        self.changed_fields.insert(ChangedField::LineJoin);
    }

    /// Sets the miter limit and marks the field as changed.
    pub fn set_miter_limit(&mut self, value: f32) {
        self.miter_limit = value;
        self.changed_fields.insert(ChangedField::MiterLimit);
    }

    /// Sets the dash pattern and marks the field as changed.
    pub fn set_line_dash_pattern(&mut self, value: Option<LineDashPattern>) {
        self.line_dash_pattern = value;
        self.changed_fields.insert(ChangedField::LineDashPattern);
    }

    /// Sets the rendering intent and marks the field as changed.
    pub fn set_rendering_intent(&mut self, value: RenderingIntent) {
        self.rendering_intent = value;
        self.changed_fields.insert(ChangedField::RenderingIntent);
    }

    /// Sets the overprint flag for strokes and marks the field as changed.
    pub fn set_overprint_stroke(&mut self, value: bool) {
        self.overprint_stroke = value;
        self.changed_fields.insert(ChangedField::OverprintStroke);
    }

    /// Sets the overprint flag for fills and marks the field as changed.
    pub fn set_overprint_fill(&mut self, value: bool) {
        self.overprint_fill = value;
        self.changed_fields.insert(ChangedField::OverprintFill);
    }

    /// Sets the overprint mode and marks the field as changed.
    pub fn set_overprint_mode(&mut self, value: OverprintMode) {
        self.overprint_mode = value;
        self.changed_fields.insert(ChangedField::OverprintMode);
    }

    /// Sets the font and marks the field as changed.
    pub fn set_font(&mut self, value: Option<BuiltinOrExternalFontId>) {
        self.font = value;
        self.changed_fields.insert(ChangedField::Font);
    }

    /// Sets the black-generation function and marks the field as changed.
    pub fn set_black_generation(&mut self, value: Option<BlackGenerationFunction>) {
        self.black_generation = value;
        self.changed_fields.insert(ChangedField::BlackGeneration);
    }

    /// Sets the extended black-generation function and marks the field as changed.
    pub fn set_black_generation_extra(&mut self, value: Option<BlackGenerationExtraFunction>) {
        self.black_generation_extra = value;
        self.changed_fields
            .insert(ChangedField::BlackGenerationExtra);
    }

    /// Sets the undercolor-removal function and marks the field as changed.
    pub fn set_under_color_removal(&mut self, value: Option<UnderColorRemovalFunction>) {
        self.under_color_removal = value;
        self.changed_fields.insert(ChangedField::UnderColorRemoval);
    }

    /// Sets the extended undercolor-removal function and marks the field as changed.
    pub fn set_under_color_removal_extra(&mut self, value: Option<UnderColorRemovalExtraFunction>) {
        self.under_color_removal_extra = value;
        self.changed_fields
            .insert(ChangedField::UnderColorRemovalExtra);
    }

    /// Sets the transfer function and marks the field as changed.
    pub fn set_transfer_function(&mut self, value: Option<TransferFunction>) {
        self.transfer_function = value;
        self.changed_fields.insert(ChangedField::TransferFunction);
    }

    /// Sets the extended transfer function and marks the field as changed.
    pub fn set_transfer_extra_function(&mut self, value: Option<TransferExtraFunction>) {
        self.transfer_extra_function = value;
        self.changed_fields
            .insert(ChangedField::TransferFunctionExtra);
    }

    /// Sets the halftone and marks the field as changed.
    pub fn set_halftone_dictionary(&mut self, value: Option<HalftoneType>) {
        self.halftone_dictionary = value;
        self.changed_fields.insert(ChangedField::HalftoneDictionary);
    }

    /// Sets the flatness tolerance and marks the field as changed.
    pub fn set_flatness_tolerance(&mut self, value: f32) {
        self.flatness_tolerance = value;
        self.changed_fields.insert(ChangedField::FlatnessTolerance);
    }

    /// Sets the smoothness tolerance and marks the field as changed.
    pub fn set_smoothness_tolerance(&mut self, value: f32) {
        self.smoothness_tolerance = value;
        self.changed_fields
            .insert(ChangedField::SmoothnessTolerance);
    }

    /// Sets the stroke adjustment flag and marks the field as changed.
    pub fn set_stroke_adjustment(&mut self, value: bool) {
        self.stroke_adjustment = value;
        self.changed_fields.insert(ChangedField::StrokeAdjustment);
    }

    /// Sets the blend mode and marks the field as changed.
    pub fn set_blend_mode(&mut self, value: BlendMode) {
        self.blend_mode = value;
        self.changed_fields.insert(ChangedField::BlendMode);
    }

    /// Sets the soft mask and marks the field as changed.
    pub fn set_soft_mask(&mut self, value: Option<SoftMask>) {
        self.soft_mask = value;
        self.changed_fields.insert(ChangedField::SoftMask);
    }

    /// Sets the opacity of strokes and marks the field as changed.
    pub fn set_current_stroke_alpha(&mut self, value: f32) {
        self.current_stroke_alpha = value;
        self.changed_fields.insert(ChangedField::CurrentStrokeAlpha);
    }

    /// Sets the opacity of fills and marks the field as changed.
    pub fn set_current_fill_alpha(&mut self, value: f32) {
        self.current_fill_alpha = value;
        self.changed_fields.insert(ChangedField::CurrentFillAlpha);
    }

    /// Sets the alpha-is-shape flag and marks the field as changed.
    pub fn set_alpha_is_shape(&mut self, value: bool) {
        self.alpha_is_shape = value;
        self.changed_fields.insert(ChangedField::AlphaIsShape);
    }

    /// Sets the text knockout flag and marks the field as changed.
    pub fn set_text_knockout(&mut self, value: bool) {
        self.text_knockout = value;
        self.changed_fields.insert(ChangedField::TextKnockout);
    }

    /// Whether the field was set, so that it is written to the PDF.
    pub fn has_changed(&self, field: ChangedField) -> bool {
        self.changed_fields.contains(&field)
    }

    /// Set line width and return self
    pub fn with_line_width(mut self, width: f32) -> Self {
        self.set_line_width(width);
        self
    }

    /// Set line cap style and return self
    pub fn with_line_cap(mut self, cap: LineCapStyle) -> Self {
        self.set_line_cap(cap);
        self
    }

    /// Set line join style and return self
    pub fn with_line_join(mut self, join: LineJoinStyle) -> Self {
        self.set_line_join(join);
        self
    }

    /// Set miter limit and return self
    pub fn with_miter_limit(mut self, limit: f32) -> Self {
        self.set_miter_limit(limit);
        self
    }

    /// Set line dash pattern and return self
    pub fn with_line_dash_pattern(mut self, pattern: Option<LineDashPattern>) -> Self {
        self.set_line_dash_pattern(pattern);
        self
    }

    /// Set rendering intent and return self
    pub fn with_rendering_intent(mut self, intent: RenderingIntent) -> Self {
        self.set_rendering_intent(intent);
        self
    }

    /// Set overprint stroke and return self
    pub fn with_overprint_stroke(mut self, overprint: bool) -> Self {
        self.set_overprint_stroke(overprint);
        self
    }

    /// Set overprint fill and return self
    pub fn with_overprint_fill(mut self, overprint: bool) -> Self {
        self.set_overprint_fill(overprint);
        self
    }

    /// Set overprint mode and return self
    pub fn with_overprint_mode(mut self, mode: OverprintMode) -> Self {
        self.set_overprint_mode(mode);
        self
    }

    /// Set font and return self
    pub fn with_font(mut self, font: Option<BuiltinOrExternalFontId>) -> Self {
        self.set_font(font);
        self
    }

    /// Set black generation function and return self
    pub fn with_black_generation(mut self, func: Option<BlackGenerationFunction>) -> Self {
        self.set_black_generation(func);
        self
    }

    /// Set black generation extra function and return self
    pub fn with_black_generation_extra(
        mut self,
        func: Option<BlackGenerationExtraFunction>,
    ) -> Self {
        self.set_black_generation_extra(func);
        self
    }

    /// Set under color removal function and return self
    pub fn with_under_color_removal(mut self, func: Option<UnderColorRemovalFunction>) -> Self {
        self.set_under_color_removal(func);
        self
    }

    /// Set under color removal extra function and return self
    pub fn with_under_color_removal_extra(
        mut self,
        func: Option<UnderColorRemovalExtraFunction>,
    ) -> Self {
        self.set_under_color_removal_extra(func);
        self
    }

    /// Set transfer function and return self
    pub fn with_transfer_function(mut self, func: Option<TransferFunction>) -> Self {
        self.set_transfer_function(func);
        self
    }

    /// Set transfer extra function and return self
    pub fn with_transfer_extra_function(mut self, func: Option<TransferExtraFunction>) -> Self {
        self.set_transfer_extra_function(func);
        self
    }

    /// Set halftone dictionary and return self
    pub fn with_halftone_dictionary(mut self, dict: Option<HalftoneType>) -> Self {
        self.set_halftone_dictionary(dict);
        self
    }

    /// Set flatness tolerance and return self
    pub fn with_flatness_tolerance(mut self, tolerance: f32) -> Self {
        self.set_flatness_tolerance(tolerance);
        self
    }

    /// Set smoothness tolerance and return self
    pub fn with_smoothness_tolerance(mut self, tolerance: f32) -> Self {
        self.set_smoothness_tolerance(tolerance);
        self
    }

    /// Set stroke adjustment and return self
    pub fn with_stroke_adjustment(mut self, adjustment: bool) -> Self {
        self.set_stroke_adjustment(adjustment);
        self
    }

    /// Set blend mode and return self
    pub fn with_blend_mode(mut self, mode: BlendMode) -> Self {
        self.set_blend_mode(mode);
        self
    }

    /// Set soft mask and return self
    pub fn with_soft_mask(mut self, mask: Option<SoftMask>) -> Self {
        self.set_soft_mask(mask);
        self
    }

    /// Set current stroke alpha and return self
    pub fn with_current_stroke_alpha(mut self, alpha: f32) -> Self {
        self.set_current_stroke_alpha(alpha);
        self
    }

    /// Set current fill alpha and return self
    pub fn with_current_fill_alpha(mut self, alpha: f32) -> Self {
        self.set_current_fill_alpha(alpha);
        self
    }

    /// Set alpha is shape and return self
    pub fn with_alpha_is_shape(mut self, is_shape: bool) -> Self {
        self.set_alpha_is_shape(is_shape);
        self
    }

    /// Set text knockout and return self
    pub fn with_text_knockout(mut self, knockout: bool) -> Self {
        self.set_text_knockout(knockout);
        self
    }
}

impl Default for ExtendedGraphicsState {
    /// The initial values, with no field marked as changed. Writing it gives an empty dictionary.
    fn default() -> Self {
        Self {
            changed_fields: HashSet::new(),
            line_width: 1.0,
            line_cap: LineCapStyle::Butt,
            line_join: LineJoinStyle::Miter,
            miter_limit: 0.0,
            line_dash_pattern: None,
            rendering_intent: RenderingIntent::RelativeColorimetric,
            overprint_stroke: false,
            overprint_fill: false,
            overprint_mode: OverprintMode::EraseUnderlying,
            font: None,
            black_generation: None,
            black_generation_extra: None,
            under_color_removal: None,
            under_color_removal_extra: None,
            transfer_function: None,
            transfer_extra_function: None,
            halftone_dictionary: None,
            flatness_tolerance: 0.0,
            smoothness_tolerance: 0.0,
            stroke_adjustment: true,
            blend_mode: BlendMode::Seperable(SeperableBlendMode::Normal),
            soft_mask: None,
            current_stroke_alpha: 1.0, /* 1.0 = opaque, not transparent */
            current_fill_alpha: 1.0,
            alpha_is_shape: false,
            text_knockout: false,
        }
    }
}

/// Converts the state to the dictionary of the page's `/ExtGState` resources. Only fields marked as
/// changed are included, and the dictionary is empty if there are none.
pub fn extgstate_to_dict(val: &ExtendedGraphicsState) -> LoDictionary {
    use std::string::String;

    use lopdf::Object::*;

    let mut gs_operations = Vec::<(String, lopdf::Object)>::new();

    if val.changed_fields.contains(&ChangedField::LineWidth) {
        gs_operations.push(("LW".to_string(), Real(val.line_width)));
    }

    if val.changed_fields.contains(&ChangedField::LineCap) {
        gs_operations.push(("LC".to_string(), Integer(val.line_cap.id())));
    }

    if val.changed_fields.contains(&ChangedField::LineJoin) {
        gs_operations.push(("LJ".to_string(), Integer(val.line_join.id())));
    }

    if val.changed_fields.contains(&ChangedField::MiterLimit) {
        gs_operations.push(("ML".to_string(), Real(val.miter_limit)));
    }

    if val
        .changed_fields
        .contains(&ChangedField::FlatnessTolerance)
    {
        gs_operations.push(("FL".to_string(), Real(val.flatness_tolerance)));
    }

    if val.changed_fields.contains(&ChangedField::RenderingIntent) {
        gs_operations.push(("RI".to_string(), Name(val.rendering_intent.get_id().into())));
    }

    if val.changed_fields.contains(&ChangedField::StrokeAdjustment) {
        gs_operations.push(("SA".to_string(), Boolean(val.stroke_adjustment)));
    }

    if val.changed_fields.contains(&ChangedField::OverprintFill) {
        gs_operations.push(("OP".to_string(), Boolean(val.overprint_fill)));
    }

    if val.changed_fields.contains(&ChangedField::OverprintStroke) {
        gs_operations.push(("op".to_string(), Boolean(val.overprint_stroke)));
    }

    if val.changed_fields.contains(&ChangedField::OverprintMode) {
        gs_operations.push(("OPM".to_string(), Integer(val.overprint_mode.get_id())));
    }

    // `ca` is the nonstroking (fill) alpha, `CA` the stroking (stroke) alpha.
    if val.changed_fields.contains(&ChangedField::CurrentFillAlpha) {
        gs_operations.push(("ca".to_string(), Real(val.current_fill_alpha)));
    }

    if val
        .changed_fields
        .contains(&ChangedField::CurrentStrokeAlpha)
    {
        gs_operations.push(("CA".to_string(), Real(val.current_stroke_alpha)));
    }

    if val.changed_fields.contains(&ChangedField::BlendMode) {
        gs_operations.push(("BM".to_string(), Name(val.blend_mode.get_id().into())));
    }

    if val.changed_fields.contains(&ChangedField::AlphaIsShape) {
        gs_operations.push(("AIS".to_string(), Boolean(val.alpha_is_shape)));
    }

    if val.changed_fields.contains(&ChangedField::TextKnockout) {
        gs_operations.push(("TK".to_string(), Boolean(val.text_knockout)));
    }

    if let Some(ldp) = &val.line_dash_pattern {
        if val.changed_fields.contains(&ChangedField::LineDashPattern) {
            let array = ldp.pattern.iter().copied().map(Real).collect();
            gs_operations.push(("D".to_string(), Array(array)));
        }
    }

    if let Some(font) = val.font.as_ref() {
        if val.changed_fields.contains(&ChangedField::Font) {
            gs_operations.push(("Font".to_string(), Name(font.get_id().as_bytes().to_vec())));
        }
    }

    // Black generation, undercolor removal, transfer functions, halftone and soft masks are not written.
    if val.changed_fields.contains(&ChangedField::BlackGeneration) {
        if let Some(ref _black_generation) = val.black_generation {
            // Not written.
        }
    }

    if val
        .changed_fields
        .contains(&ChangedField::BlackGenerationExtra)
    {
        if let Some(ref _black_generation_extra) = val.black_generation_extra {
            // Not written.
        }
    }

    if val
        .changed_fields
        .contains(&ChangedField::UnderColorRemoval)
    {
        if let Some(ref _under_color_removal) = val.under_color_removal {
            // Not written.
        }
    }

    if val
        .changed_fields
        .contains(&ChangedField::UnderColorRemovalExtra)
    {
        if let Some(ref _under_color_removal_extra) = val.under_color_removal_extra {
            // Not written.
        }
    }

    if val.changed_fields.contains(&ChangedField::TransferFunction) {
        if let Some(ref _transfer_function) = val.transfer_function {
            // Not written.
        }
    }

    if val
        .changed_fields
        .contains(&ChangedField::TransferFunctionExtra)
    {
        if let Some(ref _transfer_extra_function) = val.transfer_extra_function {
            // Not written.
        }
    }

    if val
        .changed_fields
        .contains(&ChangedField::HalftoneDictionary)
    {
        if let Some(ref _halftone_dictionary) = val.halftone_dictionary {
            // Not written.
        }
    }

    if val.changed_fields.contains(&ChangedField::SoftMask) {
        if let Some(ref _soft_mask) = val.soft_mask {
            // Not written.
        } else {
            gs_operations.push(("SM".to_string(), Name("None".as_bytes().to_vec())));
        }
    }

    if !gs_operations.is_empty() {
        gs_operations.push(("Type".to_string(), "ExtGState".into()));
    }

    LoDictionary::from_iter(gs_operations)
}

/// Overprint mode: whether a color component of 0 in a DeviceCMYK color erases that ink
/// underneath when overprint is on (ISO 32000-1, 8.6.7). It is the `OPM` key.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverprintMode {
    /// Every component, including zeros, replaces the underlying color (`OPM` 0, the default).
    EraseUnderlying,
    /// Components that are zero leave the underlying ink untouched (`OPM` 1).
    KeepUnderlying,
}

impl OverprintMode {
    /// The PDF code: 0 or 1.
    pub fn get_id(&self) -> i64 {
        match self {
            OverprintMode::EraseUnderlying => 0,
            OverprintMode::KeepUnderlying => 1,
        }
    }
}

/// Black generation: how much black ink to use when converting an RGB color to CMYK
/// (ISO 32000-1, 10.4). Stored in the graphics state but not written to the PDF.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlackGenerationFunction {
    /// The standard function: black is the smallest of cyan, magenta and yellow.
    Default,
    /// Like `Default`, meant to be paired with an undercolor-removal function.
    WithUnderColorRemoval,
}

/// Extended black generation (`BG2`). It has no variants, so no value can be created.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlackGenerationExtraFunction {}

/// Undercolor removal: how much cyan, magenta and yellow to take away to make up for the black
/// that black generation added (ISO 32000-1, 10.4). See [`BlackGenerationFunction`]. Stored in the
/// graphics state but not written to the PDF.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnderColorRemovalFunction {
    /// The standard function.
    Default,
}

/// Extended undercolor removal (`UCR2`). It has no variants, so no value can be created.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnderColorRemovalExtraFunction {}

/// Transfer function (`TR`), which adjusts color values for the output device. It has no
/// variants, so no value can be created.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferFunction {}

/// Extended transfer function (`TR2`). It has no variants, so no value can be created.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferExtraFunction {}

/// A halftone: how the printer turns continuous tones into a pattern of dots (ISO 32000-1, 10.5).
/// The variants are the PDF halftone types 1, 5, 6, 10 and 16. Stored in the graphics state but
/// not written to the PDF. Serialized to JSON as `{ "type": "type1", "data": ... }`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum HalftoneType {
    /// Type 1: one screen from a frequency (lines per inch), an angle (degrees) and a
    /// [`SpotFunction`].
    Type1(f32, f32, SpotFunction),
    /// Type 5: one halftone per colorant.
    Type5(Vec<HalftoneType>),
    /// Type 6: one screen from a threshold array of 8-bit values.
    Type6(Vec<u8>),
    /// Type 10: like type 6, for a cell that may be rotated.
    Type10(Vec<u8>),
    /// Type 16: one screen from a threshold array of 16-bit values.
    Type16(Vec<u16>),
}

impl HalftoneType {
    /// The PDF halftone type number: 1, 5, 6, 10 or 16.
    pub fn get_type(&self) -> i64 {
        use self::HalftoneType::*;
        match *self {
            Type1(_, _, _) => 1,
            Type5(_) => 5,
            Type6(_) => 6,
            Type10(_) => 10,
            Type16(_) => 16,
        }
    }
}

/// The predefined spot functions of PDF (ISO 32000-1, 10.5.3). A spot function maps a position in
/// a halftone cell to a grey value and so decides the shape of the dot as it grows.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpotFunction {
    /// Round dot.
    SimpleDot,
    /// Inverted round dot.
    InvertedSimpleDot,
    /// Two dots per cell.
    DoubleDot,
    /// Inverted double dot.
    InvertedDoubleDot,
    /// Dot shaped by a cosine.
    CosineDot,
    /// Two dots per cell, with half the horizontal frequency.
    Double,
    /// Inverted `Double`.
    InvertedDouble,
    /// Lines parallel to the x axis.
    Line,
    /// Gradient along x.
    LineX,
    /// Gradient along y.
    LineY,
    /// Dot that is round when small and inverted when large.
    Round,
    /// Elliptical dot.
    Ellipse,
    /// Elliptical dot, variant A.
    EllipseA,
    /// Inverted elliptical dot, variant A.
    InvertedEllipseA,
    /// Elliptical dot, variant B.
    EllipseB,
    /// Elliptical dot, variant C.
    EllipseC,
    /// Inverted elliptical dot, variant C.
    InvertedEllipseC,
    /// Square dot.
    Square,
    /// Cross-shaped dot.
    Cross,
    /// Rhomboid dot.
    Rhomboid,
    /// Diamond-shaped dot.
    Diamond,
}

/// How new paint is mixed with the paint already on the page (ISO 32000-1, 11.3.5).
///
/// Set it with [`ExtendedGraphicsState::with_blend_mode`]. The default is `Normal`, which covers
/// what is below. Use the constructors such as [`BlendMode::multiply`] to build a value.
/// Serialized to JSON as the kebab-case name of the inner mode, for example `"color-dodge"`.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", untagged)]
pub enum BlendMode {
    /// A mode that blends each color channel on its own.
    Seperable(SeperableBlendMode),
    /// A mode that works on hue, saturation and luminosity, so on all channels together.
    NonSeperable(NonSeperableBlendMode),
}

impl BlendMode {
    /// `Normal`: the new color covers the old one.
    pub fn normal() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Normal)
    }
    /// `Multiply`: darkens, black stays black, white changes nothing.
    pub fn multiply() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Multiply)
    }
    /// `Screen`: lightens, white stays white, black changes nothing.
    pub fn screen() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Screen)
    }
    /// `Overlay`: multiplies or screens depending on the old color.
    pub fn overlay() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Overlay)
    }
    /// `Darken`: keeps the darker color of each channel.
    pub fn darken() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Darken)
    }
    /// `Lighten`: keeps the lighter color of each channel.
    pub fn lighten() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Lighten)
    }
    /// `ColorDodge`: brightens the old color according to the new one.
    pub fn color_dodge() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::ColorDodge)
    }
    /// `ColorBurn`: darkens the old color according to the new one.
    pub fn color_burn() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::ColorBurn)
    }
    /// `HardLight`: multiplies or screens depending on the new color.
    pub fn hard_light() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::HardLight)
    }
    /// `SoftLight`: darkens or lightens softly depending on the new color.
    pub fn soft_light() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::SoftLight)
    }
    /// `Difference`: the absolute difference of the colors.
    pub fn difference() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Difference)
    }
    /// `Exclusion`: like `Difference` with less contrast.
    pub fn exclusion() -> BlendMode {
        BlendMode::Seperable(SeperableBlendMode::Exclusion)
    }
    /// `Hue`: hue of the new color, saturation and luminosity of the old.
    pub fn hue() -> BlendMode {
        BlendMode::NonSeperable(NonSeperableBlendMode::Hue)
    }
    /// `Saturation`: saturation of the new color, hue and luminosity of the old.
    pub fn saturation() -> BlendMode {
        BlendMode::NonSeperable(NonSeperableBlendMode::Saturation)
    }
    /// `Color`: hue and saturation of the new color, luminosity of the old.
    pub fn color() -> BlendMode {
        BlendMode::NonSeperable(NonSeperableBlendMode::Color)
    }
    /// `Luminosity`: luminosity of the new color, hue and saturation of the old.
    pub fn luminosity() -> BlendMode {
        BlendMode::NonSeperable(NonSeperableBlendMode::Luminosity)
    }
    /// The PDF name of the mode, as written to the `BM` key.
    pub fn get_id(&self) -> &'static str {
        use self::{BlendMode::*, NonSeperableBlendMode::*, SeperableBlendMode::*};

        match self {
            Seperable(s) => match s {
                Normal => "Normal",
                Multiply => "Multiply",
                Screen => "Screen",
                Overlay => "Overlay",
                Darken => "Darken",
                Lighten => "Lighten",
                ColorDodge => "ColorDodge",
                ColorBurn => "ColorBurn",
                HardLight => "HardLight",
                SoftLight => "SoftLight",
                Difference => "Difference",
                Exclusion => "Exclusion",
            },
            NonSeperable(n) => match n {
                Hue => "Hue",
                Saturation => "Saturation",
                Color => "Color",
                Luminosity => "Luminosity",
            },
        }
    }
}

/// Blend modes that compute each color channel on its own (ISO 32000-1, 11.3.5.2).
///
/// In the formulas `s` is the source (the new paint) and `b` the backdrop (what is already there),
/// both from 0.0 to 1.0 per channel.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeperableBlendMode {
    /// The source replaces the backdrop: `s`. The default.
    Normal,
    /// Multiplies the colors: `b * s`. The result is never lighter than either color.
    Multiply,
    /// Multiplies the complements, then complements the result: `b + s - b * s`. The result is
    /// never darker than either color.
    Screen,
    /// `Multiply` or `Screen` depending on the backdrop, so that highlights and shadows of the
    /// backdrop are kept. Same as `HardLight` with the colors swapped.
    Overlay,
    /// The darker of the two colors: `min(b, s)`.
    Darken,
    /// The lighter of the two colors: `max(b, s)`.
    Lighten,
    /// Brightens the backdrop to reflect the source. A black source changes nothing.
    ColorDodge,
    /// Darkens the backdrop to reflect the source. A white source changes nothing.
    ColorBurn,
    /// `Multiply` or `Screen` depending on the source, like a harsh spotlight.
    HardLight,
    /// Darkens or lightens depending on the source, like a diffused spotlight.
    SoftLight,
    /// The absolute difference: `abs(b - s)`. A white source inverts the backdrop.
    Difference,
    /// Like `Difference` with less contrast: `b + s - 2 * b * s`.
    Exclusion,
}

/// Blend modes that mix hue, saturation and luminosity, so they look at all color components
/// together (ISO 32000-1, 11.3.5.3).
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonSeperableBlendMode {
    /// Hue of the source, saturation and luminosity of the backdrop.
    Hue,
    /// Saturation of the source, hue and luminosity of the backdrop.
    Saturation,
    /// Hue and saturation of the source, luminosity of the backdrop.
    Color,
    /// Luminosity of the source, hue and saturation of the backdrop.
    Luminosity,
}

/// What to preserve when a color cannot be reproduced exactly on the output device (ISO 32000-1,
/// 8.6.5.8). It is the `RI` key and matters mostly for print.
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingIntent {
    /// Exact match of in-gamut colors, without adapting to the white of the paper. For logos and
    /// solid colors that must match across media.
    AbsoluteColorimetric,
    /// Exact match of in-gamut colors relative to the white of the medium. Suits vector graphics.
    RelativeColorimetric,
    /// Keeps colors saturated, even if they are not exact. Suits business graphics.
    Saturation,
    /// Keeps the look of the whole image pleasing, changing all colors a little. Suits photos.
    Perceptual,
}

impl RenderingIntent {
    /// The PDF name of the intent.
    pub fn get_id(&self) -> &'static str {
        use self::RenderingIntent::*;
        match self {
            AbsoluteColorimetric => "AbsoluteColorimetric",
            RelativeColorimetric => "RelativeColorimetric",
            Saturation => "Saturation",
            Perceptual => "Perceptual",
        }
    }
}

/// A mask that sets the opacity of what is painted, pixel by pixel, for example a fade out
/// (ISO 32000-1, 11.6.5). Bytes run from 0x00 (transparent) to 0xFF (opaque).
///
/// The type has no public constructor, and a soft mask in an [`ExtendedGraphicsState`] is not
/// written to the PDF. Images with an alpha channel get their mask automatically.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftMask {
    /// The mask samples.
    data: Vec<u8>,
    /// Bits per sample: 1 for black and white, 8 for grey, up to 16.
    bits_per_component: u8,
}

/// Where the values of a soft mask come from (the `S` key of a soft-mask dictionary).
#[derive(Debug, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoftMaskFunction {
    /// The mask is the alpha (opacity) of the mask group.
    GroupAlpha,
    /// The mask is the luminosity (brightness) of the mask group.
    GroupLuminosity,
}

#[cfg(test)]
mod tests {
    use super::*;

    // `to_array` emits reals, not rounded integers.
    #[test]
    fn rect_to_array_emits_real_not_rounded() {
        let rect = Rect::from_xywh(Pt(10.4), Pt(20.6), Pt(100.25), Pt(200.75));
        let arr = rect.to_array();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0], lopdf::Object::Real(10.4));
        assert_eq!(arr[1], lopdf::Object::Real(20.6));
        assert_eq!(arr[2], lopdf::Object::Real(100.25));
        assert_eq!(arr[3], lopdf::Object::Real(200.75));
        for o in &arr {
            assert!(matches!(o, lopdf::Object::Real(_)), "expected Real, got {o:?}");
        }
    }
}
