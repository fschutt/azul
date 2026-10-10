//! Which transport a drive's requests take: iroh first when a node's iroh id is known, HTTPS
//! as the fallback - one lane logic for every app. [`IrohLane`] is the drive's
//! [`crate::failover::Failover`]'s layer before the block endpoint: AzDrive's
//! [`crate::AzlinDrive::with_iroh`], the command line's [`CloudDrive`] and the bridge go through
//! it. A plain S3 bucket (AWS, R2, MinIO) has no node list and no lane: it is HTTPS only.
//!
//! - The nodes it dials: every ready node of the last credential refresh that names its
//!   `iroh_id` (or `sign_pubkey`), at every socket of its `iroh_addrs` - no discovery and no DNS
//!   needed - in the list's order (healthiest first). `--iroh-node` / `AZLIN_IROH_NODE` (with
//!   `--iroh-addr` / `AZLIN_IROH_ADDR`, comma-separated sockets) pins one instead.
//! - Each node is probed once before its first request: one signed HEAD of a key that never
//!   exists, waited for at most [`PROBE_TIMEOUT`]; any answer means the pipe works.
//! - A node that does not answer rests for [`IROH_RETRY_SECS`] (its own backoff: the other nodes
//!   are still asked) and the request goes on to the next node, then over HTTPS (the block
//!   endpoint, the nodes' URLs, the failover URLs, the nodes' addresses). An answer iroh carried
//!   is the service's: a refusal is not repeated over HTTPS; a busy one goes on.
//! - `transport.json` (the command line's state folder) remembers the nodes that failed, so the
//!   next run takes HTTPS at once until their rest is over.
//!
//! The preference (`--transport` / `AZCLOUD_TRANSPORT`): `auto` (the default) as above; `iroh`:
//! iroh or nothing (an end-to-end run's iroh leg must not pass over HTTPS by accident); `https`:
//! no lane. The client's relay is the configured one (`endpoints.relay`): `off`, `default` or an
//! address.
//!
//! The kit links no iroh: "S3 over iroh" sends the same signed HTTP requests over a QUIC
//! stream, so an app that links iroh plugs in an [`IrohDialer`] that makes an azul-storage
//! [`azul_storage::Transport`] to a node, and the kit does the rest.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use azul_appkit::azlin_config::Endpoint;
use azul_storage::{s3::Routed, HttpCall, HttpReply, Method, ObjectInfo, S3Config, Transport};
use serde::{Deserialize, Serialize};

use crate::{
    account::Account,
    bucket::Bucket,
    error::{fail, CloudError, CloudResult},
    failover::Node,
    now,
    settings::{redact_url, Settings},
    state::{read_json, write_json},
    store::{Conditional, RemoteObject, RemoteStore, BIG_BLOB},
};

/// How long a probe waits for iroh's answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// After a node failed over iroh, how long it rests before it is dialed again.
pub const IROH_RETRY_SECS: i64 = 300;
/// The key a probe asks for: never written (`.azlin/` is bookkeeping), so the
/// answer is a 404 over a working pipe.
pub const PROBE_KEY: &str = ".azlin/probe";
/// Why `auto` cannot use iroh without a node to dial.
pub const NO_IROH_NODE: &str = "no node's iroh id is known: pass --iroh-node (AZLIN_IROH_NODE); \
                                the token server's node list names no iroh_id";

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
    /// Its endpoint id: the node's Ed25519 key (`iroh_id`, `sign_pubkey`), hex or base32.
    pub id: String,
    /// Its iroh sockets (`ip:port`, IPv4 and IPv6) as the node list names them: dialed without
    /// discovery. Empty: the dialer's discovery or relay has to find it.
    pub addrs: Vec<String>,
    /// Where the two came from.
    pub source: String,
}

/// A node that failed over iroh, as `transport.json` keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMemory {
    pub failed_at: i64,
    pub error: String,
}

