//! HVIF, the Haiku Vector Icon Format: an icon of a few hundred bytes -
//! styles (solid colours, gradients), paths (lines and cubic curves on a 64 x
//! 64 grid) and shapes (a style filling or stroking some paths, with a
//! transform). A shape can be limited to a range of sizes (its level of
//! detail: simple shapes for 16 px, details from 64 px) and be HINTED -
//! snapped to whole pixels when drawn small, which keeps a tiny icon crisp
//! where a scaled vector drawing goes soft.
//!
//! This module reads the format into data ([`Hvif::parse`]); azul-layout's
//! CPU renderer draws it at a given pixel size (the icon resolver does, on
//! demand). Written from the format as Haiku's `FlatIconImporter` reads it.

use alloc::vec::Vec;

/// What the file starts with.
pub const MAGIC: &[u8; 4] = b"ncif";

/// The size of the grid an icon is drawn on: its coordinates run 0..64.
pub const GRID: f32 = 64.0;

/// Why bytes are no HVIF icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HvifError {
    /// They do not start with `ncif`.
    NotHvif,
    /// They end in the middle of something.
    Truncated,
    /// A style, shape or transformer type this reader does not know.
    UnknownType(u8),
}

/// An RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HvifRgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// A 2D affine transform: `x' = a*x + c*y + e`, `y' = b*x + d*y + f` (SVG's
/// `matrix(a b c d e f)`; AGG's `sx shy shx sy tx ty`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HvifAffine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl HvifAffine {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    #[must_use]
    pub const fn translate(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    /// The point `(x, y)` through this transform.
    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a.mul_add(x, self.c * y) + self.e,
            self.b.mul_add(x, self.d * y) + self.f,
        )
    }
}

/// How a gradient spreads its colours (Haiku's gradient types; the
/// gradient's own space runs -64..64, placed by its transform).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HvifGradientKind {
    /// Along x.
    Linear,
    /// By the distance from the centre.
    Circular,
    /// By the larger of |x| and |y|.
    Diamond,
    /// By the angle around the centre.
    Conic,
    /// By |x| * |y|.
    Xy,
    /// By sqrt(|x| * |y|).
    SqrtXy,
}

/// One stop: where (0..=255 along the gradient) and its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HvifStop {
    pub offset: u8,
    pub color: HvifRgba,
}

/// A gradient fill.
#[derive(Debug, Clone, PartialEq)]
pub struct HvifGradient {
    pub kind: HvifGradientKind,
    /// From the gradient's space (-64..64) into the shape's.
    pub transform: HvifAffine,
    pub stops: Vec<HvifStop>,
}

/// How a shape is painted.
#[derive(Debug, Clone, PartialEq)]
pub enum HvifStyle {
    Solid(HvifRgba),
    Gradient(HvifGradient),
}

/// One point of a path: where it is and its two curve handles (equal to the
/// point for a corner).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HvifPathPoint {
    pub x: f32,
    pub y: f32,
    pub in_x: f32,
    pub in_y: f32,
    pub out_x: f32,
    pub out_y: f32,
}

/// A path: its points; the segment from one point to the next is the cubic
/// through the first's `out` and the next's `in` handle.
#[derive(Debug, Clone, PartialEq)]
pub struct HvifPath {
    pub closed: bool,
    pub points: Vec<HvifPathPoint>,
}

/// How a stroke's corners and ends look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HvifLineJoin {
    Miter,
    MiterRevert,
    Round,
    Bevel,
    MiterRound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HvifLineCap {
    Butt,
    Square,
    Round,
}

/// What a shape does to its paths before painting them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HvifTransformer {
    /// Another transform.
    Affine(HvifAffine),
    /// The outline grown (or, negative, shrunk) by `width`.
    Contour {
        width: f32,
        join: HvifLineJoin,
        miter_limit: f32,
    },
    /// A perspective transform (3 x 3); drawn as its affine part.
    Perspective([f32; 9]),
    /// The paths stroked `width` wide instead of filled.
    Stroke {
        width: f32,
        join: HvifLineJoin,
        cap: HvifLineCap,
        miter_limit: f32,
    },
}

