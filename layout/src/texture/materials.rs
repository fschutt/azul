//! The built-in MATERIALS: `builtin(leather)`, `builtin(aluminium)`,
//! `builtin(bluejeans)` and `builtin(vellum-leaf)` (cargo feature
//! `materials`).
//!
//! A material is a surface lit for the rect it fills, at every DEVICE pixel
//! of it - never a texture stretched over the rect. What it is made of is a
//! function of the position (`surface`: the procedural leather, brushed
//! aluminium, denim and vellum of the `AzDrive` design), or, as the
//! alternative source, a small colourless tile (luma, the leather's own
//! normal, occlusion) sampled with a cubic B-spline plus micro-detail finer
//! than a texel. No map carries colour: the colour is the tint
//! (`--azul-material-tint`), moved a little by the surface's luma.
//!
//! The light is the app's one light ([`azul_core::light::Light`]), or a
//! window's own [`MaterialLight`] where it set one. The shading (`shade`, and
//! its GPU twin `shader.glsl` in `gpu`) is physically based: GGX with
//! Smith-Schlick and Schlick's Fresnel (anisotropic for brushed metal), a
//! wrapped diffuse normalised so a flat surface shows its tint, a studio
//! room reflected - and for a metal the desktop behind the window
//! (`environment`) -, a filmic shoulder, sRGB and a half level of dither.
//!
//! The cursor's sheen is no part of a frame: it is a layer of its own over
//! the surface ([`sheen_sprite`]), a soft highlight the display list moves
//! with the cursor, so a moving cursor never draws a material again.
//!
//! The shape adds its own slope: a bevel along the edge (a cushion by
//! default, so it draws no line), a padded cushion and the pillow that rounds
//! a whole area (the `AzDrive` leather area). The edge is the rect's rounded
//! border, or any `clip-path` through its exact distance field (`distance`).
//!
//! An element tunes its material with custom properties
//! ([`MaterialParams::from_custom_properties`] lists them). Renders are
//! cached by everything they depend on; the grain (the expensive part) is
//! cached on its own in tiles of window pixels, so a new light or a growing
//! height relights without recomputing it. With a GPU context the frame is
//! drawn by the GPU instead (`gpu`).

// Per-channel loops over parallel colour arrays (`for c in 0..3`) read
// clearer indexed than zipped three ways.
#![allow(clippy::needless_range_loop)]

use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use azul_core::{
    light::Light,
    refany::RefAny,
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat},
};

use super::{
    environment::{self, Environment},
    transparent_pixel, Material, MaterialLight, MaterialRays,
};
use crate::callbacks::{RenderImageCallback, RenderImageCallbackExtras, RenderImageCallbackInfo};

mod distance;
mod gpu;
mod shade;
mod surface;

pub use distance::ShapeField;
pub use gpu::material_shader_compiles;

// -- The tiles -------------------------------------------------------------------------------

const LEATHER: &[u8] = include_bytes!("../../assets/textures/leather.azm");
const BLUEJEANS: &[u8] = include_bytes!("../../assets/textures/bluejeans.azm");
const ALUMINIUM: &[u8] = include_bytes!("../../assets/textures/aluminium.azm");

/// The packed (brotli) tile of `material`; empty for the vellum, whose tile
/// is the built-in vellum's ink.
#[must_use]
pub const fn packed_tile(material: Material) -> &'static [u8] {
    match material {
        Material::Leather => LEATHER,
        Material::BlueJeans => BLUEJEANS,
        Material::Aluminium => ALUMINIUM,
        Material::Vellum => &[],
    }
}

const fn material_index(material: Material) -> usize {
    match material {
        Material::Leather => 0,
        Material::BlueJeans => 1,
        Material::Aluminium => 2,
        Material::Vellum => 3,
    }
}

/// A decoded tile: what the shader reads per texel, row by row. It wraps.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialTile {
    /// The tile's side, texels.
    pub side: usize,
    /// The albedo as the tile stores it: sRGB, 0..1 (the ambient occlusion
    /// is baked in).
    pub albedo_srgb: Vec<[f32; 3]>,
    /// The albedo in linear light, what the shader reads.
    pub albedo: Vec<[f32; 3]>,
    /// The tangent-space normal's x and y, GL convention: +y points UP the
    /// image.
    pub normal: Vec<[f32; 2]>,
    /// Roughness, 0..1.
    pub rough: Vec<f32>,
    /// Ambient occlusion, 0..1 (1 where the tile baked it into the albedo).
    pub ao: Vec<f32>,
    /// The macro map: `macro_side` x `macro_side` luma multipliers (mean 1).
    pub macro_map: Vec<f32>,
    /// The macro map's side (0 = none).
    pub macro_side: usize,
    /// The macro map's period, tile texels (the source tile's real size).
    pub macro_period: f32,
}

