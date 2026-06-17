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

// Streaming decode worker for the VideoWidget: runs the VK decode on a background
// framework Thread (off-main), exactly like the map's tile_fetch_worker. The
// hardware decode inside is video-native-gated; the worker fn is always present.
pub mod stream;
// `video_widget_dom` is the FFI `VideoWidget::dom()` entry point (wires the
// streaming worker), surfaced at the module level so `unified::video_codec`'s
// glob re-export exposes `azul_dll::unified::video_codec::video_widget_dom`.
pub use stream::video_widget_dom;

// File -> frames pipeline (demux + feed through VideoDecoder). Behind
// `video-native`; the decode step is the only hardware-gated part.
#[cfg(feature = "video-native")]
pub mod pipeline;

// Real Vulkan Video H.264 decoder (Linux + Windows). Behind `video-native`; the
// gpu-video wiring + NV12->RGBA CPU conversion live here. Other platforms keep
// the stub (Apple: VideoToolbox / Android: MediaCodec land later).
#[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
mod decode_vulkan;

// Hardware-decode capability probe + driver-provisioning planner (always built;
// no extra crate deps). Drives `capability::video_codec()` and the "install the
// drivers for me?" flow.
pub mod provision;

// The one-call startup readiness check + its outcome — the FFI/DLL surface an
// app uses at launch to verify the box is ready for hardware video decode.
pub use provision::{VideoProvisionOutcome, VideoStartupCheck};

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
    /// Real Vulkan Video decoder, when one could be opened (H.264, Linux/Windows,
    /// `video-native`). `None` => behaves like the stub (no frames produced).
    #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
    backend: Option<decode_vulkan::VulkanVideoDecoder>,
    /// Frames decoded but not yet pulled. Decode is pipelined + B-frame-reordered,
    /// so one fed chunk can yield several frames; we hand them out one per
    /// `decode` / `next_frame` call.
    #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
    pending: std::collections::VecDeque<VideoFrame>,
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

/// Engine-side recorder state: the gstreamer subprocess + its stdin (raw RGBA in).
struct RecorderInner {
    child: Option<std::process::Child>,
    stdin: Option<std::process::ChildStdin>,
    width: u32,
    height: u32,
    frames: u64,
}

