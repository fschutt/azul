//! What a material's surface IS at a point, before any light falls on it:
//! its height (for the slope), its luma (how far it moves the tint), how
//! rough it is and how occluded. Each material is a function of the position
//! in logical px, built from shared noise - value noise, fbm, Worley cells -
//! so it can be evaluated at any resolution: at every device pixel of a 2x
//! screen there is detail, never a texel stretched over several pixels.
//!
//! These are line-for-line ports of the material shader's GLSL
//! (`shader.glsl` next to this file): the CPU and the GPU must draw the same
//! surface. GLSL's `mix`, `fract`, `step`, `mod` and `smoothstep` are spelled
//! out below with GLSL's definitions.

// The formulas keep the shader's letters (shader.glsl): p, n, h, l, r, f0...
#![allow(clippy::many_single_char_names)]

use super::ColourlessTile;

pub(crate) const PI: f32 = core::f32::consts::PI;

/// One sample of a surface: `h` height (logical px), `luma` 0..1 (0.62 is
/// the tint as it is), `rough` 0..1 (where between the material's two
/// roughnesses), `ao` ambient occlusion 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sample {
    pub(crate) h: f32,
    pub(crate) luma: f32,
    pub(crate) rough: f32,
    pub(crate) ao: f32,
}

impl Sample {
    /// A smooth surface: level, the tint as it is, half rough, unoccluded.
    pub(crate) const SMOOTH: Self = Self {
        h: 0.0,
        luma: 0.62,
        rough: 0.5,
        ao: 1.0,
    };
}

#[inline]
pub(crate) fn fract(x: f32) -> f32 {
    x - x.floor()
}

#[inline]
pub(crate) fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

#[inline]
pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// GLSL's `mod`: `x - y * floor(x / y)`.
#[inline]
fn glsl_mod(x: f32, y: f32) -> f32 {
    x - y * (x / y).floor()
}

/// GLSL's `step(edge, x)`: 0 below the edge, 1 from it on.
#[inline]
fn step(edge: f32, x: f32) -> f32 {
    if x < edge {
        0.0
    } else {
        1.0
    }
}

/// A hash of a point, 0..1 (Dave Hoskins' "hash without sine").
pub(crate) fn hash12(p: [f32; 2]) -> f32 {
    let mut p3 = [fract(p[0] * 0.1031), fract(p[1] * 0.1031), fract(p[0] * 0.1031)];
    let d = p3[0] * (p3[1] + 33.33) + p3[1] * (p3[2] + 33.33) + p3[2] * (p3[0] + 33.33);
    p3 = [p3[0] + d, p3[1] + d, p3[2] + d];
    fract((p3[0] + p3[1]) * p3[2])
}

/// Two hashes of a point, each 0..1.
fn hash22(p: [f32; 2]) -> [f32; 2] {
    let mut p3 = [fract(p[0] * 0.1031), fract(p[1] * 0.1030), fract(p[0] * 0.0973)];
    let d = p3[0] * (p3[1] + 33.33) + p3[1] * (p3[2] + 33.33) + p3[2] * (p3[0] + 33.33);
    p3 = [p3[0] + d, p3[1] + d, p3[2] + d];
    [fract((p3[0] + p3[1]) * p3[2]), fract((p3[0] + p3[2]) * p3[1])]
}

/// Value noise: hashes at the integer lattice, blended smoothly.
pub(crate) fn vnoise(p: [f32; 2]) -> f32 {
    let i = [p[0].floor(), p[1].floor()];
    let f = [p[0] - i[0], p[1] - i[1]];
    let u = [f[0] * f[0] * (3.0 - 2.0 * f[0]), f[1] * f[1] * (3.0 - 2.0 * f[1])];
    let a = hash12(i);
    let b = hash12([i[0] + 1.0, i[1]]);
    let c = hash12([i[0], i[1] + 1.0]);
    let d = hash12([i[0] + 1.0, i[1] + 1.0]);
    mix(mix(a, b, u[0]), mix(c, d, u[0]), u[1])
}

