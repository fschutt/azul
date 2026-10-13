//! The 2.5D MATERIALS: `builtin(leather)`, `builtin(bluejeans)`,
//! `builtin(aluminium)` (cargo feature `materials`).
//!
//! A material is not a picture but a surface: a 128 x 128 micro tile of a CC0
//! PBR set (albedo, normal, roughness - `assets/textures/README.md`) that a
//! small CPU shader lights for the rect it fills, at that rect's device size:
//!
//! - the tile repeats (it wraps without a seam) at `--azul-material-scale` logical px per texel
//!   (default 1), with the source's own large-scale mottling laid over it at the source's real
//!   period (the MACRO map) so the repeat does not show;
//! - each texel's tangent-space normal (GL convention) tilts the surface; a BEVEL `--azul-bevel` px
//!   wide (default 3) follows the rect's rounded corners (its `border-radius`) and tilts the edge
//!   outward - raised - or, with `--azul-inset: 1`, inward - a well, a pressed button;
//! - `--azul-material-tint: <colour>` recolours the albedo and keeps each texel's brightness
//!   relative to the tile's mean, so white or black leather keeps the brown leather's grain, and
//!   a gold tint turns the aluminium (a metal, which reflects its albedo) into gold;
//! - the light is the window's ([`MaterialLight`], `CallbackInfo::set_material_light`): Lambert
//!   diffuse normalised so a flat texel shows exactly its albedo, a GGX highlight with Schlick
//!   fresnel, an ambient share with a sky-to-ground tilt, a fresnel rim (the environment
//!   reflected at grazing angles, so a bevel glints), the baked ambient occlusion, and an
//!   optional soft band of light across the whole window - god rays - that a timer can sweep.
//!
//! Fast where it matters: the shading of a FLAT texel depends only on the
//! texel and the light, so it is computed once per tile and light (the
//! preshade, 16 K texels) and the rect's interior is a lookup into it; only
//! the bevel ring is shaded per pixel. Large rects render in row bands on
//! the machine's cores. Renders are cached by everything they depend on
//! (material, size, scale factor, bevel, radii, light; the window position
//! only while god rays are on), so identical buttons share one render and a
//! still frame renders nothing.
//!
//! The tiles are cut and packed by `scripts/make_material_tiles.py`; the
//! format is [`parse_tile`]'s.

// Per-channel loops over parallel colour arrays (`for c in 0..3`) read
// clearer indexed than zipped three ways.
#![allow(clippy::needless_range_loop)]

use std::sync::{Arc, Mutex, OnceLock};

use azul_core::{
    refany::RefAny,
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat},
};

use super::{transparent_pixel, Material, MaterialLight};
use crate::callbacks::{RenderImageCallback, RenderImageCallbackExtras, RenderImageCallbackInfo};

// -- The tiles -------------------------------------------------------------------------------

const LEATHER: &[u8] = include_bytes!("../../assets/textures/leather.azm");
const BLUEJEANS: &[u8] = include_bytes!("../../assets/textures/bluejeans.azm");
const ALUMINIUM: &[u8] = include_bytes!("../../assets/textures/aluminium.azm");

/// The packed (brotli) tile of `material`.
#[must_use]
pub const fn packed_tile(material: Material) -> &'static [u8] {
    match material {
        Material::Leather => LEATHER,
        Material::BlueJeans => BLUEJEANS,
        Material::Aluminium => ALUMINIUM,
    }
}

const fn material_index(material: Material) -> usize {
    match material {
        Material::Leather => 0,
        Material::BlueJeans => 1,
        Material::Aluminium => 2,
    }
}

