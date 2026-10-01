//! Apple **VideoToolbox** H.264 encode/decode for `VideoEncoder` /
//! `VideoDecoder` (macOS 10.8+ / iOS 8+; hardware-accelerated on every Apple
//! Silicon + Intel QuickSync Mac).
//!
//! ALL symbols are `dlopen`ed at runtime (VideoToolbox, CoreMedia, CoreVideo,
//! CoreFoundation) — no build-time framework link, same rule as the Linux
//! v4l2/PipeWire backends, so the dylib loads on any macOS version and the
//! backend degrades to `None` (stub) if a symbol is missing.
//!
//! Wire format: the azul pipeline speaks **Annex-B** (start-code-delimited,
//! what `demux.rs` emits and what goes over UDP in azul-meet); VideoToolbox
//! speaks **AVCC** (4-byte big-endian length prefixes, out-of-band parameter
//! sets). This module converts both directions:
//!   - encode: SPS/PPS pulled from the output format description and emitted in-band ahead of every
//!     keyframe; each length-prefixed NAL rewritten with `00 00 00 01` start codes.
//!   - decode: SPS(7)/PPS(8) NALs collected from the Annex-B stream feed
//!     `CMVideoFormatDescriptionCreateFromH264ParameterSets`; VCL NALs are re-prefixed with 4-byte
//!     lengths and wrapped in a `CMSampleBuffer`.
//!
//! Frames go in as they come: an NV12 frame (the camera's '420v') is copied
//! plane by plane into a buffer of the session's own pixel-buffer pool
//! (IOSurface-backed, '420v' - the codec's 4:2:0, so VideoToolbox converts
//! nothing); a BGRA frame into a BGRA buffer; only an RGBA frame is
//! swizzled. The session is created for HARDWARE encode (required where the
//! OS knows the key) with low-latency rate control where available (macOS
//! 11.3+), realtime, no frame reordering, no frame delay; `settings()` says
//! what it got. Decode hands out what `set_output_format` asked for (NV12 by
//! default for a YUV tile, BGRA, or RGBA) at the size `set_output_size` asked
//! for, scaled by VideoToolbox. H.265 is not wired yet (the demos are H.264,
//! same scope as the Vulkan backend) — `open(h265=true)` yields the stub.

use std::{
    collections::VecDeque,
    ffi::c_void,
    sync::{Arc, Mutex, OnceLock},
};

use azul_core::{
    resources::{Nv12Layout, RawImageFormat},
    video::VideoFrame,
};
use azul_css::U8Vec;

// ---------------------------------------------------------------------------
// Minimal CF/CM ABI types
// ---------------------------------------------------------------------------

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFArrayRef = *const c_void;
type OSStatus = i32;

/// CoreMedia CMTime, passed by value across the C ABI.
#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32, // kCMTimeFlags_Valid = 1
    epoch: i64,
}

impl CMTime {
    fn new(value: i64, timescale: i32) -> Self {
        CMTime {
            value,
            timescale,
            flags: 1,
            epoch: 0,
        }
    }
    fn invalid() -> Self {
        CMTime {
            value: 0,
            timescale: 0,
            flags: 0,
            epoch: 0,
        }
    }
}

/// CoreMedia CMSampleTimingInfo, passed by pointer.
#[repr(C)]
#[derive(Clone, Copy)]
struct CMSampleTimingInfo {
    duration: CMTime,
    presentation_time_stamp: CMTime,
    decode_time_stamp: CMTime,
}

/// VTDecompressionOutputCallbackRecord.
#[repr(C)]
struct VTDecompressionOutputCallbackRecord {
    callback: extern "C" fn(
        refcon: *mut c_void,
        source_frame_refcon: *mut c_void,
        status: OSStatus,
        info_flags: u32,
        image_buffer: *mut c_void, // CVImageBufferRef
        pts: CMTime,
        duration: CMTime,
    ),
    refcon: *mut c_void,
}

/// 'avc1' — kCMVideoCodecType_H264.
const CODEC_H264: u32 = 0x61766331;
/// 'BGRA' — kCVPixelFormatType_32BGRA.
const PIXFMT_BGRA: u32 = 0x42475241;
/// '420v' — kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange (NV12, video range).
const PIXFMT_420V: u32 = 0x34323076;
/// '420f' — kCVPixelFormatType_420YpCbCr8BiPlanarFullRange (NV12, full range).
const PIXFMT_420F: u32 = 0x34323066;
/// kCFNumberSInt32Type.
const CF_NUMBER_SINT32: isize = 3;

// ---------------------------------------------------------------------------
// dlopen'd function table
// ---------------------------------------------------------------------------

macro_rules! vt_symbols {
    ($(($field:ident, $sym:literal, $ty:ty)),* $(,)?) => {
        /// Every VideoToolbox / CoreMedia / CoreVideo / CoreFoundation entry
        /// point this backend touches, resolved once at first use.
        #[allow(non_snake_case)]
        struct VtLib {
            _vt: libloading::Library,
            _cm: libloading::Library,
            _cv: libloading::Library,
            _cf: libloading::Library,
            $($field: $ty,)*
            // dlsym'd CFStringRef constants (data symbols, deref'd once).
            kVTCompressionPropertyKey_RealTime: CFStringRef,
            kVTCompressionPropertyKey_AverageBitRate: CFStringRef,
            kVTCompressionPropertyKey_AllowFrameReordering: CFStringRef,
            kVTCompressionPropertyKey_MaxKeyFrameInterval: CFStringRef,
            kVTCompressionPropertyKey_ProfileLevel: CFStringRef,
            kVTProfileLevel_H264_Main_AutoLevel: CFStringRef,
            kVTEncodeFrameOptionKey_ForceKeyFrame: CFStringRef,
            kVTCompressionPropertyKey_ExpectedFrameRate: CFStringRef,
            kVTCompressionPropertyKey_MaxFrameDelayCount: CFStringRef,
            kCMSampleAttachmentKey_NotSync: CFStringRef,
            kCVPixelBufferPixelFormatTypeKey: CFStringRef,
            kCVPixelBufferWidthKey: CFStringRef,
            kCVPixelBufferHeightKey: CFStringRef,
            kCVPixelBufferIOSurfacePropertiesKey: CFStringRef,
            kCVImageBufferYCbCrMatrixKey: CFStringRef,
            kCVImageBufferYCbCrMatrix_ITU_R_601_4: CFStringRef,
            // OPTIONAL constants (null where this OS does not have them):
            // the encoder specification keys (macOS only; low-latency rate
            // control since 11.3), the hardware query, the High profile, and
            // the colour tags.
            kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder: CFStringRef,
            kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: CFStringRef,
            kVTVideoEncoderSpecification_EnableLowLatencyRateControl: CFStringRef,
            kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder: CFStringRef,
            kVTProfileLevel_H264_High_AutoLevel: CFStringRef,
            kVTCompressionPropertyKey_YCbCrMatrix: CFStringRef,
            kVTCompressionPropertyKey_ColorPrimaries: CFStringRef,
            kVTCompressionPropertyKey_TransferFunction: CFStringRef,
            kCVImageBufferYCbCrMatrix_ITU_R_709_2: CFStringRef,
            kCVImageBufferColorPrimaries_ITU_R_709_2: CFStringRef,
            kCVImageBufferTransferFunction_ITU_R_709_2: CFStringRef,
            kCFBooleanTrue: CFTypeRef,
            kCFBooleanFalse: CFTypeRef,
            kCFTypeDictionaryKeyCallBacks: *const c_void,
            kCFTypeDictionaryValueCallBacks: *const c_void,
        }
    };
}

