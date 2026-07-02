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
//!   - encode: SPS/PPS pulled from the output format description and emitted
//!     in-band ahead of every keyframe; each length-prefixed NAL rewritten
//!     with `00 00 00 01` start codes.
//!   - decode: SPS(7)/PPS(8) NALs collected from the Annex-B stream feed
//!     `CMVideoFormatDescriptionCreateFromH264ParameterSets`; VCL NALs are
//!     re-prefixed with 4-byte lengths and wrapped in a `CMSampleBuffer`.
//!
//! Input frames are RGBA8 (the toolkit's universal frame format); the Apple
//! encoders accept `kCVPixelFormatType_32BGRA` directly (an internal
//! VTPixelTransferSession converts to the codec's 4:2:0), so encode does one
//! cheap RGBA→BGRA swizzle. Decode requests BGRA output for the same reason.
//! H.265 is not wired yet (the demos are H.264, same scope as the Vulkan
//! backend) — `open(h265=true)` yields the stub.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

use azul_core::video::VideoFrame;
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
        CMTime { value, timescale, flags: 1, epoch: 0 }
    }
    fn invalid() -> Self {
        CMTime { value: 0, timescale: 0, flags: 0, epoch: 0 }
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
            kCMSampleAttachmentKey_NotSync: CFStringRef,
            kCVPixelBufferPixelFormatTypeKey: CFStringRef,
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
            let open = |p: &str| libloading::Library::new(p).ok();
            let vt = open("/System/Library/Frameworks/VideoToolbox.framework/VideoToolbox")?;
            let cm = open("/System/Library/Frameworks/CoreMedia.framework/CoreMedia")?;
            let cv = open("/System/Library/Frameworks/CoreVideo.framework/CoreVideo")?;
            let cf =
                open("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")?;

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
                VTCompressionSessionPrepareToEncodeFrames:
                    f!(vt, b"VTCompressionSessionPrepareToEncodeFrames\0"),
                VTCompressionSessionEncodeFrame: f!(vt, b"VTCompressionSessionEncodeFrame\0"),
                VTCompressionSessionCompleteFrames:
                    f!(vt, b"VTCompressionSessionCompleteFrames\0"),
                VTCompressionSessionInvalidate: f!(vt, b"VTCompressionSessionInvalidate\0"),
                VTDecompressionSessionCreate: f!(vt, b"VTDecompressionSessionCreate\0"),
                VTDecompressionSessionDecodeFrame:
                    f!(vt, b"VTDecompressionSessionDecodeFrame\0"),
                VTDecompressionSessionInvalidate: f!(vt, b"VTDecompressionSessionInvalidate\0"),
                CMVideoFormatDescriptionCreateFromH264ParameterSets:
                    f!(cm, b"CMVideoFormatDescriptionCreateFromH264ParameterSets\0"),
                CMVideoFormatDescriptionGetH264ParameterSetAtIndex:
                    f!(cm, b"CMVideoFormatDescriptionGetH264ParameterSetAtIndex\0"),
                CMBlockBufferCreateWithMemoryBlock:
                    f!(cm, b"CMBlockBufferCreateWithMemoryBlock\0"),
                CMBlockBufferReplaceDataBytes: f!(cm, b"CMBlockBufferReplaceDataBytes\0"),
                CMBlockBufferGetDataPointer: f!(cm, b"CMBlockBufferGetDataPointer\0"),
                CMSampleBufferCreateReady: f!(cm, b"CMSampleBufferCreateReady\0"),
                CMSampleBufferGetDataBuffer: f!(cm, b"CMSampleBufferGetDataBuffer\0"),
                CMSampleBufferGetFormatDescription:
                    f!(cm, b"CMSampleBufferGetFormatDescription\0"),
                CMSampleBufferGetSampleAttachmentsArray:
                    f!(cm, b"CMSampleBufferGetSampleAttachmentsArray\0"),
                CVPixelBufferCreate: f!(cv, b"CVPixelBufferCreate\0"),
                CVPixelBufferLockBaseAddress: f!(cv, b"CVPixelBufferLockBaseAddress\0"),
                CVPixelBufferUnlockBaseAddress: f!(cv, b"CVPixelBufferUnlockBaseAddress\0"),
                CVPixelBufferGetBaseAddress: f!(cv, b"CVPixelBufferGetBaseAddress\0"),
                CVPixelBufferGetBytesPerRow: f!(cv, b"CVPixelBufferGetBytesPerRow\0"),
                CVPixelBufferGetWidth: f!(cv, b"CVPixelBufferGetWidth\0"),
                CVPixelBufferGetHeight: f!(cv, b"CVPixelBufferGetHeight\0"),
                CFRelease: f!(cf, b"CFRelease\0"),
                CFDictionaryCreateMutable: f!(cf, b"CFDictionaryCreateMutable\0"),
                CFDictionarySetValue: f!(cf, b"CFDictionarySetValue\0"),
                CFDictionaryGetValue: f!(cf, b"CFDictionaryGetValue\0"),
                CFNumberCreate: f!(cf, b"CFNumberCreate\0"),
                CFArrayGetCount: f!(cf, b"CFArrayGetCount\0"),
                CFArrayGetValueAtIndex: f!(cf, b"CFArrayGetValueAtIndex\0"),
                CFBooleanGetValue: f!(cf, b"CFBooleanGetValue\0"),
                kVTCompressionPropertyKey_RealTime:
                    d!(vt, b"kVTCompressionPropertyKey_RealTime\0"),
                kVTCompressionPropertyKey_AverageBitRate:
                    d!(vt, b"kVTCompressionPropertyKey_AverageBitRate\0"),
                kVTCompressionPropertyKey_AllowFrameReordering:
                    d!(vt, b"kVTCompressionPropertyKey_AllowFrameReordering\0"),
                kVTCompressionPropertyKey_MaxKeyFrameInterval:
                    d!(vt, b"kVTCompressionPropertyKey_MaxKeyFrameInterval\0"),
                kVTCompressionPropertyKey_ProfileLevel:
                    d!(vt, b"kVTCompressionPropertyKey_ProfileLevel\0"),
                kVTProfileLevel_H264_Main_AutoLevel:
                    d!(vt, b"kVTProfileLevel_H264_Main_AutoLevel\0"),
                kVTEncodeFrameOptionKey_ForceKeyFrame:
                    d!(vt, b"kVTEncodeFrameOptionKey_ForceKeyFrame\0"),
                kCMSampleAttachmentKey_NotSync: d!(cm, b"kCMSampleAttachmentKey_NotSync\0"),
                kCVPixelBufferPixelFormatTypeKey:
                    d!(cv, b"kCVPixelBufferPixelFormatTypeKey\0"),
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

/// Iterate NAL units of an Annex-B stream (3- or 4-byte start codes),
/// yielding each NAL's payload slice (start code stripped).
fn annexb_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut start: Option<usize> = None;
    while i + 3 <= data.len() {
        let (is_sc, sc_len) = if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                (true, 3)
            } else if i + 4 <= data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                (true, 4)
            } else {
                (false, 0)
            }
        } else {
            (false, 0)
        };
        if is_sc {
            if let Some(s) = start {
                if i > s {
                    out.push(&data[s..i]);
                }
            }
            i += sc_len;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        if s < data.len() {
            out.push(&data[s..]);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Chunks produced by the VT output callback (Annex-B, ready for the wire).
struct EncShared {
    chunks: Mutex<VecDeque<Vec<u8>>>,
}

/// A live VTCompressionSession (H.264, realtime, no B-frames).
pub struct VtEncoder {
    session: *mut c_void,
    shared: Arc<EncShared>,
    width: u32,
    height: u32,
    frame_idx: i64,
    fps: i32,
}

unsafe impl Send for VtEncoder {}

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
                        desc, idx, &mut ptr, &mut size, &mut count, &mut nal_hdr,
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
        let mut off = 0usize;
        while off + 4 <= bytes.len() {
            let len = u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
                as usize;
            off += 4;
            if len == 0 || off + len > bytes.len() {
                break;
            }
            chunk.extend_from_slice(&[0, 0, 0, 1]);
            chunk.extend_from_slice(&bytes[off..off + len]);
            off += len;
        }
        if !chunk.is_empty() {
            if let Ok(mut q) = shared.chunks.lock() {
                q.push_back(chunk);
            }
        }
    }
}