/// The decoded tile of `material`: decoded once per process, `None` if it
/// does not decode (the material then draws nothing).
#[must_use]
pub fn tile(material: Material) -> Option<Arc<MaterialTile>> {
    static TILES: [OnceLock<Option<Arc<MaterialTile>>>; 4] =
        [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    TILES[material_index(material)]
        .get_or_init(|| decode_tile(packed_tile(material)).map(Arc::new))
        .clone()
}

/// A tile's packed bytes (brotli over [`parse_tile`]'s format) decoded.
#[must_use]
pub fn decode_tile(packed: &[u8]) -> Option<MaterialTile> {
    let mut raw = Vec::new();
    brotli_decompressor::BrotliDecompress(&mut &packed[..], &mut raw).ok()?;
    parse_tile(&raw)
}

const CHANNELS: usize = 7;
const CH_Y: usize = 0;
const CH_CO: usize = 1;
const CH_CG: usize = 2;
const CH_NX: usize = 3;
const CH_NY: usize = 4;
const CH_ROUGH: usize = 5;
const CH_AO: usize = 6;
/// A channel no plane and no constant gives: mid grey, no chroma, flat,
/// half rough, unoccluded.
const CHANNEL_DEFAULTS: [f32; CHANNELS] = [0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0];
const CONST_ROUGH_A: usize = 16;
const CONST_ROUGH_B: usize = 17;
const CONST_NORMAL_FROM_Y: usize = 18;

/// Little-endian reads over the unpacked bytes; `None` past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let s = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(s)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// One quantised plane's header.
#[derive(Debug, Clone, Copy)]
struct Plane {
    channel: usize,
    bits: u8,
    subsample: usize,
    lo: f32,
    hi: f32,
}

/// A tile from its unpacked bytes (`AZM1`, all little-endian):
///
/// ```text
/// "AZM1"  u16 side  u8 planes  u8 constants
/// planes    x { u8 channel, u8 bits, u8 subsample, u8 0, f32 lo, f32 hi }
/// constants x { u8 id, u8 0 0 0, f32 value }
/// u8 macro side M, u8 0 0 0, f32 macro period, f32 lo, f32 hi, M*M x u8
/// the planes' symbols, one byte each, (side / subsample)^2 per plane, row by row
/// ```
///
/// Channels 0..6 are Y, Co, Cg (`YCoCg` of the sRGB albedo), normal x, normal
/// y, roughness, ambient occlusion; a symbol `q` of a `bits` plane is
/// `lo + q / (2^bits - 1) * (hi - lo)`, a subsampled plane is bilinearly
/// upsampled (it wraps). A constant id 0..6 fills its channel; 16 / 17 make
/// the roughness `a + b * Y`; 18 makes the normal the gradient of Y times
/// its value. The macro map's symbols are `lo + q / 255 * (hi - lo)`.
#[must_use]
pub fn parse_tile(raw: &[u8]) -> Option<MaterialTile> {
    let mut r = Reader { bytes: raw, at: 0 };
    if r.take(4)? != b"AZM1" {
        return None;
    }
    let side = usize::from(r.u16()?);
    let plane_count = r.u8()?;
    let const_count = r.u8()?;
    if side == 0 || side > 1024 {
        return None;
    }
    let mut planes = Vec::with_capacity(usize::from(plane_count));
    for _ in 0..plane_count {
        let channel = usize::from(r.u8()?);
        let bits = r.u8()?;
        let subsample = usize::from(r.u8()?);
        r.take(1)?;
        let lo = r.f32()?;
        let hi = r.f32()?;
        if channel >= CHANNELS
            || !(1..=8).contains(&bits)
            || subsample == 0
            || !side.is_multiple_of(subsample)
        {
            return None;
        }
        planes.push(Plane {
            channel,
            bits,
            subsample,
            lo,
            hi,
        });
    }
    let mut constants: [Option<f32>; 32] = [None; 32];
    for _ in 0..const_count {
        let id = usize::from(r.u8()?);
        r.take(3)?;
        let value = r.f32()?;
        if let Some(slot) = constants.get_mut(id) {
            *slot = Some(value);
        }
    }
    let macro_side = usize::from(r.u8()?);
    r.take(3)?;
    let macro_period = r.f32()?;
    let macro_lo = r.f32()?;
    let macro_hi = r.f32()?;
    let macro_map: Vec<f32> = r
        .take(macro_side * macro_side)?
        .iter()
        .map(|q| macro_lo + f32::from(*q) / 255.0 * (macro_hi - macro_lo))
        .collect();

    let texels = side * side;
    let mut channels: Vec<Vec<f32>> = (0..CHANNELS)
        .map(|c| vec![constants[c].unwrap_or(CHANNEL_DEFAULTS[c]); texels])
        .collect();
    for plane in &planes {
        let grid = side / plane.subsample;
        let levels = f32::from((1_u16 << plane.bits) - 1);
        let values: Vec<f32> = r
            .take(grid * grid)?
            .iter()
            .map(|q| plane.lo + f32::from(*q) / levels * (plane.hi - plane.lo))
            .collect();
        channels[plane.channel] = if plane.subsample == 1 {
            values
        } else {
            upsample_wrapping(&values, grid, plane.subsample, side)
        };
    }
    if let (Some(a), Some(b)) = (constants[CONST_ROUGH_A], constants[CONST_ROUGH_B]) {
        let rough: Vec<f32> = channels[CH_Y]
            .iter()
            .map(|y| (a + b * y).clamp(0.0, 1.0))
            .collect();
        channels[CH_ROUGH] = rough;
    }
    if let Some(k) = constants[CONST_NORMAL_FROM_Y] {
        let (nx, ny) = normals_from_luma(&channels[CH_Y], side, k);
        channels[CH_NX] = nx;
        channels[CH_NY] = ny;
    }

    let mut albedo_srgb = Vec::with_capacity(texels);
    let mut albedo = Vec::with_capacity(texels);
    let mut normal = Vec::with_capacity(texels);
    for i in 0..texels {
        let y = channels[CH_Y][i];
        let co = channels[CH_CO][i];
        let cg = channels[CH_CG][i];
        let t = y - cg;
        let srgb = [
            (t + co).clamp(0.0, 1.0),
            (y + cg).clamp(0.0, 1.0),
            (t - co).clamp(0.0, 1.0),
        ];
        albedo_srgb.push(srgb);
        albedo.push(srgb.map(srgb_to_linear));
        normal.push([channels[CH_NX][i], channels[CH_NY][i]]);
    }
    let ao = core::mem::take(&mut channels[CH_AO]);
    let rough = core::mem::take(&mut channels[CH_ROUGH]);
    Some(MaterialTile {
        side,
        albedo_srgb,
        albedo,
        normal,
        rough,
        ao,
        macro_map,
        macro_side,
        macro_period,
    })
}

/// A `grid` x `grid` plane upsampled by `factor` to `side` x `side`,
/// bilinearly at texel centres, wrapping (the packer's twin: rows first).
fn upsample_wrapping(values: &[f32], grid: usize, factor: usize, side: usize) -> Vec<f32> {
    let taps: Vec<(usize, usize, f32)> = (0..side)
        .map(|i| {
            let c = (i as f32 + 0.5) / factor as f32 - 0.5;
            let f = c.floor();
            let i0 = (f as i64).rem_euclid(grid as i64) as usize;
            (i0, (i0 + 1) % grid, c - f)
        })
        .collect();
    let mut out = vec![0.0; side * side];
    for (y, &(y0, y1, ty)) in taps.iter().enumerate() {
        for (x, &(x0, x1, tx)) in taps.iter().enumerate() {
            let r0 = values[y0 * grid + x0] * (1.0 - ty) + values[y1 * grid + x0] * ty;
            let r1 = values[y0 * grid + x1] * (1.0 - ty) + values[y1 * grid + x1] * ty;
            out[y * side + x] = r0 * (1.0 - tx) + r1 * tx;
        }
    }
    out
}

/// The normal of a surface whose height is `k` times the luma (central
/// differences, wrapping; GL convention).
fn normals_from_luma(luma: &[f32], side: usize, k: f32) -> (Vec<f32>, Vec<f32>) {
    let mut nx = vec![0.0; side * side];
    let mut ny = vec![0.0; side * side];
    for y in 0..side {
        let up = (y + side - 1) % side;
        let down = (y + 1) % side;
        for x in 0..side {
            let left = (x + side - 1) % side;
            let right = (x + 1) % side;
            let gx = -k * (luma[y * side + right] - luma[y * side + left]) / 2.0;
            let gy = k * (luma[down * side + x] - luma[up * side + x]) / 2.0;
            let len = (gx * gx + gy * gy + 1.0).sqrt();
            nx[y * side + x] = gx / len;
            ny[y * side + x] = gy / len;
        }
    }
    (nx, ny)
}

/// sRGB-encoded 0..1 to linear light.
#[must_use]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light 0..1 to sRGB-encoded.
#[must_use]
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}


// -- The colourless tiles --------------------------------------------------------------------

/// The minimum a material's tile needs, and no colour: R the luma (spread
/// over 0..255), G / B the tile's own normal (the leather's; 128 = level for
/// the others), A the occlusion; plus the source's large-scale mottling.
/// The GPU uploads exactly these bytes, so both sample the same tile.
#[derive(Debug, Clone, PartialEq)]
pub struct ColourlessTile {
    /// The tile's side, texels (it wraps).
    pub side: usize,
    /// RGBA bytes, row by row.
    pub rgba: Vec<[u8; 4]>,
    /// The mottling map's side (0 = none).
    pub macro_side: usize,
    /// The mottling, one byte per texel: `0.82 + 0.36 * q / 255` multiplies
    /// the luma.
    pub macro_q: Vec<u8>,
    /// The mottling's period, logical px.
    pub macro_period: f32,
}

impl ColourlessTile {
    /// The colourless half of a decoded tile: its sRGB luma, and the normal
    /// and occlusion when `own_normal` (the leather's; the others derive
    /// their slope from the luma).
    #[must_use]
    pub fn from_tile(tile: &MaterialTile, own_normal: bool) -> Self {
        let luma: Vec<f32> = tile
            .albedo_srgb
            .iter()
            .map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2])
            .collect();
        let lo = luma.iter().copied().fold(f32::MAX, f32::min);
        let hi = luma.iter().copied().fold(f32::MIN, f32::max);
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let rgba = (0..tile.side * tile.side)
            .map(|i| {
                let r = byte((luma[i] - lo) / (hi - lo).max(1e-6));
                if own_normal {
                    [
                        r,
                        byte(tile.normal[i][0] * 0.5 + 0.5),
                        byte(tile.normal[i][1] * 0.5 + 0.5),
                        byte(tile.ao[i]),
                    ]
                } else {
                    [r, 128, 128, 255]
                }
            })
            .collect();
        Self {
            side: tile.side,
            rgba,
            macro_side: tile.macro_side,
            macro_q: tile
                .macro_map
                .iter()
                .map(|v| byte((v - 0.82) / 0.36))
                .collect(),
            macro_period: tile.macro_period,
        }
    }

    /// The vellum's tile: the built-in vellum's ink, inverted to a luma.
    #[must_use]
    pub fn vellum() -> Self {
        let paper = super::builtin_raw_texture(super::BuiltinTexture::Vellum);
        let RawImageData::U8(bytes) = &paper.pixels else {
            return Self {
                side: 1,
                rgba: vec![[255, 128, 128, 255]],
                macro_side: 0,
                macro_q: Vec::new(),
                macro_period: 0.0,
            };
        };
        let grey: Vec<f32> = bytes.as_ref().chunks_exact(4).map(|p| f32::from(p[0])).collect();
        let lo = grey.iter().copied().fold(f32::MAX, f32::min);
        let hi = grey.iter().copied().fold(f32::MIN, f32::max);
        Self {
            side: paper.width,
            rgba: grey
                .iter()
                .map(|g| [(((g - lo) / (hi - lo).max(1e-6)) * 255.0).round() as u8, 128, 128, 255])
                .collect(),
            macro_side: 0,
            macro_q: Vec::new(),
            macro_period: 0.0,
        }
    }

    /// The tile at `uv` (tile widths), linear filtering at texel centres,
    /// wrapping: what a GPU's `texture()` returns, 0..1 per channel.
    #[allow(clippy::many_single_char_names)] // the four taps a, b, c, d and their weights
    pub(crate) fn bilinear(&self, uv: [f32; 2]) -> [f32; 4] {
        let n = self.side;
        let u = uv[0] * n as f32 - 0.5;
        let v = uv[1] * n as f32 - 0.5;
        let (fx, fy) = (u.floor(), v.floor());
        let (tx, ty) = (u - fx, v - fy);
        let x0 = wrap(fx as i64, n);
        let y0 = wrap(fy as i64, n);
        let (x1, y1) = ((x0 + 1) % n, (y0 + 1) % n);
        let at = |x: usize, y: usize| self.rgba[y * n + x];
        let mut out = [0.0; 4];
        for c in 0..4 {
            let a = f32::from(at(x0, y0)[c]) * (1.0 - tx) + f32::from(at(x1, y0)[c]) * tx;
            let b = f32::from(at(x0, y1)[c]) * (1.0 - tx) + f32::from(at(x1, y1)[c]) * tx;
            out[c] = (a * (1.0 - ty) + b * ty) / 255.0;
        }
        out
    }

    /// The mottling at `uv` (periods), linear and wrapping, 0..1.
    #[allow(clippy::many_single_char_names)] // the four taps a, b, c, d and their weights
    pub(crate) fn macro_bilinear(&self, uv: [f32; 2]) -> f32 {
        let n = self.macro_side;
        if n == 0 {
            return 0.5;
        }
        let u = uv[0] * n as f32 - 0.5;
        let v = uv[1] * n as f32 - 0.5;
        let (fx, fy) = (u.floor(), v.floor());
        let (tx, ty) = (u - fx, v - fy);
        let x0 = wrap(fx as i64, n);
        let y0 = wrap(fy as i64, n);
        let (x1, y1) = ((x0 + 1) % n, (y0 + 1) % n);
        let at = |x: usize, y: usize| f32::from(self.macro_q[y * n + x]) / 255.0;
        let a = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
        let b = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
        a * (1.0 - ty) + b * ty
    }
}

