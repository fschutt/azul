//! Unified `AudioSink` handle. See [`crate::unified`].

// Off-wasm: re-export the real desktop type (zero behaviour change). Gated on
// the same condition as `crate::desktop`.
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::audio::{AudioConfig, AudioFrame};
// wasm: stub with an identical `#[repr(C)]` layout (ptr + error +
// run_destructor) so the C-ABI transmute to `AzAudioSink` stays valid. Defined
// directly in this module so the path resolves to
// `azul_dll::unified::audio::AudioSink`. Includes a `Drop` impl to match the
// real desktop type's `custom_impl(Drop)`.
#[cfg(target_arch = "wasm32")]
#[cfg(target_arch = "wasm32")]
use azul_css::{AzString, OptionString, StringVec};

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::audio::*;

/// wasm stub of the desktop `AudioSink` handle (no audio backend on wasm).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct AudioSink {
    pub ptr: *mut c_void,
    pub error: OptionString,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for AudioSink {
    fn clone(&self) -> Self {
        AudioSink {
            ptr: self.ptr,
            error: self.error.clone(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for AudioSink {
    fn default() -> Self {
        AudioSink {
            ptr: core::ptr::null_mut(),
            error: OptionString::None,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for AudioSink {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl AudioSink {
    /// No audio backend on wasm: always returns a closed handle that says so.
    pub fn open(_config: AudioConfig) -> AudioSink {
        AudioSink {
            ptr: core::ptr::null_mut(),
            error: OptionString::Some(AzString::from(
                "this platform has no audio output backend in azul yet (wasm)",
            )),
            run_destructor: false,
        }
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn play(&self, _frame: AudioFrame) {}
    pub fn frames_played(&self) -> u64 {
        0
    }
    pub fn error_message(&self) -> OptionString {
        self.error.clone()
    }
    pub fn close(&mut self) {}
}

/// wasm stub of `AudioDeviceListResult`; layout MUST match the desktop type.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AudioDeviceListResult {
    pub devices: AudioDeviceList,
}

#[cfg(target_arch = "wasm32")]
azul_css::impl_option!(
    AudioDeviceListResult,
    OptionAudioDeviceListResult,
    copy = false,
    [Debug, Clone]
);

#[cfg(target_arch = "wasm32")]
impl AudioDeviceListResult {
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionAudioDeviceListResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

/// wasm stub of `AudioDeviceList` (no enumeration backend on wasm). `#[repr(C)]`
/// layout MUST match the desktop `audio::AudioDeviceList`.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AudioDeviceList {
    pub outputs: StringVec,
    pub inputs: StringVec,
}
#[cfg(target_arch = "wasm32")]
impl AudioDeviceList {
    /// No audio enumeration on wasm: resumes with empty lists.
    pub fn enumerate(
        data: azul_core::refany::RefAny,
        on_result: azul_layout::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let devices = Self::enumerate_blocking();
        azul_layout::request::complete(data, on_result, AudioDeviceListResult { devices })
    }
    pub fn enumerate_blocking() -> AudioDeviceList {
        AudioDeviceList {
            outputs: StringVec::from_const_slice(&[]),
            inputs: StringVec::from_const_slice(&[]),
        }
    }
}

/// wasm stubs of the desktop Opus handles (`audio::codec`): no Opus engine on
/// wasm, so every handle is closed. `#[repr(C)]` layout MUST match the desktop
/// `AudioEncoder` / `AudioDecoder` (ptr + run_destructor).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct AudioEncoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for AudioEncoder {
    fn clone(&self) -> Self {
        AudioEncoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for AudioEncoder {
    fn default() -> Self {
        AudioEncoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for AudioEncoder {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl AudioEncoder {
    pub fn create(_config: AudioConfig, _bitrate_kbps: u32) -> AudioEncoder {
        AudioEncoder::default()
    }
    pub fn backend_name() -> AzString {
        AzString::from_const_str("none")
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn encode(&mut self, _frame: AudioFrame) -> bool {
        false
    }
    pub fn recv_packet(&mut self) -> azul_css::corety::OptionU8Vec {
        azul_css::corety::OptionU8Vec::None
    }
    pub fn close(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct AudioDecoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for AudioDecoder {
    fn clone(&self) -> Self {
        AudioDecoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for AudioDecoder {
    fn default() -> Self {
        AudioDecoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for AudioDecoder {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl AudioDecoder {
    pub fn create(_config: AudioConfig) -> AudioDecoder {
        AudioDecoder::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn decode(&mut self, _packet: azul_css::U8Vec) -> azul_core::audio::OptionAudioFrame {
        azul_core::audio::OptionAudioFrame::None
    }
    pub fn close(&mut self) {}
}
