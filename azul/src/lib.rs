//! Public API for Azul
//!
//! A single function can have multiple implementations depending on whether it is
//! compiled for the Rust-desktop target, the Rust-wasm target or the C API.
//!
//! For now, the crate simply re-exports azul_core and calls the c_api functions

#[path = "./c-api.rs"]
pub mod c_api;

extern crate azul_core;
extern crate azul_css;
extern crate azul_native_style;

#[cfg(target_arch = "wasm32")]
extern crate azul_web;

#[cfg(not(target_arch = "wasm32"))]
extern crate azul_desktop;