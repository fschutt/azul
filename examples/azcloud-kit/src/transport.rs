//! Which transport a drive's requests take: iroh first when a node's iroh id is known, HTTPS
//! as the fallback.
//!
//! - `auto` (the default): when the build can dial iroh (an [`IrohDialer`] is plugged in) and
//!   there is a node to dial, one cheap signed request (a HEAD) probes it; an answer of any
//!   status means the pipe works and the session goes over iroh. No answer within 10 s, or an
//!   error, means HTTPS, and `transport.json` remembers it: the next runs take HTTPS straight
//!   away and try iroh again five minutes after the failure. A request that fails over iroh
//!   later is retried over HTTPS - unless a second probe shows iroh is fine, i.e. the error was
//!   the server's answer and HTTPS would only repeat it.
//! - `iroh`: iroh or nothing (an end-to-end run's iroh leg must not pass over HTTPS by
//!   accident).
//! - `https`: HTTPS only (the block endpoint, then the direct node URLs and what
//!   `x-azlin-alt-endpoints` taught: the [`crate::bucket::Bucket`]'s failover).
//!
//! Where the iroh node comes from: `--iroh-node` / `AZLIN_IROH_NODE` (with `--iroh-addr` /
//! `AZLIN_IROH_ADDR`, the node's UDP socket, so no discovery is needed), else the node list of
//! the last credential refresh when it names an `iroh_id` (or `sign_pubkey`) and `iroh_addrs`.
//! The client's relay is the configured one (`endpoints.relay`): `off`, `default` or an
//! address.
//!
//! The kit links no iroh: "S3 over iroh" sends the same signed HTTP requests over a QUIC
//! stream, so an iroh build plugs in an [`IrohDialer`] that makes an azul-storage
//! [`azul_storage::Transport`] to a node, and the kit does the rest.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use azul_appkit::azlin_config::Endpoint;
use azul_storage::{Credentials, HttpCall, HttpReply, ObjectInfo, S3Config, Transport};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    account::Account,
    bucket::Bucket,
    drive::TransportFactory,
    error::{fail, CloudError, CloudResult},
    now,
    settings::{redact_url, Settings},
    state::{read_json, write_json},
    store::{Conditional, RemoteObject, RemoteStore, BIG_BLOB},
};

/// How long a probe waits for iroh's answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// After iroh failed, how long `auto` keeps to HTTPS before it tries again.
pub const IROH_RETRY_SECS: i64 = 300;
/// The key a probe asks for: never written (`.azlin/` is bookkeeping), so the
/// answer is a 404 over a working pipe.
pub const PROBE_KEY: &str = ".azlin/probe";
/// Why `auto` cannot use iroh without a node to dial.
pub const NO_IROH_NODE: &str = "no node's iroh id is known: pass --iroh-node (AZLIN_IROH_NODE); \
                                the token server's node list carries no iroh ids yet";

/// The transport asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportPref {
    Auto,
    Iroh,
    Https,
}

impl TransportPref {
    /// `auto`, `iroh` (also `quic`), `https` (also `http`).
    #[must_use]
    pub fn parse(name: &str) -> Option<TransportPref> {
        match name.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(TransportPref::Auto),
            "iroh" | "quic" => Some(TransportPref::Iroh),
            "https" | "http" => Some(TransportPref::Https),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TransportPref::Auto => "auto",
            TransportPref::Iroh => "iroh",
            TransportPref::Https => "https",
        }
    }
}

/// The transport a request takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    Iroh,
    Https,
}

impl Lane {
    /// `iroh` or `https`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Lane::Iroh => "iroh",
            Lane::Https => "https",
        }
    }
}

/// A node to dial over iroh.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IrohTarget {
    /// Its endpoint id: the node's Ed25519 key (`sign_pubkey`), hex or base32.
    pub id: String,
    /// Its UDP socket (`ip:port`), when known.
    pub addr: Option<String>,
    /// Where the two came from.
    pub source: String,
}

