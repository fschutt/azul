//! In-process metric registry.
//!
//! The registry aggregates client-side, so one process exports
//! `O(instruments)` series per flush regardless of how many events it saw.
//! That is both the scale answer and the privacy answer: hundreds of users are
//! not hundreds of series, as long as no per-user label exists.
//!
//! **The cardinality contract lives in the type.** [`MetricLabels`] is a
//! struct with exactly four fields — `version`, `channel`, `os`, `arch` — and
//! there is no API that attaches a free-form label map to a metric. An
//! instrument may carry at most one extra dimension, and both its key and the
//! set of values it takes are chosen in code (`scope`, `result`, `phase`), not
//! derived from user data. Per-user identity rides on log records and crash
//! reports only, never here.
//!
//! Temporality is **cumulative**: counters and histograms accumulate for the
//! life of the process and every flush ships the running total. That is what
//! Prometheus-family backends want, and it makes a dropped upload lossless —
//! the next one carries the same information.

use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

/// `app_sessions_started_total` — one per app run. The denominator of every
/// release-health ratio, so it is never sampled.
pub const SESSIONS_STARTED: &str = "app_sessions_started_total";
/// `app_crashes_total` — native crashes recovered on the next launch.
pub const CRASHES: &str = "app_crashes_total";
/// `app_panics_total` — Rust panics seen by the panic hook.
pub const PANICS: &str = "app_panics_total";
/// `app_startup_seconds` — process start to first frame.
pub const STARTUP_SECONDS: &str = "app_startup_seconds";
/// `app_startup_rss_bytes` — RSS sampled at startup and again at 60 s.
pub const STARTUP_RSS_BYTES: &str = "app_startup_rss_bytes";
/// `app_frame_relayout_scope_total{scope}` — fed from `ProcessEventResult`.
pub const RELAYOUT_SCOPE: &str = "app_frame_relayout_scope_total";
/// `app_update_check_total{result}` — the updater observing itself.
pub const UPDATE_CHECK: &str = "app_update_check_total";
/// `app_update_apply_total{result}`.
pub const UPDATE_APPLY: &str = "app_update_apply_total";
/// `app_phase_seconds{phase}` — per-phase durations bridged from `Probe`.
pub const PHASE_SECONDS: &str = "app_phase_seconds";
/// `app_rss_bytes` — current resident set size.
pub const RSS_BYTES: &str = "app_rss_bytes";

/// Wall-clock duration of one frame, dimension `scope` (`layout` / `render`
/// / `total`). THE per-frame cost histogram — query p50/p95 in ms.
pub const FRAME_SECONDS: &str = "app_frame_seconds";

/// Wall-clock duration of one TIMER tick (animation frames ride timers).
/// Slow ticks here are what make animations stutter.
pub const TIMER_FRAME_SECONDS: &str = "app_timer_frame_seconds";

/// Frames that crossed the slow threshold, dimension `scope`. The numerator
/// of "how often is it slow"; the WARN log carries WHICH frame and why.
pub const SLOW_FRAMES: &str = "app_slow_frames_total";

/// App-supplied document size, in whatever unit the app chooses (nodes,
/// paragraphs, bytes — the app's semantics). The correlator for RAM: "more
/// RSS ← massive document" is answerable only when this is recorded.
pub const DOCUMENT_SIZE: &str = "app_document_size";

/// RSS growth from opening the current document (after − before), bytes.
pub const DOC_RSS_DELTA_BYTES: &str = "app_document_rss_delta_bytes";

/// `DOC_RSS_DELTA_BYTES / DOCUMENT_SIZE` — bytes of resident growth per
/// document unit. Flat across document sizes = healthy; a jump between
/// versions = a memory regression that document size does NOT explain.
pub const DOC_RSS_PER_UNIT: &str = "app_document_rss_bytes_per_unit";
/// `app_heap_bytes` — current allocator heap usage.
pub const HEAP_BYTES: &str = "app_heap_bytes";

