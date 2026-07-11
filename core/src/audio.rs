//! POD types for the audio surface (SUPER_PLAN_2 §4 P7).
//!
//! Audio playback + microphone capture (rodio / cpal on the desktop;
//! AVAudioEngine / AAudio on mobile). Capture mirrors the sensor manager (the
//! backend pushes [`AudioFrame`]s to a process-global channel; the layout pass
//! drains them and a callback reads them); playback queues frames to the
//! backend. The mic permission is the existing
//! `azul_layout::managers::permission::Capability::Microphone`.
//!
//! Defined in `azul-core` so the config + frame types cross the FFI without
//! `azul-layout` (or rodio / cpal) as a dependency. For azul-meet (P8),
//! [`AudioFrame`] is the unit captured -> sent over UDP -> played back.

use azul_css::F32Vec;

/// Audio stream format.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioConfig {
    /// Samples per second per channel (e.g. 48000).
    pub sample_rate: u32,
    /// Channel count (1 = mono, 2 = stereo).
    pub channels: u16,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 1,
        }
    }
}

impl AudioConfig {
    /// A config with the given rate + channel count.
    #[must_use] pub const fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}

/// A chunk of audio - interleaved `f32` samples in `[-1.0, 1.0]`.
///
/// For stereo
/// the layout is `L, R, L, R, ...`. This is the unit the mic backend delivers,
/// playback consumes, and (P8) azul-meet sends over UDP.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    /// Samples per second per channel.
    pub sample_rate: u32,
    /// Channel count (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Interleaved `f32` samples.
    pub samples: F32Vec,
}

impl AudioFrame {
    /// Number of sample *frames* (samples per channel) in this chunk.
    #[must_use] pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.as_ref().len() / self.channels as usize
        }
    }
}

// FFI Option wrapper for accessors that may have no frame yet. `copy = false`
// because AudioFrame holds a F32Vec (matches the convention in `json.rs`).
impl_option!(AudioFrame, OptionAudioFrame, copy = false, [Clone, Debug]);

#[cfg(test)]
mod autotest_generated {
    use alloc::{vec, vec::Vec};

    use super::*;

    // --- helpers ---------------------------------------------------------

    /// Build an `AudioFrame` from a channel count + owned sample vec.
    /// `AudioFrame` derives no `Default`, so each test constructs explicitly.
    fn frame(sample_rate: u32, channels: u16, samples: Vec<f32>) -> AudioFrame {
        AudioFrame {
            sample_rate,
            channels,
            samples: F32Vec::from_vec(samples),
        }
    }

    // --- AudioConfig::new (constructor) ----------------------------------

    /// invariants_hold: fields match the exact args, for representative input.
    #[test]
    fn config_new_fields_match_args() {
        let c = AudioConfig::new(48_000, 2);
        assert_eq!(c.sample_rate, 48_000);
        assert_eq!(c.channels, 2);
    }

    /// no_panic: extreme integer bounds must not panic or wrap; fields are stored verbatim.
    #[test]
    fn config_new_extreme_args_no_panic() {
        let zero = AudioConfig::new(0, 0);
        assert_eq!(zero.sample_rate, 0);
        assert_eq!(zero.channels, 0);

        let max = AudioConfig::new(u32::MAX, u16::MAX);
        assert_eq!(max.sample_rate, u32::MAX);
        assert_eq!(max.channels, u16::MAX);

        // A sample_rate of 1 with a very large channel count is nonsensical but legal POD.
        let odd = AudioConfig::new(1, u16::MAX);
        assert_eq!(odd.sample_rate, 1);
        assert_eq!(odd.channels, u16::MAX);
    }

    /// invariants_hold: `const fn` is usable in a const context (compile-time evaluation).
    #[test]
    fn config_new_is_const_evaluable() {
        const C: AudioConfig = AudioConfig::new(44_100, 1);
        assert_eq!(C.sample_rate, 44_100);
        assert_eq!(C.channels, 1);
    }

    /// invariants_hold: the documented default equals the equivalent explicit construction.
    #[test]
    fn config_default_matches_new() {
        assert_eq!(AudioConfig::default(), AudioConfig::new(48_000, 1));
    }

