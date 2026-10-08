//! One bucket of a drive with a transport preference (AZDRIVE-INTEGRATION.md
//! §3): iroh first when a node's iroh id is known, HTTPS as the fallback.
//!
//! - `auto` (the default): when this build has iroh and a node to dial, one
//!   cheap signed request (a HEAD) probes it; an answer of any status means
//!   the pipe works and the session goes over iroh. No answer within 10 s, or
//!   an error, means HTTPS, and `transport.json` remembers it: the next runs
//!   take HTTPS straight away and try iroh again five minutes after the
//!   failure. A request that fails over iroh later is retried over HTTPS -
//!   unless a second probe shows iroh is fine, i.e. the error was the
//!   server's answer and HTTPS would only repeat it.
//! - `iroh`: iroh or nothing (the e2e run's iroh leg must not pass over
//!   HTTPS by accident).
//! - `https`: HTTPS only (the block endpoint, then the direct node URLs and
//!   what `x-azlin-alt-endpoints` taught; azlin-client's failover).
//!
//! Where the iroh node comes from: `--iroh-node` / `AZLIN_IROH_NODE` (with
//! `--iroh-addr` / `AZLIN_IROH_ADDR`, the node's UDP socket, so no discovery
//! is needed), else the node list of the last credential refresh when it
//! names an `iroh_id` (or `sign_pubkey`) and `iroh_addrs`. The token server
//! does not list them yet (`drives::node_list` has no such field), so today
//! only the pinned node is dialled. The client's relay is the configured one
//! (`endpoints.relay`): `off`, `default` (n0's relays and discovery) or an
//! address (the local `iroh-relay --dev`).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use azlin_client::{Bucket, ListResult};
use azlin_proto::s3req::{parse_error, ListedObject};
use azul_appkit::azlin_config::{Endpoint, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    account::Account,
    now,
    settings::Settings,
    state::{read_json, write_json},
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
/// `iroh_built` says whether this build has the `iroh` feature.
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

/// The answer of a conditional GET.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conditional {
    NotFound,
    /// The object still has the ETag given.
    NotModified,
    Found {
        body: Vec<u8>,
        etag: Option<String>,
    },
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// One bucket of a drive, over the transport this session chose.
pub struct Drive {
    https: Bucket,
    iroh: Option<Bucket>,
    lane: Mutex<Lane>,
    pref: TransportPref,
    report: Mutex<TransportReport>,
    memory_path: PathBuf,
}

impl std::fmt::Debug for Drive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drive")
            .field("bucket", &self.https.bucket)
            .field("endpoint", &self.https.endpoint)
            .field("lane", &self.lane())
            .finish_non_exhaustive()
    }
}