/// Default histogram bounds for durations, in seconds.
pub const SECONDS_BUCKETS: &[f64] = &[
    0.000_5, 0.001, 0.002_5, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Default histogram bounds for byte counts (8 MiB … 1 GiB).
pub const BYTES_BUCKETS: &[f64] = &[
    8.0e6, 16.0e6, 32.0e6, 48.0e6, 64.0e6, 96.0e6, 128.0e6, 192.0e6, 256.0e6, 384.0e6, 512.0e6,
    768.0e6, 1.024e9,
];

/// Hard ceiling on distinct series. A bug that turns a bounded dimension into
/// an unbounded one (a file path, a document title) would otherwise take the
/// backend down; here it costs one warning and dropped data points.
pub const MAX_SERIES: usize = 512;

/// The only labels a metric may carry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct MetricLabels {
    /// App version, e.g. `1.4.2`.
    pub version: String,
    /// Release channel: `release`, `beta`, `nightly`, …
    pub channel: String,
    /// Target OS, from `std::env::consts::OS`.
    pub os: String,
    /// Target architecture, from `std::env::consts::ARCH`.
    pub arch: String,
}

impl MetricLabels {
    /// Fills `os`/`arch` from the compiled target.
    #[must_use]
    pub fn detect(version: &str, channel: &str) -> Self {
        Self {
            version: version.to_owned(),
            channel: channel.to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }

    /// The labels as `(key, value)` pairs, in a stable order.
    #[must_use]
    pub fn pairs(&self) -> [(&'static str, &str); 4] {
        [
            ("version", self.version.as_str()),
            ("channel", self.channel.as_str()),
            ("os", self.os.as_str()),
            ("arch", self.arch.as_str()),
        ]
    }
}

/// Hard cap on code-chosen labels per metric. Extra labels are dropped
/// (with a one-time warning), never silently admitted: each label KEY
/// multiplies potential cardinality.
pub const MAX_LABELS_PER_METRIC: usize = 6;

/// Longest admitted label value; longer values are truncated. Bounds the
/// wire size and stops "the whole document title as a label value".
pub const MAX_LABEL_VALUE_LEN: usize = 64;

/// Identifies one series: an instrument name plus a SMALL, sanitized set of
/// code-chosen labels (sorted by key, deduped, capped at
/// [`MAX_LABELS_PER_METRIC`]). The global [`MAX_SERIES`] ceiling still
/// applies to every distinct (name, labels) combination — free-form labels
/// widen the API, not the cardinality contract.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstrumentKey {
    /// Instrument name, e.g. `app_crashes_total`.
    pub name: String,
    /// Sorted, sanitized `(key, value)` labels.
    pub dims: Vec<(String, String)>,
}

impl InstrumentKey {
    /// A key with no extra dimension.
    #[must_use]
    pub fn plain(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            dims: Vec::new(),
        }
    }

    /// A key carrying one extra dimension.
    #[must_use]
    pub fn with_dim(name: &str, dim_key: &str, dim_value: &str) -> Self {
        Self::with_labels(name, &[(dim_key, dim_value)])
    }

    /// A key carrying free-form labels, SANITIZED into a stable identity:
    /// keys are lowercased with anything outside `[a-z0-9_]` replaced by
    /// `_`, values are truncated to [`MAX_LABEL_VALUE_LEN`], the set is
    /// sorted by key and deduped (last write wins), and anything beyond
    /// [`MAX_LABELS_PER_METRIC`] is dropped with a one-time warning. The
    /// same labels in any order therefore name the SAME series.
    #[must_use]
    pub fn with_labels(name: &str, labels: &[(&str, &str)]) -> Self {
        let mut dims: Vec<(String, String)> =
            Vec::with_capacity(labels.len().min(MAX_LABELS_PER_METRIC));
        for (k, v) in labels {
            let key: String = k
                .chars()
                .map(|c| {
                    let c = c.to_ascii_lowercase();
                    if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            if key.is_empty() {
                continue;
            }
            let mut value = (*v).to_owned();
            if value.len() > MAX_LABEL_VALUE_LEN {
                let mut cut = MAX_LABEL_VALUE_LEN;
                while !value.is_char_boundary(cut) {
                    cut -= 1;
                }
                value.truncate(cut);
            }
            match dims.iter_mut().find(|(existing, _)| *existing == key) {
                Some((_, slot)) => *slot = value,
                None => dims.push((key, value)),
            }
        }
        dims.sort_by(|a, b| a.0.cmp(&b.0));
        if dims.len() > MAX_LABELS_PER_METRIC {
            use std::sync::atomic::{AtomicBool, Ordering};
            static WARNED: AtomicBool = AtomicBool::new(false);
            if !WARNED.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "[azul telemetry] metric {name:?}: more than {MAX_LABELS_PER_METRIC} labels; \
                     extra labels are DROPPED (this warning prints once)"
                );
            }
            dims.truncate(MAX_LABELS_PER_METRIC);
        }
        Self {
            name: name.to_owned(),
            dims,
        }
    }
}

