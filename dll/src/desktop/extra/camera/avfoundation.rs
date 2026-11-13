//! macOS camera capture backend via objc2 / AVFoundation. AVFoundation is a
//! *push* API (a sample-buffer delegate), so a `define_class!` delegate parks
//! the latest frame in a shared slot; the seam's `read` takes it (push ->
//! pull). Plugs into `capture_common::register_camera_backend` like libv4l2
//! (linux) + nokhwa (windows).
//!
//! The data output is asked for the format the widget wants, in the form the
//! camera already produces it: NV12 ('420v' / '420f', a camera's native
//! 4:2:0 - what the H.264 encoder and a YUV tile take) or 32-BGRA. Neither is
//! converted on the CPU: the planes (or rows) are copied out of the locked
//! buffer as they are. Only a widget that asks for RGBA8 gets the old
//! BGRA -> RGBA swizzle.

use std::{ffi::c_void, sync::Arc};

use azul_layout::widgets::capture_common::{CaptureRead, CaptureRequest};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    AllocAnyThread, DefinedClass,
};
use objc2_av_foundation::{
    AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceDiscoverySession, AVCaptureDeviceInput,
    AVCaptureDevicePosition, AVCaptureDeviceType, AVCaptureOutput, AVCaptureSession,
    AVCaptureSessionPreset1280x720, AVCaptureSessionPreset640x480, AVCaptureSessionPresetHigh,
    AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaType,
    AVMediaTypeVideo,
};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use azul_core::resources::RawImageFormat;
use objc2_core_video::{
    kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_601_4,
    kCVPixelBufferPixelFormatTypeKey, CVBufferGetAttachment, CVPixelBuffer,
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

// The CoreVideo pixel formats and the RawImageFormat -> pixel format rule
// live with the slot, shared with the VideoToolbox codec.
pub(crate) use crate::desktop::extra::capture_slot::{
    cv_pixel_format_for, CV_PIXEL_FORMAT_420F as PIXEL_FORMAT_420F,
    CV_PIXEL_FORMAT_420V as PIXEL_FORMAT_420V,
};
use crate::desktop::extra::capture_slot::CaptureSlot;

struct DelegateIvars {
    slot: Arc<CaptureSlot>,
    /// The widget asked for RGBA8: BGRA frames are swizzled (the old
    /// contract). Otherwise frames are published as they are.
    want_rgba: bool,
}

/// Publish one locked pixel buffer into `slot` in the form it arrived in: a
/// '420v' / '420f' buffer as NV12 (its matrix from the buffer's YCbCr-matrix
/// attachment, else Rec.709 from 720 lines up and Rec.601 below), a BGRA
/// buffer as BGRA8 (or swizzled to RGBA8 when `want_rgba`). Returns `true` for
/// the slot's first frame. Shared by the camera and the screen backends.
///
/// # Safety
/// `pb` must be locked (`CVPixelBufferLockBaseAddress`) for the duration of
/// the call.
pub(crate) unsafe fn publish_pixel_buffer(
    slot: &CaptureSlot,
    pb: &CVPixelBuffer,
    want_rgba: bool,
) -> bool {
    let w = CVPixelBufferGetWidth(pb);
    let h = CVPixelBufferGetHeight(pb);
    let pixel_format = CVPixelBufferGetPixelFormatType(pb);
    if pixel_format == PIXEL_FORMAT_420V || pixel_format == PIXEL_FORMAT_420F {
        // SAFETY: an attachment lookup on a live buffer. The value is compared
        // by VALUE (`==` on CF types is `CFEqual`): the attachment comes from
        // the format description (CoreMedia's "same string"), which is not
        // CoreVideo's constant by address.
        #[allow(deprecated)] // CVBufferGetAttachment: present on every macOS
        let rec601 = unsafe {
            match CVBufferGetAttachment(pb, kCVImageBufferYCbCrMatrixKey, core::ptr::null_mut()) {
                Some(value) => *value == **kCVImageBufferYCbCrMatrix_ITU_R_601_4,
                None => h < 720,
            }
        };
        let format = RawImageFormat::nv12(!rec601, pixel_format == PIXEL_FORMAT_420F);
        let y = CVPixelBufferGetBaseAddressOfPlane(pb, 0) as *const u8;
        let y_stride = CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
        let uv = CVPixelBufferGetBaseAddressOfPlane(pb, 1) as *const u8;
        let uv_stride = CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
        // SAFETY: the caller locked the buffer; the plane addresses and row
        // strides come from CoreVideo for this very buffer (the slot rejects
        // null planes and short strides).
        return unsafe { slot.publish_nv12(y, y_stride, uv, uv_stride, w, h, format) };
    }
    let stride = CVPixelBufferGetBytesPerRow(pb);
    let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
    // SAFETY: as above, for the single-plane BGRA buffer.
    unsafe {
        if want_rgba {
            slot.publish_bgra(base, w, h, stride)
        } else {
            slot.publish_packed(base, w, h, stride, RawImageFormat::BGRA8)
        }
    }
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
            unsafe {
                let image = match sample_buffer.image_buffer() {
                    Some(i) => i,
                    None => return,
                };
                let pb = &*image;
                // kCVPixelBufferLock_ReadOnly (1): the planes are only read. A
                // read-write lock tells CoreVideo the CPU may have changed the
                // buffer, which invalidates its GPU / IOSurface caches for nothing.
                CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags(1));
                // Copy the planes (or rows) into the slot's REUSED buffer as
                // they are and wake the reader; the slot validates them.
                let ivars = self.ivars();
                if publish_pixel_buffer(&ivars.slot, pb, ivars.want_rgba) {
                    // Log the very first frame only (the callback is hot).
                    crate::plog_info!(
                        "[camera] avfoundation: first frame {}x{} pixel format {:#010x}",
                        CVPixelBufferGetWidth(pb),
                        CVPixelBufferGetHeight(pb),
                        CVPixelBufferGetPixelFormatType(pb)
                    );
                }
                CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags(1));
            }
        }
    }
);

