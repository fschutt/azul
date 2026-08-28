//! Runtime telemetry configuration: consent tier, identity, endpoints, and
//! the layered file/env loading rules.
//!
//! Unlike `AZ_PROFILE`/`AZ_LOG`, which resolve once into a `OnceLock`, this
//! config is **runtime-mutable** — a consent toggle in an app's settings must
//! be able to flip collection on and off inside a running process. The hot
//! path (`tier()`) is therefore a single relaxed atomic load, and the full
//! record lives behind an `RwLock` that only the (rare) config reads and
//! writes touch.
//!
//! Loading precedence, most specific wins:
//!
//! 1. the `AZ_TELEMETRY` environment variable (CI, corporate lockdown, tests)
//! 2. `.azul/telemetryconfig.json` next to the executable (packager/admin pin;
//!    `"tier": "off"` here suppresses even the consent dialog)
//! 3. `{config_dir}/{app-id}/telemetry.json` (this app's user choice)
//! 4. `{config_dir}/azul/telemetry.json` (user-global "remember for all azul
//!    apps")
//!
//! The files are human-readable JSON on purpose: the config *is* part of the
//! transparency story.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        OnceLock, RwLock,
    },
};

use serde_json::{json, Map, Value};

/// Parses a config layer. Thin wrapper so call sites read the same as the
/// old hand-rolled reader's `parse`.
fn parse(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

/// `u64` from a JSON value, ALSO accepting a numeric string — OTLP-style
/// configs and hand-edited files routinely quote integers.
fn value_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
}

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

/// Basename of the per-user config file, in layers 3 and 4.
pub const USER_CONFIG_FILE: &str = "telemetry.json";
/// Path of the executable-adjacent admin pin, relative to the binary.
pub const PINNED_CONFIG_PATH: &str = ".azul/telemetryconfig.json";

/// How much an opted-in user agrees to send.
///
/// Ordered: every tier includes everything the tiers below it allow.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum TelemetryTier {
    /// Nothing leaves the machine. The default, always.
    #[default]
    Off = 0,
    /// Crash and panic reports only, asked for per crash.
    Crashes = 1,
    /// Crashes plus anonymous metrics and logs.
    Metrics = 2,
    /// Everything above plus the serialized app state on a crash. Still
    /// double-keyed: the app must also have registered `RefAny::new_serde`.
    Full = 3,
}

impl TelemetryTier {
    /// Parses `off` / `crashes` / `metrics` / `full`, case-insensitively.
    ///
    /// Unknown values return `None` so a typo is visible rather than silently
    /// enabling or disabling collection.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" => Some(Self::Off),
            "crashes" | "crash" => Some(Self::Crashes),
            "metrics" | "all" => Some(Self::Metrics),
            "full" | "state" => Some(Self::Full),
            _ => None,
        }
    }

    /// The canonical lowercase name, as written to the config files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Crashes => "crashes",
            Self::Metrics => "metrics",
            Self::Full => "full",
        }
    }

    /// Reconstructs a tier from its discriminant, saturating at `Full`.
    #[must_use]
    pub const fn from_u8(raw: u8) -> Self {
        match raw {
            1 => Self::Crashes,
            2 => Self::Metrics,
            3 => Self::Full,
            _ => Self::Off,
        }
    }

    /// Whether metrics and log records may be collected and uploaded.
    #[must_use]
    pub fn allows_metrics(self) -> bool {
        self >= Self::Metrics
    }

    /// Whether crash reports may be uploaded.
    #[must_use]
    pub fn allows_crashes(self) -> bool {
        self >= Self::Crashes
    }

    /// Whether a serialized app-state snapshot may ride along on a crash.
    #[must_use]
    pub fn allows_state_snapshot(self) -> bool {
        self >= Self::Full
    }
}

/// Which config file a "remember this choice" write goes to.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConsentScope {
    /// `{config_dir}/{app-id}/telemetry.json` — this app only.
    ThisApp,
    /// `{config_dir}/azul/telemetry.json` — every azul app on this machine.
    AllAzulApps,
}

