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
//! `sink.is_open()`; `sink.error_message()`; dropping the handle (or `close`)
//! stops playback.
//!
//! The output is the platform backend behind the `OutputDevice` seam: ALSA
//! (dlopen'd) on Linux, WASAPI through cpal on Windows, AAudio (dlopen'd) on
//! Android, AVAudioEngine on macOS / iOS (the `objc2-avf-audio` feature). A
//! handle is open only if its device opened: no device, no backend in this
//! build, or a device that refuses the format gives a CLOSED handle whose
//! `error_message()` says why - never an open-looking one that "plays" into
//! nothing. A headless run never reaches the platform (see
//! [`AudioSink::open`]).

use core::ffi::c_void;

use azul_core::audio::{AudioConfig, AudioFrame};
use azul_css::{AzString, OptionString, StringVec};

#[cfg(target_os = "android")]
mod aaudio;
#[cfg(target_os = "linux")]
mod alsa;
#[cfg(any(target_os = "ios", target_os = "macos"))]
mod avfoundation_mic;
#[cfg(all(
    any(target_os = "ios", target_os = "macos"),
    feature = "objc2-avf-audio"
))]
mod avfoundation_sink;
#[cfg(target_os = "windows")]
mod cpal_mic;
#[cfg(target_os = "windows")]
mod cpal_sink;

// Opus voice coding: the `AudioEncoder` / `AudioDecoder` handles. Always
// present (codegen exposes them); open only where the platform ships an Opus
// engine (AudioToolbox on Apple).
pub mod codec;
pub use codec::{AudioDecoder, AudioEncoder};
// The AudioToolbox Opus engine behind them (dlopen'd, like VideoToolbox).
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
mod opus_apple;
// Acoustic echo cancellation: the `EchoCanceller` handle (pure Rust, every
// target).
pub mod echo;
pub use echo::EchoCanceller;
// The arithmetic of playing decoded audio (chunks, rates, channels, gain,
// levels, which track is heard): pure, every target.
pub(crate) mod playback;
// Audio FILES decoded (MP3 / AAC / FLAC / ...): the `AudioFileDecoder` handle,
// on Symphonia (feature `audio-decode`).
pub mod decode;
pub use decode::{AudioFileDecoder, AudioFileInfo, OptionAudioFileInfo};

/// Internal playback state behind an open `AudioSink` handle.
struct AudioSinkInner {
    /// The platform output; `None` for a headless run's synthetic sink, which
    /// takes every frame and plays nothing.
    device: Option<Box<dyn OutputDevice>>,
    /// Frames the output took (see [`AudioSink::frames_played`]).
    frames_played: u64,
    /// The format the sink was opened for.
    config: AudioConfig,
    /// Sample frames (per channel) the output took and did not drop
    /// ([`AudioSink::clear`] takes the dropped ones back off).
    samples_taken: u64,
    /// The synthetic sink plays what it takes in real time, by this clock
    /// (a real device reports what it has queued itself).
    clock: playback::RealTimeClock,
    /// The monotonic origin of `clock`'s times.
    epoch: std::time::Instant,
}

impl AudioSinkInner {
    fn new(device: Option<Box<dyn OutputDevice>>, config: AudioConfig) -> Self {
        Self {
            device,
            frames_played: 0,
            config,
            samples_taken: 0,
            clock: playback::RealTimeClock::new(config.sample_rate),
            epoch: std::time::Instant::now(),
        }
    }

    /// Seconds since the sink opened (the synthetic clock's time).
    fn now_s(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// Sample frames taken and not heard yet: the device's word, or the
    /// synthetic sink's real-time clock; a device that cannot tell has none.
    fn queued(&self) -> u64 {
        match &self.device {
            Some(device) => device
                .queued_frames()
                .unwrap_or(0)
                .min(self.samples_taken),
            None => self
                .samples_taken
                .saturating_sub(self.clock.played(self.samples_taken, self.now_s())),
        }
    }
}

/// One open platform output stream (ALSA, cpal / WASAPI, AAudio,
/// AVAudioEngine): the seam between the `AudioSink` handle and the device.
/// Each backend's `open` returns one of these, or a readable reason why not.
trait OutputDevice {
    /// Hands interleaved f32 `samples` to the device. False when the device
    /// did not take them (its queue is full, the write failed): that frame
    /// was never heard, so `frames_played` does not count it.
    fn play(&self, samples: &[f32]) -> bool;