vt_symbols!(
    // -- VideoToolbox ------------------------------------------------------
    (VTCompressionSessionCreate, b"VTCompressionSessionCreate",
        unsafe extern "C" fn(*const c_void, i32, i32, u32, CFDictionaryRef, CFDictionaryRef,
            *const c_void,
            extern "C" fn(*mut c_void, *mut c_void, OSStatus, u32, *mut c_void),
            *mut c_void, *mut *mut c_void) -> OSStatus),
    (VTSessionSetProperty, b"VTSessionSetProperty",
        unsafe extern "C" fn(*mut c_void, CFStringRef, CFTypeRef) -> OSStatus),
    (VTCompressionSessionPrepareToEncodeFrames, b"VTCompressionSessionPrepareToEncodeFrames",
        unsafe extern "C" fn(*mut c_void) -> OSStatus),
    (VTCompressionSessionEncodeFrame, b"VTCompressionSessionEncodeFrame",
        unsafe extern "C" fn(*mut c_void, *mut c_void, CMTime, CMTime, CFDictionaryRef,
            *mut c_void, *mut u32) -> OSStatus),
    (VTCompressionSessionCompleteFrames, b"VTCompressionSessionCompleteFrames",
        unsafe extern "C" fn(*mut c_void, CMTime) -> OSStatus),
    (VTCompressionSessionInvalidate, b"VTCompressionSessionInvalidate",
        unsafe extern "C" fn(*mut c_void)),
    (VTCompressionSessionGetPixelBufferPool, b"VTCompressionSessionGetPixelBufferPool",
        unsafe extern "C" fn(*mut c_void) -> *mut c_void),
    (VTSessionCopyProperty, b"VTSessionCopyProperty",
        unsafe extern "C" fn(*mut c_void, CFStringRef, *const c_void, *mut CFTypeRef) -> OSStatus),
    (VTDecompressionSessionCreate, b"VTDecompressionSessionCreate",
        unsafe extern "C" fn(*const c_void, *const c_void, CFDictionaryRef, CFDictionaryRef,
            *const VTDecompressionOutputCallbackRecord, *mut *mut c_void) -> OSStatus),
    (VTDecompressionSessionDecodeFrame, b"VTDecompressionSessionDecodeFrame",
        unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *mut u32) -> OSStatus),
    (VTDecompressionSessionInvalidate, b"VTDecompressionSessionInvalidate",
        unsafe extern "C" fn(*mut c_void)),
    // -- CoreMedia ---------------------------------------------------------
    (CMVideoFormatDescriptionCreateFromH264ParameterSets,
        b"CMVideoFormatDescriptionCreateFromH264ParameterSets",
        unsafe extern "C" fn(*const c_void, usize, *const *const u8, *const usize, i32,
            *mut *mut c_void) -> OSStatus),
    (CMVideoFormatDescriptionGetH264ParameterSetAtIndex,
        b"CMVideoFormatDescriptionGetH264ParameterSetAtIndex",
        unsafe extern "C" fn(*const c_void, usize, *mut *const u8, *mut usize, *mut usize,
            *mut i32) -> OSStatus),
    (CMBlockBufferCreateWithMemoryBlock, b"CMBlockBufferCreateWithMemoryBlock",
        unsafe extern "C" fn(*const c_void, *mut c_void, usize, *const c_void, *const c_void,
            usize, usize, u32, *mut *mut c_void) -> OSStatus),
    (CMBlockBufferReplaceDataBytes, b"CMBlockBufferReplaceDataBytes",
        unsafe extern "C" fn(*const c_void, *mut c_void, usize, usize) -> OSStatus),
    (CMBlockBufferGetDataPointer, b"CMBlockBufferGetDataPointer",
        unsafe extern "C" fn(*mut c_void, usize, *mut usize, *mut usize, *mut *mut u8)
            -> OSStatus),
    (CMSampleBufferCreateReady, b"CMSampleBufferCreateReady",
        unsafe extern "C" fn(*const c_void, *mut c_void, *const c_void, isize, isize,
            *const CMSampleTimingInfo, isize, *const usize, *mut *mut c_void) -> OSStatus),
    (CMSampleBufferGetDataBuffer, b"CMSampleBufferGetDataBuffer",
        unsafe extern "C" fn(*mut c_void) -> *mut c_void),
    (CMSampleBufferGetFormatDescription, b"CMSampleBufferGetFormatDescription",
        unsafe extern "C" fn(*mut c_void) -> *const c_void),
    (CMSampleBufferGetSampleAttachmentsArray, b"CMSampleBufferGetSampleAttachmentsArray",
        unsafe extern "C" fn(*mut c_void, u8) -> CFArrayRef),
    // -- CoreVideo ---------------------------------------------------------
    (CVPixelBufferCreate, b"CVPixelBufferCreate",
        unsafe extern "C" fn(*const c_void, usize, usize, u32, CFDictionaryRef,
            *mut *mut c_void) -> i32),
    (CVPixelBufferLockBaseAddress, b"CVPixelBufferLockBaseAddress",
        unsafe extern "C" fn(*mut c_void, u64) -> i32),
    (CVPixelBufferUnlockBaseAddress, b"CVPixelBufferUnlockBaseAddress",
        unsafe extern "C" fn(*mut c_void, u64) -> i32),
    (CVPixelBufferGetBaseAddress, b"CVPixelBufferGetBaseAddress",
        unsafe extern "C" fn(*mut c_void) -> *mut u8),
    (CVPixelBufferGetBytesPerRow, b"CVPixelBufferGetBytesPerRow",
        unsafe extern "C" fn(*mut c_void) -> usize),
    (CVPixelBufferGetWidth, b"CVPixelBufferGetWidth",
        unsafe extern "C" fn(*mut c_void) -> usize),
    (CVPixelBufferGetHeight, b"CVPixelBufferGetHeight",
        unsafe extern "C" fn(*mut c_void) -> usize),
    (CVPixelBufferPoolCreatePixelBuffer, b"CVPixelBufferPoolCreatePixelBuffer",
        unsafe extern "C" fn(*const c_void, *mut c_void, *mut *mut c_void) -> i32),
    (CVPixelBufferGetBaseAddressOfPlane, b"CVPixelBufferGetBaseAddressOfPlane",
        unsafe extern "C" fn(*mut c_void, usize) -> *mut u8),
    (CVPixelBufferGetBytesPerRowOfPlane, b"CVPixelBufferGetBytesPerRowOfPlane",
        unsafe extern "C" fn(*mut c_void, usize) -> usize),
    (CVPixelBufferGetPixelFormatType, b"CVPixelBufferGetPixelFormatType",
        unsafe extern "C" fn(*mut c_void) -> u32),
    (CVBufferGetAttachment, b"CVBufferGetAttachment",
        unsafe extern "C" fn(*mut c_void, CFStringRef, *mut u32) -> CFTypeRef),
    // -- CoreFoundation ----------------------------------------------------
    (CFRelease, b"CFRelease", unsafe extern "C" fn(CFTypeRef)),
    (CFDictionaryCreateMutable, b"CFDictionaryCreateMutable",
        unsafe extern "C" fn(*const c_void, isize, *const c_void, *const c_void)
            -> *mut c_void),
    (CFDictionarySetValue, b"CFDictionarySetValue",
        unsafe extern "C" fn(*mut c_void, *const c_void, *const c_void)),
    (CFDictionaryGetValue, b"CFDictionaryGetValue",
        unsafe extern "C" fn(CFDictionaryRef, *const c_void) -> *const c_void),
    (CFNumberCreate, b"CFNumberCreate",
        unsafe extern "C" fn(*const c_void, isize, *const c_void) -> *const c_void),
    (CFArrayGetCount, b"CFArrayGetCount", unsafe extern "C" fn(CFArrayRef) -> isize),
    (CFArrayGetValueAtIndex, b"CFArrayGetValueAtIndex",
        unsafe extern "C" fn(CFArrayRef, isize) -> *const c_void),
    (CFBooleanGetValue, b"CFBooleanGetValue",
        unsafe extern "C" fn(*const c_void) -> u8),
);

unsafe impl Send for VtLib {}
unsafe impl Sync for VtLib {}

static VT: OnceLock<Option<VtLib>> = OnceLock::new();

impl VtLib {
    fn get() -> Option<&'static VtLib> {
        VT.get_or_init(|| unsafe {
            // dlopen failures must be as loud as the missing-symbol paths
            // below — otherwise "framework not loadable" silently degrades to
            // the no-frames stub.
            let open = |p: &str| match libloading::Library::new(p) {
                Ok(l) => Some(l),
                Err(e) => {
                    crate::plog_warn!(
                        "[video] VideoToolbox backend disabled: dlopen {} failed: {}",
                        p,
                        e
                    );
                    None
                }
            };
            let vt = open("/System/Library/Frameworks/VideoToolbox.framework/VideoToolbox")?;
            let cm = open("/System/Library/Frameworks/CoreMedia.framework/CoreMedia")?;
            let cv = open("/System/Library/Frameworks/CoreVideo.framework/CoreVideo")?;
            let cf = open("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")?;

            // fn symbol, from the right library (VT fns in vt, CM in cm, …).
            macro_rules! f {
                ($lib:expr, $sym:literal) => {
                    match $lib.get($sym) {
                        Ok(s) => *s,
                        Err(_) => {
                            crate::plog_warn!(
                                "[video] VideoToolbox backend disabled: missing symbol {}",
                                String::from_utf8_lossy($sym)
                            );
                            return None;
                        }
                    }
                };
            }
            // data symbol (CFStringRef / CFTypeRef constant): dlsym yields a
            // pointer TO the constant; deref once.
            macro_rules! d {
                ($lib:expr, $sym:literal) => {{
                    let p: libloading::Symbol<'_, *const *const c_void> = match $lib.get($sym) {
                        Ok(s) => s,
                        Err(_) => {
                            crate::plog_warn!(
                                "[video] VideoToolbox backend disabled: missing constant {}",
                                String::from_utf8_lossy($sym)
                            );
                            return None;
                        }
                    };
                    **p
                }};
            }
            // OPTIONAL data symbol: null when this OS does not export it
            // (a newer key), instead of disabling the whole backend.
            macro_rules! od {
                ($lib:expr, $sym:literal) => {{
                    match $lib.get::<*const *const c_void>($sym) {
                        Ok(p) => **p,
                        Err(_) => core::ptr::null(),
                    }
                }};
            }
            // callback-table address (kCFTypeDictionary*CallBacks are structs,
            // we need their ADDRESS, not their first word).
            macro_rules! a {
                ($lib:expr, $sym:literal) => {{
                    let p: libloading::Symbol<'_, *const c_void> = match $lib.get($sym) {
                        Ok(s) => s,
                        Err(_) => return None,
                    };
                    p.try_as_raw_ptr().unwrap_or(core::ptr::null_mut()) as *const c_void
                }};
            }

