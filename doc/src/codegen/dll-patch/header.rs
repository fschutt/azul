
#[macro_use]
extern crate alloc;
extern crate azul_core;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
pub mod extra;
pub mod str;

pub mod azul_impl {
    #[cfg(not(target_arch = "wasm32"))]
    pub use super::desktop::*;
}

use core::ffi::c_void;