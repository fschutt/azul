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
//!
//! FRAMES keep their format ([`resample_frame_rect`]): a BGRA capture is
//! scaled as BGRA and an NV12 one as NV12 (its Y plane and its Cb,Cr plane
//! each on their own), so no frame pays a swizzle or a YCbCr conversion on
//! its way to a tile or an encoder. A cut to another aspect ratio crops the
//! centre ([`cover_crop`]) instead of squashing, and a fan-out cuts every
//! consumer from the smallest frame already made that covers it
//! ([`fan_out`]).

use alloc::vec::Vec;

use crate::{
    resources::{Nv12Layout, RawImage, RawImageData, RawImageFormat, YuvCoefficients},
    video::{ConsumerFrame, FrameConsumer, VideoFrame},
};
use azul_css::U8Vec;

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
    /// Tightly-packed pixel bytes (`width * height * bytes_per_pixel(format)`,
    /// or both planes of an NV12 image, see `Nv12Layout`).
    pub bytes: &'a [u8],
    /// The byte layout of `bytes`.
    pub format: RawImageFormat,
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
}

/// Bytes per pixel for the PACKED formats [`SrcImage::pixel`] can read.
/// `None` for a format this scaler does not sample (16-bit / float /
/// two-channel) and for the planar NV12 formats, which are sampled plane by
/// plane (see [`SrcImage::is_sampleable`]).
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
        if self.format.is_nv12() {
            return Nv12Layout::new(self.width as usize, self.height as usize)
                .checked_total_len()
                .is_some_and(|need| self.bytes.len() >= need);
        }
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
    /// gate on [`Self::is_sampleable`] first. An NV12 pixel is its own luma
    /// with the chroma of its 2x2 block, converted by the one YCbCr table
    /// (`YuvCoefficients`).
    #[must_use]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)] // clamped to [0, dim)
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 255];
        }
        if let Some(coeffs) = YuvCoefficients::of(self.format) {
            return self.nv12_pixel(&coeffs, x, y);
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

    /// [`Self::pixel`] of an NV12 source (the caller checked the format).
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)] // clamped to [0, dim)
    fn nv12_pixel(&self, coeffs: &YuvCoefficients, x: i32, y: i32) -> [u8; 4] {
        let layout = Nv12Layout::new(self.width as usize, self.height as usize);
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        let c = layout.y_len() + (y / 2) * layout.chroma_width * 2 + (x / 2) * 2;
        match (
            self.bytes.get(y * layout.width + x),
            self.bytes.get(c),
            self.bytes.get(c + 1),
        ) {
            (Some(&luma), Some(&cb), Some(&cr)) => {
                let [r, g, b] = coeffs.to_rgb(luma, cb, cr);
                [r, g, b, 255]
            }
            _ => [0, 0, 0, 255],
        }
    }
}

/// A rectangle of source pixels: the part of the source a cut reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SrcRect {
    /// Left edge in source pixels.
    pub x: u32,
    /// Top edge in source pixels.
    pub y: u32,
    /// Width in source pixels.
    pub width: u32,
    /// Height in source pixels.
    pub height: u32,
}

impl SrcRect {
    /// The whole of a `width x height` source.
    #[must_use]
    pub const fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// This rect clipped to a `width x height` source; `None` when nothing of
    /// it is left.
    #[must_use]
    pub fn clamped_to(self, width: u32, height: u32) -> Option<Self> {
        if self.x >= width || self.y >= height {
            return None;
        }
        let w = self.width.min(width - self.x);
        let h = self.height.min(height - self.y);
        (w > 0 && h > 0).then_some(Self {
            x: self.x,
            y: self.y,
            width: w,
            height: h,
        })
    }
}

