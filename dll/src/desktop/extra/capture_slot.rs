//! The producer → consumer hand-off of the macOS capture backends: the
//! AVFoundation camera and the ScreenCaptureKit screen share publish BGRA
//! frames from their dispatch queues, the widget's worker thread drains RGBA
//! frames.
//!
//! THE CLASS this exists for — "six full-resolution passes per frame"
//!: each backend's callback
//! used to `vec![0u8; w * h * 4]` on EVERY frame (8 MB of freshly zeroed
//! pages at 1080p, freed again when the next frame replaced it), swizzle it
//! with a scalar bounds-checked per-pixel loop, and the worker's `read()`
//! polled the slot every 8 ms. Both backends carried a copy of that code.
//! One slot now: the buffer is REUSED across frames, the swizzle is
//! row-wise over slices, and the reader sleeps on a condvar the callback
//! signals.
//!
//! Plain `std`, no Objective-C: the Linux CI compiles and tests it.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Inner {
    /// The latest frame, tightly packed RGBA8, alpha 255.
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    /// Bumped per published frame; a reader compares it against the last
    /// sequence it returned.
    seq: u64,
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

    /// Publish one frame from a locked BGRA pixel buffer. Returns `true` for
    /// the very first frame (callers log that one — the callback is hot).
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
        let first = slot.seq == 0;
        let row_bytes = w * 4;
        // `resize`, not a new Vec: the allocation survives from frame to frame.
        slot.rgba.resize(row_bytes * h, 0);
        for y in 0..h {
            // SAFETY: the caller guarantees `h` rows of `stride` bytes with
            // `w` BGRA pixels each.
            let src = unsafe { core::slice::from_raw_parts(base.add(y * stride), row_bytes) };
            let dst = &mut slot.rgba[y * row_bytes..(y + 1) * row_bytes];
            swizzle_bgra_row_to_rgba(src, dst);
        }
        slot.width = w as u32;
        slot.height = h as u32;
        slot.seq = slot.seq.wrapping_add(1);
        drop(slot);
        self.ready.notify_all();
        first
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
                out.extend_from_slice(&slot.rgba);
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
        out.extend_from_slice(&slot.rgba);
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