impl Drive {
    /// The drive of `account` (its credentials should be fresh:
    /// [`Account::ensure_fresh`]), over the transport `settings` ask for.
    ///
    /// # Errors
    ///
    /// When the credentials are missing, or iroh was asked for and fails.
    pub async fn open(account: &Account, settings: &Settings) -> Result<Drive> {
        let creds = account.credentials()?;
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
        let mut https = Bucket::new(&endpoint, &record.bucket, creds.clone());
        https.alternatives = Arc::new(Mutex::new(record.node_urls()));
        let target = iroh_target(settings, &record.nodes);
        let memory_path = account.state().transport_file();
        let memory: Option<TransportMemory> = read_json(&memory_path).ok().flatten();
        let pref = settings.transport_pref();
        let iroh_built = cfg!(feature = "iroh");
        let decision = decide(pref, iroh_built, target.as_ref(), memory.as_ref(), now())
            .map_err(|e| anyhow!(e))?;
        let mut report = TransportReport {
            preference: pref,
            lane: Lane::Https,
            reason: String::new(),
            iroh_built,
            iroh_target: target.clone(),
            relay: settings.relay().map(String::from),
            endpoint: crate::settings::redact_url(&endpoint),
            endpoint_source,
            probed: false,
            fell_back: None,
        };
        let mut iroh = None;
        match (decision, target.as_ref()) {
            (Decision::Probe, Some(target)) => {
                report.probed = true;
                match try_iroh(&endpoint, &record.bucket, &creds, target, settings.relay()).await {
                    Ok(bucket) => {
                        iroh = Some(bucket);
                        report.lane = Lane::Iroh;
                        report.reason = format!("iroh answered ({}, {})", target.id, target.source);
                        remember(&memory_path, Lane::Iroh, &report.reason, None);
                    }
                    Err(e) if pref == TransportPref::Iroh => {
                        bail!("iroh was asked for and failed: {e}");
                    }
                    Err(e) => {
                        report.reason = format!(
                            "iroh failed ({e}); https instead, iroh again in {IROH_RETRY_SECS} s"
                        );
                        remember(&memory_path, Lane::Https, &report.reason, Some(e));
                    }
                }
            }
            (Decision::Probe, None) => {
                report.reason = String::from(NO_IROH_NODE);
            }
            (Decision::Https(reason), _) => report.reason = reason,
        }
        Ok(Drive {
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
        self.report
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// The lane requests take now.
    #[must_use]
    pub fn lane(&self) -> Lane {
        *self.lane.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The bucket's name.
    #[must_use]
    pub fn bucket_name(&self) -> &str {
        &self.https.bucket
    }

    /// The HTTPS bucket (its endpoint and credentials: presigned links are
    /// HTTPS links whatever this session's transport).
    #[must_use]
    pub fn https_bucket(&self) -> &Bucket {
        &self.https
    }

    /// Sets how many parts or ranges of one big object are in flight.
    pub fn set_parallel(&mut self, parallel: usize) {
        let parallel = parallel.max(1);
        self.https.parallel = parallel;
        if let Some(iroh) = self.iroh.as_mut() {
            iroh.parallel = parallel;
        }
    }

    fn fall_back(&self, what: &str, error: &anyhow::Error) {
        *self.lane.lock().unwrap_or_else(|p| p.into_inner()) = Lane::Https;
        let reason = format!("{what} failed over iroh ({error:#}); https for the rest of the run");
        {
            let mut report = self.report.lock().unwrap_or_else(|p| p.into_inner());
            report.lane = Lane::Https;
            report.fell_back = Some(reason.clone());
        }
        remember(
            &self.memory_path,
            Lane::Https,
            &reason,
            Some(format!("{error:#}")),
        );
    }

    /// Runs `f` on the bucket of the current lane. Over iroh, an error is
    /// retried over HTTPS (`auto` only) when a probe shows iroh itself
    /// failed; an error iroh carried fine is the server's and is returned.
    async fn run<'a, T, F, Fut>(&'a self, what: &str, f: F) -> Result<T>
    where
        F: Fn(&'a Bucket) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let iroh = match (self.lane(), self.iroh.as_ref()) {
            (Lane::Iroh, Some(iroh)) => iroh,
            _ => return f(&self.https).await,
        };
        match f(iroh).await {
            Ok(value) => Ok(value),
            Err(e) if self.pref == TransportPref::Auto => {
                if probe(iroh).await.is_ok() {
                    return Err(e);
                }
                self.fall_back(what, &e);
                f(&self.https).await
            }
            Err(e) => Err(e),
        }
    }

    /// PUT (multipart above twice the part size); the ETag.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn put(&self, key: &str, data: Vec<u8>) -> Result<String> {
        self.run("put", |b| b.put(key, data.clone())).await
    }

    /// Conditional PUT: `If-Match: <etag>`, or `If-None-Match: *` without
    /// one; `None` when another writer won (412).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn put_if(
        &self,
        key: &str,
        data: Vec<u8>,
        if_match: Option<&str>,
    ) -> Result<Option<String>> {
        self.run("conditional put", |b| b.put_if(key, data.clone(), if_match))
            .await
    }

    /// GET of a small object; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.run("get", |b| b.get(key)).await
    }

    /// GET with parallel ranges for a big object (a HEAD first).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn get_big(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.run("ranged get", |b| b.get_parallel(key)).await
    }

    /// HEAD: the size and ETag; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn head(&self, key: &str) -> Result<Option<(u64, String)>> {
        self.run("head", |b| b.head(key)).await
    }

    /// DELETE (a missing object is no error).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.run("delete", |b| b.delete(key)).await
    }

    /// One page of a listing.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn list_page(&self, prefix: &str, token: Option<&str>) -> Result<ListResult> {
        self.run("list", |b| b.list(prefix, token)).await
    }

    /// Every object under `prefix`, page by page.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn list_all(&self, prefix: &str) -> Result<Vec<ListedObject>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page = self.list_page(prefix, token.as_deref()).await?;
            out.extend(page.contents);
            match page.next_token {
                Some(next) if page.is_truncated && !next.is_empty() => token = Some(next),
                _ => return Ok(out),
            }
        }
    }

    /// A conditional GET: `If-None-Match: <etag>` when one is given (the
    /// index's poll: a 304 means nothing changed).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub async fn get_unless(&self, key: &str, etag: Option<&str>) -> Result<Conditional> {
        self.run("conditional get", |b| async move {
            let extra: Vec<(String, String)> = etag
                .map(|e| vec![(String::from("if-none-match"), e.to_string())])
                .unwrap_or_default();
            let (status, headers, body) = match b
                .send(|c| c.get_object(now(), key, None, extra.clone()))
                .await
            {
                Ok(reply) => reply,
                Err(e) => return Err(e),
            };
            match status {
                200 => Ok(Conditional::Found {
                    etag: header(&headers, "etag").map(String::from),
                    body,
                }),
                304 => Ok(Conditional::NotModified),
                404 => Ok(Conditional::NotFound),
                _ => {
                    let e = parse_error(&String::from_utf8_lossy(&body));
                    Err(anyhow!(
                        "get {key}: {status} {} {}{}",
                        e.code,
                        e.message,
                        header(&headers, "x-azlin-error")
                            .map(|x| format!(" (x-azlin-error {x})"))
                            .unwrap_or_default()
                    ))
                }
            }
        })
        .await
    }

    /// Closes the iroh endpoint, if there is one (a polite goodbye to the
    /// node instead of a timeout on its side).
    pub async fn close(&self) {
        close_iroh(self.iroh.as_ref()).await;
    }

    /// The source of the S3 endpoint, for messages.
    #[must_use]
    pub fn endpoint_label(&self) -> String {
        let report = self.transport();
        format!("{} ({})", report.endpoint, report.endpoint_source)
    }
}

