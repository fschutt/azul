//! Android audio (mic capture + `AudioSink` playback) via the NDK AAudio C API
//! (`ndk-sys`). AAudio's blocking read/write maps cleanly to the seam's
//! pull/push - no async callbacks (unlike Camera2). Format is PCM_FLOAT (f32
//! interleaved), so no sample conversion.

use std::os::raw::c_void;
use std::ptr;

use ndk_sys::{
    AAudioStreamBuilder_delete, AAudioStreamBuilder_openStream, AAudioStreamBuilder_setChannelCount,
    AAudioStreamBuilder_setDirection, AAudioStreamBuilder_setFormat,
    AAudioStreamBuilder_setSampleRate, AAudioStream_close, AAudioStream_read,
    AAudioStream_requestStart, AAudioStream_write, AAudio_createStreamBuilder, AAudioStream,
    AAudioStreamBuilder, AAUDIO_DIRECTION_INPUT, AAUDIO_DIRECTION_OUTPUT, AAUDIO_FORMAT_PCM_FLOAT,
};

/// 100 ms read/write timeout (ns).
const TIMEOUT_NS: i64 = 100_000_000;

/// Open a started PCM_FLOAT stream in `direction`. Null on any failure.
unsafe fn open_stream(rate: u32, channels: u16, direction: ndk_sys::aaudio_direction_t) -> *mut AAudioStream {
    let mut builder: *mut AAudioStreamBuilder = ptr::null_mut();
    if AAudio_createStreamBuilder(&mut builder) < 0 || builder.is_null() {
        return ptr::null_mut();
    }
    AAudioStreamBuilder_setDirection(builder, direction);
    AAudioStreamBuilder_setFormat(builder, AAUDIO_FORMAT_PCM_FLOAT as ndk_sys::aaudio_format_t);
    AAudioStreamBuilder_setSampleRate(builder, if rate == 0 { 48_000 } else { rate } as i32);
    AAudioStreamBuilder_setChannelCount(builder, channels.max(1) as i32);
    let mut stream: *mut AAudioStream = ptr::null_mut();
    let r = AAudioStreamBuilder_openStream(builder, &mut stream);
    AAudioStreamBuilder_delete(builder);
    if r < 0 || stream.is_null() {
        return ptr::null_mut();
    }
    if AAudioStream_requestStart(stream) < 0 {
        AAudioStream_close(stream);
        return ptr::null_mut();
    }
    stream
}

// --- Microphone capture (seam vtable) ---

struct AAudioMic {
    stream: *mut AAudioStream,
    channels: u16,
}

pub fn mic_open(rate: u32, channels: u16) -> u64 {
    let stream = unsafe { open_stream(rate, channels, AAUDIO_DIRECTION_INPUT as ndk_sys::aaudio_direction_t) };
    if stream.is_null() {
        return 0;
    }
    Box::into_raw(Box::new(AAudioMic {
        stream,
        channels: channels.max(1),
    })) as u64
}

pub fn mic_read(handle: u64, out: &mut Vec<f32>) -> u32 {
    let mic = match unsafe { (handle as *const AAudioMic).as_ref() } {
        Some(m) => m,
        None => return 0,
    };
    const FRAMES: i32 = 512;
    let ch = mic.channels.max(1) as usize;
    out.clear();
    out.resize(FRAMES as usize * ch, 0.0);
    let n = unsafe {
        AAudioStream_read(mic.stream, out.as_mut_ptr() as *mut c_void, FRAMES, TIMEOUT_NS)
    };
    if n <= 0 {
        out.clear();
        return 0;
    }
    out.truncate(n as usize * ch);
    n as u32
}

pub fn mic_close(handle: u64) {
    if handle != 0 {
        unsafe {
            let m = Box::from_raw(handle as *mut AAudioMic);
            AAudioStream_close(m.stream);
        }
    }
}

// --- AudioSink playback ---

/// An open AAudio output stream for `AudioSink::play` (blocking write).
pub struct AAudioSink {
    stream: *mut AAudioStream,
    channels: u16,
}

// Single-threaded use assumed (same assertion as the ALSA/cpal sinks).
unsafe impl Send for AAudioSink {}
unsafe impl Sync for AAudioSink {}

impl AAudioSink {
    pub fn open(rate: u32, channels: u16) -> Option<AAudioSink> {
        let stream = unsafe { open_stream(rate, channels, AAUDIO_DIRECTION_OUTPUT as ndk_sys::aaudio_direction_t) };
        if stream.is_null() {
            return None;
        }
        Some(AAudioSink {
            stream,
            channels: channels.max(1),
        })
    }

    pub fn play(&self, samples: &[f32]) {
        let frames = (samples.len() / self.channels.max(1) as usize) as i32;
        if frames <= 0 {
            return;
        }
        unsafe {
            AAudioStream_write(self.stream, samples.as_ptr() as *const c_void, frames, TIMEOUT_NS);
        }
    }
}

impl Drop for AAudioSink {
    fn drop(&mut self) {
        if !self.stream.is_null() {
            unsafe { AAudioStream_close(self.stream) };
        }
    }
}
