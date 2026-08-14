//! OTLP/HTTP **JSON** encoding for metrics and log records.
//!
//! The wire protocol is the OpenTelemetry spec's own JSON encoding — same
//! `/v1/metrics` and `/v1/logs` paths, same message shapes as the protobuf
//! form, `Content-Type: application/json`. It is not a custom schema, which is
//! what keeps every backend exit open: any OTLP collector, vendor endpoint or
//! hand-written ingest proxy already understands it.
//!
//! The one rule that is easy to get wrong and impossible to notice afterwards:
//! **proto3 JSON encodes 64-bit integers as strings.** `timeUnixNano`,
//! `startTimeUnixNano`, `asInt`, `count` and `bucketCounts` are all quoted
//! here for that reason; `sum`, `asDouble` and `explicitBounds` are doubles
//! and stay bare.
//!
//! JSON rather than protobuf is a deliberate choice at this volume (a 60 s
//! flush of ~50 series is a few kilobytes): payloads are curl-able, readable
//! in a log, and the consent preview *is* the payload.

use super::{
    json::{write_number, write_string},
    metrics::{InstrumentValue, MetricsSnapshot},
};

/// `AGGREGATION_TEMPORALITY_CUMULATIVE` — the only temporality this client
/// emits. Prometheus-family backends want cumulative, and it makes a dropped
/// upload lossless.
const TEMPORALITY_CUMULATIVE: u32 = 2;

/// Identifies the sending app to the backend.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceInfo {
    /// `OTel` `service.name` — the app id.
    pub service_name: String,
    /// `OTel` `service.version`.
    pub service_version: String,
    /// Name of the instrumentation scope, e.g. `azul-layout`.
    pub scope_name: String,
    /// Version of the instrumentation scope.
    pub scope_version: String,
}

/// OTLP severity numbers.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum Severity {
    /// `SEVERITY_NUMBER_TRACE`
    Trace = 1,
    /// `SEVERITY_NUMBER_DEBUG`
    Debug = 5,
    /// `SEVERITY_NUMBER_INFO`
    #[default]
    Info = 9,
    /// `SEVERITY_NUMBER_WARN`
    Warn = 13,
    /// `SEVERITY_NUMBER_ERROR`
    Error = 17,
    /// `SEVERITY_NUMBER_FATAL`
    Fatal = 21,
}

impl Severity {
    /// The uppercase `severityText` that accompanies the number.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

/// One structured log record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    /// Event time, Unix nanoseconds.
    pub time_unix_nanos: u64,
    /// Severity.
    pub severity: Severity,
    /// The message.
    pub body: String,
    /// Extra key/value pairs. Unlike metric labels these may be
    /// high-cardinality — the `client_id` rides here, never on a metric.
    pub attributes: Vec<(String, String)>,
}

impl LogRecord {
    /// A record with no extra attributes, timestamped now.
    #[must_use]
    pub fn new(severity: Severity, body: impl Into<String>) -> Self {
        Self {
            time_unix_nanos: super::unix_nanos(),
            severity,
            body: body.into(),
            attributes: Vec::new(),
        }
    }

    /// Adds one attribute.
    #[must_use]
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((key.into(), value.into()));
        self
    }
}

/// Writes `{"key":"…","value":{"stringValue":"…"}}`.
fn write_attribute(out: &mut String, key: &str, value: &str) {
    out.push_str("{\"key\":");
    write_string(out, key);
    out.push_str(",\"value\":{\"stringValue\":");
    write_string(out, value);
    out.push_str("}}");
}

/// Writes an `attributes` array from `(key, value)` pairs.
fn write_attributes<'a>(out: &mut String, pairs: impl Iterator<Item = (&'a str, &'a str)>) {
    out.push_str("\"attributes\":[");
    for (i, (key, value)) in pairs.enumerate() {
        if i != 0 {
            out.push(',');
        }
        write_attribute(out, key, value);
    }
    out.push(']');
}