/// Accumulated histogram state.
#[derive(Debug, Clone, PartialEq)]
pub struct HistogramData {
    /// Upper bucket bounds, ascending. The implicit `+Inf` bucket is not
    /// listed; `counts` is one longer than `bounds`.
    pub bounds: Vec<f64>,
    /// Per-bucket counts, `bounds.len() + 1` entries.
    pub counts: Vec<u64>,
    /// Sum of all recorded values.
    pub sum: f64,
    /// Number of recorded values.
    pub count: u64,
}

impl HistogramData {
    fn new(bounds: &[f64]) -> Self {
        Self {
            bounds: bounds.to_vec(),
            counts: vec![0; bounds.len() + 1],
            sum: 0.0,
            count: 0,
        }
    }

    fn record(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        let index = self
            .bounds
            .iter()
            .position(|bound| value <= *bound)
            .unwrap_or(self.bounds.len());
        if let Some(slot) = self.counts.get_mut(index) {
            *slot += 1;
        }
        self.sum += value;
        self.count += 1;
    }
}

/// One instrument's accumulated value.
#[derive(Debug, Clone, PartialEq)]
pub enum InstrumentValue {
    /// Monotonic cumulative counter.
    Counter(u64),
    /// Last-value gauge.
    Gauge(f64),
    /// Cumulative histogram.
    Histogram(HistogramData),
}

/// One series in a [`MetricsSnapshot`].
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// Instrument identity.
    pub key: InstrumentKey,
    /// Accumulated value.
    pub value: InstrumentValue,
}

/// A consistent read of every instrument, ready for encoding.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricsSnapshot {
    /// The four labels every series carries.
    pub labels: MetricLabels,
    /// Process start, as Unix nanoseconds — the OTLP `startTimeUnixNano` for
    /// cumulative points.
    pub start_unix_nanos: u64,
    /// When the snapshot was taken, as Unix nanoseconds.
    pub now_unix_nanos: u64,
    /// The series, in stable (sorted) order.
    pub series: Vec<Series>,
    /// How many data points were dropped by the [`MAX_SERIES`] guard.
    pub dropped_series: u64,
}

impl MetricsSnapshot {
    /// Whether there is anything worth uploading.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }
}

#[derive(Debug, Default)]
struct Registry {
    labels: MetricLabels,
    instruments: BTreeMap<InstrumentKey, InstrumentValue>,
    bounds: BTreeMap<String, Vec<f64>>,
    start_unix_nanos: u64,
    dropped_series: u64,
    warned_about_cardinality: bool,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            start_unix_nanos: super::unix_nanos(),
            ..Registry::default()
        })
    })
}

/// Runs `f` against the registry, ignoring a poisoned lock.
///
/// A telemetry mutex poisoned by an unrelated panic must never turn into a
/// second panic on the app's hot path — the whole subsystem is optional.
fn with_registry<R>(f: impl FnOnce(&mut Registry) -> R) -> Option<R> {
    let mut guard = registry().lock().ok()?;
    Some(f(&mut guard))
}

/// Installs the label set every series carries.
pub fn set_labels(labels: MetricLabels) {
    with_registry(|reg| reg.labels = labels);
}

/// The label set currently in force.
#[must_use]
pub fn labels() -> MetricLabels {
    with_registry(|reg| reg.labels.clone()).unwrap_or_default()
}

/// Declares the bucket bounds for a histogram before first use.
///
/// Bounds must be ascending; a later call is ignored once the instrument has
/// data, so app code cannot silently reshape a histogram mid-flight.
pub fn register_histogram(name: &str, bounds: &[f64]) {
    with_registry(|reg| {
        reg.bounds
            .entry(name.to_owned())
            .or_insert_with(|| bounds.to_vec());
    });
}