/// The centred part of a `sw x sh` source with the aspect ratio of
/// `dw x dh` (CSS `object-fit: cover`): the whole height of a source that is
/// wider than asked for, the whole width of one that is taller. `even` keeps
/// the origin, and every side the crop CUTS, on even pixels, so an NV12 crop
/// covers whole chroma pairs; a side that spans the whole source keeps the
/// source's own (possibly odd) extent, whose last chroma pair the source's
/// layout holds - so a same-size cut is the whole frame, a copy. A zero
/// size anywhere gives the whole source.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // results are <= sw / sh, which are u32
pub fn cover_crop(sw: u32, sh: u32, dw: u32, dh: u32, even: bool) -> SrcRect {
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return SrcRect::full(sw, sh);
    }
    let (sw64, sh64, dw64, dh64) = (
        u64::from(sw),
        u64::from(sh),
        u64::from(dw),
        u64::from(dh),
    );
    // sw / sh > dw / dh without floats.
    let (mut cw, mut ch) = if sw64 * dh64 > dw64 * sh64 {
        (((sh64 * dw64 + dh64 / 2) / dh64) as u32, sh)
    } else {
        (sw, ((sw64 * dh64 + dw64 / 2) / dw64) as u32)
    };
    cw = cw.clamp(1, sw);
    ch = ch.clamp(1, sh);
    let mut x = (sw - cw) / 2;
    let mut y = (sh - ch) / 2;
    if even {
        x &= !1;
        y &= !1;
        if cw > 1 && cw < sw {
            cw &= !1;
        }
        if ch > 1 && ch < sh {
            ch &= !1;
        }
    }
    SrcRect {
        x,
        y,
        width: cw,
        height: ch,
    }
}

/// The value of destination pixel `(dx, dy)` of a `dst_w x dst_h` resample
/// of a `src_w x src_h` grid of `C`-channel samples, read through `read`
/// (which clamps to the grid's edge).
///
/// THE sampling math, for every channel count: [`sample`] (straight RGBA of
/// any format), the frame planes of [`resample_frame_rect`] (4 channels of a
/// BGRA or RGBA frame as they are, 1 for NV12 luma, 2 for NV12 chroma).
/// Area-averages a bounded tap grid on a downscale, bilinear on an upscale,
/// nearest at 1:1.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn sample_with<const C: usize, F: Fn(i32, i32) -> [u8; C]>(
    read: &F,
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    dx: u32,
    dy: u32,
) -> [u8; C] {
    let scale_x = src_w as f32 / dst_w.max(1) as f32;
    let scale_y = src_h as f32 / dst_h.max(1) as f32;
    // Source-space centre of this destination pixel.
    let cx = (dx as f32 + 0.5) * scale_x;
    let cy = (dy as f32 + 0.5) * scale_y;

    if scale_x <= 1.0 && scale_y <= 1.0 {
        return bilinear_with(read, cx - 0.5, cy - 0.5);
    }

    // Downscale on at least one axis: average a grid of taps spread across the
    // footprint [cx ± scale_x/2] × [cy ± scale_y/2]. An axis that is actually
    // an UPSCALE (scale < 1) takes a single centred tap.
    let nx = (scale_x.ceil() as u32).clamp(1, MAX_TAPS);
    let ny = (scale_y.ceil() as u32).clamp(1, MAX_TAPS);
    let mut acc = [0u32; C];
    for ty in 0..ny {
        // Tap centres at the (k + 0.5)/n fractions of the footprint.
        let fy = cy + ((ty as f32 + 0.5) / ny as f32 - 0.5) * scale_y;
        for tx in 0..nx {
            let fx = cx + ((tx as f32 + 0.5) / nx as f32 - 0.5) * scale_x;
            let p = read(fx.floor() as i32, fy.floor() as i32);
            for (a, v) in acc.iter_mut().zip(p.iter()) {
                *a += u32::from(*v);
            }
        }
    }
    let n = nx * ny;
    let mut out = [0u8; C];
    for (o, a) in out.iter_mut().zip(acc.iter()) {
        *o = ((*a + n / 2) / n) as u8;
    }
    out
}

/// The straight-RGBA value of destination pixel `(dx, dy)` when `src` is
/// resampled to `dst_w × dst_h`.
///
/// PURE — depends only on its arguments, so any subset of the destination can
/// be evaluated on any thread. Area-averages a bounded tap grid on a
/// downscale (kills aliasing), bilinear on an upscale (no blocky enlargement),
/// nearest at 1:1.
#[must_use]
pub fn sample(src: &SrcImage<'_>, dst_w: u32, dst_h: u32, dx: u32, dy: u32) -> [u8; 4] {
    sample_with(
        &|x: i32, y: i32| src.pixel(x, y),
        src.width,
        src.height,
        dst_w,
        dst_h,
        dx,
        dy,
    )
}

