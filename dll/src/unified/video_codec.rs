//! Unified `VideoEncoder` / `VideoDecoder` handles. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::video_codec::*;

/// wasm fallback for the `VideoWidget::dom()` shim: no decode worker on wasm, so
/// render the placeholder DOM directly (the desktop `video_widget_dom`, which
/// wires the streaming worker, is glob-re-exported above on non-wasm targets).
#[cfg(target_arch = "wasm32")]
pub fn video_widget_dom(widget: azul_layout::widgets::video::VideoWidget) -> azul_core::dom::Dom {
    widget.dom()
}

/// Always-present `pipeline` surface for the C-ABI bindings.
///
/// The real batch decoder lives in `desktop::extra::video_codec::pipeline`
/// behind the `video-native` feature (Linux/Windows), and api.json exposes
/// `DecodedVideo` / `decode_mp4_h264` through this target-stable `unified` path.
/// Codegen has no per-entry feature gating, so the path MUST resolve in every
/// `cabi_internal` build — but `link-static` (the default) enables `cabi_internal`
/// without `video-native`, and wasm has no desktop module at all. When the real
/// module isn't compiled this repr-C-identical stub stands in and reports "no
/// frames" — the same "handle always present, engine opt-in" convention as the
/// `Db` / `Pdf` handles. Mutually exclusive with the glob re-export above (which
/// supplies the real `pipeline` only under `cabi_internal + !wasm + video-native`).
#[cfg(all(
    feature = "cabi_internal",
    any(target_arch = "wasm32", not(feature = "video-native"))
))]
pub mod pipeline {
    use azul_core::video::VideoFrameVec;
    use azul_css::{impl_option, impl_option_inner};

    /// A decoded clip: stream geometry plus the decoded frames. Layout MUST
    /// match `desktop::extra::video_codec::pipeline::DecodedVideo` (the C-ABI
    /// bindings `transmute` between this and `AzDecodedVideo`).
    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct DecodedVideo {
        pub width: u32,
        pub height: u32,
        pub fps: f32,
        pub frames: VideoFrameVec,
        pub access_units_fed: usize,
    }

    impl_option!(
        DecodedVideo,
        OptionDecodedVideo,
        copy = false,
        [Clone, Debug]
    );

    /// Result of `decode_mp4_h264`. Layout MUST match
    /// `desktop::extra::video_codec::pipeline::VideoDecodeResult`.
    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct VideoDecodeResult {
        pub video: OptionDecodedVideo,
    }

    impl_option!(
        VideoDecodeResult,
        OptionVideoDecodeResult,
        copy = false,
        [Clone, Debug]
    );

    impl VideoDecodeResult {
        pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionVideoDecodeResult {
            result.downcast_ref::<Self>().map(|r| r.clone()).into()
        }
    }

    /// No video-decode backend in this build: resumes with `video: None`.
    ///
    /// Says so once on first use — a permanent `None` is otherwise
    /// indistinguishable from "the clip just produced no frames".
    pub fn decode_mp4_h264(
        bytes: azul_css::U8Vec,
        data: azul_core::refany::RefAny,
        on_result: azul_layout::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let _ = bytes;
        let video = decode_mp4_h264_stub();
        azul_layout::request::complete(data, on_result, VideoDecodeResult { video })
    }

    fn decode_mp4_h264_stub() -> OptionDecodedVideo {
        static ANNOUNCE: std::sync::Once = std::sync::Once::new();
        ANNOUNCE.call_once(|| {
            if cfg!(target_arch = "wasm32") {
                eprintln!(
                    "[azul][video] decode_mp4_h264: H.264 decode has no wasm backend — this build \
                     can NEVER return frames (always None)"
                );
            } else {
                eprintln!(
                    "[azul][video] decode_mp4_h264: this build has no `video-native` feature — \
                     H.264 decode is compiled out and can NEVER return frames (always None). \
                     Rebuild with: cargo build -p azul-dll --features build-dll,video-native"
                );
            }
        });
        OptionDecodedVideo::None
    }
}

#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::video::{OptionVideoFrame, VideoFrame};
#[cfg(target_arch = "wasm32")]
use azul_css::{impl_option_inner, AzString, U8Vec};

