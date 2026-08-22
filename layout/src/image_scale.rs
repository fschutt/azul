//! Pure-functional image resampling: the reference scaler.
//!
//! ONE piece of sampling math, shared by the places that must agree on how an
//! image is resized:
//!
//! - the CPU rasterizer's on-screen image blit (display),
//! - the capture pipeline producing a consumer's requested size — a local
//!   100×200 preview AND a remote 500×200 stream cut from ONE captured frame,
//!   so the camera is read once and sampled per consumer,
//! - anywhere a `RawImage` must be resized without pulling in the `image`
//!   crate.
//!
//! THE CONTRACT: every output pixel is a PURE function of the source and its
//! own destination coordinates ([`sample`]). A caller may therefore compute
//! any subset of the output on any thread — the whole-image [`resample_rgba`]
//! is just the serial walk, and a future threaded or platform-accelerated
//! backend (Accelerate / vImage on macOS, MPS on the GPU) plugs in behind the
//! same signature. This module is the portable fallback and the golden
//! reference the tests pin; no threading lives here on purpose.
//!
//! QUALITY: nearest-neighbour aliases badly on a downscale (the capture
//! preview's shimmer). [`sample`] area-averages a bounded grid of taps across
//! each destination pixel's source footprint on a downscale, and bilinearly
//! interpolates on an upscale — so a huge source feeding a tiny preview only
//! reads the pixels its taps land on, never the whole image.

use alloc::vec::Vec;

use azul_core::resources::RawImageFormat;

/// The most taps taken along ONE axis of a destination pixel's footprint.
/// Caps area-averaging cost at `MAX_TAPS²` reads per output pixel regardless
/// of how extreme the downscale is (a 40× downscale still costs 16 taps, not
/// 1600) — a bounded box filter, not a true area integral, which is the right
/// trade for a live preview.
const MAX_TAPS: u32 = 4;

/// A straight-RGBA view over tightly-packed image bytes, addressed per pixel
/// format. Borrows the source; holds no allocation of its own.
#[derive(Debug, Clone, Copy)]
pub struct SrcImage<'a> {
    /// Tightly-packed pixel bytes (`width * height * bytes_per_pixel(format)`).
    pub bytes: &'a [u8],
    /// The byte layout of `bytes`.
    pub format: RawImageFormat,
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
}

/// Bytes per pixel for the formats [`SrcImage::pixel`] can read. `None` for a
/// format this scaler does not sample (16-bit / float / two-channel) — the
/// caller renders those some other way.
#[must_use]
pub const fn bytes_per_pixel(format: RawImageFormat) -> Option<usize> {
    match format {
        RawImageFormat::R8 => Some(1),
        RawImageFormat::RGB8 | RawImageFormat::BGR8 => Some(3),
        RawImageFormat::RGBA8 | RawImageFormat::BGRA8 => Some(4),
        _ => None,
    }
}

impl SrcImage<'_> {
    /// Whether this scaler can sample the view's format and its `bytes` are
    /// long enough for `width × height`.
    #[must_use]
    pub fn is_sampleable(&self) -> bool {
        bytes_per_pixel(self.format).is_some_and(|bpp| {
            (self.width as usize)
                .checked_mul(self.height as usize)
                .and_then(|px| px.checked_mul(bpp))
                .is_some_and(|need| self.bytes.len() >= need)
        })
    }

    /// One source pixel as straight RGBA, clamped to the image edge (so a tap
    /// off the border repeats the border, never reads out of bounds). Returns
    /// opaque black for an unsupported format or a truncated buffer — callers
    /// gate on [`Self::is_sampleable`] first.
    #[must_use]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)] // clamped to [0, dim)
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 255];
        }
        let Some(bpp) = bytes_per_pixel(self.format) else {
            return [0, 0, 0, 255];
        };
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        let i = (y * self.width as usize + x) * bpp;
        if i + bpp > self.bytes.len() {
            return [0, 0, 0, 255];
        }
        let b = self.bytes;
        match self.format {
            RawImageFormat::RGBA8 => [b[i], b[i + 1], b[i + 2], b[i + 3]],
            RawImageFormat::BGRA8 => [b[i + 2], b[i + 1], b[i], b[i + 3]],
            RawImageFormat::RGB8 => [b[i], b[i + 1], b[i + 2], 255],
            RawImageFormat::BGR8 => [b[i + 2], b[i + 1], b[i], 255],
            // R8: replicated to RGB with an OPAQUE alpha (a coverage/luma
            // plane is not a transparency plane).
            RawImageFormat::R8 => [b[i], b[i], b[i], 255],
            _ => [0, 0, 0, 255],
        }
    }
}

