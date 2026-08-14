//! Disk-backed pending-ping queue and the uploader that drains it.
//!
//! This is Glean's pending-pings directory, reproduced: serialize a batch to
//! its own file, upload FIFO, `2xx` → delete, `4xx` → drop as poison,
//! `5xx`/offline → keep and retry later, and cap the whole directory so a user
//! who is offline for a month never accumulates unbounded data.
//!
//! Everything here is blocking IO and must therefore run off the UI thread —
//! either on the uploader thread started by [`super::spawn_uploader`], or on
//! an azul `Thread` owned by the embedding app.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::config::TelemetryConfig;
use crate::http::{http_post_with_config, HttpRequestConfig};

/// Default cap on queued files.
pub const DEFAULT_MAX_FILES: usize = 500;
/// Default cap on total queued bytes (10 MiB).
pub const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Request timeout for one upload attempt.
const UPLOAD_TIMEOUT_SECS: u64 = 20;

/// Which OTLP signal a queued file belongs to.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PingKind {
    /// An `ExportMetricsServiceRequest`, posted to `/v1/metrics`.
    Metrics,
    /// An `ExportLogsServiceRequest`, posted to `/v1/logs`.
    Logs,
    /// The final deletion-request ping. It is a log record so it can carry the
    /// `client_id` (which must never be a metric label) and so the server sees
    /// it on the same path as the events it is asked to erase.
    Deletion,
}

impl PingKind {
    /// The token embedded in the queued file's name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metrics => "metrics",
            Self::Logs => "logs",
            Self::Deletion => "deletion",
        }
    }

    /// The OTLP signal path segment this ping is posted to.
    #[must_use]
    pub const fn signal(self) -> &'static str {
        match self {
            Self::Metrics => "metrics",
            Self::Logs | Self::Deletion => "logs",
        }
    }

    /// Recovers the kind from a queued file's name.
    #[must_use]
    pub fn from_file_name(name: &str) -> Option<Self> {
        let stem = name.strip_suffix(".json")?;
        match stem.rsplit('-').next()? {
            "metrics" => Some(Self::Metrics),
            "logs" => Some(Self::Logs),
            "deletion" => Some(Self::Deletion),
            _ => None,
        }
    }
}

/// A FIFO directory of pending pings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingQueue {
    dir: PathBuf,
    max_files: usize,
    max_bytes: u64,
}

/// Monotonic counter disambiguating pings written inside the same millisecond.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl PingQueue {
    /// A queue rooted at `dir`, with the default quotas.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            max_files: DEFAULT_MAX_FILES,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    /// The conventional location: `{data_dir}/{app_id}/telemetry/pending/`.
    #[must_use]
    pub fn for_app(app_id: &str) -> Option<Self> {
        Some(Self::new(
            super::config::data_dir()?
                .join(app_id)
                .join("telemetry")
                .join("pending"),
        ))
    }

    /// Overrides the quotas.
    #[must_use]
    pub const fn with_quota(mut self, max_files: usize, max_bytes: u64) -> Self {
        self.max_files = max_files;
        self.max_bytes = max_bytes;
        self
    }

    /// The directory this queue writes to.
    #[must_use]
    pub fn dir(&self) -> &Path {
        self.dir.as_path()
    }

    /// Writes one payload to the queue and enforces the quota.
    ///
    /// # Errors
    ///
    /// Returns the IO error if the directory cannot be created or the file
    /// cannot be written.
    pub fn enqueue(&self, kind: PingKind, payload: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let millis = super::unix_nanos() / 1_000_000;
        let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // Zero-padded so lexicographic order *is* chronological order, which
        // is what makes `pending()` a FIFO without reading any metadata.
        let name = format!("{millis:013}-{seq:06}-{}.json", kind.as_str());
        let path = self.dir.join(name);
        std::fs::write(&path, payload)?;
        self.enforce_quota();
        Ok(path)
    }

    /// Every queued file, oldest first.
    #[must_use]
    pub fn pending(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        files
    }

    /// Number of queued pings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending().len()
    }

    /// Whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Deletes every queued ping. Used on opt-out: data collected before the
    /// user said no must not be uploaded afterwards.
    pub fn clear(&self) {
        for path in self.pending() {
            drop(std::fs::remove_file(path));
        }
    }

    /// Drops the oldest files until both quotas are satisfied.
    fn enforce_quota(&self) {
        let mut files = self.pending();
        let mut total: u64 = files
            .iter()
            .filter_map(|path| std::fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum();

        while files.len() > self.max_files || total > self.max_bytes {
            let Some(oldest) = files.first().cloned() else {
                break;
            };
            let size = std::fs::metadata(&oldest).map(|m| m.len()).unwrap_or(0);
            if std::fs::remove_file(&oldest).is_err() {
                break;
            }
            total = total.saturating_sub(size);
            files.remove(0);
        }
    }
}

/// What one drain of the queue accomplished.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UploadStats {
    /// Pings accepted by the server and deleted locally.
    pub uploaded: usize,
    /// Pings the server rejected permanently (4xx) and that were dropped.
    pub dropped: usize,
    /// Pings still on disk, to be retried later.
    pub retained: usize,
    /// The failure that stopped this round, if any.
    pub last_error: Option<String>,
}

