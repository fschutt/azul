//! Cross-platform microphone capture via `cpal` (CoreAudio on macOS, WASAPI
//! on Windows). cpal owns the platform audio ABI and delivers samples through a
//! callback; we bridge that to the seam's pull API - the callback appends to a
//! shared buffer and `mic_read` drains it. Linux uses the dlopen ALSA backend
//! instead (cpal's ALSA backend would build-time-link `libasound`, breaking the
//! cross-compile); Android uses AAudio.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

/// Live capture state behind the seam's `u64` handle. The cpal `Stream` is
/// `!Send`, but the mic worker calls `open`/`read`/`close` all on one thread,
/// so it never crosses a thread boundary.
struct CpalMic {
    _stream: cpal::Stream,
    shared: Arc<Mutex<Vec<f32>>>,
    channels: u16,
}

fn err_cb(_e: cpal::StreamError) {}

/// Open the default input device (cpal uses its default config; `rate`/`channels`
/// are advisory). Returns a boxed handle, or `0` on failure (test-tone fallback).
pub fn mic_open(_rate: u32, _channels: u16) -> u64 {
    let host = cpal::default_host();
    let device = match host.default_input_device() {
        Some(d) => d,
        None => return 0,
    };
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let channels = config.channels();
    let fmt = config.sample_format();
    let cfg: cpal::StreamConfig = config.into();
    let shared = Arc::new(Mutex::new(Vec::<f32>::new()));
    let s = shared.clone();
    let stream = match fmt {
        SampleFormat::F32 => device.build_input_stream(
            &cfg,
            move |d: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut b) = s.lock() {
                    b.extend_from_slice(d);
                }
            },
            err_cb,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &cfg,
            move |d: &[i16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut b) = s.lock() {
                    b.extend(d.iter().map(|&x| x as f32 / 32768.0));
                }
            },
            err_cb,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &cfg,
            move |d: &[u16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut b) = s.lock() {
                    b.extend(d.iter().map(|&x| (x as f32 - 32768.0) / 32768.0));
                }
            },
            err_cb,
            None,
        ),
        _ => return 0,
    };
    let stream = match stream {
        Ok(st) => st,
        Err(_) => return 0,
    };
    if stream.play().is_err() {
        return 0;
    }
    Box::into_raw(Box::new(CpalMic {
        _stream: stream,
        shared,
        channels,
    })) as u64
}

/// Drain captured interleaved-f32 samples into `out`. Returns the frame count
/// (samples / channels), or `0` if none captured yet (the worker retries).
pub fn mic_read(handle: u64, out: &mut Vec<f32>) -> u32 {
    let mic = match unsafe { (handle as *const CpalMic).as_ref() } {
        Some(m) => m,
        None => return 0,
    };
    for _ in 0..120 {
        if let Ok(mut b) = mic.shared.lock() {
            if !b.is_empty() {
                out.clear();
                out.append(&mut b);
                return (out.len() / mic.channels.max(1) as usize) as u32;
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    0
}

/// Stop + free the stream (drops the boxed `CpalMic`).
pub fn mic_close(handle: u64) {
    if handle != 0 {
        unsafe {
            drop(Box::from_raw(handle as *mut CpalMic));
        }
    }
}