/// The resolved telemetry configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConfig {
    /// Consent tier. `Off` unless a layer says otherwise.
    pub tier: TelemetryTier,
    /// Random UUID identifying this install for crash-free-users and adoption
    /// dedup. Present only at tier >= `Metrics`; never used as a metric label.
    pub client_id: Option<String>,
    /// OTLP/HTTP base endpoint, e.g. `http://127.0.0.1:4318`. Signal paths
    /// (`/v1/metrics`, `/v1/logs`) are appended.
    pub endpoint: String,
    /// Bearer token presented to the ingest endpoint.
    pub auth_token: Option<String>,
    /// How often the uploader flushes, in seconds.
    pub flush_interval_secs: u64,
    /// Per-signal absolute URL overrides, keyed `metrics` / `logs` / `crashes`.
    pub endpoint_overrides: BTreeMap<String, String>,
    /// App versions whose consent dialog has already been shown.
    pub asked_versions: Vec<String>,
    /// Set when a higher-precedence layer pinned `tier: off`. Apps must not
    /// show the consent dialog when this is true.
    pub pinned_off: bool,
    /// Which layer the effective tier came from, for the settings UI and the
    /// startup announce line.
    pub tier_source: TierSource,
}

/// Which layer supplied the effective tier.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum TierSource {
    /// No layer set one — collection is off by construction.
    #[default]
    Default,
    /// The `AZ_TELEMETRY` environment variable.
    Env,
    /// The executable-adjacent admin/packager pin.
    Pinned,
    /// The machine-wide `{config_dir}/azul/config.json` (channel default
    /// or per-app override) — see `telemetry::sharedconfig`.
    SharedConfig,
    /// `{config_dir}/{app-id}/telemetry.json`.
    PerApp,
    /// `{config_dir}/azul/telemetry.json`.
    UserGlobal,
    /// Set at runtime through [`set_tier`].
    Runtime,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            tier: TelemetryTier::Off,
            client_id: None,
            endpoint: String::new(),
            auth_token: None,
            flush_interval_secs: 60,
            endpoint_overrides: BTreeMap::new(),
            asked_versions: Vec::new(),
            pinned_off: false,
            tier_source: TierSource::Default,
        }
    }
}

impl TelemetryConfig {
    /// Absolute URL for a signal, honouring `endpoint_overrides`.
    ///
    /// `signal` is the OTLP path segment: `metrics`, `logs` or `traces`.
    #[must_use]
    pub fn signal_url(&self, signal: &str) -> Option<String> {
        if let Some(url) = self.endpoint_overrides.get(signal) {
            return Some(url.clone());
        }
        if self.endpoint.is_empty() {
            return None;
        }
        let base = self.endpoint.trim_end_matches('/');
        Some(format!("{base}/v1/{signal}"))
    }

    /// Renders the config back to the on-disk JSON shape.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut fields = Map::new();
        fields.insert("tier".to_owned(), json!(self.tier.as_str()));
        fields.insert(
            "flush_interval_secs".to_owned(),
            json!(self.flush_interval_secs),
        );
        if let Some(id) = &self.client_id {
            fields.insert("client_id".to_owned(), json!(id));
        }
        if !self.endpoint.is_empty() {
            fields.insert("endpoint".to_owned(), json!(self.endpoint));
        }
        if !self.endpoint_overrides.is_empty() {
            let overrides: Map<String, Value> = self
                .endpoint_overrides
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            fields.insert("endpoint_overrides".to_owned(), Value::Object(overrides));
        }
        if !self.asked_versions.is_empty() {
            fields.insert("asked_versions".to_owned(), json!(self.asked_versions));
        }
        // NOTE: `auth_token` is deliberately NOT serialized. It is developer
        // infrastructure credentials, not a user choice; it belongs in the
        // build or the environment, not in a file the user is invited to read
        // and share.
        Value::Object(fields).to_string()
    }

    /// Applies every field present in one JSON layer on top of `self`.
    fn apply_layer(&mut self, layer: &Value, source: TierSource) {
        if let Some(tier) = layer
            .get("tier")
            .and_then(Value::as_str)
            .and_then(TelemetryTier::from_name)
        {
            self.tier = tier;
            self.tier_source = source;
            // A pin (env or executable-adjacent) that says "off" also
            // suppresses the consent dialog — an employee under a corporate
            // policy should never be asked a question they cannot answer.
            if tier == TelemetryTier::Off && matches!(source, TierSource::Env | TierSource::Pinned)
            {
                self.pinned_off = true;
            }
        }
        if let Some(id) = layer.get("client_id").and_then(Value::as_str) {
            self.client_id = Some(id.to_owned());
        }
        if let Some(endpoint) = layer.get("endpoint").and_then(Value::as_str) {
            endpoint.clone_into(&mut self.endpoint);
        }
        if let Some(secs) = layer.get("flush_interval_secs").and_then(value_u64) {
            self.flush_interval_secs = secs.max(1);
        }
        if let Some(overrides) = layer.get("endpoint_overrides").and_then(Value::as_object) {
            for (key, value) in overrides {
                if let Some(url) = value.as_str() {
                    self.endpoint_overrides.insert(key.clone(), url.to_owned());
                }
            }
        }
        if let Some(versions) = layer.get("asked_versions").and_then(Value::as_array) {
            self.asked_versions = versions
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
        }
    }
}