/// `i` wrapped into `0..side`.
const fn wrap(i: i64, side: usize) -> usize {
    i.rem_euclid(side as i64) as usize
}

/// The colourless tile of `material`, made once per process (`None` when
/// the material's tile does not decode).
#[must_use]
pub fn colourless_tile(material: Material) -> Option<Arc<ColourlessTile>> {
    static TILES: [OnceLock<Option<Arc<ColourlessTile>>>; 4] =
        [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    TILES[material_index(material)]
        .get_or_init(|| match material {
            Material::Vellum => Some(Arc::new(ColourlessTile::vellum())),
            _ => tile(material).map(|t| Arc::new(ColourlessTile::from_tile(&t, material == Material::Leather))),
        })
        .clone()
}

// -- How an element wears its material ----------------------------------------------------------

/// What a material's surface is made from.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum MaterialSource {
    /// A function of the position: no data, detail at any resolution (the
    /// default).
    Procedural,
    /// The material's colourless tile, upsampled smoothly, with
    /// micro-detail.
    Tile,
    /// No grain at all: the material's colour, roughness and metal on a
    /// perfectly smooth surface (a lacquer; what shape-only tests draw).
    Smooth,
}

/// Which edge a bevel follows.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BevelFollow {
    /// The rect's border, rounded by its `border-radius` (analytic).
    Border,
    /// The element's `clip-path`, through its distance field.
    Shape,
}

/// A bevel along the edge (Photoshop's Bevel & Emboss): a height rising
/// `width` px in from the edge.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Bevel {
    /// How far in from the edge it rises, logical px (0 = none).
    pub width: f32,
    /// How high it rises, as a share of half its width.
    pub depth: f32,
    /// 0 = a chisel (straight, with a crease at its crown), 1 = a cushion
    /// (no slope at the edge or the crown, so it draws no line).
    pub softness: f32,
    /// Sunk in (a well, a pressed button) rather than raised.
    pub inset: bool,
    /// The edge it follows.
    pub follow: BevelFollow,
}

impl Bevel {
    /// No bevel.
    pub const NONE: Self = Self {
        width: 0.0,
        depth: 1.0,
        softness: 1.0,
        inset: false,
        follow: BevelFollow::Border,
    };
}

/// The pillow: the whole surface rounded up from its edges, its contours
/// running parallel to every edge and round at every corner.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Pillow {
    /// How far in from the edges it rolls, logical px at most (0 = none);
    /// never more than the surface's height allows for a level crown.
    pub width: f32,
    /// Its height as a share of its width.
    pub depth: f32,
    /// How much of the roll the bottom edge keeps (1 = all of it; less makes
    /// a short seam into a trim below).
    pub foot: f32,
}

impl Pillow {
    /// No pillow.
    pub const NONE: Self = Self {
        width: 0.0,
        depth: 0.0,
        foot: 1.0,
    };
}

/// The light a strip of the accent throws onto a surface from its foot.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Glow {
    /// Its colour, linear.
    pub color: [f32; 3],
    /// Its strength.
    pub strength: f32,
    /// How far up it reaches, logical px.
    pub reach: f32,
}

/// How one element wears its material: the material's own defaults
/// ([`Self::of`]) overridden by the element's custom properties
/// ([`Self::from_custom_properties`]).
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MaterialParams {
    /// What the surface is made from.
    pub source: MaterialSource,
    /// The colour, linear RGB.
    pub tint: [f32; 3],
    /// The roughness the surface spreads over: smoothest, roughest.
    pub roughness: [f32; 2],
    /// How strongly the grain's relief tilts the light.
    pub relief: f32,
    /// How far the surface's luma moves the tint (0 = one flat colour).
    pub variation: f32,
    /// The cursor's sheen's strength (0 = none).
    pub sheen: f32,
    /// How high over the cursor the sheen's light hangs, logical px.
    pub sheen_size: f32,
    /// The leather's grain cell, logical px.
    pub grain: f32,
    /// The pattern's size: 2 = everything twice as large.
    pub scale: f32,
    /// The bevel along the edge.
    pub bevel: Bevel,
    /// The padding: a soft rise over a wider band (width, depth px).
    pub cushion: [f32; 2],
    /// The pillow over the whole surface.
    pub pillow: Pillow,
    /// The strength of everything 3D - relief, bevels, cushion, pillow,
    /// sheen - together: 1 is the tuned look.
    pub depth: f32,
    /// The accent's glow onto the surface, if any.
    pub glow: Option<Glow>,
    /// The rect's corner radii, logical px: top-left, top-right,
    /// bottom-right, bottom-left (its `border-radius`).
    pub radii: [f32; 4],
}

impl MaterialParams {
    /// `material`'s defaults: the `AzDrive` design's settings for it.
    #[must_use]
    pub fn of(material: Material) -> Self {
        let (tint, roughness, relief, variation, sheen, sheen_size, bevel) = match material {
            Material::Leather => ([0xE4, 0xE2, 0xDD], [0.55, 0.66], 1.0, 0.3, 0.4, 240.0, 0.0),
            Material::Aluminium => ([0xE9, 0xC6, 0x72], [0.14, 0.3], 0.9, 0.46, 0.55, 150.0, 1.0),
            Material::BlueJeans => ([0x2D, 0x40, 0x61], [0.78, 0.96], 1.0, 0.55, 0.12, 220.0, 2.0),
            Material::Vellum => ([0xEF, 0xE3, 0xC6], [0.72, 0.94], 0.7, 0.4, 0.08, 240.0, 0.0),
        };
        Self {
            source: MaterialSource::Procedural,
            tint: srgb8_to_linear(tint),
            roughness,
            relief,
            variation,
            sheen,
            sheen_size,
            grain: if material == Material::Leather { 1.7 } else { 5.2 },
            scale: 1.0,
            bevel: Bevel {
                width: bevel,
                depth: 1.0,
                softness: 1.0,
                inset: false,
                follow: BevelFollow::Border,
            },
            cushion: [0.0, 0.0],
            pillow: Pillow::NONE,
            depth: 1.0,
            glow: None,
            radii: [0.0; 4],
        }
    }

    /// Set the tint from sRGB bytes.
    pub fn set_tint_srgb(&mut self, srgb: [u8; 3]) {
        self.tint = srgb8_to_linear(srgb);
    }

