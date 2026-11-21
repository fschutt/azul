//! The composite of a group or layer that is only scaled and moved, as a separable blit.
//!
//! A zoom of a map is a matrix of the form `x' = sx * x + tx`, `y' = sy * y + ty`: no shear. The
//! general composite (`pixmap::blit_pixmap_affine_general`) maps every destination pixel back
//! through the inverse matrix in f64, fetches four texels with clamping and branches, blends in
//! f32 and rounds - 21 ns a pixel, 73 ms for the 3.5 MP of the root group of a 2560x1440 zoom
//! step. With no shear the source column and weight of a destination column do not depend on its
//! row, and the source row and weight of a row not on its column: both are computed once, the
//! bilinear lerp runs in 8.8 fixed point on two channels at a time, and the rows are
//! independent, so bands of them go to a few threads.
//!
//! Same edges as the general composite: the half-texel skirt, and taps outside the source rect are
//! transparent (not clamped), so the border row of an opaque group fades over one pixel instead of
//! smearing outward. The result agrees with the general composite to 3/255 per channel (the 8-bit
//! weights; the tests compare them).

use agg_rust::trans_affine::TransAffine;

use super::pixmap::{blend_over, AzulPixmap};

/// Fewest destination pixels a thread is worth: a band of rows is a few hundred microseconds of
/// work at the fixed-point speed, a thread spawn tens.
const PX_PER_THREAD: usize = 250_000;

/// `(sx, sy, tx, ty)` when `m` only scales and moves (no shear, no flip to zero), else `None`.
pub(crate) fn scale_translate_of(m: &TransAffine) -> Option<(f64, f64, f64, f64)> {
    let finite = m.sx.is_finite() && m.sy.is_finite() && m.tx.is_finite() && m.ty.is_finite();
    (finite
        && m.shx.abs() < 1e-12
        && m.shy.abs() < 1e-12
        && m.sx.abs() > 1e-12
        && m.sy.abs() > 1e-12)
        .then_some((m.sx, m.sy, m.tx, m.ty))
}

/// One source axis for a run of destination pixels: for each, the lower source texel and the 8-bit
/// weight of the upper one, or `None` for a pixel that maps outside the source (and its skirt).
fn axis(from: i32, to: i32, scale: f64, shift: f64, extent: i32) -> Vec<Option<(i32, u32)>> {
    let inv = 1.0 / scale;
    let inv_shift = -shift * inv;
    (from..to)
        .map(|d| {
            // The pixel's centre, mapped back to source texel space (centres at n + 0.5).
            let s = (f64::from(d) + 0.5) * inv + inv_shift - 0.5;
            if s <= -1.0 || s >= f64::from(extent) {
                return None;
            }
            let lo = s.floor() as i32;
            Some((lo, ((s - f64::from(lo)) * 256.0) as u32))
        })
        .collect()
}

/// `a` to `b` by `t / 256`, rounded, on the four channels of a packed pixel.
#[inline]
fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let rb = ((a & 0x00ff_00ff) * (256 - t) + (b & 0x00ff_00ff) * t + 0x0080_0080) >> 8;
    let ag = ((a >> 8) & 0x00ff_00ff) * (256 - t) + ((b >> 8) & 0x00ff_00ff) * t + 0x0080_0080;
    (rb & 0x00ff_00ff) | (ag & 0xff00_ff00)
}

/// The texel `x` of a row of 4-byte pixels; transparent outside the row.
#[inline]
fn texel(row: &[u8], x: i32, width: i32) -> u32 {
    if x < 0 || x >= width {
        return 0;
    }
    let i = x as usize * 4;
    u32::from_le_bytes([row[i], row[i + 1], row[i + 2], row[i + 3]])
}