/// What `transport.json` keeps between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportMemory {
    pub lane: Lane,
    pub reason: String,
    pub decided_at: i64,
    /// When iroh last failed (`auto` keeps to HTTPS for five minutes after).
    #[serde(default)]
    pub iroh_failed_at: Option<i64>,
    #[serde(default)]
    pub iroh_error: Option<String>,
}

/// What to do before any request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// HTTPS, for this reason.
    Https(String),
    /// Probe iroh (and use it when it answers).
    Probe,
}

/// The decision before any request: pure, so the tests walk every case.
/// `iroh_built` says whether this build can dial iroh.
///
/// # Errors
///
/// When iroh was asked for and cannot be had.
pub fn decide(
    pref: TransportPref,
    iroh_built: bool,
    target: Option<&IrohTarget>,
    memory: Option<&TransportMemory>,
    now: i64,
) -> Result<Decision, String> {
    match pref {
        TransportPref::Https => Ok(Decision::Https(String::from(
            "https was asked for (--transport / AZCLOUD_TRANSPORT)",
        ))),
        TransportPref::Iroh => {
            if !iroh_built {
                return Err(String::from(
                    "iroh was asked for, but this azcloud was built without the iroh feature",
                ));
            }
            if target.is_none() {
                return Err(format!("iroh was asked for, but {NO_IROH_NODE}"));
            }
            Ok(Decision::Probe)
        }
        TransportPref::Auto => {
            if !iroh_built {
                return Ok(Decision::Https(String::from(
                    "this azcloud was built without the iroh feature",
                )));
            }
            if target.is_none() {
                return Ok(Decision::Https(String::from(NO_IROH_NODE)));
            }
            if let Some((failed, error)) = memory.and_then(|m| {
                m.iroh_failed_at
                    .map(|at| (at, m.iroh_error.clone().unwrap_or_default()))
            }) {
                let ago = now - failed;
                if (0..IROH_RETRY_SECS).contains(&ago) {
                    return Ok(Decision::Https(format!(
                        "iroh failed {ago} s ago ({error}); it is tried again {} s after a failure",
                        IROH_RETRY_SECS
                    )));
                }
            }
            Ok(Decision::Probe)
        }
    }
}

/// The node to dial: the pinned one (`--iroh-node` / `AZLIN_IROH_NODE`), else
/// the first node of the node list that names its iroh id.
#[must_use]
pub fn iroh_target(settings: &Settings, nodes: &[Value]) -> Option<IrohTarget> {
    if let Some(id) = settings.iroh_node.value.as_deref() {
        return Some(IrohTarget {
            id: id.to_string(),
            addr: settings.iroh_addr.value.clone(),
            source: settings.iroh_node.source.label(),
        });
    }
    target_from_nodes(nodes)
}

/// The first ready node of a node list that names its iroh id (`iroh_id`,
/// else `sign_pubkey`), with its first IPv4 socket of `iroh_addrs`.
#[must_use]
pub fn target_from_nodes(nodes: &[Value]) -> Option<IrohTarget> {
    nodes.iter().find_map(|n| {
        if n["ready"].as_bool() == Some(false) {
            return None;
        }
        let id = n["iroh_id"]
            .as_str()
            .or_else(|| n["sign_pubkey"].as_str())
            .filter(|id| !id.is_empty())?;
        let addr = n["iroh_addrs"].as_array().and_then(|addrs| {
            addrs
                .iter()
                .filter_map(Value::as_str)
                .find(|a| a.contains('.'))
                .map(String::from)
        });
        Some(IrohTarget {
            id: id.to_string(),
            addr,
            source: format!("the node list ({})", n["name"].as_str().unwrap_or("a node")),
        })
    })
}

