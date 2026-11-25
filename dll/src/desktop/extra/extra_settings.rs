//! The switches of the platform services (`desktop::extra`), typed.
//!
//! Most of the extras are entry points a host calls with no window and no `App` in reach (a
//! public C-ABI function such as `Pdf::from_dom` or the capture backend's `open`), so the app's
//! settings record cannot be handed to them. They read their few switches here, once per use:
//! [`ExtraSettings::of_process`] reads the process through the one reader at the entry of the
//! service (a PDF export, a capture session), and everything below the entry takes the value as
//! a plain field. This file is the one place of the extras that knows which variable means what.
//!
//! The headless stand-ins of the sensors and of speech (the files a test switches the battery,
//! the network and the spoken text with) are the installed `azul_core::run_mode::TestHooks`'; the
//! names of their variables are kept here for the public constants of the sensors and speech
//! modules.
//!
//! `Default` is the empty environment (`from_lookup(&NoEnv)`).

use azul_css::env::{EnvSource, NoEnv};

/// The variable naming the file a headless run switches its battery with
/// (`BatteryState::from_words`); the installed `TestHooks` hold its value.
pub const BATTERY_STATE_FILE_VAR: &str = "AZ_BATTERY_STATE_FILE";

/// The variable naming the file a headless run switches its network with
/// (`NetworkState::from_words`); the installed `TestHooks` hold its value.
pub const NETWORK_STATE_FILE_VAR: &str = "AZ_NETWORK_STATE_FILE";

/// The variable naming the file a headless run appends its utterances to.
pub const SPEECH_LOG_VAR: &str = "AZ_SPEECH_LOG";

/// The variable holding what a headless run hears.
pub const SPEECH_HEARD_VAR: &str = "AZ_SPEECH_HEARD";

/// How long the portal's share dialog may stay open before a screen capture gives up: five
/// minutes.
pub const SCREENCAP_START_TIMEOUT_SECS: u64 = 300;

/// What the platform services read from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraSettings {
    /// `AZ_PAGINATION_ENGINE=tokens`: the PDF export paginates with the token engine instead of
    /// the slicer (every other value, and unset, is the slicer).
    pub pagination_tokens: bool,
    /// `AZ_SCREENCAP_DEBUG` is set: trace the portal and PipeWire handshake of a screen capture
    /// to stderr, independent of the `logging` feature.
    pub screencap_debug: bool,
    /// `AZ_SCREENCAP_START_TIMEOUT_SECS`: how long the portal's share dialog may stay open
    /// ([`SCREENCAP_START_TIMEOUT_SECS`] when unset or not a number).
    pub screencap_start_timeout_secs: u64,
}

impl ExtraSettings {
    /// Reads the switches from `env`: the process at the entry of a service, a `MapEnv` in a
    /// test.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            pagination_tokens: env.var("AZ_PAGINATION_ENGINE").as_deref() == Some("tokens"),
            screencap_debug: env.is_set("AZ_SCREENCAP_DEBUG"),
            screencap_start_timeout_secs: env
                .var("AZ_SCREENCAP_START_TIMEOUT_SECS")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(SCREENCAP_START_TIMEOUT_SECS),
        }
    }

    /// The switches of THIS process, read now: for the entry points of a service that has no app
    /// to carry them (see the module documentation).
    #[must_use]
    pub fn of_process() -> Self {
        Self::from_lookup(&crate::env::ProcessEnv::new())
    }
}

/// The empty environment: the slicer, no trace, the portal waits five minutes.
impl Default for ExtraSettings {
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "extra_settings_test.rs"]
mod tests;
