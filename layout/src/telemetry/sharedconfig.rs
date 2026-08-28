//! The machine-wide azul config file: `{config_dir}/azul/config.json`.
//!
//! ONE human-readable file governs the *defaults* for every azul app on the
//! machine — telemetry consent by release CHANNEL, per-app overrides, and
//! update behaviour — so consent dialogs can offer "remember this setting
//! for other apps" and actually mean it:
//!
//! ```json
//! {
//!   "telemetry": {
//!     "stable":  [],
//!     "beta":    ["logs", "metrics"],
//!     "nightly": ["logs", "metrics", "appdata"],
//!     "overrides": {
//!       "myapp": { "signals": ["metrics"], "disabled_metrics": ["app_frame_seconds"] }
//!     }
//!   },
//!   "updates": {
//!     "autoupdate": true,
//!     "maintenance_window": "FREQ=DAILY;BYHOUR=2;DURATION=PT2H",
//!     "overrides": { "myapp": { "autoupdate": false } }
//!   }
//! }
//! ```
//!
//! * A channel's value is a SIGNAL LIST: any of `"crashes"`, `"logs"`,
//!   `"metrics"`, `"appdata"` — or the string `"off"` instead of a list to
//!   forbid even crash reports. An EMPTY list means crash reports only (the
//!   "please fix it" baseline); an ABSENT channel means the file says
//!   nothing and the regular consent ladder decides.
//! * `overrides` are keyed by the app's EXECUTABLE NAME
//!   (`std::env::current_exe` file stem) and win over the channel default.
//! * `disabled_metrics` is the per-metric opt-out the consent dialog's
//!   checkmark list writes: those instrument NAMES are never recorded.
//! * `updates.autoupdate: false` clamps self-updating apps to notify-only;
//!   `maintenance_window` is an RRULE(-subset) string gating when unattended
//!   update work (automatic staging today, unattended apply when it exists)
//!   may run. Supported: `FREQ=DAILY|WEEKLY`, `BYDAY=MO,..`, `BYHOUR=n`
//!   (window start, default 0), `BYMINUTE=n`, and the non-standard
//!   `DURATION=PTnH|PTnM` component (default 4h) — documented here because
//!   plain RRULE has no duration.
//!
//! Precedence within telemetry stays: env > exe-adjacent pin > per-app
//! `telemetry.json` > THIS FILE (app override, then channel default) > the
//! legacy user-global `telemetry.json` > off.

use std::{collections::BTreeMap, path::PathBuf};

use serde_json::Value;

use super::config::{config_dir, TelemetryTier};

/// Basename of the shared file, under `{config_dir}/azul/`.
pub const SHARED_CONFIG_FILE: &str = "config.json";

/// Full path of the shared config file.
#[must_use]
pub fn shared_config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("azul").join(SHARED_CONFIG_FILE))
}

/// This process's override key: the executable's file stem, lowercased.
#[must_use]
pub fn app_key() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.file_stem()?.to_string_lossy().to_ascii_lowercase())
}

/// What the shared file says about telemetry for one (channel, app).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SharedTelemetry {
    /// The resolved signal set, `None` when the file does not speak for
    /// this channel/app at all (the consent ladder continues past it).
    pub signals: Option<SignalSet>,
    /// Instrument names the user unchecked in the consent dialog. Applied
    /// whenever collection is on, regardless of which layer decided that.
    pub disabled_metrics: Vec<String>,
    /// Where the decision came from (override vs channel default) — the
    /// consent dialog shows this.
    pub from_override: bool,
}

/// The four consent signals, decomposed (finer than [`TelemetryTier`]:
/// logs and metrics get independent checkmarks).
// Four bools IS the domain: four independent consent checkboxes.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct SignalSet {
    /// Crash/panic reports.
    pub crashes: bool,
    /// Log records.
    pub logs: bool,
    /// Metrics (counters/gauges/histograms).
    pub metrics: bool,
    /// Serialized app state riding along on a crash.
    pub appdata: bool,
}

