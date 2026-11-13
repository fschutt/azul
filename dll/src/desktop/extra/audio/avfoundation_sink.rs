//! Apple (iOS + macOS) audio playback via objc2 AVAudioEngine (cpal can't
//! cross-compile to iOS/macOS without SDK headers). An `AVAudioPlayerNode`
//! feeds the engine's main mixer; `play` deinterleaves each interleaved-f32
//! frame into an `AVAudioPCMBuffer` and schedules it. Counterpart to
//! `avfoundation_mic` (capture).
//!
//! Two correctness constraints this file upholds:
//!
//! * `AVAudioPlayerNode` requires the **standard** (deinterleaved float) format — scheduling
//!   interleaved f32 buffers is a documented crash / silent-failure class, so `open` builds the
//!   format with `initStandardFormatWithSampleRate:channels:` and `play` does the strided
//!   interleaved→planar copy into `floatChannelData`.
//! * Scheduled buffers must be **bounded**: `scheduleBuffer:completionHandler:` queues without
//!   limit, so if frames arrive faster than realtime the backlog (and memory) grows forever. An
//!   `Arc<AtomicUsize>` counts in-flight buffers (incremented before scheduling, decremented in the
//!   block2 completion handler — same `RcBlock` pattern as `extra/biometric/apple.rs`); past
//!   `MAX_IN_FLIGHT` the frame is dropped.

use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc, Once,
};

use block2::RcBlock;
use objc2::{rc::Retained, AllocAnyThread};
use objc2_avf_audio::{AVAudioEngine, AVAudioFormat, AVAudioPCMBuffer, AVAudioPlayerNode};

/// Max scheduled-but-unplayed buffers before `play` starts dropping frames.
/// ~8 × 20 ms UDP audio frames ≈ 160 ms of queued audio — enough slack to
/// absorb jitter without letting a fast producer run away from realtime.
const MAX_IN_FLIGHT: usize = 8;

/// An open AVAudioEngine playback graph for `AudioSink::play`.
pub(super) struct AvfSink {
    engine: Retained<AVAudioEngine>,
    player: Retained<AVAudioPlayerNode>,
    format: Retained<AVAudioFormat>,
    channels: u16,
    /// Buffers scheduled on the player node whose completion handler hasn't
    /// fired yet — the backpressure gauge for `play`.
    in_flight: Arc<AtomicUsize>,
    /// Sample frames in those buffers: what is queued and not heard yet
    /// (`OutputDevice::queued_frames`, a player's clock).
    frames_in_flight: Arc<AtomicU64>,
    /// Bumped by `clear`: the completion of a buffer scheduled before it
    /// (fired by the `stop` that dropped it) counts nothing, so the gauges
    /// reset to zero stay right.
    generation: Arc<AtomicU64>,
}

// Single-threaded use assumed (same assertion as the cpal/AAudio sinks).
unsafe impl Send for AvfSink {}
unsafe impl Sync for AvfSink {}

impl AvfSink {
    /// Build + start an engine with a player node connected to the main mixer,
    /// using the standard (deinterleaved Float32) format the player node
    /// requires, or a readable reason why not (note: the standard-format
    /// initializer rejects more than 2 channels).
    pub(super) fn open(rate: u32, channels: u16) -> Result<AvfSink, String> {
        let ch = channels.max(1) as u32;
        let sample_rate = if rate == 0 { 48_000.0 } else { rate as f64 };
        unsafe {
            let format = match AVAudioFormat::initStandardFormatWithSampleRate_channels(
                AVAudioFormat::alloc(),
                sample_rate,
                ch,
            ) {
                Some(f) => f,
                None => {
                    return Err(format!(
                        "AVAudioEngine cannot play {} Hz x {} channels (its player takes at most \
                         2 channels)",
                        sample_rate, ch
                    ));
                }
            };
            let engine = AVAudioEngine::new();
            let player = AVAudioPlayerNode::new();
            engine.attachNode(&player);
            let mixer = engine.mainMixerNode();
            engine.connect_to_format(&player, &mixer, Some(&format));
            engine.prepare();
            if engine.startAndReturnError().is_err() {
                return Err(String::from(
                    "AVAudioEngine did not start: no audio output device, or the device refused \
                     the format",
                ));
            }
            player.play();
            Ok(AvfSink {
                engine,
                player,
                format,
                channels: channels.max(1),
                in_flight: Arc::new(AtomicUsize::new(0)),
                frames_in_flight: Arc::new(AtomicU64::new(0)),
                generation: Arc::new(AtomicU64::new(0)),
            })
        }
    }
}