    /// Sample frames (per channel) handed to the device and not heard yet;
    /// `None` when the device cannot tell (a blocking write that returns once
    /// the frames are nearly out): the sink then counts them as heard.
    fn queued_frames(&self) -> Option<u64> {
        None
    }

    /// Holds (`true`) or resumes (`false`) playback, keeping what is queued.
    /// False when this device cannot pause (the caller stops feeding it).
    fn set_paused(&self, paused: bool) -> bool {
        let _ = paused;
        false
    }

    /// Drops what is queued and not heard yet (a seek). False when this
    /// device cannot (what it has queued plays out).
    fn clear(&self) -> bool {
        false
    }
}

/// This build's output device for `config`, or a readable reason there is
/// none (no device, no backend, or the device refuses the format).
fn platform_output(config: AudioConfig) -> Result<Box<dyn OutputDevice>, String> {
    #[cfg(target_os = "linux")]
    {
        alsa::AlsaPcm::open(config.sample_rate, u32::from(config.channels))
            .map(|pcm| Box::new(pcm) as Box<dyn OutputDevice>)
    }
    #[cfg(target_os = "windows")]
    {
        cpal_sink::CpalSink::open(config.sample_rate, config.channels)
            .map(|sink| Box::new(sink) as Box<dyn OutputDevice>)
    }
    #[cfg(target_os = "android")]
    {
        aaudio::AAudioSink::open(config.sample_rate, config.channels)
            .map(|sink| Box::new(sink) as Box<dyn OutputDevice>)
    }
    #[cfg(all(
        any(target_os = "ios", target_os = "macos"),
        feature = "objc2-avf-audio"
    ))]
    {
        avfoundation_sink::AvfSink::open(config.sample_rate, config.channels)
            .map(|sink| Box::new(sink) as Box<dyn OutputDevice>)
    }
    #[cfg(all(
        any(target_os = "ios", target_os = "macos"),
        not(feature = "objc2-avf-audio")
    ))]
    {
        let _ = config;
        Err(String::from(
            "this build has no audio output (the dll was built without the objc2-avf-audio \
             feature)",
        ))
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "android",
        target_os = "ios",
        target_os = "macos"
    )))]
    {
        let _ = config;
        Err(String::from(
            "this platform has no audio output backend in azul yet",
        ))
    }
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
    /// Why the sink did not open ([`AudioSink::error_message`]); `None` while
    /// open, after `close` and on a default handle. Sits before the 1-byte
    /// `run_destructor` so the struct has no padding between fields.
    pub error: OptionString,
    /// Whether this handle owns (and on drop frees) the engine resource.
    pub run_destructor: bool,
}

impl Clone for AudioSink {
    fn clone(&self) -> Self {
        // Non-owning shallow handle copy - only the original frees the engine
        // (the FFI handle convention). The reason is plain data: copied.
        AudioSink {
            ptr: self.ptr,
            error: self.error.clone(),
            run_destructor: false,
        }
    }
}

impl Default for AudioSink {
    fn default() -> Self {
        AudioSink {
            ptr: core::ptr::null_mut(),
            error: OptionString::None,
            run_destructor: false,
        }
    }
}

impl AudioSink {
    /// Open an audio output for `config` (sample rate + channels). The handle
    /// is open (`is_open()`) only if an output device opened; otherwise it is
    /// closed and [`error_message`](Self::error_message) says why: no output
    /// device, no audio backend in this build, or a device that refuses the
    /// format. Playing into a closed handle does nothing.
    ///
    /// A headless / e2e run never opens the real output: it gets a sink that
    /// counts frames and plays nothing if it asked for one
    /// (`AZ_SYNTHETIC_DEVICES=audio_sink`, or the `mock` op), else a closed
    /// handle, recorded as "not available in a headless run".
    pub fn open(config: AudioConfig) -> AudioSink {
        use azul_layout::request::mock::{self, DeviceKind, MockDevice};
        let device = mock::device(DeviceKind::AudioSink);
        if device == MockDevice::Unavailable {
            mock::record_unavailable_device(DeviceKind::AudioSink);
        }
        Self::open_as(config, device)
    }

