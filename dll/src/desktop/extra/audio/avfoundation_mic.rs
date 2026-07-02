//! Apple (iOS + macOS) microphone capture via objc2 / AVFoundation. cpal can't
//! cross-compile to iOS (its `coreaudio-sys` bindgen build fails), so the mic
//! mirrors the camera's `AVCaptureSession` + `define_class!` sample-buffer
//! delegate (the push API): the delegate copies each audio `CMSampleBuffer`'s
//! PCM out of its `CMBlockBuffer` into a shared slot, and `mic_read` drains it
//! (push -> pull).
//!
//! Format: the delegate does NOT assume Float32 — it reads the actual
//! `AudioStreamBasicDescription` from the sample buffer's format description
//! (`CMAudioFormatDescriptionGetStreamBasicDescription`) and converts:
//! Float32 pass-through, SInt16/SInt32 → f32. Non-interleaved buffers
//! concatenate their channel planes in the block buffer, so only the first
//! plane is taken; interleaved multi-channel is downmixed to mono (average)
//! when the widget opened with `channels == 1`. The getter is feature-gated
//! out of objc2-core-media (no `objc2-core-audio-types`), so on macOS it's
//! dlsym'd from CoreMedia at runtime via `libloading` (same pattern as
//! `camera/v4l2.rs` — no link-time dep); iOS has no `libloading`, so it keeps
//! the historical Float32-interleaved assumption (what the iOS HAL delivers).

use std::os::raw::c_char;
use std::ptr;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_av_foundation::{
    AVCaptureAudioDataOutput, AVCaptureAudioDataOutputSampleBufferDelegate, AVCaptureConnection,
    AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession, AVMediaTypeAudio,
};
use objc2_core_media::{CMBlockBufferGetDataPointer, CMSampleBuffer};
use objc2_foundation::{NSObject, NSObjectProtocol};

/// Cap the backlog at ~4 s of 48 kHz mono so a slow reader can't grow it forever.
const MAX_SAMPLES: usize = 48_000 * 4;

// `AudioStreamBasicDescription.mFormatFlags` bits (CoreAudioTypes.h).
const K_AUDIO_FORMAT_FLAG_IS_FLOAT: u32 = 0x1;
const K_AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER: u32 = 0x4;
const K_AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED: u32 = 0x20;