/// Hot-path tier, so `is_enabled()` checks never take a lock.
static TIER: AtomicU8 = AtomicU8::new(TelemetryTier::Off as u8);

/// The full resolved config. `None` until [`load`] runs.
static CONFIG: OnceLock<RwLock<TelemetryConfig>> = OnceLock::new();

fn config_cell() -> &'static RwLock<TelemetryConfig> {
    CONFIG.get_or_init(|| RwLock::new(TelemetryConfig::default()))
}

/// Signal-level refinement UNDER the tier: `logs`/`metrics` can be toggled
/// independently by the shared config's signal lists. Both default ON so
/// tier-only configs behave exactly as before.
static LOGS_ENABLED: AtomicBool = AtomicBool::new(true);
static METRICS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Applies a [`super::sharedconfig::SignalSet`]'s per-signal split.
pub fn set_signal_gates(signals: super::sharedconfig::SignalSet) {
    LOGS_ENABLED.store(signals.logs, Ordering::Relaxed);
    METRICS_ENABLED.store(signals.metrics, Ordering::Relaxed);
}

/// Whether LOG records may flow (on top of the tier check).
#[must_use]
pub fn logs_enabled() -> bool {
    LOGS_ENABLED.load(Ordering::Relaxed)
}

/// Whether METRICS may flow (on top of the tier check).
#[must_use]
pub fn metrics_enabled() -> bool {
    METRICS_ENABLED.load(Ordering::Relaxed)
}

/// The consent tier currently in force. One relaxed atomic load.
#[must_use]
pub fn tier() -> TelemetryTier {
    TelemetryTier::from_u8(TIER.load(Ordering::Relaxed))
}

/// A copy of the current configuration.
#[must_use]
pub fn snapshot() -> TelemetryConfig {
    config_cell().read().map(|c| c.clone()).unwrap_or_default()
}

/// What a [`set_tier`] call implies for data already on the server.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TierChange {
    /// Tier before the call.
    pub previous: TelemetryTier,
    /// Tier after the call.
    pub current: TelemetryTier,
    /// True when the tier moved down, which obliges the caller to send a
    /// deletion-request ping naming the retired client id. The id itself is
    /// handed over by [`take_retired_client_id`], keeping this struct `Copy`.
    pub needs_deletion_request: bool,
}

/// Changes the consent tier at runtime.
///
/// A downgrade wipes the client id locally and reports
/// `needs_deletion_request`; the caller (see
/// [`super::request_deletion`]) turns that into the final ping. An upgrade to
/// tier >= `Metrics` mints a client id if there is none.
pub fn set_tier(new_tier: TelemetryTier) -> TierChange {
    let previous = tier();
    let mut retired = None;
    if let Ok(mut config) = config_cell().write() {
        if new_tier < previous {
            retired = config.client_id.take();
        } else if new_tier.allows_metrics() && config.client_id.is_none() {
            config.client_id = Some(super::new_client_id());
        }
        config.tier = new_tier;
        config.tier_source = TierSource::Runtime;
    }
    TIER.store(new_tier as u8, Ordering::Relaxed);
    let needs_deletion_request = new_tier < previous && retired.is_some();
    if let Some(id) = retired {
        if let Ok(mut slot) = retired_id_slot().write() {
            *slot = Some(id);
        }
    }
    TierChange {
        previous,
        current: new_tier,
        needs_deletion_request,
    }
}