/// What the drive uses and why (`azcloud transport`).
#[derive(Clone, Debug, Serialize)]
pub struct TransportReport {
    pub preference: TransportPref,
    pub lane: Lane,
    pub reason: String,
    pub iroh_built: bool,
    pub iroh_target: Option<IrohTarget>,
    pub relay: Option<String>,
    /// The S3 endpoint and where it came from.
    pub endpoint: String,
    pub endpoint_source: String,
    /// Whether this run probed iroh.
    pub probed: bool,
    /// Set when a request fell back from iroh to HTTPS during the run.
    pub fell_back: Option<String>,
}

/// Dials a node's S3 over iroh: what a build that links iroh plugs into [`CloudDrive::open`].
/// The transport it makes sends the same signed requests the HTTPS one does - one request and
/// its answer per QUIC stream - to the node `target`, through `relay` when one is configured
/// (`off`, `default` or an address). Dropping the transport closes the connection.
pub trait IrohDialer: Send + Sync {
    /// A transport to `target`, ready to send; why not, otherwise.
    fn dial(&self, target: &IrohTarget, relay: Option<&str>)
        -> Result<Box<dyn Transport>, String>;
}

/// One dialed transport, shared by every request of the lane.
struct Dialed(Arc<dyn Transport>);

impl Transport for Dialed {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.send(call)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps the decision in `transport.json` (best effort: a failed write only
/// costs the next run a probe).
fn remember(path: &Path, lane: Lane, reason: &str, iroh_error: Option<String>) {
    let failed = iroh_error.is_some();
    let memory = TransportMemory {
        lane,
        reason: reason.to_string(),
        decided_at: now(),
        iroh_failed_at: failed.then(now),
        iroh_error,
    };
    let _ = write_json(path, &memory, false);
}

/// One signed HEAD of a key that never exists, waited for at most [`PROBE_TIMEOUT`]: any
/// answer means the pipe works.
fn probe(bucket: &Arc<Bucket>) -> Result<(), String> {
    let (answered, answer) = mpsc::channel();
    let probing = bucket.clone();
    std::thread::spawn(move || {
        let _ = answered.send(probing.probe(PROBE_KEY));
    });
    match answer.recv_timeout(PROBE_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(format!("no answer within {} s", PROBE_TIMEOUT.as_secs())),
    }
}

/// A bucket over iroh to `target`, probed.
fn try_iroh(
    config: &S3Config,
    credentials: &Credentials,
    target: &IrohTarget,
    relay: Option<&str>,
    dialer: &dyn IrohDialer,
) -> Result<Arc<Bucket>, String> {
    let dialed: Arc<dyn Transport> = Arc::from(dialer.dial(target, relay)?);
    let transports: TransportFactory =
        Arc::new(move || Box::new(Dialed(dialed.clone())) as Box<dyn Transport>);
    let bucket = Bucket::new(config.clone(), credentials.clone(), transports)
        .map_err(|e| e.to_string())?;
    let bucket = Arc::new(bucket);
    probe(&bucket)?;
    Ok(bucket)
}

/// One bucket of an account's drive over the transport this run chose: iroh when it was asked
/// for (or `auto` found it answering), HTTPS otherwise and as the fallback. A [`RemoteStore`]:
/// the sync and the shares talk to it as to any bucket. Blocking: call it from an azul
/// `Thread`.
pub struct CloudDrive {
    https: Bucket,
    iroh: Option<Arc<Bucket>>,
    lane: Mutex<Lane>,
    pref: TransportPref,
    report: Mutex<TransportReport>,
    memory_path: PathBuf,
}

impl fmt::Debug for CloudDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudDrive")
            .field("bucket", &self.https.config().bucket)
            .field("endpoint", &self.https.config().endpoint)
            .field("lane", &self.lane())
            .finish_non_exhaustive()
    }
}

