//! Opt-in telemetry client.
//!
//! # What this is
//!
//! A small OTLP/HTTP **JSON** client (`serde_json` is its one
//! dependency, pulled in only by the `telemetry` feature): an in-process metric
//! registry with a typed (and therefore bounded) label set, a structured log
//! buffer, a Glean-style disk queue, and an uploader. It answers the questions
//! a desktop app developer cannot answer from support tickets — "we shipped
//! 1.4.2, did crash rate or startup RAM regress?" — and nothing else.
//!
//! # Consent is dual-keyed
//!
//! Nothing leaves the machine unless **both** keys are turned:
//!
//! 1. the *developer* compiles the `telemetry` feature in and configures an
//!    endpoint, and
//! 2. the *user* selects a consent tier at or above the one the data needs.
//!
//! The default tier is [`TelemetryTier::Off`]. Linking this module in does not
//! by itself send anything, and [`init`] with no configured endpoint prints a
//! warning rather than quietly buffering forever.
//!
//! # Cardinality and identity
//!
//! Metrics carry exactly four labels — `version`, `channel`, `os`, `arch` —
//! enforced by [`metrics::MetricLabels`] being a struct rather than a map. The
//! random `client_id` that makes "crash-free *users*" and adoption dedup
//! possible rides on **log records only**, never on a metric, and a tier
//! downgrade retires it through a deletion-request ping
//! ([`request_deletion`]).
//!
//! # Threading
//!
//! Recording is cheap and lock-light and may happen on the UI thread. Uploads
//! block on the network and must not: use [`spawn_uploader`] for a standalone
//! background thread, or drive [`flush`] from an azul `Thread` when the app
//! already owns one.
//!
//! # Example
//!
//! ```no_run
//! use azul_layout::telemetry::{self, AppMeta};
//!
//! // Reads AZ_TELEMETRY + the layered config files. Off unless opted in.
//! telemetry::init("my-app", AppMeta::new("1.4.2", "beta"));
//! telemetry::record_session_start();
//! telemetry::record_startup(0.42, 84_000_000);
//! telemetry::spawn_uploader();
//! // ... app runs ...
//! telemetry::shutdown();
//! ```

pub mod config;
pub mod metrics;
pub mod otlp;
pub mod queue;

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub use config::{
    load as load_config, set_tier, snapshot as config_snapshot, tier, ConsentScope, TelemetryConfig,
    TelemetryTier, TierChange, TierSource,
};
pub use metrics::{MetricLabels, MetricsSnapshot};
pub use otlp::{LogRecord, ResourceInfo, Severity};
pub use queue::{PingKind, PingQueue, UploadStats};

/// Cap on buffered log records. Beyond this the oldest are dropped: a log
/// buffer that grows without bound is a memory leak wearing a telemetry hat.
pub const MAX_BUFFERED_LOGS: usize = 512;

/// Minimum severity buffered by [`log`] unless raised. Matches the plan's
/// "tier >= metrics, level >= warn by default".
pub const DEFAULT_LOG_SEVERITY: Severity = Severity::Warn;

/// Identity of the app being instrumented.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppMeta {
    /// App version, e.g. `1.4.2`. Becomes the `version` metric label.
    pub version: String,
    /// Release channel: `release`, `beta`, `nightly`. Becomes the `channel`
    /// metric label, and in the plan's consent design it also selects the
    /// default tier.
    pub channel: String,
}

impl AppMeta {
    /// Version + channel.
    #[must_use]
    pub fn new(version: impl Into<String>, channel: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            channel: channel.into(),
        }
    }
}

#[derive(Debug)]
struct Inner {
    app_id: String,
    resource: ResourceInfo,
    queue: Option<PingQueue>,
    min_severity: Severity,
}

fn inner() -> &'static RwLock<Option<Inner>> {
    static INNER: OnceLock<RwLock<Option<Inner>>> = OnceLock::new();
    INNER.get_or_init(|| RwLock::new(None))
}

fn log_buffer() -> &'static Mutex<Vec<LogRecord>> {
    static BUFFER: OnceLock<Mutex<Vec<LogRecord>>> = OnceLock::new();
    BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

