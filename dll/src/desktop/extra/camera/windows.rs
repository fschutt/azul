//! Windows camera capture backend via `nokhwa` (which wraps Media Foundation).
//! Used only on Windows - the "difficult" platform, where hand-rolling COM /
//! Media Foundation is fragile; nokhwa owns that ABI and hands back a clean
//! pull API + RGBA decode. macOS uses objc2/AVFoundation, linux uses libv4l2;
//! all three feed the same `capture_common` seam.

use nokhwa::{
    pixel_format::RgbAFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};

/// Live capture state behind the seam's `u64` handle (worker-thread-local).
struct NokhwaCam {
    camera: Camera,
}

/// Open camera `index` at the highest frame rate the device offers (the seam's
/// requested `width`/`height` are advisory - nokhwa negotiates). Returns a
/// boxed handle, or `0` on failure (worker falls back to the test pattern).
pub fn open(index: u32, _width: u32, _height: u32) -> u64 {
    let format = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = match Camera::new(CameraIndex::Index(index), format) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    if camera.open_stream().is_err() {
        return 0;
    }
    Box::into_raw(Box::new(NokhwaCam { camera })) as u64
}

/// Capture + decode the next frame to tightly-packed RGBA8 into `out`. Returns
/// the frame `(width, height)`, or `(0, 0)` on error (the worker stops).
pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let cam = match unsafe { (handle as *mut NokhwaCam).as_mut() } {
        Some(c) => c,
        None => return (0, 0),
    };
    let frame = match cam.camera.frame() {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };
    let img = match frame.decode_image::<RgbAFormat>() {
        Ok(i) => i,
        Err(_) => return (0, 0),
    };
    let (w, h) = (img.width(), img.height());
    out.clear();
    out.extend_from_slice(img.as_raw());
    (w, h)
}

/// Stop streaming + free the capture (drops the boxed `NokhwaCam`).
pub fn close(handle: u64) {
    if handle != 0 {
        unsafe {
            drop(Box::from_raw(handle as *mut NokhwaCam));
        }
    }
}
