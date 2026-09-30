//! The producer → consumer hand-off of the macOS capture backends: the
//! AVFoundation camera and the ScreenCaptureKit screen share publish frames
//! from their dispatch queues, the widget's worker thread takes them.
//!
//! THE CLASS this exists for — "six full-resolution passes per frame"
//!: each backend's callback
//! used to `vec![0u8; w * h * 4]` on EVERY frame (8 MB of freshly zeroed
//! pages at 1080p, freed again when the next frame replaced it), swizzle it
//! with a scalar bounds-checked per-pixel loop, and the worker's `read()`
//! polled the slot every 8 ms. Both backends carried a copy of that code.
//! One slot now: the buffer is REUSED across frames, and the reader sleeps on
//! a condvar the callback signals.
//!
//! A frame is published in the format the platform handed out, not
//! converted: BGRA rows as they are ([`CaptureSlot::publish_packed`]) or
//! both NV12 planes ([`CaptureSlot::publish_nv12`]) — the only copy is the
//! one out of the locked (padded) platform buffer. [`CaptureSlot::take_newer`]
//! hands the frame over by SWAPPING buffers with the reader, so it is never
//! copied a second time. (`publish_bgra` + `read_newer` are the older
//! RGBA-converting pair.)
//!
//! Plain `std`, no Objective-C: the Linux CI compiles and tests it.

use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use azul_core::resources::{Nv12Layout, RawImageFormat};

struct Inner {
    /// The latest frame, tightly packed in `format`.
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    format: RawImageFormat,
    /// Bumped per published frame; a reader compares it against the last
    /// sequence it returned.
    seq: u64,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            width: 0,
            height: 0,
            format: RawImageFormat::RGBA8,
            seq: 0,
        }
    }
}

/// Latest-frame mailbox between a capture callback and one reader.
pub struct CaptureSlot {
    inner: Mutex<Inner>,
    ready: Condvar,
}