/// The straight-RGBA value of destination pixel `(dx, dy)` when `src` is
/// resampled to `dst_w × dst_h`.
///
/// PURE — depends only on its arguments, so any subset of the destination can
/// be evaluated on any thread. Area-averages a bounded tap grid on a
/// downscale (kills aliasing), bilinear on an upscale (no blocky enlargement),
/// nearest at 1:1.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn sample(src: &SrcImage<'_>, dst_w: u32, dst_h: u32, dx: u32, dy: u32) -> [u8; 4] {
    let scale_x = src.width as f32 / dst_w.max(1) as f32;
    let scale_y = src.height as f32 / dst_h.max(1) as f32;
    // Source-space centre of this destination pixel.
    let cx = (dx as f32 + 0.5) * scale_x;
    let cy = (dy as f32 + 0.5) * scale_y;

    if scale_x <= 1.0 && scale_y <= 1.0 {
        return bilinear(src, cx - 0.5, cy - 0.5);
    }

    // Downscale on at least one axis: average a grid of taps spread across the
    // footprint [cx ± scale_x/2] × [cy ± scale_y/2]. An axis that is actually
    // an UPSCALE (scale < 1) takes a single centred tap.
    let nx = (scale_x.ceil() as u32).clamp(1, MAX_TAPS);
    let ny = (scale_y.ceil() as u32).clamp(1, MAX_TAPS);
    let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
    for ty in 0..ny {
        // Tap centres at the (k + 0.5)/n fractions of the footprint.
        let fy = cy + ((ty as f32 + 0.5) / ny as f32 - 0.5) * scale_y;
        for tx in 0..nx {
            let fx = cx + ((tx as f32 + 0.5) / nx as f32 - 0.5) * scale_x;
            let p = src.pixel(fx.floor() as i32, fy.floor() as i32);
            r += u32::from(p[0]);
            g += u32::from(p[1]);
            b += u32::from(p[2]);
            a += u32::from(p[3]);
        }
    }
    let n = nx * ny;
    [
        ((r + n / 2) / n) as u8,
        ((g + n / 2) / n) as u8,
        ((b + n / 2) / n) as u8,
        ((a + n / 2) / n) as u8,
    ]
}

