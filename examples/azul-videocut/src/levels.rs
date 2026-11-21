//! The audio meters: how loud each channel of a media item is over time, read once from its
//! file with azul's `AudioFileDecoder` on a worker ([`envelope_from`]), and the reading at
//! the playhead from the clips on the unmuted audio tracks ([`reading`]). The meters show
//! the louder of the clips per channel, on a decibel scale from [`FLOOR_DB`] to full scale.

use std::collections::HashMap;

use azul::audio::AudioFileDecoder;

use crate::model::{Frame, Project, TrackKind};

/// The length of one slice of an envelope, in seconds: one meter reading per 20 ms.
pub const SLICE_S: f64 = 0.02;

/// The quietest level the meters show, in dBFS: below it a bar is empty.
pub const FLOOR_DB: f32 = -60.0;

/// The loudest sample of each channel in each slice of a file (`0.0..=1.0`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Envelope {
    /// One `[left, right]` peak per [`SLICE_S`] of the file, from its start; a mono file has
    /// the same value in both.
    pub peaks: Vec<[f32; 2]>,
}

impl Envelope {
    /// The peaks at `seconds` into the file: silence before its start and after its end.
    #[must_use]
    pub fn at(&self, seconds: f64) -> [f32; 2] {
        if !(seconds >= 0.0) {
            return [0.0; 2];
        }
        // A hair over, so a time on a slice's edge (1.2 s is 59.99... slices) is in it.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = (seconds / SLICE_S + 1e-6).floor() as usize;
        self.peaks.get(index).copied().unwrap_or([0.0; 2])
    }
}

/// Builds an [`Envelope`] from decoded audio, one decoded frame after the other.
#[derive(Debug, Default)]
pub struct EnvelopeBuilder {
    /// The finished slices.
    peaks: Vec<[f32; 2]>,
    /// The peaks of the slice being filled.
    current: [f32; 2],
    /// How many sample frames the slice being filled has (a slice may span two pushes).
    filled: usize,
}

impl EnvelopeBuilder {
    /// Adds interleaved `samples` of `channels` channels at `rate` Hz. The even channels count
    /// as left and the odd ones as right; one channel counts as both.
    pub fn push(&mut self, rate: u32, channels: u16, samples: &[f32]) {
        let channels = usize::from(channels.max(1));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let per_slice = ((f64::from(rate.max(1)) * SLICE_S).round() as usize).max(1);
        for frame in samples.chunks(channels) {
            for (channel, sample) in frame.iter().enumerate() {
                let level = if sample.is_finite() { sample.abs().min(1.0) } else { 0.0 };
                if channels == 1 {
                    self.current = [self.current[0].max(level), self.current[1].max(level)];
                } else {
                    let side = channel % 2;
                    self.current[side] = self.current[side].max(level);
                }
            }
            self.filled += 1;
            if self.filled == per_slice {
                self.peaks.push(self.current);
                self.current = [0.0; 2];
                self.filled = 0;
            }
        }
    }

    /// The envelope; a last, shorter slice counts too.
    #[must_use]
    pub fn finish(mut self) -> Envelope {
        if self.filled > 0 {
            self.peaks.push(self.current);
        }
        Envelope { peaks: self.peaks }
    }
}

/// The envelope of what `decoder` holds, decoded from where it stands to its end (a worker's
/// job: a whole file).
#[must_use]
pub fn envelope_from(decoder: &mut AudioFileDecoder) -> Envelope {
    let mut builder = EnvelopeBuilder::default();
    while let Some(frame) = decoder.next_frame().into_option() {
        builder.push(frame.sample_rate, frame.channels, frame.samples.as_ref());
    }
    builder.finish()
}

/// What sounds at `frame`: for each enabled clip on an unmuted audio track, its media and the
/// second of the media's file it plays (media frames are sequence frames).
#[must_use]
pub fn sounding_at(project: &Project, frame: Frame) -> Vec<(u64, f64)> {
    let fps = f64::from(project.sequence.fps.max(1));
    project
        .sequence
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Audio && !track.hidden)
        .filter_map(|track| track.clip_at(frame))
        .filter(|clip| clip.enabled)
        .map(|clip| {
            #[allow(clippy::cast_precision_loss)]
            let seconds = clip.source_frame(frame) as f64 / fps;
            (clip.media, seconds)
        })
        .collect()
}

/// The meters' reading at `frame`: per channel the loudest of the sounding clips whose levels
/// are known; silence where nothing sounds.
#[must_use]
pub fn reading(project: &Project, frame: Frame, envelopes: &HashMap<u64, Envelope>) -> [f32; 2] {
    sounding_at(project, frame)
        .into_iter()
        .filter_map(|(media, seconds)| envelopes.get(&media).map(|e| e.at(seconds)))
        .fold([0.0; 2], |a, b| [a[0].max(b[0]), a[1].max(b[1])])
}

/// `level` in dBFS, or None for silence (under [`FLOOR_DB`]).
fn dbfs(level: f32) -> Option<f32> {
    if !(level > 0.0) {
        return None;
    }
    let db = 20.0 * level.log10();
    (db >= FLOOR_DB).then_some(db)
}

/// How full a meter's bar is for `level`: `0.0..=1.0`, even in decibels from the floor to
/// full scale (as a meter's scale is drawn).
#[must_use]
pub fn fill(level: f32) -> f32 {
    dbfs(level).map_or(0.0, |db| ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0))
}

/// The meters' number for `level`: whole dBFS, or None for silence.
#[must_use]
pub fn peak_db(level: f32) -> Option<i32> {
    #[allow(clippy::cast_possible_truncation)]
    dbfs(level).map(|db| db.round() as i32)
}

#[cfg(test)]
#[path = "levels_tests.rs"]
mod tests;