/// What `transport.json` keeps between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportMemory {
    pub lane: Lane,
    pub reason: String,
    pub decided_at: i64,
    /// When iroh last failed with no node left to dial (`auto` keeps to HTTPS for five minutes
    /// after).
    #[serde(default)]
    pub iroh_failed_at: Option<i64>,
    #[serde(default)]
    pub iroh_error: Option<String>,
    /// The nodes that failed, by iroh id: each rests for five minutes.
    #[serde(default)]
    pub nodes: BTreeMap<String, NodeMemory>,
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
/// `iroh_built` says whether this build can dial iroh; `target` is the first node to dial.
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

/// The node `settings` pin (`--iroh-node` / `AZLIN_IROH_NODE`, at the comma-separated sockets of
/// `--iroh-addr` / `AZLIN_IROH_ADDR`), if any.
#[must_use]
pub fn pinned_target(settings: &Settings) -> Option<IrohTarget> {
    let id = settings.iroh_node.value.as_deref()?;
    let addrs = settings
        .iroh_addr
        .value
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(String::from)
        .collect();
    Some(IrohTarget {
        id: id.to_string(),
        addrs,
        source: settings.iroh_node.source.label(),
    })
}

/// The nodes to dial: the pinned one, else every ready node of the node list that names its
/// iroh id ([`targets_from_nodes`]).
#[must_use]
pub fn iroh_targets(settings: &Settings, nodes: &[Node]) -> Vec<IrohTarget> {
    match pinned_target(settings) {
        Some(pinned) => vec![pinned],
        None => targets_from_nodes(nodes),
    }
}