fn bounds_for(reg: &Registry, name: &str) -> Vec<f64> {
    reg.bounds
        .get(name)
        .cloned()
        .unwrap_or_else(|| SECONDS_BUCKETS.to_vec())
}

/// True when a new key may be created, accounting a drop otherwise.
/// One row of the transparency inventory the consent dialog renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentInfo {
    /// Instrument name as it appears on the wire.
    pub name: String,
    /// `counter` / `gauge` / `histogram`.
    pub kind: &'static str,
    /// One sentence: what this measures and why.
    pub description: String,
    /// Whether the user has this instrument enabled (not in the disabled
    /// set). The dialog's checkmark state.
    pub enabled: bool,
}

/// Every ENGINE instrument, with the sentence the consent dialog shows.
/// App-defined instruments join the inventory at runtime with a generic
/// description (see [`instrument_inventory`]).
pub const ENGINE_INSTRUMENTS: &[(&str, &str, &str)] = &[
    (
        SESSIONS_STARTED,
        "counter",
        "How often the app was started (adoption per version).",
    ),
    (CRASHES, "counter", "How often the app crashed."),
    (
        PANICS,
        "counter",
        "How often an internal error (panic) was caught.",
    ),
    (
        STARTUP_SECONDS,
        "histogram",
        "How long the app took to start.",
    ),
    (
        STARTUP_RSS_BYTES,
        "gauge",
        "Memory footprint right after startup.",
    ),
    (
        RELAYOUT_SCOPE,
        "counter",
        "Which relayout paths run (full vs incremental) - engine performance triage.",
    ),
    (
        UPDATE_CHECK,
        "counter",
        "Update-check outcomes (up to date / available / staggered / error).",
    ),
    (UPDATE_APPLY, "counter", "Update-install outcomes."),
    (
        PHASE_SECONDS,
        "histogram",
        "Time per engine phase (layout, repaint, callbacks).",
    ),
    (RSS_BYTES, "gauge", "Process memory footprint over time."),
    (
        FRAME_SECONDS,
        "histogram",
        "Time per rendered frame (smoothness).",
    ),
    (
        TIMER_FRAME_SECONDS,
        "histogram",
        "Time spent in app timer callbacks.",
    ),
    (
        SLOW_FRAMES,
        "counter",
        "Frames slower than the smoothness threshold.",
    ),
    (
        DOCUMENT_SIZE,
        "gauge",
        "App-reported size of the open document.",
    ),
    (
        DOC_RSS_DELTA_BYTES,
        "gauge",
        "Memory the open document added.",
    ),
    (
        DOC_RSS_PER_UNIT,
        "gauge",
        "Memory per document unit (is 300 MB reasonable for this file).",
    ),
    (HEAP_BYTES, "gauge", "Allocator heap in use."),
];

/// The consent dialog's checkmark list: every engine instrument plus any
/// app-defined instrument that has recorded this session, each flagged with
/// its current enabled state.
#[must_use]
pub fn instrument_inventory() -> Vec<InstrumentInfo> {
    let disabled = disabled_metrics();
    let mut out: Vec<InstrumentInfo> = ENGINE_INSTRUMENTS
        .iter()
        .map(|(name, kind, desc)| InstrumentInfo {
            name: (*name).to_owned(),
            kind,
            description: (*desc).to_owned(),
            enabled: !disabled.contains(*name),
        })
        .collect();
    with_registry(|reg| {
        for key in reg.instruments.keys() {
            if !out.iter().any(|i| i.name == key.name) {
                out.push(InstrumentInfo {
                    name: key.name.clone(),
                    kind: "app-defined",
                    description: "Recorded by this application (not an engine metric).".to_owned(),
                    enabled: !disabled.contains(key.name.as_str()),
                });
            }
        }
    });
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

fn disabled_set() -> &'static std::sync::RwLock<std::collections::BTreeSet<String>> {
    use std::{
        collections::BTreeSet,
        sync::{OnceLock, RwLock},
    };
    static DISABLED: OnceLock<RwLock<BTreeSet<String>>> = OnceLock::new();
    DISABLED.get_or_init(|| RwLock::new(BTreeSet::new()))
}