    /// [`open`](Self::open) once the mock store has decided what `open`
    /// resolves to (split out so tests need not arm the process-wide store).
    fn open_as(config: AudioConfig, device: azul_layout::request::mock::MockDevice) -> AudioSink {
        use azul_layout::request::mock::{DeviceKind, MockDevice};
        match device {
            MockDevice::Real => Self::open_on(config, platform_output(config)),
            MockDevice::Unavailable => Self::closed(DeviceKind::AudioSink.unavailable_message()),
            MockDevice::Synthetic => {
                crate::plog_info!(
                    "[audio] headless run: a synthetic sink that counts frames, no device \
                     ({}Hz x{}ch)",
                    config.sample_rate,
                    config.channels
                );
                Self::from_inner(AudioSinkInner::new(None, config))
            }
        }
    }

    /// A sink on `output`: the platform device, or why there is none. The
    /// seam the tests open sinks through (a real device cannot be made to
    /// fail on demand).
    fn open_on(config: AudioConfig, output: Result<Box<dyn OutputDevice>, String>) -> AudioSink {
        match output {
            Ok(device) => {
                crate::plog_info!(
                    "[audio] sink open: {}Hz x{}ch (f32 interleaved)",
                    config.sample_rate,
                    config.channels
                );
                Self::from_inner(AudioSinkInner::new(Some(device), config))
            }
            Err(why) => {
                crate::plog_warn!(
                    "[audio] AudioSink::open ({}Hz x{}ch): {} - the handle is closed \
                     (is_open() false), nothing will play",
                    config.sample_rate,
                    config.channels,
                    why
                );
                Self::closed(why)
            }
        }
    }

    /// A closed handle that says `why`.
    fn closed(why: String) -> AudioSink {
        AudioSink {
            ptr: core::ptr::null_mut(),
            error: OptionString::Some(AzString::from(why)),
            run_destructor: false,
        }
    }

    fn from_inner(inner: AudioSinkInner) -> AudioSink {
        AudioSink {
            ptr: Box::into_raw(Box::new(inner)) as *mut c_void,
            error: OptionString::None,
            run_destructor: true,
        }
    }

    /// Whether the sink is open: an output device opened (or a headless run
    /// asked for the synthetic sink) and `close` has not been called.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Hands `frame` (interleaved `f32` samples in the sink's format) to the
    /// output and says whether it took it: false on a closed handle and when
    /// the device's queue is full (offer the same frame again a moment later:
    /// a player paces itself with [`queued_frames`](Self::queued_frames)).
    pub fn try_play(&self, frame: AudioFrame) -> bool {
        let Some(inner) = (unsafe { (self.ptr as *mut AudioSinkInner).as_mut() }) else {
            return false;
        };
        let samples: &[f32] = frame.samples.as_ref();
        let frames = (samples.len() / usize::from(inner.config.channels.max(1))) as u64;
        let taken = match &inner.device {
            Some(device) => device.play(samples),
            // A headless run's synthetic sink: takes every frame and plays
            // it in real time (nothing is heard).
            None => {
                let now = inner.now_s();
                inner.clock.on_take(inner.samples_taken, now);
                true
            }
        };
        if taken {
            inner.frames_played = inner.frames_played.wrapping_add(1);
            inner.samples_taken = inner.samples_taken.saturating_add(frames);
        }
        taken
    }

    /// Sample frames (per channel) the output took and has not played yet:
    /// how far ahead of the listener the app is. `0` on a closed handle, and
    /// for a device that cannot tell (its writes block until the frames are
    /// nearly out). The headless synthetic sink plays in real time, so its
    /// queue empties as the wall clock runs.
    pub fn queued_frames(&self) -> u64 {
        unsafe { (self.ptr as *const AudioSinkInner).as_ref() }.map_or(0, AudioSinkInner::queued)
    }

