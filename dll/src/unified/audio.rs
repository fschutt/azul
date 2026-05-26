//! Unified `AudioSink` handle. See [`crate::unified`].

// Off-wasm: re-export the real desktop type (zero behaviour change). Gated on
// the same condition as `crate::desktop`.
#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::audio::*;

// wasm: stub with an identical `#[repr(C)]` layout (ptr + run_destructor) so
// the C-ABI transmute to `AzAudioSink` stays valid. Defined directly in this
// module so the path resolves to `azul_dll::unified::audio::AudioSink`.
// Includes a `Drop` impl to match the real desktop type's `custom_impl(Drop)`.
#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::audio::{AudioConfig, AudioFrame};

/// wasm stub of the desktop `AudioSink` handle (no audio backend on wasm).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct AudioSink {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for AudioSink {
    fn clone(&self) -> Self {
        AudioSink {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for AudioSink {
    fn default() -> Self {
        AudioSink {
            ptr: core::ptr::null_mut(),
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
    /// No audio backend on wasm: always returns an invalid handle.
    pub fn open(_config: AudioConfig) -> AudioSink {
        AudioSink::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn play(&self, _frame: AudioFrame) {}
    pub fn frames_played(&self) -> u64 {
        0
    }
    pub fn close(&mut self) {}
}