/// Replaces the per-metric opt-out set (the consent dialog's unchecked
/// rows, from the shared config's `disabled_metrics`). Disabled instruments
/// are never recorded - the user's checkmark is enforced at the source, not
/// at upload time.
pub fn set_disabled_metrics<I: IntoIterator<Item = String>>(names: I) {
    if let Ok(mut set) = disabled_set().write() {
        *set = names.into_iter().collect();
    }
}

/// The current per-metric opt-out set.
#[must_use]
pub fn disabled_metrics() -> std::collections::BTreeSet<String> {
    disabled_set().read().map(|s| s.clone()).unwrap_or_default()
}

fn admit(reg: &mut Registry, key: &InstrumentKey) -> bool {
    // The user's per-metric opt-out wins over everything: an unchecked
    // instrument records NOTHING, regardless of tier.
    if let Ok(disabled) = disabled_set().read() {
        if disabled.contains(key.name.as_str()) {
            return false;
        }
    }

    if reg.instruments.contains_key(key) {
        return true;
    }
    if reg.instruments.len() < MAX_SERIES {
        return true;
    }
    reg.dropped_series += 1;
    if !reg.warned_about_cardinality {
        reg.warned_about_cardinality = true;
        eprintln!(
            "[azul][telemetry] metric cardinality ceiling ({MAX_SERIES} series) reached at \
             {:?} — further new series are dropped. A dimension is unbounded; metric labels \
             must come from a fixed set.",
            key.name
        );
    }
    false
}

/// Adds to a counter.
pub fn counter_add(name: &str, value: u64) {
    add_to_counter(InstrumentKey::plain(name), value);
}

/// Adds to a counter carrying one extra dimension.
pub fn counter_add_dim(name: &str, dim_key: &str, dim_value: &str, value: u64) {
    add_to_counter(InstrumentKey::with_dim(name, dim_key, dim_value), value);
}

/// Adds to a counter carrying free-form (sanitized, capped) labels.
pub fn counter_add_labels(name: &str, labels: &[(&str, &str)], value: u64) {
    add_to_counter(InstrumentKey::with_labels(name, labels), value);
}

fn add_to_counter(key: InstrumentKey, value: u64) {
    with_registry(|reg| {
        if !admit(reg, &key) {
            return;
        }
        match reg
            .instruments
            .entry(key)
            .or_insert(InstrumentValue::Counter(0))
        {
            InstrumentValue::Counter(total) => *total = total.saturating_add(value),
            // Name reused with a different instrument kind: keep the first
            // kind, drop the write. Silently coercing would corrupt the
            // series at the backend.
            InstrumentValue::Gauge(_) | InstrumentValue::Histogram(_) => {}
        }
    });
}

/// Sets a gauge to its latest value.
pub fn gauge_set(name: &str, value: f64) {
    set_gauge(InstrumentKey::plain(name), value);
}

/// Sets a gauge carrying one extra dimension, e.g. an RSS checkpoint name.
pub fn gauge_set_dim(name: &str, dim_key: &str, dim_value: &str, value: f64) {
    set_gauge(InstrumentKey::with_dim(name, dim_key, dim_value), value);
}

/// Sets a gauge carrying free-form (sanitized, capped) labels.
pub fn gauge_set_labels(name: &str, labels: &[(&str, &str)], value: f64) {
    set_gauge(InstrumentKey::with_labels(name, labels), value);
}

fn set_gauge(key: InstrumentKey, value: f64) {
    with_registry(|reg| {
        if !admit(reg, &key) {
            return;
        }
        match reg.instruments.get_mut(&key) {
            Some(InstrumentValue::Gauge(slot)) => *slot = value,
            // Name already taken by another instrument kind: keep the first.
            Some(InstrumentValue::Counter(_) | InstrumentValue::Histogram(_)) => {}
            None => {
                reg.instruments.insert(key, InstrumentValue::Gauge(value));
            }
        }
    });
}

/// Records a histogram observation.
pub fn histogram_record(name: &str, value: f64) {
    record_in_histogram(InstrumentKey::plain(name), name, value);
}

/// Records a histogram observation on a series carrying one extra dimension.
pub fn histogram_record_dim(name: &str, dim_key: &str, dim_value: &str, value: f64) {
    record_in_histogram(
        InstrumentKey::with_dim(name, dim_key, dim_value),
        name,
        value,
    );
}

