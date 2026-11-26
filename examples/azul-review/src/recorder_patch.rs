//! Looking for the one colour of the probe's window in a picture of the screen (`recorder_probe.rs`): plain
//! bytes in, a count and a yes or no out, so that what the check decides is tested without a screen.

/// How many of a frame's pixels must be the patch's colour for the window to count as seen in that frame: a window
/// of 300 x 200 points is some tens of thousands of pixels in the picture, and a stray pixel is nothing.
pub const SEEN_PIXELS: usize = 2_000;

/// How many pixels of `rgba` (tightly packed, four bytes a pixel) are the colour of the patch: magenta, give or take
/// what a capture and the scaling of its picture make of it. Bytes that do not make a whole pixel at the end are
/// not counted.
#[must_use]
pub fn patch_pixels(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4).filter(|p| p[0] > 200 && p[1] < 70 && p[2] > 200).count()
}

/// Whether a run saw the window: it looked at frames, and in at least a tenth of them (a frame that is half
/// drawn can miss) enough pixels were the patch's colour.
#[must_use]
pub fn window_seen(frames: u32, with_patch: u32) -> bool {
    frames > 0 && u64::from(with_patch) * 10 >= u64::from(frames)
}

#[cfg(test)]
#[path = "recorder_patch_test.rs"]
mod recorder_patch_test;