/// A shape: a style painting some paths.
#[derive(Debug, Clone, PartialEq)]
pub struct HvifShape {
    /// An index into [`Hvif::styles`].
    pub style: usize,
    /// Indices into [`Hvif::paths`].
    pub paths: Vec<usize>,
    /// From the shape's space into the icon's grid.
    pub transform: HvifAffine,
    /// Snapped to whole pixels when drawn.
    pub hinting: bool,
    /// Drawn only at a scale (pixel size / 64) from `min_scale` up to
    /// `max_scale`.
    pub min_scale: f32,
    pub max_scale: f32,
    pub transformers: Vec<HvifTransformer>,
}

impl HvifShape {
    /// Whether the shape is drawn at `scale` (pixel size / 64): its level of
    /// detail.
    #[must_use]
    pub fn visible_at(&self, scale: f32) -> bool {
        scale >= self.min_scale && scale <= self.max_scale
    }

    /// The stroke it is drawn as, if a stroke transformer turns it into one.
    #[must_use]
    pub fn stroke(&self) -> Option<(f32, HvifLineJoin, HvifLineCap, f32)> {
        self.transformers.iter().find_map(|t| match t {
            HvifTransformer::Stroke {
                width,
                join,
                cap,
                miter_limit,
            } => Some((*width, *join, *cap, *miter_limit)),
            _ => None,
        })
    }
}

/// A parsed HVIF icon.
#[derive(Debug, Clone, PartialEq)]
pub struct Hvif {
    pub styles: Vec<HvifStyle>,
    pub paths: Vec<HvifPath>,
    pub shapes: Vec<HvifShape>,
}

/// Whether `bytes` look like an HVIF icon (start with its magic).
#[must_use]
pub fn is_hvif(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The byte reader.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Result<u8, HvifError> {
        let b = *self.bytes.get(self.at).ok_or(HvifError::Truncated)?;
        self.at += 1;
        Ok(b)
    }

    /// A coordinate: one byte (`b - 32`, whole units -32..95) or, with the
    /// top bit set, two (`(bits & 0x7fff) / 102 - 128`, 1/102 steps).
    fn coord(&mut self) -> Result<f32, HvifError> {
        let a = self.u8()?;
        if a & 0x80 == 0 {
            return Ok(f32::from(a) - 32.0);
        }
        let b = self.u8()?;
        let bits = (u16::from(a & 0x7f) << 8) | u16::from(b);
        Ok(f32::from(bits) / 102.0 - 128.0)
    }

    /// A 24-bit float: a sign bit, a 6-bit exponent biased by 32, a 17-bit
    /// mantissa.
    fn float24(&mut self) -> Result<f32, HvifError> {
        let (a, b, c) = (self.u8()?, self.u8()?, self.u8()?);
        let value = (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c);
        if value == 0 {
            return Ok(0.0);
        }
        let sign = u32::from(a & 0x80 != 0) << 31;
        #[allow(clippy::cast_sign_loss)] // exponent - 32 + 127 is within 95..158
        let exponent = ((i32::from((a & 0x7e) >> 1) - 32 + 127) as u32) << 23;
        let mantissa = (value & 0x01_ffff) << 6;
        Ok(f32::from_bits(sign | exponent | mantissa))
    }

    fn affine(&mut self) -> Result<HvifAffine, HvifError> {
        Ok(HvifAffine {
            a: self.float24()?,
            b: self.float24()?,
            c: self.float24()?,
            d: self.float24()?,
            e: self.float24()?,
            f: self.float24()?,
        })
    }

    fn color(&mut self, gray: bool, alpha: bool) -> Result<HvifRgba, HvifError> {
        let (r, g, b) = if gray {
            let v = self.u8()?;
            (v, v, v)
        } else {
            (self.u8()?, self.u8()?, self.u8()?)
        };
        let a = if alpha { self.u8()? } else { 255 };
        Ok(HvifRgba { r, g, b, a })
    }
}

const STYLE_SOLID: u8 = 1;
const STYLE_GRADIENT: u8 = 2;
const STYLE_SOLID_NO_ALPHA: u8 = 3;
const STYLE_GRAY: u8 = 4;
const STYLE_GRAY_NO_ALPHA: u8 = 5;

const GRADIENT_TRANSFORM: u8 = 1 << 1;
const GRADIENT_NO_ALPHA: u8 = 1 << 2;
const GRADIENT_GRAYS: u8 = 1 << 4;