/// Records a histogram observation on a series carrying free-form labels.
pub fn histogram_record_labels(name: &str, labels: &[(&str, &str)], value: f64) {
    record_in_histogram(InstrumentKey::with_labels(name, labels), name, value);
}

fn record_in_histogram(key: InstrumentKey, name: &str, value: f64) {
    with_registry(|reg| {
        if !admit(reg, &key) {
            return;
        }
        let bounds = bounds_for(reg, name);
        match reg
            .instruments
            .entry(key)
            .or_insert_with(|| InstrumentValue::Histogram(HistogramData::new(&bounds)))
        {
            InstrumentValue::Histogram(hist) => hist.record(value),
            InstrumentValue::Counter(_) | InstrumentValue::Gauge(_) => {}
        }
    });
}

/// Takes a consistent read of every instrument.
#[must_use]
pub fn snapshot() -> MetricsSnapshot {
    let now = super::unix_nanos();
    with_registry(|reg| MetricsSnapshot {
        labels: reg.labels.clone(),
        start_unix_nanos: reg.start_unix_nanos,
        now_unix_nanos: now,
        series: reg
            .instruments
            .iter()
            .map(|(key, value)| Series {
                key: key.clone(),
                value: value.clone(),
            })
            .collect(),
        dropped_series: reg.dropped_series,
    })
    .unwrap_or_else(|| MetricsSnapshot {
        labels: MetricLabels::default(),
        start_unix_nanos: now,
        now_unix_nanos: now,
        series: Vec::new(),
        dropped_series: 0,
    })
}

