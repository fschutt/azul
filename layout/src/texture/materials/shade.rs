//! Lighting a material, one device pixel at a time, on the CPU.
//!
//! The same steps as the GPU's fragment shader (`shader.glsl`), in the same
//! order and with the same numbers, so both renderers draw the same
//! surface:
//!
//! 1. the shape: the signed distance to the surface's edge (a rounded rect, or a shape's
//!    distance field), which gives the coverage and - through a bevel, a cushion and the
//!    pillow - a slope of its own;
//! 2. the grain: the material's height, luma, roughness and occlusion around the pixel, its
//!    slope by central differences half a device pixel apart;
//! 3. the light: GGX with Smith-Schlick and Schlick's Fresnel (anisotropic for brushed
//!    metal), a wrapped diffuse normalised so a flat surface shows its tint, a room
//!    reflected (a studio, and the desktop for metals), the accent's glow;
//! 4. the film: a soft shoulder above 0.86 that desaturates toward white, sRGB, and a half
//!    level of dither so no gradient bands.
//!
//! The grain is the expensive part and does not depend on the light, the
//! shape or the tint: it is computed in 32 x 32 tiles of WINDOW device
//! pixels and kept ([`grain_tile`]), so a relight (a new light, a height
//! that animates) only redoes step 1, 3 and 4.
//!
//! The cursor's sheen is not part of the surface: it is a layer of its own
//! over it (`sheen_sprite` in the parent module), so the cursor never draws
//! a material again.

// The formulas keep the shader's letters (shader.glsl): p, n, h, l, r, f0...
#![allow(clippy::many_single_char_names)]

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use super::{
    surface::{self, mix, smoothstep, Sample, PI},
    ColourlessTile, MaterialSource, ShapeField,
};
use crate::texture::{environment::Wallpaper, Material};

/// The side of a grain tile, device px.
pub(crate) const TILE: usize = 32;

/// The grain of one device pixel: the surface normal from the material's
/// own relief (before any shape slope), its luma, roughness and occlusion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Grain {
    pub(crate) n: [f32; 3],
    pub(crate) luma: f32,
    pub(crate) rough: f32,
    pub(crate) ao: f32,
}

impl Grain {
    const SMOOTH: Self = Self {
        n: [0.0, 0.0, 1.0],
        luma: 0.62,
        rough: 0.5,
        ao: 1.0,
    };
}

/// What a grain tile depends on: the material and source, the grain size,
/// the pattern scale, the relief (the normal), the device scale and the
/// tile's place in the window (tile units).
pub(crate) type GrainKey = [u32; 8];

/// What the surface functions read besides the position.
#[derive(Clone, Copy)]
pub(crate) struct SurfaceSpec<'a> {
    pub(crate) material: Material,
    pub(crate) source: MaterialSource,
    pub(crate) tile: Option<&'a ColourlessTile>,
    /// The leather's grain cell, logical px.
    pub(crate) grain: f32,
    /// The pattern's scale: 2 = everything twice as large.
    pub(crate) scale: f32,
    pub(crate) relief: f32,
    pub(crate) dpr: f32,
}

