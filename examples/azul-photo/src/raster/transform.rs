//! Geometric transforms of a layer or the canvas: affine maps with nearest
//! or bilinear sampling, exact flips and quarter turns, crop, resize, move.

use serde::{Deserialize, Serialize};

use super::{blend, geom::IRect, tile::TileGrid};

/// How a transform samples between source pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interp {
    Nearest,
    Bilinear,
}

/// `x' = a x + c y + e`, `y' = b x + d y + f` (source to destination).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    #[must_use]
    pub const fn translate(tx: f32, ty: f32) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::identity()
        }
    }

    #[must_use]
    pub const fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::identity()
        }
    }

    /// A rotation by `rad` (clockwise on screen, where y points down).
    #[must_use]
    pub fn rotate(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self`, then `next`.
    #[must_use]
    pub fn then(&self, next: &Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// `m` about the point (`cx`, `cy`) instead of the origin.
    #[must_use]
    pub fn about(cx: f32, cy: f32, m: &Self) -> Self {
        Self::translate(-cx, -cy)
            .then(m)
            .then(&Self::translate(cx, cy))
    }

    #[must_use]
    pub fn invert(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let ia = self.d / det;
        let ib = -self.b / det;
        let ic = -self.c / det;
        let id = self.a / det;
        Some(Self {
            a: ia,
            b: ib,
            c: ic,
            d: id,
            e: -(ia * self.e + ic * self.f),
            f: -(ib * self.e + id * self.f),
        })
    }

    #[inline]
    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

/// The source pixel (`x`, `y`) as premultiplied f32; transparent outside
/// unless `clamp` holds the edge.
#[inline]
fn texel(rgba: &[u8], w: usize, h: usize, x: isize, y: isize, clamp: bool) -> [f32; 4] {
    let (x, y) = if clamp {
        (x.clamp(0, w as isize - 1), y.clamp(0, h as isize - 1))
    } else if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
        return [0.0; 4];
    } else {
        (x, y)
    };
    let i = (y as usize * w + x as usize) * 4;
    blend::to_premul([rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]])
}

/// Sample at the continuous point (`x`, `y`) (pixel `i` spans `i..i+1`).
fn sample(rgba: &[u8], w: usize, h: usize, x: f32, y: f32, interp: Interp, clamp: bool) -> [u8; 4] {
    if w == 0 || h == 0 {
        return [0; 4];
    }
    match interp {
        Interp::Nearest => {
            let p = texel(rgba, w, h, x.floor() as isize, y.floor() as isize, clamp);
            blend::premul_to_rgba8(p)
        }
        Interp::Bilinear => {
            let fx = x - 0.5;
            let fy = y - 0.5;
            let x0 = fx.floor();
            let y0 = fy.floor();
            let tx = fx - x0;
            let ty = fy - y0;
            let (x0, y0) = (x0 as isize, y0 as isize);
            let p00 = texel(rgba, w, h, x0, y0, clamp);
            let p10 = texel(rgba, w, h, x0 + 1, y0, clamp);
            let p01 = texel(rgba, w, h, x0, y0 + 1, clamp);
            let p11 = texel(rgba, w, h, x0 + 1, y0 + 1, clamp);
            let mut out = [0.0f32; 4];
            for c in 0..4 {
                let top = p00[c] + (p10[c] - p00[c]) * tx;
                let bottom = p01[c] + (p11[c] - p01[c]) * tx;
                out[c] = top + (bottom - top) * ty;
            }
            blend::premul_to_rgba8(out)
        }
    }
}

/// `src` mapped through `m` (source to destination pixels) into an
/// `out_w` x `out_h` image; nothing maps to the rest (transparent).
#[must_use]
pub fn transform_grid(src: &TileGrid, m: &Affine, interp: Interp, out_w: u32, out_h: u32) -> TileGrid {
    let Some(inv) = m.invert() else {
        return TileGrid::new(out_w, out_h);
    };
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let rgba = src.to_rgba();
    // Only the destination pixels the source can reach.
    let corners = [
        m.apply(0.0, 0.0),
        m.apply(sw as f32, 0.0),
        m.apply(0.0, sh as f32),
        m.apply(sw as f32, sh as f32),
    ];
    let x0 = corners.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let y0 = corners.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let x1 = corners.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let y1 = corners.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let reach = IRect::covering(x0, y0, x1, y1).inflate(1);
    let mut out = TileGrid::new(out_w, out_h);
    let Some(reach) = reach.intersect(&out.bounds()) else {
        return out;
    };
    let mut buf = vec![0u8; (reach.w as usize) * (reach.h as usize) * 4];
    for y in 0..reach.h {
        for x in 0..reach.w {
            let (dx, dy) = ((reach.x + x) as f32 + 0.5, (reach.y + y) as f32 + 0.5);
            let (sx, sy) = inv.apply(dx, dy);
            let p = sample(&rgba, sw, sh, sx, sy, interp, false);
            let i = ((y * reach.w + x) * 4) as usize;
            buf[i..i + 4].copy_from_slice(&p);
        }
    }
    out.write_rect(reach, &buf);
    out
}

/// A mirror image (`horizontal`: left and right swap).
#[must_use]
pub fn flip(grid: &TileGrid, horizontal: bool) -> TileGrid {
    let (w, h) = (grid.width() as usize, grid.height() as usize);
    let src = grid.to_rgba();
    let mut out = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = if horizontal { (w - 1 - x, y) } else { (x, h - 1 - y) };
            let s = (sy * w + sx) * 4;
            let d = (y * w + x) * 4;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    TileGrid::from_rgba(grid.width(), grid.height(), &out)
}

/// A quarter turn; width and height swap.
#[must_use]
pub fn rotate90(grid: &TileGrid, clockwise: bool) -> TileGrid {
    let (w, h) = (grid.width() as usize, grid.height() as usize);
    let src = grid.to_rgba();
    let (nw, nh) = (h, w);
    let mut out = vec![0u8; src.len()];
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = if clockwise { (y, h - 1 - x) } else { (w - 1 - y, x) };
            let s = (sy * w + sx) * 4;
            let d = (y * nw + x) * 4;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    TileGrid::from_rgba(nw as u32, nh as u32, &out)
}

/// The rect `r` of the image as a new image of `r`'s size.
#[must_use]
pub fn crop(grid: &TileGrid, r: IRect) -> TileGrid {
    TileGrid::from_rgba(r.w.max(0) as u32, r.h.max(0) as u32, &grid.read_rect(r))
}

/// The image moved by whole pixels; what leaves the image is gone.
#[must_use]
pub fn offset(grid: &TileGrid, dx: i32, dy: i32) -> TileGrid {
    let moved = grid.read_rect(grid.bounds().offset(-dx, -dy));
    TileGrid::from_rgba(grid.width(), grid.height(), &moved)
}

/// The image on a canvas of `width` x `height`, its top-left at (`x`, `y`).
#[must_use]
pub fn place(grid: &TileGrid, width: u32, height: u32, x: i32, y: i32) -> TileGrid {
    let src = IRect::new(-x, -y, width as i32, height as i32);
    TileGrid::from_rgba(width, height, &grid.read_rect(src))
}

/// The image resampled to `width` x `height` (edges held, so a resize never
/// fades the border). Shrinking by more than half averages the footprint.
#[must_use]
pub fn resize(grid: &TileGrid, width: u32, height: u32, interp: Interp) -> TileGrid {
    let (sw, sh) = (grid.width() as usize, grid.height() as usize);
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || sw == 0 || sh == 0 {
        return TileGrid::new(width, height);
    }
    let src = grid.to_rgba();
    let fx = sw as f32 / w as f32;
    let fy = sh as f32 / h as f32;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let p = if interp == Interp::Bilinear && (fx > 2.0 || fy > 2.0) {
                box_average(&src, sw, sh, x as f32 * fx, y as f32 * fy, fx, fy)
            } else {
                let sx = (x as f32 + 0.5) * fx;
                let sy = (y as f32 + 0.5) * fy;
                sample(&src, sw, sh, sx, sy, interp, true)
            };
            let i = (y * w + x) * 4;
            out[i..i + 4].copy_from_slice(&p);
        }
    }
    TileGrid::from_rgba(width, height, &out)
}

/// The average of the source pixels whose centres fall in the footprint.
fn box_average(src: &[u8], sw: usize, sh: usize, x: f32, y: f32, fw: f32, fh: f32) -> [u8; 4] {
    let x0 = (x.floor() as usize).min(sw - 1);
    let y0 = (y.floor() as usize).min(sh - 1);
    let x1 = ((x + fw).ceil() as usize).clamp(x0 + 1, sw);
    let y1 = ((y + fh).ceil() as usize).clamp(y0 + 1, sh);
    let mut acc = [0.0f32; 4];
    let mut n = 0.0;
    for sy in y0..y1 {
        for sx in x0..x1 {
            let i = (sy * sw + sx) * 4;
            let p = blend::to_premul([src[i], src[i + 1], src[i + 2], src[i + 3]]);
            for c in 0..4 {
                acc[c] += p[c];
            }
            n += 1.0;
        }
    }
    blend::premul_to_rgba8(acc.map(|v| v / n))
}
