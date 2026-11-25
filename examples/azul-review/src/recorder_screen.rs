//! The screen track's picture: AzReview's own presented frames, kept as a shadow of the window and
//! handed to the encoder when it is time.
//!
//! The engine's frame tap tells the recorder, each time the window's frame changed, which rectangles
//! changed and shows it the pixels for the length of the call ([`TapFrame`]). The recorder copies only
//! those rows into a shadow of the window ([`ScreenCapture::tap`]): about 0.01 ms for the rectangle
//! of a pen stroke and half a millisecond for a whole 2880 x 1800 window, measured. What the encoder
//! gets is decided on the recorder's timer ([`ScreenCapture::take_frame`]), never in the tap, so a
//! burst of changes costs one frame per interval:
//!
//! * a CHANGE is sent when the window changed and at least `min_gap_us` has gone by since the last
//!   frame (10 a second at most, by default): the newest picture, not every one;
//! * after the last change of a burst the same picture is sent THREE more times, 120, 400 and 1200 ms
//!   after it changed ([`SETTLE_US`]), so the encoder refines the text to full sharpness (a static
//!   text frame is 41 dB at once and 46 dB after twenty repeats, measured); then nothing is sent
//!   until the window changes again: an idle window costs no encoding and no bytes;
//! * a KEYFRAME is asked for at the start, after a pause, when a segment is about to rotate, and
//!   when two seconds have gone by since the last one while frames are being sent.
//!
//! The pictures are the shadow's pixels as the CPU renderer made them (RGBA, premultiplied: the
//! window is opaque), cropped to an even size because H.264 wants one.
//!
//! The same pacing serves the OTHER source of the screen, the system's own capture of the window
//! (ScreenCaptureKit, for a window on the GPU path, which has no CPU frame to tap): its frames come
//! whole, in the format the system made them ([`ScreenCapture::push_external`]); there is no shadow, the
//! newest frame is kept and sent on the same pace with the same sharpening repeats. Plain `std`, no azul.

use crate::recorder_codec::{PixelFormat, RawFrame};

/// When the settle repeats are sent after the last change, microseconds.
pub static SETTLE_US: [u64; 3] = [120_000, 400_000, 1_200_000];

/// The largest window picture the recorder takes: 8192 x 8192 (a 5K display at 2x is smaller).
pub const MAX_SIDE: u32 = 8_192;

/// A rectangle of the window in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// What changed in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Damage<'a> {
    /// Everything (the first frame, a resize, a scroll that moved the page).
    Full,
    /// These rectangles.
    Rects(&'a [Rect]),
}

/// A frame the engine's tap shows, valid for the call.
#[derive(Debug, Clone, Copy)]
pub struct TapFrame<'a> {
    pub width: u32,
    pub height: u32,
    /// Bytes from one row to the next.
    pub stride: usize,
    /// Device pixels to a point (2.0 on a Retina display).
    pub scale: f32,
    pub damage: Damage<'a>,
    /// RGBA8, `stride * height` bytes at least.
    pub pixels: &'a [u8],
}

/// What a tap did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tapped {
    /// The frame cannot be a window's: why not (the recorder says it once).
    Ignored(&'static str),
    /// The shadow was made anew at this size (the first frame, or the window was resized).
    Resized,
    /// Rows were copied: how many.
    Copied { rows: u32 },
}

/// What the timer is told to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Nothing,
    /// The window changed.
    Change,
    /// The picture is the same; this is the n-th repeat (0 to 2) for sharpness.
    Settle(u8),
}

/// How a screen capture is paced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    /// The least time between two frames, microseconds.
    pub min_gap_us: u64,
    /// A keyframe when this long has gone by since the last one and a frame is sent.
    pub keyframe_every_us: u64,
}

impl Pace {
    /// At most `fps` frames a second, a keyframe every two seconds.
    #[must_use]
    pub const fn at_fps(fps: u32) -> Self {
        let fps = if fps == 0 { 1 } else { fps as u64 };
        Self {
            min_gap_us: 1_000_000 / fps,
            keyframe_every_us: 2_000_000,
        }
    }
}

/// What the capture has done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenStats {
    pub taps: u64,
    pub rows_copied: u64,
    pub changes_sent: u64,
    pub settles_sent: u64,
    pub ignored: u64,
}