impl SurfaceSpec<'_> {
    /// The shader's `surf(p)`. Without `full` only the height is right.
    fn surf(&self, p: [f32; 2], full: bool) -> Sample {
        let p = [p[0] / self.scale, p[1] / self.scale];
        match (self.source, self.tile) {
            (MaterialSource::Smooth, _) => Sample::SMOOTH,
            (MaterialSource::Tile, Some(tile)) => surface::tile_sample(tile, p),
            _ => match self.material {
                Material::Leather => surface::leather(p, self.grain, self.dpr * self.scale, full),
                Material::Aluminium => surface::aluminium(p),
                Material::BlueJeans => surface::denim(p),
                Material::Vellum => surface::vellum(p),
            },
        }
    }

    const fn key(&self, tx: i32, ty: i32) -> GrainKey {
        [
            super::material_index(self.material) as u32,
            self.source as u32,
            self.grain.to_bits(),
            self.scale.to_bits(),
            self.relief.to_bits(),
            self.dpr.to_bits(),
            tx as u32,
            ty as u32,
        ]
    }

    /// One tile of grain: window device pixels `tx * TILE ..` x `ty * TILE ..`.
    ///
    /// The slope's samples half a device pixel either side of a pixel lie on
    /// the pixel's edges, which its neighbour shares: the heights on the
    /// edges are computed once (a row of `TILE + 1` per pixel row, a column of
    /// `TILE + 1` per pixel column), three surface samples a pixel instead
    /// of five.
    fn compute_tile(&self, tx: i32, ty: i32) -> Vec<Grain> {
        let e = 0.5 / self.dpr;
        let x0 = tx * TILE as i32;
        let y0 = ty * TILE as i32;
        let edge = |i: i32| i as f32 / self.dpr;
        let centre = |i: i32| (i as f32 + 0.5) / self.dpr;
        // Heights on the vertical edges of each row's pixels ...
        let mut on_columns = vec![0.0_f32; TILE * (TILE + 1)];
        // ... and on the horizontal edges of each column's pixels.
        let mut on_rows = vec![0.0_f32; TILE * (TILE + 1)];
        for yy in 0..TILE {
            let gy = centre(y0 + yy as i32);
            for k in 0..=TILE {
                on_columns[yy * (TILE + 1) + k] = self.surf([edge(x0 + k as i32), gy], false).h;
            }
        }
        for xx in 0..TILE {
            let gx = centre(x0 + xx as i32);
            for k in 0..=TILE {
                on_rows[xx * (TILE + 1) + k] = self.surf([gx, edge(y0 + k as i32)], false).h;
            }
        }
        let mut out = Vec::with_capacity(TILE * TILE);
        let tile_normal = self.source == MaterialSource::Tile && self.material == Material::Leather;
        for yy in 0..TILE {
            let gy = centre(y0 + yy as i32);
            for xx in 0..TILE {
                let gx = centre(x0 + xx as i32);
                let s = self.surf([gx, gy], true);
                let hx0 = on_columns[yy * (TILE + 1) + xx];
                let hx1 = on_columns[yy * (TILE + 1) + xx + 1];
                let hy0 = on_rows[xx * (TILE + 1) + yy];
                let hy1 = on_rows[xx * (TILE + 1) + yy + 1];
                let grad = [(hx1 - hx0) / (2.0 * e), (hy1 - hy0) / (2.0 * e)];
                let mut n = normalize([-grad[0] * self.relief, -grad[1] * self.relief, 1.0]);
                if tile_normal {
                    if let Some(tile) = self.tile {
                        let g = [gx / self.scale, gy / self.scale];
                        let t = surface::bicubic(tile, [g[0] / 128.0, g[1] / 128.0]);
                        let tn = [
                            (t[1] * 2.0 - 1.0) * 1.4 * self.relief,
                            -(t[2] * 2.0 - 1.0) * 1.4 * self.relief,
                        ];
                        n = normalize([
                            tn[0] - grad[0] * 0.25 * self.relief,
                            tn[1] - grad[1] * 0.25 * self.relief,
                            1.0,
                        ]);
                    }
                }
                out.push(Grain {
                    n,
                    luma: s.luma,
                    rough: s.rough,
                    ao: s.ao,
                });
            }
        }
        out
    }
}

/// Grain tiles kept: about 60 MB.
const GRAIN_TILES_KEPT: usize = 2400;

/// The grain tiles computed so far, and the order they came in (the oldest
/// go first when the cache is full).
#[derive(Default)]
struct GrainCache {
    tiles: HashMap<GrainKey, Arc<Vec<Grain>>>,
    order: VecDeque<GrainKey>,
}

fn grain_cache() -> &'static Mutex<GrainCache> {
    static CACHE: OnceLock<Mutex<GrainCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(GrainCache::default()))
}

