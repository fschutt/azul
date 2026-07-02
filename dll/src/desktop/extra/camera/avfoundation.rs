//! macOS camera capture backend via objc2 / AVFoundation. AVFoundation is a
//! *push* API (a sample-buffer delegate), so a `define_class!` delegate parks
//! the latest frame (converted to RGBA) in a shared slot; the seam's `read`
//! drains it (push -> pull). Plugs into `capture_common::register_camera_backend`
//! like libv4l2 (linux) + nokhwa (windows).
//!
//! We request 32-BGRA from the data output (a single `videoSettings` dict), so
//! the delegate's pixel buffer is always BGRA8 -> a cheap channel swap to RGBA.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_av_foundation::{
    AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceDiscoverySession, AVCaptureDeviceInput,
    AVCaptureDevicePosition, AVCaptureDeviceType, AVCaptureOutput, AVCaptureSession,
    AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaType,
    AVMediaTypeVideo,
};
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

/// kCVPixelFormatType_32BGRA ('BGRA').
const PIXEL_FORMAT_32BGRA: u32 = 0x42475241;

/// Latest captured frame (RGBA + dims), filled by the delegate, drained by read.
#[derive(Default)]
struct FrameSlot {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    /// Bumped each frame; `read` returns the newest available.
    seq: u64,
}

struct DelegateIvars {
    slot: Arc<Mutex<FrameSlot>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AzulCameraDelegate"]
    #[ivars = DelegateIvars]
    struct FrameDelegate;

    unsafe impl NSObjectProtocol for FrameDelegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn capture_output(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            let image = match sample_buffer.image_buffer() {
                Some(i) => i,
                None => return,
            };
            let pb = &*image;
            CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags(0));
            let w = CVPixelBufferGetWidth(pb) as usize;
            let h = CVPixelBufferGetHeight(pb) as usize;
            let stride = CVPixelBufferGetBytesPerRow(pb);
            let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
            if !base.is_null() && w > 0 && h > 0 && stride >= w * 4 {
                let mut rgba = vec![0u8; w * h * 4];
                for y in 0..h {
                    let row = base.add(y * stride);
                    for x in 0..w {
                        let s = row.add(x * 4); // BGRA
                        let o = (y * w + x) * 4;
                        rgba[o] = *s.add(2); // R
                        rgba[o + 1] = *s.add(1); // G
                        rgba[o + 2] = *s; // B
                        rgba[o + 3] = 255;
                    }
                }
                if let Ok(mut slot) = self.ivars().slot.lock() {
                    if slot.seq == 0 {
                        // Log the very first frame only (the callback is hot).
                        crate::plog_info!(
                            "[camera] avfoundation: first frame {}x{} stride={} BGRA→RGBA ok",
                            w, h, stride
                        );
                    }
                    slot.rgba = rgba;
                    slot.width = w as u32;
                    slot.height = h as u32;
                    slot.seq = slot.seq.wrapping_add(1);
                }
            }
            CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags(0));
        }
    }
);

impl FrameDelegate {
    fn new(slot: Arc<Mutex<FrameSlot>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { slot });
        unsafe { msg_send![super(this), init] }
    }
}

/// Live capture state behind the seam's `u64` handle (worker-thread-local).
struct AvfCam {
    session: Retained<AVCaptureSession>,
    _delegate: Retained<FrameDelegate>,
    slot: Arc<Mutex<FrameSlot>>,
    last_seq: u64,
}

/// Read a possibly-NULL `AVCaptureDeviceType` extern static. Device-type
/// constants from newer SDKs (`External` is macOS 14+, `ContinuityCamera`
/// macOS 13+) resolve to NULL at runtime on older systems, so every use goes
/// through this null check instead of trusting the `&'static` type.
unsafe fn devtype_opt(
    s: *const &'static AVCaptureDeviceType,
) -> Option<&'static AVCaptureDeviceType> {
    let raw: *const *const AVCaptureDeviceType = s.cast();
    let v = *raw;
    if v.is_null() {
        None
    } else {
        Some(&*v)
    }
}