impl VtEncoder {
    /// Open a realtime H.264 VTCompressionSession. `None` if VideoToolbox is
    /// unavailable or the session can't be created (caller keeps the stub).
    pub fn open(width: u32, height: u32, bitrate_kbps: u32) -> Option<VtEncoder> {
        let lib = VtLib::get()?;
        let shared = Arc::new(EncShared { chunks: Mutex::new(VecDeque::new()) });
        unsafe {
            let mut session: *mut c_void = core::ptr::null_mut();
            let refcon = Arc::as_ptr(&shared) as *mut c_void;
            let st = (lib.VTCompressionSessionCreate)(
                core::ptr::null(),
                width as i32,
                height as i32,
                CODEC_H264,
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null(),
                enc_output,
                refcon,
                &mut session,
            );
            if st != 0 || session.is_null() {
                crate::plog_warn!("[video] VTCompressionSessionCreate failed: {}", st);
                return None;
            }
            // Realtime + no B-frames (low latency for azul-meet) + bitrate.
            let _ = (lib.VTSessionSetProperty)(
                session, lib.kVTCompressionPropertyKey_RealTime, lib.kCFBooleanTrue,
            );
            let _ = (lib.VTSessionSetProperty)(
                session, lib.kVTCompressionPropertyKey_AllowFrameReordering, lib.kCFBooleanFalse,
            );
            let _ = (lib.VTSessionSetProperty)(
                session,
                lib.kVTCompressionPropertyKey_ProfileLevel,
                lib.kVTProfileLevel_H264_Main_AutoLevel,
            );
            let bits: i32 = (bitrate_kbps.max(64) as i32).saturating_mul(1000);
            let n = (lib.CFNumberCreate)(
                core::ptr::null(), CF_NUMBER_SINT32, &bits as *const i32 as *const c_void,
            );
            if !n.is_null() {
                let _ = (lib.VTSessionSetProperty)(
                    session, lib.kVTCompressionPropertyKey_AverageBitRate, n,
                );
                (lib.CFRelease)(n);
            }
            let key_interval: i32 = 60;
            let n = (lib.CFNumberCreate)(
                core::ptr::null(), CF_NUMBER_SINT32,
                &key_interval as *const i32 as *const c_void,
            );
            if !n.is_null() {
                let _ = (lib.VTSessionSetProperty)(
                    session, lib.kVTCompressionPropertyKey_MaxKeyFrameInterval, n,
                );
                (lib.CFRelease)(n);
            }
            let _ = (lib.VTCompressionSessionPrepareToEncodeFrames)(session);
            crate::plog_info!(
                "[video] VideoToolbox H.264 encoder open: {}x{} @{}kbps (realtime)",
                width, height, bitrate_kbps
            );
            Some(VtEncoder { session, shared, width, height, frame_idx: 0, fps: 30 })
        }
    }