/// A decoded tile: what the shader reads per texel, row by row. It wraps.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
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
pub fn tile(material: Material) -> Option<Arc<Tile>> {
    static TILES: [OnceLock<Option<Arc<Tile>>>; 3] =
        [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    TILES[material_index(material)]
        .get_or_init(|| decode_tile(packed_tile(material)).map(Arc::new))
        .clone()
}

/// A tile's packed bytes (brotli over [`parse_tile`]'s format) decoded.
#[must_use]
pub fn decode_tile(packed: &[u8]) -> Option<Tile> {
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
/// Channels 0..6 are Y, Co, Cg (YCoCg of the sRGB albedo), normal x, normal
/// y, roughness, ambient occlusion; a symbol `q` of a `bits` plane is
/// `lo + q / (2^bits - 1) * (hi - lo)`, a subsampled plane is bilinearly
/// upsampled (it wraps). A constant id 0..6 fills its channel; 16 / 17 make
/// the roughness `a + b * Y`; 18 makes the normal the gradient of Y times
/// its value. The macro map's symbols are `lo + q / 255 * (hi - lo)`.
#[must_use]
pub fn parse_tile(raw: &[u8]) -> Option<Tile> {
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
    Some(Tile {
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

// -- What the surfaces are made of -----------------------------------------------------------

/// How a material reflects: dielectric or metal, how strong its relief is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// 0 = dielectric (reflects `f0` white), 1 = metal (reflects its albedo).
    pub metallic: f32,
    /// A dielectric's reflectance at normal incidence.
    pub f0: f32,
    /// The tile's normals' strength.
    pub relief: f32,
    /// The direct highlight's strength.
    pub specular: f32,
    /// The fresnel rim's strength (the environment reflected at grazing angles).
    pub environment: f32,
}

impl Surface {
    /// What `material` is made of.
    #[must_use]
    pub const fn of(material: Material) -> Self {
        match material {
            Material::Leather => Self {
                metallic: 0.0,
                f0: 0.04,
                relief: 1.6,
                specular: 1.0,
                environment: 0.6,
            },
            Material::BlueJeans => Self {
                metallic: 0.0,
                f0: 0.04,
                relief: 1.4,
                specular: 1.0,
                environment: 0.6,
            },
            Material::Aluminium => Self {
                metallic: 1.0,
                f0: 0.91,
                relief: 1.0,
                specular: 1.0,
                environment: 0.6,
            },
        }
    }
}

/// How one element wears its material - per element, from custom properties
/// on its node: `--azul-bevel: 4px; --azul-inset: 1; --azul-material-scale:
/// 2; --azul-relief: 1.5;` ([`Self::from_custom_properties`]).
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MaterialParams {
    /// The bevel's width, logical px (0 = flat to the edge).
    pub bevel: f32,
    /// Sunk in - a well, a pressed button - rather than raised.
    pub inset: bool,
    /// Logical px per tile texel: the grain's size.
    pub scale: f32,
    /// How strongly the relief shows (multiplies the tile's normals).
    pub relief: f32,
    /// The rect's corner radii, logical px: top-left, top-right,
    /// bottom-right, bottom-left (a background layer's `border-radius`).
    pub radii: [f32; 4],
    /// The colour the albedo is recoloured to, linear RGB, or `None` for
    /// the tile's own colours (`--azul-material-tint`).
    pub tint: Option<[f32; 3]>,
}

impl MaterialParams {
    /// A raised 3 px bevel, 1 px per texel, the tile's own relief, square.
    pub const DEFAULT: Self = Self {
        bevel: 3.0,
        inset: false,
        scale: 1.0,
        relief: 1.0,
        radii: [0.0; 4],
        tint: None,
    };

    /// The defaults, overridden by whichever of `--azul-bevel` (a length),
    /// `--azul-inset` (`1` / `true` / `inset` / `on`, or `0` / `false` /
    /// `raised` / `off`), `--azul-material-scale` and `--azul-relief` (numbers)
    /// and `--azul-material-tint` (a CSS colour) `get` (the node's custom
    /// properties, names without the dashes) has.
    pub fn from_custom_properties(get: impl Fn(&str) -> Option<String>, radii: [f32; 4]) -> Self {
        let mut p = Self {
            radii,
            ..Self::DEFAULT
        };
        if let Some(v) = get("azul-bevel").as_deref().and_then(parse_length) {
            p.bevel = v.max(0.0);
        }
        if let Some(v) = get("azul-inset").as_deref().and_then(parse_flag) {
            p.inset = v;
        }
        if let Some(v) = get("azul-material-scale").as_deref().and_then(parse_length) {
            if v > 0.0 {
                p.scale = v;
            }
        }
        if let Some(v) = get("azul-relief").as_deref().and_then(parse_length) {
            p.relief = v.max(0.0);
        }
        if let Some(c) = get("azul-material-tint")
            .and_then(|v| azul_css::props::basic::color::parse_css_color(v.trim()).ok())
        {
            p.tint = Some([c.r, c.g, c.b].map(|v| srgb_to_linear(f32::from(v) / 255.0)));
        }
        p
    }
}

impl Default for MaterialParams {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// `4`, `4px`, ` 4.5px ` -> the number; anything else `None`.
fn parse_length(value: &str) -> Option<f32> {
    let v = value.trim();
    let v = v.strip_suffix("px").unwrap_or(v).trim();
    v.parse::<f32>().ok().filter(|f| f.is_finite())
}

/// A custom property read as a switch.
fn parse_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "inset" => Some(true),
        "0" | "false" | "no" | "off" | "raised" | "none" => Some(false),
        _ => None,
    }
}

// -- The tint ----------------------------------------------------------------------------------

/// A tint ready for the shader: its colour and the tile's mean luma, which
/// a texel's luma is measured against.
#[derive(Debug, Clone, Copy)]
struct Tint {
    color: [f32; 3],
    inv_mean_luma: f32,
}

impl Tint {
    fn new(color: [f32; 3], tile: &Tile) -> Self {
        let n = tile.albedo.len().max(1) as f32;
        let mean = tile.albedo.iter().map(|a| luma(*a)).sum::<f32>() / n;
        Self {
            color,
            inv_mean_luma: if mean > 1e-6 { 1.0 / mean } else { 0.0 },
        }
    }

    /// `albedo` in the tint's colour, as bright against the tint as the
    /// texel is against the tile's mean: the grain stays, the colour goes.
    fn apply(&self, albedo: [f32; 3]) -> [f32; 3] {
        let k = luma(albedo) * self.inv_mean_luma;
        self.color.map(|c| (c * k).clamp(0.0, 1.0))
    }
}

/// Relative luminance of a linear RGB colour.
fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// `albedo`, tinted when there is a tint.
fn albedo_of(albedo: [f32; 3], tint: Option<&Tint>) -> [f32; 3] {
    tint.map_or(albedo, |t| t.apply(albedo))
}

/// A tint as cache-key bits; `u32::MAX` three times for none.
fn tint_bits(tint: Option<[f32; 3]>) -> [u32; 3] {
    tint.map_or([u32::MAX; 3], |t| t.map(f32::to_bits))
}

// -- The light, prepared -----------------------------------------------------------------------

/// A [`MaterialLight`] turned into the vectors the shader reads. Screen
/// space: x right, y DOWN, z toward the viewer, who looks straight in.
#[derive(Debug, Clone, Copy)]
struct Light {
    /// Toward the light.
    l: [f32; 3],
    /// Halfway between the light and the viewer.
    h: [f32; 3],
    /// The light's z (how a flat texel is lit), floored.
    lz: f32,
    /// `(1 - v.h)^5`, Schlick's term for the highlight.
    vh5: f32,
    /// The light's colour, linear.
    color: [f32; 3],
    intensity: f32,
    ambient: f32,
    rays: Option<Rays>,
}

/// God rays, prepared: a gaussian band across the window.
#[derive(Debug, Clone, Copy)]
struct Rays {
    strength: f32,
    /// Across the band (window logical px).
    across: [f32; 2],
    sigma: f32,
    spacing: f32,
    phase: f32,
}

impl Light {
    fn new(light: &MaterialLight) -> Self {
        let az = light.azimuth_deg.to_radians();
        let el = light.elevation_deg.clamp(1.0, 90.0).to_radians();
        let l = [el.cos() * az.cos(), -el.cos() * az.sin(), el.sin()];
        let h = normalize([l[0], l[1], l[2] + 1.0]);
        let rays = if light.rays.strength > 0.0 && light.rays.width > 0.0 {
            let a = light.rays.angle_deg.to_radians();
            Some(Rays {
                strength: light.rays.strength,
                across: [a.sin(), a.cos()],
                sigma: light.rays.width / 2.355,
                spacing: light.rays.spacing.max(0.0),
                phase: light.rays.phase,
            })
        } else {
            None
        };
        let c = light.color;
        Self {
            l,
            h,
            lz: l[2].max(0.05),
            vh5: (1.0 - h[2]).powi(5),
            color: [
                srgb_to_linear(f32::from(c.r) / 255.0),
                srgb_to_linear(f32::from(c.g) / 255.0),
                srgb_to_linear(f32::from(c.b) / 255.0),
            ],
            intensity: light.intensity.max(0.0),
            ambient: light.ambient.clamp(0.0, 1.0),
            rays,
        }
    }

    /// The direct light's factor at window point (`x`, `y`), logical px:
    /// 1 off the rays, up to `1 + strength` at a band's centre.
    fn ray_factor(&self, x: f32, y: f32) -> f32 {
        let Some(r) = &self.rays else {
            return 1.0;
        };
        let across = x * r.across[0] + y * r.across[1] - r.phase;
        let d = if r.spacing > 0.0 {
            across.rem_euclid(r.spacing) - r.spacing * 0.5
        } else {
            across
        };
        let t = d / r.sigma;
        if t.abs() > 4.0 {
            1.0
        } else {
            1.0 + r.strength * (-0.5 * t * t).exp()
        }
    }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-12 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

const fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// GGX (Trowbridge-Reitz) distribution times Smith-Schlick visibility,
/// over `4 n.l n.v`.
fn ggx(nh: f32, nl: f32, nv: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let dd = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (core::f32::consts::PI * dd * dd);
    let k = alpha * 0.5;
    let g = (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k));
    d * g / (4.0 * nl * nv).max(1e-4)
}

/// A texel's normal in screen space: the tile's (GL: +y up the image, so
/// its y flips) scaled by `relief`, over the SHAPE's slope `shape` (the
/// bevel's), blended "whiteout" style.
fn surface_normal(nxy: [f32; 2], relief: f32, shape: [f32; 2]) -> [f32; 3] {
    let tx = nxy[0] * relief;
    let ty = -nxy[1] * relief;
    let tz = (1.0 - tx * tx - ty * ty).max(0.0).sqrt();
    normalize([shape[0] * tz + tx, shape[1] * tz + ty, tz])
}

/// The light one texel sends to the viewer, split into what does not
/// depend on the direct light's strength (`ambient`: the ambient share and
/// the fresnel rim) and what does (`direct`: diffuse and highlight, times
/// the intensity) - the god rays scale only the second. Linear light, times
/// the light's colour. A flat texel (n = z) under a light from the front
/// sends `albedo` (plus a faint highlight): ambient + diffuse = 1.
fn shade(
    albedo: [f32; 3],
    rough: f32,
    ao: f32,
    n: [f32; 3],
    surface: &Surface,
    light: &Light,
) -> ([f32; 3], [f32; 3]) {
    let rough = rough.clamp(0.03, 1.0);
    let nl = dot(n, light.l).max(0.0);
    let nv = n[2].max(1e-3);
    let nh = dot(n, light.h).max(0.0);
    let alpha = rough * rough;
    let highlight = ggx(nh, nl.max(1e-3), nv, alpha) * nl * surface.specular;
    // A surface facing up (screen -y) sees more sky.
    let sky = 1.0 - 0.25 * n[1];
    let rim = (1.0 - nv).powi(5);
    let environment =
        surface.environment * (1.0 - rough) * (1.0 - rough) * (0.6 - 0.4 * n[1]) * ao;
    let diffuse_ambient = light.ambient * ao * sky;
    let diffuse_direct = (1.0 - light.ambient) * nl / light.lz;
    let mut ambient = [0.0; 3];
    let mut direct = [0.0; 3];
    for c in 0..3 {
        let f0 = albedo[c] * surface.metallic + surface.f0 * (1.0 - surface.metallic);
        let fresnel_highlight = f0 + (1.0 - f0) * light.vh5;
        let fresnel_rim = f0 + (1.0 - f0) * rim;
        ambient[c] = (albedo[c] * diffuse_ambient + environment * fresnel_rim) * light.color[c];
        direct[c] = (albedo[c] * diffuse_direct + highlight * fresnel_highlight)
            * light.color[c]
            * light.intensity;
    }
    (ambient, direct)
}

/// `i` wrapped into `0..side` (a mask for the power-of-two sides the tiles
/// have - no division per pixel).
fn wrap(i: i64, side: usize) -> usize {
    if side.is_power_of_two() {
        (i & (side as i64 - 1)) as usize
    } else {
        i.rem_euclid(side as i64) as usize
    }
}

/// The index after `i` on a wrapping row of `side`.
const fn next(i: usize, side: usize) -> usize {
    if i + 1 == side {
        0
    } else {
        i + 1
    }
}

/// The four texels around (`u`, `v`) (texel units, centres at `i + 0.5`)
/// on a wrapping `side` x `side` grid, and their bilinear weights.
fn bilinear_taps(u: f32, v: f32, side: usize) -> ([usize; 4], [f32; 4]) {
    let x = u - 0.5;
    let y = v - 0.5;
    let fx = x.floor();
    let fy = y.floor();
    let tx = x - fx;
    let ty = y - fy;
    let x0 = wrap(fx as i64, side);
    let y0 = wrap(fy as i64, side);
    let x1 = next(x0, side);
    let y1 = next(y0, side);
    (
        [y0 * side + x0, y0 * side + x1, y1 * side + x0, y1 * side + x1],
        [
            (1.0 - tx) * (1.0 - ty),
            tx * (1.0 - ty),
            (1.0 - tx) * ty,
            tx * ty,
        ],
    )
}

/// One texel of the tile, bilinearly sampled.
struct TexelSample {
    albedo: [f32; 3],
    normal: [f32; 2],
    rough: f32,
    ao: f32,
}

impl Tile {
    fn sample(&self, u: f32, v: f32) -> TexelSample {
        let (idx, w) = bilinear_taps(u, v, self.side);
        let mut s = TexelSample {
            albedo: [0.0; 3],
            normal: [0.0; 2],
            rough: 0.0,
            ao: 0.0,
        };
        for k in 0..4 {
            let i = idx[k];
            for c in 0..3 {
                s.albedo[c] += w[k] * self.albedo[i][c];
            }
            s.normal[0] += w[k] * self.normal[i][0];
            s.normal[1] += w[k] * self.normal[i][1];
            s.rough += w[k] * self.rough[i];
            s.ao += w[k] * self.ao[i];
        }
        s
    }

    /// The macro map's multiplier at tile coordinate (`u`, `v`).
    fn macro_at(&self, u: f32, v: f32) -> f32 {
        if self.macro_side == 0 || self.macro_period <= 0.0 {
            return 1.0;
        }
        let m = self.macro_side as f32;
        let scale = m / self.macro_period;
        let (idx, w) = bilinear_taps(u * scale, v * scale, self.macro_side);
        w[0] * self.macro_map[idx[0]]
            + w[1] * self.macro_map[idx[1]]
            + w[2] * self.macro_map[idx[2]]
            + w[3] * self.macro_map[idx[3]]
    }
}

/// Every FLAT texel of a tile shaded under one light: the interior of a
/// rect is a lookup into this.
#[derive(Debug)]
struct Preshade {
    side: usize,
    ambient: Vec<[f32; 3]>,
    direct: Vec<[f32; 3]>,
}

impl Preshade {
    fn new(tile: &Tile, surface: &Surface, relief: f32, light: &Light, tint: Option<&Tint>) -> Self {
        let n = tile.side * tile.side;
        let mut ambient = Vec::with_capacity(n);
        let mut direct = Vec::with_capacity(n);
        for i in 0..n {
            let normal = surface_normal(tile.normal[i], relief, [0.0, 0.0]);
            let albedo = albedo_of(tile.albedo[i], tint);
            let (a, d) = shade(albedo, tile.rough[i], tile.ao[i], normal, surface, light);
            ambient.push(a);
            direct.push(d);
        }
        Self {
            side: tile.side,
            ambient,
            direct,
        }
    }

    fn sample(&self, u: f32, v: f32) -> ([f32; 3], [f32; 3]) {
        let (idx, w) = bilinear_taps(u, v, self.side);
        let mut a = [0.0; 3];
        let mut d = [0.0; 3];
        for k in 0..4 {
            let (ak, dk) = (self.ambient[idx[k]], self.direct[idx[k]]);
            for c in 0..3 {
                a[c] += w[k] * ak[c];
                d[c] += w[k] * dk[c];
            }
        }
        (a, d)
    }

    /// The texel at integer pixel (`x`, `y`) when one texel is one pixel.
    fn at(&self, x: usize, y: usize) -> ([f32; 3], [f32; 3]) {
        let i = wrap(y as i64, self.side) * self.side + wrap(x as i64, self.side);
        (self.ambient[i], self.direct[i])
    }
}

// -- The shape ---------------------------------------------------------------------------------

/// The signed distance from (`px`, `py`) to the edge of the `w` x `h`
/// rounded rect at the origin (negative inside) and the edge's outward unit
/// direction there. `radii`: top-left, top-right, bottom-right, bottom-left.
fn rounded_rect_distance(px: f32, py: f32, w: f32, h: f32, radii: [f32; 4]) -> (f32, f32, f32) {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let right = px >= cx;
    let below = py >= cy;
    let r = match (right, below) {
        (false, false) => radii[0],
        (true, false) => radii[1],
        (true, true) => radii[2],
        (false, true) => radii[3],
    }
    .min(cx)
    .min(cy)
    .max(0.0);
    let qx = (px - cx).abs() - (cx - r);
    let qy = (py - cy).abs() - (cy - r);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    let outside = (ox * ox + oy * oy).sqrt();
    let d = outside + qx.max(qy).min(0.0) - r;
    let (gx, gy) = if qx > 0.0 && qy > 0.0 {
        let len = outside.max(1e-6);
        (ox / len, oy / len)
    } else if qx >= qy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    (
        d,
        if right { gx } else { -gx },
        if below { gy } else { -gy },
    )
}

/// The bevel's slope at distance `d` from the edge (negative inside): a
/// quarter-round profile `bevel` px wide, steepest at the edge.
fn bevel_slope(d: f32, bevel: f32) -> f32 {
    let t = (-d / bevel).clamp(0.0, 1.0);
    let s = 1.0 - t;
    (s / (1.0 - s * s).max(0.08).sqrt()).min(3.0)
}

// -- Rendering ---------------------------------------------------------------------------------

/// The size of the linear -> sRGB table.
const SRGB_TABLE: usize = 8192;

fn srgb_table() -> &'static [u8] {
    static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..SRGB_TABLE)
            .map(|i| (linear_to_srgb(i as f32 / (SRGB_TABLE - 1) as f32) * 255.0 + 0.5) as u8)
            .collect()
    })
}