const PATH_CLOSED: u8 = 1 << 1;
const PATH_COMMANDS: u8 = 1 << 2;
const PATH_NO_CURVES: u8 = 1 << 3;

const COMMAND_H_LINE: u8 = 0;
const COMMAND_V_LINE: u8 = 1;
const COMMAND_LINE: u8 = 2;

const SHAPE_PATH_SOURCE: u8 = 10;
const SHAPE_TRANSFORM: u8 = 1 << 1;
const SHAPE_HINTING: u8 = 1 << 2;
const SHAPE_LOD_SCALE: u8 = 1 << 3;
const SHAPE_TRANSFORMERS: u8 = 1 << 4;
const SHAPE_TRANSLATION: u8 = 1 << 5;

const TRANSFORMER_AFFINE: u8 = 20;
const TRANSFORMER_CONTOUR: u8 = 21;
const TRANSFORMER_PERSPECTIVE: u8 = 22;
const TRANSFORMER_STROKE: u8 = 23;

/// A level-of-detail scale byte: 0..=255 for 0..=4.
fn lod(byte: u8) -> f32 {
    f32::from(byte) / 63.75
}

const fn line_join(v: u8) -> HvifLineJoin {
    match v {
        1 => HvifLineJoin::MiterRevert,
        2 => HvifLineJoin::Round,
        3 => HvifLineJoin::Bevel,
        4 => HvifLineJoin::MiterRound,
        _ => HvifLineJoin::Miter,
    }
}

const fn line_cap(v: u8) -> HvifLineCap {
    match v {
        1 => HvifLineCap::Square,
        2 => HvifLineCap::Round,
        _ => HvifLineCap::Butt,
    }
}

impl Hvif {
    /// The icon in `bytes`.
    ///
    /// # Errors
    ///
    /// [`HvifError`] when the bytes are no HVIF icon or end early.
    pub fn parse(bytes: &[u8]) -> Result<Self, HvifError> {
        if !is_hvif(bytes) {
            return Err(HvifError::NotHvif);
        }
        let mut r = Reader { bytes, at: 4 };
        let mut styles = Vec::new();
        for _ in 0..r.u8()? {
            styles.push(Self::style(&mut r)?);
        }
        let mut paths = Vec::new();
        for _ in 0..r.u8()? {
            paths.push(Self::path(&mut r)?);
        }
        let mut shapes = Vec::new();
        for _ in 0..r.u8()? {
            shapes.push(Self::shape(&mut r)?);
        }
        Ok(Self {
            styles,
            paths,
            shapes,
        })
    }

    fn style(r: &mut Reader<'_>) -> Result<HvifStyle, HvifError> {
        Ok(match r.u8()? {
            STYLE_SOLID => HvifStyle::Solid(r.color(false, true)?),
            STYLE_SOLID_NO_ALPHA => HvifStyle::Solid(r.color(false, false)?),
            STYLE_GRAY => HvifStyle::Solid(r.color(true, true)?),
            STYLE_GRAY_NO_ALPHA => HvifStyle::Solid(r.color(true, false)?),
            STYLE_GRADIENT => {
                let kind = match r.u8()? {
                    0 => HvifGradientKind::Linear,
                    1 => HvifGradientKind::Circular,
                    2 => HvifGradientKind::Diamond,
                    3 => HvifGradientKind::Conic,
                    4 => HvifGradientKind::Xy,
                    5 => HvifGradientKind::SqrtXy,
                    other => return Err(HvifError::UnknownType(other)),
                };
                let flags = r.u8()?;
                let count = r.u8()?;
                let transform = if flags & GRADIENT_TRANSFORM != 0 {
                    r.affine()?
                } else {
                    HvifAffine::IDENTITY
                };
                let gray = flags & GRADIENT_GRAYS != 0;
                let alpha = flags & GRADIENT_NO_ALPHA == 0;
                let mut stops = Vec::with_capacity(usize::from(count));
                for _ in 0..count {
                    let offset = r.u8()?;
                    stops.push(HvifStop {
                        offset,
                        color: r.color(gray, alpha)?,
                    });
                }
                HvifStyle::Gradient(HvifGradient {
                    kind,
                    transform,
                    stops,
                })
            }
            other => return Err(HvifError::UnknownType(other)),
        })
    }

