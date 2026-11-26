//! Unified module: a target-stable home for the `extra::*` feature handles.
//!
//! Where the `desktop` module exists (`az_desktop`) this simply re-exports the
//! real desktop types, so there is zero behaviour change. On `wasm32`, and in
//! the `engine` profile of servers and command-line tools (`az_engine_stubs`:
//! the engine and the API without the desktop crates), the real `desktop`
//! module does not exist, so we provide stub handle types with an IDENTICAL
//! `#[repr(C)]` layout (the C-ABI `transmute`s between these and the `Az*`
//! repr-C structs, so the layout MUST match the desktop type). The stubs
//! compile there and fail gracefully at runtime (no window, no devices).
//!
//! The api.json `external` paths point here (`azul_dll::unified::<mod>::<Type>`)
//! so the generated C-ABI bindings resolve on every target.

pub mod app;
pub mod audio;
pub mod auth_session;
pub mod capability;
/// The desktop's state, polled (the app in front, the pointer, the lock): api.json's `sensor` module.
pub mod desktop_state;
pub mod iroh;
pub mod keyring;
pub mod map;
pub mod network;
pub mod pdf;
pub mod power;
/// The device-state readings (power, battery, network) beside the motion sensors: api.json's `sensor`
/// module.
pub mod sensors;
/// The system's speech (said aloud, heard): api.json's `speech` module.
pub mod speech;
pub mod sqlite;
pub mod video_codec;
pub mod webtransport;
pub mod zip;
