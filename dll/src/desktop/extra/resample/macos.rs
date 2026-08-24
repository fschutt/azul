//! Accelerate / vImage whole-frame scaler (macOS).
//!
//! `vImageScale_ARGB8888` resamples any interleaved 4 x 8-bit image — it is
//! channel-order agnostic, so RGBA8 and BGRA8 sources go through it as-is
//! and a BGRA source is swizzled to RGBA on the (smaller) OUTPUT. Formats
//! with fewer channels (RGB8 / R8) take the portable scaler; they never
//! come from a live-frame producer.
//!
//! Contract (see `capture_common::register_frame_resampler`): same inputs
//! -> same picture as `image_scale::resample_rgba` within rounding; pure;
//! safe to call from any thread (vImage is re-entrant). `kvImageHighQualityResampling`
//! selects the Lanczos5 kernel — Apple's documented quality option; the
//! temp buffer is `NULL` so vImage allocates its own scratch.

use core::ffi::c_void;

use azul_core::resources::RawImageFormat;
use azul_layout::image_scale::{self, SrcImage};

/// `vImage_Buffer` (Accelerate/vImage/vImage_Types.h).
#[repr(C)]
#[allow(non_snake_case)]
struct VImageBuffer {
    data: *mut c_void,
    height: usize,
    width: usize,
    rowBytes: usize,
}

/// `kvImageHighQualityResampling` (vImage_Types.h).
const HIGH_QUALITY_RESAMPLING: u32 = 32;
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

/// Resample `src` to `dst_w` x `dst_h` tightly-packed RGBA8 with vImage.
/// Empty on a zero size, an unsampleable source, or a vImage error (the
/// caller treats empty as "no cut").
pub fn resample_rgba(src: &SrcImage<'_>, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if dst_w == 0 || dst_h == 0 || !src.is_sampleable() {
        return Vec::new();
    }
    let bgra = match src.format {
        RawImageFormat::RGBA8 => false,
        RawImageFormat::BGRA8 => true,
        // Three- and one-channel sources: the portable scaler (never a live
        // frame, so speed is not the point).
        _ => return image_scale::resample_rgba(src, dst_w, dst_h),
    };
    let Some(out_len) = (dst_w as usize)
        .checked_mul(dst_h as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return Vec::new();
    };
    let mut out = vec![0u8; out_len];

    if src.width == dst_w && src.height == dst_h {
        out.copy_from_slice(&src.bytes[..out_len]);
    } else {
        let src_buf = VImageBuffer {
            data: src.bytes.as_ptr() as *mut c_void,
            height: src.height as usize,
            width: src.width as usize,
            rowBytes: src.width as usize * 4,
        };
        let dst_buf = VImageBuffer {
            data: out.as_mut_ptr().cast::<c_void>(),
            height: dst_h as usize,
            width: dst_w as usize,
            rowBytes: dst_w as usize * 4,
        };
        // SAFETY: both buffers describe live, correctly sized allocations
        // (`is_sampleable` checked the source length; `out` is `out_len`
        // bytes); vImage only reads `src` and only writes `dest`.
        let err = unsafe {
            vImageScale_ARGB8888(
                &src_buf,
                &dst_buf,
                core::ptr::null_mut(),
                HIGH_QUALITY_RESAMPLING,
            )
        };
        if err != NO_ERROR {
            crate::plog_warn!(
                "[resample] vImageScale_ARGB8888 failed ({}) — using the portable scaler",
                err
            );
            return image_scale::resample_rgba(src, dst_w, dst_h);
        }
    }
    if bgra {
        for px in out.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
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
            let out = resample_rgba(&src, w, h);
            assert_eq!(out.len(), (w * h * 4) as usize);
            assert!(
                out.chunks_exact(4).all(|px| px == [40, 90, 200, 255]),
                "{w}x{h}: a solid colour must come out solid"
            );
        }
    }

    #[test]
    fn a_bgra_source_comes_out_as_rgba() {
        let bytes = [10u8, 20, 200, 255].repeat(16 * 16); // B=10 G=20 R=200
        let src = SrcImage {
            bytes: &bytes,
            format: RawImageFormat::BGRA8,
            width: 16,
            height: 16,
        };
        let out = resample_rgba(&src, 4, 4);
        assert!(out.chunks_exact(4).all(|px| px == [200, 20, 10, 255]), "{:?}", &out[..8]);
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
        let fast = resample_rgba(&src, 16, 12);
        let reference = image_scale::resample_rgba(&src, 16, 12);
        assert_eq!(fast.len(), reference.len());
        let worst = fast
            .iter()
            .zip(reference.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(worst <= 16, "vImage and the portable scaler disagree by {worst} on a gradient");
    }

    #[test]
    fn bad_input_is_empty_not_a_crash() {
        let bytes = [1u8; 4];
        let src = rgba_src(&bytes, 1, 1);
        assert!(resample_rgba(&src, 0, 4).is_empty());
        let truncated = SrcImage { bytes: &bytes[..2], ..src };
        assert!(resample_rgba(&truncated, 2, 2).is_empty());
    }
}
