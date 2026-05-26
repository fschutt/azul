//! Unified `Udp` handle. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::udp::*;

#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_css::{AzString, OptionU8Vec, U8Vec};

/// wasm stub of the desktop `Udp` handle (no UDP socket backend on wasm).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
pub struct Udp {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for Udp {
    fn clone(&self) -> Self {
        Udp {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Udp {
    fn default() -> Self {
        Udp {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Udp {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl Udp {
    /// No socket backend on wasm: always returns an invalid handle.
    pub fn bind(_local_addr: AzString) -> Udp {
        Udp::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn send_to(&self, _remote_addr: AzString, _data: U8Vec) -> usize {
        0
    }
    pub fn recv(&self) -> OptionU8Vec {
        OptionU8Vec::None
    }
    pub fn send_chunked(&self, _remote_addr: AzString, _data: U8Vec) -> usize {
        0
    }
    pub fn recv_chunked(&self) -> OptionU8Vec {
        OptionU8Vec::None
    }
    pub fn local_addr(&self) -> AzString {
        AzString::from_const_str("")
    }
    pub fn close(&mut self) {}
}
