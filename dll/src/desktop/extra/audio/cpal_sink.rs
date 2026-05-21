//! cpal audio playback (CoreAudio on macOS, WASAPI on Windows) for `AudioSink`.
//! The app pushes interleaved-f32 frames via `play`; cpal's output callback
//! pulls them from a shared queue. macOS/Windows only - linux uses the dlopen
//! ALSA backend (cpal's ALSA backend would build-time-link libasound).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// An open cpal output stream + the queue the app feeds via `play`.
pub struct CpalSink {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
}

// The cpal Stream is `!Send`, but `AudioSink` follows the FFI handle convention
// (may live in app State). `play` only touches the Send+Sync queue; the stream
// is kept alive + dropped. Single-threaded use is assumed (as for the ALSA
// backend, which makes the same assertion).
unsafe impl Send for CpalSink {}
unsafe impl Sync for CpalSink {}

impl CpalSink {
    /// Open the default output device for `rate` x `channels` (f32 interleaved).
    /// `None` if there's no device or the config is rejected.
    pub fn open(rate: u32, channels: u16) -> Option<CpalSink> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let config = cpal::StreamConfig {
            channels: channels.max(1),
            sample_rate: cpal::SampleRate(if rate == 0 { 48_000 } else { rate }),
            buffer_size: cpal::BufferSize::Default,
        };
        let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let q = queue.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| match q.lock() {
                    Ok(mut qq) => {
                        for s in out.iter_mut() {
                            *s = qq.pop_front().unwrap_or(0.0);
                        }
                    }
                    Err(_) => out.iter_mut().for_each(|s| *s = 0.0),
                },
                |_e: cpal::StreamError| {},
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(CpalSink {
            _stream: stream,
            queue,
        })
    }

    /// Queue interleaved-f32 `samples` for playback. Bounded (~4 s at 48 kHz)
    /// so a stalled stream can't grow the queue without limit.
    pub fn play(&self, samples: &[f32]) {
        if let Ok(mut q) = self.queue.lock() {
            if q.len() < 48_000 * 4 {
                q.extend(samples.iter().copied());
            }
        }
    }
}
