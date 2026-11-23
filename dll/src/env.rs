//! The ONE reader of the process environment in azul-dll (and, with a tool's `main`, in the
//! whole workspace).
//!
//! Every library crate (azul-css, azul-core, azul-layout, webrender) takes a typed settings value
//! built by a pure `from_lookup(&dyn azul_css::env::EnvSource)`. This module is the source that
//! reads the real process: construct a [`ProcessEnv`], build the settings once at the start
//! (`App::create`), and hand the values down. A change of the environment after that is not seen.
//!
//! Nothing else in the dll calls `std::env::var` or `var_os`; the gate
//! `examples/azul-srcgate/tests/env_gate.rs` counts the reads per file and the count of this file
//! is its two.

use azul_css::env::EnvSource;

/// The process environment, read at the moment of the call.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn var_os(&self, name: &str) -> Option<std::ffi::OsString> {
        std::env::var_os(name)
    }
}
