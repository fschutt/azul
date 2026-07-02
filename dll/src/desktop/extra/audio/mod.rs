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
use azul_css::{AzString, StringVec};

#[cfg(target_os = "linux")]
mod alsa;
#[cfg(target_os = "windows")]
mod cpal_mic;
#[cfg(target_os = "windows")]
mod cpal_sink;
#[cfg(target_os = "android")]
mod aaudio;
#[cfg(any(target_os = "ios", target_os = "macos"))]
mod avfoundation_mic;
#[cfg(all(any(target_os = "ios", target_os = "macos"), feature = "objc2-avf-audio"))]
mod avfoundation_sink;

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
    #[cfg(target_os = "windows")]
    sink: Option<cpal_sink::CpalSink>,
    /// The live AAudio output stream on Android (`None` if no device).
    #[cfg(target_os = "android")]
    android_sink: Option<aaudio::AAudioSink>,
    /// The live AVAudioEngine playback graph on iOS (`None` if it failed).
    #[cfg(all(any(target_os = "ios", target_os = "macos"), feature = "objc2-avf-audio"))]
    ios_sink: Option<avfoundation_sink::AvfSink>,
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
        crate::plog_info!(
            "[audio] opening sink: {}Hz x{}ch (f32 interleaved)",
            config.sample_rate,
            config.channels
        );
        #[cfg(target_os = "linux")]
        let pcm = alsa::AlsaPcm::open(config.sample_rate, config.channels as u32);
        #[cfg(target_os = "windows")]
        let sink = cpal_sink::CpalSink::open(config.sample_rate, config.channels);
        #[cfg(target_os = "android")]
        let android_sink = aaudio::AAudioSink::open(config.sample_rate, config.channels);
        #[cfg(all(any(target_os = "ios", target_os = "macos"), feature = "objc2-avf-audio"))]
        let ios_sink = avfoundation_sink::AvfSink::open(config.sample_rate, config.channels);
        let inner = Box::new(AudioSinkInner {
            config,
            frames_played: 0,
            #[cfg(target_os = "linux")]
            pcm,
            #[cfg(target_os = "windows")]
            sink,
            #[cfg(target_os = "android")]
            android_sink,
            #[cfg(all(any(target_os = "ios", target_os = "macos"), feature = "objc2-avf-audio"))]
            ios_sink,
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
            #[cfg(target_os = "windows")]
            if let Some(sink) = &inner.sink {
                sink.play(frame.samples.as_ref());
            }
            #[cfg(target_os = "android")]
            if let Some(sink) = &inner.android_sink {
                sink.play(frame.samples.as_ref());
            }
            #[cfg(all(any(target_os = "ios", target_os = "macos"), feature = "objc2-avf-audio"))]
            if let Some(sink) = &inner.ios_sink {
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
    #[cfg(target_os = "windows")]
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
    #[cfg(any(target_os = "ios", target_os = "macos"))]
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

/// The audio output (sink) + input (source) devices on this machine, by name —
/// enumerate them so the app can pick where to play audio or which mic to capture.
/// (A device can be both, e.g. a duplex interface — it then appears in both lists.)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AudioDeviceList {
    /// Output device names (speakers / headphones / HDMI / monitors).
    pub outputs: StringVec,
    /// Input device names (microphones / line-in / loopback).
    pub inputs: StringVec,
}

impl AudioDeviceList {
    /// Enumerate the machine's audio devices. Linux: PipeWire/PulseAudio via
    /// `pactl list short sinks/sources`; macOS: the CoreAudio HAL
    /// (`AudioObjectGetPropertyData` on the system object, dlopen'd at runtime —
    /// no link-time dep); empty on platforms without an enumeration backend yet
    /// (and if `pactl` isn't installed / CoreAudio can't be loaded).
    pub fn enumerate() -> AudioDeviceList {
        #[cfg(target_os = "linux")]
        {
            AudioDeviceList {
                outputs: pactl_device_names("sinks"),
                inputs: pactl_device_names("sources"),
            }
        }
        #[cfg(target_os = "macos")]
        {
            let (outputs, inputs) = coreaudio_device_names();
            AudioDeviceList { outputs, inputs }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            AudioDeviceList {
                outputs: StringVec::from_vec(Vec::new()),
                inputs: StringVec::from_vec(Vec::new()),
            }
        }
    }
}

/// Names from `pactl list short <kind>` (kind = "sinks" | "sources"): the 2nd
/// tab-separated column of each line.
#[cfg(target_os = "linux")]
fn pactl_device_names(kind: &str) -> StringVec {
    let stdout = match std::process::Command::new("pactl")
        .args(["list", "short", kind])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return StringVec::from_vec(Vec::new()),
    };
    let text = String::from_utf8_lossy(&stdout);
    let names: Vec<AzString> = text
        .lines()
        .filter_map(|l| l.split('\t').nth(1))
        .filter(|n| !n.is_empty())
        .map(AzString::from)
        .collect();
    StringVec::from_vec(names)
}

/// (Output-names, input-names) from the CoreAudio HAL. CoreAudio.framework is
/// dlopen'd at runtime via `libloading` (same rule as `camera/v4l2.rs` — NO
/// link-time dep, so cross-compiles stay clean and a missing framework only
/// fails gracefully). Flow: kAudioHardwarePropertyDevices on the system object
/// → AudioObjectID list; per device kAudioDevicePropertyDeviceNameCFString for
/// the name (CFString via objc2-core-foundation) and
/// kAudioDevicePropertyStreamConfiguration in the input/output scope — a
/// non-empty AudioBufferList in a scope means the device plays (or captures)
/// there. A duplex device lands in both lists (per the `enumerate` doc).
#[cfg(target_os = "macos")]
fn coreaudio_device_names() -> (StringVec, StringVec) {
    use core::ptr::NonNull;
    use std::sync::OnceLock;

    use objc2_core_foundation::{CFRetained, CFString};

    /// `AudioObjectPropertyAddress` (CoreAudio/AudioHardwareBase.h).
    #[repr(C)]
    struct PropAddr {
        selector: u32,
        scope: u32,
        element: u32,
    }

    type GetSizeFn =
        unsafe extern "C" fn(u32, *const PropAddr, u32, *const c_void, *mut u32) -> i32;
    type GetDataFn = unsafe extern "C" fn(
        u32,
        *const PropAddr,
        u32,
        *const c_void,
        *mut u32,
        *mut c_void,
    ) -> i32;

    struct HalFns {
        get_size: GetSizeFn,
        get_data: GetDataFn,
    }

    static HAL: OnceLock<Option<(libloading::Library, HalFns)>> = OnceLock::new();
    let fns = HAL
        .get_or_init(|| unsafe {
            let lib = crate::desktop::open_first_lib(&[
                "/System/Library/Frameworks/CoreAudio.framework/CoreAudio",
            ])?;
            let fns = HalFns {
                get_size: *lib.get(b"AudioObjectGetPropertyDataSize\0").ok()?,
                get_data: *lib.get(b"AudioObjectGetPropertyData\0").ok()?,
            };
            Some((lib, fns))
        })
        .as_ref()
        .map(|(_, f)| f);
    let fns = match fns {
        Some(f) => f,
        None => {
            crate::plog_warn!("[audio] CoreAudio.framework not loadable - no device names");
            return (
                StringVec::from_vec(Vec::new()),
                StringVec::from_vec(Vec::new()),
            );
        }
    };

    // FourCC property selectors / scopes (CoreAudio/AudioHardware.h).
    const SYSTEM_OBJECT: u32 = 1; // kAudioObjectSystemObject
    const SEL_DEVICES: u32 = 0x6465_7623; // 'dev#' kAudioHardwarePropertyDevices
    const SEL_NAME: u32 = 0x6C6E_616D; // 'lnam' kAudioDevicePropertyDeviceNameCFString
    const SEL_STREAM_CFG: u32 = 0x736C_6179; // 'slay' kAudioDevicePropertyStreamConfiguration
    const SCOPE_GLOBAL: u32 = 0x676C_6F62; // 'glob'
    const SCOPE_INPUT: u32 = 0x696E_7074; // 'inpt'
    const SCOPE_OUTPUT: u32 = 0x6F75_7470; // 'outp'

    /// Whether the device has any stream buffers in `scope` (input/output):
    /// fetch the scope's AudioBufferList and check `mNumberBuffers > 0`.
    unsafe fn has_buffers(fns: &HalFns, device: u32, scope: u32) -> bool {
        let addr = PropAddr {
            selector: SEL_STREAM_CFG,
            scope,
            element: 0,
        };
        let mut size = 0u32;
        if unsafe { (fns.get_size)(device, &addr, 0, core::ptr::null(), &mut size) } != 0
            || size < 4
        {
            return false;
        }
        let mut buf = vec![0u8; size as usize];
        if unsafe {
            (fns.get_data)(
                device,
                &addr,
                0,
                core::ptr::null(),
                &mut size,
                buf.as_mut_ptr() as *mut c_void,
            )
        } != 0
            || (size as usize) < 4
        {
            return false;
        }
        // AudioBufferList starts with `UInt32 mNumberBuffers`.
        u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) > 0
    }

    let mut outputs: Vec<AzString> = Vec::new();
    let mut inputs: Vec<AzString> = Vec::new();
    unsafe {
        // All device IDs on the system object.
        let addr = PropAddr {
            selector: SEL_DEVICES,
            scope: SCOPE_GLOBAL,
            element: 0,
        };
        let mut size = 0u32;
        if (fns.get_size)(SYSTEM_OBJECT, &addr, 0, core::ptr::null(), &mut size) != 0 || size < 4 {
            return (
                StringVec::from_vec(Vec::new()),
                StringVec::from_vec(Vec::new()),
            );
        }
        let mut ids = vec![0u32; size as usize / 4];
        if (fns.get_data)(
            SYSTEM_OBJECT,
            &addr,
            0,
            core::ptr::null(),
            &mut size,
            ids.as_mut_ptr() as *mut c_void,
        ) != 0
        {
            return (
                StringVec::from_vec(Vec::new()),
                StringVec::from_vec(Vec::new()),
            );
        }
        ids.truncate(size as usize / 4);

        for id in ids {
            // Device name: a +1-retained CFStringRef the caller must release —
            // CFRetained::from_raw takes over exactly that reference.
            let addr = PropAddr {
                selector: SEL_NAME,
                scope: SCOPE_GLOBAL,
                element: 0,
            };
            let mut cf: *mut c_void = core::ptr::null_mut();
            let mut size = size_of::<*mut c_void>() as u32;
            let cf_out: *mut *mut c_void = &mut cf;
            if (fns.get_data)(
                id,
                &addr,
                0,
                core::ptr::null(),
                &mut size,
                cf_out as *mut c_void,
            ) != 0
            {
                continue;
            }
            let name = match NonNull::new(cf as *mut CFString) {
                Some(p) => CFRetained::from_raw(p).to_string(),
                None => continue,
            };
            if name.is_empty() {
                continue;
            }
            if has_buffers(fns, id, SCOPE_OUTPUT) {
                outputs.push(AzString::from(name.as_str()));
            }
            if has_buffers(fns, id, SCOPE_INPUT) {
                inputs.push(AzString::from(name.as_str()));
            }
        }
    }
    (StringVec::from_vec(outputs), StringVec::from_vec(inputs))
}

#[cfg(test)]
mod audio_device_tests {
    use super::AudioDeviceList;

    #[test]
    fn audio_device_enumerate() {
        let list = AudioDeviceList::enumerate();
        let outs: Vec<&str> = list.outputs.as_ref().iter().map(|s| s.as_str()).collect();
        let ins: Vec<&str> = list.inputs.as_ref().iter().map(|s| s.as_str()).collect();
        eprintln!("audio outputs ({}): {:?}", outs.len(), outs);
        eprintln!("audio inputs  ({}): {:?}", ins.len(), ins);
        // Must not panic; on a box with PipeWire/Pulse it lists the sinks/sources.
    }
}
