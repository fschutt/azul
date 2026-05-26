//! Unified `Db` handle. See [`crate::unified`].

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::sqlite::*;

#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::db::{DbRows, DbValueVec};
#[cfg(target_arch = "wasm32")]
use azul_css::{AzString, StringVec};

/// wasm stub of the desktop `Db` handle (no sqlite backend on wasm).
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug)]
pub struct Db {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for Db {
    fn clone(&self) -> Self {
        Db {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Db {
    fn default() -> Self {
        Db {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Db {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
impl Db {
    /// No sqlite backend on wasm: always returns an invalid handle.
    pub fn open(_path: AzString) -> Db {
        Db::default()
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn execute(&self, _sql: AzString, _params: DbValueVec) -> usize {
        0
    }
    pub fn query(&self, _sql: AzString, _params: DbValueVec) -> DbRows {
        DbRows {
            columns: StringVec::from_vec(Vec::new()),
            values: DbValueVec::from_vec(Vec::new()),
        }
    }
}
