//! The doc tool's reader of the process: the ONE place in azul-doc that calls `std::env::var`,
//! `var_os` and `temp_dir`.
//!
//! `main` calls [`read_settings`] once; every command takes the [`ToolSettings`] it returns, so a
//! change of the environment after the start is not seen and no command asks the process itself.
//! (azul-doc does not link azul-dll, so it has its own small reader beside `azul_dll::env`.)
//! The gate `examples/azul-srcgate/tests/env_gate.rs` counts the calls of this file: two reads
//! and the one temp-dir lookup.

use azul_css::env::EnvSource;

use crate::settings::ToolSettings;

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

/// The settings of this process, read once: the variables through [`ProcessEnv`], and the scratch
/// directory the platform names (`TMPDIR` on Unix, `TMP`/`TEMP` and the profile on Windows).
#[must_use]
pub fn read_settings() -> ToolSettings {
    ToolSettings {
        temp_dir: std::env::temp_dir(),
        ..ToolSettings::from_lookup(&ProcessEnv)
    }
}