/// Writes a `u64` as the quoted string proto3 JSON requires.
fn write_u64_string(out: &mut String, value: u64) {
    out.push('"');
    out.push_str(itoa(value).as_str());
    out.push('"');
}

/// `u64` to decimal without pulling in a formatting dependency.
fn itoa(value: u64) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = write!(s, "{value}");
    s
}

fn write_resource(out: &mut String, resource: &ResourceInfo) {
    out.push_str("\"resource\":{");
    write_attributes(
        out,
        [
            ("service.name", resource.service_name.as_str()),
            ("service.version", resource.service_version.as_str()),
        ]
        .into_iter(),
    );
    out.push('}');
}

fn write_scope(out: &mut String, resource: &ResourceInfo) {
    out.push_str("\"scope\":{\"name\":");
    write_string(out, &resource.scope_name);
    out.push_str(",\"version\":");
    write_string(out, &resource.scope_version);
    out.push('}');
}

/// Encodes a metrics snapshot as an OTLP `ExportMetricsServiceRequest`.
///
/// Returns `None` when the snapshot has no series, so the caller can skip the
/// upload entirely rather than posting an empty envelope.
#[must_use]
pub fn encode_metrics(snapshot: &MetricsSnapshot, resource: &ResourceInfo) -> Option<String> {
    if snapshot.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(1024);
    out.push_str("{\"resourceMetrics\":[{");
    write_resource(&mut out, resource);
    out.push_str(",\"scopeMetrics\":[{");
    write_scope(&mut out, resource);
    out.push_str(",\"metrics\":[");

    for (index, series) in snapshot.series.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        write_string(&mut out, &series.key.name);

        // Every data point carries the four bounded labels plus, at most, the
        // instrument's one code-chosen dimension.
        let label_pairs = snapshot.labels.pairs();
        let dim = series
            .key
            .dim_key
            .as_deref()
            .zip(series.key.dim_value.as_deref());
        let attribute_pairs = || label_pairs.into_iter().chain(dim);

        match &series.value {
            InstrumentValue::Counter(total) => {
                out.push_str(",\"sum\":{\"aggregationTemporality\":");
                out.push_str(itoa(u64::from(TEMPORALITY_CUMULATIVE)).as_str());
                out.push_str(",\"isMonotonic\":true,\"dataPoints\":[{");
                write_attributes(&mut out, attribute_pairs());
                out.push_str(",\"startTimeUnixNano\":");
                write_u64_string(&mut out, snapshot.start_unix_nanos);
                out.push_str(",\"timeUnixNano\":");
                write_u64_string(&mut out, snapshot.now_unix_nanos);
                out.push_str(",\"asInt\":");
                write_u64_string(&mut out, *total);
                out.push_str("}]}");
            }
            InstrumentValue::Gauge(value) => {
                out.push_str(",\"gauge\":{\"dataPoints\":[{");
                write_attributes(&mut out, attribute_pairs());
                out.push_str(",\"timeUnixNano\":");
                write_u64_string(&mut out, snapshot.now_unix_nanos);
                out.push_str(",\"asDouble\":");
                write_number(&mut out, *value);
                out.push_str("}]}");
            }
            InstrumentValue::Histogram(hist) => {
                out.push_str(",\"histogram\":{\"aggregationTemporality\":");
                out.push_str(itoa(u64::from(TEMPORALITY_CUMULATIVE)).as_str());
                out.push_str(",\"dataPoints\":[{");
                write_attributes(&mut out, attribute_pairs());
                out.push_str(",\"startTimeUnixNano\":");
                write_u64_string(&mut out, snapshot.start_unix_nanos);
                out.push_str(",\"timeUnixNano\":");
                write_u64_string(&mut out, snapshot.now_unix_nanos);
                out.push_str(",\"count\":");
                write_u64_string(&mut out, hist.count);
                out.push_str(",\"sum\":");
                write_number(&mut out, hist.sum);
                out.push_str(",\"bucketCounts\":[");
                for (i, count) in hist.counts.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    write_u64_string(&mut out, *count);
                }
                out.push_str("],\"explicitBounds\":[");
                for (i, bound) in hist.bounds.iter().enumerate() {
                    if i != 0 {
                        out.push(',');
                    }
                    write_number(&mut out, *bound);
                }
                out.push_str("]}]}");
            }
        }
        out.push('}');
    }

    out.push_str("]}]}]}");
    Some(out)
}