/// Every ready node of a node list that names its iroh id, in the list's order, with all its
/// iroh sockets.
#[must_use]
pub fn targets_from_nodes(nodes: &[Node]) -> Vec<IrohTarget> {
    nodes
        .iter()
        .filter(|n| n.ready)
        .filter_map(|n| {
            let id = n.iroh_id.as_deref().filter(|id| !id.trim().is_empty())?;
            Some(IrohTarget {
                id: id.trim().to_string(),
                addrs: n.iroh_addrs.clone(),
                source: format!(
                    "the node list ({})",
                    if n.name.is_empty() {
                        "a node"
                    } else {
                        n.name.as_str()
                    }
                ),
            })
        })
        .collect()
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

/// Dials a node's S3 over iroh: what an app that links iroh plugs into an [`IrohLane`]. The
/// transport it makes sends the same signed requests the HTTPS one does - one request and its
/// answer per QUIC stream - to the node `target` at `target.addrs`, through `relay` when one is
/// configured (`off`, `default` or an address). Dialing may connect lazily (on the first
/// request); dropping the transport closes the connection.
pub trait IrohDialer: Send + Sync {
    /// A transport to `target`, ready to send; why not, otherwise.
    fn dial(&self, target: &IrohTarget, relay: Option<&str>) -> Result<Box<dyn Transport>, String>;
}

/// A closure dials as well: `|target, relay| ...`, an app's few lines over its iroh endpoint
/// (AzDrive's and the bridge's over azul's: `azul_storage::azul_iroh::dial`).
impl<F> IrohDialer for F
where
    F: Fn(&IrohTarget, Option<&str>) -> Result<Box<dyn Transport>, String> + Send + Sync,
{
    fn dial(&self, target: &IrohTarget, relay: Option<&str>) -> Result<Box<dyn Transport>, String> {
        self(target, relay)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `call` through `transport`, waited for at most `timeout` (the probe: any answer means the
/// pipe works; a request that hangs keeps its thread until the transport gives up).
fn send_within(
    transport: &Arc<dyn Transport>,
    call: HttpCall,
    timeout: Duration,
) -> Result<HttpReply, String> {
    let (answered, answer) = mpsc::channel();
    let sending = transport.clone();
    std::thread::spawn(move || {
        let _ = answered.send(sending.send(&call));
    });
    match answer.recv_timeout(timeout) {
        Ok(result) => result,
        Err(_) => Err(format!("no answer within {} s", timeout.as_secs())),
    }
}

/// The verb of a request, for people (`put`, `get`).
fn verb(method: Method) -> String {
    method.as_str().to_ascii_lowercase()
}

/// One node's state in a lane.
#[derive(Default)]
struct NodeLane {
    /// Its transport, once dialed (dropped when it fails: dialed again after its rest).
    transport: Option<Arc<dyn Transport>>,
    /// It answered over iroh (a probe or a request) since it was dialed.
    answered: bool,
    failed_at: Option<i64>,
    error: Option<String>,
}

/// What a lane did last, for `azcloud transport` and the apps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LaneReport {
    /// The lane the last request took (`None`: no request yet).
    pub lane: Option<Lane>,
    /// Why.
    pub reason: String,
    /// A probe was sent.
    pub probed: bool,
    /// The node that answered last, else the last one tried.
    pub target: Option<IrohTarget>,
    /// Set when a request fell back to HTTPS after its node had answered over iroh.
    pub fell_back: Option<String>,
}

/// What a lane made of one request.
pub(crate) enum LaneAnswer {
    /// A node answered over iroh, and the answer is the request's.
    Answered(HttpReply),
    /// The nodes that answered were busy (or the drive moved): the last such answer.
    Busy(HttpReply),
    /// No node answered over iroh: why.
    Failed(String),
    /// No node to dial (none names an iroh id, or every one rests).
    Skipped,
}

/// Now, in seconds since 1970 (a node's rest).
pub type LaneClock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The iroh lane of one drive: the failover's layer before the block endpoint (see the module).
/// Shared by every request of the drive.
pub struct IrohLane {
    dialer: Arc<dyn IrohDialer>,
    relay: Option<String>,
    /// iroh or nothing: no HTTPS after a failure.
    required: bool,
    /// The node `--iroh-node` pins, dialed instead of the node list's.
    pinned: Option<IrohTarget>,
    probe_timeout: Duration,
    nodes: Mutex<BTreeMap<String, NodeLane>>,
    report: Mutex<LaneReport>,
    /// Where the failures are kept between runs, if anywhere.
    memory: Option<PathBuf>,
    clock: LaneClock,
}

impl fmt::Debug for IrohLane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IrohLane")
            .field("relay", &self.relay)
            .field("required", &self.required)
            .field("pinned", &self.pinned)
            .field("report", &self.report())
            .finish_non_exhaustive()
    }
}

impl IrohLane {
    /// A lane that dials through `dialer`, relayed through `relay` (`off`, `default` or an
    /// address; `None`: the dialer's own).
    #[must_use]
    pub fn new(dialer: Arc<dyn IrohDialer>, relay: Option<&str>) -> IrohLane {
        IrohLane {
            dialer,
            relay: relay.map(String::from),
            required: false,
            pinned: None,
            probe_timeout: PROBE_TIMEOUT,
            nodes: Mutex::new(BTreeMap::new()),
            report: Mutex::new(LaneReport::default()),
            memory: None,
            clock: Arc::new(now),
        }
    }

    /// iroh or nothing: a request no node answers over iroh fails instead of going over HTTPS.
    #[must_use]
    pub fn required(mut self, required: bool) -> IrohLane {
        self.required = required;
        self
    }

    /// Dials `pinned` instead of the node list's nodes.
    #[must_use]
    pub fn with_pinned(mut self, pinned: Option<IrohTarget>) -> IrohLane {
        self.pinned = pinned;
        self
    }

    /// Waits this long for a probe's answer instead of [`PROBE_TIMEOUT`].
    #[must_use]
    pub fn with_probe_timeout(mut self, timeout: Duration) -> IrohLane {
        self.probe_timeout = timeout;
        self
    }

    /// Reads the time from `clock` (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: LaneClock) -> IrohLane {
        self.clock = clock;
        self
    }

    /// Keeps the failures in `path` (`transport.json`), and starts with the ones `memory` holds:
    /// a node that failed less than five minutes ago still rests.
    #[must_use]
    pub fn with_memory(mut self, path: &Path, memory: Option<&TransportMemory>) -> IrohLane {
        if let Some(memory) = memory {
            let mut nodes = lock(&self.nodes);
            for (id, failed) in &memory.nodes {
                nodes.insert(
                    id.clone(),
                    NodeLane {
                        failed_at: Some(failed.failed_at),
                        error: Some(failed.error.clone()),
                        ..NodeLane::default()
                    },
                );
            }
        }
        self.memory = Some(path.to_path_buf());
        self
    }

    /// Whether a request no node answers fails instead of going over HTTPS.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.required
    }

    /// The relay the lane dials through.
    #[must_use]
    pub fn relay(&self) -> Option<&str> {
        self.relay.as_deref()
    }

    /// What the lane did last.
    #[must_use]
    pub fn report(&self) -> LaneReport {
        lock(&self.report).clone()
    }

    /// The nodes the lane dials for a drive with `nodes`: the pinned one, else every ready node
    /// that names its iroh id.
    #[must_use]
    pub fn targets(&self, nodes: &[Node]) -> Vec<IrohTarget> {
        match &self.pinned {
            Some(pinned) => vec![pinned.clone()],
            None => targets_from_nodes(nodes),
        }
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    /// Whether the node `id` failed less than [`IROH_RETRY_SECS`] ago.
    fn resting(&self, id: &str, now: i64) -> bool {
        lock(&self.nodes)
            .get(id)
            .and_then(|node| node.failed_at)
            .is_some_and(|at| (0..IROH_RETRY_SECS).contains(&(now - at)))
    }

    /// The lane the next request of a drive with `nodes` takes: iroh while a node to dial is not
    /// resting (an iroh-only lane: whenever there is one).
    #[must_use]
    pub fn lane_now(&self, nodes: &[Node]) -> Lane {
        let now = self.now();
        let targets = self.targets(nodes);
        let open = targets
            .iter()
            .any(|t| self.required || !self.resting(&t.id, now));
        if open {
            Lane::Iroh
        } else {
            Lane::Https
        }
    }

    /// The node's transport, dialed on first use.
    fn transport_of(&self, target: &IrohTarget) -> Result<(Arc<dyn Transport>, bool), String> {
        if let Some(node) = lock(&self.nodes).get(&target.id) {
            if let Some(transport) = &node.transport {
                return Ok((transport.clone(), node.answered));
            }
        }
        let dialed: Arc<dyn Transport> =
            Arc::from(self.dialer.dial(target, self.relay.as_deref())?);
        let mut nodes = lock(&self.nodes);
        let node = nodes.entry(target.id.clone()).or_default();
        if let Some(transport) = &node.transport {
            // Another request dialed it meanwhile.
            return Ok((transport.clone(), node.answered));
        }
        node.transport = Some(dialed.clone());
        node.answered = false;
        Ok((dialed, false))
    }

    /// `target` answered over iroh (`probed`: the answer was a probe's).
    fn answered(&self, target: &IrohTarget, probed: bool) {
        let first = {
            let mut nodes = lock(&self.nodes);
            let node = nodes.entry(target.id.clone()).or_default();
            let first = !node.answered || node.failed_at.is_some();
            node.answered = true;
            node.failed_at = None;
            node.error = None;
            first
        };
        {
            let mut report = lock(&self.report);
            report.probed |= probed;
            let changed = report.lane != Some(Lane::Iroh)
                || report.target.as_ref().map(|t| &t.id) != Some(&target.id);
            if first || changed {
                report.reason = format!("iroh answered ({}, {})", target.id, target.source);
            }
            report.lane = Some(Lane::Iroh);
            report.target = Some(target.clone());
        }
        if first {
            self.remember(&[]);
        }
    }

    /// `what` failed over iroh at `target` with `error`: the node rests, its transport is
    /// dropped. `targets` are the nodes of the request (for the memory: whether any is left).
    fn failed(&self, target: &IrohTarget, what: &str, error: &str, targets: &[IrohTarget]) {
        let now = self.now();
        let had_answered = {
            let mut nodes = lock(&self.nodes);
            let node = nodes.entry(target.id.clone()).or_default();
            let had = node.answered;
            node.transport = None;
            node.answered = false;
            node.failed_at = Some(now);
            node.error = Some(error.to_string());
            had
        };
        {
            let mut report = lock(&self.report);
            report.reason =
                format!("iroh failed ({error}); https instead, iroh again in {IROH_RETRY_SECS} s");
            if had_answered {
                report.fell_back = Some(format!(
                    "{what} failed over iroh ({error}) at {}; it is dialed again in \
                     {IROH_RETRY_SECS} s",
                    target.id
                ));
            }
            report.target = Some(target.clone());
        }
        self.remember(targets);
    }

    /// Keeps the lane's state in `transport.json` (best effort: a failed write only costs the
    /// next run a probe). With no node of `targets` left to dial, the run's failure is kept too:
    /// the next run takes HTTPS at once.
    fn remember(&self, targets: &[IrohTarget]) {
        let Some(path) = &self.memory else {
            return;
        };
        let now = self.now();
        let report = self.report();
        let failures: BTreeMap<String, NodeMemory> = lock(&self.nodes)
            .iter()
            .filter_map(|(id, node)| {
                Some((
                    id.clone(),
                    NodeMemory {
                        failed_at: node.failed_at?,
                        error: node.error.clone().unwrap_or_default(),
                    },
                ))
            })
            .collect();
        let none_left = !targets.is_empty() && targets.iter().all(|t| self.resting(&t.id, now));
        let memory = TransportMemory {
            lane: report.lane.unwrap_or(Lane::Https),
            reason: report.reason.clone(),
            decided_at: now,
            iroh_failed_at: none_left.then_some(now),
            iroh_error: none_left.then(|| report.reason.clone()),
            nodes: failures,
        };
        let _ = write_json(path, &memory, false);
    }

    /// The request gave up on iroh: the report says HTTPS, for `reason` when given.
    fn took_https(&self, reason: Option<&str>) {
        let mut report = lock(&self.report);
        report.lane = Some(Lane::Https);
        if let Some(reason) = reason {
            report.reason = reason.to_string();
        }
    }

    /// Sends `request` (signed for the drive's endpoint `block`) over iroh to the nodes of
    /// `nodes` in turn: a node not dialed yet is dialed, one that has not answered yet is probed
    /// first, one that fails rests. `is_final` says whether an answer is the request's (else the
    /// next node is asked).
    pub(crate) fn send(
        &self,
        request: &Routed<'_>,
        block: &str,
        nodes: &[Node],
        is_final: &mut dyn FnMut(&HttpReply) -> bool,
    ) -> LaneAnswer {
        let targets = self.targets(nodes);
        if targets.is_empty() {
            self.took_https(Some(NO_IROH_NODE));
            return if self.required {
                LaneAnswer::Failed(String::from(NO_IROH_NODE))
            } else {
                LaneAnswer::Skipped
            };
        }
        let what = verb(request.method());
        // The drive's own probe (CloudDrive's first request) is sent as it is, within the
        // probe's time.
        let is_probe = request.method() == Method::Head && request.key() == Some(PROBE_KEY);
        let now = self.now();
        let mut busy: Option<HttpReply> = None;
        let mut last_error: Option<String> = None;
        for target in &targets {
            if !self.required && self.resting(&target.id, now) {
                continue;
            }
            let (transport, answered) = match self.transport_of(target) {
                Ok(dialed) => dialed,
                Err(e) => {
                    self.failed(target, "the dial", &e, &targets);
                    last_error = Some(e);
                    continue;
                }
            };
            if !answered && !is_probe {
                let probe = match request.probe_for(block, PROBE_KEY) {
                    Ok(probe) => probe,
                    Err(e) => {
                        // A request that cannot sign a probe goes over HTTPS.
                        self.took_https(None);
                        return LaneAnswer::Failed(e.to_string());
                    }
                };
                lock(&self.report).probed = true;
                if let Err(e) = send_within(&transport, probe, self.probe_timeout) {
                    self.failed(target, "the probe", &e, &targets);
                    last_error = Some(e);
                    continue;
                }
                self.answered(target, true);
            }
            let call = match request.signed_for(block) {
                Ok(call) => call,
                Err(e) => {
                    self.took_https(None);
                    return LaneAnswer::Failed(e.to_string());
                }
            };
            let sent = if is_probe {
                lock(&self.report).probed = true;
                send_within(&transport, call, self.probe_timeout)
            } else {
                transport.send(&call)
            };
            match sent {
                Ok(reply) => {
                    self.answered(target, is_probe);
                    if is_final(&reply) {
                        return LaneAnswer::Answered(reply);
                    }
                    busy = Some(reply);
                }
                Err(e) => {
                    self.failed(target, &what, &e, &targets);
                    last_error = Some(e);
                }
            }
        }
        if let Some(reply) = busy {
            return LaneAnswer::Busy(reply);
        }
        self.took_https(None);
        match last_error {
            Some(e) => LaneAnswer::Failed(e),
            None if self.required => {
                LaneAnswer::Failed(String::from("every node that names an iroh id rests"))
            }
            None => LaneAnswer::Skipped,
        }
    }
}