    /// Sample frames (per channel) the listener has heard so far: what the
    /// output took minus what it still has queued. A media clock: divide by
    /// the sample rate for seconds (an audio track is the master clock a
    /// video follows). `0` on a closed handle.
    pub fn samples_played(&self) -> u64 {
        unsafe { (self.ptr as *const AudioSinkInner).as_ref() }
            .map_or(0, |inner| inner.samples_taken.saturating_sub(inner.queued()))
    }

    /// Holds playback where it is, keeping what is queued (the position
    /// stops). False when the device cannot pause: stop feeding it instead,
    /// what is queued plays out.
    pub fn pause(&self) -> bool {
        let Some(inner) = (unsafe { (self.ptr as *mut AudioSinkInner).as_mut() }) else {
            return false;
        };
        match &inner.device {
            Some(device) => device.set_paused(true),
            None => {
                let now = inner.now_s();
                inner.clock.pause(inner.samples_taken, now);
                true
            }
        }
    }

    /// Plays on after [`pause`](Self::pause).
    pub fn resume(&self) {
        let Some(inner) = (unsafe { (self.ptr as *mut AudioSinkInner).as_mut() }) else {
            return;
        };
        match &inner.device {
            Some(device) => {
                let _ = device.set_paused(false);
            }
            None => {
                let now = inner.now_s();
                inner.clock.resume(now);
            }
        }
    }

    /// Drops what is queued and not heard yet (a seek: the old position must
    /// not play on); [`samples_played`](Self::samples_played) stays where the
    /// listener is. False when the device cannot drop its queue.
    pub fn clear(&self) -> bool {
        let Some(inner) = (unsafe { (self.ptr as *mut AudioSinkInner).as_mut() }) else {
            return false;
        };
        let queued = inner.queued();
        let dropped = match &inner.device {
            Some(device) => device.clear(),
            None => true,
        };
        if dropped {
            // The dropped frames were never heard: what remains taken is
            // exactly what was played, so `samples_played` does not move.
            inner.samples_taken = inner.samples_taken.saturating_sub(queued);
            if inner.device.is_none() {
                inner.clock.clear(inner.samples_taken);
            }
        }
        dropped
    }

    /// The format the sink was opened for (`AudioConfig::default()` on a
    /// closed handle).
    pub fn config(&self) -> AudioConfig {
        unsafe { (self.ptr as *const AudioSinkInner).as_ref() }
            .map_or_else(AudioConfig::default, |inner| inner.config)
    }

    /// Hands `frame` (interleaved `f32` samples in the frame's format) to the
    /// output device. Does nothing on a closed handle.
    pub fn play(&self, frame: AudioFrame) {
        let _ = self.try_play(frame);
    }

    /// Frames the output device took from [`play`](Self::play) so far. A
    /// frame the device did not take (its queue full, the write failed) is
    /// not counted, and a closed handle counts nothing (`0`). A headless
    /// run's synthetic sink counts every frame.
    pub fn frames_played(&self) -> u64 {
        unsafe { (self.ptr as *const AudioSinkInner).as_ref() }
            .map(|i| i.frames_played)
            .unwrap_or(0)
    }