    /// Encode one RGBA frame → Annex-B chunk(s). Empty while VT buffers.
    pub fn encode(&mut self, rgba: &[u8], force_keyframe: bool) -> Vec<u8> {
        let lib = match VtLib::get() {
            Some(l) => l,
            None => return Vec::new(),
        };
        let (w, h) = (self.width as usize, self.height as usize);
        if rgba.len() < w * h * 4 {
            return Vec::new();
        }
        unsafe {
            // RGBA → BGRA CVPixelBuffer (the Apple encoders take 32BGRA
            // directly; VT converts to 4:2:0 internally).
            let mut pb: *mut c_void = core::ptr::null_mut();
            if (lib.CVPixelBufferCreate)(core::ptr::null(), w, h, PIXFMT_BGRA,
                core::ptr::null(), &mut pb) != 0 || pb.is_null()
            {
                return Vec::new();
            }
            (lib.CVPixelBufferLockBaseAddress)(pb, 0);
            let base = (lib.CVPixelBufferGetBaseAddress)(pb);
            let stride = (lib.CVPixelBufferGetBytesPerRow)(pb);
            if base.is_null() || stride < w * 4 {
                (lib.CVPixelBufferUnlockBaseAddress)(pb, 0);
                (lib.CFRelease)(pb);
                return Vec::new();
            }
            for y in 0..h {
                let src = &rgba[y * w * 4..(y + 1) * w * 4];
                let dst = std::slice::from_raw_parts_mut(base.add(y * stride), w * 4);
                for x in 0..w {
                    dst[x * 4] = src[x * 4 + 2]; // B
                    dst[x * 4 + 1] = src[x * 4 + 1]; // G
                    dst[x * 4 + 2] = src[x * 4]; // R
                    dst[x * 4 + 3] = 255;
                }
            }
            (lib.CVPixelBufferUnlockBaseAddress)(pb, 0);

            let pts = CMTime::new(self.frame_idx, self.fps);
            self.frame_idx += 1;
            let mut props: CFDictionaryRef = core::ptr::null();
            let mut props_owned: *mut c_void = core::ptr::null_mut();
            if force_keyframe {
                props_owned = (lib.CFDictionaryCreateMutable)(
                    core::ptr::null(), 1,
                    lib.kCFTypeDictionaryKeyCallBacks,
                    lib.kCFTypeDictionaryValueCallBacks,
                );
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
                self.session, pb, pts, CMTime::invalid(), props,
                core::ptr::null_mut(), core::ptr::null_mut(),
            );
            if !props_owned.is_null() {
                (lib.CFRelease)(props_owned);
            }
            (lib.CFRelease)(pb);
            if st != 0 {
                crate::plog_warn!("[video] VTCompressionSessionEncodeFrame failed: {}", st);
                return Vec::new();
            }
            // Realtime session, no reordering: force emission of this frame so
            // encode() behaves synchronously for the caller.
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

/// Frames produced by the VT decode callback (BGRA → RGBA already done).
struct DecShared {
    frames: Mutex<VecDeque<VideoFrame>>,
}

/// A live VTDecompressionSession fed Annex-B H.264.
pub struct VtDecoder {
    session: *mut c_void,
    format_desc: *mut c_void,
    shared: Arc<DecShared>,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    frame_idx: i64,
}

unsafe impl Send for VtDecoder {}

/// VTDecompressionOutputCallback: BGRA CVPixelBuffer → RGBA `VideoFrame`.
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
        let w = (lib.CVPixelBufferGetWidth)(image);
        let h = (lib.CVPixelBufferGetHeight)(image);
        let stride = (lib.CVPixelBufferGetBytesPerRow)(image);
        let base = (lib.CVPixelBufferGetBaseAddress)(image);
        if !base.is_null() && w > 0 && h > 0 && stride >= w * 4 {
            let mut rgba = vec![0u8; w * h * 4];
            for y in 0..h {
                let row = std::slice::from_raw_parts(base.add(y * stride), w * 4);
                let dst = &mut rgba[y * w * 4..(y + 1) * w * 4];
                for x in 0..w {
                    dst[x * 4] = row[x * 4 + 2]; // R
                    dst[x * 4 + 1] = row[x * 4 + 1]; // G
                    dst[x * 4 + 2] = row[x * 4]; // B
                    dst[x * 4 + 3] = 255;
                }
            }
            if let Ok(mut q) = shared.frames.lock() {
                q.push_back(VideoFrame {
                    width: w as u32,
                    height: h as u32,
                    bytes: U8Vec::from_vec(rgba),
                });
            }
        }
        (lib.CVPixelBufferUnlockBaseAddress)(image, 1);
    }
}

impl VtDecoder {
    /// `None` if VideoToolbox is unavailable (caller keeps the stub). The
    /// session itself is created lazily once SPS+PPS arrive in the stream.
    pub fn open_h264() -> Option<VtDecoder> {
        VtLib::get()?;
        crate::plog_info!("[video] VideoToolbox H.264 decoder open (session on first SPS/PPS)");
        Some(VtDecoder {
            session: core::ptr::null_mut(),
            format_desc: core::ptr::null_mut(),
            shared: Arc::new(DecShared { frames: Mutex::new(VecDeque::new()) }),
            sps: None,
            pps: None,
            frame_idx: 0,
        })
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
                core::ptr::null(), 2, ptrs.as_ptr(), sizes.as_ptr(), 4, &mut desc,
            );
            if st != 0 || desc.is_null() {
                crate::plog_warn!("[video] H264 format description failed: {}", st);
                return false;
            }
            // Request BGRA output (VT does hardware YUV→RGB for us).
            let attrs = (lib.CFDictionaryCreateMutable)(
                core::ptr::null(), 1,
                lib.kCFTypeDictionaryKeyCallBacks,
                lib.kCFTypeDictionaryValueCallBacks,
            );
            let fmt: i32 = PIXFMT_BGRA as i32;
            let n = (lib.CFNumberCreate)(
                core::ptr::null(), CF_NUMBER_SINT32, &fmt as *const i32 as *const c_void,
            );
            if !attrs.is_null() && !n.is_null() {
                (lib.CFDictionarySetValue)(attrs, lib.kCVPixelBufferPixelFormatTypeKey, n);
            }
            let record = VTDecompressionOutputCallbackRecord {
                callback: dec_output,
                refcon: Arc::as_ptr(&self.shared) as *mut c_void,
            };
            let mut session: *mut c_void = core::ptr::null_mut();
            let st = (lib.VTDecompressionSessionCreate)(
                core::ptr::null(), desc, core::ptr::null(), attrs, &record, &mut session,
            );
            if !n.is_null() {
                (lib.CFRelease)(n);
            }
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
            crate::plog_info!("[video] VideoToolbox decompression session created (BGRA out)");
            true
        }
    }

    /// Feed one Annex-B chunk; decoded frames appear via the callback
    /// (synchronous decode → typically before this returns).
    pub fn decode(&mut self, data: &[u8]) -> Vec<VideoFrame> {
        let lib = match VtLib::get() {
            Some(l) => l,
            None => return Vec::new(),
        };
        // Split NALs; latch parameter sets; batch VCL NALs into one AU.
        let mut au: Vec<u8> = Vec::with_capacity(data.len() + 16);
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
                _ => {
                    au.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                    au.extend_from_slice(nal);
                }
            }
        }
        if au.is_empty() || !self.ensure_session() {
            return self.drain();
        }
        unsafe {
            // Copy the AU into a CMBlockBuffer.
            let mut bb: *mut c_void = core::ptr::null_mut();
            let st = (lib.CMBlockBufferCreateWithMemoryBlock)(
                core::ptr::null(), core::ptr::null_mut(), au.len(),
                core::ptr::null(), // kCFAllocatorDefault → CM allocates
                core::ptr::null(), 0, au.len(), 0, &mut bb,
            );
            if st != 0 || bb.is_null() {
                return self.drain();
            }
            if (lib.CMBlockBufferReplaceDataBytes)(
                au.as_ptr() as *const c_void, bb, 0, au.len(),
            ) != 0
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
                core::ptr::null(), bb, self.format_desc, 1, 1, &timing, 1,
                sizes.as_ptr(), &mut sample,
            );
            if st == 0 && !sample.is_null() {
                // flags = 0 → synchronous decode (callback fires inline).
                let st = (lib.VTDecompressionSessionDecodeFrame)(
                    self.session, sample, 0, core::ptr::null_mut(), core::ptr::null_mut(),
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
    pub fn flush(&mut self) -> Vec<VideoFrame> {
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
            let chunk = enc.encode(&rgba, f == 0);
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
        assert!(decoded >= 20, "decoder produced too few frames ({})", decoded);
    }
}