/// The shadow of the window and the pacing of what is sent from it.
pub struct ScreenCapture {
    pace: Pace,
    width: u32,
    height: u32,
    scale: f32,
    shadow: Vec<u8>,
    dirty: bool,
    last_change_us: u64,
    last_sent_us: Option<u64>,
    settles_done: u8,
    last_key_us: Option<u64>,
    force_key: bool,
    last_taken: Due,
    /// The newest whole frame of the system's capture; when there is one it is the picture.
    external: Option<RawFrame>,
    stats: ScreenStats,
}

impl ScreenCapture {
    /// A capture with nothing seen yet.
    #[must_use]
    pub const fn new(pace: Pace) -> Self {
        Self {
            pace,
            width: 0,
            height: 0,
            scale: 1.0,
            shadow: Vec::new(),
            dirty: false,
            last_change_us: 0,
            last_sent_us: None,
            settles_done: 0,
            last_key_us: None,
            force_key: true,
            last_taken: Due::Nothing,
            external: None,
            stats: ScreenStats { taps: 0, rows_copied: 0, changes_sent: 0, settles_sent: 0, ignored: 0 },
        }
    }

    /// What it has done.
    #[must_use]
    pub const fn stats(&self) -> ScreenStats {
        self.stats
    }

    /// The window's size in device pixels as last seen; `(0, 0)` before the first frame.
    #[must_use]
    pub const fn window_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Device pixels to a point.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// The size of the pictures sent: the window's, made even.
    #[must_use]
    pub const fn picture_size(&self) -> (u32, u32) {
        (self.width & !1, self.height & !1)
    }

    /// The next frame must be a keyframe (a pause ended, a segment is about to rotate).
    pub fn request_keyframe(&mut self) {
        self.force_key = true;
    }

    /// A frame sent to the encoder was refused: send the picture again at the next opportunity.
    pub fn retry(&mut self) {
        self.dirty = true;
    }

    /// What the last frame taken was: a refused CHANGE is sent again ([`Self::retry`]); a refused repeat is
    /// not (the next repeat is already on its way).
    #[must_use]
    pub const fn last_taken(&self) -> Due {
        self.last_taken
    }

    /// Whether the picture comes from the system's capture (not from the engine's tap).
    #[must_use]
    pub const fn is_external(&self) -> bool {
        self.external.is_some()
    }

    /// The system's capture delivered a whole frame of the window (NV12 or BGRA, as it came).
    pub fn push_external(&mut self, frame: RawFrame, scale: f32, now_us: u64) -> Tapped {
        self.stats.taps += 1;
        let (w, h) = (frame.width, frame.height);
        let need = match frame.format {
            PixelFormat::Nv12 => (w as usize) * (h as usize) * 3 / 2,
            PixelFormat::Rgba8 | PixelFormat::Bgra8 => (w as usize) * (h as usize) * 4,
        };
        if w < 2 || h < 2 || w > MAX_SIDE || h > MAX_SIDE || w % 2 != 0 || h % 2 != 0 {
            self.stats.ignored += 1;
            return Tapped::Ignored("the captured window's size is out of range or odd");
        }
        if frame.bytes.len() < need {
            self.stats.ignored += 1;
            return Tapped::Ignored("the captured frame's pixels are shorter than its size says");
        }
        let resized = (w, h) != (self.width, self.height);
        self.width = w;
        self.height = h;
        self.scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        // Frames of the system's capture are the picture from now on: the shadow is not needed.
        self.shadow = Vec::new();
        self.external = Some(frame);
        self.dirty = true;
        self.last_change_us = now_us;
        if resized {
            Tapped::Resized
        } else {
            Tapped::Copied { rows: h }
        }
    }

    /// A resumed recording starts with the picture as it is now.
    pub fn mark_changed(&mut self, now_us: u64) {
        if !self.shadow.is_empty() || self.external.is_some() {
            self.dirty = true;
            self.last_change_us = now_us;
            self.force_key = true;
        }
    }

