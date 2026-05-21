//! Linux v4l2 camera capture backend for the capture seam, via `rscam`
//! (libc ioctls, no system-lib link, so it cross-compiles). Plugs into
//! `capture_common::register_camera_backend`, so `CameraWidget` shows the real
//! camera where `/dev/video*` exists; the worker falls back to its test
//! pattern when `open` returns `0` (no device / unsupported format).
//!
//! Captures YUYV (the near-universal UVC format) and converts to the seam's
//! tightly-packed RGBA8. `rscam` owns the v4l2 ABI + mmap streaming, so there
//! are no fragile structs / ioctl numbers transcribed here.

/// Live capture state behind the seam's `u64` handle. Worker-thread-local (the
/// camera worker calls `open`/`read`/`close` on one thread), so no `Send`.
struct V4l2Cam {
    camera: rscam::Camera,
    width: u32,
    height: u32,
}

/// Open `/dev/video{index}` at `width` x `height` (YUYV @ ~30 fps). Returns a
/// boxed `V4l2Cam` as the opaque handle, or `0` on any failure (no device,
/// format rejected) so the worker falls back to the test pattern.
pub fn open(index: u32, width: u32, height: u32) -> u64 {
    let path = format!("/dev/video{}", index);
    let mut camera = match rscam::Camera::new(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let width = if width == 0 { 640 } else { width };
    let height = if height == 0 { 480 } else { height };
    if camera
        .start(&rscam::Config {
            interval: (1, 30),
            resolution: (width, height),
            format: b"YUYV",
            ..Default::default()
        })
        .is_err()
    {
        return 0;
    }
    Box::into_raw(Box::new(V4l2Cam {
        camera,
        width,
        height,
    })) as u64
}

/// Capture the next frame, converting YUYV -> tightly-packed RGBA8 into `out`.
/// Returns the frame `(width, height)`, or `(0, 0)` on error (worker stops).
pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let cam = match unsafe { (handle as *mut V4l2Cam).as_mut() } {
        Some(c) => c,
        None => return (0, 0),
    };
    let frame = match cam.camera.capture() {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };
    let (w, h) = (cam.width, cam.height);
    out.clear();
    out.resize((w as usize) * (h as usize) * 4, 0);
    yuyv_to_rgba(&frame[..], w, h, out);
    (w, h)
}

/// Stop streaming + free the capture (drops the boxed `V4l2Cam`).
pub fn close(handle: u64) {
    if handle != 0 {
        unsafe {
            drop(Box::from_raw(handle as *mut V4l2Cam));
        }
    }
}

/// YUYV (YUV 4:2:2, 2 bytes/pixel) -> RGBA8. Each 4-byte group `Y0 U Y1 V`
/// yields two RGBA pixels sharing the chroma (BT.601 coefficients).
fn yuyv_to_rgba(yuyv: &[u8], w: u32, h: u32, out: &mut [u8]) {
    let pixels = (w as usize) * (h as usize);
    for i in 0..(pixels / 2) {
        let b = i * 4;
        if b + 3 >= yuyv.len() {
            break;
        }
        let y0 = yuyv[b] as f32;
        let u = yuyv[b + 1] as f32 - 128.0;
        let y1 = yuyv[b + 2] as f32;
        let v = yuyv[b + 3] as f32 - 128.0;
        let o0 = i * 8;
        let o1 = o0 + 4;
        if o1 + 3 >= out.len() {
            break;
        }
        out[o0] = (y0 + 1.402 * v).clamp(0.0, 255.0) as u8;
        out[o0 + 1] = (y0 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
        out[o0 + 2] = (y0 + 1.772 * u).clamp(0.0, 255.0) as u8;
        out[o0 + 3] = 255;
        out[o1] = (y1 + 1.402 * v).clamp(0.0, 255.0) as u8;
        out[o1 + 1] = (y1 - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u8;
        out[o1 + 2] = (y1 + 1.772 * u).clamp(0.0, 255.0) as u8;
        out[o1 + 3] = 255;
    }
}