/// The grain tiles `tx.0..tx.1` x `ty.0..ty.1` (tile units in window
/// device px), from the cache or computed now (on several cores when many
/// are missing), row by row.
pub(crate) fn grain_tiles(spec: &SurfaceSpec<'_>, tx: (i32, i32), ty: (i32, i32)) -> Vec<Arc<Vec<Grain>>> {
    let wanted: Vec<(i32, i32)> = (ty.0..ty.1)
        .flat_map(|y| (tx.0..tx.1).map(move |x| (x, y)))
        .collect();
    let mut found: Vec<Option<Arc<Vec<Grain>>>> = vec![None; wanted.len()];
    if let Ok(cache) = grain_cache().lock() {
        for (i, &(x, y)) in wanted.iter().enumerate() {
            found[i] = cache.tiles.get(&spec.key(x, y)).cloned();
        }
    }
    let missing: Vec<usize> = (0..wanted.len()).filter(|i| found[*i].is_none()).collect();
    if !missing.is_empty() {
        let threads = std::thread::available_parallelism()
            .map_or(1, core::num::NonZeroUsize::get)
            .min(8)
            .min(missing.len());
        let made: Vec<(usize, Arc<Vec<Grain>>)> = if threads <= 1 {
            missing
                .iter()
                .map(|&i| (i, Arc::new(spec.compute_tile(wanted[i].0, wanted[i].1))))
                .collect()
        } else {
            let per = missing.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let handles: Vec<_> = missing
                    .chunks(per)
                    .map(|chunk| {
                        let wanted = &wanted;
                        scope.spawn(move || {
                            chunk
                                .iter()
                                .map(|&i| (i, Arc::new(spec.compute_tile(wanted[i].0, wanted[i].1))))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|h| h.join().unwrap_or_default())
                    .collect()
            })
        };
        if let Ok(mut cache) = grain_cache().lock() {
            for (i, tile) in &made {
                let key = spec.key(wanted[*i].0, wanted[*i].1);
                if cache.tiles.insert(key, tile.clone()).is_none() {
                    cache.order.push_back(key);
                }
            }
            while cache.order.len() > GRAIN_TILES_KEPT {
                if let Some(old) = cache.order.pop_front() {
                    cache.tiles.remove(&old);
                }
            }
        }
        for (i, tile) in made {
            found[i] = Some(tile);
        }
    }
    found
        .into_iter()
        .map(|t| t.unwrap_or_else(|| Arc::new(vec![Grain::SMOOTH; TILE * TILE])))
        .collect()
}

pub(crate) fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-12 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// GGX (Trowbridge-Reitz) normal distribution.
fn ggx(nh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = nh * nh * (a2 - 1.0) + 1.0;
    a2 / (PI * d * d)
}

/// Anisotropic GGX, brushed along x.
fn ggx_aniso(n: [f32; 3], h: [f32; 3], ax: f32, ay: f32) -> f32 {
    let t = normalize([1.0 - n[0] * n[0], -n[1] * n[0], -n[2] * n[0]]);
    let b = cross(n, t);
    let th = dot(t, h) / ax;
    let bh = dot(b, h) / ay;
    let nh = dot(n, h);
    let d = th * th + bh * bh + nh * nh;
    1.0 / (PI * ax * ay * d * d)
}

/// Smith-Schlick geometry term, k = a / 2.
fn smith_g(nl: f32, nv: f32, a: f32) -> f32 {
    let k = a * 0.5;
    (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k))
}

fn schlick(f0: [f32; 3], c: f32) -> [f32; 3] {
    let k = (1.0 - c).powf(5.0);
    f0.map(|f| f + (1.0 - f) * k)
}