/// One bucket of an account's drive over the transport this run chose: through the iroh lane
/// first when it was asked for (or `auto` found a node answering the probe), HTTPS otherwise and
/// as the fallback - both in the bucket's one failover. A [`RemoteStore`]: the sync and the
/// shares talk to it as to any bucket. Blocking: call it from an azul `Thread`.
pub struct CloudDrive {
    bucket: Bucket,
    lane: Option<Arc<IrohLane>>,
    /// The parts of the report that do not change during the run.
    report: TransportReport,
}

impl fmt::Debug for CloudDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudDrive")
            .field("bucket", &self.bucket.config().bucket)
            .field("endpoint", &self.bucket.config().endpoint)
            .field("lane", &self.lane())
            .finish_non_exhaustive()
    }
}

impl CloudDrive {
    /// The drive of `account` (its credentials should be fresh: [`Account::ensure_fresh`]), over
    /// the transport `settings` ask for. HTTPS goes through the account's transports; iroh
    /// through `dialer` when the build can dial it (`None`: HTTPS only). With a lane, one probe
    /// is sent at once (`azcloud transport` reports it).
    ///
    /// # Errors
    ///
    /// When the credentials are missing, the endpoint or the bucket cannot be one, or iroh was
    /// asked for and fails.
    pub fn open(
        account: &Account,
        settings: &Settings,
        dialer: Option<Arc<dyn IrohDialer>>,
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
        let bucket = Bucket::new(config, credentials, account.transports().clone())?
            .with_nodes(Node::list(&record.nodes))
            .with_alternatives(record.failover.clone());
        let targets = iroh_targets(settings, &bucket.failover().nodes());
        let memory_path = account.state().transport_file();
        let memory: Option<TransportMemory> = read_json(&memory_path).ok().flatten();
        let pref = settings.transport_pref();
        let iroh_built = dialer.is_some();
        let decision = decide(pref, iroh_built, targets.first(), memory.as_ref(), now())
            .map_err(CloudError::Failed)?;
        let mut report = TransportReport {
            preference: pref,
            lane: Lane::Https,
            reason: String::new(),
            iroh_built,
            iroh_target: targets.first().cloned(),
            relay: settings.relay().map(String::from),
            endpoint: redact_url(&endpoint),
            endpoint_source,
            probed: false,
            fell_back: None,
        };
        let mut lane = None;
        match (decision, dialer) {
            (Decision::Probe, Some(dialer)) if !targets.is_empty() => {
                let iroh = Arc::new(
                    IrohLane::new(dialer, settings.relay())
                        .required(pref == TransportPref::Iroh)
                        .with_pinned(pinned_target(settings))
                        .with_memory(&memory_path, memory.as_ref()),
                );
                bucket.failover().set_lane(Some(iroh.clone()));
                // The probe, through the lane (and on to HTTPS when iroh does not answer).
                let probed = bucket.head(PROBE_KEY);
                if pref == TransportPref::Iroh && iroh.report().lane != Some(Lane::Iroh) {
                    let why = match probed {
                        Err(e) => e.to_string(),
                        Ok(_) => iroh.report().reason,
                    };
                    fail!("iroh was asked for and failed: {why}");
                }
                lane = Some(iroh);
            }
            (Decision::Probe, _) => report.reason = String::from(NO_IROH_NODE),
            (Decision::Https(reason), _) => report.reason = reason,
        }
        Ok(CloudDrive {
            bucket,
            lane,
            report,
        })
    }

