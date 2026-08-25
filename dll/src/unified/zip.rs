//! Unified `Zip` handle. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::zip::*;

#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_css::{AzString, U8Vec};

/// wasm stub of the desktop `Zip` handle (no compressor on wasm).
/// Identical `#[repr(C)]` layout to the real type — the C-ABI transmutes
/// between the two.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug)]
pub struct Zip {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for Zip {
    fn clone(&self) -> Self {
        Zip {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Zip {
    fn default() -> Self {
        Zip {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Zip {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl Zip {
    /// No ZIP backend on wasm: always an invalid handle.
    pub fn new() -> Zip {
        Zip::default()
    }
    /// No ZIP backend on wasm: always an invalid handle.
    pub fn from_bytes(_bytes: U8Vec) -> Zip {
        Zip::default()
    }
    /// No ZIP backend on wasm (and no filesystem): always an invalid handle.
    pub fn from_file(_path: AzString) -> Zip {
        Zip::default()
    }
    pub fn is_valid(&self) -> bool {
        false
    }
    pub fn add_file(&mut self, _path: AzString, _data: U8Vec) {}
    pub fn add_directory(&mut self, _path: AzString) {}
    pub fn remove(&mut self, _path: AzString) {}
    pub fn contains(&self, _path: AzString) -> bool {
        false
    }
    pub fn file_count(&self) -> usize {
        0
    }
    pub fn file_path(&self, _index: usize) -> AzString {
        AzString::from_const_str("")
    }
    pub fn file_data(&self, _index: usize) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    pub fn file_is_directory(&self, _index: usize) -> bool {
        false
    }
    pub fn get_file(&self, _path: AzString) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    pub fn to_bytes(&self) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    pub fn to_bytes_with_level(&self, _level: u8) -> U8Vec {
        U8Vec::from_vec(Vec::new())
    }
    pub fn to_file(&self, _path: AzString) -> bool {
        false
    }
}