/// Bilinear sample at source coordinate `(fx, fy)` in pixel units (a pixel's
/// centre is at its integer index). Used on an upscale.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
#[allow(clippy::many_single_char_names)] // p00/p10/.. corners + tx/ty weights
fn bilinear_with<const C: usize, F: Fn(i32, i32) -> [u8; C]>(
    read: &F,
    fx: f32,
    fy: f32,
) -> [u8; C] {
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = fx - x0;
    let ty = fy - y0;
    let (x0, y0) = (x0 as i32, y0 as i32);
    let p00 = read(x0, y0);
    let p10 = read(x0 + 1, y0);
    let p01 = read(x0, y0 + 1);
    let p11 = read(x0 + 1, y0 + 1);
    let mut out = [0u8; C];
    for c in 0..C {
        let top = f32::from(p00[c]) * (1.0 - tx) + f32::from(p10[c]) * tx;
        let bot = f32::from(p01[c]) * (1.0 - tx) + f32::from(p11[c]) * tx;
        out[c] = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Resample `src` to a tightly-packed `dst_w × dst_h` RGBA8 buffer.
///
/// The serial whole-image walk over [`sample`] — the convenience path for a
/// one-off `RawImage` resize to straight RGBA. Returns an empty `Vec` for a
/// zero destination or an unsampleable source. Frames (which keep their
/// format) go through [`resample_frame_rect`].
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

/// The byte format [`resample_frame_rect`] produces for a source format: the
/// frame formats keep their own (a BGRA capture stays BGRA, an NV12 one stays
/// NV12, RGBA stays RGBA), anything else becomes RGBA8.
#[must_use]
pub const fn frame_output_format(format: RawImageFormat) -> RawImageFormat {
    if format.is_nv12() {
        return format;
    }
    match format {
        RawImageFormat::BGRA8 => RawImageFormat::BGRA8,
        _ => RawImageFormat::RGBA8,
    }
}

/// One tightly packed plane of `C`-byte samples, read inside `rect` with
/// `rect`-relative coordinates clamped to its edge.
#[derive(Clone, Copy)]
struct Plane<'a> {
    bytes: &'a [u8],
    /// Samples per row of the whole plane.
    stride: usize,
    rect: SrcRect,
}

impl Plane<'_> {
    #[inline]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)] // clamped to [0, dim)
    fn read<const C: usize>(&self, x: i32, y: i32) -> [u8; C] {
        let x = x.clamp(0, self.rect.width as i32 - 1) as usize + self.rect.x as usize;
        let y = y.clamp(0, self.rect.height as i32 - 1) as usize + self.rect.y as usize;
        let i = (y * self.stride + x) * C;
        let mut out = [0u8; C];
        if let Some(s) = self.bytes.get(i..i + C) {
            out.copy_from_slice(s);
        }
        out
    }

    /// Append the `dst_w x dst_h` resample of `rect` to `out`: row copies
    /// at 1:1 (a crop that already has the asked size), [`sample_with`]
    /// otherwise. `rect` must be non-empty.
    fn resample_into<const C: usize>(&self, dst_w: u32, dst_h: u32, out: &mut Vec<u8>) {
        let row_len = dst_w as usize * C;
        if self.rect.width == dst_w && self.rect.height == dst_h {
            for y in 0..dst_h as usize {
                let start = ((self.rect.y as usize + y) * self.stride + self.rect.x as usize) * C;
                match self.bytes.get(start..start + row_len) {
                    Some(row) => out.extend_from_slice(row),
                    None => out.resize(out.len() + row_len, 0),
                }
            }
            return;
        }
        let read = |x: i32, y: i32| self.read::<C>(x, y);
        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let px = sample_with(
                    &read,
                    self.rect.width,
                    self.rect.height,
                    dst_w,
                    dst_h,
                    dx,
                    dy,
                );
                out.extend_from_slice(&px);
            }
        }
    }
}