fn retired_id_slot() -> &'static RwLock<Option<String>> {
    static RETIRED: OnceLock<RwLock<Option<String>>> = OnceLock::new();
    RETIRED.get_or_init(|| RwLock::new(None))
}

/// Takes the client id retired by the most recent downgrade, if any.
#[must_use]
pub fn take_retired_client_id() -> Option<String> {
    retired_id_slot().write().ok().and_then(|mut s| s.take())
}

/// Overwrites the whole config, e.g. after a settings dialog.
pub fn store(config: TelemetryConfig) {
    TIER.store(config.tier as u8, Ordering::Relaxed);
    if let Ok(mut slot) = config_cell().write() {
        *slot = config;
    }
}

/// Resolves the four layers for `app_id` and installs the result.
///
/// Returns the resolved config. Safe to call more than once; the last call
/// wins.
#[must_use]
pub fn load(app_id: &str) -> TelemetryConfig {
    load_with_channel(app_id, "")
}

/// [`load`], with the release CHANNEL so the machine-wide shared config
/// (`{config_dir}/azul/config.json`) can supply its per-channel default and
/// per-app override. The shared file sits between the legacy user-global
/// layer and the per-app file; it also feeds the PER-SIGNAL gates (logs and
/// metrics independently) and the per-metric opt-out set.
#[must_use]
pub fn load_with_channel(app_id: &str, channel: &str) -> TelemetryConfig {
    let mut config = TelemetryConfig::default();

    // Least specific first: each layer overwrites the fields it declares.
    for (path, source) in [
        (user_global_config_path(), TierSource::UserGlobal),
        (per_app_config_path(app_id), TierSource::PerApp),
        (pinned_config_path(), TierSource::Pinned),
    ] {
        let Some(path) = path else { continue };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match parse(&text) {
            Ok(layer) => config.apply_layer(&layer, source),
            Err(e) => {
                // Loud, once: a malformed consent file that silently reverts
                // to "off" is indistinguishable from a working one.
                eprintln!("[azul][telemetry] ignoring {}: {e}", path.display());
            }
        }
    }

    // Machine-wide shared config: channel default + per-app override.
    // Precedence: it may override the LEGACY user-global file but never the
    // per-app file, the exe-adjacent pin, or the environment — the loop
    // above already applied those, so only fill in when nothing more
    // specific spoke.
    let shared = super::sharedconfig::SharedConfig::load();
    let shared_app = super::sharedconfig::app_key().unwrap_or_default();
    let shared_tele = shared.telemetry_for(channel, &shared_app);
    if matches!(
        config.tier_source,
        TierSource::Default | TierSource::UserGlobal
    ) {
        if let Some(signals) = shared_tele.signals {
            config.tier = signals.tier();
            config.tier_source = TierSource::SharedConfig;
            set_signal_gates(signals);
        }
    } else if let Some(signals) = shared_tele.signals {
        // A more specific layer picked the tier, but the shared file's
        // signal SPLIT still refines which streams flow at that tier.
        set_signal_gates(signals);
    }
    // The per-metric opt-out applies regardless of which layer set the tier.
    super::metrics::set_disabled_metrics(shared_tele.disabled_metrics.iter().cloned());

    apply_env(&mut config);

    if config.tier.allows_metrics() && config.client_id.is_none() {
        config.client_id = Some(super::new_client_id());
    }
    if config.tier.allows_metrics() && config.signal_url("metrics").is_none() {
        eprintln!(
            "[azul][telemetry] tier={} but no endpoint is configured: nothing will be uploaded. \
             Set {ENV_ENDPOINT}=<otlp-http-base-url> or an \"endpoint\" key in the config file.",
            config.tier.as_str()
        );
    }

    store(config.clone());
    config
}