/// Encodes log records as an OTLP `ExportLogsServiceRequest`.
///
/// Returns `None` for an empty batch.
#[must_use]
pub fn encode_logs(records: &[LogRecord], resource: &ResourceInfo) -> Option<String> {
    if records.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(512);
    out.push_str("{\"resourceLogs\":[{");
    write_resource(&mut out, resource);
    out.push_str(",\"scopeLogs\":[{");
    write_scope(&mut out, resource);
    out.push_str(",\"logRecords\":[");

    for (index, record) in records.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str("{\"timeUnixNano\":");
        write_u64_string(&mut out, record.time_unix_nanos);
        out.push_str(",\"observedTimeUnixNano\":");
        write_u64_string(&mut out, record.time_unix_nanos);
        out.push_str(",\"severityNumber\":");
        out.push_str(itoa(u64::from(record.severity as u8)).as_str());
        out.push_str(",\"severityText\":");
        write_string(&mut out, record.severity.as_str());
        out.push_str(",\"body\":{\"stringValue\":");
        write_string(&mut out, &record.body);
        out.push_str("},");
        write_attributes(
            &mut out,
            record
                .attributes
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str())),
        );
        out.push('}');
    }

    out.push_str("]}]}]}");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{
        json::{parse, JsonValue},
        metrics::{HistogramData, InstrumentKey, MetricLabels, Series},
    };

    fn resource() -> ResourceInfo {
        ResourceInfo {
            service_name: "azul-demo".to_owned(),
            service_version: "1.4.2".to_owned(),
            scope_name: "azul-layout".to_owned(),
            scope_version: "0.0.13".to_owned(),
        }
    }

    fn snapshot_with(series: Vec<Series>) -> MetricsSnapshot {
        MetricsSnapshot {
            labels: MetricLabels {
                version: "1.4.2".to_owned(),
                channel: "beta".to_owned(),
                os: "linux".to_owned(),
                arch: "x86_64".to_owned(),
            },
            start_unix_nanos: 1_700_000_000_000_000_000,
            now_unix_nanos: 1_700_000_060_000_000_000,
            series,
            dropped_series: 0,
        }
    }

    #[test]
    fn an_empty_snapshot_encodes_to_nothing() {
        assert!(encode_metrics(&snapshot_with(Vec::new()), &resource()).is_none());
        assert!(encode_logs(&[], &resource()).is_none());
    }

    #[test]
    fn counters_encode_as_cumulative_monotonic_sums() {
        let json = encode_metrics(
            &snapshot_with(vec![Series {
                key: InstrumentKey::plain("app_sessions_started_total"),
                value: InstrumentValue::Counter(3),
            }]),
            &resource(),
        )
        .expect("non-empty");

        let parsed = parse(&json).expect("valid JSON");
        let metric = &parsed
            .get("resourceMetrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("scopeMetrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("metrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0];

        assert_eq!(
            metric.get("name").and_then(JsonValue::as_str),
            Some("app_sessions_started_total")
        );
        let sum = metric.get("sum").expect("sum");
        assert_eq!(sum.get("isMonotonic").and_then(JsonValue::as_bool), Some(true));
        assert_eq!(
            sum.get("aggregationTemporality").and_then(JsonValue::as_u64),
            Some(2)
        );
        let point = &sum.get("dataPoints").and_then(JsonValue::as_array).unwrap()[0];
        // 64-bit ints MUST be strings in the proto3 JSON mapping.
        assert_eq!(point.get("asInt"), Some(&JsonValue::Str("3".to_owned())));
        assert_eq!(
            point.get("timeUnixNano"),
            Some(&JsonValue::Str("1700000060000000000".to_owned()))
        );
    }

    #[test]
    fn every_data_point_carries_exactly_the_four_labels_plus_its_dimension() {
        let json = encode_metrics(
            &snapshot_with(vec![Series {
                key: InstrumentKey::with_dim("app_update_check_total", "result", "ok"),
                value: InstrumentValue::Counter(1),
            }]),
            &resource(),
        )
        .expect("non-empty");
        let parsed = parse(&json).expect("valid JSON");
        let point = &parsed
            .get("resourceMetrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("scopeMetrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("metrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("sum")
            .unwrap()
            .get("dataPoints")
            .and_then(JsonValue::as_array)
            .unwrap()[0];

        let attributes = point.get("attributes").and_then(JsonValue::as_array).unwrap();
        let keys: Vec<&str> = attributes
            .iter()
            .filter_map(|a| a.get("key").and_then(JsonValue::as_str))
            .collect();
        assert_eq!(keys, vec!["version", "channel", "os", "arch", "result"]);
    }

    #[test]
    fn histograms_encode_bounds_as_doubles_and_counts_as_strings() {
        let mut hist = HistogramData {
            bounds: vec![0.1, 1.0],
            counts: vec![1, 2, 0],
            sum: 1.75,
            count: 3,
        };
        hist.counts[2] = 0;
        let json = encode_metrics(
            &snapshot_with(vec![Series {
                key: InstrumentKey::plain("app_startup_seconds"),
                value: InstrumentValue::Histogram(hist),
            }]),
            &resource(),
        )
        .expect("non-empty");
        let parsed = parse(&json).expect("valid JSON");
        let point = &parsed
            .get("resourceMetrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("scopeMetrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("metrics")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("histogram")
            .unwrap()
            .get("dataPoints")
            .and_then(JsonValue::as_array)
            .unwrap()[0];

        assert_eq!(point.get("count"), Some(&JsonValue::Str("3".to_owned())));
        assert_eq!(point.get("sum"), Some(&JsonValue::Number(1.75)));
        let buckets = point.get("bucketCounts").and_then(JsonValue::as_array).unwrap();
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[1], JsonValue::Str("2".to_owned()));
        let bounds = point.get("explicitBounds").and_then(JsonValue::as_array).unwrap();
        assert_eq!(bounds, [JsonValue::Number(0.1), JsonValue::Number(1.0)]);
    }

    #[test]
    fn log_records_carry_severity_number_text_and_attributes() {
        let record = LogRecord {
            time_unix_nanos: 1_700_000_000_000_000_000,
            severity: Severity::Warn,
            body: "font cache miss".to_owned(),
            attributes: vec![("client_id".to_owned(), "uuid-1".to_owned())],
        };
        let json = encode_logs(std::slice::from_ref(&record), &resource()).expect("non-empty");
        let parsed = parse(&json).expect("valid JSON");
        let log = &parsed
            .get("resourceLogs")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("scopeLogs")
            .and_then(JsonValue::as_array)
            .unwrap()[0]
            .get("logRecords")
            .and_then(JsonValue::as_array)
            .unwrap()[0];

        assert_eq!(log.get("severityNumber").and_then(JsonValue::as_u64), Some(13));
        assert_eq!(log.get("severityText").and_then(JsonValue::as_str), Some("WARN"));
        assert_eq!(
            log.get("body").and_then(|b| b.get("stringValue")).and_then(JsonValue::as_str),
            Some("font cache miss")
        );
        let attrs = log.get("attributes").and_then(JsonValue::as_array).unwrap();
        assert_eq!(attrs[0].get("key").and_then(JsonValue::as_str), Some("client_id"));
    }

    #[test]
    fn message_bodies_with_quotes_and_newlines_stay_parseable() {
        let record = LogRecord::new(Severity::Error, "panicked at \"src/x.rs\"\nline 2\ttab");
        let json = encode_logs(std::slice::from_ref(&record), &resource()).expect("non-empty");
        let parsed = parse(&json).expect("payload must survive escaping");
        assert!(parsed.get("resourceLogs").is_some());
    }
}