/// Resample the `crop` of a frame to `dst_w x dst_h`, keeping its format
/// ([`frame_output_format`]): BGRA8 / RGBA8 bytes are scaled as they are
/// (the sampler is channel-order agnostic), an NV12 frame's Y plane and its
/// Cb,Cr plane are each scaled on their own, anything else is sampled to
/// straight RGBA8. A crop that already has the asked size is row copies.
/// Empty for a zero size, an empty crop or an unsampleable source.
///
/// THE whole-frame scaler of the capture pipeline ([`ResampleFn`]); the
/// dll may register a platform one with the same contract.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // crop sizes are bounded by the source's u32 size
pub fn resample_frame_rect(src: &SrcImage<'_>, crop: SrcRect, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if dst_w == 0 || dst_h == 0 || !src.is_sampleable() {
        return Vec::new();
    }
    let Some(crop) = crop.clamped_to(src.width, src.height) else {
        return Vec::new();
    };
    let px = dst_w as usize * dst_h as usize;
    match src.format {
        RawImageFormat::RGBA8 | RawImageFormat::BGRA8 => {
            let mut out = Vec::with_capacity(px * 4);
            Plane {
                bytes: src.bytes,
                stride: src.width as usize,
                rect: crop,
            }
            .resample_into::<4>(dst_w, dst_h, &mut out);
            out
        }
        f if f.is_nv12() => resample_nv12(src, crop, dst_w, dst_h),
        _ => {
            let (cx, cy) = (crop.x as i32, crop.y as i32);
            let (cw, ch) = (crop.width as i32, crop.height as i32);
            let read =
                |x: i32, y: i32| src.pixel(x.clamp(0, cw - 1) + cx, y.clamp(0, ch - 1) + cy);
            let mut out = Vec::with_capacity(px * 4);
            for dy in 0..dst_h {
                for dx in 0..dst_w {
                    out.extend_from_slice(&sample_with(
                        &read,
                        crop.width,
                        crop.height,
                        dst_w,
                        dst_h,
                        dx,
                        dy,
                    ));
                }
            }
            out
        }
    }
}

/// [`resample_frame_rect`] of an NV12 source: the luma crop to `dst_w x
/// dst_h`, the chroma crop (half size, rounded out) to the destination's
/// chroma size.
#[allow(clippy::cast_possible_truncation)] // chroma sizes are half of u32 sizes
fn resample_nv12(src: &SrcImage<'_>, crop: SrcRect, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let sl = Nv12Layout::new(src.width as usize, src.height as usize);
    let dl = Nv12Layout::new(dst_w as usize, dst_h as usize);
    let Some(total) = dl.checked_total_len() else {
        return Vec::new();
    };
    let (Some(y_plane), Some(uv_plane)) = (
        src.bytes.get(..sl.y_len()),
        src.bytes.get(sl.y_len()..sl.y_len() + sl.uv_len()),
    ) else {
        return Vec::new();
    };
    // The chroma pairs under the luma crop: from the pair of its first
    // column / row to the pair of its last one.
    let cx0 = crop.x / 2;
    let cy0 = crop.y / 2;
    let cx1 = ((crop.x + crop.width - 1) / 2 + 1).min(sl.chroma_width as u32);
    let cy1 = ((crop.y + crop.height - 1) / 2 + 1).min(sl.chroma_height as u32);
    let chroma = SrcRect {
        x: cx0,
        y: cy0,
        width: cx1.saturating_sub(cx0).max(1),
        height: cy1.saturating_sub(cy0).max(1),
    };
    let mut out = Vec::with_capacity(total);
    Plane {
        bytes: y_plane,
        stride: sl.width,
        rect: crop,
    }
    .resample_into::<1>(dst_w, dst_h, &mut out);
    Plane {
        bytes: uv_plane,
        stride: sl.chroma_width,
        rect: chroma,
    }
    .resample_into::<2>(dl.chroma_width as u32, dl.chroma_height as u32, &mut out);
    out
}

/// [`resample_frame_rect`] of the whole frame (a stretch to `dst_w x
/// dst_h`, no crop).
#[must_use]
pub fn resample_frame(src: &SrcImage<'_>, dst_w: u32, dst_h: u32) -> Vec<u8> {
    resample_frame_rect(src, SrcRect::full(src.width, src.height), dst_w, dst_h)
}