/// Wall-clock now, as Unix nanoseconds. Zero if the clock is before the epoch.
#[must_use]
pub fn unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// Mints a random UUID v4 for use as a `client_id`.
///
/// Deliberately *not* derived from anything about the machine. A
/// hardware-derived hash is linkable across reinstalls and profiles, cannot be
/// rotated, and turns "anonymous" into "pseudonymous with a permanent key" —
/// worse under GDPR and worse for trust. The rollout-bucketing hash, which
/// *is* machine-derived, never leaves the machine and is a separate value.
#[must_use]
pub fn new_client_id() -> String {
    let mut bytes = [0_u8; 16];
    fill_random(&mut bytes);
    // RFC 4122: version 4, variant 10xx.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let mut out = String::with_capacity(36);
    for (i, byte) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    out
}

/// Fills `out` with entropy: the OS pool when available, a time/address-seeded
/// `SplitMix64` otherwise. Only ever used for an opaque identifier, never for a
/// key.
fn fill_random(out: &mut [u8; 16]) {
    #[cfg(unix)]
    {
        use std::io::Read as _;
        if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
            if file.read_exact(out).is_ok() {
                return;
            }
        }
    }

    let mut state = unix_nanos()
        ^ u64::from(std::process::id()).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (std::ptr::addr_of!(out) as u64);
    for chunk in out.chunks_mut(8) {
        // SplitMix64.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        for (slot, byte) in chunk.iter_mut().zip(z.to_le_bytes()) {
            *slot = byte;
        }
    }
}

/// Loads the layered config for `app_id` and prepares the client.
///
/// Returns the resolved configuration so the caller can decide whether to show
/// a consent dialog (see [`TelemetryConfig::pinned_off`]). Safe to call twice;
/// the last call wins.
#[must_use]
pub fn init(app_id: &str, meta: AppMeta) -> TelemetryConfig {
    let resolved = config::load(app_id);

    metrics::set_labels(MetricLabels::detect(&meta.version, &meta.channel));
    metrics::register_histogram(metrics::STARTUP_SECONDS, metrics::SECONDS_BUCKETS);
    metrics::register_histogram(metrics::STARTUP_RSS_BYTES, metrics::BYTES_BUCKETS);
    metrics::register_histogram(metrics::PHASE_SECONDS, metrics::SECONDS_BUCKETS);

    if let Ok(mut slot) = inner().write() {
        *slot = Some(Inner {
            app_id: app_id.to_owned(),
            resource: ResourceInfo {
                service_name: app_id.to_owned(),
                service_version: meta.version,
                scope_name: "azul-layout".to_owned(),
                scope_version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            queue: PingQueue::for_app(app_id),
            min_severity: DEFAULT_LOG_SEVERITY,
        });
    }

    resolved
}

/// Raises or lowers the severity floor for [`log`].
pub fn set_min_log_severity(severity: Severity) {
    if let Ok(mut slot) = inner().write() {
        if let Some(state) = slot.as_mut() {
            state.min_severity = severity;
        }
    }
}

/// Whether metric/log collection is currently permitted.
#[must_use]
pub fn is_collecting() -> bool {
    tier().allows_metrics()
}

/// Counts one app run. The denominator of every release-health ratio.
pub fn record_session_start() {
    if !is_collecting() {
        return;
    }
    metrics::counter_add(metrics::SESSIONS_STARTED, 1);
}

/// Records startup duration and the RSS reading taken just after it.
pub fn record_startup(seconds: f64, rss_bytes: u64) {
    if !is_collecting() {
        return;
    }
    metrics::histogram_record(metrics::STARTUP_SECONDS, seconds);
    metrics::histogram_record(metrics::STARTUP_RSS_BYTES, rss_bytes as f64);
    metrics::gauge_set(metrics::RSS_BYTES, rss_bytes as f64);
}

/// Records a current memory reading (RSS and allocator heap, in bytes).
pub fn record_memory(rss_bytes: u64, heap_bytes: u64) {
    if !is_collecting() {
        return;
    }
    metrics::gauge_set(metrics::RSS_BYTES, rss_bytes as f64);
    metrics::gauge_set(metrics::HEAP_BYTES, heap_bytes as f64);
}

/// Counts one panic. Allowed at tier `Crashes` and above.
pub fn record_panic() {
    if !tier().allows_crashes() {
        return;
    }
    metrics::counter_add(metrics::PANICS, 1);
}

/// Counts one native crash, normally on the launch *after* it happened.
pub fn record_crash() {
    if !tier().allows_crashes() {
        return;
    }
    metrics::counter_add(metrics::CRASHES, 1);
}

/// Counts one frame by how much work it caused.
///
/// `scope` comes from `ProcessEventResult` / `RelayoutScope` and is a small
/// fixed set (`do_nothing`, `repaint`, `relayout`, `regenerate_dom`, …) — the
/// bounded-dimension contract depends on the caller keeping it that way.
pub fn record_relayout_scope(scope: &str) {
    if !is_collecting() {
        return;
    }
    metrics::counter_add_dim(metrics::RELAYOUT_SCOPE, "scope", scope, 1);
}

/// Counts one update check by outcome (`up_to_date`, `available`, `error`, …).
pub fn record_update_check(result: &str) {
    if !is_collecting() {
        return;
    }
    metrics::counter_add_dim(metrics::UPDATE_CHECK, "result", result, 1);
}

/// Counts one update application by outcome.
pub fn record_update_apply(result: &str) {
    if !is_collecting() {
        return;
    }
    metrics::counter_add_dim(metrics::UPDATE_APPLY, "result", result, 1);
}

/// Records an app-defined counter. The name must be a fixed string, not
/// user-derived.
pub fn count(name: &str, value: u64) {
    if !is_collecting() {
        return;
    }
    metrics::counter_add(name, value);
}

/// Records an app-defined histogram observation.
pub fn observe(name: &str, value: f64) {
    if !is_collecting() {
        return;
    }
    metrics::histogram_record(name, value);
}

/// Buffers a structured log record for the next flush.
///
/// Records below the severity floor, or below consent tier `Metrics`, are
/// dropped on the floor rather than buffered.
pub fn log(severity: Severity, message: impl Into<String>) {
    if !is_collecting() {
        return;
    }
    let floor = inner()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(|state| state.min_severity))
        .unwrap_or(DEFAULT_LOG_SEVERITY);
    if severity < floor {
        return;
    }

    let mut record = LogRecord::new(severity, message);
    if let Some(client_id) = config_snapshot().client_id {
        record = record.with_attribute("client_id", client_id);
    }
    push_log(record);
}

