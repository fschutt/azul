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
//!
//! Encoding goes through `serde_json` (a `telemetry`-feature dependency —
//! USER ruling 2026-08-18, replacing a hand-rolled writer). The quoted-64-bit
//! rule is enforced BY CONSTRUCTION: [`ju64`] renders every 64-bit integer as
//! a `Value::String`, so a `u64` can never reach a bare JSON number.

use serde_json::{json, Value};

use super::metrics::{InstrumentValue, MetricsSnapshot};

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

/// One `{"key": …, "value": {"stringValue": …}}` attribute list, in
/// iteration order (JSON arrays are ordered; tests pin the label order).
fn jattrs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> Value {
    Value::Array(
        pairs
            .map(|(key, value)| json!({"key": key, "value": {"stringValue": value}}))
            .collect(),
    )
}

/// A `u64` as the QUOTED string proto3 JSON requires (`timeUnixNano`,
/// `asInt`, `count`, `bucketCounts`). Type-level enforcement of the one rule
/// that is easy to get wrong and impossible to notice afterwards.
fn ju64(value: u64) -> Value {
    Value::String(value.to_string())
}

/// A finite JSON number. JSON has no `NaN`/`Infinity`, so non-finite values
/// become `0` — a dropped data point beats an unparseable payload at the
/// collector.
fn jnum(n: f64) -> Value {
    serde_json::Number::from_f64(n).map_or_else(|| json!(0), Value::Number)
}

fn jresource(resource: &ResourceInfo) -> Value {
    json!({
        "attributes": jattrs(
            [
                ("service.name", resource.service_name.as_str()),
                ("service.version", resource.service_version.as_str()),
            ]
            .into_iter(),
        )
    })
}

fn jscope(resource: &ResourceInfo) -> Value {
    json!({"name": resource.scope_name, "version": resource.scope_version})
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

    let metrics: Vec<Value> = snapshot
        .series
        .iter()
        .map(|series| {
            // Every data point carries the four bounded resource labels plus
            // the instrument's sanitized, capped code-chosen labels.
            let label_pairs = snapshot.labels.pairs();
            let dims = series
                .key
                .dims
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()));
            let attributes = jattrs(label_pairs.into_iter().chain(dims));

            let body = match &series.value {
                InstrumentValue::Counter(total) => json!({
                    "sum": {
                        "aggregationTemporality": TEMPORALITY_CUMULATIVE,
                        "isMonotonic": true,
                        "dataPoints": [{
                            "attributes": attributes,
                            "startTimeUnixNano": ju64(snapshot.start_unix_nanos),
                            "timeUnixNano": ju64(snapshot.now_unix_nanos),
                            "asInt": ju64(*total),
                        }],
                    }
                }),
                InstrumentValue::Gauge(value) => json!({
                    "gauge": {
                        "dataPoints": [{
                            "attributes": attributes,
                            "timeUnixNano": ju64(snapshot.now_unix_nanos),
                            "asDouble": jnum(*value),
                        }],
                    }
                }),
                InstrumentValue::Histogram(hist) => json!({
                    "histogram": {
                        "aggregationTemporality": TEMPORALITY_CUMULATIVE,
                        "dataPoints": [{
                            "attributes": attributes,
                            "startTimeUnixNano": ju64(snapshot.start_unix_nanos),
                            "timeUnixNano": ju64(snapshot.now_unix_nanos),
                            "count": ju64(hist.count),
                            "sum": jnum(hist.sum),
                            "bucketCounts": Value::Array(
                                hist.counts.iter().map(|c| ju64(*c)).collect(),
                            ),
                            "explicitBounds": Value::Array(
                                hist.bounds.iter().map(|b| jnum(*b)).collect(),
                            ),
                        }],
                    }
                }),
            };
            let mut metric = body;
            if let Value::Object(map) = &mut metric {
                map.insert("name".to_owned(), json!(series.key.name));
            }
            metric
        })
        .collect();

    Some(
        json!({
            "resourceMetrics": [{
                "resource": jresource(resource),
                "scopeMetrics": [{
                    "scope": jscope(resource),
                    "metrics": metrics,
                }],
            }],
        })
        .to_string(),
    )
}

