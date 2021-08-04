
#[macro_use]
extern crate alloc;
extern crate azul_core;

#[cfg(target_arch = "wasm32")]
extern crate azul_web as azul_impl;
#[cfg(not(target_arch = "wasm32"))]
extern crate azul_desktop as azul_impl;

use core::ffi::c_void;