//! Accelerate / vImage whole-frame scaler (macOS).
//!
//! `vImageScale_ARGB8888` resamples any interleaved 4 x 8-bit image — it is
//! channel-order agnostic, so RGBA8 and BGRA8 frames go through it as they
//! are and come out in their own order (a BGRA capture stays BGRA: no
//! swizzle on the way to a tile or an encoder). NV12 and the formats with
//! fewer channels take the portable scaler (NV12 is 1.5 bytes a pixel and is
//! scaled plane by plane there; a vImage `Planar8` + `CbCr8` path is a
//! follow-up).
//!
//! Contract (see `capture_common::register_frame_resampler`): same inputs
//! -> same picture as `image_scale::resample_frame_rect` within rounding;
//! pure; safe to call from any thread (vImage is re-entrant). The default
//! kernel (no flags) is used: `kvImageHighQualityResampling` (Lanczos5)
//! costs about twice as much and a live frame at (about) the size it is
//! shown at gains nothing from it. The temp buffer is `NULL` so vImage
//! allocates its own scratch.

use core::ffi::c_void;

use azul_core::resources::RawImageFormat;
use azul_layout::image_scale::{self, SrcImage, SrcRect};

/// `vImage_Buffer` (Accelerate/vImage/vImage_Types.h).
#[repr(C)]
#[allow(non_snake_case)]
struct VImageBuffer {
    data: *mut c_void,
    height: usize,
    width: usize,
    rowBytes: usize,
}

/// `kvImageNoFlags` (vImage_Types.h): the default resampling kernel.
const NO_FLAGS: u32 = 0;
/// `kvImageNoError`.
const NO_ERROR: isize = 0;

#[link(name = "Accelerate", kind = "framework")]
extern "C" {
    fn vImageScale_ARGB8888(
        src: *const VImageBuffer,
        dest: *const VImageBuffer,
        temp_buffer: *mut c_void,
        flags: u32,
    ) -> isize;
}