fn encode(table: &[u8], linear: f32) -> u8 {
    let i = (linear.clamp(0.0, 1.0) * (SRGB_TABLE - 1) as f32 + 0.5) as usize;
    table[i.min(SRGB_TABLE - 1)]
}

/// Rects at least this big render in row bands on several cores.
const PARALLEL_PIXELS: usize = 256 * 1024;

/// What one render reads, shared by its row bands.
struct Job<'a> {
    tile: &'a Tile,
    preshade: &'a Preshade,
    surface: Surface,
    light: &'a Light,
    width: usize,
    height: usize,
    /// Texels per device px.
    per_px: f32,
    /// Device px.
    bevel: f32,
    inset: bool,
    relief: f32,
    /// Device px.
    radii: [f32; 4],
    /// The rect's top-left in the window, logical px (the god rays).
    origin: [f32; 2],
    /// Logical px per device px.
    inv_dpi: f32,
    tint: Option<Tint>,
    table: &'a [u8],
}

impl Job<'_> {
    /// Rows `first..` into `out` (whole rows, RGBA8 premultiplied).
    fn rows(&self, first: usize, out: &mut [u8]) {
        let w = self.width;
        let (wf, hf) = (self.width as f32, self.height as f32);
        // One texel per pixel: the texel IS the pixel, no filtering.
        let unit = self.per_px == 1.0;
        // Rows clear of the rounded corners and the bevel are flat between
        // the side bevels: no distance, no per-pixel shading there.
        let edge = self.bevel + 1.5;
        let top = self.radii[0].max(self.radii[1]) + edge;
        let bottom = self.radii[2].max(self.radii[3]) + edge;
        let inner_lo = edge.ceil() as usize;
        let inner_hi = (wf - edge).floor().max(0.0) as usize;
        for (row, line) in out.chunks_exact_mut(w * 4).enumerate() {
            let y = first + row;
            let py = y as f32 + 0.5;
            let v = py * self.per_px;
            let middle = py > top && py < hf - bottom;
            for x in 0..w {
                let px = x as f32 + 0.5;
                let u = px * self.per_px;
                let ray = self.light.ray_factor(
                    self.origin[0] + px * self.inv_dpi,
                    self.origin[1] + py * self.inv_dpi,
                );
                let (rgb, coverage) = if middle && x >= inner_lo && x < inner_hi {
                    let (a, d) = if unit {
                        self.preshade.at(x, y)
                    } else {
                        self.preshade.sample(u, v)
                    };
                    (
                        [a[0] + ray * d[0], a[1] + ray * d[1], a[2] + ray * d[2]],
                        1.0,
                    )
                } else {
                    let (d, gx, gy) = rounded_rect_distance(px, py, wf, hf, self.radii);
                    let coverage = (0.5 - d).clamp(0.0, 1.0);
                    if coverage <= 0.0 {
                        line[x * 4..x * 4 + 4].copy_from_slice(&[0; 4]);
                        continue;
                    }
                    let (a, dd) = if self.bevel > 0.0 && d > -self.bevel {
                        let t = self.tile.sample(u, v);
                        let slope = bevel_slope(d, self.bevel) * if self.inset { -1.0 } else { 1.0 };
                        let n = surface_normal(t.normal, self.relief, [slope * gx, slope * gy]);
                        let albedo = albedo_of(t.albedo, self.tint.as_ref());
                        shade(albedo, t.rough, t.ao, n, &self.surface, self.light)
                    } else if unit {
                        self.preshade.at(x, y)
                    } else {
                        self.preshade.sample(u, v)
                    };
                    (
                        [a[0] + ray * dd[0], a[1] + ray * dd[1], a[2] + ray * dd[2]],
                        coverage,
                    )
                };
                let m = self.tile.macro_at(u, v);
                let px_out = &mut line[x * 4..x * 4 + 4];
                for c in 0..3 {
                    let s = f32::from(encode(self.table, rgb[c] * m));
                    px_out[c] = (s * coverage + 0.5) as u8;
                }
                px_out[3] = (coverage * 255.0 + 0.5) as u8;
            }
        }
    }

    fn render(&self) -> Vec<u8> {
        let mut out = vec![0_u8; self.width * self.height * 4];
        if out.is_empty() {
            return out;
        }
        let threads = if self.width * self.height >= PARALLEL_PIXELS {
            std::thread::available_parallelism().map_or(1, core::num::NonZeroUsize::get).min(8)
        } else {
            1
        };
        if threads <= 1 {
            self.rows(0, &mut out);
            return out;
        }
        let band = self.height.div_ceil(threads);
        std::thread::scope(|scope| {
            for (i, chunk) in out.chunks_mut(band * self.width * 4).enumerate() {
                scope.spawn(move || self.rows(i * band, chunk));
            }
        });
        out
    }
}