    fn path(r: &mut Reader<'_>) -> Result<HvifPath, HvifError> {
        let flags = r.u8()?;
        let count = usize::from(r.u8()?);
        let corner = |x: f32, y: f32| HvifPathPoint {
            x,
            y,
            in_x: x,
            in_y: y,
            out_x: x,
            out_y: y,
        };
        let mut points: Vec<HvifPathPoint> = Vec::with_capacity(count);
        if flags & PATH_COMMANDS != 0 {
            // Two bits a command, four a byte, lowest bits first.
            let mut commands = Vec::with_capacity(count.div_ceil(4));
            for _ in 0..count.div_ceil(4) {
                commands.push(r.u8()?);
            }
            for i in 0..count {
                let command = (commands[i / 4] >> ((i % 4) * 2)) & 0x03;
                let (last_x, last_y) = points.last().map_or((0.0, 0.0), |p| (p.x, p.y));
                points.push(match command {
                    COMMAND_H_LINE => corner(r.coord()?, last_y),
                    COMMAND_V_LINE => corner(last_x, r.coord()?),
                    COMMAND_LINE => {
                        let x = r.coord()?;
                        corner(x, r.coord()?)
                    }
                    _ => HvifPathPoint {
                        x: r.coord()?,
                        y: r.coord()?,
                        in_x: r.coord()?,
                        in_y: r.coord()?,
                        out_x: r.coord()?,
                        out_y: r.coord()?,
                    },
                });
            }
        } else if flags & PATH_NO_CURVES != 0 {
            for _ in 0..count {
                let x = r.coord()?;
                points.push(corner(x, r.coord()?));
            }
        } else {
            for _ in 0..count {
                points.push(HvifPathPoint {
                    x: r.coord()?,
                    y: r.coord()?,
                    in_x: r.coord()?,
                    in_y: r.coord()?,
                    out_x: r.coord()?,
                    out_y: r.coord()?,
                });
            }
        }
        Ok(HvifPath {
            closed: flags & PATH_CLOSED != 0,
            points,
        })
    }

    fn shape(r: &mut Reader<'_>) -> Result<HvifShape, HvifError> {
        let kind = r.u8()?;
        if kind != SHAPE_PATH_SOURCE {
            return Err(HvifError::UnknownType(kind));
        }
        let style = usize::from(r.u8()?);
        let mut paths = Vec::new();
        for _ in 0..r.u8()? {
            paths.push(usize::from(r.u8()?));
        }
        let flags = r.u8()?;
        let transform = if flags & SHAPE_TRANSFORM != 0 {
            r.affine()?
        } else if flags & SHAPE_TRANSLATION != 0 {
            let x = r.coord()?;
            HvifAffine::translate(x, r.coord()?)
        } else {
            HvifAffine::IDENTITY
        };
        let (min_scale, max_scale) = if flags & SHAPE_LOD_SCALE != 0 {
            (lod(r.u8()?), lod(r.u8()?))
        } else {
            (0.0, 4.0)
        };
        let mut transformers = Vec::new();
        if flags & SHAPE_TRANSFORMERS != 0 {
            for _ in 0..r.u8()? {
                transformers.push(match r.u8()? {
                    TRANSFORMER_AFFINE => HvifTransformer::Affine(r.affine()?),
                    TRANSFORMER_CONTOUR => HvifTransformer::Contour {
                        width: f32::from(r.u8()?) - 128.0,
                        join: line_join(r.u8()?),
                        miter_limit: f32::from(r.u8()?),
                    },
                    TRANSFORMER_PERSPECTIVE => {
                        let mut m = [0.0; 9];
                        for v in &mut m {
                            *v = r.float24()?;
                        }
                        HvifTransformer::Perspective(m)
                    }
                    TRANSFORMER_STROKE => {
                        let width = f32::from(r.u8()?) - 128.0;
                        let options = r.u8()?;
                        HvifTransformer::Stroke {
                            width,
                            join: line_join(options & 0x0f),
                            cap: line_cap(options >> 4),
                            miter_limit: f32::from(r.u8()?),
                        }
                    }
                    other => return Err(HvifError::UnknownType(other)),
                });
            }
        }
        Ok(HvifShape {
            style,
            paths,
            transform,
            hinting: flags & SHAPE_HINTING != 0,
            min_scale,
            max_scale,
            transformers,
        })
    }
}

#[cfg(test)]
#[path = "hvif_test.rs"]
mod tests;