    /// `material`'s defaults, overridden by the element's custom properties
    /// (`get` answers a name without its dashes), with the rect's `radii`:
    ///
    /// - `--azul-material-tint: <colour>`; `--azul-material-source: procedural | tile | smooth`;
    /// - `--azul-material-roughness: 0.55 0.66` (or one number), `--azul-material-relief`
    ///   (also `--azul-relief`), `--azul-material-variation`, `--azul-material-grain: 1.7px`,
    ///   `--azul-material-scale`, `--azul-material-depth` (the 3D strength, 1 = tuned);
    /// - `--azul-material-sheen`, `--azul-material-sheen-size: 240px`, `--azul-sheen-scale`;
    /// - `--azul-bevel: 2px` or the shorthand `bevel(2px 1 1 inset)` (width, depth, softness,
    ///   style), `--azul-bevel-depth`, `--azul-bevel-softness`, `--azul-bevel-style: raised |
    ///   inset` (also `--azul-inset: 1`), `--azul-bevel-follow: border | shape`;
    /// - `--azul-cushion: 12px 2px` (width, depth);
    /// - `--azul-pillow: 76px .16` (width, depth ratio, and optionally how much the foot keeps);
    /// - `--azul-material-glow: <colour> <strength> <reach px>`.
    pub fn from_custom_properties(
        material: Material,
        get: impl Fn(&str) -> Option<String>,
        radii: [f32; 4],
    ) -> Self {
        let mut p = Self {
            radii,
            ..Self::of(material)
        };
        let number = |name: &str| get(name).as_deref().and_then(parse_length);
        let numbers = |name: &str| get(name).map(|v| parse_numbers(&v)).unwrap_or_default();
        if let Some(c) = get("azul-material-tint").and_then(|v| parse_colour(&v)) {
            p.tint = srgb8_to_linear(c);
        }
        if let Some(v) = get("azul-material-source") {
            match v.trim().to_ascii_lowercase().as_str() {
                "procedural" => p.source = MaterialSource::Procedural,
                "tile" => p.source = MaterialSource::Tile,
                "smooth" | "none" => p.source = MaterialSource::Smooth,
                _ => {}
            }
        }
        match numbers("azul-material-roughness").as_slice() {
            [one] => p.roughness = [one.clamp(0.0, 1.0); 2],
            [lo, hi, ..] => p.roughness = [lo.clamp(0.0, 1.0), hi.clamp(0.0, 1.0)],
            [] => {}
        }
        if let Some(v) = number("azul-material-relief").or_else(|| number("azul-relief")) {
            p.relief = v.max(0.0);
        }
        if let Some(v) = number("azul-material-variation") {
            p.variation = v.max(0.0);
        }
        if let Some(v) = number("azul-material-grain") {
            p.grain = v.max(0.1);
        }
        if let Some(v) = number("azul-material-scale") {
            if v > 0.0 {
                p.scale = v;
            }
        }
        if let Some(v) = number("azul-material-depth") {
            p.depth = v.max(0.0);
        }
        if let Some(v) = number("azul-material-sheen") {
            p.sheen = v.max(0.0);
        }
        if let Some(v) = number("azul-material-sheen-size") {
            p.sheen_size = v.max(1.0);
        }
        if let Some(v) = number("azul-sheen-scale") {
            p.sheen *= v.max(0.0);
        }
        if let Some(v) = get("azul-bevel") {
            parse_bevel(&v, &mut p.bevel);
        }
        if let Some(v) = number("azul-bevel-depth") {
            p.bevel.depth = v.max(0.0);
        }
        if let Some(v) = number("azul-bevel-softness") {
            p.bevel.softness = v.clamp(0.0, 1.0);
        }
        if let Some(v) = get("azul-bevel-style").as_deref().and_then(parse_flag) {
            p.bevel.inset = v;
        }
        if let Some(v) = get("azul-inset").as_deref().and_then(parse_flag) {
            p.bevel.inset = v;
        }
        if let Some(v) = get("azul-bevel-follow") {
            match v.trim().to_ascii_lowercase().as_str() {
                "shape" | "clip-path" | "mask" => p.bevel.follow = BevelFollow::Shape,
                "border" | "border-radius" => p.bevel.follow = BevelFollow::Border,
                _ => {}
            }
        }
        if let [w, d, ..] = numbers("azul-cushion").as_slice() {
            p.cushion = [w.max(0.0), d.max(0.0)];
        }
        match numbers("azul-pillow").as_slice() {
            [w, d, foot, ..] => {
                p.pillow = Pillow {
                    width: w.max(0.0),
                    depth: d.max(0.0),
                    foot: foot.max(0.0),
                }
            }
            [w, d] => {
                p.pillow = Pillow {
                    width: w.max(0.0),
                    depth: d.max(0.0),
                    foot: 1.0,
                }
            }
            _ => {}
        }
        if let Some(v) = get("azul-material-glow") {
            p.glow = parse_glow(&v);
        }
        p
    }
}

impl Default for MaterialParams {
    fn default() -> Self {
        Self::of(Material::Leather)
    }
}

fn srgb8_to_linear(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| srgb_to_linear(f32::from(v) / 255.0))
}

/// `4`, `4px`, ` 4.5px ` -> the number; anything else `None`.
fn parse_length(value: &str) -> Option<f32> {
    let v = value.trim();
    let v = v.strip_suffix("px").unwrap_or(v).trim();
    v.parse::<f32>().ok().filter(|f| f.is_finite())
}

/// Every number in a space-separated value (`76px .16` -> [76, 0.16]).
fn parse_numbers(value: &str) -> Vec<f32> {
    value.split_whitespace().filter_map(parse_length).collect()
}

fn parse_colour(value: &str) -> Option<[u8; 3]> {
    azul_css::props::basic::color::parse_css_color(value.trim())
        .ok()
        .map(|c| [c.r, c.g, c.b])
}

/// A custom property read as a switch: `inset` / `1` / `on`, or `raised` /
/// `0` / `off`.
fn parse_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "inset" => Some(true),
        "0" | "false" | "no" | "off" | "raised" | "none" => Some(false),
        _ => None,
    }
}

/// `--azul-bevel`: a width (`2px`), or the shorthand `bevel(2px 1 .7 inset)`
/// - width, depth, softness, style, each optional after the width.
fn parse_bevel(value: &str, bevel: &mut Bevel) {
    let v = value.trim();
    let inner = v
        .strip_prefix("bevel(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or(v);
    let mut numbers = Vec::new();
    for word in inner.split_whitespace() {
        if let Some(n) = parse_length(word) {
            numbers.push(n);
        } else if let Some(inset) = parse_flag(word) {
            bevel.inset = inset;
        }
    }
    if let Some(w) = numbers.first() {
        bevel.width = w.max(0.0);
    }
    if let Some(d) = numbers.get(1) {
        bevel.depth = d.max(0.0);
    }
    if let Some(s) = numbers.get(2) {
        bevel.softness = s.clamp(0.0, 1.0);
    }
}

/// `--azul-material-glow: #4C79E5 0.12 10px` - the colour, its strength and
/// its reach.
fn parse_glow(value: &str) -> Option<Glow> {
    let words: Vec<&str> = value.split_whitespace().collect();
    let (colour, rest) = words.split_first()?;
    let c = parse_colour(colour)?;
    let strength = rest.first().and_then(|w| parse_length(w)).unwrap_or(0.12);
    let reach = rest.get(1).and_then(|w| parse_length(w)).unwrap_or(10.0);
    (strength > 0.0 && reach > 0.0).then(|| Glow {
        color: srgb8_to_linear(c),
        strength,
        reach,
    })
}

// -- What a surface is lit by ---------------------------------------------------------------------

/// Everything around a surface that its look depends on besides its own
/// params: the light, where it sits, the room it reflects and
/// the shape it is cut to.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceScene {
    /// Where the light comes from (the app's light, or the window's own).
    pub light: Light,
    /// The light's colour, linear (a warm white by default).
    pub light_color: [f32; 3],
    /// The light's strength (1 = a flat surface shows its tint).
    pub intensity: f32,
    /// The share of the light that comes from everywhere.
    pub ambient: f32,
    /// The window's god rays (off by default; drawn on the CPU only).
    pub rays: MaterialRays,
    /// The surface's top-left in the window, logical px: the grain is laid
    /// in window space, so neighbouring surfaces are one hide.
    pub window_offset: [f32; 2],
    /// The surface's top-left on the desktop, desktop points: where a point
    /// light and the wallpaper are seen from.
    pub desktop_origin: [f32; 2],
    /// The screen the surface is on, desktop points: x, y, width, height.
    pub screen: [f32; 4],
    /// What a metal reflects.
    pub environment: Environment,
    /// The shape's distance field, for a bevel that follows the shape.
    pub shape: Option<Arc<ShapeField>>,
}

impl Default for SurfaceScene {
    fn default() -> Self {
        Self {
            light: Light::DEFAULT,
            light_color: [1.0, 0.975, 0.94],
            intensity: 1.0,
            ambient: 0.45,
            rays: MaterialRays::OFF,
            window_offset: [0.0, 0.0],
            desktop_origin: [0.0, 0.0],
            screen: [0.0, 0.0, 1920.0, 1080.0],
            environment: Environment::default(),
            shape: None,
        }
    }
}