            Some(VtLib {
                VTCompressionSessionCreate: f!(vt, b"VTCompressionSessionCreate\0"),
                VTSessionSetProperty: f!(vt, b"VTSessionSetProperty\0"),
                VTCompressionSessionPrepareToEncodeFrames: f!(
                    vt,
                    b"VTCompressionSessionPrepareToEncodeFrames\0"
                ),
                VTCompressionSessionEncodeFrame: f!(vt, b"VTCompressionSessionEncodeFrame\0"),
                VTCompressionSessionCompleteFrames: f!(vt, b"VTCompressionSessionCompleteFrames\0"),
                VTCompressionSessionInvalidate: f!(vt, b"VTCompressionSessionInvalidate\0"),
                VTCompressionSessionGetPixelBufferPool: f!(
                    vt,
                    b"VTCompressionSessionGetPixelBufferPool\0"
                ),
                VTSessionCopyProperty: f!(vt, b"VTSessionCopyProperty\0"),
                VTDecompressionSessionCreate: f!(vt, b"VTDecompressionSessionCreate\0"),
                VTDecompressionSessionDecodeFrame: f!(vt, b"VTDecompressionSessionDecodeFrame\0"),
                VTDecompressionSessionInvalidate: f!(vt, b"VTDecompressionSessionInvalidate\0"),
                CMVideoFormatDescriptionCreateFromH264ParameterSets: f!(
                    cm,
                    b"CMVideoFormatDescriptionCreateFromH264ParameterSets\0"
                ),
                CMVideoFormatDescriptionGetH264ParameterSetAtIndex: f!(
                    cm,
                    b"CMVideoFormatDescriptionGetH264ParameterSetAtIndex\0"
                ),
                CMBlockBufferCreateWithMemoryBlock: f!(cm, b"CMBlockBufferCreateWithMemoryBlock\0"),
                CMBlockBufferReplaceDataBytes: f!(cm, b"CMBlockBufferReplaceDataBytes\0"),
                CMBlockBufferGetDataPointer: f!(cm, b"CMBlockBufferGetDataPointer\0"),
                CMSampleBufferCreateReady: f!(cm, b"CMSampleBufferCreateReady\0"),
                CMSampleBufferGetDataBuffer: f!(cm, b"CMSampleBufferGetDataBuffer\0"),
                CMSampleBufferGetFormatDescription: f!(cm, b"CMSampleBufferGetFormatDescription\0"),
                CMSampleBufferGetSampleAttachmentsArray: f!(
                    cm,
                    b"CMSampleBufferGetSampleAttachmentsArray\0"
                ),
                CVPixelBufferCreate: f!(cv, b"CVPixelBufferCreate\0"),
                CVPixelBufferLockBaseAddress: f!(cv, b"CVPixelBufferLockBaseAddress\0"),
                CVPixelBufferUnlockBaseAddress: f!(cv, b"CVPixelBufferUnlockBaseAddress\0"),
                CVPixelBufferGetBaseAddress: f!(cv, b"CVPixelBufferGetBaseAddress\0"),
                CVPixelBufferGetBytesPerRow: f!(cv, b"CVPixelBufferGetBytesPerRow\0"),
                CVPixelBufferGetWidth: f!(cv, b"CVPixelBufferGetWidth\0"),
                CVPixelBufferGetHeight: f!(cv, b"CVPixelBufferGetHeight\0"),
                CVPixelBufferPoolCreatePixelBuffer: f!(cv, b"CVPixelBufferPoolCreatePixelBuffer\0"),
                CVPixelBufferGetBaseAddressOfPlane: f!(cv, b"CVPixelBufferGetBaseAddressOfPlane\0"),
                CVPixelBufferGetBytesPerRowOfPlane: f!(cv, b"CVPixelBufferGetBytesPerRowOfPlane\0"),
                CVPixelBufferGetPixelFormatType: f!(cv, b"CVPixelBufferGetPixelFormatType\0"),
                CVBufferGetAttachment: f!(cv, b"CVBufferGetAttachment\0"),
                CFRelease: f!(cf, b"CFRelease\0"),
                CFDictionaryCreateMutable: f!(cf, b"CFDictionaryCreateMutable\0"),
                CFDictionarySetValue: f!(cf, b"CFDictionarySetValue\0"),
                CFDictionaryGetValue: f!(cf, b"CFDictionaryGetValue\0"),
                CFNumberCreate: f!(cf, b"CFNumberCreate\0"),
                CFArrayGetCount: f!(cf, b"CFArrayGetCount\0"),
                CFArrayGetValueAtIndex: f!(cf, b"CFArrayGetValueAtIndex\0"),
                CFBooleanGetValue: f!(cf, b"CFBooleanGetValue\0"),
                kVTCompressionPropertyKey_RealTime: d!(vt, b"kVTCompressionPropertyKey_RealTime\0"),
                kVTCompressionPropertyKey_AverageBitRate: d!(
                    vt,
                    b"kVTCompressionPropertyKey_AverageBitRate\0"
                ),
                kVTCompressionPropertyKey_AllowFrameReordering: d!(
                    vt,
                    b"kVTCompressionPropertyKey_AllowFrameReordering\0"
                ),
                kVTCompressionPropertyKey_MaxKeyFrameInterval: d!(
                    vt,
                    b"kVTCompressionPropertyKey_MaxKeyFrameInterval\0"
                ),
                kVTCompressionPropertyKey_ProfileLevel: d!(
                    vt,
                    b"kVTCompressionPropertyKey_ProfileLevel\0"
                ),
                kVTProfileLevel_H264_Main_AutoLevel: d!(
                    vt,
                    b"kVTProfileLevel_H264_Main_AutoLevel\0"
                ),
                kVTEncodeFrameOptionKey_ForceKeyFrame: d!(
                    vt,
                    b"kVTEncodeFrameOptionKey_ForceKeyFrame\0"
                ),
                kVTCompressionPropertyKey_ExpectedFrameRate: d!(
                    vt,
                    b"kVTCompressionPropertyKey_ExpectedFrameRate\0"
                ),
                kVTCompressionPropertyKey_MaxFrameDelayCount: d!(
                    vt,
                    b"kVTCompressionPropertyKey_MaxFrameDelayCount\0"
                ),
                kCMSampleAttachmentKey_NotSync: d!(cm, b"kCMSampleAttachmentKey_NotSync\0"),
                kCVPixelBufferPixelFormatTypeKey: d!(cv, b"kCVPixelBufferPixelFormatTypeKey\0"),
                kCVPixelBufferWidthKey: d!(cv, b"kCVPixelBufferWidthKey\0"),
                kCVPixelBufferHeightKey: d!(cv, b"kCVPixelBufferHeightKey\0"),
                kCVPixelBufferIOSurfacePropertiesKey: d!(
                    cv,
                    b"kCVPixelBufferIOSurfacePropertiesKey\0"
                ),
                kCVImageBufferYCbCrMatrixKey: d!(cv, b"kCVImageBufferYCbCrMatrixKey\0"),
                kCVImageBufferYCbCrMatrix_ITU_R_601_4: d!(
                    cv,
                    b"kCVImageBufferYCbCrMatrix_ITU_R_601_4\0"
                ),
                kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder: od!(
                    vt,
                    b"kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder\0"
                ),
                kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: od!(
                    vt,
                    b"kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder\0"
                ),
                kVTVideoEncoderSpecification_EnableLowLatencyRateControl: od!(
                    vt,
                    b"kVTVideoEncoderSpecification_EnableLowLatencyRateControl\0"
                ),
                kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder: od!(
                    vt,
                    b"kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder\0"
                ),
                kVTProfileLevel_H264_High_AutoLevel: od!(vt, b"kVTProfileLevel_H264_High_AutoLevel\0"),
                kVTCompressionPropertyKey_YCbCrMatrix: od!(
                    vt,
                    b"kVTCompressionPropertyKey_YCbCrMatrix\0"
                ),
                kVTCompressionPropertyKey_ColorPrimaries: od!(
                    vt,
                    b"kVTCompressionPropertyKey_ColorPrimaries\0"
                ),
                kVTCompressionPropertyKey_TransferFunction: od!(
                    vt,
                    b"kVTCompressionPropertyKey_TransferFunction\0"
                ),
                kCVImageBufferYCbCrMatrix_ITU_R_709_2: od!(
                    cv,
                    b"kCVImageBufferYCbCrMatrix_ITU_R_709_2\0"
                ),
                kCVImageBufferColorPrimaries_ITU_R_709_2: od!(
                    cv,
                    b"kCVImageBufferColorPrimaries_ITU_R_709_2\0"
                ),
                kCVImageBufferTransferFunction_ITU_R_709_2: od!(
                    cv,
                    b"kCVImageBufferTransferFunction_ITU_R_709_2\0"
                ),
                kCFBooleanTrue: d!(cf, b"kCFBooleanTrue\0"),
                kCFBooleanFalse: d!(cf, b"kCFBooleanFalse\0"),
                kCFTypeDictionaryKeyCallBacks: a!(cf, b"kCFTypeDictionaryKeyCallBacks\0"),
                kCFTypeDictionaryValueCallBacks: a!(cf, b"kCFTypeDictionaryValueCallBacks\0"),
                _vt: vt,
                _cm: cm,
                _cv: cv,
                _cf: cf,
            })
        })
        .as_ref()
    }
}