/// Linear light to sRGB, 0..1.
pub(crate) fn encode(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c < 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The signed distance to a rounded rect of half size `half` centred at the
/// origin, per-corner radii (top-left, top-right, bottom-right, bottom-left).
pub(crate) fn sd_round(p: [f32; 2], half: [f32; 2], r: [f32; 4]) -> f32 {
    let rr = if p[0] < 0.0 {
        if p[1] < 0.0 { r[0] } else { r[3] }
    } else if p[1] < 0.0 {
        r[1]
    } else {
        r[2]
    };
    let rr = rr.min(half[0].min(half[1]));
    let q = [p[0].abs() - half[0] + rr, p[1].abs() - half[1] + rr];
    q[0].max(0.0).hypot(q[1].max(0.0)) + q[0].max(q[1]).min(0.0) - rr
}

/// The polynomial smooth minimum: `min(a, b)` rounded over a band `k` wide.
pub(crate) fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// A material lit for one surface: every per-render constant the shader
/// reads (its uniforms).
pub(crate) struct Shader<'a> {
    pub(crate) spec: SurfaceSpec<'a>,
    /// The frame, device px.
    pub(crate) size: [usize; 2],
    /// The surface's top-left in the window, device px (rounded: the grain
    /// lies on the window's device pixels).
    pub(crate) origin_px: [i32; 2],
    pub(crate) radii: [f32; 4],
    pub(crate) shape: Option<&'a ShapeField>,
    /// Width, depth, softness, inset (1) or raised (0).
    pub(crate) bevel: [f32; 4],
    /// The cushion: width, depth.
    pub(crate) cushion: [f32; 2],
    /// The pillow: its effective width and depth, logical px.
    pub(crate) pillow: [f32; 2],
    pub(crate) pillow_foot: f32,
    /// Linear.
    pub(crate) tint: [f32; 3],
    pub(crate) rough: [f32; 2],
    pub(crate) variation: f32,
    pub(crate) metal: f32,
    pub(crate) aniso: bool,
    /// Toward the sun (unit), when the light is the sun.
    pub(crate) sun: [f32; 3],
    /// The point light (x, y desktop points, height), when it is one.
    pub(crate) point: Option<[f32; 3]>,
    /// Linear, times the intensity.
    pub(crate) light_color: [f32; 3],
    pub(crate) ambient: f32,
    /// The surface on the screen of its wallpaper: x, y (logical px from the
    /// screen's corner), the screen's width and height.
    pub(crate) env_rect: [f32; 4],
    /// The surface's top-left on the desktop (a point light's frame).
    pub(crate) desktop_origin: [f32; 2],
    pub(crate) wallpaper: Option<&'a Wallpaper>,
    /// The flat environment without a wallpaper, linear.
    pub(crate) env_flat: [f32; 3],
    /// The accent's glow: rgb (linear, times its strength), its reach (px).
    pub(crate) glow: [f32; 4],
    /// The window's god rays (the CPU draws them; a frame with rays never
    /// goes to the GPU) and the surface's top-left in the window, logical px.
    pub(crate) rays: Option<(crate::texture::MaterialRays, [f32; 2])>,
}