impl SignalSet {
    /// The closest [`TelemetryTier`] — the coarse atomic the hot path reads.
    /// Signal-level refinement (logs off while metrics on) is enforced on
    /// top by the per-signal gates.
    #[must_use]
    pub fn tier(self) -> TelemetryTier {
        if self.appdata {
            TelemetryTier::Full
        } else if self.metrics || self.logs {
            TelemetryTier::Metrics
        } else if self.crashes {
            TelemetryTier::Crashes
        } else {
            TelemetryTier::Off
        }
    }

    /// Parses a channel/override value: a list of signal names, or the
    /// string `"off"`. An empty list = crashes only. Unknown names are
    /// IGNORED (an old binary reading a newer file must not turn a partial
    /// consent into a bigger one, and must not throw the file away).
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        if let Some(s) = v.as_str() {
            return match s.trim().to_ascii_lowercase().as_str() {
                "off" | "none" | "disabled" => Some(Self::default()),
                _ => None,
            };
        }
        let list = v.as_array()?;
        let mut set = Self {
            crashes: true, // listing signals at all opts into the baseline
            ..Self::default()
        };
        for item in list {
            match item.as_str().map(str::to_ascii_lowercase).as_deref() {
                Some("crashes" | "crash") => set.crashes = true,
                Some("logs" | "log") => set.logs = true,
                Some("metrics" | "metric") => set.metrics = true,
                Some("appdata" | "state") => set.appdata = true,
                _ => {}
            }
        }
        Some(set)
    }

    /// Renders back to the on-disk list form (or `"off"`).
    #[must_use]
    pub fn to_value(self) -> Value {
        if self == Self::default() {
            return Value::String("off".to_owned());
        }
        let mut list = Vec::new();
        if self.crashes {
            list.push(Value::String("crashes".to_owned()));
        }
        if self.logs {
            list.push(Value::String("logs".to_owned()));
        }
        if self.metrics {
            list.push(Value::String("metrics".to_owned()));
        }
        if self.appdata {
            list.push(Value::String("appdata".to_owned()));
        }
        Value::Array(list)
    }
}

/// What the shared file says about updates for one app.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SharedUpdates {
    /// `Some(false)` clamps self-update to notify-only; `None` = the file
    /// does not speak.
    pub autoupdate: Option<bool>,
    /// RRULE(-subset) maintenance window; `None` = no restriction.
    pub maintenance_window: Option<String>,
}

/// The parsed shared file (both sections raw enough to re-serialize).
#[derive(Debug, Clone, Default)]
pub struct SharedConfig {
    root: Value,
}