/// Whether the VideoToolbox backend is usable on this machine (all four
/// frameworks loaded + every symbol resolved). Drives the capability probe.
pub(crate) fn is_available() -> bool {
    VtLib::get().is_some()
}

// ---------------------------------------------------------------------------
// Annex-B helpers
// ---------------------------------------------------------------------------

// The NAL splitter is the container module's (`container::annexb_nals`): one
// splitter for the decoder here, the MP4 muxer and the demuxer's tests.
use super::container::{annexb_nals, append_avcc_as_annexb};

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Chunks produced by the VT output callback (Annex-B, ready for the wire).
struct EncShared {
    chunks: Mutex<VecDeque<Vec<u8>>>,
}

/// VTCompressionOutputCallback: convert each AVCC sample to Annex-B (SPS/PPS
/// in-band ahead of keyframes) and queue it. Runs on a VT-internal thread.
extern "C" fn enc_output(
    refcon: *mut c_void,
    _src: *mut c_void,
    status: OSStatus,
    _flags: u32,
    sample: *mut c_void,
) {
    if status != 0 || sample.is_null() || refcon.is_null() {
        return;
    }
    let lib = match VtLib::get() {
        Some(l) => l,
        None => return,
    };
    let shared = unsafe { &*(refcon as *const EncShared) };
    unsafe {
        // Keyframe = attachments[0][NotSync] absent or false.
        let mut keyframe = true;
        let atts = (lib.CMSampleBufferGetSampleAttachmentsArray)(sample, 0);
        if !atts.is_null() && (lib.CFArrayGetCount)(atts) > 0 {
            let dict = (lib.CFArrayGetValueAtIndex)(atts, 0);
            if !dict.is_null() {
                let not_sync = (lib.CFDictionaryGetValue)(dict, lib.kCMSampleAttachmentKey_NotSync);
                if !not_sync.is_null() && (lib.CFBooleanGetValue)(not_sync) != 0 {
                    keyframe = false;
                }
            }
        }

        let mut chunk: Vec<u8> = Vec::with_capacity(4096);
        if keyframe {
            // Parameter sets live in the format description, not the stream.
            let desc = (lib.CMSampleBufferGetFormatDescription)(sample);
            if !desc.is_null() {
                for idx in 0..2usize {
                    let mut ptr: *const u8 = core::ptr::null();
                    let mut size = 0usize;
                    let mut count = 0usize;
                    let mut nal_hdr = 0i32;
                    let st = (lib.CMVideoFormatDescriptionGetH264ParameterSetAtIndex)(
                        desc,
                        idx,
                        &mut ptr,
                        &mut size,
                        &mut count,
                        &mut nal_hdr,
                    );
                    if st == 0 && !ptr.is_null() && size > 0 {
                        chunk.extend_from_slice(&[0, 0, 0, 1]);
                        chunk.extend_from_slice(std::slice::from_raw_parts(ptr, size));
                    }
                }
            }
        }

        // AVCC → Annex-B: rewrite 4-byte BE lengths as start codes.
        let bb = (lib.CMSampleBufferGetDataBuffer)(sample);
        if bb.is_null() {
            return;
        }
        let mut total = 0usize;
        let mut data: *mut u8 = core::ptr::null_mut();
        let mut at_off = 0usize;
        if (lib.CMBlockBufferGetDataPointer)(bb, 0, &mut at_off, &mut total, &mut data) != 0
            || data.is_null()
        {
            return;
        }
        let bytes = std::slice::from_raw_parts(data, total);
        append_avcc_as_annexb(bytes, &mut chunk);
        if !chunk.is_empty() {
            if let Ok(mut q) = shared.chunks.lock() {
                q.push_back(chunk);
            }
        }
    }
}

/// What a VideoToolbox encoder session actually got (logged at open, and the
/// line a report quotes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EncoderSettings {
    /// `Some(true)`: VideoToolbox says the session encodes in hardware (or
    /// it was created with hardware REQUIRED); `Some(false)`: in software;
    /// `None`: this OS cannot say (the query is refused while low-latency
    /// rate control is on, and hardware was not required).
    pub hardware: Option<bool>,
    /// Low-latency rate control (macOS 11.3+): one frame in, one frame out.
    pub low_latency: bool,
    /// `kVTCompressionPropertyKey_RealTime`.
    pub realtime: bool,
    /// `kVTCompressionPropertyKey_AllowFrameReordering` (B-frames).
    pub frame_reordering: bool,
    /// The H.264 profile asked for.
    pub profile: &'static str,
}

/// A live VTCompressionSession (H.264, realtime, no B-frames).
pub(super) struct VtEncoder {
    session: *mut c_void,
    shared: Arc<EncShared>,
    width: u32,
    height: u32,
    /// The encoder's clock: presentation times are real elapsed time, so a
    /// dropped frame is visible to rate control (not a fixed 1/30 s step).
    started: std::time::Instant,
    settings: EncoderSettings,
}

unsafe impl Send for VtEncoder {}

/// A CFNumber (SInt32), or null. The caller releases it.
unsafe fn cf_i32(lib: &VtLib, value: i32) -> *const c_void {
    unsafe {
        (lib.CFNumberCreate)(
            core::ptr::null(),
            CF_NUMBER_SINT32,
            &value as *const i32 as *const c_void,
        )
    }
}

/// Set an SInt32 session property (best effort: an unsupported key is
/// ignored, as VideoToolbox itself does).
unsafe fn set_i32_property(lib: &VtLib, session: *mut c_void, key: CFStringRef, value: i32) {
    if key.is_null() {
        return;
    }
    unsafe {
        let n = cf_i32(lib, value);
        if !n.is_null() {
            let _ = (lib.VTSessionSetProperty)(session, key, n);
            (lib.CFRelease)(n);
        }
    }
}

/// A fresh mutable CFDictionary with CFType callbacks, or null.
unsafe fn cf_dict(lib: &VtLib) -> *mut c_void {
    unsafe {
        (lib.CFDictionaryCreateMutable)(
            core::ptr::null(),
            0,
            lib.kCFTypeDictionaryKeyCallBacks,
            lib.kCFTypeDictionaryValueCallBacks,
        )
    }
}

/// `{ PixelFormatType: pixel_format, IOSurfaceProperties: {} }` plus the
/// width and height when given: the buffers a session hands out (the
/// encoder's source pool) or its decoder produces. IOSurface-backed, so
/// VideoToolbox, the GPU and the display can share them without a copy. The
/// caller releases the dictionary (null on failure).
unsafe fn buffer_attributes(
    lib: &VtLib,
    pixel_format: u32,
    size: Option<(u32, u32)>,
) -> *mut c_void {
    unsafe {
        let attrs = cf_dict(lib);
        if attrs.is_null() {
            return attrs;
        }
        let set_i32 = |key: CFStringRef, value: i32| {
            let n = cf_i32(lib, value);
            if !n.is_null() {
                (lib.CFDictionarySetValue)(attrs, key, n);
                (lib.CFRelease)(n);
            }
        };
        set_i32(lib.kCVPixelBufferPixelFormatTypeKey, pixel_format as i32);
        if let Some((w, h)) = size {
            set_i32(lib.kCVPixelBufferWidthKey, w as i32);
            set_i32(lib.kCVPixelBufferHeightKey, h as i32);
        }
        let io = cf_dict(lib);
        if !io.is_null() {
            (lib.CFDictionarySetValue)(attrs, lib.kCVPixelBufferIOSurfacePropertiesKey, io);
            (lib.CFRelease)(io);
        }
        attrs
    }
}

