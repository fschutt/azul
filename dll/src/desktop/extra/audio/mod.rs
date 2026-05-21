//! Audio playback handle (SUPER_PLAN_2 §4 P7) - `AudioSink`.
//!
//! The playback counterpart to `MicrophoneWidget` (capture). Like `Db` / the
//! `Pdf` handle, `AudioSink` carries an engine resource, so it's a handle
//! (`ptr` + `run_destructor`, the C-ABI ownership convention) rather than a
//! widget - the app holds it in its own State (no globals) and calls
//! `play(frame)` whenever it has audio to play (e.g. an `AudioFrame` just
//! received over UDP for azul-meet).
//!
//! `AudioSink::open(config) -> AudioSink`; `sink.play(AudioFrame)`;
//! `sink.is_open()`; dropping the handle (or `close`) stops playback.
//!
//! The actual output (rodio / cpal on the desktop, AVAudioEngine / AAudio on
//! mobile) is the on-device backend - same as the mic capture worker. This
//! tick ships the handle + a **stub** engine (counts frames, no sound) so the
//! API surface + ownership are real and codegen-exposed; the real backend
//! swaps in behind a feature later.

use core::ffi::c_void;

use azul_core::audio::{AudioConfig, AudioFrame};

#[cfg(target_os = "linux")]
mod alsa;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod cpal_mic;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod cpal_sink;
#[cfg(target_os = "android")]
mod aaudio;
#[cfg(target_os = "ios")]
mod avfoundation_mic;

/// Internal playback state behind the `AudioSink` handle. The stub tracks the
/// config + how many frames were submitted; the real backend replaces it with
/// a live output stream + queue.
struct AudioSinkInner {
    #[allow(dead_code)]
    config: AudioConfig,
    frames_played: u64,
    /// The live ALSA playback stream on Linux (`None` if ALSA / no device).
    #[cfg(target_os = "linux")]
    pcm: Option<alsa::AlsaPcm>,
    /// The live cpal output stream on macOS/Windows (`None` if no device).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    sink: Option<cpal_sink::CpalSink>,
    /// The live AAudio output stream on Android (`None` if no device).
    #[cfg(target_os = "android")]
    android_sink: Option<aaudio::AAudioSink>,
}

/// An audio output handle. Open one with [`AudioSink::open`], feed it
/// [`AudioFrame`]s with [`play`](Self::play); drop it to stop. Carries an
/// engine resource (the output stream), so it follows the C-ABI handle
/// convention (`run_destructor` + custom `Drop`) like `Db`.
#[repr(C)]
pub struct AudioSink {
    /// Opaque pointer to the engine-side `AudioSinkInner` (or null when not
    /// open / on failure).
    pub ptr: *mut c_void,
    /// Whether this handle owns (and on drop frees) the engine resource.
    pub run_destructor: bool,
}

impl Clone for AudioSink {
    fn clone(&self) -> Self {
        // Non-owning shallow handle copy - only the original frees the engine
        // (the FFI handle convention).
        AudioSink {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for AudioSink {
    fn default() -> Self {
        AudioSink {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl AudioSink {
    /// Open an audio output for `config` (sample rate + channels). Returns an
    /// invalid handle (`is_open()` false) on failure. The stub engine always
    /// "opens"; the real rodio / AVAudio backend may fail (no device).
    pub fn open(config: AudioConfig) -> AudioSink {
        #[cfg(target_os = "linux")]
        let pcm = alsa::AlsaPcm::open(config.sample_rate, config.channels as u32);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let sink = cpal_sink::CpalSink::open(config.sample_rate, config.channels);
        #[cfg(target_os = "android")]
        let android_sink = aaudio::AAudioSink::open(config.sample_rate, config.channels);
        let inner = Box::new(AudioSinkInner {
            config,
            frames_played: 0,
            #[cfg(target_os = "linux")]
            pcm,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            sink,
            #[cfg(target_os = "android")]
            android_sink,
        });
        AudioSink {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// Whether the sink opened successfully.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Queue `frame` for playback. Interleaved `f32` samples in the frame's
    /// format are sent to the output. (Stub: counts the frame; the on-device
    /// backend plays the samples.)
    pub fn play(&self, frame: AudioFrame) {
        if let Some(inner) = unsafe { (self.ptr as *mut AudioSinkInner).as_mut() } {
            inner.frames_played = inner.frames_played.wrapping_add(1);
            #[cfg(target_os = "linux")]
            if let Some(pcm) = &inner.pcm {
                pcm.write(frame.samples.as_ref());
            }
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            if let Some(sink) = &inner.sink {
                sink.play(frame.samples.as_ref());
            }
            #[cfg(target_os = "android")]
            if let Some(sink) = &inner.android_sink {
                sink.play(frame.samples.as_ref());
            }
            let _ = frame;
        }
    }

    /// Number of frames submitted via [`play`](Self::play) so far (`0` if not
    /// open). Mostly a stub progress signal until the real backend lands.
    pub fn frames_played(&self) -> u64 {
        unsafe { (self.ptr as *const AudioSinkInner).as_ref() }
            .map(|i| i.frames_played)
            .unwrap_or(0)
    }

    /// Stop playback + release the output. (Dropping the handle does this too;
    /// `close` is for explicit/FFI control.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut AudioSinkInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for AudioSink {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

/// Register the platform microphone-capture backend with the layout seam, once.
/// Called from the per-frame layout pass (like `sensors::ensure_started`) so
/// `MicrophoneWidget` captures real audio where a backend exists (ALSA on
/// Linux); a no-op everywhere else (the widget keeps its test tone).
pub fn ensure_mic_backend() {
    #[cfg(target_os = "linux")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_mic_backend(
                azul_layout::widgets::capture_common::AudioCaptureVTable {
                    open: alsa::mic_open,
                    read: alsa::mic_read,
                    close: alsa::mic_close,
                },
            );
        });
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_mic_backend(
                azul_layout::widgets::capture_common::AudioCaptureVTable {
                    open: cpal_mic::mic_open,
                    read: cpal_mic::mic_read,
                    close: cpal_mic::mic_close,
                },
            );
        });
    }
    #[cfg(target_os = "android")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_mic_backend(
                azul_layout::widgets::capture_common::AudioCaptureVTable {
                    open: aaudio::mic_open,
                    read: aaudio::mic_read,
                    close: aaudio::mic_close,
                },
            );
        });
    }
    #[cfg(target_os = "ios")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            azul_layout::widgets::capture_common::register_mic_backend(
                azul_layout::widgets::capture_common::AudioCaptureVTable {
                    open: avfoundation_mic::mic_open,
                    read: avfoundation_mic::mic_read,
                    close: avfoundation_mic::mic_close,
                },
            );
        });
    }
}