impl SurfaceScene {
    /// The scene under a window's own `light`: its colour, strength, ambient
    /// share and rays, and its direction as the sun.
    #[must_use]
    pub fn with_material_light(mut self, light: &MaterialLight) -> Self {
        self.light = Light::sun(light.azimuth_deg, light.elevation_deg);
        self.apply_light_colour(light);
        self
    }

    /// Take `light`'s colour, strength, ambient share and rays (not its
    /// direction).
    pub fn apply_light_colour(&mut self, light: &MaterialLight) {
        let c = light.color;
        self.light_color = [c.r, c.g, c.b].map(|v| srgb_to_linear(f32::from(v) / 255.0));
        self.intensity = light.intensity.max(0.0);
        self.ambient = light.ambient.clamp(0.0, 1.0);
        self.rays = light.rays;
    }
}

/// Whether `material` is a metal (it reflects its colour and the room) and
/// whether it is brushed (anisotropic).
const fn metal_of(material: Material) -> (f32, bool) {
    match material {
        Material::Aluminium => (1.0, true),
        _ => (0.0, false),
    }
}

// -- The cursor's sheen -------------------------------------------------------------------------

/// The cursor's sheen on a material, as the layer the display list draws
/// over it ([`sheen_sprite`]): one colour drawn OVER the material at an
/// alpha that falls with the distance from the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct SheenSprite {
    /// How far the highlight reaches from the cursor, logical px, along x
    /// and along y (a brushed metal's highlight is an ellipse).
    pub radius: [f32; 2],
    /// The highlight's colour, sRGB, opaque.
    pub color: azul_css::props::basic::ColorU,
    /// Its alpha (0..1, at full strength: the layer's opacity is the sheen's
    /// eased amount) at distances from the cursor, as shares of the radius
    /// from 0 (the cursor) to 1 (the reach). They lie closer together near
    /// the cursor, where the lobe bends most.
    pub stops: Vec<(f32, f32)>,
}

/// How many stops the highlight's gradient has.
const SHEEN_STOPS: usize = 16;

impl SheenSprite {
    /// The highlight as a radial gradient filling a rect of twice its radius,
    /// centred on that rect.
    #[must_use]
    pub fn gradient(&self) -> azul_css::props::style::background::RadialGradient {
        use azul_css::props::{
            basic::{ColorU, PercentageValue},
            style::background::{
                BackgroundPositionHorizontal, BackgroundPositionVertical, ExtendMode,
                NormalizedLinearColorStop, RadialGradient, RadialGradientSize, Shape,
                StyleBackgroundPosition,
            },
        };

        let stops: Vec<NormalizedLinearColorStop> = self
            .stops
            .iter()
            .map(|&(at, alpha)| {
                NormalizedLinearColorStop::new(
                    PercentageValue::new(at * 100.0),
                    ColorU {
                        a: (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
                        ..self.color
                    },
                )
            })
            .collect();
        RadialGradient {
            shape: if (self.radius[0] - self.radius[1]).abs() < 0.5 {
                Shape::Circle
            } else {
                Shape::Ellipse
            },
            size: RadialGradientSize::ClosestSide,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            extend_mode: ExtendMode::Clamp,
            stops: stops.into(),
        }
    }
}

/// The cursor's sheen on `material` worn as `params`, in `light`'s colour:
/// the highlight the display list draws over the material while the cursor
/// is on it, `None` for a material without a sheen.
///
/// The sheen is the material's second GGX lobe: a light `sheen_size` px over
/// the cursor, wider than the surface's own specular (alpha 1.8 times for a
/// dielectric, 1.1 times for a metal), on a satin base of 25% (a metal's own
/// colour), `sheen x 3 x depth` strong. On a flat surface it depends on the
/// distance to the cursor alone. Drawn over the material's colour (its tint
/// under the light) in one colour at a falling alpha, it brightens the
/// surface as the lobe does - the grain shows through, as it does under the
/// lobe.
// The lobe keeps the shader's letters (shader.glsl): a, n, k...
#[allow(clippy::many_single_char_names)]
#[must_use]
pub fn sheen_sprite(
    material: Material,
    params: &MaterialParams,
    light: &MaterialLight,
) -> Option<SheenSprite> {
    let strength = params.sheen * 3.0 * params.depth;
    if strength <= 0.0 {
        return None;
    }
    let (metal, aniso) = metal_of(material);
    let light_color = [light.color.r, light.color.g, light.color.b]
        .map(|v| srgb_to_linear(f32::from(v) / 255.0) * light.intensity.max(0.0));
    // The lobe's roughness: the middle of the range the surface spreads over.
    let rough = 0.5 * (params.roughness[0] + params.roughness[1]);
    let a = (rough * rough).max(0.002);
    let am = (a * surface::mix(1.8, 1.1, metal)).max(0.02);
    let base = params
        .tint
        .map(|c| surface::mix(0.25, surface::mix(0.04, c, metal), metal));
    let size = params.sheen_size.max(1.0);
    // The light the lobe adds at `(dx, dy)` from the cursor, linear.
    let added = |dx: f32, dy: f32| -> [f32; 3] {
        let n = [0.0, 0.0, 1.0];
        let lm = shade::normalize([-dx, -dy, size]);
        let hm = shade::normalize([lm[0], lm[1], lm[2] + 1.0]);
        let nlm = lm[2].max(0.0);
        let dm = if aniso {
            shade::ggx_aniso(n, hm, (am * 0.35).max(0.004), (am * 1.8).max(0.02))
        } else {
            shade::ggx(hm[2].max(0.0), am)
        };
        let fm = shade::schlick(base, hm[2].max(0.0));
        let k = strength * dm * shade::smith_g(nlm.max(1e-3), 1.0, am) / (4.0 * nlm).max(1e-4)
            * nlm
            * surface::mix(1.2, 0.8, metal);
        [0_usize, 1, 2].map(|c| k * fm[c] * light_color[c])
    };
    let shown = |col: [f32; 3]| shade::film(col).map(shade::encode);
    let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let plus = |x: [f32; 3], y: [f32; 3]| [x[0] + y[0], x[1] + y[1], x[2] + y[2]];

    let surface_lit = [0_usize, 1, 2].map(|c| params.tint[c] * light_color[c]);
    let under = shown(surface_lit);
    let at_cursor = added(0.0, 0.0);
    let peak = shown(plus(surface_lit, at_cursor));
    if luma(peak) - luma(under) < 0.5 / 255.0 {
        return None;
    }
    // The colour of the light the lobe adds, at full brightness - or the
    // lit colour under the cursor where that is brighter still (a polished
    // metal's highlight burns to white).
    let brightest = at_cursor[0].max(at_cursor[1]).max(at_cursor[2]).max(1e-6);
    let hue = at_cursor.map(|c| shade::encode(c / brightest));
    let color = if luma(peak) > luma(hue) { peak } else { hue };
    let span = (luma(color) - luma(under)).max(1e-4);
    let alpha_at = |dx: f32, dy: f32| {
        ((luma(shown(plus(surface_lit, added(dx, dy)))) - luma(under)) / span).clamp(0.0, 1.0)
    };
    // How far the highlight reaches along one axis: where it brightens the
    // surface by less than half a level. A rough surface's lobe is wide - a
    // light hanging over it lights it far around - so this is far more than
    // the light's height.
    let reach = |along_x: bool| -> f32 {
        let step = (size / 32.0).max(0.5);
        let mut d = step;
        while d < size * 64.0 {
            let alpha = if along_x { alpha_at(d, 0.0) } else { alpha_at(0.0, d) };
            if alpha * span < 0.5 / 255.0 {
                break;
            }
            d += step;
        }
        d
    };
    let radius = [reach(true), reach(false)];
    // The stops at squared shares of the radius: dense near the cursor.
    let stops = (0..SHEEN_STOPS)
        .map(|i| {
            let t = i as f32 / (SHEEN_STOPS - 1) as f32;
            let at = t * t;
            let alpha = if i + 1 == SHEEN_STOPS {
                0.0
            } else {
                alpha_at(at * radius[0], 0.0)
            };
            (at, alpha)
        })
        .collect();
    let byte = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
    Some(SheenSprite {
        radius,
        color: azul_css::props::basic::ColorU {
            r: byte(color[0]),
            g: byte(color[1]),
            b: byte(color[2]),
            a: 255,
        },
        stops,
    })
}

// -- Rendering ----------------------------------------------------------------------------------

/// The uniforms of one render, for the CPU's shader and the GPU's alike:
/// the params and the scene turned into what the shader reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Prepared {
    pub(crate) size: [usize; 2],
    pub(crate) dpr: f32,
    /// The surface's top-left in the window, device px (rounded).
    pub(crate) origin_px: [i32; 2],
    pub(crate) bevel: [f32; 4],
    pub(crate) cushion: [f32; 2],
    pub(crate) pillow: [f32; 2],
    pub(crate) pillow_foot: f32,
    pub(crate) relief: f32,
    pub(crate) metal: f32,
    pub(crate) aniso: bool,
    pub(crate) sun: [f32; 3],
    pub(crate) point: Option<[f32; 3]>,
    pub(crate) light_color: [f32; 3],
    /// The surface on its wallpaper's screen (x, y from the screen's
    /// corner) and the screen's size.
    pub(crate) env_rect: [f32; 4],
    pub(crate) glow: [f32; 4],
}

