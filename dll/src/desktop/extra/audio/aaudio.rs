//! Android audio (mic capture + `AudioSink` playback) via the NDK AAudio C API,
//! loaded at RUNTIME with `libloading` (dlopen of `libaaudio.so`) — NOT a
//! build-time link. AAudio is API 26+; dlopen'ing it (instead of `-laaudio`)
//! keeps the app installable + loadable on older Android (API 24/25), where the
//! lib is simply absent and audio degrades to "unavailable" (`None`/`0`) rather
//! than the whole `.so` failing to load. Format is PCM_FLOAT (f32 interleaved),
//! so no sample conversion. Mirrors the dlopen rule of `audio/alsa.rs`.

use core::ffi::{c_int, c_void};
use std::{ptr, sync::OnceLock};

// AAudio enum constants (stable ABI).
const AAUDIO_DIRECTION_OUTPUT: c_int = 0;
const AAUDIO_DIRECTION_INPUT: c_int = 1;
const AAUDIO_FORMAT_PCM_FLOAT: c_int = 2;

/// 100 ms read/write timeout (ns).
const TIMEOUT_NS: i64 = 100_000_000;

// Opaque AAudio handles (we only ever pass the pointers back to AAudio).
type AAudioStreamBuilder = c_void;
type AAudioStream = c_void;

struct AAudioFns {
    create_builder: unsafe extern "C" fn(*mut *mut AAudioStreamBuilder) -> c_int,
    set_direction: unsafe extern "C" fn(*mut AAudioStreamBuilder, c_int),
    set_format: unsafe extern "C" fn(*mut AAudioStreamBuilder, c_int),
    set_sample_rate: unsafe extern "C" fn(*mut AAudioStreamBuilder, c_int),
    set_channel_count: unsafe extern "C" fn(*mut AAudioStreamBuilder, c_int),
    open_stream: unsafe extern "C" fn(*mut AAudioStreamBuilder, *mut *mut AAudioStream) -> c_int,
    delete_builder: unsafe extern "C" fn(*mut AAudioStreamBuilder) -> c_int,
    request_start: unsafe extern "C" fn(*mut AAudioStream) -> c_int,
    close: unsafe extern "C" fn(*mut AAudioStream) -> c_int,
    read: unsafe extern "C" fn(*mut AAudioStream, *mut c_void, c_int, i64) -> c_int,
    write: unsafe extern "C" fn(*mut AAudioStream, *const c_void, c_int, i64) -> c_int,
}

static AAUDIO: OnceLock<Option<(libloading::Library, AAudioFns)>> = OnceLock::new();

/// Resolve libaaudio.so + its functions, once. `None` on Android < 26 (the lib
/// is absent) — callers then report audio unavailable instead of crashing.
fn aaudio() -> Option<&'static AAudioFns> {
    AAUDIO
        .get_or_init(|| unsafe {
            let lib = crate::desktop::open_first_lib(&["libaaudio.so"])?;
            let fns = AAudioFns {
                create_builder: *lib.get(b"AAudio_createStreamBuilder\0").ok()?,
                set_direction: *lib.get(b"AAudioStreamBuilder_setDirection\0").ok()?,
                set_format: *lib.get(b"AAudioStreamBuilder_setFormat\0").ok()?,
                set_sample_rate: *lib.get(b"AAudioStreamBuilder_setSampleRate\0").ok()?,
                set_channel_count: *lib.get(b"AAudioStreamBuilder_setChannelCount\0").ok()?,
                open_stream: *lib.get(b"AAudioStreamBuilder_openStream\0").ok()?,
                delete_builder: *lib.get(b"AAudioStreamBuilder_delete\0").ok()?,
                request_start: *lib.get(b"AAudioStream_requestStart\0").ok()?,
                close: *lib.get(b"AAudioStream_close\0").ok()?,
                read: *lib.get(b"AAudioStream_read\0").ok()?,
                write: *lib.get(b"AAudioStream_write\0").ok()?,
            };
            Some((lib, fns))
        })
        .as_ref()
        .map(|(_, f)| f)
}