/// Composite `src`, scaled by `(sx, sy)` and moved by `(tx, ty)` (source device pixels to
/// destination device pixels), over `dst` at opacity `op` (0..=255), inside the destination box
/// `(x0, y0, x1, y1)` (half-open, already clamped to `dst`).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::many_single_char_names)]
pub(crate) fn blit_scale_translate(
    src: &AzulPixmap,
    dst: &mut AzulPixmap,
    (sx, sy, tx, ty): (f64, f64, f64, f64),
    op: u32,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
) {
    let (sw, sh) = (src.width as i32, src.height as i32);
    let dw = dst.width as i32;
    if x1 <= x0 || y1 <= y0 || sw == 0 || sh == 0 {
        return;
    }
    let columns = axis(x0, x1, sx, tx, sw);
    let rows = axis(y0, y1, sy, ty, sh);
    let src_bytes: &[u8] = &src.data;
    let stride = dw as usize * 4;
    let band_rows = |first: i32, band: &mut [u8], rows_in_band: usize| {
        for r in 0..rows_in_band {
            let Some((y_lo, wy)) = rows[(first - y0) as usize + r] else {
                continue;
            };
            let row_at = |y: i32| -> &[u8] {
                if y < 0 || y >= sh {
                    &[]
                } else {
                    &src_bytes[y as usize * sw as usize * 4..(y as usize + 1) * sw as usize * 4]
                }
            };
            let (ra, rb) = (row_at(y_lo), row_at(y_lo + 1));
            let out = &mut band[r * stride..(r + 1) * stride];
            for (c, column) in columns.iter().enumerate() {
                let Some((x_lo, wx)) = *column else {
                    continue;
                };
                let tap = |row: &[u8], x: i32| if row.is_empty() { 0 } else { texel(row, x, sw) };
                let top = lerp(tap(ra, x_lo), tap(ra, x_lo + 1), wx);
                let bottom = lerp(tap(rb, x_lo), tap(rb, x_lo + 1), wx);
                let v = lerp(top, bottom, wy);
                let alpha = v >> 24;
                if alpha == 0 {
                    continue;
                }
                let o = (x0 as usize + c) * 4;
                if alpha == 255 && op == 255 {
                    out[o..o + 4].copy_from_slice(&v.to_le_bytes());
                } else {
                    blend_over(&mut out[o..o + 4], v.to_le_bytes(), op);
                }
            }
        }
    };

    let total_rows = (y1 - y0) as usize;
    let px = total_rows * (x1 - x0) as usize;
    // At most the repaint's thread budget (`bands::max_threads`: `AZ_CPU_THREADS`, 4 by default,
    // the machine's count asked once, not on every composite), and one inside a band of a
    // repaint: the band's thread is the composite's.
    let threads = super::bands::composite_threads(px / PX_PER_THREAD);
    let area = &mut dst.data[y0 as usize * stride..y1 as usize * stride];
    if threads <= 1 {
        band_rows(y0, area, total_rows);
        return;
    }
    let per = total_rows.div_ceil(threads);
    std::thread::scope(|scope| {
        for (i, band) in area.chunks_mut(per * stride).enumerate() {
            let first = y0 + (i * per) as i32;
            let rows_in_band = band.len() / stride;
            let band_rows = &band_rows;
            scope.spawn(move || band_rows(first, band, rows_in_band));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpurender::pixmap::blit_pixmap_affine_general;

    /// A pixmap of noise with a transparent margin and a half-transparent patch, so the edges, the
    /// skirt and the blend all show.
    fn noise(w: u32, h: u32, seed: u32) -> AzulPixmap {
        let mut pm = AzulPixmap::new(w, h).expect("a pixmap");
        let mut s = seed;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s
        };
        for y in 0..h {
            for x in 0..w {
                let r = next();
                let i = ((y * w + x) * 4) as usize;
                let inside = x >= 6 && y >= 4 && x < w - 6 && y < h - 4;
                let a: u8 = if !inside { 0 } else if (x / 9 + y / 7) % 5 == 0 { 120 } else { 255 };
                // premultiplied: the colour cannot exceed the alpha
                let c = |v: u32| (((v & 0xff) as u16 * u16::from(a)) / 255) as u8;
                pm.data[i] = c(r);
                pm.data[i + 1] = c(r >> 8);
                pm.data[i + 2] = c(r >> 16);
                pm.data[i + 3] = a;
            }
        }
        pm
    }

    fn matrix(sx: f64, sy: f64, tx: f64, ty: f64) -> TransAffine {
        TransAffine { sx, shy: 0.0, shx: 0.0, sy, tx, ty }
    }

    /// The destination box the composite covers: the transformed source rect, clamped to `dst`.
    fn bbox(src: &AzulPixmap, dst: &AzulPixmap, m: &TransAffine) -> (i32, i32, i32, i32) {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (cx, cy) in [(0.0, 0.0), (f64::from(src.width), 0.0), (0.0, f64::from(src.height)), (f64::from(src.width), f64::from(src.height))] {
            let (mut x, mut y) = (cx, cy);
            m.transform(&mut x, &mut y);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        (
            (min_x.floor() as i32).max(0),
            (min_y.floor() as i32).max(0),
            (max_x.ceil() as i32).min(dst.width as i32),
            (max_y.ceil() as i32).min(dst.height as i32),
        )
    }

    /// The largest per-channel difference between the separable blit and the general composite.
    fn max_difference(sx: f64, sy: f64, tx: f64, ty: f64, op: u32) -> (u8, usize) {
        let src = noise(120, 90, 0x1234_5678);
        let mut slow = noise(240, 200, 0x00c0_ffee);
        let mut fast = noise(240, 200, 0x00c0_ffee);
        let m = matrix(sx, sy, tx, ty);
        let b = bbox(&src, &slow, &m);
        blit_pixmap_affine_general(&src, &mut slow, &m, op, b);
        blit_scale_translate(&src, &mut fast, (sx, sy, tx, ty), op, b);
        let mut worst = 0u8;
        let mut off = 0usize;
        for (a, b) in slow.data.iter().zip(fast.data.iter()) {
            let d = a.abs_diff(*b);
            worst = worst.max(d);
            off += usize::from(d > 3);
        }
        (worst, off)
    }

    #[test]
    fn the_separable_blit_agrees_with_the_general_composite_within_three_levels() {
        for (sx, sy, tx, ty) in [
            (1.414, 1.414, 10.3, 7.7),
            (0.62, 0.62, 20.5, 12.25),
            (2.0, 2.0, -15.0, -6.0),
            (1.0, 1.0, 13.0, 9.0),
            (1.0, 1.0, 13.4, 9.6),
            (1.9, 0.8, 5.5, 3.2),
        ] {
            for op in [255, 128] {
                let (worst, off) = max_difference(sx, sy, tx, ty, op);
                assert!(
                    off == 0,
                    "scale ({sx}, {sy}) at ({tx}, {ty}), opacity {op}: {off} channels differ by more than 3 (worst {worst})"
                );
            }
        }
    }

    #[test]
    fn a_composite_over_a_band_of_threads_is_the_composite_of_one() {
        // 980 x 625 destination pixels: more than a thread's share, so the composite is split.
        let src = noise(700, 500, 99);
        let m = matrix(1.4, 1.25, 3.0, 2.0);
        let mut one = noise(1000, 700, 7);
        let mut many = noise(1000, 700, 7);
        let b = bbox(&src, &one, &m);
        // one thread: a box too small to split
        let rows = (b.3 - b.1) as usize;
        assert!(rows * (b.2 - b.0) as usize > 2 * PX_PER_THREAD, "harness: the box is split");
        blit_scale_translate(&src, &mut many, (1.4, 1.25, 3.0, 2.0), 255, b);
        let (top, bottom) = ((b.0, b.1, b.2, b.1 + (b.3 - b.1) / 2), (b.0, b.1 + (b.3 - b.1) / 2, b.2, b.3));
        blit_scale_translate(&src, &mut one, (1.4, 1.25, 3.0, 2.0), 255, top);
        blit_scale_translate(&src, &mut one, (1.4, 1.25, 3.0, 2.0), 255, bottom);
        assert!(one.data == many.data, "the bands of one composite are the composite");
    }

    #[test]
    fn a_degenerate_scale_or_a_shear_is_no_scale_translate() {
        assert!(scale_translate_of(&matrix(0.0, 1.0, 0.0, 0.0)).is_none());
        assert!(scale_translate_of(&TransAffine { sx: 1.0, shy: 0.5, shx: 0.0, sy: 1.0, tx: 0.0, ty: 0.0 }).is_none());
        assert!(scale_translate_of(&matrix(1.5, 1.5, 3.0, -2.0)).is_some());
    }
}
