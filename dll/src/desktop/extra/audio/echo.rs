//! Acoustic echo cancellation - the `EchoCanceller` handle.
//!
//! In a call without headphones the microphone hears the speaker: every peer hears itself a
//! moment later. An echo canceller learns the path from what is played (the FAR end) to what
//! the microphone captures (the NEAR end) and subtracts its estimate of the echo from the
//! microphone, so only the person in the room is sent.
//!
//! Pure Rust, no platform API: one adaptive filter design for every target, tested here on
//! synthetic rooms.
//!
//! # Design
//!
//! A partitioned-block frequency-domain adaptive filter (overlap-save, the MDF of Soo & Pang as
//! Speex uses it): blocks of [`block_len`] samples, an FFT twice that long, as many partitions
//! as the echo tail needs. The step is normalized per frequency bin by the far end's power over
//! all partitions (frequency-domain NLMS); one partition per block gets the gradient constraint
//! (round robin), the rest adapt unconstrained.
//!
//! Double talk (the person in the room speaks while the far end plays) is what breaks a single
//! adaptive filter: it adapts on the near-end voice and smears it. So there are two filters, as
//! in Speex: a BACKGROUND filter adapts every block; the FOREGROUND filter makes the output and
//! takes the background's coefficients only when the background cancels significantly better
//! (the improvement must stand out against how far the two filters' outputs differ - a near-end
//! voice raises both filters' errors alike); a background that became significantly worse is
//! reset to the foreground. Should the foreground's output ever be louder than the microphone,
//! the microphone passes unchanged.
//!
//! Limits: a pure tone at the near end while the far end plays can be partly learned (an
//! adaptive filter notches a steady sinusoid); no residual-echo suppressor or comfort noise yet;
//! one rate for both ends (no resampler).

use core::ffi::c_void;

use azul_core::audio::AudioFrame;
use azul_css::F32Vec;

/// The echo tail an app should ask for when it does not know better (ms): a laptop's speaker to
/// its microphone, the room, and the audio buffers of both sides.
pub const DEFAULT_TAIL_MS: u32 = 300;

/// Samples in one block at `sample_rate`: 256 (5.3 ms at 48 kHz), 128 at 24 kHz and below.
pub fn block_len(sample_rate: u32) -> usize {
    if sample_rate <= 24_000 {
        128
    } else {
        256
    }
}