impl VtEncoder {
    /// Open a realtime H.264 VTCompressionSession, strongest settings first:
    /// hardware REQUIRED with low-latency rate control, hardware required
    /// alone, then whatever the OS picks (on every Mac VideoToolbox knows,
    /// that is hardware too). `None` if VideoToolbox is unavailable or no
    /// session opens (the caller keeps the stub).
    pub(super) fn open(width: u32, height: u32, bitrate_kbps: u32) -> Option<VtEncoder> {
        let lib = VtLib::get()?;
        let can_require =
            !lib.kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder.is_null();
        let can_low_latency =
            !lib.kVTVideoEncoderSpecification_EnableLowLatencyRateControl.is_null();
        for (require_hw, low_latency) in [(true, true), (true, false), (false, false)] {
            if (require_hw && !can_require) || (low_latency && !can_low_latency) {
                continue;
            }
            if let Some(enc) = Self::open_with(lib, width, height, bitrate_kbps, require_hw, low_latency)
            {
                return Some(enc);
            }
        }
        None
    }

    fn open_with(
        lib: &'static VtLib,
        width: u32,
        height: u32,
        bitrate_kbps: u32,
        require_hw: bool,
        low_latency: bool,
    ) -> Option<VtEncoder> {
        let shared = Arc::new(EncShared {
            chunks: Mutex::new(VecDeque::new()),
        });
        unsafe {
            // Encoder specification: hardware (required where asked), and
            // low-latency rate control where asked.
            let spec = cf_dict(lib);
            if !spec.is_null() {
                let hw = lib.kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder;
                if !hw.is_null() {
                    (lib.CFDictionarySetValue)(spec, hw, lib.kCFBooleanTrue);
                }
                if require_hw {
                    (lib.CFDictionarySetValue)(
                        spec,
                        lib.kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder,
                        lib.kCFBooleanTrue,
                    );
                }
                if low_latency {
                    (lib.CFDictionarySetValue)(
                        spec,
                        lib.kVTVideoEncoderSpecification_EnableLowLatencyRateControl,
                        lib.kCFBooleanTrue,
                    );
                }
            }
            // The source buffers: '420v' (the codec's own 4:2:0, so an NV12
            // camera frame is not converted), IOSurface-backed, from the
            // session's pool.
            let source = buffer_attributes(lib, PIXFMT_420V, Some((width, height)));

            let mut session: *mut c_void = core::ptr::null_mut();
            let refcon = Arc::as_ptr(&shared) as *mut c_void;
            let st = (lib.VTCompressionSessionCreate)(
                core::ptr::null(),
                width as i32,
                height as i32,
                CODEC_H264,
                spec,
                source,
                core::ptr::null(),
                enc_output,
                refcon,
                &mut session,
            );
            if !spec.is_null() {
                (lib.CFRelease)(spec);
            }
            if !source.is_null() {
                (lib.CFRelease)(source);
            }
            if st != 0 || session.is_null() {
                crate::plog_warn!(
                    "[video] VTCompressionSessionCreate ({}x{}, hardware required: {}, \
                     low-latency: {}) failed: {}",
                    width,
                    height,
                    require_hw,
                    low_latency,
                    st
                );
                return None;
            }
            // Realtime + no B-frames + no frame delay (low latency for
            // azul-meet) + bitrate.
            let _ = (lib.VTSessionSetProperty)(
                session,
                lib.kVTCompressionPropertyKey_RealTime,
                lib.kCFBooleanTrue,
            );
            let _ = (lib.VTSessionSetProperty)(
                session,
                lib.kVTCompressionPropertyKey_AllowFrameReordering,
                lib.kCFBooleanFalse,
            );
            // Low-latency rate control takes the High profiles; the classic
            // session keeps Main.
            let (profile, profile_name) =
                if low_latency && !lib.kVTProfileLevel_H264_High_AutoLevel.is_null() {
                    (lib.kVTProfileLevel_H264_High_AutoLevel, "High")
                } else {
                    (lib.kVTProfileLevel_H264_Main_AutoLevel, "Main")
                };
            let _ = (lib.VTSessionSetProperty)(
                session,
                lib.kVTCompressionPropertyKey_ProfileLevel,
                profile,
            );
            set_i32_property(
                lib,
                session,
                lib.kVTCompressionPropertyKey_AverageBitRate,
                (bitrate_kbps.max(64) as i32).saturating_mul(1000),
            );
            set_i32_property(lib, session, lib.kVTCompressionPropertyKey_MaxKeyFrameInterval, 60);
            set_i32_property(lib, session, lib.kVTCompressionPropertyKey_ExpectedFrameRate, 30);
            set_i32_property(lib, session, lib.kVTCompressionPropertyKey_MaxFrameDelayCount, 0);
            // Colour tags (Rec.709, the camera's HD matrix): the decoder's
            // NV12 then says which matrix it is in.
            for (key, value) in [
                (
                    lib.kVTCompressionPropertyKey_YCbCrMatrix,
                    lib.kCVImageBufferYCbCrMatrix_ITU_R_709_2,
                ),
                (
                    lib.kVTCompressionPropertyKey_ColorPrimaries,
                    lib.kCVImageBufferColorPrimaries_ITU_R_709_2,
                ),
                (
                    lib.kVTCompressionPropertyKey_TransferFunction,
                    lib.kCVImageBufferTransferFunction_ITU_R_709_2,
                ),
            ] {
                if !key.is_null() && !value.is_null() {
                    let _ = (lib.VTSessionSetProperty)(session, key, value);
                }
            }
            let _ = (lib.VTCompressionSessionPrepareToEncodeFrames)(session);

            // What did it get? The hardware query is refused (-12900) while
            // low-latency rate control is on; hardware REQUIRED answers it.
            let mut hardware = if require_hw { Some(true) } else { None };
            let query = lib.kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder;
            if !query.is_null() {
                let mut value: CFTypeRef = core::ptr::null();
                let st = (lib.VTSessionCopyProperty)(session, query, core::ptr::null(), &mut value);
                if st == 0 && !value.is_null() {
                    hardware = Some((lib.CFBooleanGetValue)(value) != 0);
                    (lib.CFRelease)(value);
                }
            }
            let settings = EncoderSettings {
                hardware,
                low_latency,
                realtime: true,
                frame_reordering: false,
                profile: profile_name,
            };
            crate::plog_info!(
                "[video] VideoToolbox H.264 encoder open: {}x{} @{}kbps, {:?}",
                width,
                height,
                bitrate_kbps,
                settings
            );
            Some(VtEncoder {
                session,
                shared,
                width,
                height,
                started: std::time::Instant::now(),
                settings,
            })
        }
    }

    /// What this session got (hardware, low latency, profile).
    pub(super) fn settings(&self) -> EncoderSettings {
        self.settings
    }

    /// A '420v' buffer holding `frame`'s two planes: from the session's own
    /// pool (IOSurface-backed, what VideoToolbox encodes without a copy of
    /// its own), else a plain one. The planes are copied row by row into the
    /// buffer's strides - no conversion. `None` for a short frame.
    unsafe fn nv12_buffer(&self, lib: &VtLib, frame: &VideoFrame) -> Option<*mut c_void> {
        let (w, h) = (self.width as usize, self.height as usize);
        let layout = Nv12Layout::new(w, h);
        let bytes = frame.bytes.as_ref();
        if bytes.len() < layout.checked_total_len()? {
            return None;
        }
        unsafe {
            let mut pb: *mut c_void = core::ptr::null_mut();
            let pool = (lib.VTCompressionSessionGetPixelBufferPool)(self.session);
            let pooled = !pool.is_null()
                && (lib.CVPixelBufferPoolCreatePixelBuffer)(core::ptr::null(), pool, &mut pb) == 0
                && !pb.is_null();
            if pooled {
                let format = (lib.CVPixelBufferGetPixelFormatType)(pb);
                if format != PIXFMT_420V && format != PIXFMT_420F {
                    (lib.CFRelease)(pb);
                    pb = core::ptr::null_mut();
                }
            } else {
                pb = core::ptr::null_mut();
            }
            if pb.is_null()
                && ((lib.CVPixelBufferCreate)(
                    core::ptr::null(),
                    w,
                    h,
                    PIXFMT_420V,
                    core::ptr::null(),
                    &mut pb,
                ) != 0
                    || pb.is_null())
            {
                return None;
            }
            (lib.CVPixelBufferLockBaseAddress)(pb, 0);
            let y = (lib.CVPixelBufferGetBaseAddressOfPlane)(pb, 0);
            let y_stride = (lib.CVPixelBufferGetBytesPerRowOfPlane)(pb, 0);
            let uv = (lib.CVPixelBufferGetBaseAddressOfPlane)(pb, 1);
            let uv_stride = (lib.CVPixelBufferGetBytesPerRowOfPlane)(pb, 1);
            let uv_row = layout.chroma_width * 2;
            let ok = !y.is_null() && !uv.is_null() && y_stride >= w && uv_stride >= uv_row;
            if ok {
                for row in 0..h {
                    core::ptr::copy_nonoverlapping(
                        bytes.as_ptr().add(row * w),
                        y.add(row * y_stride),
                        w,
                    );
                }
                let chroma = bytes.as_ptr().add(layout.y_len());
                for row in 0..layout.chroma_height {
                    core::ptr::copy_nonoverlapping(
                        chroma.add(row * uv_row),
                        uv.add(row * uv_stride),
                        uv_row,
                    );
                }
            }
            (lib.CVPixelBufferUnlockBaseAddress)(pb, 0);
            if !ok {
                (lib.CFRelease)(pb);
                return None;
            }
            Some(pb)
        }
    }

