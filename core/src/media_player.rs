//! Media playback POD types - the state the six media events describe.
//!
//! Stateful manager lives in `azul_layout::managers::media_player`; this is
//! only the value a callback reads back through
//! `CallbackInfo::get_media_state`.
//!
//! `EventType::{Play, Pause, Ended, TimeUpdate, VolumeChange, MediaError}`
//! shipped with no player behind them - the unified layer has a decoder, an
//! encoder and an audio sink, but no transport, no position, no duration and
//! no volume - so the six events described a state machine that did not
//! exist and dispatched to nothing. This struct is that state (11c).

/// What a media node's playback looks like right now.
///
/// Field order is by descending alignment (the `f32`s, then the `bool`s):
/// the repo's alignment-order check is a hard error, and a `bool` wedged
/// between two `f32`s is what trips it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackState {
    /// Playback position, in seconds from the start.
    pub position_s: f32,
    /// Total length in seconds, or `0.0` when it is not known yet (a live
    /// stream, or metadata that has not arrived). An unknown duration means
    /// playback never reaches an end, so `Ended` cannot fire.
    pub duration_s: f32,
    /// Output gain, `0.0..=1.0`.
    pub volume: f32,
    /// Whether the transport is running.
    pub playing: bool,
    /// Muted independently of `volume`, exactly as the web models it - so
    /// unmuting restores the level the user had chosen.
    pub muted: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position_s: 0.0,
            duration_s: 0.0,
            // Full volume, unmuted: the state a `<video>` starts in.
            volume: 1.0,
            playing: false,
            muted: false,
        }
    }
}

impl PlaybackState {
    /// How far through the media we are, `0.0..=1.0`, or `None` while the
    /// duration is unknown - a progress bar with no known length is a real
    /// state, not a zero.
    #[must_use]
    pub fn progress(&self) -> Option<f32> {
        if self.duration_s > 0.0 {
            Some((self.position_s / self.duration_s).clamp(0.0, 1.0))
        } else {
            None
        }
    }

    /// The gain that actually reaches the sink: `0.0` while muted.
    #[must_use]
    pub fn effective_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }
}

// FFI Option wrapper for `CallbackInfo::get_media_state(node) ->
// Option<PlaybackState>` (mirrors `OptionSensorReading`). `None` means "this
// node is not a media node", which is a different answer from a default
// state and the app can act on the difference.
impl_option!(
    PlaybackState,
    OptionPlaybackState,
    [Debug, Clone, Copy, PartialEq]
);