impl super::OutputDevice for AvfSink {
    /// Deinterleave `samples` (interleaved f32) into a standard-format PCM
    /// buffer + schedule it. Not taken (logged once) when more than
    /// `MAX_IN_FLIGHT` buffers are already queued on the player node, or when
    /// no buffer could be made for it.
    fn play(&self, samples: &[f32]) -> bool {
        let ch = self.channels.max(1) as usize;
        let frames = samples.len() / ch;
        if frames == 0 {
            return false;
        }
        // Backpressure: never let the scheduled backlog grow past the cap.
        if self.in_flight.load(Ordering::Acquire) >= MAX_IN_FLIGHT {
            static DROPPED: Once = Once::new();
            DROPPED.call_once(|| {
                crate::plog_warn!(
                    "[audio] sink backlog full ({} buffers in flight) - dropping frames (producer \
                     faster than realtime; logged once)",
                    MAX_IN_FLIGHT
                );
            });
            return false;
        }
        unsafe {
            let buf = match AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &self.format,
                frames as u32,
            ) {
                Some(b) => b,
                None => return false,
            };
            let data = buf.floatChannelData();
            if data.is_null() {
                return false;
            }
            // Standard format = deinterleaved: `data` is an array of `ch`
            // per-channel plane pointers. Strided copy interleaved → planar.
            let planes = std::slice::from_raw_parts(data, ch);
            for (c, plane) in planes.iter().enumerate() {
                let plane = plane.as_ptr();
                for f in 0..frames {
                    *plane.add(f) = samples[f * ch + c];
                }
            }
            buf.setFrameLength(frames as u32);

            // Count the buffer in-flight until its completion block fires
            // (AVFoundation copies the block, so the RcBlock ref we drop at
            // the end of this scope isn't the last one). A buffer scheduled
            // before a `clear` counts nothing when `stop` completes it.
            self.in_flight.fetch_add(1, Ordering::AcqRel);
            self.frames_in_flight
                .fetch_add(frames as u64, Ordering::AcqRel);
            let in_flight = self.in_flight.clone();
            let frames_in_flight = self.frames_in_flight.clone();
            let generation = self.generation.clone();
            let scheduled_in = generation.load(Ordering::Acquire);
            let done = RcBlock::new(move || {
                if generation.load(Ordering::Acquire) == scheduled_in {
                    in_flight.fetch_sub(1, Ordering::AcqRel);
                    frames_in_flight.fetch_sub(frames as u64, Ordering::AcqRel);
                }
            });
            self.player
                .scheduleBuffer_completionHandler(&buf, RcBlock::as_ptr(&done));
        }
        true
    }

    fn queued_frames(&self) -> Option<u64> {
        Some(self.frames_in_flight.load(Ordering::Acquire))
    }

    /// `pause` holds the player node's time and its scheduled buffers;
    /// `play` resumes them.
    fn set_paused(&self, paused: bool) -> bool {
        unsafe {
            if paused {
                self.player.pause();
            } else {
                self.player.play();
            }
        }
        true
    }

    /// `stop` drops every scheduled buffer (their completions fire and count
    /// nothing: the generation moved on), then `play` takes new ones.
    fn clear(&self) -> bool {
        self.generation.fetch_add(1, Ordering::AcqRel);
        unsafe { self.player.stop() };
        self.in_flight.store(0, Ordering::Release);
        self.frames_in_flight.store(0, Ordering::Release);
        unsafe { self.player.play() };
        true
    }
}

impl Drop for AvfSink {
    fn drop(&mut self) {
        unsafe { self.engine.stop() };
    }
}