/// Resample the `crop` of `src` to `dst_w` x `dst_h` with vImage, in the
/// source frame's own format. Empty on a zero size, an unsampleable source
/// or an empty crop; a vImage error falls back to the portable scaler.
pub fn resample_frame_rect(src: &SrcImage<'_>, crop: SrcRect, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if dst_w == 0 || dst_h == 0 || !src.is_sampleable() {
        return Vec::new();
    }
    if !matches!(src.format, RawImageFormat::RGBA8 | RawImageFormat::BGRA8) {
        // NV12 (plane by plane) and the non-frame formats.
        return image_scale::resample_frame_rect(src, crop, dst_w, dst_h);
    }
    let Some(crop) = crop.clamped_to(src.width, src.height) else {
        return Vec::new();
    };
    if crop.width == dst_w && crop.height == dst_h {
        // A crop that already has the asked size is row copies.
        return image_scale::resample_frame_rect(src, crop, dst_w, dst_h);
    }
    let Some(out_len) = (dst_w as usize)
        .checked_mul(dst_h as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return Vec::new();
    };
    let row_bytes = src.width as usize * 4;
    let offset = crop.y as usize * row_bytes + crop.x as usize * 4;
    let Some(first) = src.bytes.get(offset..) else {
        return Vec::new();
    };
    let mut out = vec![0u8; out_len];
    let src_buf = VImageBuffer {
        data: first.as_ptr() as *mut c_void,
        height: crop.height as usize,
        width: crop.width as usize,
        rowBytes: row_bytes,
    };
    let dst_buf = VImageBuffer {
        data: out.as_mut_ptr().cast::<c_void>(),
        height: dst_h as usize,
        width: dst_w as usize,
        rowBytes: dst_w as usize * 4,
    };
    // SAFETY: `src_buf` describes `crop.height` rows of `row_bytes` inside
    // the source (`is_sampleable` checked its length and the crop is clamped
    // to it); `out` is `out_len` bytes; vImage only reads `src` and only
    // writes `dest`.
    let err = unsafe { vImageScale_ARGB8888(&src_buf, &dst_buf, core::ptr::null_mut(), NO_FLAGS) };
    if err != NO_ERROR {
        crate::plog_warn!(
            "[resample] vImageScale_ARGB8888 failed ({}) — using the portable scaler",
            err
        );
        return image_scale::resample_frame_rect(src, crop, dst_w, dst_h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_src(bytes: &[u8], w: u32, h: u32) -> SrcImage<'_> {
        SrcImage {
            bytes,
            format: RawImageFormat::RGBA8,
            width: w,
            height: h,
        }
    }

    #[test]
    fn a_solid_colour_survives_any_scale_exactly() {
        let bytes = [40u8, 90, 200, 255].repeat(32 * 24);
        let src = rgba_src(&bytes, 32, 24);
        for (w, h) in [(8, 6), (64, 48), (1, 1), (32, 24)] {
            let out = resample_frame_rect(&src, SrcRect::full(32, 24), w, h);
            assert_eq!(out.len(), (w * h * 4) as usize);
            assert!(
                out.chunks_exact(4).all(|px| px == [40, 90, 200, 255]),
                "{w}x{h}: a solid colour must come out solid"
            );
        }
    }

    #[test]
    fn a_bgra_frame_stays_bgra() {
        let bytes = [10u8, 20, 200, 255].repeat(16 * 16); // B=10 G=20 R=200
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::BGRA8,
            width: 16,
            height: 16,
        };
        let out = resample_frame_rect(&src, SrcRect::full(16, 16), 4, 4);
        assert!(
            out.chunks_exact(4).all(|px| px == [10, 20, 200, 255]),
            "{:?}",
            &out[..8]
        );
    }

    #[test]
    fn a_crop_reads_only_its_rect() {
        // 8x2: the left half red, the right half blue; the right half alone
        // scaled down is pure blue.
        let mut bytes: Vec<u8> = Vec::new();
        for _ in 0..2 {
            for x in 0..8 {
                let px: [u8; 4] = if x < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                bytes.extend_from_slice(&px);
            }
        }
        let src = rgba_src(&bytes, 8, 2);
        let crop = SrcRect {
            x: 4,
            y: 0,
            width: 4,
            height: 2,
        };
        let out = resample_frame_rect(&src, crop, 2, 1);
        assert!(out.chunks_exact(4).all(|px| px == [0, 0, 255, 255]), "{out:?}");
    }

    #[test]
    fn vimage_matches_the_portable_scaler_within_rounding_on_a_gradient() {
        // The contract behind `register_frame_resampler`: the platform
        // scaler is a drop-in for the reference one. Lanczos vs. the
        // reference box filter differ by a few LSBs on a smooth gradient.
        let (w, h) = (64u32, 48u32);
        let mut bytes = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                bytes.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, 128, 255]);
            }
        }
        let src = rgba_src(&bytes, w, h);
        let fast = resample_frame_rect(&src, SrcRect::full(w, h), 16, 12);
        let reference = image_scale::resample_frame_rect(&src, SrcRect::full(w, h), 16, 12);
        assert_eq!(fast.len(), reference.len());
        let worst = fast
            .iter()
            .zip(reference.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 16,
            "vImage and the portable scaler disagree by {worst} on a gradient"
        );
    }

    #[test]
    fn bad_input_is_empty_not_a_crash() {
        let bytes = [1u8; 4];
        let src = rgba_src(&bytes, 1, 1);
        assert!(resample_frame_rect(&src, SrcRect::full(1, 1), 0, 4).is_empty());
        let truncated = SrcImage {
            bytes: &bytes[..2],
            ..src
        };
        assert!(resample_frame_rect(&truncated, SrcRect::full(1, 1), 2, 2).is_empty());
    }
}
