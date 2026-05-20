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
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}

/// A chunk of audio - interleaved `f32` samples in `[-1.0, 1.0]`. For stereo
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
    pub fn frame_count(&self) -> usize {
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