impl CloudDrive {
    /// The drive of `account` (its credentials should be fresh: [`Account::ensure_fresh`]), over
    /// the transport `settings` ask for. HTTPS goes through the account's transports; iroh
    /// through `dialer` when the build can dial it (`None`: HTTPS only).
    ///
    /// # Errors
    ///
    /// When the credentials are missing, the endpoint or the bucket cannot be one, or iroh was
    /// asked for and fails.
    pub fn open(
        account: &Account,
        settings: &Settings,
        dialer: Option<&dyn IrohDialer>,
    ) -> CloudResult<CloudDrive> {
        let credentials = account.credentials()?;
        let record = account.record();
        let (endpoint, endpoint_source) = match settings.endpoints.url(Endpoint::S3) {
            Some(url) => (
                url.to_string(),
                settings.endpoints.get(Endpoint::S3).source.label(),
            ),
            None => (
                record.endpoint.clone(),
                format!("the drive's grant ({})", record.id),
            ),
        };
        let config = S3Config {
            endpoint: endpoint.clone(),
            region: record.region.clone(),
            bucket: record.bucket.clone(),
            path_style: record.path_style,
        };
        let https = Bucket::new(
            config.clone(),
            credentials.clone(),
            account.transports().clone(),
        )?
        .with_alternatives(record.node_urls());
        let target = iroh_target(settings, &record.nodes);
        let memory_path = account.state().transport_file();
        let memory: Option<TransportMemory> = read_json(&memory_path).ok().flatten();
        let pref = settings.transport_pref();
        let iroh_built = dialer.is_some();
        let decision = decide(pref, iroh_built, target.as_ref(), memory.as_ref(), now())
            .map_err(CloudError::Failed)?;
        let mut report = TransportReport {
            preference: pref,
            lane: Lane::Https,
            reason: String::new(),
            iroh_built,
            iroh_target: target.clone(),
            relay: settings.relay().map(String::from),
            endpoint: redact_url(&endpoint),
            endpoint_source,
            probed: false,
            fell_back: None,
        };
        let mut iroh = None;
        match (decision, target.as_ref(), dialer) {
            (Decision::Probe, Some(target), Some(dialer)) => {
                report.probed = true;
                match try_iroh(&config, &credentials, target, settings.relay(), dialer) {
                    Ok(bucket) => {
                        iroh = Some(bucket);
                        report.lane = Lane::Iroh;
                        report.reason = format!("iroh answered ({}, {})", target.id, target.source);
                        remember(&memory_path, Lane::Iroh, &report.reason, None);
                    }
                    Err(e) if pref == TransportPref::Iroh => {
                        fail!("iroh was asked for and failed: {e}");
                    }
                    Err(e) => {
                        report.reason = format!(
                            "iroh failed ({e}); https instead, iroh again in {IROH_RETRY_SECS} s"
                        );
                        remember(&memory_path, Lane::Https, &report.reason, Some(e));
                    }
                }
            }
            (Decision::Probe, _, _) => {
                report.reason = String::from(NO_IROH_NODE);
            }
            (Decision::Https(reason), _, _) => report.reason = reason,
        }
        Ok(CloudDrive {
            https,
            iroh,
            lane: Mutex::new(report.lane),
            pref,
            report: Mutex::new(report),
            memory_path,
        })
    }

    /// The transport now and why.
    #[must_use]
    pub fn transport(&self) -> TransportReport {
        lock(&self.report).clone()
    }

    /// The lane requests take now.
    #[must_use]
    pub fn lane(&self) -> Lane {
        *lock(&self.lane)
    }

    /// The bucket's name.
    #[must_use]
    pub fn bucket_name(&self) -> &str {
        self.https.name()
    }

    /// The HTTPS bucket (its endpoint and credentials: a presigned link is an HTTPS link
    /// whatever this session's transport).
    #[must_use]
    pub fn https_bucket(&self) -> &Bucket {
        &self.https
    }