/// A **software** screen/frame recorder: feed it RGBA [`VideoFrame`]s and it muxes
/// them into an MP4 via a gstreamer `x264enc` subprocess (`fdsrc ! rawvideoparse !
/// videoconvert ! video/x-raw,format=I420 ! x264enc ! mp4mux ! filesink`). This is
/// the fallback when there's no hardware encode (see [`provision::VideoEncodeCheck`]
/// — true on this GTX 960). A C-ABI handle like [`VideoEncoder`].
#[repr(C)]
pub struct ScreenRecorder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for ScreenRecorder {
    fn clone(&self) -> Self {
        ScreenRecorder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
impl Default for ScreenRecorder {
    fn default() -> Self {
        ScreenRecorder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl ScreenRecorder {
    /// Start recording RGBA frames of `width`x`height` at `fps` to the MP4 at
    /// `path` (software x264). Returns an invalid handle (`is_recording()` false)
    /// if gstreamer (`gst-launch-1.0`) isn't installed.
    pub fn start(path: AzString, width: u32, height: u32, fps: u32) -> ScreenRecorder {
        use std::process::{Command, Stdio};
        let fps = fps.max(1);
        let child = Command::new("gst-launch-1.0")
            .args([
                "-q",
                "fdsrc",
                "fd=0",
                "!",
                "rawvideoparse",
                &format!("width={}", width),
                &format!("height={}", height),
                "format=rgba",
                &format!("framerate={}/1", fps),
                "!",
                "videoconvert",
                "!",
                "video/x-raw,format=I420",
                "!",
                "x264enc",
                "!",
                "mp4mux",
                "!",
                "filesink",
                &format!("location={}", path.as_str()),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(_) => return ScreenRecorder::default(),
        };
        let stdin = child.stdin.take();
        let inner = Box::new(RecorderInner {
            child: Some(child),
            stdin,
            width,
            height,
            frames: 0,
        });
        ScreenRecorder {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// Whether recording is active (the gstreamer subprocess started).
    pub fn is_recording(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Feed one RGBA `VideoFrame` (its `width`x`height` must match `start`). Returns
    /// false if not recording, the frame is too small, or the encoder has exited.
    pub fn write_frame(&self, frame: VideoFrame) -> bool {
        use std::io::Write;
        if let Some(inner) = unsafe { (self.ptr as *mut RecorderInner).as_mut() } {
            let need = (inner.width as usize) * (inner.height as usize) * 4;
            let bytes = frame.bytes.as_ref();
            if bytes.len() < need {
                return false;
            }
            if let Some(si) = inner.stdin.as_mut() {
                if si.write_all(&bytes[..need]).is_ok() {
                    inner.frames = inner.frames.wrapping_add(1);
                    return true;
                }
            }
        }
        false
    }

    /// Frames written so far.
    pub fn frames_written(&self) -> u64 {
        unsafe { (self.ptr as *const RecorderInner).as_ref() }
            .map(|i| i.frames)
            .unwrap_or(0)
    }

    /// Finish: close the encoder's input so it finalizes the MP4, wait for it, and
    /// release the handle. Returns true if gstreamer exited cleanly. (Drop does a
    /// best-effort finalize too, if you don't call this.)
    pub fn finish(&mut self) -> bool {
        let ok = if let Some(inner) = unsafe { (self.ptr as *mut RecorderInner).as_mut() } {
            drop(inner.stdin.take()); // EOF → gst writes the moov atom + exits
            match inner.child.take() {
                Some(mut c) => c.wait().map(|s| s.success()).unwrap_or(false),
                None => false,
            }
        } else {
            false
        };
        self.drop_inner();
        ok
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut RecorderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for ScreenRecorder {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            if let Some(inner) = unsafe { (self.ptr as *mut RecorderInner).as_mut() } {
                drop(inner.stdin.take());
                if let Some(mut c) = inner.child.take() {
                    let _ = c.wait();
                }
            }
        }
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
            #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
            backend: if h265 {
                // H.265 decode isn't wired into the bytes-decoder path yet; the
                // demos are H.264. Leaving this None keeps the stub behaviour.
                None
            } else {
                decode_vulkan::VulkanVideoDecoder::open_h264()
            },
            #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
            pending: std::collections::VecDeque::new(),
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

    /// Decode one encoded chunk (Annex-B H.264), returning the next decoded
    /// `VideoFrame` if one is ready. Extra frames produced by this chunk (decode
    /// is pipelined / reordered) are buffered — pull them with
    /// [`next_frame`](Self::next_frame). Returns `None` while buffering, when not
    /// open, or where no real backend exists (the stub).
    pub fn decode(&self, data: U8Vec) -> OptionVideoFrame {
        let inner = match unsafe { (self.ptr as *mut DecoderInner).as_mut() } {
            Some(i) => i,
            None => return OptionVideoFrame::None,
        };
        inner.frames_decoded = inner.frames_decoded.wrapping_add(1);
        #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
        {
            if let Some(backend) = inner.backend.as_mut() {
                for f in backend.decode(data.as_slice()) {
                    inner.pending.push_back(f);
                }
                if let Some(f) = inner.pending.pop_front() {
                    return OptionVideoFrame::Some(f);
                }
            }
        }
        let _ = data;
        OptionVideoFrame::None
    }

    /// Pull the next already-decoded frame without feeding more input. After a
    /// `decode` / `flush` there may be several frames buffered (pipelining +
    /// B-frame reordering); loop `next_frame` until it returns `None`.
    pub fn next_frame(&self) -> OptionVideoFrame {
        #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
        if let Some(inner) = unsafe { (self.ptr as *mut DecoderInner).as_mut() } {
            if let Some(f) = inner.pending.pop_front() {
                return OptionVideoFrame::Some(f);
            }
        }
        OptionVideoFrame::None
    }

    /// Flush the decoder at end-of-stream, returning the first trailing frame
    /// (drain the rest with [`next_frame`](Self::next_frame)). Frames held back
    /// for B-frame reordering only come out after a flush.
    pub fn flush(&self) -> OptionVideoFrame {
        #[cfg(all(feature = "video-native", target_arch = "x86_64", any(target_os = "linux", target_os = "windows")))]
        if let Some(inner) = unsafe { (self.ptr as *mut DecoderInner).as_mut() } {
            if let Some(backend) = inner.backend.as_mut() {
                for f in backend.flush() {
                    inner.pending.push_back(f);
                }
            }
            if let Some(f) = inner.pending.pop_front() {
                return OptionVideoFrame::Some(f);
            }
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

#[cfg(test)]
mod screenrec_tests {
    use super::{ScreenRecorder, VideoFrame};
    use azul_css::{AzString, U8Vec};

    // End-to-end: record synthetic RGBA frames → a real MP4 via the gst x264 sink.
    // Skips cleanly if gstreamer isn't installed (e.g. minimal CI).
    #[test]
    fn screen_recorder_smoke() {
        let path = "/tmp/azul_screenrec_test.mp4";
        let _ = std::fs::remove_file(path);
        let mut r = ScreenRecorder::start(AzString::from(path), 64, 48, 30);
        if !r.is_recording() {
            eprintln!("gstreamer unavailable — skipping ScreenRecorder smoke test");
            return;
        }
        for f in 0..24u32 {
            let mut buf = vec![0u8; 64 * 48 * 4];
            for px in buf.chunks_exact_mut(4) {
                px[0] = (f * 10) as u8;
                px[1] = 120;
                px[2] = 64;
                px[3] = 255;
            }
            let frame = VideoFrame {
                width: 64,
                height: 48,
                bytes: U8Vec::from_vec(buf),
            };
            assert!(r.write_frame(frame), "write_frame {}", f);
        }
        assert_eq!(r.frames_written(), 24);
        assert!(r.finish(), "finish (gst exit)");
        let sz = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        assert!(sz > 0, "mp4 should be non-empty (got {} bytes)", sz);
        eprintln!("ScreenRecorder smoke: wrote {}-byte mp4", sz);
    }
}