pub(crate) fn prepare(
    material: Material,
    [width, height]: [u32; 2],
    dpr: f32,
    params: &MaterialParams,
    scene: &SurfaceScene,
) -> Prepared {
    let dpr = if dpr.is_finite() && dpr > 0.0 { dpr } else { 1.0 };
    let h_l = height as f32 / dpr;
    let d3 = params.depth;
    let (metal, aniso) = metal_of(material);
    // The pillow is as wide as a level crown allows: the smooth-min's crown
    // sits at h/2 - k/4 with k = 0.75 w, so w = h / 2 / 1.1875.
    let pillow_w = params.pillow.width.min(h_l * 0.5 / 1.1875).max(0.0);
    let pillow = if params.pillow.width > 0.0 {
        [pillow_w, pillow_w * params.pillow.depth * d3]
    } else {
        [0.0, 0.0]
    };
    // The padding never takes more than a third of a short surface.
    let cushion = if params.cushion[0] > 0.0 {
        [
            params.cushion[0].min(h_l * 0.34),
            params.cushion[1] * (h_l / 60.0).min(1.0) * d3,
        ]
    } else {
        [0.0, 0.0]
    };
    let (sun, point) = match scene.light {
        Light::Sun(s) => (azul_core::light::sun_vector(s.azimuth_deg, s.elevation_deg), None),
        Light::Point(p) => (Light::DEFAULT.direction_at(0.0, 0.0), Some([p.x, p.y, p.height.max(1.0)])),
    };
    let wall_screen = scene
        .environment
        .wallpaper_at(scene.desktop_origin)
        .map_or(scene.screen, |w| w.screen);
    Prepared {
        size: [width as usize, height as usize],
        dpr,
        origin_px: [
            (scene.window_offset[0] * dpr).round() as i32,
            (scene.window_offset[1] * dpr).round() as i32,
        ],
        bevel: [
            params.bevel.width.max(0.0),
            params.bevel.depth * d3,
            params.bevel.softness,
            if params.bevel.inset { 1.0 } else { 0.0 },
        ],
        cushion,
        pillow,
        pillow_foot: params.pillow.foot,
        relief: params.relief * d3.sqrt(),
        metal,
        aniso,
        sun,
        point,
        light_color: scene.light_color.map(|c| c * scene.intensity),
        env_rect: [
            scene.desktop_origin[0] - wall_screen[0],
            scene.desktop_origin[1] - wall_screen[1],
            wall_screen[2].max(1.0),
            wall_screen[3].max(1.0),
        ],
        glow: params.glow.map_or([0.0; 4], |g| {
            [
                g.color[0] * g.strength,
                g.color[1] * g.strength,
                g.color[2] * g.strength,
                g.reach,
            ]
        }),
    }
}