    /// Sets how many parts or ranges of one big object are in flight.
    pub fn set_parallel(&self, parallel: usize) {
        self.https.set_parallel(parallel);
        if let Some(iroh) = &self.iroh {
            iroh.set_parallel(parallel);
        }
    }

    /// The S3 endpoint and where it came from, for messages.
    #[must_use]
    pub fn endpoint_label(&self) -> String {
        let report = self.transport();
        format!("{} ({})", report.endpoint, report.endpoint_source)
    }

    /// Closes the iroh connection, if there is one (a goodbye to the node instead of a
    /// timeout on its side); dropping the drive does the same.
    pub fn close(self) {}

    fn fall_back(&self, what: &str, error: &CloudError) {
        *lock(&self.lane) = Lane::Https;
        let reason = format!("{what} failed over iroh ({error}); https for the rest of the run");
        {
            let mut report = lock(&self.report);
            report.lane = Lane::Https;
            report.fell_back = Some(reason.clone());
        }
        remember(
            &self.memory_path,
            Lane::Https,
            &reason,
            Some(error.to_string()),
        );
    }

    /// Runs `call` on the bucket of the current lane. Over iroh, an error is retried over HTTPS
    /// (`auto` only) when a probe shows iroh itself failed; an error iroh carried fine is the
    /// server's, and is returned.
    fn run<T>(&self, what: &str, call: impl Fn(&Bucket) -> CloudResult<T>) -> CloudResult<T> {
        let iroh = match (self.lane(), self.iroh.as_ref()) {
            (Lane::Iroh, Some(iroh)) => iroh,
            _ => return call(&self.https),
        };
        match call(iroh) {
            Ok(value) => Ok(value),
            Err(e) if self.pref == TransportPref::Auto => {
                if probe(iroh).is_ok() {
                    return Err(e);
                }
                self.fall_back(what, &e);
                call(&self.https)
            }
            Err(e) => Err(e),
        }
    }

    /// PUT (multipart above twice the part size); the ETag.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        self.run("put", |b| b.put(key, data))
    }

    /// Conditional PUT: `If-Match: <etag>`, or `If-None-Match: *` without one; `None` when
    /// another writer won (412).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        self.run("conditional put", |b| b.put_if(key, data, if_match))
    }

    /// GET of a small object; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn get(&self, key: &str) -> CloudResult<Option<Vec<u8>>> {
        self.run("get", |b| b.get(key))
    }

    /// GET with ranges, several at once, for a big object (a HEAD first).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn get_big(&self, key: &str) -> CloudResult<Option<Vec<u8>>> {
        self.run("ranged get", |b| b.get_big(key))
    }

    /// HEAD: the size and ETag; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn head(&self, key: &str) -> CloudResult<Option<(u64, String)>> {
        self.run("head", |b| b.head(key))
    }

    /// DELETE (a missing object is no error).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn delete(&self, key: &str) -> CloudResult<()> {
        self.run("delete", |b| b.delete(key))
    }

    /// Every object under `prefix`, page by page.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn list_all(&self, prefix: &str) -> CloudResult<Vec<ObjectInfo>> {
        self.run("list", |b| b.list_all(prefix))
    }

    /// A conditional GET: `If-None-Match: <etag>` when one is given (the index's poll: a 304
    /// means nothing changed).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.run("conditional get", |b| b.get_unless(key, etag))
    }
}

impl RemoteStore for CloudDrive {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        CloudDrive::get_unless(self, key, etag)
    }

    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        if size > BIG_BLOB {
            self.get_big(key)
        } else {
            self.get(key)
        }
    }

    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        CloudDrive::put(self, key, data)
    }

    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        CloudDrive::put_if(self, key, data, if_match)
    }

    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        Ok(CloudDrive::head(self, key)?.map(|(size, _)| size))
    }

    fn delete(&self, key: &str) -> CloudResult<()> {
        CloudDrive::delete(self, key)
    }

    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        self.run("list", |b| RemoteStore::list(b, prefix))
    }
}
