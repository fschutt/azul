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
    #[must_use]
    pub const fn new(sample_rate: u32, channels: u16) -> Self {
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
    #[must_use]
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

// ==== Audio files and the player (MEDIA9) ====

use azul_css::{AzString, U8Vec};

/// What an audio file holds: its format, length and tags. Every text is empty when the file does
/// not say; numbers are 0.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AudioFileInfo {
    /// The track's title (`TITLE`, ID3 `TIT2`, the MP4 title atom).
    pub title: AzString,
    /// The performer.
    pub artist: AzString,
    /// The album.
    pub album: AzString,
    /// The album's artist (a compilation's "Various Artists").
    pub album_artist: AzString,
    /// The genre.
    pub genre: AzString,
    /// The date or year the file gives, as it gives it ("2024", "2024-05-01").
    pub date: AzString,
    /// Unsynchronised lyrics, when the file carries them.
    pub lyrics: AzString,
    /// The codec ("mp3", "aac", "flac", "vorbis", "opus", "`pcm_s16le`", ...).
    pub codec: AzString,
    /// The container ("wave", "flac", "isomp4", "ogg", "mkv", ...).
    pub container: AzString,
    /// The cover art's media type ("image/jpeg", "image/png"), empty without a cover.
    pub cover_mime: AzString,
    /// The cover art, encoded as stored (decode it with `RawImage::decode_image_bytes_any`);
    /// empty without a cover. The front cover when the file has several pictures.
    pub cover: U8Vec,
    /// The length in seconds (0 when the file does not say and it was not measured).
    pub duration_s: f64,
    /// Samples per second per channel.
    pub sample_rate: u32,
    /// The track's number on its album (0 = not given).
    pub track_number: u32,
    /// The track count of the album (0 = not given).
    pub track_total: u32,
    /// The disc's number (0 = not given).
    pub disc_number: u32,
    /// Channels (1 mono, 2 stereo, ...).
    pub channels: u16,
}

impl_option!(
    AudioFileInfo,
    OptionAudioFileInfo,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// What an `AudioPlayer` is doing, as the listener hears it (`AudioPlayer::get_state`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioPlayerState {
    /// Where the listener is in the track heard now, in seconds.
    pub position_s: f64,
    /// The length of the track heard now, in seconds (0 when unknown).
    pub duration_s: f64,
    /// Decoded audio waiting ahead of the listener, in seconds. A track handed over with
    /// `AudioPlayer::preload_file` is ready to start at once (`play`) when this is above zero:
    /// the file is open, the output is open, and its first samples are decoded.
    pub buffered_s: f64,
    /// The id of the track heard now (what `load_*` / `queue_*` returned); 0 = none.
    pub track: u64,
    /// The id of the last track that could not be opened (0 = none): its reason is
    /// `AudioPlayer::error_message`.
    pub failed_track: u64,
    /// The player's volume, `0.0..=1.0`.
    pub volume: f32,
    /// The peak level (`0.0..=1.0`) of the left channel of the audio heard now (a meter's input).
    pub peak_left: f32,
    /// The peak level of the right channel.
    pub peak_right: f32,
    /// Tracks queued after the one being decoded.
    pub queued_tracks: u32,
    /// Playback runs (not paused, something to play, an output open).
    pub playing: bool,
    /// Everything loaded and queued has been heard to its end.
    pub finished: bool,
    /// An audio output opened: false in a headless run without the synthetic sink, and on a
    /// machine without an output device (`AudioPlayer::error_message` says why).
    pub has_output: bool,
}

#[cfg(test)]
#[path = "audio_test.rs"]
mod audio_test;