impl FrameDelegate {
    fn new(slot: Arc<CaptureSlot>, want_rgba: bool) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { slot, want_rgba });
        unsafe { msg_send![super(this), init] }
    }
}

/// Live capture state behind the seam's `u64` handle (worker-thread-local).
struct AvfCam {
    session: Retained<AVCaptureSession>,
    /// Kept for `reconfigure` (frame rate lives on the device).
    device: Retained<AVCaptureDevice>,
    _delegate: Retained<FrameDelegate>,
    slot: Arc<CaptureSlot>,
    last_seq: u64,
}

/// `AVCaptureSessionPreset960x540` exists in macOS's AVFoundation only. iOS
/// has no such export, and merely referencing the symbol makes the arm64 iOS
/// link fail with "Undefined symbols" — so it is not in the `use` list above
/// and is reached only through this item-level cfg, which is the one place a
/// `#[cfg]` can remove the reference outright.
#[cfg(target_os = "macos")]
fn preset_960x540() -> Option<&'static NSString> {
    // SAFETY: an extern NSString static AVFoundation defines since 10.7,
    // read exactly as the other presets in `preset_for` are.
    Some(unsafe { objc2_av_foundation::AVCaptureSessionPreset960x540 })
}

#[cfg(not(target_os = "macos"))]
fn preset_960x540() -> Option<&'static NSString> {
    None
}

/// The smallest session preset that covers `width` x `height`: the presets
/// are extern NSString statics present since 10.7. `None` for a zero size
/// (leave the session's default). Larger than 720p -> `High` (the device's
/// best).
fn preset_for(width: u32, height: u32) -> Option<&'static NSString> {
    // SAFETY: the preset names are extern NSString statics AVFoundation
    // defines since 10.7; reading them is how every caller uses them.
    unsafe {
        if width == 0 || height == 0 {
            None
        } else if width <= 640 && height <= 480 {
            Some(AVCaptureSessionPreset640x480)
        } else if width <= 960 && height <= 540 && preset_960x540().is_some() {
            // macOS only — see `preset_960x540`. On iOS the arm is never
            // taken and a 960x540 request falls through to the 720p preset.
            preset_960x540()
        } else if width <= 1280 && height <= 720 {
            Some(AVCaptureSessionPreset1280x720)
        } else {
            Some(AVCaptureSessionPresetHigh)
        }
    }
}