/// `AudioStreamBasicDescription` (CoreAudioTypes.h), hand-transcribed because
/// objc2-core-media's binding is behind its `objc2-core-audio-types` feature
/// (not enabled for this crate).
#[repr(C)]
#[derive(Clone, Copy)]
struct Asbd {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

/// Runtime-resolved `CMAudioFormatDescriptionGetStreamBasicDescription`
/// (macOS only — dlsym from CoreMedia.framework, no link-time dep).
#[cfg(target_os = "macos")]
fn cm_get_asbd() -> Option<unsafe extern "C" fn(*const core::ffi::c_void) -> *const Asbd> {
    use std::sync::OnceLock;
    type GetAsbd = unsafe extern "C" fn(*const core::ffi::c_void) -> *const Asbd;
    static CM: OnceLock<Option<(libloading::Library, GetAsbd)>> = OnceLock::new();
    CM.get_or_init(|| unsafe {
        let lib = crate::desktop::open_first_lib(&[
            "/System/Library/Frameworks/CoreMedia.framework/CoreMedia",
        ])?;
        let f: GetAsbd = *lib
            .get(b"CMAudioFormatDescriptionGetStreamBasicDescription\0")
            .ok()?;
        Some((lib, f))
    })
    .as_ref()
    .map(|(_, f)| *f)
}

/// Read the actual PCM format of `sample_buffer` (macOS). `None` if CoreMedia
/// couldn't be resolved / the buffer has no audio format description.
#[cfg(target_os = "macos")]
unsafe fn read_asbd(sample_buffer: &CMSampleBuffer) -> Option<Asbd> {
    let getter = cm_get_asbd()?;
    let fd = unsafe { sample_buffer.format_description() }?;
    let fd_ptr: *const objc2_core_media::CMFormatDescription = &*fd;
    let p = unsafe { getter(fd_ptr.cast()) };
    if p.is_null() {
        None
    } else {
        Some(unsafe { *p })
    }
}

/// iOS: no `libloading` in the dependency graph — keep the documented
/// Float32-interleaved HAL assumption (pass-through path below).
#[cfg(not(target_os = "macos"))]
unsafe fn read_asbd(_sample_buffer: &CMSampleBuffer) -> Option<Asbd> {
    None
}

/// Convert the block-buffer `bytes` (format per `asbd`, or Float32 interleaved
/// when `None`) to mono-mixable f32 appended to `out`. Non-interleaved input
/// takes the first channel plane only; interleaved multi-channel is averaged
/// down to mono when the widget asked for `want_channels == 1`.
fn convert_samples(bytes: &[u8], asbd: Option<&Asbd>, want_channels: u16, out: &mut Vec<f32>) {
    let (is_float, bits, channels, non_interleaved) = match asbd {
        Some(a) => (
            a.format_flags & K_AUDIO_FORMAT_FLAG_IS_FLOAT != 0,
            a.bits_per_channel,
            a.channels_per_frame.max(1) as usize,
            a.format_flags & K_AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED != 0,
        ),
        // No format info: historical Float32-interleaved assumption.
        None => (true, 32, 1, false),
    };
    // Refuse formats we can't decode (e.g. unsigned int, 24-bit packed).
    let is_int = asbd
        .map(|a| a.format_flags & K_AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER != 0)
        .unwrap_or(false);
    if !is_float && !is_int {
        return;
    }

    // Non-interleaved: the CMBlockBuffer concatenates the channel planes, so
    // plane 0 (= mono view of channel 0) is the first total/channels bytes.
    let usable = if non_interleaved {
        &bytes[..bytes.len() / channels]
    } else {
        bytes
    };

    let mut f32s: Vec<f32> = match (is_float, bits) {
        (true, 32) => usable
            .chunks_exact(4)
            .map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        (false, 16) => usable
            .chunks_exact(2)
            .map(|c| i16::from_ne_bytes([c[0], c[1]]) as f32 / 32_768.0)
            .collect(),
        (false, 32) => usable
            .chunks_exact(4)
            .map(|c| i32::from_ne_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0)
            .collect(),
        _ => return, // 24-bit packed / float64: not produced by the HAL in practice.
    };

    // Downmix interleaved multi-channel to mono by averaging, but only if the
    // widget opened mono (the open() args are advisory - stored for this).
    if !non_interleaved && channels > 1 && want_channels == 1 {
        f32s = f32s
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
    }
    out.append(&mut f32s);
}

struct DelegateIvars {
    slot: Arc<Mutex<Vec<f32>>>,
    /// Advisory rate/channels from `mic_open` (the HAL picks the real format;
    /// `want_channels == 1` triggers the mono downmix in the delegate).
    want_rate: u32,
    want_channels: u16,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AzulMicDelegate"]
    #[ivars = DelegateIvars]
    struct AudioDelegate;

    unsafe impl NSObjectProtocol for AudioDelegate {}

    unsafe impl AVCaptureAudioDataOutputSampleBufferDelegate for AudioDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn capture_output(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            let block = match unsafe { sample_buffer.data_buffer() } {
                Some(b) => b,
                None => return,
            };
            let mut total_len: usize = 0;
            let mut data_ptr: *mut c_char = ptr::null_mut();
            let status = unsafe {
                CMBlockBufferGetDataPointer(
                    &block,
                    0,
                    ptr::null_mut(),
                    &mut total_len,
                    &mut data_ptr,
                )
            };
            if status != 0 || data_ptr.is_null() || total_len < 2 {
                return;
            }
            let asbd = unsafe { read_asbd(sample_buffer) };

            // Log the format the HAL actually delivers, once.
            static FMT_LOGGED: Once = Once::new();
            FMT_LOGGED.call_once(|| match &asbd {
                Some(a) => crate::plog_info!(
                    "[audio] mic format: {}Hz x{}ch {}bit float={} interleaved={} \
                     (advisory: {}Hz x{}ch)",
                    a.sample_rate,
                    a.channels_per_frame,
                    a.bits_per_channel,
                    a.format_flags & K_AUDIO_FORMAT_FLAG_IS_FLOAT != 0,
                    a.format_flags & K_AUDIO_FORMAT_FLAG_IS_NON_INTERLEAVED == 0,
                    self.ivars().want_rate,
                    self.ivars().want_channels,
                ),
                None => crate::plog_info!(
                    "[audio] mic format: unknown (no ASBD) - assuming Float32 interleaved"
                ),
            });

            let bytes = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, total_len) };
            if let Ok(mut slot) = self.ivars().slot.lock() {
                if slot.len() < MAX_SAMPLES {
                    convert_samples(
                        bytes,
                        asbd.as_ref(),
                        self.ivars().want_channels,
                        &mut slot,
                    );
                }
            }
        }
    }
);

