//! Windows camera capture backend.
//!
//! Two implementations behind the `capture_common` seam (open/read/close):
//!
//! - **`camera-native` feature ON** → the real `nokhwa` (Media Foundation)
//!   backend with RGBA decode. nokhwa's `decoding` feature pulls `mozjpeg-sys`,
//!   whose build script C-compiles libjpeg-turbo, so this needs a Windows/mingw
//!   C toolchain and does NOT `cargo check --target *-windows-gnu` without one.
//!
//! - **`camera-native` feature OFF** (default) → a pure-Rust STUB: `open` fails,
//!   so the capture worker falls back to its test pattern. No `nokhwa` dep, so
//!   Windows CROSS-COMPILES with no C toolchain ("everything pure-Rust / dlopen").
//!
//! Keep the real backend (it's correct) and flip `camera-native` on once a
//! pure-Rust Media-Foundation + JPEG/YUYV decode path replaces mozjpeg.
//! macOS (objc2/AVFoundation) and Linux (libv4l2) backends are unaffected.

#[cfg(feature = "camera-native")]
mod native {
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
        let format =
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
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
}

#[cfg(feature = "camera-native")]
pub use native::{close, open, read};

#[cfg(not(feature = "camera-native"))]
mod stub {
    /// Stub: always fails to open (`0`) → the worker uses the test pattern.
    pub fn open(_index: u32, _width: u32, _height: u32) -> u64 {
        crate::plog_warn!(
            "[camera] Windows camera is the pure-Rust stub (build with feature \
             `camera-native` for the nokhwa backend) — using the test pattern"
        );
        0
    }
    /// Stub: no frames.
    pub fn read(_handle: u64, _out: &mut Vec<u8>) -> (u32, u32) {
        (0, 0)
    }
    /// Stub: nothing to free.
    pub fn close(_handle: u64) {}
}

#[cfg(not(feature = "camera-native"))]
pub use stub::{close, open, read};