    /// The transport now and why.
    #[must_use]
    pub fn transport(&self) -> TransportReport {
        let mut report = self.report.clone();
        if let Some(lane) = &self.lane {
            let now = lane.report();
            report.lane = self.lane();
            if !now.reason.is_empty() {
                report.reason = now.reason;
            }
            report.probed = now.probed;
            report.fell_back = now.fell_back;
            if now.target.is_some() {
                report.iroh_target = now.target;
            }
        }
        report
    }

    /// The lane requests take now.
    #[must_use]
    pub fn lane(&self) -> Lane {
        match &self.lane {
            Some(lane) => lane.lane_now(&self.bucket.failover().nodes()),
            None => Lane::Https,
        }
    }

    /// The bucket's name.
    #[must_use]
    pub fn bucket_name(&self) -> &str {
        self.bucket.name()
    }

    /// The bucket (its endpoint and credentials: a presigned link is an HTTPS link whatever this
    /// session's transport).
    #[must_use]
    pub fn https_bucket(&self) -> &Bucket {
        &self.bucket
    }

    /// Sets how many parts or ranges of one big object are in flight.
    pub fn set_parallel(&self, parallel: usize) {
        self.bucket.set_parallel(parallel);
    }

    /// The S3 endpoint and where it came from, for messages.
    #[must_use]
    pub fn endpoint_label(&self) -> String {
        let report = self.transport();
        format!("{} ({})", report.endpoint, report.endpoint_source)
    }