#[cfg(feature = "iroh")]
async fn close_iroh(bucket: Option<&Bucket>) {
    if let Some(bucket) = bucket {
        if let azlin_client::Transport::Iroh { endpoint, .. } = &bucket.transport {
            endpoint.close().await;
        }
    }
}

#[cfg(not(feature = "iroh"))]
async fn close_iroh(_bucket: Option<&Bucket>) {}

/// Keeps the decision in `transport.json` (best effort: a failed write only
/// costs the next run a probe).
fn remember(path: &std::path::Path, lane: Lane, reason: &str, iroh_error: Option<String>) {
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

/// One signed HEAD of a key that never exists: any answer means the pipe
/// works.
async fn probe(bucket: &Bucket) -> Result<(), String> {
    let request = bucket.send(|c| c.head_object(now(), PROBE_KEY));
    match tokio::time::timeout(PROBE_TIMEOUT, request).await {
        Err(_) => Err(format!("no answer within {} s", PROBE_TIMEOUT.as_secs())),
        Ok(Err(e)) => Err(format!("{e:#}")),
        Ok(Ok(_)) => Ok(()),
    }
}

/// The client endpoint with the configured relay: `default` = n0's relays and
/// discovery (`presets::N0`), an address = that relay only, `off` (or none) =
/// no relay, direct addresses only.
#[cfg(feature = "iroh")]
async fn iroh_endpoint(relay: Option<&str>) -> Result<iroh::Endpoint> {
    use iroh::endpoint::presets;
    let builder = match relay {
        Some("default") => iroh::Endpoint::builder(presets::N0),
        Some("off") | None => {
            iroh::Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled)
        }
        Some(url) => {
            let url: iroh::RelayUrl = url
                .parse()
                .map_err(|e| anyhow!("the relay {url} is no relay address: {e}"))?;
            iroh::Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::custom([url]))
        }
    };
    builder
        .bind()
        .await
        .map_err(|e| anyhow!("the iroh endpoint cannot start: {e}"))
}

/// The node's address: its id and, when known, its UDP socket.
#[cfg(feature = "iroh")]
fn iroh_addr(target: &IrohTarget) -> Result<iroh::EndpointAddr> {
    let id: iroh::EndpointId = target
        .id
        .trim()
        .parse()
        .map_err(|e| anyhow!("{} is no iroh endpoint id: {e}", target.id))?;
    let mut addr = iroh::EndpointAddr::new(id);
    if let Some(socket) = target.addr.as_deref() {
        let socket: std::net::SocketAddr = socket
            .parse()
            .map_err(|e| anyhow!("{socket} is no ip:port: {e}"))?;
        addr = addr.with_ip_addr(socket);
    }
    Ok(addr)
}

/// A bucket over iroh to `target`, probed.
#[cfg(feature = "iroh")]
async fn try_iroh(
    endpoint: &str,
    bucket: &str,
    creds: &azlin_proto::creds::Credentials,
    target: &IrohTarget,
    relay: Option<&str>,
) -> Result<Bucket, String> {
    let addr = iroh_addr(target).map_err(|e| format!("{e:#}"))?;
    let ep = iroh_endpoint(relay).await.map_err(|e| format!("{e:#}"))?;
    let bucket = Bucket::new(endpoint, bucket, creds.clone()).with_transport(
        azlin_client::Transport::Iroh {
            endpoint: ep.clone(),
            node: addr,
        },
    );
    if let Err(e) = probe(&bucket).await {
        ep.close().await;
        return Err(e);
    }
    Ok(bucket)
}