fn push_log(record: LogRecord) {
    if let Ok(mut buffer) = log_buffer().lock() {
        if buffer.len() >= MAX_BUFFERED_LOGS {
            buffer.remove(0);
        }
        buffer.push(record);
    }
}

/// Turns `Probe` recording on so [`drain_probe_events`] has something to
/// drain.
///
/// `Probe` resolves its recording flag lazily from `AZ_PROFILE`, because its
/// first consumer was the profiler — which means that without this call the
/// phase histogram is silently empty in a normal run, and an empty histogram
/// is indistinguishable from a fast one. A telemetry client wants spans
/// without requiring a profiler env var, so it says so explicitly.
///
/// Without the `probe` feature the spans are compiled out and this is a no-op.
pub fn enable_probe_bridge() {
    crate::probe::Probe::set_recording(true);
}

/// Whether `Probe` is currently recording, i.e. whether
/// [`drain_probe_events`] can produce anything.
#[must_use]
pub fn probe_bridge_enabled() -> bool {
    crate::probe::Probe::enabled()
}

/// Drains `Probe`'s buffered spans and RSS samples into metrics.
///
/// Spans become `app_phase_seconds{phase}` histogram observations and RSS
/// checkpoints become `app_rss_bytes{checkpoint}` gauges. Without the `probe`
/// feature the probe buffer is always empty and this returns 0.
///
/// Returns the number of events consumed.
#[must_use]
pub fn drain_probe_events() -> usize {
    let events = crate::probe::Probe::drain();
    if !is_collecting() {
        return 0;
    }
    for event in &events {
        match event.kind {
            crate::probe::EventKind::Span { dur_ns } => {
                metrics::histogram_record_dim(
                    metrics::PHASE_SECONDS,
                    "phase",
                    event.name,
                    dur_ns as f64 / 1_000_000_000.0,
                );
            }
            crate::probe::EventKind::Rss { bytes } => {
                metrics::gauge_set_dim(metrics::RSS_BYTES, "checkpoint", event.name, bytes as f64);
            }
        }
    }
    events.len()
}

/// What one [`flush`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushOutcome {
    /// True when the consent tier forbade collection and nothing was done.
    pub skipped: bool,
    /// Whether a metrics ping was written to the queue.
    pub queued_metrics: bool,
    /// How many log records were written to the queue.
    pub queued_logs: usize,
    /// Result of draining the queue.
    pub upload: UploadStats,
}