/// Applies the environment layer (highest precedence).
fn apply_env(config: &mut TelemetryConfig) {
    if let Ok(raw) = std::env::var(ENV_TIER) {
        match TelemetryTier::from_name(&raw) {
            Some(tier) => {
                config.tier = tier;
                config.tier_source = TierSource::Env;
                if tier == TelemetryTier::Off {
                    config.pinned_off = true;
                }
            }
            None => eprintln!(
                "[azul][telemetry] {ENV_TIER}={raw:?} is not one of off|crashes|metrics|full \
                 — ignoring it (telemetry stays {})",
                config.tier.as_str()
            ),
        }
    }
    if let Ok(endpoint) = std::env::var(ENV_ENDPOINT) {
        if !endpoint.is_empty() {
            config.endpoint = endpoint;
        }
    }
    if let Ok(token) = std::env::var(ENV_TOKEN) {
        if !token.is_empty() {
            config.auth_token = Some(token);
        }
    }
    if let Ok(secs) = std::env::var(ENV_FLUSH_SECS) {
        if let Ok(parsed) = secs.parse::<u64>() {
            config.flush_interval_secs = parsed.max(1);
        }
    }
    if let Ok(id) = std::env::var(ENV_CLIENT_ID) {
        if !id.is_empty() {
            config.client_id = Some(id);
        }
    }
}

/// Persists the current tier (and client id) as a user choice.
///
/// # Errors
///
/// Returns the underlying IO error if the directory cannot be created or the
/// file cannot be written.
pub fn save_user_choice(app_id: &str, scope: ConsentScope) -> std::io::Result<PathBuf> {
    let path = match scope {
        ConsentScope::ThisApp => per_app_config_path(app_id),
        ConsentScope::AllAzulApps => user_global_config_path(),
    }
    .ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no user config directory on this platform",
        )
    })?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, snapshot().to_json())?;
    Ok(path)
}

/// `{config_dir}/azul/telemetry.json`.
#[must_use]
pub fn user_global_config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("azul").join(USER_CONFIG_FILE))
}

/// `{config_dir}/{app-id}/telemetry.json`.
#[must_use]
pub fn per_app_config_path(app_id: &str) -> Option<PathBuf> {
    Some(config_dir()?.join(app_id).join(USER_CONFIG_FILE))
}

/// `.azul/telemetryconfig.json` next to the running executable.
#[must_use]
pub fn pinned_config_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(PINNED_CONFIG_PATH))
}

/// The platform config directory.
///
/// Uses [`crate::file::FilePath::get_config_dir`] (the `dirs` crate) when the
/// `extra` feature is on, and falls back to the platform environment
/// variables otherwise, so `telemetry` does not have to pull `extra` — and
/// with it the native-dialog dependency — into a headless build.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(feature = "extra")]
    {
        if let Some(dir) = crate::file::FilePath::get_config_dir() {
            return Some(PathBuf::from(dir.inner.as_str()));
        }
    }
    fallback_dir(true)
}

/// The platform data directory, where the pending-ping queue lives.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(feature = "extra")]
    {
        if let Some(dir) = crate::file::FilePath::get_data_dir() {
            return Some(PathBuf::from(dir.inner.as_str()));
        }
    }
    fallback_dir(false)
}