/// `material` rendered on the CPU for a `width` x `height` device-px rect at
/// `dpr`, worn as `params` say, in `scene`. RGBA8, premultiplied, exactly
/// `width` x `height`. Not cached as a frame (its grain is) - see
/// [`render_material`].
#[must_use]
pub fn render_material_pixels(
    material: Material,
    width: u32,
    height: u32,
    dpr: f32,
    params: &MaterialParams,
    scene: &SurfaceScene,
) -> Option<RawImage> {
    if width > 16_384 || height > 16_384 {
        return None;
    }
    let u = prepare(material, [width, height], dpr, params, scene);
    let tile = match params.source {
        MaterialSource::Tile => colourless_tile(material),
        _ => None,
    };
    // A tile that does not decode is drawn from the procedural surface.
    let source = if params.source == MaterialSource::Tile && tile.is_none() {
        MaterialSource::Procedural
    } else {
        params.source
    };
    let shape = if params.bevel.follow == BevelFollow::Shape {
        scene.shape.as_deref()
    } else {
        None
    };
    let wallpaper = scene.environment.wallpaper_at(scene.desktop_origin);
    let env_flat = scene.environment.flat_colour();
    let shader = shade::Shader {
        spec: shade::SurfaceSpec {
            material,
            source,
            tile: tile.as_deref(),
            grain: params.grain,
            scale: params.scale,
            relief: u.relief,
            dpr: u.dpr,
        },
        size: u.size,
        origin_px: u.origin_px,
        radii: params.radii,
        shape,
        bevel: u.bevel,
        cushion: u.cushion,
        pillow: u.pillow,
        pillow_foot: u.pillow_foot,
        tint: params.tint,
        rough: params.roughness,
        variation: params.variation,
        metal: u.metal,
        aniso: u.aniso,
        sun: u.sun,
        point: u.point,
        light_color: u.light_color,
        ambient: scene.ambient,
        env_rect: u.env_rect,
        desktop_origin: scene.desktop_origin,
        wallpaper: wallpaper.map(|w| &**w),
        env_flat,
        glow: u.glow,
        rays: (scene.rays.strength > 0.0).then_some((scene.rays, scene.window_offset)),
    };
    Some(RawImage {
        pixels: RawImageData::U8(shader.render().into()),
        width: u.size[0],
        height: u.size[1],
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
}

/// A hash of everything a render depends on.
pub(crate) fn render_key(
    material: Material,
    width: u32,
    height: u32,
    dpr: f32,
    params: &MaterialParams,
    scene: &SurfaceScene,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    material.hash(&mut h);
    (width, height, dpr.to_bits()).hash(&mut h);
    let bits = |v: &[f32]| v.iter().map(|f| f.to_bits()).collect::<Vec<u32>>();
    params.source.hash(&mut h);
    bits(&params.tint).hash(&mut h);
    bits(&params.roughness).hash(&mut h);
    bits(&[
        params.relief,
        params.variation,
        params.grain,
        params.scale,
        params.bevel.width,
        params.bevel.depth,
        params.bevel.softness,
        params.cushion[0],
        params.cushion[1],
        params.pillow.width,
        params.pillow.depth,
        params.pillow.foot,
        params.depth,
    ])
    .hash(&mut h);
    (params.bevel.inset, params.bevel.follow).hash(&mut h);
    params
        .glow
        .map(|g| bits(&[g.color[0], g.color[1], g.color[2], g.strength, g.reach]))
        .hash(&mut h);
    bits(&params.radii).hash(&mut h);
    scene.light.key_bits().hash(&mut h);
    bits(&scene.light_color).hash(&mut h);
    bits(&[scene.intensity, scene.ambient]).hash(&mut h);
    bits(&[
        scene.rays.strength,
        scene.rays.angle_deg,
        scene.rays.width,
        scene.rays.spacing,
        scene.rays.phase,
    ])
    .hash(&mut h);
    // The window offset in device px is where the grain lies; the desktop
    // place matters to a point light, the rays and the wallpaper.
    [
        (scene.window_offset[0] * dpr).round() as i32,
        (scene.window_offset[1] * dpr).round() as i32,
    ]
    .hash(&mut h);
    let (metal, _) = metal_of(material);
    if scene.light.depends_on_position() || scene.rays.strength > 0.0 || metal > 0.0 {
        bits(&scene.desktop_origin).hash(&mut h);
        bits(&scene.window_offset).hash(&mut h);
        bits(&scene.screen).hash(&mut h);
    }
    if metal > 0.0 {
        scene.environment.key().hash(&mut h);
    }
    if params.bevel.follow == BevelFollow::Shape {
        scene.shape.as_deref().hash(&mut h);
    }
    h.finish()
}

std::thread_local! {
    /// How many material surfaces this thread had a shader draw
    /// ([`renders_on_this_thread`]).
    static RENDERS_ON_THIS_THREAD: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many material surfaces a shader drew for this thread: the CPU's
/// (a render cache miss) or the GPU's. A material is drawn when something
/// it is drawn from changes, never per frame; this is what tests count. Per
/// thread, so tests running side by side do not see each other's renders.
#[must_use]
pub fn renders_on_this_thread() -> u64 {
    RENDERS_ON_THIS_THREAD.with(std::cell::Cell::get)
}

/// One more surface drawn for this thread ([`renders_on_this_thread`]).
pub(crate) fn count_render() {
    RENDERS_ON_THIS_THREAD.with(|n| n.set(n.get() + 1));
}

/// The renders kept, at most this many bytes of pixels ...
const RENDERS_BYTES: usize = 64 * 1024 * 1024;
/// ... and this many renders.
const RENDERS_KEPT: usize = 96;

/// [`render_material_pixels`] as an image, from the process-wide render
/// cache when the same surface was rendered before (the same `ImageRef`:
/// nothing is uploaded or repainted again). A transparent pixel for an
/// empty rect.
#[must_use]
pub fn render_material(
    material: Material,
    width: u32,
    height: u32,
    dpr: f32,
    params: &MaterialParams,
    scene: &SurfaceScene,
) -> ImageRef {
    static RENDERS: Mutex<Vec<(u64, ImageRef, usize)>> = Mutex::new(Vec::new());
    if width == 0 || height == 0 {
        return transparent_pixel();
    }
    let key = render_key(material, width, height, dpr, params, scene);
    if let Ok(mut cache) = RENDERS.lock() {
        if let Some(i) = cache.iter().position(|(k, _, _)| *k == key) {
            let entry = cache.remove(i);
            let found = entry.1.clone();
            cache.push(entry);
            return found;
        }
    }
    count_render();
    let Some(image) = render_material_pixels(material, width, height, dpr, params, scene)
        .and_then(ImageRef::new_rawimage)
    else {
        return transparent_pixel();
    };
    if let Ok(mut cache) = RENDERS.lock() {
        let bytes = width as usize * height as usize * 4;
        cache.push((key, image.clone(), bytes));
        let mut total: usize = cache.iter().map(|(_, _, b)| *b).sum();
        while cache.len() > 1 && (total > RENDERS_BYTES || cache.len() > RENDERS_KEPT) {
            total -= cache.remove(0).2;
        }
    }
    image
}

// -- The callback image -------------------------------------------------------------------------

/// What a material's callback image carries.
#[derive(Debug, Clone, Copy)]
struct MaterialImageData {
    material: Material,
}

/// `material` as a callback image: rendered by the engine for the rect it
/// fills - a background layer's (with its corner radii and its node's
/// `--azul-*` properties) or an image node's - under the window's light.
#[must_use]
pub fn material_callback_image(material: Material) -> ImageRef {
    ImageRef::callback(
        RenderImageCallback::create(render_material_callback),
        RefAny::new(MaterialImageData { material }),
    )
}

/// The params a node's custom properties give `material`
/// ([`MaterialParams::from_custom_properties`]), the radii the engine's.
#[must_use]
pub fn params_from_extras(material: Material, extras: Option<&RenderImageCallbackExtras>) -> MaterialParams {
    extras.map_or_else(
        || MaterialParams::of(material),
        |e| {
            MaterialParams::from_custom_properties(
                material,
                |name| e.custom_property(name).map(str::to_string),
                e.border_radii,
            )
        },
    )
}

/// The scene the engine describes for a rect `size` (logical px) at `dpr`:
/// its light, place and shape, and - for a metal - the desktop.
#[must_use]
pub fn scene_from_extras(
    material: Material,
    extras: Option<&RenderImageCallbackExtras>,
    params: &MaterialParams,
    size: [f32; 2],
    dpr: f32,
) -> SurfaceScene {
    let Some(e) = extras else {
        return SurfaceScene::default();
    };
    let mut scene = SurfaceScene {
        light: e.light,
        window_offset: [e.origin.x, e.origin.y],
        desktop_origin: [
            e.window_desktop_origin.x + e.origin.x,
            e.window_desktop_origin.y + e.origin.y,
        ],
        screen: [
            e.screen.origin.x,
            e.screen.origin.y,
            e.screen.size.width,
            e.screen.size.height,
        ],
        ..SurfaceScene::default()
    };
    scene.apply_light_colour(&e.material_light);
    let highlight = [e.highlight.r, e.highlight.g, e.highlight.b];
    scene.environment = if metal_of(material).0 > 0.0 {
        environment::desktop_environment(highlight)
    } else {
        Environment::highlight(highlight)
    };
    if params.bevel.follow == BevelFollow::Shape {
        // SAFETY: the engine points this at the node's clip path for the
        // duration of the invocation, or null.
        if let Some(clip) = unsafe { e.clip_path.as_ref() } {
            let range = 8.0_f32
                .max(params.bevel.width + 1.0)
                .max(params.cushion[0] + 1.0)
                .max(params.pillow.width + 1.0);
            scene.shape = ShapeField::from_clip_path_with_range(clip, size[0], size[1], dpr, range).map(Arc::new);
        }
    }
    scene
}

/// The callback behind every material image: the rect's device size, its
/// radii, node properties and light (the engine's extras). With a
/// usable GPU context the GPU draws it; otherwise (and for god rays) the
/// CPU.
extern "C" fn render_material_callback(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let Some(material) = data.downcast_ref::<MaterialImageData>().map(|d| d.material) else {
        return transparent_pixel();
    };
    let bounds = info.get_bounds();
    let dpr = bounds.get_hidpi_factor().inner.get();
    let size = bounds.get_physical_size();
    let logical = [bounds.logical_size.width, bounds.logical_size.height];
    let extras = info.get_extras();
    let params = params_from_extras(material, extras);
    let scene = scene_from_extras(material, extras, &params, logical, dpr);
    if scene.rays.strength <= 0.0 {
        if let Some(gl) = info.get_gl_context().as_ref() {
            if let Some(frame) = gpu::render_material_gl(gl, material, size.width, size.height, dpr, &params, &scene) {
                return frame;
            }
        }
    }
    render_material(material, size.width, size.height, dpr, &params, &scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decoded texel: (x, y), sRGB8 albedo, normal x / y, roughness.
    type Texel = ((usize, usize), [u8; 3], [f32; 2], f32);

    /// The packer's decoded texels (`make_material_tiles.py --vectors`).
    const LEATHER_TEXELS: &[Texel] = &[
        ((0, 0), [71, 36, 8], [0.0345, -0.0509], 0.4717),
        ((17, 5), [70, 36, 8], [-0.0657, -0.0509], 0.4717),
        ((127, 127), [66, 31, 3], [0.0345, 0.0509], 0.3590),
        ((64, 100), [75, 41, 13], [-0.0657, 0.0509], 0.5844),
    ];
    const BLUEJEANS_TEXELS: &[Texel] = &[
        ((0, 0), [71, 85, 100], [0.0, 0.3203], 0.7498),
        ((17, 5), [33, 46, 63], [0.0841, -0.0421], 0.7847),
        ((127, 127), [42, 54, 69], [0.0421, -0.0841], 0.7777),
        ((64, 100), [34, 46, 61], [-0.2066, 0.0413], 0.7847),
    ];
    const ALUMINIUM_TEXELS: &[Texel] = &[
        ((0, 0), [173, 173, 173], [0.0, 0.0], 0.42),
        ((17, 5), [160, 160, 160], [0.0, 0.0], 0.42),
        ((127, 127), [160, 160, 160], [0.0, 0.0], 0.42),
        ((64, 100), [167, 167, 167], [0.0, 0.0], 0.42),
    ];

    fn srgb8(c: f32) -> u8 {
        (c * 255.0).round() as u8
    }

    #[test]
    fn every_tile_decodes_to_the_texels_the_packer_wrote() {
        for (material, texels) in [
            (Material::Leather, LEATHER_TEXELS),
            (Material::BlueJeans, BLUEJEANS_TEXELS),
            (Material::Aluminium, ALUMINIUM_TEXELS),
        ] {
            let tile = tile(material).expect("the tile decodes");
            assert_eq!(tile.side, 128, "{material:?}");
            assert_eq!(tile.albedo.len(), 128 * 128);
            assert_eq!(tile.macro_side, 16);
            assert!(tile.macro_period > 128.0, "the macro map spans many tiles");
            for &((x, y), albedo, normal, rough) in texels {
                let i = y * tile.side + x;
                let got = tile.albedo_srgb[i].map(srgb8);
                for c in 0..3 {
                    assert!(
                        got[c].abs_diff(albedo[c]) <= 1,
                        "{material:?} texel ({x},{y}) albedo {got:?}, the packer {albedo:?}"
                    );
                }
                for c in 0..2 {
                    assert!(
                        (tile.normal[i][c] - normal[c]).abs() < 2e-3,
                        "{material:?} texel ({x},{y}) normal {:?}, the packer {normal:?}",
                        tile.normal[i]
                    );
                }
                assert!((tile.rough[i] - rough).abs() < 2e-3, "{material:?} ({x},{y}) roughness");
            }
        }
    }

    #[test]
    fn every_tile_wraps_without_a_seam() {
        // The last column / row is as close to the first as neighbours
        // inside the tile are to each other (the packer cut the seams).
        for material in [Material::Leather, Material::BlueJeans, Material::Aluminium] {
            let tile = tile(material).expect("the tile decodes");
            let n = tile.side;
            let diff = |a: usize, b: usize| -> f32 {
                (0..3).map(|c| (tile.albedo_srgb[a][c] - tile.albedo_srgb[b][c]).abs()).sum()
            };
            let (mut wrap_cols, mut inner_cols, mut wrap_rows, mut inner_rows) = (0.0, 0.0, 0.0, 0.0);
            for y in 0..n {
                wrap_cols += diff(y * n + n - 1, y * n);
                for x in 0..n - 1 {
                    inner_cols += diff(y * n + x, y * n + x + 1);
                }
            }
            for x in 0..n {
                wrap_rows += diff((n - 1) * n + x, x);
                for y in 0..n - 1 {
                    inner_rows += diff(y * n + x, (y + 1) * n + x);
                }
            }
            let (wrap_cols, wrap_rows) = (wrap_cols / n as f32, wrap_rows / n as f32);
            let (inner_cols, inner_rows) =
                (inner_cols / (n * (n - 1)) as f32, inner_rows / (n * (n - 1)) as f32);
            assert!(
                wrap_cols <= 1.5 * inner_cols && wrap_rows <= 1.5 * inner_rows,
                "{material:?}: across the wrap {wrap_cols:.4} / {wrap_rows:.4}, inside \
                 {inner_cols:.4} / {inner_rows:.4}"
            );
        }
    }

    #[test]
    fn every_material_has_a_colourless_tile() {
        for material in Material::ALL {
            let t = colourless_tile(material).expect("a tile");
            assert!(t.side >= 16);
            assert_eq!(t.rgba.len(), t.side * t.side);
            let lumas: Vec<u8> = t.rgba.iter().map(|p| p[0]).collect();
            assert!(lumas.iter().any(|&l| l < 40) && lumas.iter().any(|&l| l > 215), "spread over the bytes");
        }
        // Only the leather keeps a normal of its own.
        let aluminium = colourless_tile(Material::Aluminium).expect("a tile");
        assert!(aluminium.rgba.iter().all(|p| p[1] == 128 && p[2] == 128 && p[3] == 255));
    }

    #[test]
    fn custom_properties_set_the_bevel_the_pillow_and_the_tint() {
        let props = |name: &str| -> Option<String> {
            match name {
                "azul-bevel" => Some("bevel(3px 0.9 1 inset)".to_string()),
                "azul-bevel-follow" => Some("shape".to_string()),
                "azul-pillow" => Some("76px .16".to_string()),
                "azul-material-tint" => Some("#2D4061".to_string()),
                "azul-material-roughness" => Some("0.2 0.4".to_string()),
                "azul-material-source" => Some("tile".to_string()),
                "azul-material-depth" => Some("2.5".to_string()),
                "azul-relief" => Some("not a number".to_string()),
                _ => None,
            }
        };
        let p = MaterialParams::from_custom_properties(Material::Leather, props, [4.0; 4]);
        assert_eq!(p.bevel.width, 3.0);
        assert_eq!(p.bevel.depth, 0.9);
        assert!(p.bevel.inset);
        assert_eq!(p.bevel.follow, BevelFollow::Shape);
        assert_eq!((p.pillow.width, p.pillow.depth, p.pillow.foot), (76.0, 0.16, 1.0));
        assert_eq!(p.roughness, [0.2, 0.4]);
        assert_eq!(p.source, MaterialSource::Tile);
        assert_eq!(p.depth, 2.5);
        assert_eq!(p.relief, 1.0, "an unreadable value keeps the default");
        assert_eq!(p.radii, [4.0; 4]);
        assert!(p.tint[2] > p.tint[0], "denim blue");
        let plain = MaterialParams::from_custom_properties(Material::Aluminium, |_| None, [0.0; 4]);
        assert_eq!(plain, MaterialParams::of(Material::Aluminium));
        assert_eq!(plain.bevel.width, 1.0, "the aluminium's own bevel");
    }

    #[test]
    fn a_render_is_exactly_the_rect_s_size_and_rounds_its_corners() {
        for (w, h) in [(200, 40), (1, 1), (37, 13), (300, 7)] {
            for material in Material::ALL {
                let image = render_material_pixels(
                    material,
                    w,
                    h,
                    1.0,
                    &MaterialParams::of(material),
                    &SurfaceScene::default(),
                )
                .expect("a render");
                assert_eq!((image.width, image.height), (w as usize, h as usize));
            }
        }
        let mut rounded = MaterialParams::of(Material::Leather);
        rounded.radii = [12.0; 4];
        let image = render_material_pixels(Material::Leather, 120, 40, 1.0, &rounded, &SurfaceScene::default())
            .expect("a render");
        let RawImageData::U8(bytes) = &image.pixels else {
            panic!("RGBA8 bytes");
        };
        assert_eq!(bytes.as_ref()[3], 0, "outside the rounded corner");
        assert_eq!(bytes.as_ref()[(20 * 120 + 60) * 4 + 3], 255, "inside");
    }

    #[test]
    fn the_same_surface_renders_once_and_a_new_light_renders_again() {
        let params = MaterialParams::of(Material::BlueJeans);
        let scene = SurfaceScene::default();
        let a = render_material(Material::BlueJeans, 64, 24, 1.0, &params, &scene);
        let b = render_material(Material::BlueJeans, 64, 24, 1.0, &params, &scene);
        assert_eq!(a.get_hash(), b.get_hash(), "the same surface is the same frame");
        let lower = SurfaceScene {
            light: Light::sun(270.0, 40.0),
            ..SurfaceScene::default()
        };
        let c = render_material(Material::BlueJeans, 64, 24, 1.0, &params, &lower);
        assert_ne!(a.get_hash(), c.get_hash(), "another light is another render");
    }

    #[test]
    fn the_leather_s_sheen_is_a_round_highlight_that_fades_out_at_its_reach() {
        let params = MaterialParams::of(Material::Leather);
        let sprite = sheen_sprite(Material::Leather, &params, &MaterialLight::DEFAULT)
            .expect("the leather shines under the cursor");
        assert_eq!(sprite.radius[0], sprite.radius[1], "a dielectric's highlight is round");
        assert!(
            sprite.radius[0] > params.sheen_size,
            "a light 240 px over the cursor lights more than 240 px around it: {:?}",
            sprite.radius
        );
        assert_eq!(sprite.stops.len(), SHEEN_STOPS);
        let alpha: Vec<f32> = sprite.stops.iter().map(|&(_, a)| a).collect();
        assert!(
            alpha[0] > 0.05 && alpha[0] < 1.0,
            "visible under the cursor, the grain still showing through: {}",
            alpha[0]
        );
        assert!(
            alpha.windows(2).all(|pair| pair[1] <= pair[0] + 1e-6),
            "it fades with the distance: {alpha:?}"
        );
        assert_eq!(alpha.last().copied(), Some(0.0), "and is gone at its reach");
        assert!(
            sprite.stops.windows(2).all(|pair| pair[1].0 > pair[0].0),
            "the stops run outward"
        );
        assert_eq!(sprite.stops.last().map(|&(at, _)| at), Some(1.0));
        let c = sprite.color;
        assert!(c.r >= c.g && c.g >= c.b && c.b > 200, "a warm white light: {c:?}");
    }

    #[test]
    fn a_brushed_metal_s_sheen_is_a_streak_across_its_brushing() {
        let params = MaterialParams::of(Material::Aluminium);
        let sprite = sheen_sprite(Material::Aluminium, &params, &MaterialLight::DEFAULT)
            .expect("gold shines under the cursor");
        assert!(
            sprite.radius[1] > sprite.radius[0] * 2.0,
            "brushed along x, the highlight runs along y: {:?}",
            sprite.radius
        );
    }

    #[test]
    fn a_material_without_a_sheen_draws_no_highlight() {
        let mut params = MaterialParams::of(Material::Leather);
        params.sheen = 0.0;
        assert!(sheen_sprite(Material::Leather, &params, &MaterialLight::DEFAULT).is_none());
    }
}