/// Open a started PCM_FLOAT stream in `direction`, or a readable reason why
/// not (including AAudio being unavailable on this Android version).
unsafe fn open_stream(
    rate: u32,
    channels: u16,
    direction: c_int,
) -> Result<*mut AAudioStream, String> {
    let what = if direction == AAUDIO_DIRECTION_OUTPUT {
        "output"
    } else {
        "input"
    };
    let f = aaudio().ok_or_else(|| {
        String::from(
            "AAudio (libaaudio.so) is not available: audio needs Android 8.0 (API 26) or newer",
        )
    })?;
    let mut builder: *mut AAudioStreamBuilder = ptr::null_mut();
    let rc = (f.create_builder)(&mut builder);
    if rc < 0 || builder.is_null() {
        return Err(format!(
            "AAudio could not create a stream builder (rc={})",
            rc
        ));
    }
    let rate = if rate == 0 { 48_000 } else { rate };
    let channels = channels.max(1);
    (f.set_direction)(builder, direction);
    (f.set_format)(builder, AAUDIO_FORMAT_PCM_FLOAT);
    (f.set_sample_rate)(builder, rate as c_int);
    (f.set_channel_count)(builder, channels as c_int);
    let mut stream: *mut AAudioStream = ptr::null_mut();
    let r = (f.open_stream)(builder, &mut stream);
    (f.delete_builder)(builder);
    if r < 0 || stream.is_null() {
        return Err(format!(
            "no AAudio {} device took {} Hz x {} f32 (AAudioStreamBuilder_openStream rc={})",
            what, rate, channels, r
        ));
    }
    let rc = (f.request_start)(stream);
    if rc < 0 {
        (f.close)(stream);
        return Err(format!(
            "the AAudio {} stream did not start (rc={})",
            what, rc
        ));
    }
    Ok(stream)
}

// --- Microphone capture (seam vtable) ---

struct AAudioMic {
    stream: *mut AAudioStream,
    channels: u16,
}

pub fn mic_open(rate: u32, channels: u16) -> u64 {
    let stream = match unsafe { open_stream(rate, channels, AAUDIO_DIRECTION_INPUT) } {
        Ok(stream) => stream,
        Err(_) => return 0,
    };
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
    let f = match aaudio() {
        Some(f) => f,
        None => return 0,
    };
    const FRAMES: c_int = 512;
    let ch = mic.channels.max(1) as usize;
    out.clear();
    out.resize(FRAMES as usize * ch, 0.0);
    let n = unsafe {
        (f.read)(
            mic.stream,
            out.as_mut_ptr() as *mut c_void,
            FRAMES,
            TIMEOUT_NS,
        )
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
            if let Some(f) = aaudio() {
                (f.close)(m.stream);
            }
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
    /// Open a started output stream for `rate` x `channels` (f32
    /// interleaved), or a readable reason why not.
    pub fn open(rate: u32, channels: u16) -> Result<AAudioSink, String> {
        let stream = unsafe { open_stream(rate, channels, AAUDIO_DIRECTION_OUTPUT) }?;
        Ok(AAudioSink {
            stream,
            channels: channels.max(1),
        })
    }
}

impl super::OutputDevice for AAudioSink {
    /// Write interleaved f32 `samples` (blocking up to `TIMEOUT_NS`). Taken
    /// when AAudio accepted frames.
    fn play(&self, samples: &[f32]) -> bool {
        let frames = (samples.len() / self.channels.max(1) as usize) as c_int;
        if frames <= 0 {
            return false;
        }
        match aaudio() {
            Some(f) => unsafe {
                (f.write)(
                    self.stream,
                    samples.as_ptr() as *const c_void,
                    frames,
                    TIMEOUT_NS,
                ) > 0
            },
            None => false,
        }
    }
}

impl Drop for AAudioSink {
    fn drop(&mut self) {
        if !self.stream.is_null() {
            if let Some(f) = aaudio() {
                unsafe { (f.close)(self.stream) };
            }
        }
    }
}