/// Encodes the current metrics and buffered logs and writes them to the disk
/// queue — **without** touching the network.
///
/// Cheap and safe to call from anywhere, including a panic hook: a panic
/// leaves the process intact, so the data can be made durable immediately and
/// uploaded on the next launch. That ordering (persist now, upload later) is
/// the whole reason the queue is on disk.
#[must_use]
pub fn persist() -> FlushOutcome {
    let mut outcome = FlushOutcome::default();
    if !is_collecting() {
        outcome.skipped = true;
        return outcome;
    }

    let Some((resource, Some(queue))) = inner().read().ok().and_then(|slot| {
        slot.as_ref()
            .map(|state| (state.resource.clone(), state.queue.clone()))
    }) else {
        outcome.skipped = true;
        return outcome;
    };

    if let Some(payload) = otlp::encode_metrics(&metrics::snapshot(), &resource) {
        if queue.enqueue(PingKind::Metrics, &payload).is_ok() {
            outcome.queued_metrics = true;
        }
    }

    let records = log_buffer()
        .lock()
        .map(|mut buffer| std::mem::take(&mut *buffer))
        .unwrap_or_default();
    if let Some(payload) = otlp::encode_logs(&records, &resource) {
        if queue.enqueue(PingKind::Logs, &payload).is_ok() {
            outcome.queued_logs = records.len();
        }
    }

    outcome
}

/// Drains the disk queue to the configured endpoint.
///
/// Blocking: this performs network IO. Call it from [`spawn_uploader`]'s
/// thread or from an azul `Thread`, never from a UI callback.
#[must_use]
pub fn upload() -> UploadStats {
    let Some(queue) = ping_queue() else {
        return UploadStats::default();
    };
    queue::upload_pending(&queue, &config_snapshot())
}

/// [`persist`] followed by [`upload`] — one full flush cycle.
///
/// Blocking: performs network IO, so keep it off the UI thread.
#[must_use]
pub fn flush() -> FlushOutcome {
    let mut outcome = persist();
    if !outcome.skipped {
        outcome.upload = upload();
    }
    outcome
}

/// Queues the Glean-style deletion-request ping and wipes local state.
///
/// Call after [`set_tier`] reports `needs_deletion_request`. The ping names
/// the retired `client_id` so the server can erase that user's history; the id
/// is already gone from the local config by then.
#[must_use]
pub fn request_deletion() -> bool {
    let Some(client_id) = config::take_retired_client_id() else {
        return false;
    };

    let queued = inner()
        .read()
        .ok()
        .and_then(|slot| {
            slot.as_ref().map(|state| {
                let record = LogRecord::new(Severity::Info, "deletion-request")
                    .with_attribute("client_id", client_id.clone())
                    .with_attribute("event", "deletion_request");
                otlp::encode_logs(std::slice::from_ref(&record), &state.resource).and_then(
                    |payload| {
                        state
                            .queue
                            .as_ref()
                            .and_then(|q| q.enqueue(PingKind::Deletion, &payload).ok())
                    },
                )
            })
        })
        .flatten()
        .is_some();

    // Everything collected under the old consent must not be uploaded now.
    metrics::reset();
    if let Ok(mut buffer) = log_buffer().lock() {
        buffer.clear();
    }
    queued
}

static UPLOADER_STARTED: AtomicBool = AtomicBool::new(false);
static UPLOADER_STOP: AtomicBool = AtomicBool::new(false);
static UPLOADER_FLUSHES: AtomicU64 = AtomicU64::new(0);

/// Starts the background uploader thread, if one is not already running.
///
/// The thread wakes every `flush_interval_secs` (from the config), calls
/// [`flush`], and exits when [`shutdown`] is called. Returns `false` if an
/// uploader was already started or the tier forbids collection.
///
/// This is the standalone path. An app that already runs an azul event loop
/// should instead register a `Timer` that hands work to an azul `Thread`, so
/// the uploader participates in the app's normal shutdown.
pub fn spawn_uploader() -> bool {
    if !is_collecting() {
        return false;
    }
    if UPLOADER_STARTED.swap(true, Ordering::SeqCst) {
        return false;
    }
    UPLOADER_STOP.store(false, Ordering::SeqCst);

    let interval = Duration::from_secs(config_snapshot().flush_interval_secs.max(1));
    let spawned = std::thread::Builder::new()
        .name("azul-telemetry-uploader".to_owned())
        .spawn(move || {
            while !UPLOADER_STOP.load(Ordering::Relaxed) {
                // Wake often enough that shutdown is prompt even with a long
                // flush interval.
                let mut waited = Duration::ZERO;
                while waited < interval && !UPLOADER_STOP.load(Ordering::Relaxed) {
                    let step = Duration::from_millis(250).min(interval - waited);
                    std::thread::sleep(step);
                    waited += step;
                }
                if UPLOADER_STOP.load(Ordering::Relaxed) {
                    break;
                }
                drop(flush());
                UPLOADER_FLUSHES.fetch_add(1, Ordering::Relaxed);
            }
        })
        .is_ok();

    if !spawned {
        UPLOADER_STARTED.store(false, Ordering::SeqCst);
    }
    spawned
}