impl Shader<'_> {
    fn size_l(&self) -> [f32; 2] {
        [self.size[0] as f32 / self.spec.dpr, self.size[1] as f32 / self.spec.dpr]
    }

    /// The shader's `shapeD`: signed distance to the edge, logical px.
    fn shape_d(&self, p: [f32; 2]) -> f32 {
        let s = self.size_l();
        match self.shape {
            Some(field) => field.distance_at(p[0], p[1]),
            None => sd_round([p[0] - s[0] * 0.5, p[1] - s[1] * 0.5], [s[0] * 0.5, s[1] * 0.5], self.radii),
        }
    }

    fn pillow_dist(&self, p: [f32; 2], d: f32) -> f32 {
        if self.shape.is_some() {
            return (-d).max(0.0);
        }
        let s = self.size_l();
        let bottom = if self.pillow_foot > 0.0 {
            (s[1] - p[1]) / self.pillow_foot
        } else {
            1e5
        };
        let k = self.pillow[0] * 0.75;
        let across = smin(p[1], bottom, k);
        let along = smin(p[0], s[0] - p[0], k);
        smin(across, along, k).min(-d).max(0.0)
    }

    fn pillow_t(&self, p: [f32; 2]) -> f32 {
        if self.pillow[0] > 0.0 {
            0.5 - 0.5 * (PI * (self.pillow_dist(p, self.shape_d(p)) / self.pillow[0]).clamp(0.0, 1.0)).cos()
        } else {
            1.0
        }
    }

    fn pillow_h(&self, p: [f32; 2]) -> f32 {
        if self.pillow[0] > 0.0 {
            self.pillow[1] * self.pillow_t(p)
        } else {
            0.0
        }
    }

    fn bevel_h(&self, d: f32) -> f32 {
        let t = (-d / self.bevel[0].max(1e-3)).clamp(0.0, 1.0);
        let prof = mix(t, t * t * (3.0 - 2.0 * t), self.bevel[2]);
        let b = self.bevel[1] * self.bevel[0] * 0.5 * if self.bevel[3] > 0.5 { 1.0 - prof } else { prof };
        let c = if self.cushion[0] > 0.0 {
            self.cushion[1] * smoothstep(0.0, 1.0, (-d / self.cushion[0]).clamp(0.0, 1.0))
        } else {
            0.0
        };
        b + c
    }

    fn env_at(&self, p: [f32; 2], n: [f32; 3], rough: f32) -> [f32; 3] {
        let Some(w) = self.wallpaper else {
            return self.env_flat;
        };
        let screen = [
            self.env_rect[0] + p[0] + n[0] * 220.0,
            self.env_rect[1] + p[1] + n[1] * 220.0,
        ];
        let uv = [
            (screen[0] / self.env_rect[2]).clamp(0.0, 1.0),
            (screen[1] / self.env_rect[3]).clamp(0.0, 1.0),
        ];
        // The GPU samples the sRGB bytes and applies `pow(c, 2.2)`; the mips
        // here are already linear (the sRGB curve, a hair from 2.2).
        w.sample(uv, 1.5 + rough * 7.0)
    }

    /// One device pixel (`x`, `y` from the frame's top-left) with its grain:
    /// premultiplied RGBA8.
    pub(crate) fn pixel(&self, x: usize, y: usize, grain: Grain) -> [u8; 4] {
        let dpr = self.spec.dpr;
        let p = [(x as f32 + 0.5) / dpr, (y as f32 + 0.5) / dpr];
        let d = self.shape_d(p);
        let alpha = (0.5 - d * dpr).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return [0; 4];
        }
        let e = 0.5 / dpr;
        let mut n = grain.n;
        let mut ao = grain.ao;
        if (self.bevel[0] > 0.0 && self.bevel[1] > 0.0) || self.cushion[0] > 0.0 || self.pillow[0] > 0.0 {
            let h_at = |q: [f32; 2]| self.bevel_h(self.shape_d(q)) + self.pillow_h(q);
            let bx = (h_at([p[0] + e, p[1]]) - h_at([p[0] - e, p[1]])) / (2.0 * e);
            let by = (h_at([p[0], p[1] + e]) - h_at([p[0], p[1] - e])) / (2.0 * e);
            let mut bg = [bx, by];
            let bl = bg[0].hypot(bg[1]);
            if bl > 3.7 {
                bg = [bg[0] * 3.7 / bl, bg[1] * 3.7 / bl];
            }
            let bn = normalize([-bg[0], -bg[1], 1.0]);
            n = normalize([bn[0] * n[2] + n[0], bn[1] * n[2] + n[1], n[2] * bn[2]]);
        }
        if self.pillow[0] > 0.0 {
            ao *= mix(0.9, 1.0, self.pillow_t(p));
        }
        let v = [0.0, 0.0, 1.0];
        let nv = n[2].max(1e-3);
        let k = (1.0 + self.variation * (grain.luma - 0.62) * 2.0).clamp(0.0, 2.0);
        let albedo = self.tint.map(|c| c * k);
        let rough = mix(self.rough[0], self.rough[1], grain.rough);
        let a = (rough * rough).max(0.002);
        let f0 = albedo.map(|c| mix(0.04, c, self.metal));
        let hemi = (1.0 - 0.35 * n[1]).clamp(0.55, 1.45);

        let l = match self.point {
            Some(lp) => normalize([
                lp[0] - (self.desktop_origin[0] + p[0]),
                lp[1] - (self.desktop_origin[1] + p[1]),
                lp[2],
            ]),
            None => self.sun,
        };
        let h = normalize([l[0] + v[0], l[1] + v[1], l[2] + v[2]]);
        let nl = dot(n, l).max(0.0);
        let nh = dot(n, h).max(0.0);
        let vh = dot(v, h).max(0.0);
        let dd = if self.aniso {
            ggx_aniso(n, h, (a * 0.35).max(0.002), (a * 1.8).max(0.01))
        } else {
            ggx(nh, a)
        };
        let fs = schlick(f0, vh);
        let g = smith_g(nl.max(1e-3), nv, a);
        let spec_k = dd * g / (4.0 * nl * nv).max(1e-4) * nl;
        let spec = fs.map(|f| f * spec_k);
        let wrap = 0.45 * (1.0 - self.metal);
        let nlw = ((dot(n, l) + wrap) / (1.0 + wrap)).max(0.0);
        let flatw = (l[2] + wrap) / (1.0 + wrap);
        let diffuse_k = (1.0 - self.metal) * (self.ambient * ao * hemi + (1.0 - self.ambient) * nlw / flatw.max(0.05));
        let light_color = match &self.rays {
            Some((rays, origin)) => {
                let k = ray_factor(rays, origin[0] + p[0], origin[1] + p[1]);
                self.light_color.map(|c| c * k)
            }
            None => self.light_color,
        };
        let r = [2.0 * n[2] * n[0], 2.0 * n[2] * n[1], 2.0 * n[2] * n[2] - 1.0];
        let up = -r[1];
        let studio = 0.28
            + 0.34 * smoothstep(-0.5, 0.0, up)
            + 0.72 * smoothstep(0.0, 0.35, up)
            + 0.5 * dot(r, l).max(0.0).powf(12.0);
        let wall = self.env_at(p, n, rough);
        let room = wall.map(|w| mix(studio, w * 1.25, 0.45));
        let fe = schlick(f0, nv);
        let env_k = mix(0.25, 1.0, 1.0 - rough) * ao;
        let env_w = mix(0.55 * (1.0 - 0.5 * rough), 1.0, self.metal);
        let mut col = [0.0_f32; 3];
        for c in 0..3 {
            let diffuse = albedo[c] * diffuse_k;
            let satin = self.metal * albedo[c] * (0.22 * hemi + 0.3 * nl);
            let env = fe[c] * mix(studio * 1.1, room[c], self.metal) * env_k;
            col[c] = (diffuse + satin) * light_color[c]
                + spec[c] * light_color[c] * (1.0 - self.ambient)
                + env * env_w;
        }
        if self.glow[3] > 0.0 {
            let below = self.size_l()[1] - p[1];
            let k = (-below / self.glow[3]).exp() * (0.55 + 0.45 * ao) * mix(1.0, 0.5, self.metal);
            for c in 0..3 {
                col[c] += self.glow[c] * k;
            }
        }
        let peak = col[0].max(col[1]).max(col[2]);
        if peak > 0.86 {
            let k = (0.86 + 0.14 * (1.0 - (-(peak - 0.86) / 0.14).exp())) / peak;
            let scaled = col.map(|c| c * k);
            let luma = 0.2126 * scaled[0] + 0.7152 * scaled[1] + 0.0722 * scaled[2];
            let t = ((peak - 0.86) * 0.4).clamp(0.0, 0.25);
            col = scaled.map(|c| mix(c, luma, t));
        }
        // The dither is hashed on the WINDOW's device pixel, so two surfaces
        // side by side dither as one.
        let wx = (self.origin_px[0] + x as i32) as f32;
        let wy = (self.origin_px[1] + y as i32) as f32;
        let dither = (surface::hash12([wx + 0.5 + 0.37, wy + 0.5 + 0.37]) - 0.5) / 255.0;
        let mut out = [0_u8; 4];
        for c in 0..3 {
            let s = encode(col[c]) + dither;
            out[c] = ((s * alpha).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        out[3] = (alpha * 255.0 + 0.5) as u8;
        out
    }

    /// Every pixel of the frame, RGBA8 premultiplied, on several cores for a
    /// large frame.
    pub(crate) fn render(&self) -> Vec<u8> {
        let [w, h] = self.size;
        let mut out = vec![0_u8; w * h * 4];
        if out.is_empty() {
            return out;
        }
        // The grain of every tile the frame touches, in window device px.
        let smooth = self.spec.source == MaterialSource::Smooth;
        let t = TILE as i32;
        let tx = (self.origin_px[0].div_euclid(t), (self.origin_px[0] + w as i32 - 1).div_euclid(t) + 1);
        let ty = (self.origin_px[1].div_euclid(t), (self.origin_px[1] + h as i32 - 1).div_euclid(t) + 1);
        let tiles = if smooth {
            Vec::new()
        } else {
            grain_tiles(&self.spec, tx, ty)
        };
        let columns = (tx.1 - tx.0) as usize;
        let grain_at = |x: usize, y: usize| -> Grain {
            if smooth {
                return Grain::SMOOTH;
            }
            let wx = self.origin_px[0] + x as i32;
            let wy = self.origin_px[1] + y as i32;
            let (cx, cy) = ((wx.div_euclid(t) - tx.0) as usize, (wy.div_euclid(t) - ty.0) as usize);
            let (ix, iy) = (wx.rem_euclid(t) as usize, wy.rem_euclid(t) as usize);
            tiles[cy * columns + cx][iy * TILE + ix]
        };
        let rows = |first: usize, chunk: &mut [u8]| {
            for (row, line) in chunk.chunks_exact_mut(w * 4).enumerate() {
                let y = first + row;
                for x in 0..w {
                    let px = self.pixel(x, y, grain_at(x, y));
                    line[x * 4..x * 4 + 4].copy_from_slice(&px);
                }
            }
        };
        let threads = if w * h >= 64 * 1024 {
            std::thread::available_parallelism()
                .map_or(1, core::num::NonZeroUsize::get)
                .min(8)
        } else {
            1
        };
        if threads <= 1 {
            rows(0, &mut out);
            return out;
        }
        let band = h.div_ceil(threads);
        std::thread::scope(|scope| {
            for (i, chunk) in out.chunks_mut(band * w * 4).enumerate() {
                let rows = &rows;
                scope.spawn(move || rows(i * band, chunk));
            }
        });
        out
    }
}

