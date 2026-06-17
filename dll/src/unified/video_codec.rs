//! Unified `VideoEncoder` / `VideoDecoder` handles. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::video_codec::*;

/// wasm fallback for the `VideoWidget::dom()` shim: no decode worker on wasm, so
/// render the placeholder DOM directly (the desktop `video_widget_dom`, which
/// wires the streaming worker, is glob-re-exported above on non-wasm targets).
#[cfg(target_arch = "wasm32")]
pub fn video_widget_dom(
    widget: azul_layout::widgets::video::VideoWidget,
) -> azul_core::dom::Dom {
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

    impl_option!(DecodedVideo, OptionDecodedVideo, copy = false, [Clone, Debug]);

    /// No video-decode backend in this build: always returns `None`.
    pub fn decode_mp4_h264(_bytes: &[u8]) -> OptionDecodedVideo {
        OptionDecodedVideo::None
    }
}

#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::video::{OptionVideoFrame, VideoFrame};
#[cfg(target_arch = "wasm32")]
use azul_css::{AzString, U8Vec};

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
    pub fn encode(&self, _frame: VideoFrame, _force_keyframe: bool) -> U8Vec {
        U8Vec::from_vec(Vec::new())
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
    pub fn finish(&mut self) -> bool {
        false
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
    pub fn decode(&self, _data: U8Vec) -> OptionVideoFrame {
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
