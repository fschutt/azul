//! Video encode/decode (SUPER_PLAN_2 P7/P8) - `VideoEncoder` / `VideoDecoder`.
//!
//! For azul-meet's video leg: compress captured `VideoFrame`s before
//! `Udp::send_chunked`, and decompress received bitstreams back into frames for
//! a display widget. Like `AudioSink` / `Db`, these are C-ABI handles the app
//! holds in its own State (no globals).
//!
//! **Native-per-platform backend** (per the user directive + the vk-video
//! research): the codec used is whatever is native to the platform -
//!   - desktop Linux / Windows: **gpu-video** (Vulkan Video, H.264/H.265),
//!   - Apple (macOS / iOS): **VideoToolbox** (Vulkan Video can't build on
//!     Apple - no MoltenVK video),
//!   - Android: **MediaCodec**,
//!   - anything else: none (encode/decode no-op).
//! [`VideoEncoder::backend_name`] reports the selection. The codec FFI itself
//! is on-device per platform (like the camera/mic capture backends); this lands
//! the cross-platform API + the backend selection + a stub engine, so the API
//! is exercisable + cross-compiles everywhere, with the real codec swapped in
//! per OS.

use core::ffi::c_void;

use azul_core::video::{OptionVideoFrame, VideoFrame};
use azul_css::{AzString, U8Vec};

// MP4 -> H.264 Annex-B demux (the elementary stream gpu-video needs). Behind
// `video-native`; pure Rust + unit-tested, no GPU required.
#[cfg(feature = "video-native")]
pub mod demux;

// File -> frames pipeline (demux + feed through VideoDecoder). Behind
// `video-native`; the decode step is the only hardware-gated part.
#[cfg(feature = "video-native")]
pub mod pipeline;

// Hardware-decode capability probe + driver-provisioning planner (always built;
// no extra crate deps). Drives `capability::video_codec()` and the "install the
// drivers for me?" flow.
pub mod provision;

/// The native codec backend this build selects, by target OS.
fn backend() -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "macos")) {
        "VideoToolbox"
    } else if cfg!(target_os = "android") {
        "MediaCodec"
    } else if cfg!(any(target_os = "linux", target_os = "windows")) {
        "gpu-video"
    } else {
        "none"
    }
}

/// Engine-side encoder state. The stub records the params; the real backend
/// (VideoToolbox / MediaCodec / gpu-video session) replaces this per platform.
struct EncoderInner {
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,
    #[allow(dead_code)]
    h265: bool,
    #[allow(dead_code)]
    bitrate_kbps: u32,
    frames_encoded: u64,
}

struct DecoderInner {
    #[allow(dead_code)]
    h265: bool,
    frames_decoded: u64,
}

/// A hardware video encoder handle. `open(...)` selects the native backend for
/// the platform; `encode` turns a `VideoFrame` (RGBA) into an encoded chunk.
#[repr(C)]
pub struct VideoEncoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for VideoEncoder {
    fn clone(&self) -> Self {
        VideoEncoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
impl Default for VideoEncoder {
    fn default() -> Self {
        VideoEncoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl VideoEncoder {
    /// Open an encoder for `width` x `height`, H.265 if `h265` else H.264, at
    /// `bitrate_kbps`. Uses the platform-native backend ([`backend_name`]).
    /// Returns an invalid handle (`is_open()` false) where no backend exists.
    pub fn open(width: u32, height: u32, h265: bool, bitrate_kbps: u32) -> VideoEncoder {
        if backend() == "none" {
            return VideoEncoder::default();
        }
        let inner = Box::new(EncoderInner {
            width,
            height,
            h265,
            bitrate_kbps,
            frames_encoded: 0,
        });
        VideoEncoder {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// The native codec backend selected for this platform ("VideoToolbox",
    /// "MediaCodec", "gpu-video", or "none").
    pub fn backend_name() -> AzString {
        AzString::from_const_str(backend())
    }

    /// Whether the encoder opened (a backend exists for this platform).
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Encode one `VideoFrame` (RGBA), returning the encoded chunk (Annex-B for
    /// H.264/H.265), or empty if buffered / not open. `force_keyframe` requests
    /// an IDR. (Stub: counts frames + returns empty; the on-device backend
    /// produces the bitstream.)
    pub fn encode(&self, frame: VideoFrame, force_keyframe: bool) -> U8Vec {
        if let Some(inner) = unsafe { (self.ptr as *mut EncoderInner).as_mut() } {
            inner.frames_encoded = inner.frames_encoded.wrapping_add(1);
            let _ = (frame, force_keyframe);
        }
        U8Vec::from_const_slice(&[])
    }

    /// Frames submitted to [`encode`](Self::encode) so far (stub progress).
    pub fn frames_encoded(&self) -> u64 {
        unsafe { (self.ptr as *const EncoderInner).as_ref() }
            .map(|i| i.frames_encoded)
            .unwrap_or(0)
    }

    /// Release the encoder. (Drop does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut EncoderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

/// A hardware video decoder handle. Feed it encoded chunks with `decode`; it
/// returns decoded `VideoFrame`s as they become available.
#[repr(C)]
pub struct VideoDecoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for VideoDecoder {
    fn clone(&self) -> Self {
        VideoDecoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
impl Default for VideoDecoder {
    fn default() -> Self {
        VideoDecoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl VideoDecoder {
    /// Open a decoder (H.265 if `h265` else H.264) using the platform-native
    /// backend. Invalid handle where no backend exists.
    pub fn open(h265: bool) -> VideoDecoder {
        if backend() == "none" {
            return VideoDecoder::default();
        }
        let inner = Box::new(DecoderInner {
            h265,
            frames_decoded: 0,
        });
        VideoDecoder {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// Whether the decoder opened.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Decode one encoded chunk, returning the next decoded `VideoFrame` if one
    /// is ready (`None` while buffering / not open). (Stub: counts + returns
    /// None; the on-device backend produces frames.)
    pub fn decode(&self, data: U8Vec) -> OptionVideoFrame {
        if let Some(inner) = unsafe { (self.ptr as *mut DecoderInner).as_mut() } {
            inner.frames_decoded = inner.frames_decoded.wrapping_add(1);
            let _ = data;
        }
        OptionVideoFrame::None
    }

    /// Release the decoder. (Drop does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut DecoderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.drop_inner();
    }
}
