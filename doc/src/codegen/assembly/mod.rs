//! Assembly modules for code generation
//!
//! These modules combine the individual blocks to generate complete output files.
//!
//! - `ffi`: Generates the FFI layer (dll_api.rs) for include!() in azul-dll
//! - `api`: Generates the nice Rust API wrapper (api.rs)

pub mod ffi;
pub mod api;
