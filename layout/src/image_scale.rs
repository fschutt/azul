//! Pure-functional image resampling: the reference scaler.
//!
//! ONE piece of sampling math, shared by the places that must agree on how an
//! image is resized:
//!
//! - the CPU rasterizer's on-screen image blit (display),
//! - the capture pipeline producing a consumer's requested size — a local 100×200 preview AND a
//!   remote 500×200 stream cut from ONE captured frame, so the camera is read once and sampled per
//!   consumer,
//! - anywhere a `RawImage` must be resized without pulling in the `image` crate.
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

use azul_core::{
    resources::RawImageFormat,
    video::{ConsumerFrame, FrameConsumer, VideoFrame},
};

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
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
#[allow(clippy::many_single_char_names)] // r/g/b/a channel accumulators + tap coords
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
#[allow(clippy::many_single_char_names)] // p00/p10/.. corners + tx/ty weights
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

/// A whole-frame scaler: `(source, dst_w, dst_h) -> tightly-packed RGBA8`
/// (`dst_w * dst_h * 4` bytes, or empty on failure). [`resample_rgba`] is
/// the portable one; the dll may register a platform-accelerated one
/// (Accelerate/vImage on macOS) with the same signature — see
/// `widgets::capture_common::register_frame_resampler`. Every implementation
/// must be a pure function of its inputs so the fan-out can run per consumer
/// on any thread.
pub type ResampleFn = fn(&SrcImage<'_>, u32, u32) -> Vec<u8>;

/// The smallest capture size that covers every requested size: the per-axis
/// maximum. Zero-sized entries are ignored; `None` when nothing is left.
///
/// "Client Bob wants 500x200, the local preview is 100x200" -> capture at
/// 500x200: every consumer is then a downscale of the captured frame (never
/// an upscale, which would only invent pixels) and the device is never asked
/// for more than the largest consumer can use.
#[must_use]
pub fn covering_size<I: IntoIterator<Item = (u32, u32)>>(sizes: I) -> Option<(u32, u32)> {
    sizes
        .into_iter()
        .filter(|&(w, h)| w > 0 && h > 0)
        .reduce(|(aw, ah), (w, h)| (aw.max(w), ah.max(h)))
}

/// Cut `src` to `width x height` RGBA8 with `resample`. A same-size RGBA8
/// source is copied, not resampled (the common "the camera already captures
/// at the largest consumer's size" case costs one memcpy).
#[must_use]
pub fn cut(src: &SrcImage<'_>, width: u32, height: u32, resample: ResampleFn) -> Vec<u8> {
    if width == 0 || height == 0 || !src.is_sampleable() {
        return Vec::new();
    }
    if src.width == width && src.height == height && src.format == RawImageFormat::RGBA8 {
        return src.bytes.to_vec();
    }
    resample(src, width, height)
}

/// Cut ONE captured frame to every consumer's requested size.
///
/// Each element is independent of the others (a pure function of `src` and
/// its own consumer), so this serial loop can become a parallel map without
/// touching the callers. Invalid consumers (zero size, the reserved preview
/// id) and failed cuts are skipped, so the result may be shorter than the
/// input; match results to requests by `ConsumerFrame::consumer.id`.
#[must_use]
pub fn fan_out(
    src: &SrcImage<'_>,
    consumers: &[FrameConsumer],
    resample: ResampleFn,
) -> Vec<ConsumerFrame> {
    consumers
        .iter()
        .copied()
        .filter(FrameConsumer::is_valid)
        .filter_map(|c| {
            let rgba = cut(src, c.width, c.height, resample);
            (!rgba.is_empty())
                .then(|| ConsumerFrame::new(c, VideoFrame::new(c.width, c.height, rgba.into())))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(bytes: &[u8], w: u32, h: u32) -> SrcImage<'_> {
        SrcImage {
            bytes,
            format: RawImageFormat::RGBA8,
            width: w,
            height: h,
        }
    }

    // --- consumers ---------------------------------------------------------

    #[test]
    fn the_covering_size_is_the_per_axis_max_ignoring_zero_entries() {
        // Bob 500x200 + a 100x200 preview -> 500x200 captured, once.
        assert_eq!(covering_size([(500, 200), (100, 200)]), Some((500, 200)));
        // A tall and a wide consumer cover each other's axis.
        assert_eq!(covering_size([(100, 900), (800, 50)]), Some((800, 900)));
        assert_eq!(
            covering_size([(0, 0), (0, 480)]),
            None,
            "zero sizes are not requests"
        );
        assert_eq!(covering_size([]), None);
    }

    #[test]
    fn fan_out_cuts_one_frame_to_every_valid_consumer() {
        // a 4x2 solid-green frame
        let bytes = [0u8, 200, 0, 255].repeat(8);
        let src = rgba(&bytes, 4, 2);
        let consumers = [
            FrameConsumer::new(7, 2, 1),  // Bob, a downscale
            FrameConsumer::new(8, 4, 2),  // the recorder at the captured size -> copy
            FrameConsumer::new(0, 4, 2),  // the preview id is NOT a fan-out consumer
            FrameConsumer::new(9, 0, 10), // a zero size is skipped
        ];
        let cuts = fan_out(&src, &consumers, resample_rgba);
        let ids: Vec<u32> = cuts.iter().map(|c| c.consumer.id).collect();
        assert_eq!(
            ids,
            vec![7, 8],
            "only the valid consumers get a frame: {ids:?}"
        );
        assert_eq!(cuts[0].frame.width, 2);
        assert_eq!(cuts[0].frame.height, 1);
        assert_eq!(
            cuts[0].frame.bytes.as_ref(),
            &[0, 200, 0, 255, 0, 200, 0, 255]
        );
        assert_eq!(
            cuts[1].frame.bytes.as_ref(),
            &bytes[..],
            "a same-size RGBA8 consumer is a copy of the captured frame"
        );
    }

    #[test]
    fn a_cut_never_upscales_past_what_was_asked_and_rejects_bad_input() {
        let bytes = [9u8; 4];
        let src = rgba(&bytes, 1, 1);
        assert_eq!(cut(&src, 3, 2, resample_rgba).len(), 3 * 2 * 4);
        assert!(cut(&src, 0, 2, resample_rgba).is_empty());
        let truncated = SrcImage {
            bytes: &bytes[..2],
            ..src
        };
        assert!(
            cut(&truncated, 1, 1, resample_rgba).is_empty(),
            "an unsampleable source cuts nothing"
        );
    }

    #[test]
    fn a_pixel_reads_each_format_as_straight_rgba() {
        // one pixel, four formats
        assert_eq!(rgba(&[10, 20, 30, 40], 1, 1).pixel(0, 0), [10, 20, 30, 40]);
        assert_eq!(
            SrcImage {
                bytes: &[10, 20, 30, 40],
                format: RawImageFormat::BGRA8,
                width: 1,
                height: 1
            }
            .pixel(0, 0),
            [30, 20, 10, 40]
        );
        assert_eq!(
            SrcImage {
                bytes: &[10, 20, 30],
                format: RawImageFormat::RGB8,
                width: 1,
                height: 1
            }
            .pixel(0, 0),
            [10, 20, 30, 255]
        );
        assert_eq!(
            SrcImage {
                bytes: &[10, 20, 30],
                format: RawImageFormat::BGR8,
                width: 1,
                height: 1
            }
            .pixel(0, 0),
            [30, 20, 10, 255]
        );
        assert_eq!(
            SrcImage {
                bytes: &[77],
                format: RawImageFormat::R8,
                width: 1,
                height: 1
            }
            .pixel(0, 0),
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
            SrcImage {
                bytes: &[],
                format: RawImageFormat::RGBAF32,
                width: 1,
                height: 1
            }
            .pixel(0, 0),
            [0, 0, 0, 255]
        );
    }

    #[test]
    fn a_2x2_downscaled_to_1x1_is_the_area_average() {
        // corners 0, 40, 80, 120 in every channel
        let bytes = [
            0, 0, 0, 0, 40, 40, 40, 40, 80, 80, 80, 80, 120, 120, 120, 120,
        ];
        let out = sample(&rgba(&bytes, 2, 2), 1, 1, 0, 0);
        // (0+40+80+120)/4 = 60, exact
        assert_eq!(
            out,
            [60, 60, 60, 60],
            "a 2x downscale must average, not pick one (nearest)"
        );
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
        assert!(
            reds[0] < reds[2] && reds[2] < reds[4],
            "not monotone: {reds:?}"
        );
        assert!(
            (1..100).contains(&reds[2]),
            "the middle must interpolate: {reds:?}"
        );
    }

    #[test]
    fn an_extreme_downscale_is_bounded_and_does_not_panic() {
        // 400x400 -> 1x1: a true area average would read 160 000 pixels; the
        // bounded grid reads at most MAX_TAPS^2 = 16.
        let bytes = alloc::vec![128u8; 400 * 400 * 4];
        let out = sample(&rgba(&bytes, 400, 400), 1, 1, 0, 0);
        assert_eq!(out, [128, 128, 128, 128]);
    }

    // --- frame formats: BGRA stays BGRA, NV12 stays NV12 --------------------

    /// A `w x h` NV12 frame of one colour (`y`, `cb`, `cr`).
    fn solid_nv12(w: u32, h: u32, y: u8, cb: u8, cr: u8) -> Vec<u8> {
        let layout = azul_core::resources::Nv12Layout::new(w as usize, h as usize);
        let mut bytes = alloc::vec![y; layout.y_len()];
        for _ in 0..layout.chroma_width * layout.chroma_height {
            bytes.push(cb);
            bytes.push(cr);
        }
        bytes
    }

    #[test]
    fn an_nv12_pixel_reads_as_its_rgb_with_the_chroma_of_its_block() {
        // 2x2, video range: black, white / grey, white over neutral chroma.
        let bytes = [16u8, 235, 126, 235, 128, 128];
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::NV12Rec601Video,
            width: 2,
            height: 2,
        };
        assert!(src.is_sampleable());
        assert_eq!(src.pixel(0, 0), [0, 0, 0, 255]);
        assert_eq!(src.pixel(1, 0), [255, 255, 255, 255]);
        assert_eq!(src.pixel(9, 9), [255, 255, 255, 255], "edge-clamped");
        let short = SrcImage {
            bytes: &bytes[..5],
            ..src
        };
        assert!(!short.is_sampleable(), "a truncated chroma plane");
    }

    #[test]
    fn a_bgra_frame_resamples_to_bgra_not_rgba() {
        // The capture pipeline carries BGRA end to end: a scaler that hands
        // back RGBA forces a swizzle on every frame, twice.
        let bytes = [10u8, 20, 200, 255].repeat(4 * 4); // B G R A
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::BGRA8,
            width: 4,
            height: 4,
        };
        assert_eq!(frame_output_format(RawImageFormat::BGRA8), RawImageFormat::BGRA8);
        let out = resample_frame(&src, 2, 2);
        assert_eq!(out, [10u8, 20, 200, 255].repeat(4));
    }

    #[test]
    fn an_nv12_frame_resamples_to_nv12_at_the_new_size() {
        let bytes = solid_nv12(8, 6, 100, 90, 200);
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::NV12Rec709Video,
            width: 8,
            height: 6,
        };
        assert_eq!(
            frame_output_format(RawImageFormat::NV12Rec709Video),
            RawImageFormat::NV12Rec709Video
        );
        // 5x3: odd on both axes, so the chroma plane is 3x2 pairs.
        let out = resample_frame(&src, 5, 3);
        assert_eq!(out, solid_nv12(5, 3, 100, 90, 200));
    }

    #[test]
    fn a_same_size_cut_of_a_frame_is_a_copy_in_its_own_format() {
        let bytes = solid_nv12(4, 2, 50, 60, 70);
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::NV12Rec601Full,
            width: 4,
            height: 2,
        };
        assert_eq!(cut(&src, 4, 2, resample_frame_rect), bytes);
        let bgra = [1u8, 2, 3, 255].repeat(6);
        let src = SrcImage {
            bytes: &bgra,
            format: RawImageFormat::BGRA8,
            width: 3,
            height: 2,
        };
        assert_eq!(cut(&src, 3, 2, resample_frame_rect), bgra);
        // Formats that are not frame formats come out as RGBA8, as before.
        assert_eq!(frame_output_format(RawImageFormat::RGB8), RawImageFormat::RGBA8);
    }

    #[test]
    fn a_cut_to_another_aspect_crops_the_centre_instead_of_squashing() {
        // A 16:9 camera frame shown in a 3:2 tile, or a 4:3 camera sent as a
        // 16:9 rendition: stretching squashes faces. The cut keeps the
        // aspect of what was asked for and takes the centre of the source
        // (CSS `object-fit: cover`).
        // 4x2: an outer column of blue on each side of two green columns.
        let blue = [0u8, 0, 255, 255];
        let green = [0u8, 255, 0, 255];
        let mut bytes = Vec::new();
        for _ in 0..2 {
            for px in [blue, green, green, blue] {
                bytes.extend_from_slice(&px);
            }
        }
        let src = rgba(&bytes, 4, 2);
        assert_eq!(
            cover_crop(4, 2, 2, 2, false),
            SrcRect {
                x: 1,
                y: 0,
                width: 2,
                height: 2
            }
        );
        let out = cut(&src, 2, 2, resample_frame_rect);
        assert_eq!(out, green.repeat(4), "the centre, not a squash of all four columns");
        // NV12 crops land on even pixels, so the chroma pairs stay aligned.
        let crop = cover_crop(1280, 720, 300, 200, true);
        assert_eq!((crop.x % 2, crop.y % 2, crop.width % 2, crop.height % 2), (0, 0, 0, 0));
        assert!(crop.width <= 1280 && crop.height == 720, "{crop:?}");
    }

    /// Every resample a test's fan-out ran: (source crop width, height,
    /// destination width, height).
    std::thread_local! {
        static CUTS: core::cell::RefCell<Vec<(u32, u32, u32, u32)>> =
            const { core::cell::RefCell::new(Vec::new()) };
    }

    fn counting_resample(src: &SrcImage<'_>, crop: SrcRect, w: u32, h: u32) -> Vec<u8> {
        CUTS.with(|c| c.borrow_mut().push((crop.width, crop.height, w, h)));
        resample_frame_rect(src, crop, w, h)
    }

    #[test]
    fn each_rendition_is_cut_from_the_smallest_frame_that_covers_it() {
        // One capture, three consumers: 640x360, 320x180 and 160x90. Each is
        // ONE downscale, and each from the smallest frame already made that
        // covers it - not three full-size passes over the capture.
        let bytes = [50u8, 60, 70, 255].repeat(1280 * 720);
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::BGRA8,
            width: 1280,
            height: 720,
        };
        CUTS.with(|c| c.borrow_mut().clear());
        let consumers = [
            FrameConsumer::new(3, 160, 90),
            FrameConsumer::new(1, 640, 360),
            FrameConsumer::new(2, 320, 180),
        ];
        let cuts = fan_out(&src, &consumers, counting_resample);
        let ids: Vec<u32> = cuts.iter().map(|c| c.consumer.id).collect();
        assert_eq!(ids, vec![3, 1, 2], "results keep the consumers' order");
        assert!(cuts.iter().all(|c| c.frame.format == RawImageFormat::BGRA8));
        let runs = CUTS.with(|c| c.borrow().clone());
        assert_eq!(
            runs,
            vec![
                (1280, 720, 640, 360),
                (640, 360, 320, 180),
                (320, 180, 160, 90),
            ],
            "a cascade: 720 -> 360 -> 180 -> 90"
        );
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
