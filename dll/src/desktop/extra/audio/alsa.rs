//! Minimal ALSA PCM playback (Linux), loaded at runtime via `libloading`.
//!
//! `libasound.so.2` is loaded lazily + dispatched through fn pointers (no
//! build-time link, so it cross-compiles + degrades gracefully when ALSA isn't
//! installed) - the dlopen rule, same as `forks/libudev-sys`. Only the handful
//! of PCM functions `AudioSink` needs are bound; ALSA's playback API is
//! opaque-pointer + scalar-arg (via `snd_pcm_set_params`), so there are no
//! fragile structs to transcribe.

use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::sync::OnceLock;

// ALSA enum constants (stable ABI).
const SND_PCM_STREAM_PLAYBACK: c_int = 0;
const SND_PCM_FORMAT_FLOAT_LE: c_int = 14;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;

struct AlsaFns {
    open: unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int,
    set_params:
        unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int,
    writei: unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long,
    recover: unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int,
    drain: unsafe extern "C" fn(*mut c_void) -> c_int,
    close: unsafe extern "C" fn(*mut c_void) -> c_int,
}

static ALSA: OnceLock<Option<(libloading::Library, AlsaFns)>> = OnceLock::new();

fn alsa() -> Option<&'static AlsaFns> {
    ALSA.get_or_init(|| unsafe {
        let lib = libloading::Library::new("libasound.so.2")
            .or_else(|_| libloading::Library::new("libasound.so"))
            .ok()?;
        let fns = AlsaFns {
            open: *lib.get(b"snd_pcm_open\0").ok()?,
            set_params: *lib.get(b"snd_pcm_set_params\0").ok()?,
            writei: *lib.get(b"snd_pcm_writei\0").ok()?,
            recover: *lib.get(b"snd_pcm_recover\0").ok()?,
            drain: *lib.get(b"snd_pcm_drain\0").ok()?,
            close: *lib.get(b"snd_pcm_close\0").ok()?,
        };
        Some((lib, fns))
    })
    .as_ref()
    .map(|(_, f)| f)
}

/// An open ALSA PCM playback stream (`*mut c_void` = `snd_pcm_t*`).
pub struct AlsaPcm {
    pcm: *mut c_void,
    channels: u32,
}

// The handle is used single-threaded from the AudioSink, but lives in a Boxed
// inner that the FFI handle may move between threads; the pointer is opaque.
unsafe impl Send for AlsaPcm {}
unsafe impl Sync for AlsaPcm {}

impl AlsaPcm {
    /// Open the default playback device for `rate` Hz x `channels`, interleaved
    /// f32. `None` if ALSA isn't loadable / no device / params rejected.
    pub fn open(rate: u32, channels: u32) -> Option<AlsaPcm> {
        if channels == 0 || rate == 0 {
            return None;
        }
        let f = alsa()?;
        unsafe {
            let mut pcm: *mut c_void = core::ptr::null_mut();
            let name = b"default\0";
            if (f.open)(
                &mut pcm,
                name.as_ptr() as *const c_char,
                SND_PCM_STREAM_PLAYBACK,
                0,
            ) < 0
                || pcm.is_null()
            {
                return None;
            }
            // FLOAT_LE interleaved, allow resample, ~100 ms latency.
            if (f.set_params)(
                pcm,
                SND_PCM_FORMAT_FLOAT_LE,
                SND_PCM_ACCESS_RW_INTERLEAVED,
                channels,
                rate,
                1,
                100_000,
            ) < 0
            {
                (f.close)(pcm);
                return None;
            }
            Some(AlsaPcm { pcm, channels })
        }
    }

    /// Write interleaved f32 `samples` (blocking), recovering once from an
    /// underrun. A partial/short write is acceptable for realtime audio.
    pub fn write(&self, samples: &[f32]) {
        let f = match alsa() {
            Some(f) => f,
            None => return,
        };
        let frames = (samples.len() / self.channels as usize) as c_ulong;
        if frames == 0 || self.pcm.is_null() {
            return;
        }
        unsafe {
            let n = (f.writei)(self.pcm, samples.as_ptr() as *const c_void, frames);
            if n < 0 {
                // -EPIPE (underrun) / -ESTRPIPE (suspend): recover + retry once.
                (f.recover)(self.pcm, n as c_int, 1);
                let _ = (f.writei)(self.pcm, samples.as_ptr() as *const c_void, frames);
            }
        }
    }
}

impl Drop for AlsaPcm {
    fn drop(&mut self) {
        if let Some(f) = alsa() {
            if !self.pcm.is_null() {
                unsafe {
                    (f.drain)(self.pcm);
                    (f.close)(self.pcm);
                }
            }
        }
    }
}