/// An echo canceller handle: feed it what is played ([`far_end`](Self::far_end)) and pass the
/// microphone through [`process`](Self::process).
#[repr(C)]
pub struct EchoCanceller {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for EchoCanceller {
    fn clone(&self) -> Self {
        EchoCanceller {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for EchoCanceller {
    fn default() -> Self {
        EchoCanceller {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl EchoCanceller {
    /// A canceller for mono audio at `sample_rate` that removes echoes arriving up to `tail_ms`
    /// after the sound was played (the audio buffers' delay plus the room's reverberation;
    /// [`DEFAULT_TAIL_MS`] when unsure). Closed for a zero rate or tail.
    pub fn create(sample_rate: u32, tail_ms: u32) -> EchoCanceller {
        // RED stub: no canceller yet.
        let _ = (sample_rate, tail_ms);
        EchoCanceller::default()
    }

    /// Whether the canceller opened.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// The far end: `frame` is about to be played (mono, at the canceller's rate). Feed every
    /// frame that goes to the speaker, in order, when it is handed to the output. False when the
    /// canceller is not open or the frame is in another format.
    pub fn far_end(&self, frame: AudioFrame) -> bool {
        let _ = frame;
        false
    }

    /// The near end: `frame` as the microphone captured it (mono, at the canceller's rate),
    /// returned with the echo of the far end removed - as many samples as came in, [`latency`]
    /// samples later (one block). A frame in another format, or a closed canceller, returns
    /// the frame unchanged.
    ///
    /// [`latency`]: Self::latency_samples
    pub fn process(&self, frame: AudioFrame) -> AudioFrame {
        frame
    }

    /// How many samples `process` delays the microphone by (one block).
    pub fn latency_samples(&self) -> u32 {
        0
    }

    /// How much of the echo is removed right now, in dB (echo return loss enhancement, over the
    /// last half second the far end played); 0 while nothing played.
    pub fn erle_db(&self) -> f32 {
        0.0
    }

    /// Release the canceller. (Drop does this too.)
    pub fn close(&mut self) {
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for EchoCanceller {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod echo_tests {
    use azul_core::audio::AudioFrame;
    use azul_css::F32Vec;

    use super::{block_len, EchoCanceller};

    const RATE: u32 = 48_000;

    /// Deterministic white noise in -1..1.
    fn noise(seed: u32, n: usize, amplitude: f32) -> Vec<f32> {
        let mut s = seed.max(1);
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
            })
            .collect()
    }

    /// A room: `delay_ms` of silence, then `taps_ms` of exponentially decaying random taps
    /// (decay constant `decay_ms`), scaled to an L2 norm of `gain`.
    fn room(seed: u32, delay_ms: u32, taps_ms: u32, decay_ms: f32, gain: f32) -> Vec<f32> {
        let delay = (RATE * delay_ms / 1000) as usize;
        let taps = (RATE * taps_ms / 1000) as usize;
        let random = noise(seed, taps, 0.6);
        let mut h = vec![0.0f32; delay + taps];
        for (i, r) in random.iter().enumerate() {
            h[delay + i] = r * (-(i as f32) / (RATE as f32 * decay_ms / 1000.0)).exp();
        }
        let norm = h.iter().map(|v| v * v).sum::<f32>().sqrt();
        h.iter().map(|v| v * gain / norm).collect()
    }

    fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
        let taps: Vec<(usize, f32)> = h
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0.0)
            .map(|(i, v)| (i, *v))
            .collect();
        (0..x.len())
            .map(|n| {
                taps.iter()
                    .take_while(|(i, _)| *i <= n)
                    .map(|(i, v)| v * x[n - i])
                    .sum()
            })
            .collect()
    }

    /// Speech-like near-end talk: noise under a 4 Hz syllable envelope.
    fn talk(n: usize) -> Vec<f32> {
        noise(999, n, 0.25)
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                let envelope =
                    0.5 + 0.5 * (2.0 * core::f32::consts::PI * 4.0 * i as f32 / RATE as f32).sin();
                v * envelope
            })
            .collect()
    }

    fn power_db(x: &[f32]) -> f32 {
        let p = x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32;
        10.0 * (p + 1e-20).log10()
    }

    fn mono(samples: Vec<f32>) -> AudioFrame {
        AudioFrame {
            sample_rate: RATE,
            channels: 1,
            samples: F32Vec::from_vec(samples),
        }
    }

    /// Runs a call: `far` is played (fed 20 ms at a time), `near` is what the microphone
    /// captured; returns the canceller's output, lined up with `near` (its latency removed).
    fn run(aec: &EchoCanceller, far: &[f32], near: &[f32]) -> Vec<f32> {
        let chunk = (RATE / 50) as usize;
        let mut out = Vec::with_capacity(near.len());
        for (f, n) in far.chunks(chunk).zip(near.chunks(chunk)) {
            assert!(
                aec.far_end(mono(f.to_vec())),
                "an open canceller takes the far end"
            );
            let processed = aec.process(mono(n.to_vec()));
            assert_eq!(
                processed.samples.as_ref().len(),
                n.len(),
                "as many samples as came in"
            );
            out.extend_from_slice(processed.samples.as_ref());
        }
        let latency = aec.latency_samples() as usize;
        out.drain(..latency.min(out.len()));
        out
    }

    /// The echo of white noise through a room (20 ms away, 80 ms of reverberation, 6 dB down):
    /// after three seconds of learning, the fourth second is at least 25 dB quieter than the
    /// microphone heard it.
    #[test]
    fn a_converged_canceller_removes_the_echo_of_the_far_end() {
        let aec = EchoCanceller::create(RATE, 200);
        assert!(aec.is_open());
        let n = RATE as usize * 4;
        let far = noise(12345, n, 0.3);
        let near = convolve(&far, &room(7, 20, 80, 20.0, 0.5));
        let out = run(&aec, &far, &near);
        let last = (RATE as usize * 3)..out.len();
        let erle = power_db(&near[last.clone()]) - power_db(&out[last]);
        assert!(
            erle >= 25.0,
            "the echo is only {erle:.1} dB quieter after three seconds"
        );
        assert!(
            aec.erle_db() >= 20.0,
            "the canceller reports {:.1} dB",
            aec.erle_db()
        );
    }

    /// The person in the room starts speaking while the far end plays (double talk): their
    /// voice goes out as they spoke it, and the echo stays cancelled.
    #[test]
    fn near_end_talk_passes_through_while_the_echo_stays_cancelled() {
        let aec = EchoCanceller::create(RATE, 200);
        let n = RATE as usize * 6;
        let far = noise(12345, n, 0.3);
        let echo = convolve(&far, &room(7, 20, 80, 20.0, 0.5));
        let start = RATE as usize * 3;
        let mut voice = vec![0.0f32; n];
        voice[start..].copy_from_slice(&talk(n - start));
        let near: Vec<f32> = echo.iter().zip(&voice).map(|(e, v)| e + v).collect();
        let out = run(&aec, &far, &near);
        let during = (RATE as usize * 4)..out.len();
        let residual: Vec<f32> = during.clone().map(|i| out[i] - voice[i]).collect();
        let erle = power_db(&echo[during.clone()]) - power_db(&residual);
        assert!(
            erle >= 20.0,
            "while the near end talks the echo is only {erle:.1} dB down"
        );
        let kept = power_db(&out[during.clone()]) - power_db(&voice[during]);
        assert!(
            kept.abs() <= 1.0,
            "the near-end voice came out {kept:.2} dB off"
        );
    }

    /// The echo path changes (the laptop moved): the canceller learns the new one.
    #[test]
    fn a_new_echo_path_is_learned_again() {
        let aec = EchoCanceller::create(RATE, 200);
        let n = RATE as usize * 7;
        let far = noise(12345, n, 0.3);
        let first = convolve(&far, &room(7, 20, 80, 20.0, 0.5));
        let second = convolve(&far, &room(31337, 35, 60, 15.0, 0.7));
        let cut = RATE as usize * 3;
        let near: Vec<f32> = (0..n)
            .map(|i| if i < cut { first[i] } else { second[i] })
            .collect();
        let out = run(&aec, &far, &near);
        let last = (RATE as usize * 6)..out.len();
        let erle = power_db(&near[last.clone()]) - power_db(&out[last]);
        assert!(
            erle >= 20.0,
            "three seconds after the change the echo is {erle:.1} dB down"
        );
    }

    /// Nothing played: the microphone comes back as it was, one block later.
    #[test]
    fn without_a_far_end_the_microphone_passes_unchanged() {
        let aec = EchoCanceller::create(RATE, 200);
        assert_eq!(aec.latency_samples() as usize, block_len(RATE));
        let voice = talk(RATE as usize);
        let mut out = Vec::new();
        for chunk in voice.chunks(441) {
            out.extend_from_slice(aec.process(mono(chunk.to_vec())).samples.as_ref());
        }
        let latency = block_len(RATE);
        assert!(
            out[..latency].iter().all(|v| *v == 0.0),
            "the first block is the delay"
        );
        assert_eq!(&out[latency..], &voice[..voice.len() - latency]);
    }

    /// A closed canceller, or a frame in another format, changes nothing.
    #[test]
    fn a_closed_canceller_or_another_format_changes_nothing() {
        let closed = EchoCanceller::default();
        assert!(!closed.is_open());
        let frame = mono(vec![0.5; 960]);
        assert!(!closed.far_end(frame.clone()));
        assert_eq!(closed.process(frame.clone()), frame);
        let aec = EchoCanceller::create(RATE, 200);
        let mut stereo = mono(vec![0.25; 960]);
        stereo.channels = 2;
        assert!(!aec.far_end(stereo.clone()));
        assert_eq!(aec.process(stereo.clone()), stereo);
        let mut other_rate = mono(vec![0.25; 441]);
        other_rate.sample_rate = 44_100;
        assert_eq!(aec.process(other_rate.clone()), other_rate);
        assert!(!EchoCanceller::create(0, 200).is_open());
        assert!(!EchoCanceller::create(RATE, 0).is_open());
    }
}