/// `material` rendered for a `width` x `height` device-px rect at `dpi`,
/// worn as `params` say, lit by `light`, the rect's top-left at `origin`
/// in the window (logical px; it only matters to the god rays). RGBA8,
/// premultiplied, exactly `width` x `height`. Not cached - see
/// [`render_material`].
#[must_use]
pub fn render_material_pixels(
    material: Material,
    width: u32,
    height: u32,
    dpi: f32,
    params: &MaterialParams,
    light: &MaterialLight,
    origin: [f32; 2],
) -> Option<RawImage> {
    let tile = tile(material)?;
    render(
        &tile,
        Surface::of(material),
        Some(material),
        [width, height],
        dpi,
        params,
        light,
        origin,
    )
}

/// [`render_material_pixels`] for any tile and surface (not cached).
#[must_use]
pub fn render_tile(
    tile: &Tile,
    surface: Surface,
    width: u32,
    height: u32,
    dpi: f32,
    params: &MaterialParams,
    light: &MaterialLight,
    origin: [f32; 2],
) -> Option<RawImage> {
    render(tile, surface, None, [width, height], dpi, params, light, origin)
}

/// The renderer behind both: `material` is the tile's when it is a built-in
/// one (its preshade is then cached).
fn render(
    tile: &Tile,
    surface: Surface,
    material: Option<Material>,
    [width, height]: [u32; 2],
    dpi: f32,
    params: &MaterialParams,
    light: &MaterialLight,
    origin: [f32; 2],
) -> Option<RawImage> {
    let (width, height) = (width as usize, height as usize);
    if tile.side == 0 || width > 16_384 || height > 16_384 {
        return None;
    }
    let dpi = if dpi.is_finite() && dpi > 0.0 { dpi } else { 1.0 };
    let prepared = Light::new(light);
    let relief = surface.relief * params.relief.max(0.0);
    let tint = params.tint.map(|c| Tint::new(c, tile));
    let preshade = match material {
        Some(material) => {
            preshade_for(material, tile, &surface, relief, &prepared, light, params.tint)
        }
        None => Arc::new(Preshade::new(tile, &surface, relief, &prepared, tint.as_ref())),
    };
    let texel = (params.scale.max(0.01) * dpi).max(0.05);
    let job = Job {
        tile,
        preshade: &preshade,
        surface,
        light: &prepared,
        width,
        height,
        per_px: 1.0 / texel,
        bevel: params.bevel.max(0.0) * dpi,
        inset: params.inset,
        relief,
        radii: params.radii.map(|r| r.max(0.0) * dpi),
        origin,
        inv_dpi: 1.0 / dpi,
        tint,
        table: srgb_table(),
    };
    Some(RawImage {
        pixels: RawImageData::U8(job.render().into()),
        width,
        height,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
}

// -- Caches ------------------------------------------------------------------------------------

/// Preshades kept (a few lights x materials x reliefs).
const PRESHADES_KEPT: usize = 12;

/// A built-in tile's preshade for this light, relief and tint, from the
/// cache (the light's direction, colour and shares; the rays do not enter
/// it).
fn preshade_for(
    material: Material,
    tile: &Tile,
    surface: &Surface,
    relief: f32,
    prepared: &Light,
    light: &MaterialLight,
    tint: Option<[f32; 3]>,
) -> Arc<Preshade> {
    type Key = [u32; 10];
    static PRESHADES: Mutex<Vec<(Key, Arc<Preshade>)>> = Mutex::new(Vec::new());
    let c = light.color;
    let key: Key = [
        material_index(material) as u32,
        light.azimuth_deg.to_bits(),
        light.elevation_deg.to_bits(),
        u32::from_le_bytes([c.r, c.g, c.b, c.a]),
        light.intensity.to_bits(),
        light.ambient.to_bits(),
        relief.to_bits(),
        tint_bits(tint)[0],
        tint_bits(tint)[1],
        tint_bits(tint)[2],
    ];
    if let Ok(mut cache) = PRESHADES.lock() {
        if let Some(i) = cache.iter().position(|(k, _)| *k == key) {
            let entry = cache.remove(i);
            let found = entry.1.clone();
            cache.push(entry);
            return found;
        }
    }
    let tint = tint.map(|c| Tint::new(c, tile));
    let made = Arc::new(Preshade::new(tile, surface, relief, prepared, tint.as_ref()));
    if let Ok(mut cache) = PRESHADES.lock() {
        cache.push((key, made.clone()));
        while cache.len() > PRESHADES_KEPT {
            cache.remove(0);
        }
    }
    made
}

/// The renders kept, at most this many bytes of pixels ...
const RENDERS_BYTES: usize = 64 * 1024 * 1024;
/// ... and this many renders.
const RENDERS_KEPT: usize = 96;

/// Everything a render depends on, as bit patterns.
type RenderKey = [u32; 27];

fn render_key(
    material: Material,
    width: u32,
    height: u32,
    dpi: f32,
    params: &MaterialParams,
    light: &MaterialLight,
    origin: [f32; 2],
) -> RenderKey {
    // The window position matters only to the god rays: without them two
    // identical buttons anywhere share one render.
    let origin = if light.rays.strength > 0.0 {
        origin
    } else {
        [0.0, 0.0]
    };
    let c = light.color;
    [
        material_index(material) as u32,
        width,
        height,
        dpi.to_bits(),
        params.bevel.to_bits(),
        u32::from(params.inset),
        params.scale.to_bits(),
        params.relief.to_bits(),
        params.radii[0].to_bits(),
        params.radii[1].to_bits(),
        params.radii[2].to_bits(),
        params.radii[3].to_bits(),
        light.azimuth_deg.to_bits(),
        light.elevation_deg.to_bits(),
        u32::from_le_bytes([c.r, c.g, c.b, c.a]),
        light.intensity.to_bits(),
        light.ambient.to_bits(),
        light.rays.strength.to_bits(),
        light.rays.angle_deg.to_bits(),
        light.rays.width.to_bits(),
        light.rays.spacing.to_bits(),
        light.rays.phase.to_bits(),
        origin[0].to_bits(),
        origin[1].to_bits(),
        tint_bits(params.tint)[0],
        tint_bits(params.tint)[1],
        tint_bits(params.tint)[2],
    ]
}

/// [`render_material_pixels`] as an image, from the process-wide render
/// cache when the same surface was rendered before (the same `ImageRef`:
/// nothing is uploaded or repainted again). A transparent pixel for an
/// empty rect or a tile that does not decode.
#[must_use]
pub fn render_material(
    material: Material,
    width: u32,
    height: u32,
    dpi: f32,
    params: &MaterialParams,
    light: &MaterialLight,
    origin: [f32; 2],
) -> ImageRef {
    static RENDERS: Mutex<Vec<(RenderKey, ImageRef, usize)>> = Mutex::new(Vec::new());
    if width == 0 || height == 0 {
        return transparent_pixel();
    }
    let key = render_key(material, width, height, dpi, params, light, origin);
    if let Ok(mut cache) = RENDERS.lock() {
        if let Some(i) = cache.iter().position(|(k, _, _)| *k == key) {
            let entry = cache.remove(i);
            let found = entry.1.clone();
            cache.push(entry);
            return found;
        }
    }
    let Some(image) = render_material_pixels(material, width, height, dpi, params, light, origin)
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

// -- The callback image ------------------------------------------------------------------------

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

/// The params a node's custom properties give a material
/// ([`MaterialParams::from_custom_properties`]), the radii the engine's.
#[must_use]
pub fn params_from_extras(extras: Option<&RenderImageCallbackExtras>) -> MaterialParams {
    extras.map_or(MaterialParams::DEFAULT, |e| {
        MaterialParams::from_custom_properties(
            |name| e.custom_property(name).map(str::to_string),
            e.border_radii,
        )
    })
}

/// The callback behind every material image: the rect's device size, its
/// radii and node properties (the engine's extras), the window's light.
extern "C" fn render_material_callback(
    mut data: RefAny,
    info: RenderImageCallbackInfo,
) -> ImageRef {
    let Some(material) = data.downcast_ref::<MaterialImageData>().map(|d| d.material) else {
        return transparent_pixel();
    };
    let bounds = info.get_bounds();
    let dpi = bounds.get_hidpi_factor().inner.get();
    let size = bounds.get_physical_size();
    let extras = info.get_extras();
    let params = params_from_extras(extras);
    let origin = extras.map_or([0.0, 0.0], |e| [e.origin.x, e.origin.y]);
    render_material(
        material,
        size.width,
        size.height,
        dpi,
        &params,
        &info.get_material_light(),
        origin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The packer's decoded texels (`make_material_tiles.py --vectors`):
    /// (x, y), sRGB8 albedo, normal x / y, roughness.
    const LEATHER_TEXELS: &[((usize, usize), [u8; 3], [f32; 2], f32)] = &[
        ((0, 0), [71, 36, 8], [0.0345, -0.0509], 0.4717),
        ((17, 5), [70, 36, 8], [-0.0657, -0.0509], 0.4717),
        ((127, 127), [66, 31, 3], [0.0345, 0.0509], 0.3590),
        ((64, 100), [75, 41, 13], [-0.0657, 0.0509], 0.5844),
    ];
    const BLUEJEANS_TEXELS: &[((usize, usize), [u8; 3], [f32; 2], f32)] = &[
        ((0, 0), [71, 85, 100], [0.0, 0.3203], 0.7498),
        ((17, 5), [33, 46, 63], [0.0841, -0.0421], 0.7847),
        ((127, 127), [42, 54, 69], [0.0421, -0.0841], 0.7777),
        ((64, 100), [34, 46, 61], [-0.2066, 0.0413], 0.7847),
    ];
    const ALUMINIUM_TEXELS: &[((usize, usize), [u8; 3], [f32; 2], f32)] = &[
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
        for material in Material::ALL {
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

    /// A tile of one flat texel colour.
    fn flat_tile(albedo_linear: f32, rough: f32) -> Tile {
        let n = 4;
        let srgb = linear_to_srgb(albedo_linear);
        Tile {
            side: n,
            albedo_srgb: vec![[srgb; 3]; n * n],
            albedo: vec![[albedo_linear; 3]; n * n],
            normal: vec![[0.0, 0.0]; n * n],
            rough: vec![rough; n * n],
            ao: vec![1.0; n * n],
            macro_map: Vec::new(),
            macro_side: 0,
            macro_period: 0.0,
        }
    }

    const MATTE: Surface = Surface {
        metallic: 0.0,
        f0: 0.04,
        relief: 1.0,
        specular: 1.0,
        environment: 0.6,
    };

    fn pixel(image: &RawImage, x: usize, y: usize) -> [u8; 4] {
        let RawImageData::U8(bytes) = &image.pixels else {
            panic!("RGBA8 bytes");
        };
        let i = (y * image.width + x) * 4;
        let b = bytes.as_ref();
        [b[i], b[i + 1], b[i + 2], b[i + 3]]
    }

    fn luma(p: [u8; 4]) -> f32 {
        0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
    }

    #[test]
    fn a_flat_surface_lit_from_the_front_shows_its_albedo() {
        // n = z and the light straight in front: ambient + diffuse = 1, and
        // a fully rough surface adds only GGX's faint 1/(4 pi) * f0.
        let tile = flat_tile(0.25, 1.0);
        let params = MaterialParams {
            bevel: 0.0,
            ..MaterialParams::DEFAULT
        };
        let light = MaterialLight::from_direction(135.0, 90.0);
        let image = render_tile(&tile, MATTE, 16, 16, 1.0, &params, &light, [0.0, 0.0])
            .expect("a render");
        let lc = srgb_to_linear(250.0 / 255.0); // the default light's green
        let expected = linear_to_srgb((0.25 + 0.04 / (4.0 * core::f32::consts::PI)) * lc) * 255.0;
        for (x, y) in [(0, 0), (8, 8), (15, 3)] {
            let p = pixel(&image, x, y);
            assert_eq!(p[3], 255, "opaque");
            assert!(
                (f32::from(p[1]) - expected).abs() <= 1.5,
                "({x},{y}) green {} vs albedo x light {expected:.1}",
                p[1]
            );
        }
    }

    #[test]
    fn the_bevel_s_lit_edge_faces_the_light() {
        let tile = flat_tile(0.3, 0.7);
        let raised = MaterialParams {
            bevel: 6.0,
            ..MaterialParams::DEFAULT
        };
        let inset = MaterialParams {
            inset: true,
            ..raised
        };
        let from_upper_left = MaterialLight::from_direction(135.0, 35.0);
        let edges = |image: &RawImage| -> (f32, f32) {
            // the left and top bevel vs the right and bottom bevel, mid-edge
            let lit = luma(pixel(image, 2, 20)) + luma(pixel(image, 40, 2));
            let shade = luma(pixel(image, 77, 20)) + luma(pixel(image, 40, 37));
            (lit, shade)
        };
        let image = render_tile(&tile, MATTE, 80, 40, 1.0, &raised, &from_upper_left, [0.0, 0.0])
            .expect("a render");
        let (lit, shade) = edges(&image);
        assert!(lit > shade + 20.0, "raised, lit from the upper left: {lit} vs {shade}");
        let image = render_tile(&tile, MATTE, 80, 40, 1.0, &inset, &from_upper_left, [0.0, 0.0])
            .expect("a render");
        let (lit, shade) = edges(&image);
        assert!(shade > lit + 20.0, "a well, lit from the upper left: {lit} vs {shade}");
        // The light moves, the highlight follows: from the lower right the
        // raised bevel is lit on the other side.
        let from_lower_right = MaterialLight::from_direction(315.0, 35.0);
        let image = render_tile(&tile, MATTE, 80, 40, 1.0, &raised, &from_lower_right, [0.0, 0.0])
            .expect("a render");
        let (lit, shade) = edges(&image);
        assert!(shade > lit + 20.0, "raised, lit from the lower right: {lit} vs {shade}");
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
                    &MaterialParams::DEFAULT,
                    &MaterialLight::DEFAULT,
                    [0.0, 0.0],
                )
                .expect("a render");
                assert_eq!((image.width, image.height), (w as usize, h as usize));
                let RawImageData::U8(bytes) = &image.pixels else {
                    panic!("RGBA8 bytes");
                };
                assert_eq!(bytes.as_ref().len(), w as usize * h as usize * 4);
            }
        }
        let rounded = MaterialParams {
            radii: [12.0; 4],
            ..MaterialParams::DEFAULT
        };
        let image = render_material_pixels(
            Material::Leather,
            120,
            40,
            1.0,
            &rounded,
            &MaterialLight::DEFAULT,
            [0.0, 0.0],
        )
        .expect("a render");
        assert_eq!(pixel(&image, 0, 0)[3], 0, "outside the rounded corner");
        assert_eq!(pixel(&image, 60, 20)[3], 255, "inside");
        let p = pixel(&image, 60, 20);
        assert!(p[0] > p[1] && p[1] > p[2], "leather is brown: {p:?}");
    }

    #[test]
    fn the_same_surface_renders_once_and_a_new_light_renders_again() {
        let params = MaterialParams::DEFAULT;
        let light = MaterialLight::DEFAULT;
        let a = render_material(Material::Aluminium, 64, 24, 1.0, &params, &light, [10.0, 10.0]);
        let b = render_material(Material::Aluminium, 64, 24, 1.0, &params, &light, [300.0, 50.0]);
        assert_eq!(a.get_hash(), b.get_hash(), "no rays: the position does not matter");
        let lower = MaterialLight::from_direction(270.0, 40.0);
        let c = render_material(Material::Aluminium, 64, 24, 1.0, &params, &lower, [10.0, 10.0]);
        assert_ne!(a.get_hash(), c.get_hash(), "another light is another render");
        let rays = light.with_rays(crate::texture::MaterialRays::create(0.8, 135.0, 60.0, 30.0));
        let d = render_material(Material::Aluminium, 64, 24, 1.0, &params, &rays, [10.0, 10.0]);
        let e = render_material(Material::Aluminium, 64, 24, 1.0, &params, &rays, [300.0, 50.0]);
        assert_ne!(d.get_hash(), e.get_hash(), "under god rays the position matters");
        let f = render_material(Material::Aluminium, 64, 24, 1.0, &params, &rays, [300.0, 50.0]);
        assert_eq!(e.get_hash(), f.get_hash());
    }

    #[test]
    fn custom_properties_set_the_bevel_the_inset_and_the_scale() {
        let props = |name: &str| -> Option<String> {
            match name {
                "azul-bevel" => Some(" 5px ".to_string()),
                "azul-inset" => Some("inset".to_string()),
                "azul-material-scale" => Some("2".to_string()),
                "azul-relief" => Some("not a number".to_string()),
                _ => None,
            }
        };
        let p = MaterialParams::from_custom_properties(props, [4.0; 4]);
        assert_eq!(p.bevel, 5.0);
        assert!(p.inset);
        assert_eq!(p.scale, 2.0);
        assert_eq!(p.relief, 1.0, "an unreadable value keeps the default");
        assert_eq!(p.radii, [4.0; 4]);
        assert_eq!(
            MaterialParams::from_custom_properties(|_| None, [0.0; 4]),
            MaterialParams::DEFAULT
        );
    }

    /// What a render costs - not a check. Run it in release:
    /// `cargo test --release -p azul-layout --lib --features materials --
    /// --ignored print_what_a_render_costs --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn print_what_a_render_costs() {
        let params = MaterialParams {
            radii: [8.0; 4],
            ..MaterialParams::DEFAULT
        };
        for material in Material::ALL {
            // Decode the tile and preshade the light first: once per process.
            let _ = render_material_pixels(material, 8, 8, 1.0, &params, &MaterialLight::DEFAULT, [0.0; 2]);
            for (w, h) in [(200, 40), (800, 600), (1920, 1080)] {
                for (dpi, rays) in [(1.0, false), (1.0, true), (2.0, false)] {
                    let runs = 5;
                    let start = std::time::Instant::now();
                    for i in 0..runs {
                        let light = if rays {
                            MaterialLight::DEFAULT.with_rays(crate::texture::MaterialRays::create(
                                0.6,
                                135.0,
                                120.0,
                                i as f32 * 10.0,
                            ))
                        } else {
                            MaterialLight::DEFAULT
                        };
                        let _ = render_material_pixels(material, w, h, dpi, &params, &light, [0.0; 2]);
                    }
                    let ms = start.elapsed().as_secs_f64() * 1e3 / f64::from(runs);
                    eprintln!(
                        "{material:?} {w}x{h} device px, dpi {dpi}, rays {rays}: {ms:.2} ms"
                    );
                }
            }
        }
    }

    #[test]
    fn god_rays_brighten_a_band_and_nothing_else() {
        let tile = flat_tile(0.2, 0.8);
        let params = MaterialParams {
            bevel: 0.0,
            ..MaterialParams::DEFAULT
        };
        // A vertical band (running straight up: across it is +x) 20 px wide
        // at x = 100 in the window; the rect sits at x = 10, so its pixel 90.
        let rays = crate::texture::MaterialRays::create(1.0, 90.0, 20.0, 100.0);
        let light = MaterialLight::DEFAULT.with_rays(rays);
        let image = render_tile(&tile, MATTE, 200, 4, 1.0, &params, &light, [10.0, 0.0])
            .expect("a render");
        let at = |x| luma(pixel(&image, x, 2));
        assert!(at(90) > at(20) + 10.0, "the band is brighter: {} vs {}", at(90), at(20));
        assert!((at(20) - at(180)).abs() < 1.0, "away from it the light is the same");
    }
}