/// Without the `iroh` feature there is no iroh to try.
#[cfg(not(feature = "iroh"))]
async fn try_iroh(
    _endpoint: &str,
    _bucket: &str,
    _creds: &azlin_proto::creds::Credentials,
    _target: &IrohTarget,
    _relay: Option<&str>,
) -> Result<Bucket, String> {
    Err(String::from(
        "this azcloud was built without the iroh feature",
    ))
}

/// Where the S3 endpoint of a session comes from, for `azcloud status`.
#[must_use]
pub fn endpoint_source(settings: &Settings) -> Option<Source> {
    let r = settings.endpoints.get(Endpoint::S3);
    r.value.as_ref().map(|_| r.source.clone())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn target() -> IrohTarget {
        IrohTarget {
            id: "ab".repeat(32),
            addr: Some(String::from("127.0.0.1:41000")),
            source: String::from("environment AZLIN_IROH_NODE"),
        }
    }

    #[test]
    fn https_is_taken_when_asked_for_without_iroh_or_without_a_node_to_dial() {
        let t = target();
        assert!(matches!(
            decide(TransportPref::Https, true, Some(&t), None, 0),
            Ok(Decision::Https(_))
        ));
        assert!(matches!(
            decide(TransportPref::Auto, false, Some(&t), None, 0),
            Ok(Decision::Https(_))
        ));
        match decide(TransportPref::Auto, true, None, None, 0) {
            Ok(Decision::Https(reason)) => assert!(reason.contains("iroh id"), "{reason}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn auto_probes_iroh_and_after_a_failure_keeps_to_https_for_five_minutes() {
        let t = target();
        assert_eq!(
            decide(TransportPref::Auto, true, Some(&t), None, 1000),
            Ok(Decision::Probe)
        );
        let failed = TransportMemory {
            lane: Lane::Https,
            reason: String::from("iroh failed"),
            decided_at: 1000,
            iroh_failed_at: Some(1000),
            iroh_error: Some(String::from("no answer within 10 s")),
        };
        match decide(TransportPref::Auto, true, Some(&t), Some(&failed), 1100) {
            Ok(Decision::Https(reason)) => {
                assert!(reason.contains("no answer within 10 s"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            decide(
                TransportPref::Auto,
                true,
                Some(&t),
                Some(&failed),
                1000 + IROH_RETRY_SECS
            ),
            Ok(Decision::Probe),
            "five minutes later iroh is tried again"
        );
        let worked = TransportMemory {
            lane: Lane::Iroh,
            iroh_failed_at: None,
            iroh_error: None,
            ..failed
        };
        assert_eq!(
            decide(TransportPref::Auto, true, Some(&t), Some(&worked), 1100),
            Ok(Decision::Probe)
        );
    }

    #[test]
    fn iroh_only_is_an_error_when_it_cannot_be_had_and_never_a_silent_https() {
        let t = target();
        assert!(decide(TransportPref::Iroh, false, Some(&t), None, 0).is_err());
        assert!(decide(TransportPref::Iroh, true, None, None, 0).is_err());
        let failed = TransportMemory {
            lane: Lane::Https,
            reason: String::new(),
            decided_at: 0,
            iroh_failed_at: Some(0),
            iroh_error: None,
        };
        assert_eq!(
            decide(TransportPref::Iroh, true, Some(&t), Some(&failed), 1),
            Ok(Decision::Probe),
            "the memory of a failure does not stop an iroh-only run"
        );
    }

    #[test]
    fn a_node_list_that_names_iroh_ids_gives_the_first_ready_node_with_its_ipv4_socket() {
        let nodes = vec![
            json!({"name": "n1", "url": "http://127.0.0.1:9001", "ready": false, "iroh_id": "aa"}),
            json!({"name": "n2", "url": "http://127.0.0.1:9002", "ready": true}),
            json!({"name": "n3", "ready": true, "sign_pubkey": "cc",
                   "iroh_addrs": ["[::1]:4433", "127.0.0.1:4433"]}),
        ];
        let t = target_from_nodes(&nodes).expect("n3");
        assert_eq!(t.id, "cc");
        assert_eq!(t.addr.as_deref(), Some("127.0.0.1:4433"));
        assert!(t.source.contains("n3"));
        assert_eq!(
            target_from_nodes(&[json!({"name": "n1", "url": "http://x"})]),
            None,
            "today's token server lists no iroh ids"
        );
    }

    #[test]
    fn the_transport_names_parse_in_any_case() {
        assert_eq!(TransportPref::parse(" HTTPS "), Some(TransportPref::Https));
        assert_eq!(TransportPref::parse("http"), Some(TransportPref::Https));
        assert_eq!(TransportPref::parse("quic"), Some(TransportPref::Iroh));
        assert_eq!(TransportPref::parse("auto"), Some(TransportPref::Auto));
        assert_eq!(TransportPref::parse("tcp"), None);
    }
}
