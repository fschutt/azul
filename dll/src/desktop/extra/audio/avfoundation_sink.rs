//! Apple (iOS + macOS) audio playback via objc2 AVAudioEngine (cpal can't
//! cross-compile to iOS/macOS without SDK headers). An `AVAudioPlayerNode`
//! feeds the engine's main mixer; `play` deinterleaves each interleaved-f32
//! frame into an `AVAudioPCMBuffer` and schedules it. Counterpart to
//! `avfoundation_mic` (capture).
//!
//! Two correctness constraints this file upholds:
//!
//! * `AVAudioPlayerNode` requires the **standard** (deinterleaved float)
//!   format — scheduling interleaved f32 buffers is a documented crash /
//!   silent-failure class, so `open` builds the format with
//!   `initStandardFormatWithSampleRate:channels:` and `play` does the
//!   strided interleaved→planar copy into `floatChannelData`.
//! * Scheduled buffers must be **bounded**: `scheduleBuffer:completionHandler:`
//!   queues without limit, so if frames arrive faster than realtime the
//!   backlog (and memory) grows forever. An `Arc<AtomicUsize>` counts
//!   in-flight buffers (incremented before scheduling, decremented in the
//!   block2 completion handler — same `RcBlock` pattern as
//!   `extra/biometric/apple.rs`); past `MAX_IN_FLIGHT` the frame is dropped.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Once};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_avf_audio::{AVAudioEngine, AVAudioFormat, AVAudioPCMBuffer, AVAudioPlayerNode};

/// Max scheduled-but-unplayed buffers before `play` starts dropping frames.
/// ~8 × 20 ms UDP audio frames ≈ 160 ms of queued audio — enough slack to
/// absorb jitter without letting a fast producer run away from realtime.
const MAX_IN_FLIGHT: usize = 8;

/// An open AVAudioEngine playback graph for `AudioSink::play`.
pub struct AvfSink {
    engine: Retained<AVAudioEngine>,
    player: Retained<AVAudioPlayerNode>,
    format: Retained<AVAudioFormat>,
    channels: u16,
    /// Buffers scheduled on the player node whose completion handler hasn't
    /// fired yet — the backpressure gauge for `play`.
    in_flight: Arc<AtomicUsize>,
}

// Single-threaded use assumed (same assertion as the cpal/AAudio sinks).
unsafe impl Send for AvfSink {}
unsafe impl Sync for AvfSink {}

impl AvfSink {
    /// Build + start an engine with a player node connected to the main mixer,
    /// using the standard (deinterleaved Float32) format the player node
    /// requires. `None` on failure (note: the standard-format initializer
    /// rejects more than 2 channels).
    pub fn open(rate: u32, channels: u16) -> Option<AvfSink> {
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
                    crate::plog_warn!(
                        "[audio] AVAudioFormat standard init failed ({}Hz x{}ch) - no sink",
                        sample_rate,
                        ch
                    );
                    return None;
                }
            };
            let engine = AVAudioEngine::new();
            let player = AVAudioPlayerNode::new();
            engine.attachNode(&player);
            let mixer = engine.mainMixerNode();
            engine.connect_to_format(&player, &mixer, Some(&format));
            engine.prepare();
            if engine.startAndReturnError().is_err() {
                return None;
            }
            player.play();
            Some(AvfSink {
                engine,
                player,
                format,
                channels: channels.max(1),
                in_flight: Arc::new(AtomicUsize::new(0)),
            })
        }
    }

    /// Deinterleave `samples` (interleaved f32) into a standard-format PCM
    /// buffer + schedule it. Drops the frame (logged once) when more than
    /// [`MAX_IN_FLIGHT`] buffers are already queued on the player node.
    pub fn play(&self, samples: &[f32]) {
        let ch = self.channels.max(1) as usize;
        let frames = samples.len() / ch;
        if frames == 0 {
            return;
        }
        // Backpressure: never let the scheduled backlog grow past the cap.
        if self.in_flight.load(Ordering::Acquire) >= MAX_IN_FLIGHT {
            static DROPPED: Once = Once::new();
            DROPPED.call_once(|| {
                crate::plog_warn!(
                    "[audio] sink backlog full ({} buffers in flight) - dropping frames \
                     (producer faster than realtime; logged once)",
                    MAX_IN_FLIGHT
                );
            });
            return;
        }
        unsafe {
            let buf = match AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &self.format,
                frames as u32,
            ) {
                Some(b) => b,
                None => return,
            };
            let data = buf.floatChannelData();
            if data.is_null() {
                return;
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
            // the end of this scope isn't the last one).
            self.in_flight.fetch_add(1, Ordering::AcqRel);
            let in_flight = self.in_flight.clone();
            let done = RcBlock::new(move || {
                in_flight.fetch_sub(1, Ordering::AcqRel);
            });
            self.player
                .scheduleBuffer_completionHandler(&buf, RcBlock::as_ptr(&done));
        }
    }
}

impl Drop for AvfSink {
    fn drop(&mut self) {
        unsafe { self.engine.stop() };
    }
}