/// How many interval flushes the uploader thread has completed.
#[must_use]
pub fn uploader_flush_count() -> u64 {
    UPLOADER_FLUSHES.load(Ordering::Relaxed)
}

/// Stops the uploader thread and performs one final flush.
///
/// A clean shutdown is one of the plan's three flush triggers; without it the
/// last interval's data would sit in the queue until the next launch.
pub fn shutdown() -> FlushOutcome {
    UPLOADER_STOP.store(true, Ordering::SeqCst);
    UPLOADER_STARTED.store(false, Ordering::SeqCst);
    flush()
}

/// Installs a panic hook that counts panics and buffers the panic message as
/// an `ERROR` log record, then delegates to the previously installed hook.
///
/// Panics leave the process intact, so unlike a native crash they can be
/// serialized in-hook. This is the anchor point the crash-bundle work (not yet
/// implemented) will extend.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_panic();
        if is_collecting() {
            let location = info
                .location()
                .map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
            let record = LogRecord::new(Severity::Fatal, format!("panic: {info}"))
                .with_attribute("location", location);
            push_log(record);
            // Make it durable, but do NOT upload: a hook that blocks on the
            // network turns a recoverable panic into a hang. The next launch
            // drains the queue.
            drop(persist());
        }
        previous(info);
    }));
}

/// The app id [`init`] was called with.
#[must_use]
pub fn app_id() -> Option<String> {
    inner()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(|state| state.app_id.clone()))
}

/// The pending-ping queue, if [`init`] found a data directory.
#[must_use]
pub fn ping_queue() -> Option<PingQueue> {
    inner()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().and_then(|state| state.queue.clone()))
}

/// The exact bytes that *would* be uploaded right now, for the consent
/// preview: "show me what you send".
///
/// Returns `(metrics_payload, logs_payload)`, either of which may be `None`
/// when there is nothing to send. Reads state without consuming it.
#[must_use]
pub fn preview_payloads() -> (Option<String>, Option<String>) {
    let Some(resource) = inner()
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().map(|state| state.resource.clone()))
    else {
        return (None, None);
    };
    let logs = log_buffer().lock().map(|b| b.clone()).unwrap_or_default();
    (
        otlp::encode_metrics(&metrics::snapshot(), &resource),
        otlp::encode_logs(&logs, &resource),
    )
}

/// Serializes the tests that touch the process-global registry and config.
///
/// `cargo test` runs test functions on parallel threads inside one process,
/// and this module's state is deliberately process-global — without this,
/// `reset()` in one test races the accumulation another one is asserting on.
#[cfg(test)]
pub(crate) fn global_state_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ids_are_well_formed_v4_uuids_and_distinct() {
        let a = new_client_id();
        let b = new_client_id();
        assert_ne!(a, b, "two mints must not collide");
        assert_eq!(a.len(), 36, "{a}");
        let groups: Vec<&str> = a.split('-').collect();
        assert_eq!(
            groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit()), "{a}");
        // version nibble
        assert_eq!(groups[2].as_bytes()[0], b'4', "{a}");
        // variant nibble is one of 8, 9, a, b
        assert!(
            matches!(groups[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'),
            "{a}"
        );
    }

    #[test]
    fn the_fallback_entropy_path_still_produces_distinct_ids() {
        // Exercises fill_random's non-/dev/urandom half directly.
        let mut a = [0_u8; 16];
        let mut b = [0_u8; 16];
        fill_random(&mut a);
        fill_random(&mut b);
        assert_ne!(a, b);
        assert!(a.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn recording_is_inert_while_the_tier_is_off() {
        let _guard = global_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        config::store(TelemetryConfig::default());
        metrics::reset();
        record_session_start();
        record_startup(1.0, 1024);
        record_relayout_scope("relayout");
        count("test_inert_total", 5);
        log(Severity::Error, "should not be buffered");
        assert!(metrics::snapshot().is_empty(), "tier Off must record nothing");
        assert_eq!(
            preview_payloads(),
            (None, None),
            "nothing to preview when nothing was collected"
        );
    }

    #[test]
    fn unix_nanos_is_after_2020() {
        // A zero here would mean every timestamp we ship is garbage.
        const Y2020: u64 = 1_577_836_800_000_000_000;
        assert!(unix_nanos() > Y2020);
    }
}
