//! Unified module: a target-stable home for the `extra::*` feature handles.
//!
//! Off-wasm (and where the `desktop` module exists) this simply re-exports the
//! real desktop types, so there is zero behaviour change. On `wasm32` the real
//! `desktop` module is `#[cfg(not(target_arch = "wasm32"))]` and does not
//! exist, so we provide stub handle types with an IDENTICAL `#[repr(C)]` layout
//! (the C-ABI `transmute`s between these and the `Az*` repr-C structs, so the
//! layout MUST match the desktop type). The stubs compile on wasm and fail
//! gracefully at runtime (these features have no wasm backend).
//!
//! The api.json `external` paths point here (`azul_dll::unified::<mod>::<Type>`)
//! so the generated C-ABI bindings resolve on every target.

pub mod app;
pub mod audio;
pub mod capability;
pub mod pdf;
pub mod sqlite;
pub mod udp;
pub mod video_codec;