    /// A BGRA buffer holding `frame` (BGRA8 rows copied as they are, RGBA8
    /// rows swizzled). VideoToolbox converts it to 4:2:0 itself. `None` for a
    /// short frame.
    unsafe fn bgra_buffer(&self, lib: &VtLib, frame: &VideoFrame) -> Option<*mut c_void> {
        let (w, h) = (self.width as usize, self.height as usize);
        let bytes = frame.bytes.as_ref();
        if bytes.len() < w * h * 4 {
            return None;
        }
        unsafe {
            let mut pb: *mut c_void = core::ptr::null_mut();
            if (lib.CVPixelBufferCreate)(
                core::ptr::null(),
                w,
                h,
                PIXFMT_BGRA,
                core::ptr::null(),
                &mut pb,
            ) != 0
                || pb.is_null()
            {
                return None;
            }
            (lib.CVPixelBufferLockBaseAddress)(pb, 0);
            let base = (lib.CVPixelBufferGetBaseAddress)(pb);
            let stride = (lib.CVPixelBufferGetBytesPerRow)(pb);
            if base.is_null() || stride < w * 4 {
                (lib.CVPixelBufferUnlockBaseAddress)(pb, 0);
                (lib.CFRelease)(pb);
                return None;
            }
            let swizzle = frame.format != RawImageFormat::BGRA8;
            for y in 0..h {
                let src = &bytes[y * w * 4..(y + 1) * w * 4];
                let dst = std::slice::from_raw_parts_mut(base.add(y * stride), w * 4);
                if swizzle {
                    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                        d[0] = s[2]; // B
                        d[1] = s[1]; // G
                        d[2] = s[0]; // R
                        d[3] = 255;
                    }
                } else {
                    dst.copy_from_slice(src);
                }
            }
            (lib.CVPixelBufferUnlockBaseAddress)(pb, 0);
            Some(pb)
        }
    }

    /// Encode one frame (NV12, BGRA8 or RGBA8, at the session's size) →
    /// Annex-B chunk(s). Empty while VT buffers, or for a frame of the wrong
    /// size.
    pub(super) fn encode(&mut self, frame: &VideoFrame, force_keyframe: bool) -> Vec<u8> {
        let lib = match VtLib::get() {
            Some(l) => l,
            None => return Vec::new(),
        };
        if frame.width != self.width || frame.height != self.height {
            return Vec::new();
        }
        unsafe {
            let pb = if frame.format.is_nv12() {
                self.nv12_buffer(lib, frame)
            } else {
                self.bgra_buffer(lib, frame)
            };
            let Some(pb) = pb else {
                return Vec::new();
            };

            let micros = i64::try_from(self.started.elapsed().as_micros()).unwrap_or(i64::MAX);
            let pts = CMTime::new(micros, 1_000_000);
            let mut props: CFDictionaryRef = core::ptr::null();
            let mut props_owned: *mut c_void = core::ptr::null_mut();
            if force_keyframe {
                props_owned = cf_dict(lib);
                if !props_owned.is_null() {
                    (lib.CFDictionarySetValue)(
                        props_owned,
                        lib.kVTEncodeFrameOptionKey_ForceKeyFrame,
                        lib.kCFBooleanTrue,
                    );
                    props = props_owned;
                }
            }
            let st = (lib.VTCompressionSessionEncodeFrame)(
                self.session,
                pb,
                pts,
                CMTime::invalid(),
                props,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            );
            if !props_owned.is_null() {
                (lib.CFRelease)(props_owned);
            }
            (lib.CFRelease)(pb);
            if st != 0 {
                crate::plog_warn!("[video] VTCompressionSessionEncodeFrame failed: {}", st);
                return Vec::new();
            }
            // Realtime session, no reordering, no frame delay: force emission
            // of this frame so encode() behaves synchronously for the caller
            // (the keyframe policy reads each frame's packets right after it).
            let _ = (lib.VTCompressionSessionCompleteFrames)(self.session, pts);
        }
        // Drain everything queued (usually exactly one chunk).
        let mut out = Vec::new();
        if let Ok(mut q) = self.shared.chunks.lock() {
            while let Some(c) = q.pop_front() {
                out.extend_from_slice(&c);
            }
        }
        out
    }
}

impl Drop for VtEncoder {
    fn drop(&mut self) {
        if let Some(lib) = VtLib::get() {
            unsafe {
                (lib.VTCompressionSessionCompleteFrames)(self.session, CMTime::invalid());
                (lib.VTCompressionSessionInvalidate)(self.session);
                (lib.CFRelease)(self.session);
            }
        }
    }
}


// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

/// Frames produced by the VT decode callback, in the asked-for format.
struct DecShared {
    frames: Mutex<VecDeque<VideoFrame>>,
    /// The app asked for RGBA8 (the old contract): a BGRA buffer is
    /// swizzled. NV12 and BGRA8 are handed out as they are.
    want_rgba: std::sync::atomic::AtomicBool,
}

/// What the decoder hands out: the pixel format and the size. A change waits
/// for the next keyframe (a session recreated mid-GOP has no reference
/// frames).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecoderOutput {
    format: RawImageFormat,
    /// `None`: the stream's own size.
    size: Option<(u32, u32)>,
}

/// A live VTDecompressionSession fed Annex-B H.264.
pub(super) struct VtDecoder {
    session: *mut c_void,
    format_desc: *mut c_void,
    shared: Arc<DecShared>,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    frame_idx: i64,
    /// The output the session was (or will be) created for.
    output: DecoderOutput,
    /// An output change waiting for the next keyframe.
    pending_output: Option<DecoderOutput>,
}

unsafe impl Send for VtDecoder {}

/// VTDecompressionOutputCallback: the decoded CVPixelBuffer → a `VideoFrame`
/// in the buffer's own form - NV12 plane by plane ('420v' / '420f', its
/// matrix from the buffer's YCbCr attachment), BGRA rows as they are, or
/// (asked for RGBA8) swizzled.
extern "C" fn dec_output(
    refcon: *mut c_void,
    _src: *mut c_void,
    status: OSStatus,
    _flags: u32,
    image: *mut c_void,
    _pts: CMTime,
    _duration: CMTime,
) {
    if status != 0 || image.is_null() || refcon.is_null() {
        return;
    }
    let lib = match VtLib::get() {
        Some(l) => l,
        None => return,
    };
    let shared = unsafe { &*(refcon as *const DecShared) };
    unsafe {
        (lib.CVPixelBufferLockBaseAddress)(image, 1 /* kCVPixelBufferLock_ReadOnly */);
        let frame = copy_decoded(lib, image, shared);
        (lib.CVPixelBufferUnlockBaseAddress)(image, 1);
        if let (Some(frame), Ok(mut q)) = (frame, shared.frames.lock()) {
            q.push_back(frame);
        }
    }
}

/// The frame of a locked decoder output buffer (see [`dec_output`]).
unsafe fn copy_decoded(lib: &VtLib, image: *mut c_void, shared: &DecShared) -> Option<VideoFrame> {
    unsafe {
        let w = (lib.CVPixelBufferGetWidth)(image);
        let h = (lib.CVPixelBufferGetHeight)(image);
        if w == 0 || h == 0 {
            return None;
        }
        let pixel_format = (lib.CVPixelBufferGetPixelFormatType)(image);
        if pixel_format == PIXFMT_420V || pixel_format == PIXFMT_420F {
            let layout = Nv12Layout::new(w, h);
            let y = (lib.CVPixelBufferGetBaseAddressOfPlane)(image, 0);
            let y_stride = (lib.CVPixelBufferGetBytesPerRowOfPlane)(image, 0);
            let uv = (lib.CVPixelBufferGetBaseAddressOfPlane)(image, 1);
            let uv_stride = (lib.CVPixelBufferGetBytesPerRowOfPlane)(image, 1);
            let uv_row = layout.chroma_width * 2;
            if y.is_null() || uv.is_null() || y_stride < w || uv_stride < uv_row {
                return None;
            }
            // Not zeroed: every byte is written right below.
            let mut out = Vec::with_capacity(layout.checked_total_len()?);
            for row in 0..h {
                out.extend_from_slice(std::slice::from_raw_parts(y.add(row * y_stride), w));
            }
            for row in 0..layout.chroma_height {
                out.extend_from_slice(std::slice::from_raw_parts(uv.add(row * uv_stride), uv_row));
            }
            let matrix = (lib.CVBufferGetAttachment)(
                image,
                lib.kCVImageBufferYCbCrMatrixKey,
                core::ptr::null_mut(),
            );
            let rec601 = if matrix.is_null() {
                h < 720
            } else {
                matrix == lib.kCVImageBufferYCbCrMatrix_ITU_R_601_4
            };
            return Some(VideoFrame::with_format(
                w as u32,
                h as u32,
                U8Vec::from_vec(out),
                RawImageFormat::nv12(!rec601, pixel_format == PIXFMT_420F),
            ));
        }
        let stride = (lib.CVPixelBufferGetBytesPerRow)(image);
        let base = (lib.CVPixelBufferGetBaseAddress)(image);
        if base.is_null() || stride < w * 4 {
            return None;
        }
        let want_rgba = shared
            .want_rgba
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let row = std::slice::from_raw_parts(base.add(y * stride), w * 4);
            if want_rgba {
                for px in row.chunks_exact(4) {
                    out.extend_from_slice(&[px[2], px[1], px[0], 255]);
                }
            } else {
                out.extend_from_slice(row);
            }
        }
        let format = if want_rgba {
            RawImageFormat::RGBA8
        } else {
            RawImageFormat::BGRA8
        };
        Some(VideoFrame::with_format(
            w as u32,
            h as u32,
            U8Vec::from_vec(out),
            format,
        ))
    }
}