/// Clears every instrument. Exposed for tests and for the tier-downgrade path,
/// where accumulated-but-unsent data must not survive an opt-out.
pub fn reset() {
    with_registry(|reg| {
        reg.instruments.clear();
        reg.dropped_series = 0;
        reg.warned_about_cardinality = false;
        reg.start_unix_nanos = super::unix_nanos();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(snapshot: &'a MetricsSnapshot, key: &InstrumentKey) -> Option<&'a InstrumentValue> {
        snapshot
            .series
            .iter()
            .find(|s| &s.key == key)
            .map(|s| &s.value)
    }

    #[test]
    fn histogram_buckets_are_cumulative_upper_bounds() {
        let mut hist = HistogramData::new(&[1.0, 2.0, 5.0]);
        for value in [0.5, 1.0, 1.5, 4.0, 100.0] {
            hist.record(value);
        }
        // <=1: 0.5 and 1.0 ; <=2: 1.5 ; <=5: 4.0 ; +Inf: 100.0
        assert_eq!(hist.counts, vec![2, 1, 1, 1]);
        assert_eq!(hist.count, 5);
        assert!((hist.sum - 107.0).abs() < 1e-9);
    }

    #[test]
    fn histogram_ignores_non_finite_values() {
        let mut hist = HistogramData::new(&[1.0]);
        hist.record(f64::NAN);
        hist.record(f64::INFINITY);
        assert_eq!(hist.count, 0);
        assert_eq!(hist.counts, vec![0, 0]);
    }

    #[test]
    fn labels_expose_exactly_four_dimensions() {
        let labels = MetricLabels::detect("1.4.2", "beta");
        let pairs = labels.pairs();
        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[0], ("version", "1.4.2"));
        assert_eq!(pairs[1], ("channel", "beta"));
        assert_eq!(pairs[2].0, "os");
        assert_eq!(pairs[3].0, "arch");
    }

    #[test]
    fn a_name_reused_with_a_different_kind_keeps_the_first_kind() {
        let mut reg = Registry::default();
        let key = InstrumentKey::plain("collide");
        reg.instruments
            .insert(key.clone(), InstrumentValue::Counter(7));
        // Simulate the histogram path against the same key.
        let bounds = bounds_for(&reg, "collide");
        match reg
            .instruments
            .entry(key.clone())
            .or_insert_with(|| InstrumentValue::Histogram(HistogramData::new(&bounds)))
        {
            InstrumentValue::Histogram(hist) => hist.record(1.0),
            InstrumentValue::Counter(_) | InstrumentValue::Gauge(_) => {}
        }
        assert_eq!(
            reg.instruments.get(&key),
            Some(&InstrumentValue::Counter(7)),
            "the counter must survive"
        );
    }

    #[test]
    fn the_cardinality_ceiling_drops_new_series_and_keeps_existing_ones() {
        let mut reg = Registry::default();
        for i in 0..MAX_SERIES {
            let key = InstrumentKey::with_dim("bounded", "dim", &i.to_string());
            assert!(admit(&mut reg, &key));
            reg.instruments.insert(key, InstrumentValue::Counter(1));
        }
        let overflow = InstrumentKey::with_dim("bounded", "dim", "one-too-many");
        assert!(!admit(&mut reg, &overflow));
        assert_eq!(reg.dropped_series, 1);
        // An already-known series still accepts writes.
        let known = InstrumentKey::with_dim("bounded", "dim", "0");
        assert!(admit(&mut reg, &known));
    }

    #[test]
    fn global_registry_accumulates_and_snapshots() {
        // The registry is process-global by design, and another test resets
        // it; hold the shared lock and use names unique to this test.
        let _guard = super::super::global_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        counter_add("test_unique_counter_total", 2);
        counter_add("test_unique_counter_total", 3);
        counter_add_dim("test_unique_dim_total", "result", "ok", 1);
        gauge_set("test_unique_gauge", 12.5);
        register_histogram("test_unique_hist", &[1.0, 10.0]);
        histogram_record("test_unique_hist", 5.0);

        let snap = snapshot();
        assert_eq!(
            find(&snap, &InstrumentKey::plain("test_unique_counter_total")),
            Some(&InstrumentValue::Counter(5))
        );
        assert_eq!(
            find(
                &snap,
                &InstrumentKey::with_dim("test_unique_dim_total", "result", "ok")
            ),
            Some(&InstrumentValue::Counter(1))
        );
        assert_eq!(
            find(&snap, &InstrumentKey::plain("test_unique_gauge")),
            Some(&InstrumentValue::Gauge(12.5))
        );
        let Some(InstrumentValue::Histogram(hist)) =
            find(&snap, &InstrumentKey::plain("test_unique_hist"))
        else {
            panic!("histogram missing from snapshot");
        };
        assert_eq!(hist.bounds, vec![1.0, 10.0]);
        assert_eq!(hist.counts, vec![0, 1, 0]);
        assert!(snap.now_unix_nanos >= snap.start_unix_nanos);
    }

    #[test]
    fn labels_sanitize_into_one_stable_series_identity() {
        // Same labels in ANY order, with hostile keys and an oversized
        // value, must collapse to the SAME sorted, sanitized key.
        let long_value = "v".repeat(200);
        let a = InstrumentKey::with_labels(
            "test_labels_total",
            &[("Doc-Type", "pdf"), ("SIZE!", &long_value)],
        );
        let b = InstrumentKey::with_labels(
            "test_labels_total",
            &[("size_", &long_value), ("doc_type", "pdf")],
        );
        // "SIZE!" sanitizes to "size_", so both spell the same two keys.
        assert_eq!(a, b, "label order / raw spelling must not fork series");
        assert_eq!(a.dims.len(), 2);
        assert_eq!(a.dims[0].0, "doc_type");
        assert_eq!(a.dims[1].0, "size_");
        assert_eq!(
            a.dims[1].1.len(),
            MAX_LABEL_VALUE_LEN,
            "value must be truncated"
        );
    }

    #[test]
    fn labels_beyond_the_cap_are_dropped_not_admitted() {
        let labels: Vec<(String, String)> = (0..10)
            .map(|i| (format!("k{i}"), format!("v{i}")))
            .collect();
        let borrowed: Vec<(&str, &str)> = labels
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let key = InstrumentKey::with_labels("test_labels_cap_total", &borrowed);
        assert_eq!(
            key.dims.len(),
            MAX_LABELS_PER_METRIC,
            "extra labels must be dropped, never silently admitted"
        );
    }

    #[test]
    fn duplicate_label_keys_last_write_wins() {
        let key = InstrumentKey::with_labels(
            "test_labels_dupe_total",
            &[("phase", "first"), ("phase", "second")],
        );
        assert_eq!(key.dims, vec![("phase".to_owned(), "second".to_owned())]);
    }
}