/// A whole-frame scaler: `(source, crop, dst_w, dst_h) -> the crop at
/// dst_w x dst_h in frame_output_format(source.format)` (empty on failure).
/// [`resample_frame_rect`] is the portable one; the dll may register a
/// platform-accelerated one (Accelerate/vImage on macOS) with the same
/// signature — see `widgets::capture_common::register_frame_resampler`.
/// Every implementation must be a pure function of its inputs so the
/// fan-out can run per consumer on any thread.
pub type ResampleFn = fn(&SrcImage<'_>, SrcRect, u32, u32) -> Vec<u8>;

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

/// Cut `src` to `width x height` with `resample`, in
/// [`frame_output_format`]`(src.format)`. A source of another aspect ratio
/// gives its centre ([`cover_crop`]; even-aligned for NV12) instead of being
/// squashed, and a crop that already has the asked size is a row copy — the
/// common "the camera already captures at the largest consumer's size" case.
#[must_use]
pub fn cut(src: &SrcImage<'_>, width: u32, height: u32, resample: ResampleFn) -> Vec<u8> {
    if width == 0 || height == 0 || !src.is_sampleable() {
        return Vec::new();
    }
    let crop = cover_crop(src.width, src.height, width, height, src.format.is_nv12());
    resample(src, crop, width, height)
}

/// Cut ONE captured frame to every consumer's requested size.
///
/// A cascade, largest first: each consumer is cut from the smallest frame
/// already made that covers it (else from `src`), so "720p capture, 360p and
/// 180p renditions" is 720 -> 360 -> 180, each pass smaller than the last,
/// instead of every rendition re-reading the full capture. Invalid consumers
/// (zero size, the reserved preview id) and failed cuts are skipped, so the
/// result may be shorter than the input; it keeps the consumers' order, and
/// requests are matched by `ConsumerFrame::consumer.id`. Every frame is in
/// [`frame_output_format`]`(src.format)`.
#[must_use]
pub fn fan_out(
    src: &SrcImage<'_>,
    consumers: &[FrameConsumer],
    resample: ResampleFn,
) -> Vec<ConsumerFrame> {
    let valid: Vec<FrameConsumer> = consumers
        .iter()
        .copied()
        .filter(FrameConsumer::is_valid)
        .collect();
    let area = |c: &FrameConsumer| u64::from(c.width) * u64::from(c.height);
    // Largest first; the sort is stable, so equal sizes keep their order.
    let mut order: Vec<usize> = (0..valid.len()).collect();
    order.sort_by(|&a, &b| area(&valid[b]).cmp(&area(&valid[a])));
    let out_format = frame_output_format(src.format);
    let mut made: Vec<Option<ConsumerFrame>> = valid.iter().map(|_| None).collect();
    for i in order {
        let c = valid[i];
        let base = made
            .iter()
            .flatten()
            .filter(|m| m.frame.width >= c.width && m.frame.height >= c.height)
            .min_by_key(|m| u64::from(m.frame.width) * u64::from(m.frame.height));
        let bytes = match base {
            Some(m) => cut(
                &SrcImage {
                    bytes: m.frame.bytes.as_ref(),
                    format: m.frame.format,
                    width: m.frame.width,
                    height: m.frame.height,
                },
                c.width,
                c.height,
                resample,
            ),
            None => cut(src, c.width, c.height, resample),
        };
        if !bytes.is_empty() {
            made[i] = Some(ConsumerFrame::new(
                c,
                VideoFrame::with_format(c.width, c.height, bytes.into(), out_format),
            ));
        }
    }
    made.into_iter().flatten().collect()
}

/// `width x height` scaled DOWN (never up) to fit `max_w x max_h`, the
/// aspect ratio kept, at least one pixel per axis; `(0, 0)` for an empty
/// size or an empty box.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn fit_within(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width == 0 || height == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    if width <= max_w && height <= max_h {
        return (width, height);
    }
    let scale = (f64::from(max_w) / f64::from(width)).min(f64::from(max_h) / f64::from(height));
    let w = ((f64::from(width) * scale).round() as u32).clamp(1, max_w);
    let h = ((f64::from(height) * scale).round() as u32).clamp(1, max_h);
    (w, h)
}

/// A copy of `image` scaled down to fit `max_w x max_h` ([`fit_within`]) as
/// straight RGBA8 - a thumbnail, sampled by [`resample_rgba`]. `None` for a
/// source the scaler cannot read (16-bit, float or two-channel pixels) and
/// for an empty one.
#[must_use]
pub fn thumbnail(image: &RawImage, max_w: u32, max_h: u32) -> Option<RawImage> {
    let width = u32::try_from(image.width).ok()?;
    let height = u32::try_from(image.height).ok()?;
    let (w, h) = fit_within(width, height, max_w, max_h);
    resized(image, w, h)
}

