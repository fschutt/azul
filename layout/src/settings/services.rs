//! The settings of the services of this crate (the file picker, the profiler's symbol lookup, the
//! platform folders) that come from the environment, and the one place of the crate that names
//! their variables.
//!
//! Nothing here reads the process: [`ServiceSettings::from_lookup`] takes a `&dyn EnvSource`
//! (`azul_css::env`), the one process reader is `azul_dll::env::ProcessEnv`, and `Default` is the
//! behaviour of an EMPTY environment (no picker named, symbols recovered with `addr2line`, no forced
//! rollout cohort).
//!
//! What a service needs from a place that carries no context (a `FileDialog` function is a plain
//! static function, the profiler is process-wide) it gets once from [`ServiceSettings::install`],
//! called by the app start. The rest travels in the value: [`ServiceSettings::platform`] is handed
//! to the code that asks for the user's folders.

use azul_css::env::EnvSource;

use super::platform::PlatformEnv;

/// The environment variable that picks the file picker: `native`, or the path of a registration
/// file (see `desktop::file_picker`).
pub const PICKER_VAR: &str = "AZUL_FILE_PICKER";

/// What the environment asks of the services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSettings {
    /// The platform variables (the user's folders, the windowing system, the snap marker).
    pub platform: PlatformEnv,
    /// `AZUL_FILE_PICKER`: `native` never asks a provider, a path names the registration file of
    /// the provider to ask (also in an e2e run); `None` leaves it to the registration in the config
    /// folder.
    pub file_picker: Option<String>,
    /// `AZ_PROBE_ADDR2LINE` is not `0`: the profiler may ask the system's `addr2line` for the name
    /// of a callback it only knows by its address. On by default.
    pub probe_addr2line: bool,
    /// `AZ_UPDATE_BUCKET=<0-99>`: the rollout cohort the updater uses instead of drawing one, for
    /// drills and tests; `None` when it is unset or no number from 0 to 99.
    pub update_bucket: Option<u8>,
}

impl Default for ServiceSettings {
    /// The empty environment's: no picker named, `addr2line` allowed.
    fn default() -> Self {
        Self {
            platform: PlatformEnv::default(),
            file_picker: None,
            probe_addr2line: true,
            update_bucket: None,
        }
    }
}

impl ServiceSettings {
    /// What the environment `env` asks for; an empty environment asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            platform: PlatformEnv::from_lookup(env),
            file_picker: env.var(PICKER_VAR),
            probe_addr2line: env.var("AZ_PROBE_ADDR2LINE").is_none_or(|v| v != "0"),
            update_bucket: env
                .var("AZ_UPDATE_BUCKET")
                .and_then(|v| v.parse::<u8>().ok())
                .filter(|b| *b < 100),
        }
    }

    /// Puts what the context-free code of the services needs where it finds it: the request
    /// runtime learns which file picker to ask (`request::set_file_picker`, with the `text_layout`
    /// feature) and the profiler learns whether it may call `addr2line` (`Probe::set_addr2line`).
    ///
    /// Call it once, at the start.
    pub fn install(&self) {
        #[cfg(feature = "text_layout")]
        crate::request::set_file_picker(self.file_picker.as_deref());
        crate::probe::Probe::set_addr2line(self.probe_addr2line);
    }
}

#[cfg(test)]
#[path = "services_test.rs"]
mod tests;