/// wasm stub of the desktop `VideoEncoder` handle (no codec backend on wasm).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct VideoEncoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for VideoEncoder {
    fn clone(&self) -> Self {
        VideoEncoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Default for VideoEncoder {
    fn default() -> Self {
        VideoEncoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for VideoEncoder {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl VideoEncoder {
    /// No codec backend on wasm: always returns an invalid handle.
    pub fn open(_width: u32, _height: u32, _h265: bool, _bitrate_kbps: u32) -> VideoEncoder {
        VideoEncoder::default()
    }
    pub fn backend_name() -> AzString {
        AzString::from_const_str("none")
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn encode(&self, _frame: VideoFrame, _force_keyframe: bool) -> bool {
        false
    }
    pub fn encode_at(&self, _frame: VideoFrame, _timestamp_us: u64, _force_keyframe: bool) -> bool {
        false
    }
    pub fn recv_packet(&mut self) -> azul_css::corety::OptionU8Vec {
        azul_css::corety::OptionU8Vec::None
    }
    pub fn flush(&self) {}
    pub fn is_hardware(&self) -> bool {
        false
    }
    pub fn frames_encoded(&self) -> u64 {
        0
    }
    pub fn close(&mut self) {}
}

/// wasm stub of the desktop `ScreenRecorder` (no subprocess/gstreamer on wasm).
/// `#[repr(C)]` layout MUST match `video_codec::ScreenRecorder`.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct ScreenRecorder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}
#[cfg(target_arch = "wasm32")]
impl Clone for ScreenRecorder {
    fn clone(&self) -> Self {
        ScreenRecorder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Default for ScreenRecorder {
    fn default() -> Self {
        ScreenRecorder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for ScreenRecorder {
    fn drop(&mut self) {}
}
#[cfg(target_arch = "wasm32")]
impl ScreenRecorder {
    /// No gstreamer on wasm: always returns an invalid handle.
    pub fn start(_path: AzString, _width: u32, _height: u32, _fps: u32) -> ScreenRecorder {
        ScreenRecorder::default()
    }
    pub fn is_recording(&self) -> bool {
        false
    }
    pub fn write_frame(&self, _frame: VideoFrame) -> bool {
        false
    }
    pub fn frames_written(&self) -> u64 {
        0
    }
    pub fn finish(
        &mut self,
        data: azul_core::refany::RefAny,
        on_result: azul_layout::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        azul_layout::request::complete(
            data,
            on_result,
            ScreenRecordingResult {
                ok: false,
                error: azul_css::corety::OptionString::Some(AzString::from_const_str(
                    "no screen recorder on wasm",
                )),
            },
        )
    }
}

/// wasm stub of `ScreenRecordingResult`; layout MUST match the desktop type.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRecordingResult {
    pub ok: bool,
    pub error: azul_css::corety::OptionString,
}

#[cfg(target_arch = "wasm32")]
azul_css::impl_option!(
    ScreenRecordingResult,
    OptionScreenRecordingResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

#[cfg(target_arch = "wasm32")]
impl ScreenRecordingResult {
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionScreenRecordingResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// wasm stub of the desktop `VideoDecoder` handle (no codec backend on wasm).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct VideoDecoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for VideoDecoder {
    fn clone(&self) -> Self {
        VideoDecoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Default for VideoDecoder {
    fn default() -> Self {
        VideoDecoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for VideoDecoder {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl VideoDecoder {
    /// No codec backend on wasm: always returns an invalid handle.
    pub fn open(_h265: bool) -> VideoDecoder {
        VideoDecoder::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn decode(&self, _data: U8Vec) -> bool {
        false
    }
    pub fn set_output_format(&self, _format: azul_core::resources::RawImageFormat) {}
    pub fn set_output_size(&self, _width: u32, _height: u32) {}
    pub fn recv_frame(&mut self) -> OptionVideoFrame {
        OptionVideoFrame::None
    }
    pub fn close(&mut self) {}
}

/// wasm stubs of the desktop `provision` startup-check types (no GPU driver /
/// kernel provisioning on wasm). `#[repr(C)]` layout MUST match the desktop
/// `video_codec::provision::{VideoStartupCheck, VideoProvisionOutcome}` because
/// the C-ABI bindings `transmute` between these and the `Az*` structs.
#[cfg(target_arch = "wasm32")]
pub mod provision {
    use azul_css::AzString;

    // The `#[cfg(target_arch = "wasm32")]` on each item (redundant with the
    // module cfg above) is what the autofix type-indexer's `is_wasm32_only`
    // check looks for — it inspects per-item attrs, so without this it would
    // index these stubs as real types and clobber the canonical desktop
    // `video_codec::provision::*` definitions (which DO have the From impl).
    #[cfg(target_arch = "wasm32")]
    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct VideoProvisionOutcome {
        pub ok: bool,
        pub reboot_required: bool,
        pub message: AzString,
    }

    #[cfg(target_arch = "wasm32")]
    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct VideoStartupCheck {
        pub hw_decode_ready: bool,
        pub boot_safe: bool,
        pub can_remediate: bool,
        pub needs_reboot: bool,
        pub summary: AzString,
        pub detail: AzString,
    }

    impl VideoStartupCheck {
        /// No hardware-decode provisioning on wasm: reports "unavailable", nothing to do.
        pub fn run() -> VideoStartupCheck {
            VideoStartupCheck {
                hw_decode_ready: false,
                boot_safe: true,
                can_remediate: false,
                needs_reboot: false,
                summary: AzString::from_const_str("Hardware video decode is unavailable on wasm."),
                detail: AzString::from_const_str("video provisioning has no wasm backend"),
            }
        }
        pub fn remediate() -> VideoProvisionOutcome {
            VideoProvisionOutcome {
                ok: false,
                reboot_required: false,
                message: AzString::from_const_str("video provisioning has no wasm backend"),
            }
        }
    }

    /// wasm stub of the desktop `VideoEncodeCheck`. `#[repr(C)]` layout MUST match
    /// `video_codec::provision::VideoEncodeCheck` (the C-ABI bindings `transmute`).
    #[cfg(target_arch = "wasm32")]
    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct VideoEncodeCheck {
        pub hw_encode_ready: bool,
        pub software_fallback: bool,
        pub backend: AzString,
        pub summary: AzString,
        pub detail: AzString,
    }

    #[cfg(target_arch = "wasm32")]
    impl VideoEncodeCheck {
        /// No codec backend on wasm: reports "no encoder available".
        pub fn run() -> VideoEncodeCheck {
            VideoEncodeCheck {
                hw_encode_ready: false,
                software_fallback: false,
                backend: AzString::from_const_str("none"),
                summary: AzString::from_const_str("Hardware video encode is unavailable on wasm."),
                detail: AzString::from_const_str("video encode has no wasm backend"),
            }
        }
    }
}

// ==== MP4 container handles (wasm stubs) ====
//
// The desktop `video_codec::container::{Mp4Demuxer, Mp4Muxer}` over the `mp4`
// crate; on wasm there is no container engine, so the demuxer never opens
// (and says why) and the muxer does not open. `#[repr(C)]` layout MUST match
// the desktop handles (the C-ABI bindings `transmute`).

/// wasm stub of the desktop `Mp4Demuxer` handle.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct Mp4Demuxer {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for Mp4Demuxer {
    fn clone(&self) -> Self {
        Mp4Demuxer {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Default for Mp4Demuxer {
    fn default() -> Self {
        Mp4Demuxer {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for Mp4Demuxer {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl Mp4Demuxer {
    /// No container engine on wasm: always a handle that did not open.
    pub fn create(_bytes: U8Vec) -> Mp4Demuxer {
        Mp4Demuxer::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn error(&self) -> AzString {
        AzString::from_const_str("MP4 demux has no wasm backend")
    }
    pub fn width(&self) -> u32 {
        0
    }
    pub fn height(&self) -> u32 {
        0
    }
    pub fn fps(&self) -> f32 {
        0.0
    }
    pub fn duration_ms(&self) -> f64 {
        0.0
    }
    pub fn chunk_count(&self) -> usize {
        0
    }
    pub fn chunk(&self, _index: usize) -> azul_core::video::OptionVideoChunk {
        azul_core::video::OptionVideoChunk::None
    }
    pub fn frame_at(&self, _time_ms: f64) -> usize {
        0
    }
    pub fn keyframe_before(&self, _index: usize) -> usize {
        0
    }
    pub fn close(&mut self) {}
}

/// wasm stub of the desktop `Mp4Muxer` handle.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct Mp4Muxer {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for Mp4Muxer {
    fn clone(&self) -> Self {
        Mp4Muxer {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Default for Mp4Muxer {
    fn default() -> Self {
        Mp4Muxer {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for Mp4Muxer {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl Mp4Muxer {
    /// No container engine on wasm: always an invalid handle.
    pub fn create(_width: u32, _height: u32, _fps: f32) -> Mp4Muxer {
        Mp4Muxer::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn write_annexb(&mut self, _data: U8Vec) -> bool {
        false
    }
    pub fn samples_written(&self) -> u64 {
        0
    }
    pub fn finish(&mut self) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    pub fn error(&self) -> AzString {
        AzString::from_const_str("MP4 mux has no wasm backend")
    }
    pub fn close(&mut self) {}
}