/// The ray factor of the window's god rays at window point (`x`, `y`): 1
/// off the rays, up to `1 + strength` at a band's centre.
pub(crate) fn ray_factor(rays: &crate::texture::MaterialRays, x: f32, y: f32) -> f32 {
    if rays.strength <= 0.0 || rays.width <= 0.0 {
        return 1.0;
    }
    let a = rays.angle_deg.to_radians();
    let across = x * a.sin() + y * a.cos() - rays.phase;
    let d = if rays.spacing > 0.0 {
        across.rem_euclid(rays.spacing) - rays.spacing * 0.5
    } else {
        across
    };
    let t = d / (rays.width / 2.355);
    if t.abs() > 4.0 {
        1.0
    } else {
        1.0 + rays.strength * (-0.5 * t * t).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rounded_rect_distance_is_zero_on_its_edge_and_negative_inside() {
        let half = [50.0, 20.0];
        let r = [8.0; 4];
        assert!(sd_round([0.0, 0.0], half, r) < -19.0);
        assert!(sd_round([50.0, 0.0], half, r).abs() < 1e-4);
        // On the corner's arc.
        let c = [
            42.0 + 8.0 * core::f32::consts::FRAC_1_SQRT_2,
            -12.0 - 8.0 * core::f32::consts::FRAC_1_SQRT_2,
        ];
        assert!(sd_round(c, half, r).abs() < 1e-3);
    }

    #[test]
    fn the_smooth_minimum_rounds_only_near_the_crossing() {
        assert_eq!(smin(1.0, 10.0, 2.0), 1.0);
        assert!(smin(1.0, 1.0, 2.0) < 1.0);
    }

    #[test]
    fn srgb_encoding_is_monotonic_and_ends_at_one() {
        let mut last = -1.0;
        for i in 0..=1000 {
            let v = encode(i as f32 / 1000.0);
            assert!(v >= last);
            last = v;
        }
        assert!((encode(1.0) - 1.0).abs() < 1e-5);
    }
}
