//! Waveform widget - an audio file's loudness over time as bars, the part already played in the
//! accent, a click or a drag seeks.
//!
//! The bars are PEAKS: the loudest sample of each slice of the audio, `0.0..=1.0`. They come
//! from the decoder (`AudioFileDecoder::waveform` decodes a whole file into them on a `Thread`)
//! or from the app's own samples through [`WaveformPeaks`] and [`resample_peaks`] - the one pair
//! of helpers for the concern, which the decoder uses too.
//!
//! Key types: [`WaveformPeaks`], [`resample_peaks`].

use alloc::vec::Vec;

/// The peaks of a signal as it streams by: the loudest sample (of any channel) of every
/// `block_frames` frames. Feed it decoded audio with [`push`](Self::push), take the peaks with
/// [`finish`](Self::finish).
#[derive(Debug, Clone)]
pub struct WaveformPeaks {
    channels: usize,
    block: usize,
    in_block: usize,
    current: f32,
    peaks: Vec<f32>,
}

impl WaveformPeaks {
    /// Peaks of every `block_frames` frames of `channels` interleaved channels (both at least 1).
    #[must_use]
    pub fn new(channels: u16, block_frames: u32) -> Self {
        Self {
            channels: usize::from(channels.max(1)),
            block: block_frames.max(1) as usize,
            in_block: 0,
            current: 0.0,
            peaks: Vec::new(),
        }
    }

    /// More audio (interleaved; a frame may be split across calls).
    pub fn push(&mut self, samples: &[f32]) {
        // Counted in samples, so a frame split across two calls lands in its block.
        let block_samples = self.block * self.channels;
        for s in samples {
            let v = s.abs();
            if v > self.current {
                self.current = v;
            }
            self.in_block += 1;
            if self.in_block == block_samples {
                self.peaks.push(self.current.min(1.0));
                self.current = 0.0;
                self.in_block = 0;
            }
        }
    }

    /// The peaks, the last (partial) block included.
    #[must_use]
    pub fn finish(mut self) -> Vec<f32> {
        if self.in_block > 0 {
            self.peaks.push(self.current.min(1.0));
        }
        self.peaks
    }
}

/// `peaks` as `buckets` values: each bucket is the loudest of the peaks it covers (fewer peaks
/// than buckets stretch, each peak covering several buckets). All zero without peaks.
#[must_use]
pub fn resample_peaks(peaks: &[f32], buckets: usize) -> Vec<f32> {
    let len = peaks.len();
    if len == 0 {
        return alloc::vec![0.0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let start = (b * len / buckets).min(len - 1);
            let end = ((b + 1) * len / buckets).clamp(start + 1, len);
            peaks[start..end].iter().copied().fold(0.0f32, f32::max)
        })
        .collect()
}

#[cfg(test)]
mod peaks_tests {
    use super::*;

    fn tone(frames: usize, amp: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| amp * (core::f32::consts::TAU * i as f32 / 40.0).sin())
            .collect()
    }

    #[test]
    fn the_peaks_are_the_loudest_sample_of_each_block_of_any_channel() {
        let mut p = WaveformPeaks::new(2, 2);
        // Frames: (0.1, -0.5), (0.2, 0.0) | (0.9, 0.1), (-0.3, 0.0) | (0.05, 0.0)
        p.push(&[0.1, -0.5, 0.2, 0.0, 0.9]);
        p.push(&[0.1, -0.3, 0.0, 0.05, 0.0]);
        assert_eq!(p.finish(), vec![0.5, 0.9, 0.05]);
    }

    #[test]
    fn a_loud_half_and_a_quiet_half_stay_apart_at_any_bucket_count() {
        let mut p = WaveformPeaks::new(1, 1000);
        p.push(&tone(2000, 0.8));
        p.push(&tone(2000, 0.2));
        let peaks = p.finish();
        assert_eq!(peaks.len(), 4);
        let two = resample_peaks(&peaks, 2);
        assert!(
            (two[0] - 0.8).abs() < 1e-3 && (two[1] - 0.2).abs() < 1e-3,
            "{two:?}"
        );
        let eight = resample_peaks(&peaks, 8);
        assert_eq!(eight.len(), 8, "fewer peaks than buckets stretch");
        assert!(
            eight[..4].iter().all(|v| (v - 0.8).abs() < 1e-3),
            "{eight:?}"
        );
        assert!(
            eight[4..].iter().all(|v| (v - 0.2).abs() < 1e-3),
            "{eight:?}"
        );
        assert_eq!(resample_peaks(&[], 3), vec![0.0, 0.0, 0.0]);
        assert!(resample_peaks(&peaks, 0).is_empty());
    }
}