/// Encodes log records as an OTLP `ExportLogsServiceRequest`.
///
/// Returns `None` for an empty batch.
#[must_use]
pub fn encode_logs(records: &[LogRecord], resource: &ResourceInfo) -> Option<String> {
    if records.is_empty() {
        return None;
    }

    let log_records: Vec<Value> = records
        .iter()
        .map(|record| {
            json!({
                "timeUnixNano": ju64(record.time_unix_nanos),
                "observedTimeUnixNano": ju64(record.time_unix_nanos),
                "severityNumber": record.severity as u8,
                "severityText": record.severity.as_str(),
                "body": {"stringValue": record.body},
                "attributes": jattrs(
                    record.attributes.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                ),
            })
        })
        .collect();

    Some(
        json!({
            "resourceLogs": [{
                "resource": jresource(resource),
                "scopeLogs": [{
                    "scope": jscope(resource),
                    "logRecords": log_records,
                }],
            }],
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    use crate::telemetry::metrics::{HistogramData, InstrumentKey, MetricLabels, Series};

    fn parse(s: &str) -> Result<Value, serde_json::Error> {
        serde_json::from_str(s)
    }

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
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("scopeMetrics")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("metrics")
            .and_then(Value::as_array)
            .unwrap()[0];

        assert_eq!(
            metric.get("name").and_then(Value::as_str),
            Some("app_sessions_started_total")
        );
        let sum = metric.get("sum").expect("sum");
        assert_eq!(sum.get("isMonotonic").and_then(Value::as_bool), Some(true));
        assert_eq!(
            sum.get("aggregationTemporality").and_then(Value::as_u64),
            Some(2)
        );
        let point = &sum.get("dataPoints").and_then(Value::as_array).unwrap()[0];
        // 64-bit ints MUST be strings in the proto3 JSON mapping.
        assert_eq!(point.get("asInt").and_then(Value::as_str), Some("3"));
        assert_eq!(
            point.get("timeUnixNano").and_then(Value::as_str),
            Some("1700000060000000000")
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
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("scopeMetrics")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("metrics")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("sum")
            .unwrap()
            .get("dataPoints")
            .and_then(Value::as_array)
            .unwrap()[0];

        let attributes = point.get("attributes").and_then(Value::as_array).unwrap();
        let keys: Vec<&str> = attributes
            .iter()
            .filter_map(|a| a.get("key").and_then(Value::as_str))
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
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("scopeMetrics")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("metrics")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("histogram")
            .unwrap()
            .get("dataPoints")
            .and_then(Value::as_array)
            .unwrap()[0];

        assert_eq!(point.get("count").and_then(Value::as_str), Some("3"));
        assert_eq!(point.get("sum").and_then(Value::as_f64), Some(1.75));
        let buckets = point.get("bucketCounts").and_then(Value::as_array).unwrap();
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[1].as_str(), Some("2"));
        let bounds = point
            .get("explicitBounds")
            .and_then(Value::as_array)
            .unwrap();
        let bound_vals: Vec<f64> = bounds.iter().filter_map(Value::as_f64).collect();
        assert_eq!(bound_vals, vec![0.1, 1.0]);
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
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("scopeLogs")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("logRecords")
            .and_then(Value::as_array)
            .unwrap()[0];

        assert_eq!(log.get("severityNumber").and_then(Value::as_u64), Some(13));
        assert_eq!(
            log.get("severityText").and_then(Value::as_str),
            Some("WARN")
        );
        assert_eq!(
            log.get("body")
                .and_then(|b| b.get("stringValue"))
                .and_then(Value::as_str),
            Some("font cache miss")
        );
        let attrs = log.get("attributes").and_then(Value::as_array).unwrap();
        assert_eq!(
            attrs[0].get("key").and_then(Value::as_str),
            Some("client_id")
        );
    }

    #[test]
    fn message_bodies_with_quotes_and_newlines_stay_parseable() {
        let record = LogRecord::new(Severity::Error, "panicked at \"src/x.rs\"\nline 2\ttab");
        let json = encode_logs(std::slice::from_ref(&record), &resource()).expect("non-empty");
        let parsed = parse(&json).expect("payload must survive escaping");
        assert!(parsed.get("resourceLogs").is_some());
    }
}