/// Environment-only resolution of the config/data directory.
fn fallback_dir(config: bool) -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let var = if config { "APPDATA" } else { "LOCALAPPDATA" };
        return std::env::var_os(var).map(PathBuf::from);
    }
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support"),
        );
    }
    let (xdg, suffix) = if config {
        ("XDG_CONFIG_HOME", ".config")
    } else {
        ("XDG_DATA_HOME", ".local/share")
    };
    if let Some(dir) = std::env::var_os(xdg) {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return Some(path);
        }
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_are_ordered_and_round_trip() {
        assert!(TelemetryTier::Off < TelemetryTier::Crashes);
        assert!(TelemetryTier::Crashes < TelemetryTier::Metrics);
        assert!(TelemetryTier::Metrics < TelemetryTier::Full);
        for tier in [
            TelemetryTier::Off,
            TelemetryTier::Crashes,
            TelemetryTier::Metrics,
            TelemetryTier::Full,
        ] {
            assert_eq!(TelemetryTier::from_name(tier.as_str()), Some(tier));
            assert_eq!(TelemetryTier::from_u8(tier as u8), tier);
        }
        assert_eq!(
            TelemetryTier::from_name("MeTrIcS"),
            Some(TelemetryTier::Metrics)
        );
        assert_eq!(TelemetryTier::from_name("yes-please"), None);
    }

    #[test]
    fn tier_capabilities_are_cumulative() {
        assert!(!TelemetryTier::Off.allows_crashes());
        assert!(TelemetryTier::Crashes.allows_crashes());
        assert!(!TelemetryTier::Crashes.allows_metrics());
        assert!(TelemetryTier::Metrics.allows_metrics());
        assert!(!TelemetryTier::Metrics.allows_state_snapshot());
        assert!(TelemetryTier::Full.allows_state_snapshot());
    }

    #[test]
    fn more_specific_layers_win() {
        let mut config = TelemetryConfig::default();
        config.apply_layer(
            &parse(r#"{"tier":"metrics","endpoint":"http://global","flush_interval_secs":300}"#)
                .unwrap(),
            TierSource::UserGlobal,
        );
        assert_eq!(config.tier, TelemetryTier::Metrics);
        assert_eq!(config.flush_interval_secs, 300);

        config.apply_layer(
            &parse(r#"{"tier":"full","endpoint":"http://per-app"}"#).unwrap(),
            TierSource::PerApp,
        );
        assert_eq!(config.tier, TelemetryTier::Full);
        assert_eq!(config.endpoint, "http://per-app");
        // Untouched by the more specific layer.
        assert_eq!(config.flush_interval_secs, 300);
        assert_eq!(config.tier_source, TierSource::PerApp);
    }

    #[test]
    fn an_admin_pin_of_off_suppresses_the_dialog() {
        let mut config = TelemetryConfig::default();
        config.apply_layer(&parse(r#"{"tier":"full"}"#).unwrap(), TierSource::PerApp);
        assert!(!config.pinned_off);
        config.apply_layer(&parse(r#"{"tier":"off"}"#).unwrap(), TierSource::Pinned);
        assert_eq!(config.tier, TelemetryTier::Off);
        assert!(config.pinned_off, "a packager pin must suppress the ask");
    }

    #[test]
    fn a_user_choice_of_off_does_not_suppress_the_dialog() {
        // Only env/packager pins suppress; a user turning it off should still
        // be asked again on a channel switch.
        let mut config = TelemetryConfig::default();
        config.apply_layer(&parse(r#"{"tier":"off"}"#).unwrap(), TierSource::PerApp);
        assert!(!config.pinned_off);
    }

    #[test]
    fn signal_urls_append_the_otlp_path_and_honour_overrides() {
        let mut config = TelemetryConfig {
            endpoint: "http://127.0.0.1:4318/".to_owned(),
            ..TelemetryConfig::default()
        };
        assert_eq!(
            config.signal_url("metrics").as_deref(),
            Some("http://127.0.0.1:4318/v1/metrics")
        );
        config
            .endpoint_overrides
            .insert("logs".to_owned(), "https://ingest.example/logs".to_owned());
        assert_eq!(
            config.signal_url("logs").as_deref(),
            Some("https://ingest.example/logs")
        );
        assert_eq!(TelemetryConfig::default().signal_url("metrics"), None);
    }

    #[test]
    fn the_auth_token_is_never_written_to_disk() {
        let config = TelemetryConfig {
            tier: TelemetryTier::Metrics,
            auth_token: Some("super-secret".to_owned()),
            client_id: Some("id-1".to_owned()),
            ..TelemetryConfig::default()
        };
        let json = config.to_json();
        assert!(!json.contains("super-secret"), "json = {json}");
        assert!(json.contains("\"tier\":\"metrics\""));
        assert!(json.contains("id-1"));
    }

    #[test]
    fn saved_config_reparses_into_the_same_values() {
        let mut config = TelemetryConfig {
            tier: TelemetryTier::Full,
            client_id: Some("abc-123".to_owned()),
            endpoint: "http://localhost:4318".to_owned(),
            flush_interval_secs: 15,
            asked_versions: vec!["1.4".to_owned()],
            ..TelemetryConfig::default()
        };
        let text = config.to_json();
        let mut round = TelemetryConfig::default();
        round.apply_layer(&parse(&text).unwrap(), TierSource::PerApp);
        config.tier_source = TierSource::PerApp;
        assert_eq!(round.tier, config.tier);
        assert_eq!(round.client_id, config.client_id);
        assert_eq!(round.endpoint, config.endpoint);
        assert_eq!(round.flush_interval_secs, config.flush_interval_secs);
        assert_eq!(round.asked_versions, config.asked_versions);
    }
}