    /// Closes the iroh connections, if there are any (a goodbye to the nodes instead of a
    /// timeout on their side); dropping the drive does the same.
    pub fn close(self) {
        self.bucket.failover().set_lane(None);
    }

    /// PUT (multipart above twice the part size); the ETag.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        self.bucket.put(key, data)
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
        self.bucket.put_if(key, data, if_match)
    }

    /// GET of a small object; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn get(&self, key: &str) -> CloudResult<Option<Vec<u8>>> {
        self.bucket.get(key)
    }

    /// GET with ranges, several at once, for a big object (a HEAD first).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn get_big(&self, key: &str) -> CloudResult<Option<Vec<u8>>> {
        self.bucket.get_big(key)
    }

    /// PUT of what `body` reads, streamed (a big body in parts); the ETag. Each part goes
    /// through the lanes on its own: a part iroh does not carry goes over HTTPS.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn put_from(&self, key: &str, body: &mut dyn std::io::Read) -> CloudResult<String> {
        self.bucket.put_from(key, body)
    }

    /// PUT of the local file `path`: its parts several at once, resumable after the app was
    /// killed; the bytes sent.
    ///
    /// # Errors
    ///
    /// The file, the service's refusal, or no endpoint answers.
    pub fn put_file(
        &self,
        key: &str,
        path: &std::path::Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> CloudResult<u64> {
        self.bucket.put_file(key, path, progress)
    }

    /// GET of `key` into the file `dest`: ranges several at once into a hidden file next to it,
    /// resumed by the next download of the same version; the bytes written.
    ///
    /// # Errors
    ///
    /// The service's refusal, no endpoint answers, or the file cannot be written.
    pub fn download_to(
        &self,
        key: &str,
        dest: &std::path::Path,
        progress: &mut dyn FnMut(u64),
    ) -> CloudResult<u64> {
        self.bucket.download_to(key, dest, progress)
    }

    /// HEAD: the size and ETag; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn head(&self, key: &str) -> CloudResult<Option<(u64, String)>> {
        self.bucket.head(key)
    }

    /// DELETE (a missing object is no error).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn delete(&self, key: &str) -> CloudResult<()> {
        self.bucket.delete(key)
    }

    /// Every object under `prefix`, page by page.
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn list_all(&self, prefix: &str) -> CloudResult<Vec<ObjectInfo>> {
        self.bucket.list_all(prefix)
    }

    /// A conditional GET: `If-None-Match: <etag>` when one is given (the index's poll: a 304
    /// means nothing changed).
    ///
    /// # Errors
    ///
    /// The service's refusal, or no endpoint answers.
    pub fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.bucket.get_unless(key, etag)
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
        RemoteStore::list(&self.bucket, prefix)
    }

    fn put_from(&self, key: &str, body: &mut dyn std::io::Read, _size: u64) -> CloudResult<String> {
        CloudDrive::put_from(self, key, body)
    }

    fn fetch_to(&self, key: &str, size: u64, dest: &std::path::Path) -> CloudResult<bool> {
        RemoteStore::fetch_to(&self.bucket, key, size, dest)
    }
}