impl VtDecoder {
    /// `None` if VideoToolbox is unavailable (caller keeps the stub). The
    /// session itself is created lazily once SPS+PPS arrive in the stream.
    /// Hands out RGBA8 at the stream's size until told otherwise.
    pub(super) fn open_h264() -> Option<VtDecoder> {
        VtLib::get()?;
        crate::plog_info!("[video] VideoToolbox H.264 decoder open (session on first SPS/PPS)");
        Some(VtDecoder {
            session: core::ptr::null_mut(),
            format_desc: core::ptr::null_mut(),
            shared: Arc::new(DecShared {
                frames: Mutex::new(VecDeque::new()),
                want_rgba: std::sync::atomic::AtomicBool::new(true),
            }),
            sps: None,
            pps: None,
            frame_idx: 0,
            output: DecoderOutput {
                format: RawImageFormat::RGBA8,
                size: None,
            },
            pending_output: None,
        })
    }

    /// Hand frames out in `format`: an NV12 variant (the decoder's own 4:2:0,
    /// no conversion - for a YUV tile or the CPU's fused convert), BGRA8 (as
    /// VideoToolbox renders it) or RGBA8 (swizzled; the default).
    pub(super) fn set_output_format(&mut self, format: RawImageFormat) {
        let next = DecoderOutput {
            format,
            ..self.pending_output.unwrap_or(self.output)
        };
        self.request_output(next);
    }

    /// Hand frames out at `width` x `height` - VideoToolbox scales in the
    /// decode session, so a 720p stream shown in a small tile never reaches
    /// the CPU at 720p. `0 x 0`: the stream's own size.
    pub(super) fn set_output_size(&mut self, width: u32, height: u32) {
        let size = (width > 0 && height > 0).then_some((width, height));
        let next = DecoderOutput {
            size,
            ..self.pending_output.unwrap_or(self.output)
        };
        self.request_output(next);
    }

    /// Apply an output change now if no session runs yet, else at the next
    /// keyframe.
    fn request_output(&mut self, next: DecoderOutput) {
        if next == self.output {
            self.pending_output = None;
        } else if self.session.is_null() {
            self.output = next;
            self.pending_output = None;
        } else {
            self.pending_output = Some(next);
        }
    }

    /// (Re)create the decompression session from the current SPS/PPS.
    fn ensure_session(&mut self) -> bool {
        if !self.session.is_null() {
            return true;
        }
        let (lib, sps, pps) = match (VtLib::get(), self.sps.as_ref(), self.pps.as_ref()) {
            (Some(l), Some(s), Some(p)) => (l, s, p),
            _ => return false,
        };
        unsafe {
            let ptrs = [sps.as_ptr(), pps.as_ptr()];
            let sizes = [sps.len(), pps.len()];
            let mut desc: *mut c_void = core::ptr::null_mut();
            let st = (lib.CMVideoFormatDescriptionCreateFromH264ParameterSets)(
                core::ptr::null(),
                2,
                ptrs.as_ptr(),
                sizes.as_ptr(),
                4,
                &mut desc,
            );
            if st != 0 || desc.is_null() {
                crate::plog_warn!("[video] H264 format description failed: {}", st);
                return false;
            }
            // The output buffers: NV12 ('420v' / '420f') or BGRA, IOSurface-
            // backed, at the asked size (VideoToolbox scales).
            let format = self.output.format;
            let pixel_format = if format.is_nv12() {
                if format.is_full_range() {
                    PIXFMT_420F
                } else {
                    PIXFMT_420V
                }
            } else {
                PIXFMT_BGRA
            };
            self.shared.want_rgba.store(
                format == RawImageFormat::RGBA8,
                std::sync::atomic::Ordering::Relaxed,
            );
            let attrs = buffer_attributes(lib, pixel_format, self.output.size);
            let record = VTDecompressionOutputCallbackRecord {
                callback: dec_output,
                refcon: Arc::as_ptr(&self.shared) as *mut c_void,
            };
            let mut session: *mut c_void = core::ptr::null_mut();
            let st = (lib.VTDecompressionSessionCreate)(
                core::ptr::null(),
                desc,
                core::ptr::null(),
                attrs,
                &record,
                &mut session,
            );
            if !attrs.is_null() {
                (lib.CFRelease)(attrs);
            }
            if st != 0 || session.is_null() {
                (lib.CFRelease)(desc);
                crate::plog_warn!("[video] VTDecompressionSessionCreate failed: {}", st);
                return false;
            }
            self.format_desc = desc;
            self.session = session;
            crate::plog_info!(
                "[video] VideoToolbox decompression session created ({:?} out, size {:?})",
                self.output.format,
                self.output.size
            );
            true
        }
    }

    /// Feed one Annex-B chunk; decoded frames appear via the callback
    /// (synchronous decode → typically before this returns).
    pub(super) fn decode(&mut self, data: &[u8]) -> Vec<VideoFrame> {
        let lib = match VtLib::get() {
            Some(l) => l,
            None => return Vec::new(),
        };
        // Split NALs; latch parameter sets; batch VCL NALs into one AU.
        let mut au: Vec<u8> = Vec::with_capacity(data.len() + 16);
        let mut keyframe = false;
        for nal in annexb_nals(data) {
            if nal.is_empty() {
                continue;
            }
            match nal[0] & 0x1f {
                7 => {
                    if self.sps.as_deref() != Some(nal) {
                        self.sps = Some(nal.to_vec());
                        // New parameter sets → session must be rebuilt.
                        self.reset_session();
                    }
                }
                8 => {
                    if self.pps.as_deref() != Some(nal) {
                        self.pps = Some(nal.to_vec());
                        self.reset_session();
                    }
                }
                kind => {
                    keyframe |= kind == 5;
                    au.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                    au.extend_from_slice(nal);
                }
            }
        }
        // A new output (format / size) starts at a keyframe: the session
        // recreated for it has no reference frames for anything before.
        if keyframe {
            if let Some(next) = self.pending_output.take() {
                self.output = next;
                self.reset_session();
            }
        }
        if au.is_empty() || !self.ensure_session() {
            return self.drain();
        }
        unsafe {
            // Copy the AU into a CMBlockBuffer.
            let mut bb: *mut c_void = core::ptr::null_mut();
            let st = (lib.CMBlockBufferCreateWithMemoryBlock)(
                core::ptr::null(),
                core::ptr::null_mut(),
                au.len(),
                core::ptr::null(), // kCFAllocatorDefault → CM allocates
                core::ptr::null(),
                0,
                au.len(),
                0,
                &mut bb,
            );
            if st != 0 || bb.is_null() {
                return self.drain();
            }
            if (lib.CMBlockBufferReplaceDataBytes)(au.as_ptr() as *const c_void, bb, 0, au.len())
                != 0
            {
                (lib.CFRelease)(bb);
                return self.drain();
            }
            let timing = CMSampleTimingInfo {
                duration: CMTime::new(1, 30),
                presentation_time_stamp: CMTime::new(self.frame_idx, 30),
                decode_time_stamp: CMTime::invalid(),
            };
            self.frame_idx += 1;
            let sizes = [au.len()];
            let mut sample: *mut c_void = core::ptr::null_mut();
            let st = (lib.CMSampleBufferCreateReady)(
                core::ptr::null(),
                bb,
                self.format_desc,
                1,
                1,
                &timing,
                1,
                sizes.as_ptr(),
                &mut sample,
            );
            if st == 0 && !sample.is_null() {
                // flags = 0 → synchronous decode (callback fires inline).
                let st = (lib.VTDecompressionSessionDecodeFrame)(
                    self.session,
                    sample,
                    0,
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                );
                if st != 0 {
                    crate::plog_warn!("[video] VT decode failed: {}", st);
                }
                (lib.CFRelease)(sample);
            }
            (lib.CFRelease)(bb);
        }
        self.drain()
    }