impl CaptureSlot {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            ready: Condvar::new(),
        })
    }

    /// Record a published frame and wake the reader. Returns `true` for the
    /// very first frame.
    fn finish_publish(&self, mut slot: std::sync::MutexGuard<'_, Inner>, w: usize, h: usize, format: RawImageFormat) -> bool {
        let first = slot.seq == 0;
        slot.width = w as u32;
        slot.height = h as u32;
        slot.format = format;
        slot.seq = slot.seq.wrapping_add(1);
        drop(slot);
        self.ready.notify_all();
        first
    }

    /// Publish one frame from a locked BGRA pixel buffer, converted to RGBA8.
    /// Returns `true` for the very first frame (callers log that one — the
    /// callback is hot).
    ///
    /// # Safety
    /// `base` must point at `h` rows of `stride` bytes each, every row
    /// holding at least `w` packed BGRA pixels, all readable for the
    /// duration of the call (a CoreVideo buffer locked by the caller).
    pub unsafe fn publish_bgra(&self, base: *const u8, w: usize, h: usize, stride: usize) -> bool {
        if base.is_null() || w == 0 || h == 0 || stride < w * 4 {
            return false;
        }
        let Ok(mut slot) = self.inner.lock() else {
            return false;
        };
        let row_bytes = w * 4;
        // `resize`, not a new Vec: the allocation survives from frame to frame.
        slot.bytes.resize(row_bytes * h, 0);
        for y in 0..h {
            // SAFETY: the caller guarantees `h` rows of `stride` bytes with
            // `w` BGRA pixels each.
            let src = unsafe { core::slice::from_raw_parts(base.add(y * stride), row_bytes) };
            let dst = &mut slot.bytes[y * row_bytes..(y + 1) * row_bytes];
            swizzle_bgra_row_to_rgba(src, dst);
        }
        self.finish_publish(slot, w, h, RawImageFormat::RGBA8)
    }

    /// Publish one frame of 4-byte pixels (`format`: BGRA8 or RGBA8) from a
    /// locked pixel buffer AS IT IS: the rows are copied out of the padded
    /// platform buffer, nothing is converted. Returns `true` for the very
    /// first frame.
    ///
    /// # Safety
    /// As [`Self::publish_bgra`]: `h` readable rows of `stride` bytes, each
    /// holding at least `w` 4-byte pixels.
    pub unsafe fn publish_packed(
        &self,
        base: *const u8,
        w: usize,
        h: usize,
        stride: usize,
        format: RawImageFormat,
    ) -> bool {
        if base.is_null() || w == 0 || h == 0 || stride < w * 4 {
            return false;
        }
        let Ok(mut slot) = self.inner.lock() else {
            return false;
        };
        let row_bytes = w * 4;
        slot.bytes.resize(row_bytes * h, 0);
        for y in 0..h {
            // SAFETY: the caller guarantees `h` rows of `stride` bytes.
            let src = unsafe { core::slice::from_raw_parts(base.add(y * stride), row_bytes) };
            slot.bytes[y * row_bytes..(y + 1) * row_bytes].copy_from_slice(src);
        }
        self.finish_publish(slot, w, h, format)
    }

    /// Publish one NV12 frame from the two planes of a locked bi-planar
    /// pixel buffer ('420v' / '420f'): the `w x h` luma plane (rows of
    /// `y_stride` bytes) and the `ceil(w/2) x ceil(h/2)` Cb,Cr plane (rows of
    /// `uv_stride` bytes), packed tight one after the other
    /// ([`Nv12Layout`]). `format` names the matrix and range. Returns `true`
    /// for the very first frame.
    ///
    /// # Safety
    /// `y` must point at `h` readable rows of `y_stride` bytes (at least `w`
    /// each), `uv` at `ceil(h/2)` rows of `uv_stride` bytes (at least
    /// `2 * ceil(w/2)` each), for the duration of the call.
    #[allow(clippy::too_many_arguments)] // two planes, their strides, the size and the format
    pub unsafe fn publish_nv12(
        &self,
        y: *const u8,
        y_stride: usize,
        uv: *const u8,
        uv_stride: usize,
        w: usize,
        h: usize,
        format: RawImageFormat,
    ) -> bool {
        let layout = Nv12Layout::new(w, h);
        let uv_row = layout.chroma_width * 2;
        if y.is_null()
            || uv.is_null()
            || w == 0
            || h == 0
            || y_stride < w
            || uv_stride < uv_row
            || !format.is_nv12()
        {
            return false;
        }
        let Some(total) = layout.checked_total_len() else {
            return false;
        };
        let Ok(mut slot) = self.inner.lock() else {
            return false;
        };
        slot.bytes.resize(total, 0);
        for row in 0..h {
            // SAFETY: the caller guarantees `h` luma rows of `y_stride` bytes.
            let src = unsafe { core::slice::from_raw_parts(y.add(row * y_stride), w) };
            slot.bytes[row * w..(row + 1) * w].copy_from_slice(src);
        }
        let uv_base = layout.y_len();
        for row in 0..layout.chroma_height {
            // SAFETY: the caller guarantees `ceil(h/2)` chroma rows of
            // `uv_stride` bytes.
            let src = unsafe { core::slice::from_raw_parts(uv.add(row * uv_stride), uv_row) };
            let at = uv_base + row * uv_row;
            slot.bytes[at..at + uv_row].copy_from_slice(src);
        }
        self.finish_publish(slot, w, h, format)
    }

    /// Wait up to `timeout` for a frame newer than `*last_seq` and hand it
    /// over by SWAPPING buffers with `out` (the slot writes its next frame
    /// into `out`'s old allocation), returning its size and format. `None`
    /// when no newer frame arrived in time or the lock is poisoned.
    pub fn take_newer(
        &self,
        last_seq: &mut u64,
        out: &mut Vec<u8>,
        timeout: Duration,
    ) -> Option<(u32, u32, RawImageFormat)> {
        let deadline = Instant::now() + timeout;
        let mut slot = self.inner.lock().ok()?;
        loop {
            if slot.seq != *last_seq && slot.width > 0 {
                *last_seq = slot.seq;
                core::mem::swap(out, &mut slot.bytes);
                return Some((slot.width, slot.height, slot.format));
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = self.ready.wait_timeout(slot, deadline - now).ok()?;
            slot = guard;
        }
    }

    /// Wait up to `timeout` for a frame newer than `*last_seq`, copy it into
    /// `out` (reusing `out`'s allocation) and return its size. `None` when no
    /// newer frame arrived in time or the lock is poisoned.
    pub fn read_newer(
        &self,
        last_seq: &mut u64,
        out: &mut Vec<u8>,
        timeout: Duration,
    ) -> Option<(u32, u32)> {
        let deadline = Instant::now() + timeout;
        let mut slot = self.inner.lock().ok()?;
        loop {
            if slot.seq != *last_seq && slot.width > 0 {
                *last_seq = slot.seq;
                out.clear();
                out.extend_from_slice(&slot.bytes);
                return Some((slot.width, slot.height));
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = self.ready.wait_timeout(slot, deadline - now).ok()?;
            slot = guard;
        }
    }

    /// The last published frame, whatever its sequence (the screen-share
    /// idle path: a desktop that does not change emits nothing, and
    /// returning "no frame" would read as end-of-stream).
    pub fn read_last(&self, out: &mut Vec<u8>) -> Option<(u32, u32)> {
        let slot = self.inner.lock().ok()?;
        if slot.width == 0 {
            return None;
        }
        out.clear();
        out.extend_from_slice(&slot.bytes);
        Some((slot.width, slot.height))
    }
}

/// One row: packed BGRA → packed RGBA, alpha forced opaque (every capture
/// source is opaque; a real alpha would be premultiplied junk anyway).
fn swizzle_bgra_row_to_rgba(src: &[u8], dst: &mut [u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 BGRA plane with 4 bytes of row padding (stride 12).
    fn plane() -> Vec<u8> {
        let mut p = Vec::new();
        for row in 0..2u8 {
            for col in 0..2u8 {
                let v = row * 2 + col; // 0..4
                p.extend_from_slice(&[10 + v, 20 + v, 30 + v, 7]); // B G R A
            }
            p.extend_from_slice(&[0xEE; 4]); // padding the reader must skip
        }
        p
    }

    #[test]
    fn publishes_swizzled_rgba_and_skips_the_stride_padding() {
        let slot = CaptureSlot::new();
        let p = plane();
        let first = unsafe { slot.publish_bgra(p.as_ptr(), 2, 2, 12) };
        assert!(first, "the first frame is reported as such");
        let mut out = Vec::new();
        let mut seq = 0;
        let dims = slot.read_newer(&mut seq, &mut out, Duration::from_millis(10));
        assert_eq!(dims, Some((2, 2)));
        assert_eq!(out.len(), 16, "tightly packed, no padding");
        // pixel (1, 1): v = 3 → B=13 G=23 R=33 → RGBA (33, 23, 13, 255)
        assert_eq!(&out[12..16], &[33, 23, 13, 255]);
        assert_eq!(&out[0..4], &[30, 20, 10, 255]);
        assert!(
            !unsafe { slot.publish_bgra(p.as_ptr(), 2, 2, 12) },
            "later frames are not 'first'"
        );
    }

    #[test]
    fn a_reader_sees_each_frame_once_and_times_out_without_a_new_one() {
        let slot = CaptureSlot::new();
        let p = plane();
        unsafe { slot.publish_bgra(p.as_ptr(), 2, 2, 12) };
        let mut out = Vec::new();
        let mut seq = 0;
        assert!(slot
            .read_newer(&mut seq, &mut out, Duration::from_millis(10))
            .is_some());
        let t0 = Instant::now();
        assert!(
            slot.read_newer(&mut seq, &mut out, Duration::from_millis(30))
                .is_none(),
            "the same frame is not served twice as 'newer'"
        );
        assert!(
            t0.elapsed() >= Duration::from_millis(25),
            "the wait is a timed condvar wait"
        );
        // The idle path still hands out the last frame.
        assert_eq!(slot.read_last(&mut out), Some((2, 2)));
        assert_eq!(out.len(), 16);
    }

    #[test]
    fn the_reader_is_woken_by_the_publisher() {
        let slot = CaptureSlot::new();
        let publisher = slot.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            let p = plane();
            unsafe { publisher.publish_bgra(p.as_ptr(), 2, 2, 12) };
        });
        let mut out = Vec::new();
        let mut seq = 0;
        let t0 = Instant::now();
        let dims = slot.read_newer(&mut seq, &mut out, Duration::from_secs(5));
        assert_eq!(dims, Some((2, 2)));
        assert!(
            t0.elapsed() < Duration::from_secs(4),
            "woken by the publish, not by the deadline"
        );
        handle.join().unwrap();
    }

    #[test]
    fn an_nv12_frame_is_packed_tight_from_its_padded_planes() {
        use azul_core::resources::RawImageFormat;
        // 2x2 luma with rows padded to 4 bytes; one Cb,Cr pair, row padded.
        let y = [1u8, 2, 0xEE, 0xEE, 3, 4, 0xEE, 0xEE];
        let uv = [100u8, 200, 0xEE, 0xEE];
        let slot = CaptureSlot::new();
        let first = unsafe {
            slot.publish_nv12(
                y.as_ptr(),
                4,
                uv.as_ptr(),
                4,
                2,
                2,
                RawImageFormat::NV12Rec709Video,
            )
        };
        assert!(first);
        let mut out = Vec::new();
        let mut seq = 0;
        let got = slot.take_newer(&mut seq, &mut out, Duration::from_millis(10));
        assert_eq!(got, Some((2, 2, RawImageFormat::NV12Rec709Video)));
        assert_eq!(out, vec![1, 2, 3, 4, 100, 200], "both planes, tight, no padding");
    }

    #[test]
    fn a_bgra_frame_is_handed_over_as_it_is() {
        use azul_core::resources::RawImageFormat;
        let slot = CaptureSlot::new();
        let p = plane();
        unsafe { slot.publish_packed(p.as_ptr(), 2, 2, 12, RawImageFormat::BGRA8) };
        let mut out = Vec::new();
        let mut seq = 0;
        let got = slot.take_newer(&mut seq, &mut out, Duration::from_millis(10));
        assert_eq!(got, Some((2, 2, RawImageFormat::BGRA8)));
        assert_eq!(&out[0..4], &[10, 20, 30, 7], "no swizzle: BGRA bytes as captured");
        assert_eq!(out.len(), 16, "tightly packed");
    }

    #[test]
    fn take_newer_swaps_buffers_instead_of_copying_the_frame() {
        use azul_core::resources::RawImageFormat;
        // The reader's buffer and the slot's trade places: the two
        // allocations ping-pong, and no frame is ever copied a second time.
        let slot = CaptureSlot::new();
        let p = plane();
        let mut seq = 0;
        let mut out = Vec::with_capacity(64);
        let mine = out.as_ptr();
        unsafe { slot.publish_packed(p.as_ptr(), 2, 2, 12, RawImageFormat::BGRA8) };
        slot.take_newer(&mut seq, &mut out, Duration::from_millis(10))
            .expect("a frame");
        let theirs = out.as_ptr();
        assert_ne!(theirs, mine, "the reader got the slot's buffer");
        unsafe { slot.publish_packed(p.as_ptr(), 2, 2, 12, RawImageFormat::BGRA8) };
        slot.take_newer(&mut seq, &mut out, Duration::from_millis(10))
            .expect("a frame");
        assert_eq!(out.as_ptr(), mine, "and the slot wrote the next one into the reader's old one");
    }

    #[test]
    fn a_degenerate_plane_is_rejected() {
        let slot = CaptureSlot::new();
        let p = plane();
        assert!(!unsafe { slot.publish_bgra(core::ptr::null(), 2, 2, 12) });
        assert!(!unsafe { slot.publish_bgra(p.as_ptr(), 0, 2, 12) });
        assert!(
            !unsafe { slot.publish_bgra(p.as_ptr(), 2, 2, 4) },
            "stride shorter than a row"
        );
        let mut out = Vec::new();
        assert!(slot.read_last(&mut out).is_none());
    }
}
