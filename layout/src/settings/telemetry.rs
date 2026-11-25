//! The settings of the telemetry client that come from the environment, and the one place of the
//! crate that names their variables.
//!
//! The telemetry client is ONE per process (its consent tier, metric registry, log buffer, uploader
//! thread and crash hook are statics of `crate::telemetry`), so its settings are handed to it once,
//! by the app start: `telemetry::init` takes a [`TelemetrySettings`], keeps it with the rest of the
//! process state, and every later question that used to ask the environment (the config folder, the
//! home folder to strip from a backtrace, whether a scripted run owns the process) asks that copy.
//! Nothing here reads the process: [`TelemetrySettings::from_lookup`] takes a `&dyn EnvSource`
//! (`azul_css::env`), the one process reader is `azul_dll::env::ProcessEnv`, and `Default` is the
//! behaviour of an EMPTY environment (no layer says anything).

use std::ffi::OsString;

use azul_css::env::EnvSource;

use super::platform::{os_var, PlatformEnv};

/// The ONE-VAR local-observability switch. `AZ_OBSERVE=1` (or `local`) = "give me everything
/// against the local Grafana stack": tier `metrics`, endpoint `http://127.0.0.1:4318`, the local
/// stack's token, 5s flushes. `AZ_OBSERVE=<url>` targets a different collector. The granular
/// `AZ_TELEMETRY*` variables still override individual pieces.
pub const ENV_OBSERVE: &str = "AZ_OBSERVE";
/// Environment variable selecting the consent tier.
pub const ENV_TIER: &str = "AZ_TELEMETRY";
/// Environment variable overriding the OTLP base endpoint.
pub const ENV_ENDPOINT: &str = "AZ_TELEMETRY_ENDPOINT";
/// Environment variable supplying the ingest bearer token.
pub const ENV_TOKEN: &str = "AZ_TELEMETRY_TOKEN";
/// Environment variable overriding the flush interval, in seconds.
pub const ENV_FLUSH_SECS: &str = "AZ_TELEMETRY_FLUSH_SECS";
/// Environment variable pinning the client id (test/CI determinism).
pub const ENV_CLIENT_ID: &str = "AZ_TELEMETRY_CLIENT_ID";

/// The env var carrying the crash-dump path into the REINVOKED reporter process. `AzApp::run` (and
/// the telemetry demo) check it at startup: when set, the process is not a normal launch - it parses
/// the dump, shows it to the user (CPU rendering only; the crash may well be the GPU path) and offers
/// manual submission.
pub const CRASH_DUMP_ENV: &str = "AZ_CRASH_DUMP";

/// What the environment asks of the telemetry client: the layer of the configuration that wins over
/// every file, and the facts about the process the client needs at crash time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// `AZ_OBSERVE`: the local-observability switch or the collector's URL, as written.
    pub observe: Option<String>,
    /// `AZ_TELEMETRY`: the consent tier, as written (`off`, `crashes`, `metrics`, `full`; anything
    /// else is reported and ignored where it is applied).
    pub tier: Option<String>,
    /// `AZ_TELEMETRY_ENDPOINT`: the OTLP base endpoint.
    pub endpoint: Option<String>,
    /// `AZ_TELEMETRY_TOKEN`: the ingest bearer token.
    pub token: Option<String>,
    /// `AZ_TELEMETRY_FLUSH_SECS`: the flush interval in seconds, as written.
    pub flush_secs: Option<String>,
    /// `AZ_TELEMETRY_CLIENT_ID`: the pinned client id.
    pub client_id: Option<String>,
    /// `AZ_CRASH_DUMP`: the crash dump this process was started to show - it IS the crash reporter,
    /// not the app.
    pub crash_dump: Option<OsString>,
    /// `AZ_E2E` is set: a scripted run owns the process; nobody could dismiss a crash reporter.
    pub e2e_run: bool,
    /// The platform variables: the config and data folders when `dirs` has none, the home folder
    /// stripped from a backtrace, the windowing system reported with a slow frame.
    pub platform: PlatformEnv,
}

impl TelemetrySettings {
    /// What the environment `env` says; an empty environment says nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            observe: env.var(ENV_OBSERVE),
            tier: env.var(ENV_TIER),
            endpoint: env.var(ENV_ENDPOINT),
            token: env.var(ENV_TOKEN),
            flush_secs: env.var(ENV_FLUSH_SECS),
            client_id: env.var(ENV_CLIENT_ID),
            crash_dump: os_var(env, CRASH_DUMP_ENV),
            e2e_run: env.is_set("AZ_E2E"),
            platform: PlatformEnv::from_lookup(env),
        }
    }
}

#[cfg(test)]
#[path = "telemetry_test.rs"]
mod tests;