/// Apply `fps` to the device (0 = leave its default). The active format's
/// supported ranges are checked FIRST: `setActiveVideoMinFrameDuration`
/// throws `NSInvalidArgumentException` for an unsupported value, which would
/// abort the process. Call with the session running, because a preset
/// change may swap the active format and reset the durations.
unsafe fn apply_fps(device: &AVCaptureDevice, fps: u32) {
    if fps == 0 {
        return;
    }
    let wanted = f64::from(fps);
    // SAFETY: `device` is a live AVCaptureDevice owned by the running
    // session; the frame-rate setters are only called after
    // `lockForConfiguration` succeeded and with a rate the active format's
    // ranges include (an unsupported rate would throw).
    unsafe {
        let format = device.activeFormat();
        let ranges = format.videoSupportedFrameRateRanges();
        let supported = ranges
            .iter()
            .any(|r| r.minFrameRate() - 0.01 <= wanted && wanted <= r.maxFrameRate() + 0.01);
        if !supported {
            crate::plog_info!(
                "[camera] avfoundation: {} fps is outside the active format's ranges — keeping \
                 the default rate",
                fps
            );
            return;
        }
        if device.lockForConfiguration().is_err() {
            return;
        }
        let duration = CMTime {
            value: 1,
            timescale: fps as i32,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        device.setActiveVideoMinFrameDuration(duration);
        device.setActiveVideoMaxFrameDuration(duration);
        device.unlockForConfiguration();
    }
}

/// Read a possibly-NULL `AVCaptureDeviceType` extern static. Device-type
/// constants from newer SDKs (`External` is macOS 14+, `ContinuityCamera`
/// macOS 13+) resolve to NULL at runtime on older systems, so every use goes
/// through this null check instead of trusting the `&'static` type.
unsafe fn devtype_opt(
    s: *const &'static AVCaptureDeviceType,
) -> Option<&'static AVCaptureDeviceType> { unsafe {
    let raw: *const *const AVCaptureDeviceType = s.cast();
    let v = *raw;
    if v.is_null() {
        None
    } else {
        Some(&*v)
    }
}}

/// Pick the capture device for `index` via `AVCaptureDeviceDiscoverySession`
/// (built-in wide angle + external + Continuity cameras — whichever device
/// types this macOS knows about). Graceful fallback: `index` out of range →
/// device 0 → `defaultDeviceWithMediaType`. Logs the enumerated device names
/// (localizedName) once per process.
unsafe fn select_device(media: &AVMediaType, index: u32) -> Option<Retained<AVCaptureDevice>> { unsafe {
    #[allow(deprecated)] // pre-macOS-14 synonym for AVCaptureDeviceTypeExternal
    use objc2_av_foundation::AVCaptureDeviceTypeExternalUnknown;
    use objc2_av_foundation::{
        AVCaptureDeviceTypeBuiltInWideAngleCamera, AVCaptureDeviceTypeContinuityCamera,
        AVCaptureDeviceTypeExternal,
    };

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
        let session =
            AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
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
                    "[camera] avfoundation: device index {} out of range ({} device(s)) → falling \
                     back to device 0",
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
}}

/// Open the video device at `request.index`, request BGRA frames at the
/// smallest preset covering the requested size and the requested fps, start
/// the session. Returns a boxed handle, or `0` on failure (worker uses the
/// test pattern).
pub(super) fn open(request: &CaptureRequest) -> u64 {
    let (index, width, height) = (request.index, request.width, request.height);
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
        // HONOUR THE REQUESTED SIZE. Without a preset the session runs at
        // AVCaptureSessionPresetHigh — 1080p on a modern Mac — so a 300×200
        // tile received 8 MB frames and paid six full-resolution passes per
        // frame for them (the AzMeet "high CPU"). The smallest preset that
        // covers the request wins; the presets are extern NSString statics
        // present since 10.7, and `canSetSessionPreset` guards a device
        // that cannot do one.
        let preset: Option<&NSString> = preset_for(width, height);
        if !session.canAddInput(&input) {
            return 0;
        }
        session.addInput(&input);
        if let Some(preset) = preset {
            if session.canSetSessionPreset(preset) {
                session.setSessionPreset(preset);
            }
        }

        let output = AVCaptureVideoDataOutput::new();
        // The format the widget wants, in the form the camera produces it:
        // NV12 ('420v' / '420f') or 32-BGRA - never a CPU conversion.
        let pixel_format = cv_pixel_format_for(request.format);
        let key: &NSString = &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
        let val = NSNumber::new_u32(pixel_format);
        let settings: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[key], &[val.as_ref() as &AnyObject]);
        output.setVideoSettings(Some(&settings));
        output.setAlwaysDiscardsLateVideoFrames(true);

        let slot = CaptureSlot::new();
        let delegate = FrameDelegate::new(slot.clone(), request.format == RawImageFormat::RGBA8);
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
        // After startRunning: the preset has settled the active format.
        apply_fps(&device, request.fps);

        let cam = AvfCam {
            session,
            device,
            _delegate: delegate,
            slot,
            last_seq: 0,
        };
        crate::plog_info!(
            "[camera] avfoundation: opened device (index {}), requested {}x{} as {:?} \
             (CoreVideo {:#010x}, no conversion)",
            index,
            width,
            height,
            request.format,
            pixel_format
        );
        Box::into_raw(Box::new(cam)) as u64
    }
}

