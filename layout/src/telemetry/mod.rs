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
#[cfg(feature = "crash-mail")]
pub mod crash_mail;
pub mod metrics;
pub mod otlp;
pub mod queue;
pub mod sharedconfig;
pub mod sysinfo;

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub use config::{
    load as load_config, set_tier, snapshot as config_snapshot, tier, ConsentScope,
    TelemetryConfig, TelemetryTier, TierChange, TierSource,
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

    // Consent ARMS the probe: a tier that ships metrics wants
    // `app_phase_seconds{phase}` filled, and requiring a second, unrelated
    // switch (`AZ_PROFILE`, the agent/e2e-local debugging tool) for that
    // made the phase histogram silently empty on every consenting run - and
    // an empty histogram is indistinguishable from a fast one. `AZ_PROFILE`
    // remains orthogonal: it arms the same recorder for LOCAL consumers
    // (e2e cross-frame dumps) with no telemetry involved.
    if resolved.tier.allows_metrics() {
        enable_probe_bridge();
    }

    metrics::set_labels(MetricLabels::detect(&meta.version, &meta.channel));
    metrics::register_histogram(metrics::STARTUP_SECONDS, metrics::SECONDS_BUCKETS);
    metrics::register_histogram(metrics::STARTUP_RSS_BYTES, metrics::BYTES_BUCKETS);
    metrics::register_histogram(metrics::PHASE_SECONDS, metrics::MICRO_SECONDS_BUCKETS);

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
    tier().allows_metrics() && config::metrics_enabled()
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

/// Slow-frame threshold in milliseconds (f64 bits). Default 32 ms — two
/// missed frames at 60 Hz, the point where an animation visibly hitches.
static SLOW_FRAME_THRESHOLD_MS: AtomicU64 = AtomicU64::new(0);
/// Whether the one-shot system-info attachment already went out this session.
static SYSINFO_SENT: AtomicBool = AtomicBool::new(false);
/// App-supplied document size (f64 bits; unit is the app's own semantics).
static DOCUMENT_SIZE: AtomicU64 = AtomicU64::new(0);

const DEFAULT_SLOW_FRAME_MS: f64 = 32.0;

/// Sets the slow-frame threshold in milliseconds (default 32 ms). Frames and
/// probe spans at or above it count into `app_slow_frames_total` and produce
/// a WARN log; the FIRST slow event of a session additionally carries the
/// full [`sysinfo`] snapshot — hardware context ships only when something is
/// actually slow.
pub fn set_slow_frame_threshold_ms(ms: f64) {
    SLOW_FRAME_THRESHOLD_MS.store(ms.max(0.0).to_bits(), Ordering::Relaxed);
}

fn slow_frame_threshold_ms() -> f64 {
    let bits = SLOW_FRAME_THRESHOLD_MS.load(Ordering::Relaxed);
    if bits == 0 {
        DEFAULT_SLOW_FRAME_MS
    } else {
        f64::from_bits(bits)
    }
}

/// Supplies the GPU/renderer description (e.g. `GL_RENDERER`). The app owns
/// the GL context and always knows better than the `/sys` guess; first call
/// wins.
pub fn set_gpu_info(renderer: impl Into<String>) {
    drop(sysinfo::GPU_INFO.set(renderer.into()));
}

/// Records the app-defined DOCUMENT SIZE — nodes, paragraphs, bytes,
/// whatever the app's unit is. This is the correlator that turns "RSS went
/// up" into "RSS went up because the user opened a huge document".
pub fn set_document_size(size: f64) {
    DOCUMENT_SIZE.store(size.to_bits(), Ordering::Relaxed);
    if is_collecting() {
        metrics::gauge_set(metrics::DOCUMENT_SIZE, size);
    }
}

/// The last value passed to [`set_document_size`] (0.0 before the first).
#[must_use]
pub fn document_size() -> f64 {
    f64::from_bits(DOCUMENT_SIZE.load(Ordering::Relaxed))
}

/// RSS captured by [`record_document_open_begin`], consumed by
/// [`record_document_opened`].
#[derive(Debug, Clone, Copy)]
pub struct DocOpenToken {
    rss_before: u64,
}

/// Snapshot RSS BEFORE opening a document. Also records the
/// `checkpoint="before_document"` RSS gauge.
#[must_use]
pub fn record_document_open_begin() -> DocOpenToken {
    let (rss, _) = crate::probe::current_rss_bytes();
    if is_collecting() {
        metrics::gauge_set_dim(
            metrics::RSS_BYTES,
            "checkpoint",
            "before_document",
            rss as f64,
        );
    }
    DocOpenToken { rss_before: rss }
}

/// The document finished opening: records `checkpoint="after_document"` RSS,
/// the document size, the RSS DELTA the open cost, and the delta divided by
/// document size (bytes of resident growth per document unit — flat across
/// sizes = healthy, a per-version jump = a leak document size does not
/// explain).
pub fn record_document_opened(token: DocOpenToken, doc_size: f64) {
    set_document_size(doc_size);
    let (rss_after, _) = crate::probe::current_rss_bytes();
    if !is_collecting() {
        return;
    }
    metrics::gauge_set_dim(
        metrics::RSS_BYTES,
        "checkpoint",
        "after_document",
        rss_after as f64,
    );
    let delta = rss_after.saturating_sub(token.rss_before) as f64;
    metrics::gauge_set(metrics::DOC_RSS_DELTA_BYTES, delta);
    if doc_size > 0.0 {
        metrics::gauge_set(metrics::DOC_RSS_PER_UNIT, delta / doc_size);
    }
}

/// Records one frame's wall-clock duration under `scope` (`layout`,
/// `render`, `total` — a small fixed set). Slow frames additionally count
/// into `app_slow_frames_total` and produce a WARN log; see
/// [`set_slow_frame_threshold_ms`].
/// RAII frame pump for ENGINE render paths: measures from construction to
/// drop, then records `app_frame_seconds{scope}` and drains the probe
/// buffer into `app_phase_seconds{phase}` — so every solver/raster span of
/// the frame lands in the same scrape. Placed at the top of `render_frame`
/// / `regenerate_layout`, the Drop covers EVERY return path (early-outs,
/// cache hits, errors). All sinks are tier-guarded; at tier off the probe
/// buffer is still drained so it stays bounded.
#[derive(Debug)]
pub struct FramePump {
    scope: &'static str,
    // azul_core's Instant, NOT std's: `std::time::Instant::now()` PANICS on
    // wasm32-unknown-unknown, and `feature = "std"` does not exclude wasm
    // here (the web target builds azul-core with default features). The core
    // Instant answers browser frames as ticks instead of trapping.
    start: azul_core::task::Instant,
}

impl FramePump {
    /// Starts the frame clock for `scope` ("layout", "present", …).
    #[must_use]
    pub fn begin(scope: &'static str) -> Self {
        Self {
            scope,
            start: azul_core::task::Instant::now(),
        }
    }
}

impl Drop for FramePump {
    fn drop(&mut self) {
        // `as_nanos` is the common denominator: on wasm the elapsed value is
        // a TICK count that converts through the same accessor, so a frame
        // measured in browser frames still lands in the seconds histogram.
        #[allow(clippy::cast_precision_loss)]
        let seconds = azul_core::task::Instant::now()
            .duration_since(&self.start)
            .as_nanos() as f64
            / 1e9;
        record_frame(self.scope, seconds);
        let _ = drain_probe_events();
    }
}

pub fn record_frame(scope: &str, seconds: f64) {
    if !is_collecting() {
        return;
    }
    metrics::histogram_record_dim(metrics::FRAME_SECONDS, "scope", scope, seconds);
    note_if_slow(scope, seconds);
}

/// Records one TIMER tick's duration — the clock animations ride, so slow
/// ticks here are what make animations stutter. Slow ticks warn like slow
/// frames, under scope `timer`.
pub fn record_timer_frame(seconds: f64) {
    if !is_collecting() {
        return;
    }
    metrics::histogram_record(metrics::TIMER_FRAME_SECONDS, seconds);
    note_if_slow("timer", seconds);
}

/// Shared slow-event path for frames, timer ticks and probe spans: count it,
/// WARN with the exact name + duration + current document size, and attach
/// the one-shot system-info snapshot if this is the session's first slow
/// event.
fn note_if_slow(what: &str, seconds: f64) {
    let ms = seconds * 1_000.0;
    if ms < slow_frame_threshold_ms() {
        return;
    }
    metrics::counter_add_dim(metrics::SLOW_FRAMES, "scope", what, 1);
    let mut record = LogRecord::new(
        Severity::Warn,
        format!(
            "slow {what}: {ms:.1} ms (threshold {:.1} ms)",
            slow_frame_threshold_ms()
        ),
    )
    .with_attribute("event.kind", "slow_frame")
    .with_attribute("slow.scope", what.to_owned())
    .with_attribute("slow.ms", format!("{ms:.2}"))
    .with_attribute("app.document_size", format!("{}", document_size()));
    if let Some(client_id) = config_snapshot().client_id {
        record = record.with_attribute("client_id", client_id);
    }
    if !SYSINFO_SENT.swap(true, Ordering::Relaxed) {
        for (k, v) in sysinfo::get().as_attributes() {
            record = record.with_attribute(k, v);
        }
    }
    push_log(record);
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

/// Records an app-defined counter with free-form labels. Labels are
/// sanitized and capped (6 keys, 64-char values) and every distinct
/// combination counts against the global series ceiling — see
/// `metrics::InstrumentKey::with_labels`.
pub fn count_with(name: &str, value: u64, labels: &[(&str, &str)]) {
    if !is_collecting() {
        return;
    }
    metrics::counter_add_labels(name, labels, value);
}

/// Records an app-defined histogram observation with free-form labels
/// (same sanitization and caps as [`count_with`]).
pub fn observe_with(name: &str, value: f64, labels: &[(&str, &str)]) {
    if !is_collecting() {
        return;
    }
    metrics::histogram_record_labels(name, labels, value);
}

/// Sets an app-defined gauge with free-form labels (same sanitization and
/// caps as [`count_with`]).
pub fn gauge_with(name: &str, value: f64, labels: &[(&str, &str)]) {
    if !is_collecting() {
        return;
    }
    metrics::gauge_set_labels(name, labels, value);
}

/// Log with the running e2e scenario attached as an ATTRIBUTE, not just inside
/// the message text.
///
/// An attribute is what Grafana can filter and group on. Putting the test name
/// only in the body means reading it back out with a regex in every query,
/// which is exactly the difference between "the logs are in there somewhere"
/// and "show me this test's story".
fn emit_tagged(severity: Severity, message: &str) {
    match azul_core::diagnostics::current_scope() {
        Some(test) => log_with_attributes(severity, message, &[("test", test.as_str())]),
        None => log(severity, message.to_string()),
    }
}

/// An e2e assertion FAILED. Reported at Error severity so it stands out in
/// Grafana the way a crash does, and carries the scenario + step as attributes
/// so a failure can be traced back to the exact step that produced it.
///
/// Scenarios run serially, so the resulting stream reads in order: everything
/// one test emitted, then its verdict.
pub fn report_e2e_failure(scenario: &str, step: &str, detail: &str) {
    log_with_attributes(
        Severity::Error,
        &format!("e2e assertion failed: {detail}"),
        &[("test", scenario), ("step", step), ("kind", "e2e_failure")],
    );
}

/// An e2e scenario finished. `passed` distinguishes the two outcomes without
/// needing to parse the message.
pub fn report_e2e_result(scenario: &str, passed: bool, steps: usize) {
    log_with_attributes(
        if passed {
            Severity::Info
        } else {
            Severity::Error
        },
        &format!(
            "e2e scenario {} after {steps} step(s)",
            if passed { "passed" } else { "FAILED" }
        ),
        &[
            ("test", scenario),
            ("kind", "e2e_result"),
            ("passed", if passed { "true" } else { "false" }),
        ],
    );
}

/// `log`, plus attributes.
fn log_with_attributes(severity: Severity, message: &str, attrs: &[(&str, &str)]) {
    if !tier().allows_metrics() || !config::logs_enabled() {
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
    let mut record = LogRecord::new(severity, message.to_string());
    for (k, v) in attrs {
        record = record.with_attribute(*k, (*v).to_string());
    }
    if let Some(client_id) = config_snapshot().client_id {
        record = record.with_attribute("client_id", client_id);
    }
    push_log(record);
}

/// Send every framework DIAGNOSTIC through telemetry, so engine lints land in
/// the same place as everything else — and, in QA or production, in Loki behind
/// Grafana rather than on a stderr nobody reads.
///
/// azul-core cannot call this directly: it does not know about telemetry, and
/// must not. It exposes an installable sink instead
/// (`azul_core::diagnostics::set_sink`), and this is what an application
/// installs into it. One call at startup and every existing lint — image-churn,
/// text-without-block, whatever comes next — is routed, because they all go
/// through `diagnostics::emit`.
///
/// Diagnostics are warnings by definition: the engine only emits one when
/// something the app built will misbehave.
pub fn install_diagnostics_bridge() {
    azul_core::diagnostics::set_sink(|message| {
        // Keep the developer-visible behaviour: a warning on stderr is what
        // someone running the app locally expects to see. Telemetry is
        // ADDITIONAL, and silently does nothing when it is not configured.
        eprintln!("{message}");
        emit_tagged(Severity::Warn, message);
    });
}

/// Buffers a structured log record for the next flush.
///
/// Records below the severity floor, or below consent tier `Metrics`, are
/// dropped on the floor rather than buffered.
pub fn log(severity: Severity, message: impl Into<String>) {
    // Logs have their OWN signal gate: the shared config can turn logs off
    // while metrics keep flowing (and vice versa - is_collecting() gates
    // the metric paths with metrics_enabled()).
    if !tier().allows_metrics() || !config::logs_enabled() {
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
    // At most this many slow-SPAN warnings per drain: one slow frame can
    // contain a dozen slow nested spans, and the OUTERMOST ones carry the
    // diagnosis. The metric still counts every one.
    let mut slow_logs_left = 5usize;
    for event in &events {
        match event.kind {
            crate::probe::EventKind::Span { dur_ns } => {
                let seconds = dur_ns as f64 / 1_000_000_000.0;
                metrics::histogram_record_dim(metrics::PHASE_SECONDS, "phase", event.name, seconds);
                // WHICH span was slow, by name — the per-phase histogram
                // says "something in text_shape is slow at p95", this log
                // says "span text_shape took 41.3 ms in THIS session".
                let ms = seconds * 1_000.0;
                if ms >= slow_frame_threshold_ms() && slow_logs_left > 0 {
                    slow_logs_left -= 1;
                    metrics::counter_add_dim(metrics::SLOW_FRAMES, "scope", event.name, 1);
                    let mut record = LogRecord::new(
                        Severity::Warn,
                        format!("slow span {}: {ms:.1} ms", event.name),
                    )
                    .with_attribute("event.kind", "slow_span")
                    .with_attribute("slow.span", event.name.to_owned())
                    .with_attribute("slow.ms", format!("{ms:.2}"))
                    .with_attribute("app.document_size", format!("{}", document_size()));
                    if let Some(client_id) = config_snapshot().client_id {
                        record = record.with_attribute("client_id", client_id);
                    }
                    if !SYSINFO_SENT.swap(true, Ordering::Relaxed) {
                        for (k, v) in sysinfo::get().as_attributes() {
                            record = record.with_attribute(k, v);
                        }
                    }
                    push_log(record);
                }
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
    // Sample the RSS gauge on every flush cycle so `app_rss_bytes` exists for
    // EVERY app, not only those that call the startup/document-open hooks -
    // an engine-only run (any demo) used to leave the dashboard's RSS panels
    // permanently at "No data". `current_rss_bytes` is a syscall read; at
    // tier Off `is_collecting` gates the write like every other sink.
    if is_collecting() {
        let (rss, _) = crate::probe::current_rss_bytes();
        if rss > 0 {
            metrics::gauge_set(metrics::RSS_BYTES, rss as f64);
        }
    }
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

/// The env var carrying the crash-dump path into the REINVOKED reporter
/// process. `AzApp::run` (and this demo) check it at startup: when set, the
/// process is not a normal launch — it parses the dump, shows it to the
/// user (CPU rendering only; the crash may well be the GPU path) and offers
/// manual submission.
pub const CRASH_DUMP_ENV: &str = "AZ_CRASH_DUMP";

/// Whether the app registered a crash CONTACT (a mailbox) — the marker that
/// the reinvoke-reporter flow is wanted at all.
static CRASH_CONTACT_SET: AtomicBool = AtomicBool::new(false);

/// Marks that a crash contact exists (set by
/// `crash_mail::set_crash_contact`); the panic hook only spawns the
/// reporter process when this is true AND no OTLP endpoint is configured —
/// with an endpoint the pipeline is automatic and no dialog is owed.
pub(crate) fn mark_crash_contact(set: bool) {
    CRASH_CONTACT_SET.store(set, Ordering::Relaxed);
}

/// A parsed crash dump, for the reporter process.
#[derive(Debug, Clone)]
pub struct CrashDump {
    /// Where the dump file lives (the reporter deletes it after submission).
    pub path: std::path::PathBuf,
    /// The raw JSON, verbatim — this is what gets attached/submitted.
    pub raw: String,
    /// Panic message (the `expect` reason).
    pub message: String,
    /// `file:line`, paths stripped.
    pub location: String,
    /// The live probe-span scope at crash time.
    pub scope: String,
    /// Path-stripped backtrace.
    pub backtrace: String,
    /// The action journal at crash time, as JSON (`[]` when the app never
    /// armed it) — the reporter shows it as "recent actions".
    pub recent_actions: String,
}

impl CrashDump {
    /// Loads a dump written by the panic hook.
    ///
    /// # Errors
    ///
    /// Returns the IO/parse error as text.
    pub fn load(path: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let get = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        Ok(Self {
            path,
            message: get("message"),
            location: get("location"),
            scope: get("scope"),
            backtrace: get("backtrace"),
            // The journal is an ARRAY, not a string — render it back to JSON
            // text for display/attachment.
            recent_actions: v
                .get("recent_actions")
                .map_or_else(|| "[]".to_owned(), serde_json::Value::to_string),
            raw,
        })
    }
}

/// The crash dump handed to THIS process via [`CRASH_DUMP_ENV`], if any —
/// the first thing `AzApp::run` (or an app's own main) should check: `Some`
/// means "you are the crash reporter, not the app".
#[must_use]
pub fn crash_dump_from_env() -> Option<CrashDump> {
    let path = std::env::var_os(CRASH_DUMP_ENV)?;
    CrashDump::load(std::path::PathBuf::from(path)).ok()
}

/// Strips user paths from a backtrace/location string: `$HOME` becomes `~`,
/// `/rustc/<hash>/` and registry paths collapse, and any remaining absolute
/// path keeps only its last three components. The FRAMES stay (that is the
/// diagnostic); the user's directory layout does not travel.
#[must_use]
pub fn strip_user_paths(text: &str) -> String {
    let home = std::env::var("HOME").ok().filter(|h| h.len() > 1);
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.lines().enumerate() {
        if i != 0 {
            out.push('\n');
        }
        let mut line = line.to_owned();
        if let Some(home) = &home {
            line = line.replace(home.as_str(), "~");
        }
        // /rustc/<40-hex>/library/... -> rust:library/...
        if let Some(idx) = line.find("/rustc/") {
            let rest = &line[idx + "/rustc/".len()..];
            if let Some(slash) = rest.find('/') {
                let tail = rest[slash + 1..].to_owned();
                line = format!("{}rust:{}", &line[..idx], tail);
            }
        }
        // Long absolute paths keep their 3 last components.
        while let Some(idx) = line.find(" /") {
            let replacement = {
                let (head, path) = line.split_at(idx + 1);
                let end = path
                    .find(|c: char| c.is_whitespace() || c == ':')
                    .unwrap_or(path.len());
                let (path, tail) = path.split_at(end);
                let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
                if parts.len() <= 3 {
                    None
                } else {
                    Some(format!(
                        "{head}…/{}{tail}",
                        parts[parts.len() - 3..].join("/")
                    ))
                }
            };
            match replacement {
                Some(next) => line = next,
                None => break,
            }
        }
        out.push_str(&line);
    }
    out
}

/// Extracts the panic PAYLOAD (the `panic!`/`expect` message) as text.
fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

/// Installs a panic hook that captures EVERYTHING knowable at crash time and
/// makes it durable, then delegates to the previously installed hook.
///
/// Captured per crash: the panic message (`expect` reason), `file:line`
/// location, a path-stripped backtrace (frames stay, `$HOME` and toolchain
/// prefixes do not), the live [`crate::probe`] span scope ("what was the app
/// doing"), the app-supplied document size, and the [`sysinfo`] snapshot.
///
/// Two durable artifacts, gated on tier `Crashes` (metrics consent NOT
/// required — this is exactly the "telemetry off, crash reports on" mode):
///
/// * a `Severity::Error` LOG record (red in Loki, `event.kind="crash"`),
///   queued for the OTLP `/v1/logs` path when an endpoint exists;
/// * a self-contained JSON CRASH DUMP queued as [`PingKind::Crash`] — never
///   uploaded over OTLP, it is the payload the `crash-mail` backup transport
///   attaches for deployments with no collector at all.
///
/// Nothing is uploaded in-hook: a hook that blocks on the network turns a
/// recoverable panic into a hang. The next launch (or the crash mailer)
/// drains the queue.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_panic();
        if tier().allows_crashes() {
            let message = panic_message(info);
            let location = info
                .location()
                .map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
            let location = strip_user_paths(&location);
            let backtrace =
                strip_user_paths(&std::backtrace::Backtrace::force_capture().to_string());
            let scope = crate::probe::Probe::span_path();
            let sys = sysinfo::get();
            let doc_size = document_size();
            let client_id = config_snapshot().client_id;

            // 1. The Loki-facing log record. Severity ERROR renders red;
            //    FATAL maps to Grafana's purple "critical" band.
            let mut record =
                LogRecord::new(Severity::Error, format!("crash: {message} (at {location})"))
                    .with_attribute("event.kind", "crash")
                    .with_attribute("crash.message", message.clone())
                    .with_attribute("crash.location", location.clone())
                    .with_attribute("crash.scope", scope.clone())
                    .with_attribute("crash.backtrace", backtrace.clone())
                    .with_attribute("app.document_size", format!("{doc_size}"));
            for (k, v) in sys.as_attributes() {
                record = record.with_attribute(k, v);
            }
            if let Some(id) = &client_id {
                record = record.with_attribute("client_id", id.clone());
            }
            push_log(record);

            // 2. Durability, WITHOUT the metrics-tier gate `persist()`
            //    carries: encode the buffered log records and the crash dump
            //    straight into the queue. At tier `Crashes` this is the ONLY
            //    write path that runs.
            if let Some((resource, Some(queue))) = inner().read().ok().and_then(|slot| {
                slot.as_ref()
                    .map(|state| (state.resource.clone(), state.queue.clone()))
            }) {
                let records = log_buffer()
                    .lock()
                    .map(|mut buffer| std::mem::take(&mut *buffer))
                    .unwrap_or_default();
                if let Some(payload) = otlp::encode_logs(&records, &resource) {
                    drop(queue.enqueue(PingKind::Logs, &payload));
                }
                let dump = serde_json::json!({
                    "kind": "azul-crash-dump",
                    "time_unix": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs()),
                    "app": resource.service_name,
                    "version": resource.service_version,
                    "client_id": client_id,
                    "message": message,
                    "location": location,
                    "scope": scope,
                    "backtrace": backtrace,
                    // The action journal: what ran just before the crash.
                    // Empty unless the app armed it (handler names + node
                    // ids only — never what the user typed).
                    "recent_actions": serde_json::from_str::<serde_json::Value>(
                        &crate::journal::recent_json(crate::journal::DEFAULT_CAPACITY),
                    )
                    .unwrap_or_else(|_| serde_json::json!([])),
                    "document_size": doc_size,
                    "system": {
                        "cpu_model": sys.cpu_model,
                        "cpu_count": sys.cpu_count,
                        "ram_total_bytes": sys.ram_total_bytes,
                        "os": sys.os,
                        "windowing": sys.windowing,
                        "gpu": sys.gpu,
                    },
                })
                .to_string();
                drop(queue.enqueue(PingKind::Crash, &dump));

                // The REPORTER flow: only when the app registered a crash
                // contact (a mailbox) AND no OTLP endpoint exists — with an
                // endpoint the pipeline is automatic and no dialog is owed.
                // Write the dump to a temp file and reinvoke OUR OWN
                // executable, detached, with AZ_CRASH_DUMP pointing at it;
                // the dying process then finishes dying. The reinvoked
                // process (AzApp::run checks the env var) parses the dump,
                // shows it (CPU rendering only) and asks about submission.
                let endpoint_configured = config_snapshot().signal_url("logs").is_some();
                if CRASH_CONTACT_SET.load(Ordering::Relaxed) && !endpoint_configured {
                    let file = std::env::temp_dir().join(format!(
                        "azul-crash-{}-{}.json",
                        std::process::id(),
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs()),
                    ));
                    if std::fs::write(&file, &dump).is_ok() {
                        if let Ok(exe) = std::env::current_exe() {
                            drop(
                                std::process::Command::new(exe)
                                    .env(CRASH_DUMP_ENV, &file)
                                    .stdin(std::process::Stdio::null())
                                    .spawn(),
                            );
                        }
                    }
                }
            }
            // Metrics durability for the tiers that collect them.
            if is_collecting() {
                drop(persist());
            }
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

    #[test]
    fn strip_user_paths_removes_home_and_shortens_absolutes() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/testuser".into());
        let input = format!(
            "panicked at {home}/Development/azul/layout/src/window.rs:42\n\
             at /rustc/abcdef1234567890abcdef1234567890abcdef12/library/std/src/panic.rs:10\n\
             at /very/long/absolute/path/to/some/crate/src/lib.rs:7"
        );
        let out = super::strip_user_paths(&input);
        assert!(!out.contains(&home), "home dir must not survive: {out}");
        assert!(out.contains("~/Development"), "home becomes ~: {out}");
        assert!(
            out.contains("rust:library/std/src/panic.rs"),
            "rustc prefix collapses: {out}"
        );
        assert!(
            out.contains("…/crate/src/lib.rs"),
            "long paths keep 3 components: {out}"
        );
        // The frame information itself survives.
        assert!(out.contains("window.rs:42"));
    }
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
        assert!(
            metrics::snapshot().is_empty(),
            "tier Off must record nothing"
        );
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