/// Four octaves of value noise, 0..1.
pub(crate) fn fbm(p: [f32; 2]) -> f32 {
    let mut s = 0.0;
    let mut a = 0.5;
    let mut p = p;
    for _ in 0..4 {
        s += a * vnoise(p);
        p = [p[0] * 2.03 + 17.1, p[1] * 2.03 + 17.1];
        a *= 0.5;
    }
    s / 0.9375
}

/// The distances to the nearest and the second nearest cell point (Worley
/// F1, F2), one jittered point per lattice cell.
fn worley(p: [f32; 2]) -> [f32; 2] {
    let i = [p[0].floor(), p[1].floor()];
    let f = [p[0] - i[0], p[1] - i[1]];
    let mut d1 = 8.0_f32;
    let mut d2 = 8.0_f32;
    for y in -1..=1 {
        for x in -1..=1 {
            let g = [x as f32, y as f32];
            let h = hash22([i[0] + g[0], i[1] + g[1]]);
            let o = [h[0] * 0.8 + 0.1, h[1] * 0.8 + 0.1];
            let d = (g[0] + o[0] - f[0]).hypot(g[1] + o[1] - f[1]);
            if d < d1 {
                d2 = d1;
                d1 = d;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    [d1, d2]
}

/// One layer of grain cells `gs` px across, their edges warped so no cell is
/// a polygon: the dome of each cell and the crease between cells.
fn grain_cells(p: [f32; 2], gs: f32, seed: f32) -> [f32; 2] {
    let warp = [p[0] * 0.6 / gs + seed, p[1] * 0.6 / gs + seed];
    let q = [
        p[0] / gs + 0.45 * vnoise(warp) + seed,
        p[1] / gs + 0.45 * vnoise([warp[0] + 19.7, warp[1] + 19.7]) + seed,
    ];
    let w = worley(q);
    let edge = w[1] - w[0];
    [smoothstep(0.0, 0.6, edge).sqrt(), 1.0 - smoothstep(0.0, 0.14, edge)]
}

/// Nappa leather: fine grain cells (two layers blended so the grain is finer
/// in some places than others), pores, wandering wrinkles, broad folds and
/// mottling. `gs` is the grain cell, logical px; `dpr` fades out a grain finer
/// than about a device pixel instead of letting it shimmer. Without `luma`
/// only the height is computed (the slope's neighbours need nothing else).
pub(crate) fn leather(p: [f32; 2], gs: f32, dpr: f32, luma: bool) -> Sample {
    let m = smoothstep(0.3, 0.7, fbm([p[0] * 0.006 + 4.2, p[1] * 0.006 + 4.2]));
    let g1 = grain_cells(p, gs * 0.85, 0.0);
    let g2 = grain_cells(p, gs * 1.25, 37.0);
    let dome = mix(g1[0], g2[0], m);
    let crease = mix(g1[1], g2[1], m);
    let lod = ((gs * dpr - 1.1) / 1.6).clamp(0.0, 1.0);
    let pores = fbm([p[0] * 2.2 / gs, p[1] * 2.2 / gs]);
    let wp = [
        p[0] * 0.02 + 1.6 * vnoise([p[0] * 0.006 + 2.0, p[1] * 0.006 + 2.0]),
        p[1] * 0.02 + 1.6 * vnoise([p[0] * 0.006 + 8.0, p[1] * 0.006 + 8.0]),
    ];
    let wrinkle = 1.0 - (fbm([wp[0] + 3.1, wp[1] + 3.1]) * 2.0 - 1.0).abs();
    let wrinkle = 0.55 * wrinkle * wrinkle;
    let fold = fbm([p[0] * 0.007 + 9.0, p[1] * 0.007 + 9.0]);
    let h = lod * gs * (0.05 * dome - 0.035 * crease + 0.012 * pores) + 0.55 * wrinkle + 1.3 * fold;
    if !luma {
        return Sample { h, ..Sample::SMOOTH };
    }
    let mottle = fbm([p[0] * 0.0045 + 1.7, p[1] * 0.0045 + 1.7]) - 0.5;
    let blotch = fbm([p[0] * 0.017 + 8.3, p[1] * 0.017 + 8.3]) - 0.5;
    let l = 0.62
        + lod * (0.035 * dome - 0.09 * crease + 0.02 * (pores - 0.5))
        + 0.05 * (wrinkle - 0.5)
        + 0.06 * (fold - 0.5)
        + 0.16 * mottle
        + 0.07 * blotch;
    Sample {
        h,
        luma: l,
        rough: (0.35 + 0.45 * crease * lod + 0.2 * (1.0 - dome)).clamp(0.0, 1.0),
        ao: 1.0 - 0.18 * crease * lod,
    }
}

/// Brushed aluminium: value noise stretched 1 : 140 along the brushing (x).
pub(crate) fn aluminium(p: [f32; 2]) -> Sample {
    let a = vnoise([p[0] * 0.012, p[1] * 1.7]);
    let b = vnoise([p[0] * 0.05 + 7.0, p[1] * 4.3 + 7.0]);
    let c = vnoise([p[0] * 0.4 + 3.0, p[1] * 11.0 + 3.0]);
    let d = vnoise([p[0] * 0.35 + 11.0, p[1] * 3.0 + 11.0]);
    let f = vnoise([p[0] * 1.3 + 23.0, p[1] * 7.0 + 23.0]);
    let lines = 0.38 * a + 0.22 * b + 0.12 * c + 0.16 * d + 0.12 * f;
    Sample {
        h: 0.26 * lines,
        luma: 0.72 + 0.36 * lines + 0.08 * (fbm([p[0] * 0.03, p[1] * 0.03]) - 0.5),
        rough: 0.3 + 0.7 * (1.0 - lines),
        ao: 1.0,
    }
}

/// Denim: a 3/1 twill of sine-profiled threads 2.1 px apart, slubbed, with
/// fibres.
pub(crate) fn denim(p: [f32; 2]) -> Sample {
    let c = [(p[0] / 2.1).floor(), (p[1] / 2.1).floor()];
    let f = [fract(p[0] / 2.1), fract(p[1] / 2.1)];
    let over = step(glsl_mod(c[0] - c[1] + 400.0, 4.0), 2.5);
    let warp = (f[0] * PI).sin();
    let weft = (f[1] * PI).sin();
    let slub = vnoise([p[0] * 0.6, p[1] * 0.035]);
    let k = mix(0.6, 1.0, over);
    let fib = fbm([p[0] * 1.4 * k + over * 23.0, p[1] * 4.0 * k + over * 23.0]);
    let over = over > 0.5;
    Sample {
        h: if over { 0.55 * warp + 0.15 * fib } else { 0.35 * weft + 0.15 * fib },
        luma: if over { 0.32 + 0.28 * slub + 0.15 * fib } else { 0.86 + 0.1 * fib },
        rough: 0.6 + 0.4 * fib,
        ao: 0.72 + 0.28 * if over { warp } else { weft },
    }
}

/// Vellum: soft blotches, fibres along the skin, ridged creases and specks.
pub(crate) fn vellum(p: [f32; 2]) -> Sample {
    let blot = fbm([p[0] * 0.012, p[1] * 0.012]);
    let fib = fbm([p[0] * 0.09 + p[1] * 0.03, p[1] * 0.35]);
    let cr = (1.0 - (fbm([p[0] * 0.04, p[1] * 0.04]) * 2.0 - 1.0).abs()).powf(6.0);
    let speck = step(0.992, hash12([(p[0] * 1.5).floor(), (p[1] * 1.5).floor()]));
    Sample {
        h: 0.6 * blot + 0.12 * fib - 0.3 * cr,
        luma: 0.94 - 0.16 * blot - 0.08 * fib - 0.22 * cr - 0.3 * speck,
        rough: 0.85,
        ao: 1.0 - 0.3 * cr,
    }
}

/// The cubic B-spline's four weights at fraction `v`.
fn cubic_weights(v: f32) -> [f32; 4] {
    let n = [1.0 - v, 2.0 - v, 3.0 - v, 4.0 - v];
    let s = n.map(|x| x * x * x);
    let x = s[0];
    let y = s[1] - 4.0 * s[0];
    let z = s[2] - 4.0 * s[1] + 6.0 * s[0];
    let w = 6.0 - x - y - z;
    [x / 6.0, y / 6.0, z / 6.0, w / 6.0]
}

/// The tile at `uv` (tile widths, it wraps) through a cubic B-spline made of
/// four bilinear taps, so its texels never show as squares. RGBA 0..1: R
/// luma, G / B the normal's x / y (0.5 = level), A occlusion.
pub(crate) fn bicubic(tile: &ColourlessTile, uv: [f32; 2]) -> [f32; 4] {
    let size = tile.side as f32;
    let inv = 1.0 / size;
    let mut st = [uv[0] * size - 0.5, uv[1] * size - 0.5];
    let fxy = [fract(st[0]), fract(st[1])];
    st = [st[0] - fxy[0], st[1] - fxy[1]];
    let xc = cubic_weights(fxy[0]);
    let yc = cubic_weights(fxy[1]);
    let c = [st[0] - 0.5, st[0] + 1.5, st[1] - 0.5, st[1] + 1.5];
    let s = [xc[0] + xc[1], xc[2] + xc[3], yc[0] + yc[1], yc[2] + yc[3]];
    let o = [
        (c[0] + xc[1] / s[0]) * inv,
        (c[1] + xc[3] / s[1]) * inv,
        (c[2] + yc[1] / s[2]) * inv,
        (c[3] + yc[3] / s[3]) * inv,
    ];
    let s0 = tile.bilinear([o[0], o[2]]);
    let s1 = tile.bilinear([o[1], o[2]]);
    let s2 = tile.bilinear([o[0], o[3]]);
    let s3 = tile.bilinear([o[1], o[3]]);
    let sx = s[0] / (s[0] + s[1]);
    let sy = s[2] / (s[2] + s[3]);
    let mut out = [0.0; 4];
    for k in 0..4 {
        out[k] = mix(mix(s3[k], s2[k], sx), mix(s1[k], s0[k], sx), sy);
    }
    out
}

/// The tile source: the colourless tile upsampled, its mottling laid over at
/// the source's real period, and micro-detail finer than a texel.
pub(crate) fn tile_sample(tile: &ColourlessTile, p: [f32; 2]) -> Sample {
    let t = bicubic(tile, [p[0] / 128.0, p[1] / 128.0]);
    let m = if tile.macro_period > 0.0 && tile.macro_side > 0 {
        0.82 + 0.36 * tile.macro_bilinear([p[0] / tile.macro_period, p[1] / tile.macro_period])
    } else {
        1.0
    };
    let detail = vnoise([p[0] * 1.7, p[1] * 1.7]) - 0.5
        + 0.5 * (vnoise([p[0] * 3.9 + 5.0, p[1] * 3.9 + 5.0]) - 0.5);
    Sample {
        h: 1.1 * t[0] + 0.16 * detail,
        luma: (t[0] * m + 0.06 * detail).clamp(0.0, 1.0),
        rough: 1.0 - t[0],
        ao: t[3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_noise_is_in_its_range_and_continuous() {
        for i in 0..2000 {
            let p = [i as f32 * 0.37 - 300.0, i as f32 * 0.21 + 11.0];
            let h = hash12(p);
            assert!((0.0..1.0).contains(&h), "hash {h}");
            let v = vnoise(p);
            assert!((0.0..=1.0).contains(&v), "value noise {v}");
            let f = fbm(p);
            assert!((0.0..=1.0 + 1e-5).contains(&f), "fbm {f}");
            // A step a hundredth of a pixel long moves value noise a little.
            let near = vnoise([p[0] + 0.01, p[1]]);
            assert!((near - v).abs() < 0.05, "continuous: {v} -> {near}");
        }
    }

    #[test]
    fn worley_finds_the_nearest_two_cell_points() {
        for i in 0..200 {
            let w = worley([i as f32 * 0.173, i as f32 * 0.311]);
            assert!(w[0] <= w[1], "{w:?}");
            assert!(w[0] < 1.5, "a cell point is always near: {w:?}");
        }
    }

    #[test]
    fn denim_is_a_three_one_twill() {
        // Along a row, three of every four thread crossings show the warp.
        let mut over = 0;
        for c in 0..400 {
            let x = (c as f32 + 0.5) * 2.1;
            if (glsl_mod((x / 2.1).floor() - 0.0 + 400.0, 4.0)) <= 2.5 {
                over += 1;
            }
        }
        assert_eq!(over, 300);
        let s = denim([3.0, 4.0]);
        assert!((0.0..=1.0).contains(&s.ao));
    }
}
