//! Filters: Gaussian blur and sharpen (unsharp mask), restricted to the
//! selection. The separable Gaussian is the one blur of the raster core: the
//! selection's feather uses it too.

use super::{selection::Mask, tile::TileGrid};

/// The worker threads a filter splits its rows across.
#[must_use]
pub fn workers() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .clamp(1, 16)
}

/// A normalised 1D Gaussian of `sigma`, `2 * ceil(3 sigma) + 1` taps.
#[must_use]
pub fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let sigma = sigma.max(0.01);
    let r = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut k: Vec<f32> = (-r..=r)
        .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    k
}

/// Blur one `w` x `h` plane in place: a horizontal then a vertical pass,
/// edges clamped, rows split across [`workers`].
pub fn blur_plane(plane: &mut [f32], w: usize, h: usize, sigma: f32) {
    if sigma <= 0.0 || w == 0 || h == 0 || plane.len() < w * h {
        return;
    }
    let k = gaussian_kernel(sigma);
    let r = (k.len() / 2) as isize;
    let rows_per = h.div_ceil(workers()).max(1);
    let mut tmp = vec![0.0f32; w * h];
    {
        let src: &[f32] = plane;
        let k = &k;
        std::thread::scope(|s| {
            for (ci, out) in tmp.chunks_mut(rows_per * w).enumerate() {
                s.spawn(move || {
                    let y0 = ci * rows_per;
                    for (ry, orow) in out.chunks_mut(w).enumerate() {
                        let y = y0 + ry;
                        let row = &src[y * w..(y + 1) * w];
                        for (x, o) in orow.iter_mut().enumerate() {
                            let mut acc = 0.0;
                            for (i, kv) in k.iter().enumerate() {
                                let sx = (x as isize + i as isize - r).clamp(0, w as isize - 1);
                                acc += row[sx as usize] * kv;
                            }
                            *o = acc;
                        }
                    }
                });
            }
        });
    }
    let src: &[f32] = &tmp;
    let k = &k;
    std::thread::scope(|s| {
        for (ci, out) in plane[..w * h].chunks_mut(rows_per * w).enumerate() {
            s.spawn(move || {
                let y0 = ci * rows_per;
                for (ry, orow) in out.chunks_mut(w).enumerate() {
                    let y = y0 + ry;
                    for (x, o) in orow.iter_mut().enumerate() {
                        let mut acc = 0.0;
                        for (i, kv) in k.iter().enumerate() {
                            let sy = (y as isize + i as isize - r).clamp(0, h as isize - 1);
                            acc += src[sy as usize * w + x] * kv;
                        }
                        *o = acc;
                    }
                }
            });
        }
    });
}

/// The image blurred with a Gaussian of `sigma` (premultiplied, so a
/// transparent neighbour does not darken an edge), only inside the selection
/// (a soft selection mixes).
#[must_use]
pub fn gaussian_blur(grid: &TileGrid, sigma: f32, selection: Option<&Mask>) -> TileGrid {
    let (w, h) = (grid.width() as usize, grid.height() as usize);
    let rgba = grid.to_rgba();
    let blurred = blur_rgba(&rgba, w, h, sigma);
    TileGrid::from_rgba(grid.width(), grid.height(), &mix_selected(&rgba, &blurred, selection))
}

/// The image sharpened: `original + amount * (original - blurred)` per
/// channel, with a Gaussian of `radius` as the blur.
#[must_use]
pub fn sharpen(grid: &TileGrid, amount: f32, radius: f32, selection: Option<&Mask>) -> TileGrid {
    let (w, h) = (grid.width() as usize, grid.height() as usize);
    let rgba = grid.to_rgba();
    let blurred = blur_rgba(&rgba, w, h, radius);
    let mut sharp = rgba.clone();
    for (i, (o, b)) in rgba.iter().zip(&blurred).enumerate() {
        if i % 4 == 3 {
            continue;
        }
        let o = f32::from(*o);
        let v = o + amount * (o - f32::from(*b));
        sharp[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    TileGrid::from_rgba(grid.width(), grid.height(), &mix_selected(&rgba, &sharp, selection))
}

/// Blur straight RGBA8 through premultiplied planes.
fn blur_rgba(rgba: &[u8], w: usize, h: usize, sigma: f32) -> Vec<u8> {
    let n = w * h;
    let mut planes = vec![vec![0.0f32; n]; 4];
    for i in 0..n {
        let a = f32::from(rgba[i * 4 + 3]);
        for c in 0..3 {
            planes[c][i] = f32::from(rgba[i * 4 + c]) * a / 255.0;
        }
        planes[3][i] = a;
    }
    for plane in &mut planes {
        blur_plane(plane, w, h, sigma);
    }
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let a = planes[3][i];
        if a <= 0.5 {
            continue;
        }
        for c in 0..3 {
            out[i * 4 + c] = (planes[c][i] * 255.0 / a).round().clamp(0.0, 255.0) as u8;
        }
        out[i * 4 + 3] = a.round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// `changed` where the selection covers a pixel, `original` elsewhere, mixed
/// by a soft selection's coverage.
#[must_use]
pub fn mix_selected(original: &[u8], changed: &[u8], selection: Option<&Mask>) -> Vec<u8> {
    let Some(mask) = selection else {
        return changed.to_vec();
    };
    let mut out = original.to_vec();
    for (i, m) in mask.data.iter().enumerate() {
        match *m {
            0 => {}
            255 => out[i * 4..i * 4 + 4].copy_from_slice(&changed[i * 4..i * 4 + 4]),
            m => {
                let t = f32::from(m) / 255.0;
                for c in 0..4 {
                    let o = f32::from(original[i * 4 + c]);
                    let n = f32::from(changed[i * 4 + c]);
                    out[i * 4 + c] = (o + (n - o) * t).round() as u8;
                }
            }
        }
    }
    out
}
