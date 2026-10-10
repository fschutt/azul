//! Moved: the unified `PowerState` lives in [`crate::unified::sensors`] now, with the other
//! device-state readings (api.json's `sensor` module). This path re-exports it for one release,
//! so a build whose api.json still names `azul_dll::unified::power::PowerState` keeps working;
//! it goes once the external paths say `azul_dll::unified::sensors::PowerState`.

#[cfg(any(target_arch = "wasm32", feature = "cabi_internal"))]
pub use super::sensors::PowerState;