/// Pick the capture device for `index` via `AVCaptureDeviceDiscoverySession`
/// (built-in wide angle + external + Continuity cameras — whichever device
/// types this macOS knows about). Graceful fallback: `index` out of range →
/// device 0 → `defaultDeviceWithMediaType`. Logs the enumerated device names
/// (localizedName) once per process.
unsafe fn select_device(media: &AVMediaType, index: u32) -> Option<Retained<AVCaptureDevice>> {
    use objc2_av_foundation::{
        AVCaptureDeviceTypeBuiltInWideAngleCamera, AVCaptureDeviceTypeContinuityCamera,
        AVCaptureDeviceTypeExternal,
    };
    #[allow(deprecated)] // pre-macOS-14 synonym for AVCaptureDeviceTypeExternal
    use objc2_av_foundation::AVCaptureDeviceTypeExternalUnknown;

    #[allow(deprecated)]
    let external_unknown = core::ptr::addr_of!(AVCaptureDeviceTypeExternalUnknown);
    let mut types: Vec<&'static AVCaptureDeviceType> = Vec::new();
    for ptr in [
        core::ptr::addr_of!(AVCaptureDeviceTypeBuiltInWideAngleCamera),
        core::ptr::addr_of!(AVCaptureDeviceTypeExternal),
        external_unknown,
        core::ptr::addr_of!(AVCaptureDeviceTypeContinuityCamera),
    ] {
        if let Some(t) = devtype_opt(ptr) {
            // Dedup External vs ExternalUnknown (same string on macOS 14+
            // would double-count; the discovery session rejects dupes).
            if !types.iter().any(|e| *e == t) {
                types.push(t);
            }
        }
    }

    let devices = if types.is_empty() {
        None
    } else {
        let type_array = NSArray::from_slice(&types);
        let session = AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
            &type_array,
            Some(media),
            AVCaptureDevicePosition::Unspecified,
        );
        Some(session.devices())
    };

    if let Some(devices) = devices {
        let count = devices.count();
        // Log the device list once (open() re-runs on every capture start).
        static LOGGED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        LOGGED.get_or_init(|| {
            let names: Vec<String> = (0..count)
                .map(|i| devices.objectAtIndex(i).localizedName().to_string())
                .collect();
            crate::plog_info!(
                "[camera] avfoundation: {} device(s) discovered: [{}]",
                count,
                names.join(", ")
            );
        });
        if count > 0 {
            let picked = if (index as usize) < count {
                index as usize
            } else {
                crate::plog_warn!(
                    "[camera] avfoundation: device index {} out of range ({} device(s)) → \
                     falling back to device 0",
                    index,
                    count
                );
                0
            };
            return Some(devices.objectAtIndex(picked));
        }
    }
    // No discovery session types resolved / no devices found — last resort.
    AVCaptureDevice::defaultDeviceWithMediaType(media)
}

/// Open the video device at `index`, request BGRA frames, start the session.
/// Returns a boxed handle, or `0` on failure (worker uses the test pattern).
pub fn open(index: u32, _width: u32, _height: u32) -> u64 {
    // TCC gate first: without authorization the session runs but vends only
    // black frames. Blocking (≤60 s prompt wait) is fine on this worker thread.
    if !super::avf_auth::ensure_camera_access() {
        return 0;
    }
    unsafe {
        let media = match AVMediaTypeVideo {
            Some(m) => m,
            None => return 0,
        };
        let device = match select_device(media, index) {
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

        let output = AVCaptureVideoDataOutput::new();
        // Request 32-BGRA so the delegate always gets a packed BGRA8 buffer.
        let key: &NSString = &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
        let val = NSNumber::new_u32(PIXEL_FORMAT_32BGRA);
        let settings: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[key], &[val.as_ref() as &AnyObject]);
        output.setVideoSettings(Some(&settings));
        output.setAlwaysDiscardsLateVideoFrames(true);

        let slot = Arc::new(Mutex::new(FrameSlot::default()));
        let delegate = FrameDelegate::new(slot.clone());
        let queue = dispatch2::DispatchQueue::new("azul.camera", None);
        output.setSampleBufferDelegate_queue(
            Some(ProtocolObject::from_ref(&*delegate)),
            Some(&queue),
        );
        if !session.canAddOutput(&output) {
            return 0;
        }
        session.addOutput(&output);
        session.startRunning();

        let cam = AvfCam {
            session,
            _delegate: delegate,
            slot,
            last_seq: 0,
        };
        crate::plog_info!(
            "[camera] avfoundation: opened device (index {}), requested 32-BGRA → converting to \
             RGBA8",
            index
        );
        Box::into_raw(Box::new(cam)) as u64
    }
}

/// Drain the newest frame into `out` (RGBA8). Spins briefly for the first
/// frame after `open`. Returns `(width, height)`, or `(0, 0)` on error.
pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let cam = match unsafe { (handle as *mut AvfCam).as_mut() } {
        Some(c) => c,
        None => return (0, 0),
    };
    // Wait (bounded) for a frame newer than the last one we returned.
    for _ in 0..120 {
        {
            let slot = match cam.slot.lock() {
                Ok(s) => s,
                Err(_) => return (0, 0),
            };
            if slot.seq != cam.last_seq && slot.width > 0 {
                cam.last_seq = slot.seq;
                out.clear();
                out.extend_from_slice(&slot.rgba);
                return (slot.width, slot.height);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    (0, 0)
}

/// Stop the session + free the capture (drops the boxed `AvfCam`).
pub fn close(handle: u64) {
    if handle != 0 {
        unsafe {
            let cam = Box::from_raw(handle as *mut AvfCam);
            cam.session.stopRunning();
            drop(cam);
        }
    }
}