    /// invariants_hold: `Copy`/`Eq`/`Hash` are mutually consistent (equal values hash equal).
    #[test]
    fn config_eq_hash_consistent() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let a = AudioConfig::new(96_000, 2);
        let b = a; // Copy
        assert_eq!(a, b);

        let mut ha = DefaultHasher::new();
        let mut hb = DefaultHasher::new();
        a.hash(&mut ha);
        b.hash(&mut hb);
        assert_eq!(ha.finish(), hb.finish());

        // Differing fields compare unequal.
        assert_ne!(AudioConfig::new(96_000, 2), AudioConfig::new(96_000, 1));
        assert_ne!(AudioConfig::new(96_000, 2), AudioConfig::new(48_000, 2));
    }

    // --- AudioFrame::frame_count (getter) --------------------------------

    /// basic_access: mono => frame_count == sample count.
    #[test]
    fn frame_count_mono_known_value() {
        let f = frame(48_000, 1, vec![0.0, 0.5, -0.5, 1.0]);
        assert_eq!(f.frame_count(), 4);
    }

    /// basic_access: stereo => frame_count == samples / 2.
    #[test]
    fn frame_count_stereo_known_value() {
        let f = frame(48_000, 2, vec![0.0, 0.5, -0.5, 1.0]);
        assert_eq!(f.frame_count(), 2);
    }

    /// edge_access: channels == 0 must hit the guard and return 0, NOT divide by zero.
    #[test]
    fn frame_count_zero_channels_no_div_by_zero() {
        let f = frame(48_000, 0, vec![0.0, 0.1, 0.2, 0.3, 0.4]);
        assert_eq!(f.frame_count(), 0);
    }

    /// edge_access: zero channels with zero samples is still 0 (guard short-circuits first).
    #[test]
    fn frame_count_zero_channels_empty_samples() {
        let f = frame(0, 0, Vec::new());
        assert_eq!(f.frame_count(), 0);
    }

    /// edge_access: empty sample buffer yields 0 frames for any nonzero channel count.
    #[test]
    fn frame_count_empty_samples() {
        assert_eq!(frame(48_000, 1, Vec::new()).frame_count(), 0);
        assert_eq!(frame(48_000, 2, Vec::new()).frame_count(), 0);
        assert_eq!(
            frame(48_000, u16::MAX, Vec::new()).frame_count(),
            0
        );
    }

    /// edge_access: a partial trailing frame is truncated by integer division (5 / 2 == 2).
    #[test]
    fn frame_count_truncates_partial_frame() {
        let f = frame(48_000, 2, vec![0.0, 0.1, 0.2, 0.3, 0.4]);
        assert_eq!(f.frame_count(), 2);
    }

    /// edge_access: channel count far exceeding the sample count yields 0, never underflow/panic.
    #[test]
    fn frame_count_channels_exceed_samples() {
        let f = frame(48_000, u16::MAX, vec![0.0, 1.0, -1.0]);
        assert_eq!(f.frame_count(), 0);
    }

    /// edge_access: exactly one full frame across the maximum channel count.
    #[test]
    fn frame_count_one_full_max_channel_frame() {
        let samples = vec![0.25_f32; u16::MAX as usize];
        let f = frame(48_000, u16::MAX, samples);
        assert_eq!(f.frame_count(), 1);
    }

    /// edge_access: non-finite sample values (NaN / +-inf) do not affect the length-only math.
    #[test]
    fn frame_count_ignores_non_finite_samples() {
        let f = frame(
            48_000,
            2,
            vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0],
        );
        assert_eq!(f.frame_count(), 2);
    }

    /// edge_access: a large buffer computes without overflow and matches the plain division.
    #[test]
    fn frame_count_large_buffer() {
        let n = 100_000usize;
        let f = frame(48_000, 2, vec![0.0_f32; n]);
        assert_eq!(f.frame_count(), n / 2);
    }

    // --- round-trip / trait invariants -----------------------------------

    /// round-trip: cloning an `AudioFrame` reproduces an equal value (finite samples).
    #[test]
    fn frame_clone_round_trip_eq() {
        let original = frame(44_100, 2, vec![0.0, 0.5, -0.5, 1.0, -1.0, 0.25]);
        let cloned = original.clone();
        assert_eq!(original, cloned);
        assert_eq!(original.frame_count(), cloned.frame_count());
        assert_eq!(original.samples.as_ref(), cloned.samples.as_ref());
    }

    /// round-trip: `Option<AudioFrame>` <-> `OptionAudioFrame` preserves the payload both ways.
    #[test]
    fn option_audio_frame_round_trip() {
        let f = frame(48_000, 1, vec![0.1, 0.2, 0.3]);

        let wrapped: OptionAudioFrame = Some(f.clone()).into();
        let unwrapped: Option<AudioFrame> = wrapped.into();
        assert_eq!(unwrapped, Some(f));

        let none_wrapped: OptionAudioFrame = None.into();
        let none_unwrapped: Option<AudioFrame> = none_wrapped.into();
        assert_eq!(none_unwrapped, None);
    }

    /// numeric edge: `PartialEq` follows IEEE-754 — a NaN sample makes a frame unequal to its clone.
    #[test]
    fn frame_with_nan_is_not_self_equal() {
        let f = frame(48_000, 1, vec![f32::NAN]);
        // NaN != NaN, so PartialEq on the sample vec must report inequality.
        assert_ne!(f, f.clone());
        // ...yet the length-based getter is unaffected.
        assert_eq!(f.frame_count(), 1);
    }

    // --- AudioConfig: FFI / POD layout + numeric invariants --------------

    /// invariants_hold: `#[repr(C)]` layout is the one the FFI headers assume
    /// (u32 + u16 + 2 bytes tail padding, 4-byte aligned). A silent layout
    /// change here would corrupt every cross-language `AudioConfig`.
    #[test]
    fn config_repr_c_layout_is_stable() {
        assert_eq!(core::mem::size_of::<AudioConfig>(), 8);
        assert_eq!(core::mem::align_of::<AudioConfig>(), 4);
    }

    /// numeric: no field is truncated, sign-extended or swapped for any
    /// boundary combination — `new` must store both args verbatim.
    #[test]
    fn config_new_no_field_truncation_sweep() {
        const RATES: [u32; 8] = [
            0,
            1,
            8_000,
            44_100,
            48_000,
            192_000,
            u16::MAX as u32, // would alias `channels` if the fields were swapped
            u32::MAX,
        ];
        const CHANNELS: [u16; 6] = [0, 1, 2, 8, 255, u16::MAX];

        for &rate in &RATES {
            for &ch in &CHANNELS {
                let c = AudioConfig::new(rate, ch);
                assert_eq!(c.sample_rate, rate, "rate {rate} ch {ch}");
                assert_eq!(c.channels, ch, "rate {rate} ch {ch}");
                // Round-trip through the struct must be lossless.
                assert_eq!(c, AudioConfig::new(c.sample_rate, c.channels));
            }
        }
    }

    /// invariants_hold: `new` performs no normalization — a nonsense config is
    /// stored as given and is NOT silently coerced to the default.
    #[test]
    fn config_new_does_not_normalize() {
        let c = AudioConfig::new(0, 0);
        assert_ne!(c, AudioConfig::default());
        assert_eq!(c.sample_rate, 0);
        assert_eq!(c.channels, 0);
    }

    /// invariants_hold: `Copy` gives an independent value — mutating the copy
    /// must not write through to the original.
    #[test]
    fn config_copy_is_independent() {
        let original = AudioConfig::new(48_000, 2);
        let mut copy = original;
        copy.sample_rate = 8_000;
        copy.channels = 1;
        assert_eq!(original.sample_rate, 48_000);
        assert_eq!(original.channels, 2);
        assert_eq!(copy, AudioConfig::new(8_000, 1));
    }

    /// invariants_hold: equal configs dedup in a hash set, distinct ones do not
    /// (Eq/Hash agreement under a real hasher, not just `DefaultHasher`).
    #[test]
    fn config_hash_set_dedup() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        assert!(set.insert(AudioConfig::new(48_000, 2)));
        assert!(!set.insert(AudioConfig::new(48_000, 2))); // equal -> dedup
        assert!(set.insert(AudioConfig::new(48_000, 1))); // channels differ
        assert!(set.insert(AudioConfig::new(44_100, 2))); // rate differs
        assert!(set.insert(AudioConfig::new(u32::MAX, u16::MAX)));
        assert_eq!(set.len(), 4);
        assert!(set.contains(&AudioConfig::new(48_000, 2)));
    }

    // --- AudioFrame::frame_count: exhaustive invariant sweep --------------

    /// invariants_hold: for every (channels, sample-count) pair,
    /// `frame_count()` equals the integer division, never panics, and satisfies
    /// `frame_count * channels <= len` with `len - frame_count * channels < channels`.
    #[test]
    fn frame_count_division_invariant_sweep() {
        const CHANNELS: [u16; 9] = [0, 1, 2, 3, 5, 7, 8, 255, u16::MAX];

        for &ch in &CHANNELS {
            for len in 0..24usize {
                let f = frame(48_000, ch, vec![0.5_f32; len]);
                let fc = f.frame_count();

                if ch == 0 {
                    // Guarded: must return 0 rather than divide by zero.
                    assert_eq!(fc, 0, "ch=0 len={len}");
                    continue;
                }

                let ch_us = ch as usize;
                assert_eq!(fc, len / ch_us, "ch={ch} len={len}");
                // No over-counting: the reported frames must fit in the buffer.
                let consumed = fc.checked_mul(ch_us).expect("frame_count * channels overflowed");
                assert!(consumed <= len, "ch={ch} len={len} consumed={consumed}");
                // ...and at most one partial frame may be left over.
                assert!(len - consumed < ch_us, "ch={ch} len={len}");
            }
        }
    }

    /// edge_access: the boundary around one full max-channel frame —
    /// one sample short is 0 frames, one sample over is still 1 (truncation).
    #[test]
    fn frame_count_max_channel_boundaries() {
        let n = u16::MAX as usize;
        assert_eq!(frame(48_000, u16::MAX, vec![0.0; n - 1]).frame_count(), 0);
        assert_eq!(frame(48_000, u16::MAX, vec![0.0; n]).frame_count(), 1);
        assert_eq!(frame(48_000, u16::MAX, vec![0.0; n + 1]).frame_count(), 1);
        assert_eq!(frame(48_000, u16::MAX, vec![0.0; 2 * n]).frame_count(), 2);
        assert_eq!(
            frame(48_000, u16::MAX, vec![0.0; 2 * n - 1]).frame_count(),
            1
        );
    }

    /// invariants_hold: `sample_rate` is not an input to `frame_count` — the
    /// extremes must not change the result.
    #[test]
    fn frame_count_ignores_sample_rate() {
        let samples = vec![0.0, 0.1, 0.2, 0.3];
        for &rate in &[0u32, 1, 48_000, u32::MAX] {
            assert_eq!(frame(rate, 2, samples.clone()).frame_count(), 2, "rate={rate}");
        }
    }

    /// invariants_hold: `frame_count` is a pure read — repeated calls agree and
    /// the sample buffer is untouched.
    #[test]
    fn frame_count_is_pure() {
        let f = frame(48_000, 2, vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5]);
        let before: Vec<f32> = f.samples.as_ref().to_vec();
        let first = f.frame_count();
        let second = f.frame_count();
        let third = f.frame_count();
        assert_eq!(first, 3);
        assert_eq!(first, second);
        assert_eq!(second, third);
        assert_eq!(f.samples.as_ref(), before.as_slice());
        assert_eq!(f.samples.len(), 6);
    }

    /// edge_access: a frame whose `F32Vec` is backed by a `'static` slice
    /// (no heap allocation, `NoDestructor`) must count + drop cleanly.
    #[test]
    fn frame_count_static_backed_samples() {
        static SAMPLES: [f32; 6] = [0.0, 0.5, -0.5, 1.0, -1.0, 0.25];

        let f = AudioFrame {
            sample_rate: 48_000,
            channels: 2,
            samples: F32Vec::from_const_slice(&SAMPLES),
        };
        assert_eq!(f.frame_count(), 3);
        assert_eq!(f.samples.as_ref(), &SAMPLES[..]);

        // Cloning a static-backed vec must not alias the static into a heap free.
        let cloned = f.clone();
        assert_eq!(cloned, f);
        drop(f);
        assert_eq!(cloned.frame_count(), 3);
    }

    /// edge_access: an empty `F32Vec::new()` (null-ptr, zero-cap vec) is a valid
    /// zero-frame buffer and must not panic on read, clone or drop.
    #[test]
    fn frame_count_default_f32vec_is_empty() {
        let f = AudioFrame {
            sample_rate: 48_000,
            channels: 2,
            samples: F32Vec::new(),
        };
        assert_eq!(f.frame_count(), 0);
        assert!(f.samples.is_empty());
        assert_eq!(f.samples.len(), 0);
        assert_eq!(f.samples.as_ref(), &[] as &[f32]);
        assert_eq!(f.clone(), f);
    }

    /// invariants_hold: building the sample vec through `FromIterator` yields the
    /// same frame count as `from_vec`.
    #[test]
    fn frame_count_from_iterator_matches_from_vec() {
        let via_iter = AudioFrame {
            sample_rate: 48_000,
            channels: 2,
            samples: (0..10).map(|i| i as f32).collect::<F32Vec>(),
        };
        let via_vec = frame(48_000, 2, (0..10).map(|i| i as f32).collect::<Vec<f32>>());
        assert_eq!(via_iter, via_vec);
        assert_eq!(via_iter.frame_count(), 5);
        assert_eq!(via_iter.frame_count(), via_vec.frame_count());
    }

    /// edge_access: out-of-bounds sample reads return `None` rather than panicking,
    /// including on the last valid index of the final counted frame.
    #[test]
    fn frame_sample_access_is_bounds_checked() {
        let f = frame(48_000, 2, vec![0.0, 0.1, 0.2, 0.3]);
        let last = f.frame_count() * f.channels as usize - 1;
        assert_eq!(f.samples.get(last), Some(&0.3));
        assert_eq!(f.samples.get(f.samples.len()), None);
        assert_eq!(f.samples.get(usize::MAX), None);
    }

    /// numeric: extreme / non-finite / signed-zero samples survive a clone
    /// bit-for-bit, and none of them perturb the length-only getter.
    #[test]
    fn frame_extreme_sample_values_preserved_bitwise() {
        let raw = vec![
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            0.0,
            -0.0,
            f32::EPSILON,
            1e-45, // subnormal
        ];
        let f = frame(48_000, 2, raw.clone());
        assert_eq!(f.frame_count(), 4);

        let cloned = f.clone();
        let got: Vec<u32> = cloned.samples.as_ref().iter().map(|s| s.to_bits()).collect();
        let want: Vec<u32> = raw.iter().map(|s| s.to_bits()).collect();
        assert_eq!(got, want, "clone must be bit-exact (incl. -0.0 and subnormals)");

        // Sanity: -0.0 == 0.0 under PartialEq but their bits differ, so a
        // value-level compare still says equal while the bit compare above is strict.
        assert_eq!(cloned, f);
        assert_ne!(0.0_f32.to_bits(), (-0.0_f32).to_bits());
    }

    /// invariants_hold: `PartialEq` on `AudioFrame` discriminates the header
    /// fields, not just the samples.
    #[test]
    fn frame_eq_discriminates_header_fields() {
        let samples = vec![0.0, 0.5, -0.5, 1.0];
        let base = frame(48_000, 2, samples.clone());
        assert_eq!(base, frame(48_000, 2, samples.clone()));
        assert_ne!(base, frame(44_100, 2, samples.clone()));
        assert_ne!(base, frame(48_000, 1, samples.clone()));
        assert_ne!(base, frame(48_000, 2, vec![0.0, 0.5, -0.5]));
        // Same frame_count (2) via different (channels, len) pairs is still not equal.
        assert_eq!(frame(48_000, 1, vec![0.0, 0.5]).frame_count(), 2);
        assert_ne!(base, frame(48_000, 1, vec![0.0, 0.5]));
    }

    /// edge_access: repeated clone/drop cycles of a heap-backed frame must not
    /// double-free or corrupt the buffer (FFI vec destructor regression guard).
    #[test]
    fn frame_repeated_clone_drop_is_stable() {
        let original = frame(48_000, 2, (0..64).map(|i| i as f32).collect::<Vec<f32>>());
        for _ in 0..64 {
            let c = original.clone();
            assert_eq!(c.frame_count(), 32);
            assert_eq!(c.samples.as_ref(), original.samples.as_ref());
            drop(c);
        }
        // The original survived every clone + drop.
        assert_eq!(original.frame_count(), 32);
        assert_eq!(original.samples.len(), 64);
    }

    // --- OptionAudioFrame (FFI option wrapper) ----------------------------

    /// invariants_hold: the FFI option defaults to `None` and its predicates agree.
    #[test]
    fn option_audio_frame_predicates() {
        let none = OptionAudioFrame::default();
        assert!(none.is_none());
        assert!(!none.is_some());
        assert!(none.as_ref().is_none());
        assert!(none.as_option().is_none());

        let some: OptionAudioFrame = Some(frame(48_000, 2, vec![0.0, 1.0])).into();
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_ref().map(AudioFrame::frame_count), Some(1));
    }

    /// edge_access: `into_option` clones out of a `&self`, so calling it twice must
    /// yield two equal, independently-owned frames (no move-out / double-free).
    #[test]
    fn option_audio_frame_into_option_twice() {
        let wrapped: OptionAudioFrame = Some(frame(48_000, 2, vec![0.0, 0.5, -0.5, 1.0])).into();
        let a = wrapped.into_option().expect("some");
        let b = wrapped.into_option().expect("some");
        assert_eq!(a, b);
        assert_eq!(a.frame_count(), 2);
        drop(a);
        // `b` and `wrapped` must still be intact after `a` is dropped.
        assert_eq!(b.frame_count(), 2);
        assert!(wrapped.is_some());
    }

    /// invariants_hold: `replace` returns the previous value (mem::replace semantics)
    /// and installs the new one.
    #[test]
    fn option_audio_frame_replace_returns_previous() {
        let mut slot = OptionAudioFrame::None;
        let prev = slot.replace(frame(48_000, 1, vec![0.0, 0.1]));
        assert!(prev.is_none());
        assert_eq!(slot.as_ref().map(AudioFrame::frame_count), Some(2));

        let prev = slot.replace(frame(48_000, 2, vec![0.0, 0.1, 0.2, 0.3]));
        assert_eq!(prev.as_ref().map(AudioFrame::frame_count), Some(2));
        assert_eq!(slot.as_ref().map(AudioFrame::frame_count), Some(2));
        assert_eq!(slot.as_ref().map(|f| f.channels), Some(2));
    }

    /// edge_access: `as_mut` hands out a live borrow — mutating the samples through it
    /// must be visible in the wrapper's `frame_count`.
    #[test]
    fn option_audio_frame_as_mut_mutation_visible() {
        let mut slot: OptionAudioFrame = Some(frame(48_000, 2, vec![0.0, 0.1, 0.2, 0.3])).into();
        assert_eq!(slot.as_ref().map(AudioFrame::frame_count), Some(2));

        if let Some(f) = slot.as_mut() {
            f.channels = 0; // divide-by-zero guard must engage through the wrapper too
        }
        assert_eq!(slot.as_ref().map(AudioFrame::frame_count), Some(0));

        if let Some(f) = slot.as_mut() {
            f.channels = 1;
        }
        assert_eq!(slot.as_ref().map(AudioFrame::frame_count), Some(4));
    }

    /// round-trip: an empty-sample frame survives the `Option` <-> FFI-option
    /// round-trip (the zero-length vec is the likeliest null-ptr edge).
    #[test]
    fn option_audio_frame_round_trip_empty_samples() {
        let f = frame(48_000, 2, Vec::new());
        let wrapped: OptionAudioFrame = Some(f.clone()).into();
        let back: Option<AudioFrame> = wrapped.into();
        let back = back.expect("some");
        assert_eq!(back, f);
        assert_eq!(back.frame_count(), 0);
        assert!(back.samples.is_empty());
    }
}
