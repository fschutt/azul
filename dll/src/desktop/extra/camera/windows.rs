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
    use azul_layout::widgets::capture_common::{CaptureRead, CaptureRequest};
    use nokhwa::{
        pixel_format::RgbAFormat,
        utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution},
        Camera,
    };

    /// Live capture state behind the seam's `u64` handle (worker-thread-local).
    struct NokhwaCam {
        camera: Camera,
    }

    /// Open camera `index` at the format closest to the requested size + fps
    /// (nokhwa negotiates; a zero size asks for the highest frame rate the
    /// device offers). Returns a boxed handle, or `0` on failure (worker falls
    /// back to the test pattern).
    pub fn open(request: &CaptureRequest) -> u64 {
        let index = request.index;
        let wanted = if request.width > 0 && request.height > 0 {
            RequestedFormatType::Closest(CameraFormat::new(
                Resolution::new(request.width, request.height),
                FrameFormat::MJPEG,
                request.fps_or(30),
            ))
        } else {
            RequestedFormatType::AbsoluteHighestFrameRate
        };
        let format = RequestedFormat::new::<RgbAFormat>(wanted);
        let mut camera = match Camera::new(CameraIndex::Index(index), format) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        if camera.open_stream().is_err() {
            return 0;
        }
        Box::into_raw(Box::new(NokhwaCam { camera })) as u64
    }

    /// Capture + decode the next frame to tightly-packed RGBA8 into `out`.
    /// `Frame` on success; a frame that failed to arrive is `Idle` (the
    /// worker keeps polling), a frame that failed to DECODE is `Ended`.
    pub fn read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
        let cam = match unsafe { (handle as *mut NokhwaCam).as_mut() } {
            Some(c) => c,
            None => return CaptureRead::Ended,
        };
        let frame = match cam.camera.frame() {
            Ok(f) => f,
            Err(_) => return CaptureRead::Idle,
        };
        let img = match frame.decode_image::<RgbAFormat>() {
            Ok(i) => i,
            Err(_) => return CaptureRead::Ended,
        };
        let (w, h) = (img.width(), img.height());
        out.clear();
        out.extend_from_slice(img.as_raw());
        CaptureRead::Frame {
            width: w,
            height: h,
        }
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
    pub fn open(_request: &azul_layout::widgets::capture_common::CaptureRequest) -> u64 {
        crate::plog_warn!(
            "[camera] Windows camera is the pure-Rust stub (build with feature \
             `camera-native` for the nokhwa backend) — using the test pattern"
        );
        0
    }
    /// Stub: no frames.
    pub fn read(_handle: u64, _out: &mut Vec<u8>) -> azul_layout::widgets::capture_common::CaptureRead {
        azul_layout::widgets::capture_common::CaptureRead::Ended
    }
    /// Stub: nothing to free.
    pub fn close(_handle: u64) {}
}

#[cfg(not(feature = "camera-native"))]
pub use stub::{close, open, read};