    /// End-of-stream: nothing is held back (synchronous decode, no B-frame
    /// delay in this profile) — just drain the queue.
    pub(super) fn flush(&mut self) -> Vec<VideoFrame> {
        self.drain()
    }

    fn drain(&mut self) -> Vec<VideoFrame> {
        match self.shared.frames.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn reset_session(&mut self) {
        if let Some(lib) = VtLib::get() {
            unsafe {
                if !self.session.is_null() {
                    (lib.VTDecompressionSessionInvalidate)(self.session);
                    (lib.CFRelease)(self.session);
                    self.session = core::ptr::null_mut();
                }
                if !self.format_desc.is_null() {
                    (lib.CFRelease)(self.format_desc);
                    self.format_desc = core::ptr::null_mut();
                }
            }
        }
    }
}

impl Drop for VtDecoder {
    fn drop(&mut self) {
        self.reset_session();
    }
}


// ---------------------------------------------------------------------------
// Tests — a real encode → decode roundtrip on Apple hardware (no TCC needed).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod vt_tests {
    use super::*;

    /// Encode 30 synthetic RGBA frames → Annex-B → decode them back. Verifies
    /// the whole VT session + Annex-B/AVCC conversion works on this machine.
    #[test]
    fn videotoolbox_roundtrip() {
        if VtLib::get().is_none() {
            eprintln!("VideoToolbox unavailable — skipping roundtrip test");
            return;
        }
        let (w, h) = (320u32, 240u32);
        let mut enc = VtEncoder::open(w, h, 800).expect("encoder open");
        let mut dec = VtDecoder::open_h264().expect("decoder open");

        let mut encoded_total = 0usize;
        let mut decoded = 0usize;
        for f in 0..30u32 {
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
                px[0] = ((i as u32 + f * 7) % 255) as u8;
                px[1] = (f * 8) as u8;
                px[2] = 128;
                px[3] = 255;
            }
            let frame = VideoFrame::new(w, h, U8Vec::from_vec(rgba));
            let chunk = enc.encode(&frame, f == 0);
            encoded_total += chunk.len();
            if !chunk.is_empty() {
                for frame in dec.decode(&chunk) {
                    assert_eq!(frame.width, w);
                    assert_eq!(frame.height, h);
                    decoded += 1;
                }
            }
        }
        for frame in dec.flush() {
            assert_eq!(frame.width, w);
            decoded += 1;
        }
        eprintln!(
            "VideoToolbox roundtrip: {} bytes encoded, {} frames decoded",
            encoded_total, decoded
        );
        assert!(encoded_total > 0, "encoder produced no bytes");
        assert!(
            decoded >= 20,
            "decoder produced too few frames ({})",
            decoded
        );
    }

    /// A 320x240 NV12 frame: a luma ramp moving with `f`, neutral chroma.
    fn nv12_frame(f: u32, w: u32, h: u32) -> VideoFrame {
        use azul_core::resources::{Nv12Layout, RawImageFormat};
        let layout = Nv12Layout::new(w as usize, h as usize);
        let mut bytes = Vec::with_capacity(layout.checked_total_len().expect("small"));
        for i in 0..layout.y_len() {
            bytes.push(((i as u32 + f * 7) % 219 + 16) as u8);
        }
        bytes.resize(bytes.len() + layout.uv_len(), 128);
        VideoFrame::with_format(w, h, U8Vec::from_vec(bytes), RawImageFormat::NV12Rec709Video)
    }

    /// NV12 end to end: the camera's frame goes into the encoder without a
    /// conversion (a pooled '420v' buffer), and the decoder hands NV12 back
    /// for a YUV tile.
    #[test]
    fn an_nv12_frame_encodes_without_a_conversion_and_decodes_back_as_nv12() {
        use azul_core::resources::RawImageFormat;
        if VtLib::get().is_none() {
            eprintln!("VideoToolbox unavailable — skipping");
            return;
        }
        let (w, h) = (320u32, 240u32);
        let mut enc = VtEncoder::open(w, h, 800).expect("encoder open");
        let mut dec = VtDecoder::open_h264().expect("decoder open");
        dec.set_output_format(RawImageFormat::NV12Rec709Video);
        let mut decoded = Vec::new();
        for f in 0..30u32 {
            let chunk = enc.encode(&nv12_frame(f, w, h), f == 0);
            if !chunk.is_empty() {
                decoded.extend(dec.decode(&chunk));
            }
        }
        decoded.extend(dec.flush());
        assert!(decoded.len() >= 20, "{} frames decoded", decoded.len());
        for frame in &decoded {
            assert!(frame.format.is_nv12(), "decoded as {:?}", frame.format);
            assert_eq!((frame.width, frame.height), (w, h));
            assert_eq!(Some(frame.bytes.as_ref().len()), frame.expected_len());
        }
    }

    /// The decoder scales in hardware to the size the tile needs, so a
    /// 720p stream in a 300x200 tile never reaches the CPU at 720p.
    #[test]
    fn the_decoder_hands_frames_out_at_the_asked_size() {
        use azul_core::resources::RawImageFormat;
        if VtLib::get().is_none() {
            eprintln!("VideoToolbox unavailable — skipping");
            return;
        }
        let (w, h) = (320u32, 240u32);
        let mut enc = VtEncoder::open(w, h, 800).expect("encoder open");
        let mut dec = VtDecoder::open_h264().expect("decoder open");
        dec.set_output_format(RawImageFormat::NV12Rec709Video);
        dec.set_output_size(160, 120);
        let mut sizes = Vec::new();
        for f in 0..10u32 {
            let chunk = enc.encode(&nv12_frame(f, w, h), f == 0);
            if !chunk.is_empty() {
                sizes.extend(dec.decode(&chunk).iter().map(|fr| (fr.width, fr.height)));
            }
        }
        assert!(!sizes.is_empty());
        assert!(sizes.iter().all(|s| *s == (160, 120)), "{sizes:?}");
    }

    /// Encode runs in hardware with the realtime, no-reordering settings,
    /// and says what it got (the log line AzMeet's report quotes).
    #[test]
    fn the_encoder_says_it_runs_in_hardware_in_realtime() {
        if VtLib::get().is_none() {
            eprintln!("VideoToolbox unavailable — skipping");
            return;
        }
        let enc = VtEncoder::open(640, 360, 800).expect("encoder open");
        let settings = enc.settings();
        eprintln!("VideoToolbox encoder settings: {settings:?}");
        assert!(settings.realtime);
        assert!(!settings.frame_reordering);
        assert_ne!(
            settings.hardware,
            Some(false),
            "a software H.264 encoder on this Mac"
        );
    }

    /// Noise frames: every pixel differs from frame to frame, so the
    /// encoder spends every bit the rate control allows.
    fn noise_frame(f: u32, w: u32, h: u32) -> VideoFrame {
        let mut state = 0x9E37_79B9u32 ^ f.wrapping_mul(0x85EB_CA6B);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_exact_mut(4) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            px.copy_from_slice(&[state as u8, (state >> 8) as u8, (state >> 16) as u8, 255]);
        }
        VideoFrame::new(w, h, U8Vec::from_vec(rgba))
    }

    /// An EXPORT submits frames as fast as it renders them, far faster than
    /// they play. Stamped with the wall clock (what `encode` does for a
    /// live call), sixty frames rendered in a fraction of a second are a
    /// fraction of a second of video to the rate control, which then gives
    /// them a fraction of the bitrate; stamped with the frames' own times
    /// (`encode_at`), they are two seconds of video and get two seconds'
    /// worth of bits.
    #[test]
    fn frames_stamped_with_their_own_times_get_the_bitrate_of_their_duration() {
        if VtLib::get().is_none() {
            eprintln!("VideoToolbox unavailable — skipping");
            return;
        }
        let (w, h, kbps) = (320u32, 240u32, 1000u32);
        let mut live = VtEncoder::open(w, h, kbps).expect("encoder open");
        let mut offline = VtEncoder::open(w, h, kbps).expect("encoder open");
        let (mut live_bytes, mut offline_bytes) = (0usize, 0usize);
        for f in 0..60u32 {
            let frame = noise_frame(f, w, h);
            live_bytes += live.encode(&frame, f == 0).len();
            offline_bytes += offline
                .encode_at(&frame, f == 0, i64::from(f) * 33_333)
                .len();
        }
        eprintln!("60 noise frames at {kbps} kbps: wall clock {live_bytes} B, own times {offline_bytes} B");
        assert!(offline_bytes > 0 && live_bytes > 0);
        assert!(
            offline_bytes >= 2 * live_bytes,
            "frames 1/30 s apart must get their duration's bits: {offline_bytes} B vs {live_bytes} B \
             stamped with the wall clock"
        );
    }
}