impl SharedConfig {
    /// Loads `{config_dir}/azul/config.json`; a missing or malformed file is
    /// the empty config (this file is OPTIONAL machine state, and a corrupt
    /// one must not take telemetry or updates hostage).
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = shared_config_path() else {
            return Self::default();
        };
        Self::load_from(&path)
    }

    /// Loads from an explicit path (tests).
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(root) if root.is_object() => Self { root },
            _ => Self::default(),
        }
    }

    /// Telemetry resolution for (channel, app): the app OVERRIDE wins over
    /// the channel default; `disabled_metrics` is the UNION of the shared
    /// list and the override's list (unchecking a metric anywhere keeps it
    /// unchecked).
    #[must_use]
    pub fn telemetry_for(&self, channel: &str, app: &str) -> SharedTelemetry {
        let mut out = SharedTelemetry::default();
        let Some(tele) = self.root.get("telemetry") else {
            return out;
        };
        if let Some(v) = tele.get(channel) {
            out.signals = SignalSet::from_value(v);
        }
        if let Some(list) = tele.get("disabled_metrics").and_then(Value::as_array) {
            out.disabled_metrics
                .extend(list.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        if let Some(ov) = tele.get("overrides").and_then(|o| o.get(app)) {
            // An override may refine just the signals, just the disabled
            // list, or both. `signals` may also be keyed per channel.
            let sig = ov.get("signals").or_else(|| ov.get(channel));
            if let Some(v) = sig {
                if let Some(parsed) = SignalSet::from_value(v) {
                    out.signals = Some(parsed);
                    out.from_override = true;
                }
            }
            if let Some(list) = ov.get("disabled_metrics").and_then(Value::as_array) {
                out.disabled_metrics
                    .extend(list.iter().filter_map(Value::as_str).map(str::to_owned));
                out.from_override = true;
            }
        }
        out.disabled_metrics.sort();
        out.disabled_metrics.dedup();
        out
    }

    /// Update policy for one app (override wins field-by-field).
    #[must_use]
    pub fn updates_for(&self, app: &str) -> SharedUpdates {
        let mut out = SharedUpdates::default();
        let Some(up) = self.root.get("updates") else {
            return out;
        };
        out.autoupdate = up.get("autoupdate").and_then(Value::as_bool);
        out.maintenance_window = up
            .get("maintenance_window")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(ov) = up.get("overrides").and_then(|o| o.get(app)) {
            if let Some(b) = ov.get("autoupdate").and_then(Value::as_bool) {
                out.autoupdate = Some(b);
            }
            if let Some(w) = ov.get("maintenance_window").and_then(Value::as_str) {
                out.maintenance_window = Some(w.to_owned());
            }
        }
        out
    }

    /// Writes a telemetry decision. `app: None` = the shared channel
    /// default ("remember this setting for other apps"); `app: Some` = this
    /// app's override. `disabled_metrics: None` leaves the existing list.
    pub fn set_telemetry(
        &mut self,
        channel: &str,
        app: Option<&str>,
        signals: SignalSet,
        disabled_metrics: Option<&[String]>,
    ) {
        let root = self.root.as_object_mut_or_init();
        let tele = root
            .entry("telemetry")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let tele = tele.as_object_mut_or_init();
        match app {
            None => {
                tele.insert(channel.to_owned(), signals.to_value());
                if let Some(list) = disabled_metrics {
                    tele.insert(
                        "disabled_metrics".to_owned(),
                        Value::Array(list.iter().map(|s| Value::String(s.clone())).collect()),
                    );
                }
            }
            Some(app) => {
                let ovs = tele
                    .entry("overrides")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                let ovs = ovs.as_object_mut_or_init();
                let entry = ovs
                    .entry(app.to_owned())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                let entry = entry.as_object_mut_or_init();
                entry.insert("signals".to_owned(), signals.to_value());
                if let Some(list) = disabled_metrics {
                    entry.insert(
                        "disabled_metrics".to_owned(),
                        Value::Array(list.iter().map(|s| Value::String(s.clone())).collect()),
                    );
                }
            }
        }
    }

    /// Writes an update decision (same shared-vs-override split).
    pub fn set_autoupdate(&mut self, app: Option<&str>, autoupdate: bool) {
        let root = self.root.as_object_mut_or_init();
        let up = root
            .entry("updates")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let up = up.as_object_mut_or_init();
        match app {
            None => {
                up.insert("autoupdate".to_owned(), Value::Bool(autoupdate));
            }
            Some(app) => {
                let ovs = up
                    .entry("overrides")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                let ovs = ovs.as_object_mut_or_init();
                let entry = ovs
                    .entry(app.to_owned())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                entry
                    .as_object_mut_or_init()
                    .insert("autoupdate".to_owned(), Value::Bool(autoupdate));
            }
        }
    }

    /// Saves atomically (tmp + rename) to the default path.
    ///
    /// # Errors
    ///
    /// Returns a description on IO failure.
    pub fn save(&self) -> Result<(), String> {
        let path = shared_config_path().ok_or("no config dir on this platform")?;
        self.save_to(&path)
    }

    /// Saves atomically to an explicit path (tests).
    ///
    /// # Errors
    ///
    /// Returns a description on IO failure.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(&self.root).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

/// `Value::as_object_mut` that replaces non-objects with an empty object —
/// a hand-edited scalar in the file must not make the writer panic.
trait AsObjectMutOrInit {
    fn as_object_mut_or_init(&mut self) -> &mut serde_json::Map<String, Value>;
}

impl AsObjectMutOrInit for Value {
    fn as_object_mut_or_init(&mut self) -> &mut serde_json::Map<String, Value> {
        if !self.is_object() {
            *self = Self::Object(serde_json::Map::new());
        }
        self.as_object_mut().expect("just ensured object")
    }
}

// ---------------------------------------------------------------------------
// Maintenance window: RRULE subset
// ---------------------------------------------------------------------------

/// Evaluates the maintenance-window RRULE subset at `now` (unix seconds,
/// interpreted in UTC — the machine park this feature targets schedules in
/// UTC; a local-time mode can ride a `TZID=` extension later).
///
/// Supported components: `FREQ=DAILY|WEEKLY`, `BYDAY=MO,TU,..` (WEEKLY),
/// `BYHOUR=n` (window start hour, default 0), `BYMINUTE=n` (default 0), and
/// the documented non-standard `DURATION=PTnH`/`PTnM` (default 4 hours).
/// An unparseable rule returns `true` — a broken schedule must not wedge
/// updates off forever; it fails OPEN with the whole day as the window.
#[must_use]
pub fn within_maintenance_window(rrule: &str, now_unix: u64) -> bool {
    let rrule = rrule.trim();
    if rrule.is_empty() {
        return true;
    }
    let mut freq = None;
    let mut by_day: Option<Vec<u8>> = None; // 0 = Monday
    let mut by_hour = 0u64;
    let mut by_minute = 0u64;
    let mut duration_secs = 4 * 3600u64;
    for part in rrule.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => freq = Some(value.trim().to_ascii_uppercase()),
            "BYDAY" => {
                let days: Vec<u8> = value
                    .split(',')
                    .filter_map(|d| match d.trim().to_ascii_uppercase().as_str() {
                        "MO" => Some(0),
                        "TU" => Some(1),
                        "WE" => Some(2),
                        "TH" => Some(3),
                        "FR" => Some(4),
                        "SA" => Some(5),
                        "SU" => Some(6),
                        _ => None,
                    })
                    .collect();
                if !days.is_empty() {
                    by_day = Some(days);
                }
            }
            "BYHOUR" => by_hour = value.trim().parse().unwrap_or(0).min(23),
            "BYMINUTE" => by_minute = value.trim().parse().unwrap_or(0).min(59),
            "DURATION" => {
                let v = value.trim().to_ascii_uppercase();
                let v = v.strip_prefix("PT").unwrap_or(&v);
                if let Some(h) = v.strip_suffix('H').and_then(|n| n.parse::<u64>().ok()) {
                    duration_secs = h * 3600;
                } else if let Some(m) = v.strip_suffix('M').and_then(|n| n.parse::<u64>().ok()) {
                    duration_secs = m * 60;
                }
            }
            _ => {}
        }
    }
    let Some(freq) = freq else {
        return true; // fail open: no FREQ = not a rule we understand
    };
    let secs_of_day = now_unix % 86_400;
    // 1970-01-01 was a Thursday: day 0 of unix time = weekday 3 (0 = Monday).
    let weekday = ((now_unix / 86_400) + 3) % 7;
    let start = by_hour * 3600 + by_minute * 60;
    let in_window_today = |secs: u64| -> bool {
        let end = start + duration_secs;
        if end <= 86_400 {
            secs >= start && secs < end
        } else {
            // window wraps past midnight
            secs >= start || secs < (end - 86_400)
        }
    };
    match freq.as_str() {
        "DAILY" => in_window_today(secs_of_day),
        "WEEKLY" => {
            let days = by_day.unwrap_or_else(|| vec![u8::try_from(weekday).unwrap_or(0)]);
            // A wrapping window that started yesterday also counts.
            let today = u8::try_from(weekday).unwrap_or(0);
            let yesterday = u8::try_from((weekday + 6) % 7).unwrap_or(0);
            let starts_today = days.contains(&today) && in_window_today(secs_of_day);
            let wrapped_from_yesterday = start + duration_secs > 86_400
                && days.contains(&yesterday)
                && secs_of_day < (start + duration_secs - 86_400);
            starts_today || wrapped_from_yesterday
        }
        _ => true, // fail open on unknown FREQ
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture(tag: &str, json: &str) -> (std::path::PathBuf, SharedConfig) {
        let dir = std::env::temp_dir().join(format!("azul-sharedcfg-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, json).unwrap();
        let cfg = SharedConfig::load_from(&path);
        (path, cfg)
    }

    #[test]
    fn channel_default_resolves_and_override_wins() {
        let (path, cfg) = write_fixture(
            "chan",
            r#"{
                "telemetry": {
                    "stable": [],
                    "beta": ["logs", "metrics"],
                    "nightly": ["logs", "metrics", "appdata"],
                    "overrides": {
                        "myapp": { "signals": ["metrics"], "disabled_metrics": ["app_frame_seconds"] }
                    }
                }
            }"#,
        );
        // channel defaults
        let stable = cfg.telemetry_for("stable", "otherapp");
        assert_eq!(
            stable.signals,
            Some(SignalSet {
                crashes: true,
                ..SignalSet::default()
            }),
            "empty list = crash baseline"
        );
        assert_eq!(stable.signals.unwrap().tier(), TelemetryTier::Crashes);
        let beta = cfg.telemetry_for("beta", "otherapp").signals.unwrap();
        assert!(beta.logs && beta.metrics && !beta.appdata);
        assert_eq!(beta.tier(), TelemetryTier::Metrics);
        let nightly = cfg.telemetry_for("nightly", "otherapp").signals.unwrap();
        assert_eq!(nightly.tier(), TelemetryTier::Full);
        // absent channel: the file does not speak
        assert_eq!(cfg.telemetry_for("weird", "otherapp").signals, None);
        // override wins and carries the disabled metric
        let mine = cfg.telemetry_for("nightly", "myapp");
        let sig = mine.signals.unwrap();
        assert!(
            sig.metrics && !sig.logs && !sig.appdata,
            "override replaced the channel default"
        );
        assert!(mine.from_override);
        assert_eq!(mine.disabled_metrics, vec!["app_frame_seconds".to_owned()]);
        drop(std::fs::remove_file(path));
    }

    #[test]
    fn off_string_forbids_even_crashes() {
        let (path, cfg) = write_fixture("off", r#"{ "telemetry": { "stable": "off" } }"#);
        let sig = cfg.telemetry_for("stable", "x").signals.unwrap();
        assert_eq!(sig.tier(), TelemetryTier::Off);
        drop(std::fs::remove_file(path));
    }

    #[test]
    fn updates_override_wins_field_by_field() {
        let (path, cfg) = write_fixture(
            "upd",
            r#"{
                "updates": {
                    "autoupdate": true,
                    "maintenance_window": "FREQ=DAILY;BYHOUR=2;DURATION=PT2H",
                    "overrides": { "myapp": { "autoupdate": false } }
                }
            }"#,
        );
        let other = cfg.updates_for("otherapp");
        assert_eq!(other.autoupdate, Some(true));
        assert_eq!(
            other.maintenance_window.as_deref(),
            Some("FREQ=DAILY;BYHOUR=2;DURATION=PT2H")
        );
        let mine = cfg.updates_for("myapp");
        assert_eq!(mine.autoupdate, Some(false), "override wins");
        assert_eq!(
            mine.maintenance_window.as_deref(),
            Some("FREQ=DAILY;BYHOUR=2;DURATION=PT2H"),
            "unoverridden fields keep the shared value"
        );
        drop(std::fs::remove_file(path));
    }

    #[test]
    fn writer_round_trips_both_scopes() {
        let dir = std::env::temp_dir().join(format!("azul-sharedcfg-w-{}", std::process::id()));
        let path = dir.join("config.json");
        let mut cfg = SharedConfig::default();
        cfg.set_telemetry(
            "beta",
            None,
            SignalSet {
                crashes: true,
                logs: true,
                metrics: true,
                appdata: false,
            },
            None,
        );
        cfg.set_telemetry(
            "beta",
            Some("myapp"),
            SignalSet {
                crashes: true,
                metrics: true,
                ..SignalSet::default()
            },
            Some(&["app_frame_seconds".to_owned()]),
        );
        cfg.set_autoupdate(None, true);
        cfg.set_autoupdate(Some("myapp"), false);
        cfg.save_to(&path).unwrap();
        let re = SharedConfig::load_from(&path);
        assert_eq!(
            re.telemetry_for("beta", "other").signals.unwrap().tier(),
            TelemetryTier::Metrics
        );
        let mine = re.telemetry_for("beta", "myapp");
        assert!(mine.from_override);
        assert!(!mine.signals.unwrap().logs);
        assert_eq!(mine.disabled_metrics, vec!["app_frame_seconds".to_owned()]);
        assert_eq!(re.updates_for("other").autoupdate, Some(true));
        assert_eq!(re.updates_for("myapp").autoupdate, Some(false));
        drop(std::fs::remove_dir_all(dir));
    }

    // ---- maintenance window ------------------------------------------------

    // Oracle: 2027-01-04 is a Monday. 1_798_761_600 = 2027-01-01T00:00:00Z
    // (Friday). Derived by hand from days-since-epoch arithmetic; the
    // weekday law below pins the epoch anchor independently.
    const FRI_MIDNIGHT: u64 = 1_798_761_600;

    #[test]
    fn daily_window_gates_by_hour() {
        let rule = "FREQ=DAILY;BYHOUR=2;DURATION=PT2H";
        assert!(!within_maintenance_window(rule, FRI_MIDNIGHT)); // 00:00
        assert!(within_maintenance_window(rule, FRI_MIDNIGHT + 2 * 3600)); // 02:00
        assert!(within_maintenance_window(
            rule,
            FRI_MIDNIGHT + 3 * 3600 + 59 * 60
        )); // 03:59
        assert!(!within_maintenance_window(rule, FRI_MIDNIGHT + 4 * 3600)); // 04:00
    }

    #[test]
    fn weekly_window_gates_by_day_and_wraps_midnight() {
        // Friday 23:00 + 2h wraps into Saturday 01:00.
        let rule = "FREQ=WEEKLY;BYDAY=FR;BYHOUR=23;DURATION=PT2H";
        assert!(within_maintenance_window(rule, FRI_MIDNIGHT + 23 * 3600)); // Fri 23:00
        assert!(
            within_maintenance_window(rule, FRI_MIDNIGHT + 24 * 3600 + 30 * 60),
            "wrapped window continues into Saturday 00:30"
        );
        assert!(!within_maintenance_window(rule, FRI_MIDNIGHT + 25 * 3600)); // Sat 01:00
        assert!(!within_maintenance_window(rule, FRI_MIDNIGHT + 12 * 3600)); // Fri noon
                                                                             // Monday is outside BYDAY=FR entirely.
        let monday_noon = FRI_MIDNIGHT + 3 * 86_400 + 12 * 3600;
        assert!(!within_maintenance_window(rule, monday_noon));
    }

    #[test]
    fn unparseable_rules_fail_open() {
        assert!(within_maintenance_window("", FRI_MIDNIGHT));
        assert!(within_maintenance_window("garbage", FRI_MIDNIGHT));
        assert!(within_maintenance_window(
            "FREQ=MONTHLY;BYHOUR=2",
            FRI_MIDNIGHT
        ));
    }

    #[test]
    fn epoch_weekday_anchor_is_correct() {
        // 1970-01-01 (unix day 0) was a THURSDAY = weekday 3 with 0=Monday.
        // A weekly Thursday rule must therefore match unix time 0.
        assert!(within_maintenance_window(
            "FREQ=WEEKLY;BYDAY=TH;BYHOUR=0;DURATION=PT1H",
            0
        ));
        assert!(!within_maintenance_window(
            "FREQ=WEEKLY;BYDAY=FR;BYHOUR=0;DURATION=PT1H",
            0
        ));
    }
}
