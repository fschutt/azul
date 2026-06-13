//! Unified `VideoEncoder` / `VideoDecoder` handles. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::video_codec::*;

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

    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct VideoProvisionOutcome {
        pub ok: bool,
        pub reboot_required: bool,
        pub message: AzString,
    }

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
}