impl AudioDelegate {
    fn new(slot: Arc<Mutex<Vec<f32>>>, want_rate: u32, want_channels: u16) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            slot,
            want_rate,
            want_channels,
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// Live capture state behind the seam's `u64` handle (mic-worker-thread-local).
struct AvfMic {
    session: Retained<AVCaptureSession>,
    _delegate: Retained<AudioDelegate>,
    slot: Arc<Mutex<Vec<f32>>>,
}

/// Open the default audio input + start an AVCaptureSession feeding the delegate.
/// `rate`/`channels` are advisory (the HAL chooses the delivery format; they're
/// stored so the delegate can downmix to mono when `channels == 1`). `0` on
/// failure (test tone).
pub fn mic_open(rate: u32, channels: u16) -> u64 {
    // TCC gate first: without authorization the session runs but vends only
    // silence. Blocking (≤60 s prompt wait) is fine on this worker thread.
    // The helper lives with the camera backend (same AVCaptureDevice API).
    #[cfg(feature = "objc2-av-foundation")]
    if !crate::desktop::extra::camera::avf_auth::ensure_mic_access() {
        return 0;
    }
    unsafe {
        let media = match AVMediaTypeAudio {
            Some(m) => m,
            None => return 0,
        };
        let device = match AVCaptureDevice::defaultDeviceWithMediaType(media) {
            Some(d) => d,
            None => return 0,
        };
        let input = match AVCaptureDeviceInput::deviceInputWithDevice_error(&device) {
            Ok(i) => i,
            Err(_) => return 0,
        };
        let session = AVCaptureSession::new();
        if !session.canAddInput(&input) {
            return 0;
        }
        session.addInput(&input);

        let output = AVCaptureAudioDataOutput::new();
        let slot = Arc::new(Mutex::new(Vec::<f32>::new()));
        let delegate = AudioDelegate::new(slot.clone(), rate, channels);
        let queue = dispatch2::DispatchQueue::new("azul.mic", None);
        output.setSampleBufferDelegate_queue(
            Some(ProtocolObject::from_ref(&*delegate)),
            Some(&queue),
        );
        if !session.canAddOutput(&output) {
            return 0;
        }
        session.addOutput(&output);
        session.startRunning();

        Box::into_raw(Box::new(AvfMic {
            session,
            _delegate: delegate,
            slot,
        })) as u64
    }
}

/// Drain captured f32 samples into `out`. Spins briefly for the first buffer.
/// Returns the sample count, or `0` if none yet (the worker retries).
pub fn mic_read(handle: u64, out: &mut Vec<f32>) -> u32 {
    let mic = match unsafe { (handle as *const AvfMic).as_ref() } {
        Some(m) => m,
        None => return 0,
    };
    for _ in 0..120 {
        if let Ok(mut slot) = mic.slot.lock() {
            if !slot.is_empty() {
                out.clear();
                out.append(&mut slot);
                return out.len() as u32;
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    0
}

/// Stop the session + free the capture (drops the boxed `AvfMic`).
pub fn mic_close(handle: u64) {
    if handle != 0 {
        unsafe {
            let mic = Box::from_raw(handle as *mut AvfMic);
            mic.session.stopRunning();
            drop(mic);
        }
    }
}
