//! iOS microphone capture via objc2 / AVFoundation. cpal can't cross-compile to
//! iOS (its `coreaudio-sys` bindgen build fails), so the iOS mic mirrors the
//! camera's `AVCaptureSession` + `define_class!` sample-buffer delegate (the
//! push API): the delegate copies each audio `CMSampleBuffer`'s PCM out of its
//! `CMBlockBuffer` into a shared slot, and `mic_read` drains it (push -> pull).
//!
//! Format: the iOS audio HAL delivers Float32 interleaved (no `audioSettings`
//! override), so the block bytes are read directly as f32 - no conversion.

use std::os::raw::c_char;
use std::ptr;
use std::sync::{Arc, Mutex};
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

struct DelegateIvars {
    slot: Arc<Mutex<Vec<f32>>>,
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
            let block = match sample_buffer.data_buffer() {
                Some(b) => b,
                None => return,
            };
            let mut total_len: usize = 0;
            let mut data_ptr: *mut c_char = ptr::null_mut();
            let status = CMBlockBufferGetDataPointer(
                &block,
                0,
                ptr::null_mut(),
                &mut total_len,
                &mut data_ptr,
            );
            if status != 0 || data_ptr.is_null() || total_len < 4 {
                return;
            }
            let n = total_len / 4; // Float32
            let samples = std::slice::from_raw_parts(data_ptr as *const f32, n);
            if let Ok(mut slot) = self.ivars().slot.lock() {
                if slot.len() < MAX_SAMPLES {
                    slot.extend_from_slice(samples);
                }
            }
        }
    }
);

impl AudioDelegate {
    fn new(slot: Arc<Mutex<Vec<f32>>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { slot });
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
/// `rate`/`channels` are advisory (the HAL chooses). `0` on failure (test tone).
pub fn mic_open(_rate: u32, _channels: u16) -> u64 {
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
        let delegate = AudioDelegate::new(slot.clone());
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