    /// Why this sink is not open, readable enough to show the user: no output
    /// device, no audio backend in this build, the device refused the
    /// format, or a headless run without the synthetic sink. `None` while the
    /// sink is open, after an explicit `close`, and on a default handle.
    pub fn error_message(&self) -> OptionString {
        self.error.clone()
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

/// Result of [`AudioDeviceList::enumerate`].
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AudioDeviceListResult {
    pub devices: AudioDeviceList,
}

azul_css::impl_option!(
    AudioDeviceListResult,
    OptionAudioDeviceListResult,
    copy = false,
    [Debug, Clone]
);

impl AudioDeviceListResult {
    /// Downcast the `result` RefAny delivered to a `ResumeCallback`.
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionAudioDeviceListResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

impl AudioDeviceList {
    /// Enumerate the machine's audio devices and resume `on_result` with an
    /// [`AudioDeviceListResult`]. Linux: PipeWire/PulseAudio via
    /// `pactl list short sinks/sources`; macOS: the CoreAudio HAL
    /// (`AudioObjectGetPropertyData` on the system object, dlopen'd at runtime —
    /// no link-time dep); Windows: WASAPI endpoints through cpal; empty on
    /// platforms without an enumeration backend yet (and if `pactl` isn't
    /// installed / CoreAudio can't be loaded).
    pub fn enumerate(
        data: azul_core::refany::RefAny,
        on_result: azul_layout::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let devices = Self::enumerate_blocking();
        azul_layout::request::complete(data, on_result, AudioDeviceListResult { devices })
    }

    /// The synchronous enumeration behind [`Self::enumerate`] (Rust-internal;
    /// `enumerate` is the API on every target because the browser's
    /// `enumerateDevices()` is asynchronous).
    pub fn enumerate_blocking() -> AudioDeviceList {
        // Deterministic under e2e: the mock store's lists, or empty lists
        // (recorded as unmocked) - never the machine's real devices.
        match azul_layout::request::mock::take_audio_devices() {
            azul_layout::request::mock::Answer::NotArmed => {}
            azul_layout::request::mock::Answer::Mocked((outputs, inputs)) => {
                return AudioDeviceList {
                    outputs: StringVec::from_vec(outputs),
                    inputs: StringVec::from_vec(inputs),
                };
            }
            azul_layout::request::mock::Answer::Unmocked => {
                return AudioDeviceList {
                    outputs: StringVec::from_vec(Vec::new()),
                    inputs: StringVec::from_vec(Vec::new()),
                };
            }
        }
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
        #[cfg(target_os = "windows")]
        {
            use cpal::traits::{DeviceTrait, HostTrait};
            let host = cpal::default_host();
            let outputs: Vec<AzString> = host
                .output_devices()
                .map(|list| list.filter_map(|d| d.name().ok()).map(AzString::from).collect())
                .unwrap_or_default();
            let inputs: Vec<AzString> = host
                .input_devices()
                .map(|list| list.filter_map(|d| d.name().ok()).map(AzString::from).collect())
                .unwrap_or_default();
            AudioDeviceList {
                outputs: StringVec::from_vec(outputs),
                inputs: StringVec::from_vec(inputs),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
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
                "/System/Library/Frameworks/CoreAudio.framework/CoreAudio"
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
mod headless_sink_tests {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    };

    use azul_core::audio::{AudioConfig, AudioFrame};
    use azul_css::{F32Vec, OptionString};
    use azul_layout::request::mock::MockDevice;

    use super::{AudioSink, OutputDevice};

    const CONFIG: AudioConfig = AudioConfig {
        sample_rate: 48_000,
        channels: 1,
    };

    fn chunk() -> AudioFrame {
        AudioFrame {
            sample_rate: 48_000,
            channels: 1,
            samples: F32Vec::from_vec(vec![0.25; 960]),
        }
    }

    /// A headless run that did not ask for a synthetic audio output opens
    /// none: the handle says so (`is_open()` false), and playing into it is a
    /// no-op, never a sound.
    #[test]
    fn a_headless_audio_sink_opens_no_device_and_is_not_open() {
        let sink = AudioSink::open_as(CONFIG, MockDevice::Unavailable);
        assert!(!sink.is_open());
        sink.play(chunk());
        assert_eq!(sink.frames_played(), 0);
    }

    /// A headless run that asked for one gets a sink that counts what it is
    /// given and plays nothing.
    #[test]
    fn a_synthetic_audio_sink_counts_the_frames_it_is_given() {
        let mut sink = AudioSink::open_as(CONFIG, MockDevice::Synthetic);
        assert!(sink.is_open());
        for _ in 0..3 {
            sink.play(chunk());
        }
        assert_eq!(sink.frames_played(), 3);
        sink.close();
        assert!(!sink.is_open());
    }

    /// The reason a sink gives for not being open, as plain text.
    fn reason(sink: &AudioSink) -> Option<String> {
        match sink.error_message() {
            OptionString::Some(why) => Some(why.as_str().to_string()),
            OptionString::None => None,
        }
    }

    /// A stand-in output device: takes every other frame (its queue is full
    /// for the rest) and counts every frame it is handed.
    struct TakesEveryOther {
        handed: Arc<AtomicU32>,
    }

    impl OutputDevice for TakesEveryOther {
        fn play(&self, samples: &[f32]) -> bool {
            assert_eq!(samples.len(), 960, "the frame's samples reach the device");
            let n = self.handed.fetch_add(1, Ordering::SeqCst) + 1;
            n % 2 == 1
        }
    }

    /// An output that does not open (no device, no backend in this build, or
    /// the device refuses the format) gives a CLOSED handle that says why -
    /// never an open-looking one that plays into nothing.
    #[test]
    fn a_device_that_does_not_open_gives_a_closed_handle_that_says_why() {
        let why = "the output device refused 48000 Hz x 1 f32";
        let sink = AudioSink::open_on(CONFIG, Err(String::from(why)));
        assert!(!sink.is_open());
        assert_eq!(reason(&sink).as_deref(), Some(why));
        sink.play(chunk());
        assert_eq!(sink.frames_played(), 0);
        // A copy of the handle carries the reason too.
        assert_eq!(reason(&sink.clone()).as_deref(), Some(why));
    }

    /// `frames_played` counts only the frames a device TOOK: a frame the
    /// device did not take (its queue full, the write failed) was never
    /// heard, so it is not counted.
    #[test]
    fn frames_played_counts_only_the_frames_the_device_took() {
        let handed = Arc::new(AtomicU32::new(0));
        let device: Box<dyn OutputDevice> = Box::new(TakesEveryOther {
            handed: handed.clone(),
        });
        let mut sink = AudioSink::open_on(CONFIG, Ok(device));
        assert!(sink.is_open());
        assert_eq!(reason(&sink), None);
        for _ in 0..4 {
            sink.play(chunk());
        }
        assert_eq!(
            handed.load(Ordering::SeqCst),
            4,
            "every frame reached the device"
        );
        assert_eq!(sink.frames_played(), 2, "only the two it took count");
        sink.close();
        assert!(!sink.is_open());
        assert_eq!(reason(&sink), None, "closing on purpose is not an error");
    }

    /// `try_play` says whether the output took the frame: a full device queue
    /// is "not taken", so a player offers the same frame again.
    #[test]
    fn try_play_says_whether_the_output_took_the_frame() {
        let handed = Arc::new(AtomicU32::new(0));
        let device: Box<dyn OutputDevice> = Box::new(TakesEveryOther {
            handed: handed.clone(),
        });
        let sink = AudioSink::open_on(CONFIG, Ok(device));
        assert!(sink.try_play(chunk()));
        assert!(!sink.try_play(chunk()));
        assert!(sink.try_play(chunk()));
        assert!(!AudioSink::default().try_play(chunk()), "a closed sink takes nothing");
        assert_eq!(sink.config(), CONFIG);
    }

    /// The headless synthetic sink plays what it takes in real time: right
    /// after a frame it is queued, once the wall clock ran past it it is
    /// heard - so a headless player's position moves like a real one's.
    #[test]
    fn a_synthetic_sink_plays_what_it_takes_in_real_time() {
        let sink = AudioSink::open_as(CONFIG, MockDevice::Synthetic);
        // 4800 frames: 100 ms at 48 kHz.
        let frame = AudioFrame {
            sample_rate: 48_000,
            channels: 1,
            samples: F32Vec::from_vec(vec![0.25; 4800]),
        };
        assert!(sink.try_play(frame));
        assert!(
            sink.queued_frames() >= 2400,
            "most of it is still queued: {}",
            sink.queued_frames()
        );
        assert!(sink.samples_played() <= 2400);
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert_eq!(sink.queued_frames(), 0);
        assert_eq!(sink.samples_played(), 4800);
    }

    /// Paused, the synthetic sink's position holds; cleared, what was queued
    /// is gone and the position stays where the listener was.
    #[test]
    fn a_paused_synthetic_sink_holds_and_a_cleared_one_drops_its_queue() {
        let sink = AudioSink::open_as(CONFIG, MockDevice::Synthetic);
        let second = AudioFrame {
            sample_rate: 48_000,
            channels: 1,
            samples: F32Vec::from_vec(vec![0.25; 48_000]),
        };
        assert!(sink.try_play(second));
        assert!(sink.pause());
        let held = sink.samples_played();
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(sink.samples_played(), held, "paused: the position holds");
        assert!(sink.clear());
        assert_eq!(sink.queued_frames(), 0, "cleared: nothing queued");
        assert_eq!(sink.samples_played(), held, "cleared: the listener is where they were");
        sink.resume();
    }

    /// A device that reports its queue: the sink reports the same queue, a
    /// pause and a clear reach the device, and a clear takes the dropped
    /// frames back off what was taken, so the played count never jumps.
    #[test]
    fn a_device_queue_pause_and_clear_reach_the_device_and_played_never_jumps() {
        let state = Arc::new(Mutex::new(QueueState::default()));
        let device: Box<dyn OutputDevice> = Box::new(Queueing {
            state: state.clone(),
        });
        let sink = AudioSink::open_on(CONFIG, Ok(device));
        assert!(sink.try_play(chunk()));
        assert!(sink.try_play(chunk()));
        // 1920 frames taken; the device has played 500 of them.
        state.lock().unwrap().queued = 1420;
        assert_eq!(sink.queued_frames(), 1420);
        assert_eq!(sink.samples_played(), 500);
        assert!(sink.pause());
        assert_eq!(state.lock().unwrap().paused, Some(true));
        sink.resume();
        assert_eq!(state.lock().unwrap().paused, Some(false));
        assert!(sink.clear());
        assert!(state.lock().unwrap().cleared);
        assert_eq!(sink.queued_frames(), 0);
        assert_eq!(sink.samples_played(), 500, "the dropped frames were never heard");
        assert!(sink.try_play(chunk()));
        state.lock().unwrap().queued = 960;
        assert_eq!(sink.samples_played(), 500);
    }

    #[derive(Default)]
    struct QueueState {
        queued: u64,
        paused: Option<bool>,
        cleared: bool,
    }

    /// A stand-in device that reports a queue the test sets, and records a
    /// pause and a clear.
    struct Queueing {
        state: Arc<Mutex<QueueState>>,
    }

    impl OutputDevice for Queueing {
        fn play(&self, samples: &[f32]) -> bool {
            self.state.lock().unwrap().queued += samples.len() as u64;
            true
        }
        fn queued_frames(&self) -> Option<u64> {
            Some(self.state.lock().unwrap().queued)
        }
        fn set_paused(&self, paused: bool) -> bool {
            self.state.lock().unwrap().paused = Some(paused);
            true
        }
        fn clear(&self) -> bool {
            let mut s = self.state.lock().unwrap();
            s.queued = 0;
            s.cleared = true;
            true
        }
    }

    /// A headless run that opens no audio output says why and how to get the
    /// synthetic stand-in; the stand-in is open and has nothing to report.
    #[test]
    fn a_headless_sink_that_opens_nothing_says_why() {
        let sink = AudioSink::open_as(CONFIG, MockDevice::Unavailable);
        let why = reason(&sink).expect("an unavailable sink says why");
        assert!(why.contains("not available in a headless run"), "{why}");
        assert!(why.contains("AZ_SYNTHETIC_DEVICES=audio_sink"), "{why}");

        let synthetic = AudioSink::open_as(CONFIG, MockDevice::Synthetic);
        assert!(synthetic.is_open());
        assert_eq!(reason(&synthetic), None);
        assert_eq!(reason(&AudioSink::default()), None);
    }
}

#[cfg(test)]
mod audio_device_tests {
    use super::AudioDeviceList;

    #[test]
    fn audio_device_enumerate() {
        let list = AudioDeviceList::enumerate_blocking();
        let outs: Vec<&str> = list.outputs.as_ref().iter().map(|s| s.as_str()).collect();
        let ins: Vec<&str> = list.inputs.as_ref().iter().map(|s| s.as_str()).collect();
        eprintln!("audio outputs ({}): {:?}", outs.len(), outs);
        eprintln!("audio inputs  ({}): {:?}", ins.len(), ins);
        // Must not panic; on a box with PipeWire/Pulse it lists the sinks/sources.
    }
}