    /// The engine showed a frame: copies what changed into the shadow.
    pub fn tap(&mut self, frame: &TapFrame<'_>, now_us: u64) -> Tapped {
        self.stats.taps += 1;
        let (w, h) = (frame.width, frame.height);
        let row = (w as usize).saturating_mul(4);
        if w < 2 || h < 2 || w > MAX_SIDE || h > MAX_SIDE {
            self.stats.ignored += 1;
            return Tapped::Ignored("the window's size is out of range");
        }
        if frame.stride < row || frame.pixels.len() < frame.stride.saturating_mul(h as usize - 1).saturating_add(row) {
            self.stats.ignored += 1;
            return Tapped::Ignored("the frame's pixels are shorter than its size says");
        }
        let resized = (w, h) != (self.width, self.height);
        if resized {
            self.width = w;
            self.height = h;
            self.shadow = vec![0; row * h as usize];
        }
        self.scale = if frame.scale.is_finite() && frame.scale > 0.0 { frame.scale } else { 1.0 };
        let full = resized || matches!(frame.damage, Damage::Full);
        let mut rows = 0_u32;
        if full {
            for y in 0..h as usize {
                let src = &frame.pixels[y * frame.stride..y * frame.stride + row];
                self.shadow[y * row..(y + 1) * row].copy_from_slice(src);
            }
            rows = h;
        } else if let Damage::Rects(rects) = frame.damage {
            for r in rects {
                let (x0, y0) = (r.x.min(w), r.y.min(h));
                let (x1, y1) = (r.x.saturating_add(r.w).min(w), r.y.saturating_add(r.h).min(h));
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                for y in y0..y1 {
                    let (a, b) = (x0 as usize * 4, x1 as usize * 4);
                    let src = &frame.pixels[y as usize * frame.stride + a..y as usize * frame.stride + b];
                    self.shadow[y as usize * row + a..y as usize * row + b].copy_from_slice(src);
                }
                rows += y1 - y0;
            }
        }
        self.stats.rows_copied += u64::from(rows);
        if rows > 0 || resized {
            self.dirty = true;
            self.last_change_us = now_us;
        }
        if resized {
            Tapped::Resized
        } else {
            Tapped::Copied { rows }
        }
    }

    /// What the timer should send now.
    #[must_use]
    pub fn due(&self, now_us: u64) -> Due {
        if self.shadow.is_empty() && self.external.is_none() {
            return Due::Nothing;
        }
        let gap_ok = self.last_sent_us.is_none_or(|t| now_us.saturating_sub(t) >= self.pace.min_gap_us);
        if self.dirty {
            return if gap_ok { Due::Change } else { Due::Nothing };
        }
        let n = usize::from(self.settles_done);
        if self.last_sent_us.is_some() && n < SETTLE_US.len() && gap_ok && now_us.saturating_sub(self.last_change_us) >= SETTLE_US[n] {
            return Due::Settle(self.settles_done);
        }
        Due::Nothing
    }

    /// Takes the picture to send, if one is due: the shadow, cropped to an even size, and whether it
    /// must be a keyframe. The pacing moves on as if it were sent (call [`Self::retry`] if it is refused).
    pub fn take_frame(&mut self, now_us: u64) -> Option<(RawFrame, bool)> {
        let due = self.due(now_us);
        if due == Due::Nothing {
            return None;
        }
        let (w, h) = self.picture_size();
        let picture = if let Some(frame) = &self.external {
            frame.clone()
        } else {
            let row = self.width as usize * 4;
            let tight = w as usize * 4;
            let mut bytes = Vec::with_capacity(tight * h as usize);
            for y in 0..h as usize {
                bytes.extend_from_slice(&self.shadow[y * row..y * row + tight]);
            }
            RawFrame { width: w, height: h, format: PixelFormat::Rgba8, bytes }
        };
        let keyframe = self.force_key || self.last_key_us.is_none_or(|k| now_us.saturating_sub(k) >= self.pace.keyframe_every_us);
        if keyframe {
            self.last_key_us = Some(now_us);
            self.force_key = false;
        }
        match due {
            Due::Change => {
                self.dirty = false;
                self.settles_done = 0;
                self.stats.changes_sent += 1;
            }
            Due::Settle(n) => {
                self.settles_done = n + 1;
                self.stats.settles_sent += 1;
            }
            Due::Nothing => {}
        }
        self.last_sent_us = Some(now_us);
        self.last_taken = due;
        Some((picture, keyframe))
    }
}

#[cfg(test)]
#[path = "recorder_screen_test.rs"]
mod recorder_screen_test;
