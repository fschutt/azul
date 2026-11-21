//! cpal audio playback (CoreAudio on macOS, WASAPI on Windows) for `AudioSink`.
//! The app pushes interleaved-f32 frames via `play`; cpal's output callback
//! pulls them from a shared queue. macOS/Windows only - linux uses the dlopen
//! ALSA backend (cpal's ALSA backend would build-time-link libasound).

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// An open cpal output stream + the queue the app feeds via `play`.
pub struct CpalSink {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    /// Held: the output callback plays silence and takes nothing from the
    /// queue (`OutputDevice::set_paused`).
    paused: Arc<AtomicBool>,
    channels: usize,
}

// The cpal Stream is `!Send`, but `AudioSink` follows the FFI handle convention
// (may live in app State). `play` only touches the Send+Sync queue; the stream
// is kept alive + dropped. Single-threaded use is assumed (as for the ALSA
// backend, which makes the same assertion).
unsafe impl Send for CpalSink {}
unsafe impl Sync for CpalSink {}

impl CpalSink {
    /// Open the default output device for `rate` x `channels` (f32 interleaved),
    /// or a readable reason why not: no output device, or the device refused
    /// the format.
    pub fn open(rate: u32, channels: u16) -> Result<CpalSink, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| String::from("no audio output device (WASAPI has no default output)"))?;
        let config = cpal::StreamConfig {
            channels: channels.max(1),
            sample_rate: cpal::SampleRate(if rate == 0 { 48_000 } else { rate }),
            buffer_size: cpal::BufferSize::Default,
        };
        let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let q = queue.clone();
        let paused = Arc::new(AtomicBool::new(false));
        let held = paused.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if held.load(Ordering::Acquire) {
                        out.iter_mut().for_each(|s| *s = 0.0);
                        return;
                    }
                    match q.lock() {
                        Ok(mut qq) => {
                            for s in out.iter_mut() {
                                *s = qq.pop_front().unwrap_or(0.0);
                            }
                        }
                        Err(_) => out.iter_mut().for_each(|s| *s = 0.0),
                    }
                },
                |e: cpal::StreamError| {
                    // Was a no-op: a device unplug mid-playback silently
                    // stopped audio with no line anywhere.
                    crate::plog_warn!(
                        "[audio] cpal output stream error: {} — playback may have stopped",
                        e
                    );
                },
                None,
            )
            .map_err(|e| {
                format!(
                    "the audio output device refused {} Hz x {} f32: {}",
                    config.sample_rate.0, config.channels, e
                )
            })?;
        stream
            .play()
            .map_err(|e| format!("the audio output stream did not start: {}", e))?;
        Ok(CpalSink {
            _stream: stream,
            queue,
            paused,
            channels: usize::from(config.channels.max(1)),
        })
    }
}

impl super::OutputDevice for CpalSink {
    /// Queue interleaved-f32 `samples` for the output callback. Bounded (~4 s
    /// at 48 kHz) so a stalled stream can't grow the queue without limit: a
    /// frame that does not fit is not taken.
    fn play(&self, samples: &[f32]) -> bool {
        if let Ok(mut q) = self.queue.lock() {
            if q.len() < 48_000 * 4 {
                q.extend(samples.iter().copied());
                return true;
            }
        }
        false
    }

    fn queued_frames(&self) -> Option<u64> {
        self.queue
            .lock()
            .ok()
            .map(|q| (q.len() / self.channels) as u64)
    }

    fn set_paused(&self, paused: bool) -> bool {
        self.paused.store(paused, Ordering::Release);
        true
    }

    fn clear(&self) -> bool {
        match self.queue.lock() {
            Ok(mut q) => {
                q.clear();
                true
            }
            Err(_) => false,
        }
    }
}