/// Bilinear sample at source coordinate `(fx, fy)` in pixel units (a pixel's
/// centre is at its integer index). Used on an upscale.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
fn bilinear(src: &SrcImage<'_>, fx: f32, fy: f32) -> [u8; 4] {
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = fx - x0;
    let ty = fy - y0;
    let (x0, y0) = (x0 as i32, y0 as i32);
    let p00 = src.pixel(x0, y0);
    let p10 = src.pixel(x0 + 1, y0);
    let p01 = src.pixel(x0, y0 + 1);
    let p11 = src.pixel(x0 + 1, y0 + 1);
    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = f32::from(p00[c]) * (1.0 - tx) + f32::from(p10[c]) * tx;
        let bot = f32::from(p01[c]) * (1.0 - tx) + f32::from(p11[c]) * tx;
        out[c] = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Resample `src` to a tightly-packed `dst_w × dst_h` RGBA8 buffer.
///
/// The serial whole-image walk over [`sample`] — the frame scaler the capture
/// pipeline uses to cut each consumer's size from one captured frame, and the
/// convenience path for a one-off `RawImage` resize. Returns an empty `Vec`
/// for a zero destination or an unsampleable source.
#[must_use]
pub fn resample_rgba(src: &SrcImage<'_>, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if dst_w == 0 || dst_h == 0 || !src.is_sampleable() {
        return Vec::new();
    }
    let mut out = alloc::vec![0u8; dst_w as usize * dst_h as usize * 4];
    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let px = sample(src, dst_w, dst_h, dx, dy);
            let i = (dy as usize * dst_w as usize + dx as usize) * 4;
            out[i..i + 4].copy_from_slice(&px);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(bytes: &[u8], w: u32, h: u32) -> SrcImage<'_> {
        SrcImage { bytes, format: RawImageFormat::RGBA8, width: w, height: h }
    }

    #[test]
    fn a_pixel_reads_each_format_as_straight_rgba() {
        // one pixel, four formats
        assert_eq!(rgba(&[10, 20, 30, 40], 1, 1).pixel(0, 0), [10, 20, 30, 40]);
        assert_eq!(
            SrcImage { bytes: &[10, 20, 30, 40], format: RawImageFormat::BGRA8, width: 1, height: 1 }.pixel(0, 0),
            [30, 20, 10, 40]
        );
        assert_eq!(
            SrcImage { bytes: &[10, 20, 30], format: RawImageFormat::RGB8, width: 1, height: 1 }.pixel(0, 0),
            [10, 20, 30, 255]
        );
        assert_eq!(
            SrcImage { bytes: &[10, 20, 30], format: RawImageFormat::BGR8, width: 1, height: 1 }.pixel(0, 0),
            [30, 20, 10, 255]
        );
        assert_eq!(
            SrcImage { bytes: &[77], format: RawImageFormat::R8, width: 1, height: 1 }.pixel(0, 0),
            [77, 77, 77, 255]
        );
    }

    #[test]
    fn a_tap_off_the_edge_repeats_the_border_and_never_reads_out_of_bounds() {
        let img = rgba(&[1, 2, 3, 4], 1, 1);
        for (x, y) in [(-5, -5), (99, 0), (0, 99), (i32::MIN, i32::MAX)] {
            assert_eq!(img.pixel(x, y), [1, 2, 3, 4]);
        }
        // Unsupported format / short buffer → opaque black, no panic.
        assert_eq!(
            SrcImage { bytes: &[], format: RawImageFormat::RGBAF32, width: 1, height: 1 }.pixel(0, 0),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn a_2x2_downscaled_to_1x1_is_the_area_average() {
        // corners 0, 40, 80, 120 in every channel
        let bytes = [
            0, 0, 0, 0,        40, 40, 40, 40,
            80, 80, 80, 80,    120, 120, 120, 120,
        ];
        let out = sample(&rgba(&bytes, 2, 2), 1, 1, 0, 0);
        // (0+40+80+120)/4 = 60, exact
        assert_eq!(out, [60, 60, 60, 60], "a 2x downscale must average, not pick one (nearest)");
    }

    #[test]
    fn a_solid_source_survives_any_scale_unchanged() {
        // A 3x3 of a single colour resamples to that colour at any size, both
        // directions — no averaging drift, no edge darkening.
        let px = [90u8, 110, 130, 200];
        let mut bytes = Vec::new();
        for _ in 0..9 {
            bytes.extend_from_slice(&px);
        }
        let src = rgba(&bytes, 3, 3);
        for (w, h) in [(1, 1), (2, 2), (7, 5), (30, 30)] {
            let out = resample_rgba(&src, w, h);
            assert_eq!(out.len(), w as usize * h as usize * 4);
            for chunk in out.chunks_exact(4) {
                assert_eq!(chunk, px, "solid colour changed at {w}x{h}");
            }
        }
    }

    #[test]
    fn an_upscale_is_bilinear_not_blocky() {
        // A 2x1 gradient 0 -> 100 upscaled to 5x1: the middle samples must lie
        // strictly between the ends (nearest would jump 0 -> 100 with no
        // in-between value).
        let bytes = [0, 0, 0, 255, 100, 100, 100, 255];
        let src = rgba(&bytes, 2, 1);
        let out = resample_rgba(&src, 5, 1);
        let reds: Vec<u8> = out.chunks_exact(4).map(|c| c[0]).collect();
        assert!(reds[0] < reds[2] && reds[2] < reds[4], "not monotone: {reds:?}");
        assert!((1..100).contains(&reds[2]), "the middle must interpolate: {reds:?}");
    }

    #[test]
    fn an_extreme_downscale_is_bounded_and_does_not_panic() {
        // 400x400 -> 1x1: a true area average would read 160 000 pixels; the
        // bounded grid reads at most MAX_TAPS^2 = 16.
        let bytes = alloc::vec![128u8; 400 * 400 * 4];
        let out = sample(&rgba(&bytes, 400, 400), 1, 1, 0, 0);
        assert_eq!(out, [128, 128, 128, 128]);
    }

    #[test]
    fn resample_rejects_a_zero_destination_or_unsampleable_source() {
        let bytes = [1u8, 2, 3, 4];
        assert!(resample_rgba(&rgba(&bytes, 1, 1), 0, 4).is_empty());
        // buffer too short for the claimed dimensions
        assert!(!rgba(&[1, 2, 3], 2, 2).is_sampleable());
        assert!(resample_rgba(&rgba(&[1, 2, 3], 2, 2), 4, 4).is_empty());
    }
}
