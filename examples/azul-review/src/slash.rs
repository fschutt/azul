//! The quick slash: a short, fast, nearly level stroke through a word deletes it, with no wait.
//!
//! This is only the shape: samples in (where, and when in ms after the pen went down), a [`Line`] out, or nothing. Which
//! words the line crosses, and what is done to them, is `deletion.rs`'s. The numbers are those of a quick hand
//! (`stroke_fixtures`: a slash of 76 px takes 110 ms, one of 160 px 220 ms; a loop takes half a second, an underline 240
//! ms for 70 px) and each is a named constant:
//!
//! - it takes between [`MIN_DURATION_MS`] and [`MAX_DURATION_MS`]: quicker than any hand is a test or a script (a stroke
//!   with no time recorded is never a slash), slower is a deliberate stroke - a highlight of the marker, an underline;
//! - it is between [`MIN_LENGTH_PX`] and [`MAX_LENGTH_PX`] long (from its first sample to its last): a flick across a few
//!   letters up to one across a line of code;
//! - it moves at least [`MIN_SPEED`] px per ms: 76 px in 250 ms is not a flick, however short the time;
//! - it is straight: its ends are at least [`MIN_STRAIGHTNESS`] of the way its path went (a hook, a "V", a tick, a
//!   scribble, an arc are not);
//! - it lies in a band of [`MAX_BAND_PX`] px (a line of glyphs; the lines are 15 px apart): a slope across two lines is no
//!   slash, which also keeps the angle small.
//!
//! No window, no azul: `slash_test.rs` draws it on the shared fixtures and their mirrors.

/// One sample of a stroke: where, and when (ms after the pen went down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    pub t_ms: u32,
}

/// A recognised slash: where it lay, from left to right whichever way it was drawn, and how long it took.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// The leftmost and the rightmost of its samples.
    pub x0: f32,
    pub x1: f32,
    /// Its height: the mean of its samples.
    pub y: f32,
    pub duration_ms: u32,
}

impl Line {
    /// The middle of the stroke, left to right: the word under it is the word "here".
    #[must_use]
    pub fn centre(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
}

/// The fewest samples of a stroke that is a slash (a tap has two).
pub const MIN_POINTS: usize = 3;
/// The shortest time of a slash, ms: no hand is quicker.
pub const MIN_DURATION_MS: u32 = 30;
/// The longest time of a slash, ms.
pub const MAX_DURATION_MS: u32 = 250;
/// The shortest slash, px: a flick across a few letters.
pub const MIN_LENGTH_PX: f32 = 24.0;
/// The longest, px: across a line of code, not across a page.
pub const MAX_LENGTH_PX: f32 = 420.0;
/// The slowest, px per ms.
pub const MIN_SPEED: f32 = 0.5;
/// How straight: the chord over the path.
pub const MIN_STRAIGHTNESS: f32 = 0.92;
/// How tall the band is that all its samples lie in, px.
pub const MAX_BAND_PX: f32 = 9.0;

/// The slash `samples` are, if they are one.
#[must_use]
pub fn recognise(samples: &[Sample]) -> Option<Line> {
    if samples.len() < MIN_POINTS || !samples.iter().all(|s| s.x.is_finite() && s.y.is_finite()) {
        return None;
    }
    if samples.windows(2).any(|w| w[1].t_ms < w[0].t_ms) {
        return None;
    }
    let (first, last) = (samples[0], samples[samples.len() - 1]);
    let duration_ms = last.t_ms - first.t_ms;
    if !(MIN_DURATION_MS..=MAX_DURATION_MS).contains(&duration_ms) {
        return None;
    }
    let chord = (last.x - first.x).hypot(last.y - first.y);
    if !(MIN_LENGTH_PX..=MAX_LENGTH_PX).contains(&chord) {
        return None;
    }
    let path: f32 = samples.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
    if path <= 0.0 || chord / path < MIN_STRAIGHTNESS {
        return None;
    }
    if path / (duration_ms as f32) < MIN_SPEED {
        return None;
    }
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut x0, mut x1) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut sum = 0.0;
    for s in samples {
        lo = lo.min(s.y);
        hi = hi.max(s.y);
        x0 = x0.min(s.x);
        x1 = x1.max(s.x);
        sum += s.y;
    }
    if hi - lo > MAX_BAND_PX {
        return None;
    }
    Some(Line {
        x0,
        x1,
        y: sum / samples.len() as f32,
        duration_ms,
    })
}

#[cfg(test)]
#[path = "slash_test.rs"]
mod slash_test;