/// Uploads pending pings FIFO until the queue is empty or something fails.
///
/// A retryable failure (transport error, 408, 429, 5xx) stops the round and
/// leaves the rest of the queue in place — the interval timer will try again.
/// A permanent rejection (any other 4xx) drops just that ping: it is poison
/// and retrying it forever would block everything behind it.
pub fn upload_pending(queue: &PingQueue, config: &TelemetryConfig) -> UploadStats {
    let mut stats = UploadStats::default();
    let pending = queue.pending();
    if pending.is_empty() {
        return stats;
    }

    let mut request_config = HttpRequestConfig::new()
        .with_timeout(UPLOAD_TIMEOUT_SECS)
        .with_user_agent("azul-telemetry/1.0");
    if let Some(token) = &config.auth_token {
        request_config = request_config.with_header("Authorization", format!("Bearer {token}"));
    }

    let mut remaining = pending.into_iter();
    while let Some(path) = remaining.next() {
        let Some(kind) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(PingKind::from_file_name)
        else {
            // Not one of ours: leave it alone rather than deleting a file we
            // do not understand.
            stats.retained += 1;
            continue;
        };
        let Some(url) = config.signal_url(kind.signal()) else {
            stats.last_error = Some("no endpoint configured".to_owned());
            stats.retained += 1 + remaining.count();
            return stats;
        };
        let Ok(payload) = std::fs::read(&path) else {
            stats.retained += 1;
            continue;
        };

        match http_post_with_config(&url, &payload, "application/json", &request_config) {
            Ok(response) if (200..300).contains(&response.status_code) => {
                drop(std::fs::remove_file(&path));
                stats.uploaded += 1;
            }
            Ok(response) if is_retryable_status(response.status_code) => {
                stats.last_error = Some(format!("HTTP {}", response.status_code));
                stats.retained += 1 + remaining.count();
                return stats;
            }
            Ok(response) => {
                // Poison: the server will never accept this payload.
                drop(std::fs::remove_file(&path));
                stats.dropped += 1;
                stats.last_error = Some(format!("HTTP {} (dropped)", response.status_code));
            }
            Err(e) => {
                stats.last_error = Some(e.to_string());
                stats.retained += 1 + remaining.count();
                return stats;
            }
        }
    }

    stats
}

/// Statuses worth retrying: request timeout, rate limit, and every server
/// error.
const fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 429 || status >= 500
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch directory under the system temp dir, removed by the
    /// caller. Avoids a dev-dependency on `tempfile`.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "azul-telemetry-test-{tag}-{}-{}",
            std::process::id(),
            super::super::unix_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn ping_kind_round_trips_through_the_file_name() {
        for kind in [PingKind::Metrics, PingKind::Logs, PingKind::Deletion] {
            let name = format!("0000000000001-000002-{}.json", kind.as_str());
            assert_eq!(PingKind::from_file_name(&name), Some(kind));
        }
        assert_eq!(PingKind::from_file_name("not-a-ping.txt"), None);
        assert_eq!(PingKind::from_file_name("0-0-unknown.json"), None);
    }

    #[test]
    fn deletion_pings_are_posted_to_the_logs_signal() {
        // The deletion request carries the client_id, which must never touch
        // the metrics path.
        assert_eq!(PingKind::Deletion.signal(), "logs");
        assert_eq!(PingKind::Metrics.signal(), "metrics");
    }

    #[test]
    fn enqueue_is_fifo_by_file_name() {
        let dir = scratch("fifo");
        let queue = PingQueue::new(dir.clone());
        for i in 0..5 {
            queue
                .enqueue(PingKind::Metrics, &format!("{{\"n\":{i}}}"))
                .expect("enqueue");
        }
        let pending = queue.pending();
        assert_eq!(pending.len(), 5);
        let mut sorted = pending.clone();
        sorted.sort();
        assert_eq!(pending, sorted, "pending() must already be chronological");
        let first = std::fs::read_to_string(&pending[0]).unwrap();
        assert_eq!(first, "{\"n\":0}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn quota_drops_the_oldest_pings_first() {
        let dir = scratch("quota");
        let queue = PingQueue::new(dir.clone()).with_quota(3, DEFAULT_MAX_BYTES);
        for i in 0..6 {
            queue
                .enqueue(PingKind::Logs, &format!("{{\"n\":{i}}}"))
                .expect("enqueue");
        }
        let pending = queue.pending();
        assert_eq!(pending.len(), 3, "quota must be enforced on write");
        let kept: Vec<String> = pending
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        assert_eq!(kept, vec!["{\"n\":3}", "{\"n\":4}", "{\"n\":5}"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn byte_quota_is_enforced_too() {
        let dir = scratch("bytes");
        let queue = PingQueue::new(dir.clone()).with_quota(DEFAULT_MAX_FILES, 16);
        for _ in 0..5 {
            queue.enqueue(PingKind::Metrics, "0123456789").expect("enqueue");
        }
        let total: u64 = queue
            .pending()
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        assert!(total <= 16, "total = {total}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn upload_without_an_endpoint_retains_everything() {
        let dir = scratch("no-endpoint");
        let queue = PingQueue::new(dir.clone());
        queue.enqueue(PingKind::Metrics, "{}").expect("enqueue");
        queue.enqueue(PingKind::Metrics, "{}").expect("enqueue");

        let stats = upload_pending(&queue, &TelemetryConfig::default());
        assert_eq!(stats.uploaded, 0);
        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.retained, 2);
        assert_eq!(queue.len(), 2, "nothing may be deleted without an upload");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_empties_the_queue() {
        let dir = scratch("clear");
        let queue = PingQueue::new(dir.clone());
        queue.enqueue(PingKind::Logs, "{}").expect("enqueue");
        assert!(!queue.is_empty());
        queue.clear();
        assert!(queue.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn retryable_statuses_are_the_transient_ones() {
        assert!(is_retryable_status(408));
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(413));
    }
}