/// Take the newest frame into `out` (swapping buffers with the slot, no
/// copy), in the format it was published in. Waits (on the slot's condvar,
/// bounded at ~1 s) for a frame newer than the last one returned. A timeout
/// is `Idle` — a stalled camera (sleep/wake, a Continuity camera
/// reconnecting) used to be reported as end-of-stream, which killed the
/// worker and froze the tile for good.
pub(super) fn read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
    let cam = match unsafe { (handle as *mut AvfCam).as_mut() } {
        Some(c) => c,
        None => return CaptureRead::Ended,
    };
    match cam.slot.take_newer(
        &mut cam.last_seq,
        out,
        std::time::Duration::from_millis(1000),
    ) {
        Some((width, height, format)) => CaptureRead::FrameIn {
            width,
            height,
            format,
        },
        None => CaptureRead::Idle,
    }
}

/// Switch a RUNNING session to the preset covering the new size + the new
/// fps, without tearing the capture down (`beginConfiguration` /
/// `commitConfiguration` is the documented live path). `false` only for a
/// dead handle, in which case the worker reopens.
pub(super) fn reconfigure(handle: u64, request: &CaptureRequest) -> bool {
    let cam = match unsafe { (handle as *mut AvfCam).as_mut() } {
        Some(c) => c,
        None => return false,
    };
    unsafe {
        if let Some(preset) = preset_for(request.width, request.height) {
            cam.session.beginConfiguration();
            if cam.session.canSetSessionPreset(preset) {
                cam.session.setSessionPreset(preset);
            }
            cam.session.commitConfiguration();
        }
        apply_fps(&cam.device, request.fps);
    }
    crate::plog_info!(
        "[camera] avfoundation: reconfigured to {}x{} @ {} fps (live preset switch)",
        request.width,
        request.height,
        request.fps
    );
    true
}

/// Stop the session + free the capture (drops the boxed `AvfCam`).
pub(super) fn close(handle: u64) {
    if handle != 0 {
        unsafe {
            let cam = Box::from_raw(handle as *mut AvfCam);
            cam.session.stopRunning();
            drop(cam);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod matrix_tests {
    use core::ptr::NonNull;

    use azul_core::resources::RawImageFormat;
    use objc2_core_foundation::{CFRetained, CFString};
    use objc2_core_video::{
        kCVImageBufferYCbCrMatrixKey, CVAttachmentMode, CVPixelBuffer, CVPixelBufferCreate,
        CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    };

    use super::{publish_pixel_buffer, PIXEL_FORMAT_420V};
    use crate::desktop::extra::capture_slot::CaptureSlot;

    /// A buffer's YCbCr matrix is read by VALUE. The attachment a capture or
    /// a decoder sets comes from the format description (CoreMedia's
    /// `kCMFormatDescriptionYCbCrMatrix_ITU_R_601_4`): "the same string" as
    /// CoreVideo's `kCVImageBufferYCbCrMatrix_ITU_R_601_4`, not the same
    /// pointer. Compared by address, a Rec.601 camera read as Rec.709 and its
    /// picture showed with the wrong hue. 720 lines, so the "no attachment"
    /// fallback would say Rec.709 too: only the attachment can say 601.
    #[test]
    fn a_rec601_matrix_named_by_an_equal_string_reads_as_rec601() {
        let mut raw: *mut CVPixelBuffer = core::ptr::null_mut();
        let status = unsafe {
            CVPixelBufferCreate(
                None,
                1280,
                720,
                PIXEL_FORMAT_420V,
                None,
                NonNull::from(&mut raw),
            )
        };
        assert_eq!(status, 0, "CVPixelBufferCreate");
        let pb: CFRetained<CVPixelBuffer> =
            unsafe { CFRetained::from_raw(NonNull::new(raw).expect("a pixel buffer")) };
        // The same characters as CoreVideo's constant, in a string of its own.
        let matrix = CFString::from_str("ITU_R_601_4");
        unsafe {
            pb.set_attachment(
                kCVImageBufferYCbCrMatrixKey,
                &matrix,
                CVAttachmentMode::ShouldPropagate,
            );
        }
        let slot = CaptureSlot::new();
        unsafe {
            CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags(1));
            publish_pixel_buffer(&slot, &pb, false);
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags(1));
        }
        let mut seq = 0;
        let mut out = Vec::new();
        let (w, h, format) = slot
            .take_newer(&mut seq, &mut out, std::time::Duration::from_millis(10))
            .expect("the buffer was published");
        assert_eq!((w, h), (1280, 720));
        assert_eq!(format, RawImageFormat::NV12Rec601Video);
    }
}