/// A copy of `image` resampled to exactly `width x height` - up or down,
/// the aspect NOT kept ([`thumbnail`] keeps it) - as straight RGBA8,
/// sampled by [`resample_rgba`] (area-averaging down, bilinear up). `None`
/// for a source the scaler cannot read (16-bit, float or two-channel
/// pixels, a buffer shorter than its size), for an empty one and for a
/// zero size.
#[must_use]
pub fn resized(image: &RawImage, width: u32, height: u32) -> Option<RawImage> {
    let bytes: &[u8] = match &image.pixels {
        RawImageData::U8(bytes) => bytes.as_ref(),
        RawImageData::U16(_) | RawImageData::F32(_) => return None,
    };
    let src = SrcImage {
        bytes,
        format: image.data_format,
        width: u32::try_from(image.width).ok()?,
        height: u32::try_from(image.height).ok()?,
    };
    if src.width == 0 || src.height == 0 || width == 0 || height == 0 || !src.is_sampleable() {
        return None;
    }
    let pixels = resample_rgba(&src, width, height);
    if pixels.is_empty() {
        return None;
    }
    Some(RawImage {
        pixels: RawImageData::U8(U8Vec::from_vec(pixels)),
        width: width as usize,
        height: height as usize,
        premultiplied_alpha: image.premultiplied_alpha,
        data_format: RawImageFormat::RGBA8,
        tag: U8Vec::from_vec(Vec::new()),
    })
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

    // --- thumbnails --------------------------------------------------------

    /// An app that sizes its own buffer by the thumbnail rule (a video
    /// decoder's output, a monitor) asks `RawImage::fit_within` - the C API
    /// face of [`fit_within`] - instead of keeping a copy of it (AzVideoCut
    /// did, in f32, DEDUP_OFFICE D15).
    #[test]
    fn raw_image_fit_within_is_the_thumbnail_rule_as_a_size() {
        use crate::{geom::PhysicalSizeU32, resources::RawImage};

        assert_eq!(RawImage::fit_within(1920, 1080, 640, 360), PhysicalSizeU32::new(640, 360));
        assert_eq!(RawImage::fit_within(1000, 1000, 640, 360), PhysicalSizeU32::new(360, 360));
        assert_eq!(RawImage::fit_within(320, 240, 640, 360), PhysicalSizeU32::new(320, 240), "never up");
        assert_eq!(RawImage::fit_within(0, 10, 10, 10), PhysicalSizeU32::new(0, 0), "nothing to fit");
    }

    /// A file manager's Large icons show pictures as thumbnails: an image
    /// scaled down to a box, its aspect kept, never enlarged, as straight
    /// RGBA8 - a few kilobytes on the GPU instead of the whole photo.
    #[test]
    fn a_thumbnail_fits_the_box_keeps_the_aspect_and_never_enlarges() {
        use crate::resources::{RawImage, RawImageData};
        use azul_css::{F32Vec, U8Vec};

        assert_eq!(fit_within(400, 200, 100, 100), (100, 50));
        assert_eq!(fit_within(200, 400, 100, 100), (50, 100));
        assert_eq!(fit_within(40, 20, 100, 100), (40, 20), "never enlarged");
        assert_eq!(fit_within(1000, 1, 10, 10), (10, 1), "at least one pixel");
        assert_eq!(fit_within(0, 10, 10, 10), (0, 0), "nothing to fit");

        let green = RawImage {
            pixels: RawImageData::U8(U8Vec::from_vec([0u8, 200, 0, 255].repeat(8))),
            width: 4,
            height: 2,
            premultiplied_alpha: false,
            data_format: RawImageFormat::RGBA8,
            tag: U8Vec::from_vec(Vec::new()),
        };
        let thumb = thumbnail(&green, 2, 2).expect("an RGBA8 source scales");
        assert_eq!((thumb.width, thumb.height), (2, 1));
        assert_eq!(thumb.data_format, RawImageFormat::RGBA8);
        match &thumb.pixels {
            RawImageData::U8(bytes) => {
                assert_eq!(bytes.as_ref(), &[0, 200, 0, 255, 0, 200, 0, 255]);
            }
            other => panic!("not 8-bit pixels: {other:?}"),
        }
        let small = thumbnail(&green, 100, 100).expect("a small source");
        assert_eq!((small.width, small.height), (4, 2), "a small image keeps its size");

        let hdr = RawImage {
            pixels: RawImageData::F32(F32Vec::from_vec(Vec::new())),
            width: 2,
            height: 2,
            premultiplied_alpha: false,
            data_format: RawImageFormat::RGBAF32,
            tag: U8Vec::from_vec(Vec::new()),
        };
        assert!(thumbnail(&hdr, 2, 2).is_none(), "a float image is not sampled");
    }

    /// `RawImage::create_rgba8` is the RGBA8 image four apps spelled out
    /// field by field, and `RawImage::resized` scales one to an exact size,
    /// up or down - AzVideoCut's monitor and bin had their own nearest
    /// sampler for it (DEDUP_OFFICE D15 / A7).
    #[test]
    fn an_rgba8_image_resizes_to_exactly_the_size_asked_for() {
        use crate::resources::{RawImage, RawImageData};
        use azul_css::U8Vec;

        let green = RawImage::create_rgba8(4, 2, U8Vec::from_vec([0u8, 200, 0, 255].repeat(8)), false);
        assert_eq!((green.width, green.height), (4, 2));
        assert_eq!(green.data_format, RawImageFormat::RGBA8);
        assert!(!green.premultiplied_alpha);

        let down = green.resized(2, 2).expect("an RGBA8 source scales");
        assert_eq!((down.width, down.height), (2, 2), "the exact size, aspect not kept");
        let up = green.resized(8, 6).expect("and enlarges");
        assert_eq!((up.width, up.height), (8, 6));
        match &up.pixels {
            RawImageData::U8(bytes) => {
                assert_eq!(bytes.as_ref().len(), 8 * 6 * 4);
                assert_eq!(&bytes.as_ref()[..4], &[0, 200, 0, 255]);
            }
            other => panic!("not 8-bit pixels: {other:?}"),
        }
        assert!(green.resized(0, 4).is_none(), "no pixels to make");
        let empty = RawImage::create_rgba8(0, 0, U8Vec::from_vec(Vec::new()), false);
        assert!(empty.resized(4, 4).is_none(), "nothing to sample");
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
        let cuts = fan_out(&src, &consumers, resample_frame_rect);
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
        assert_eq!(cut(&src, 3, 2, resample_frame_rect).len(), 3 * 2 * 4);
        assert!(cut(&src, 0, 2, resample_frame_rect).is_empty());
        let truncated = SrcImage {
            bytes: &bytes[..2],
            ..src
        };
        assert!(
            cut(&truncated, 1, 1, resample_frame_rect).is_empty(),
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
        let layout = crate::resources::Nv12Layout::new(w as usize, h as usize);
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

    #[test]
    fn a_same_size_cut_of_an_odd_sized_nv12_frame_is_a_copy_not_a_resample() {
        // A 641x361 screen capture shown in a 641x361 tile (odd device sizes
        // are ordinary): the crop is the whole frame, and the frame's own
        // NV12 layout already holds a chroma pair for its last column and
        // its last row. The even alignment is for a crop INSIDE the frame,
        // whose origin and size must land on chroma pairs; applied to the
        // whole frame it cut it to 640x360, so every frame was resampled
        // (an upscale by one pixel) instead of copied, and a column and a
        // row of the picture were lost.
        assert_eq!(
            cover_crop(641, 361, 641, 361, true),
            SrcRect::full(641, 361)
        );
        // A crop that keeps the whole width (odd) but not the whole height
        // still evens what it cuts: the origin, and the height.
        assert_eq!(
            cover_crop(641, 480, 641, 361, true),
            SrcRect {
                x: 0,
                y: 58,
                width: 641,
                height: 360
            }
        );
        // The same-size cut of a non-uniform frame is its bytes.
        let layout = crate::resources::Nv12Layout::new(641, 361);
        let bytes: Vec<u8> = (0..layout.checked_total_len().expect("small"))
            .map(|i| ((i * 37 + 11) % 256) as u8)
            .collect();
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::NV12Rec709Video,
            width: 641,
            height: 361,
        };
        assert_eq!(cut(&src, 641, 361, resample_frame_rect), bytes);
    }
}
