//! The failover of an Azlin drive's requests (D35): the iroh lane, four HTTPS layers, tried in
//! order, and retries by the class of what went wrong.
//!
//! 0. **The iroh lane**, when the app plugged in a dialer ([`Failover::set_lane`],
//!    [`crate::transport::IrohLane`]): every ready node the node list names with an iroh id,
//!    dialed at its iroh sockets - no DNS at all. A node that fails rests for five minutes and
//!    the request goes on, to the next node and then to the layers below.
//! 1. **The block endpoint** - the drive's own (its DNS name balances over the healthy nodes).
//! 2. **The hint**: a node that cannot serve (draining, no majority, overloaded) answers `503`
//!    with `x-azlin-alt-endpoints`; the SAME request goes to the nodes it names at once (with
//!    `Retry-After: 0` there is no pause), and they are remembered for the next requests. Only
//!    a [`TrustedEndpoint`] is followed - one the token server's last answer listed - and never
//!    plain http for an https drive: a node cannot send a drive's requests, their bodies and
//!    the session token to a host of its choice. A hint or a move to an endpoint nobody listed
//!    asks the drive for a fresh view ([`Failover::take_view_refresh`]) instead.
//! 3. **The node list** of the last credential refresh ([`Node::list`]: ready nodes first, in
//!    the server's order), then the refresh's failover URLs. The endpoint that answered stays
//!    first for [`STICKY_SECS`].
//! 4. **The node's addresses**: as soon as the node list is known, every request first tells the
//!    transport where each node is reached when its name does not resolve
//!    ([`azul_storage::Transport::fallback_addresses`]), and the block host at every node's
//!    addresses (the nodes serve the block's name; its certificate covers them) - the request
//!    still goes to the name, so TLS verifies the certificate for it. With DNS down from the
//!    start, the first request already answers. A transport that reports a name that did not
//!    resolve ([`azul_storage::transport::is_dns_failure`]) is told the node's addresses once
//!    more and the request is sent again.
//!
//! What happens to an answer is what [`crate::answer`] reads in it (no table here): what passes
//! by itself (a busy or draining node, one that failed inside, no answer) is asked again at the
//! next endpoint and, after every endpoint, again under the kit's one policy ([`Retry`]): never
//! sooner than the backoff nor than the pause a node asked for (`Retry-After`), and a pause
//! longer than [`Policy::max_wait`] is the caller's to wait out (the answer is returned); a
//! drive that moved (`wrong_block`) is followed to the endpoint the answer names; a damaged
//! object is read once more from another node; everything the user must act on (a full drive,
//! a device signed out, a conflict) and every ordinary answer (404, 412, ...) is returned as it
//! is. A request no endpoint answered fails with the last endpoint's reason.
//!
//! Every request names the epoch of the cluster map the last refresh brought (`x-azlin-map`);
//! a node with a newer map answers 409 `map_stale` with its epoch, and the request goes to it
//! once more with that one (the drive takes the token server's view before its next request).
//! A request that may not be sent twice ([`Idempotency::Unsafe`]) whose outcome is unknown is
//! checked first: a completed multipart upload by the key's ETag.
//!
//! [`Failover`] is the [`Router`] the kit's [`crate::bucket::Bucket`] and
//! [`crate::drive::AzlinDrive`] hand their S3 drive: every request - each part of an upload, each
//! range of a download - goes through it on its own.

use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use azlin_contract::{map_epoch, AzlinError, MAP_HEADER};
use azul_storage::{
    s3::{Idempotency, Routed, Router, Verify},
    time::now_unix,
    transport::is_dns_failure,
    DriveError, HttpCall, HttpReply, S3Drive, Transport,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    answer::{Outcome, Spent},
    policy::Policy,
    transport::{IrohLane, LaneAnswer},
    user_errors::Behaviour,
};

/// How long the endpoint that answered stays first (seconds).
pub const STICKY_SECS: u64 = 600;
/// How often the endpoints are asked for one request, by default ([`Policy::NODE`]).
pub const ROUNDS: u32 = Policy::NODE.attempts;
/// The pause after the first round without an answer; it doubles every round.
pub const BACKOFF: Duration = Policy::NODE.base;
/// The longest `Retry-After` a request waits out itself.
pub const MAX_PAUSE: Duration = Policy::NODE.max_wait;
/// The most addresses one host is reached at (what azul's HTTP client keeps of a host).
pub const MAX_ADDRESSES: usize = 16;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An endpoint as the failover compares it: trimmed, without a trailing `/`.
fn normal(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Whether `url` would take a request of the drive at `block` from https to plain http.
fn downgrades(block: &str, url: &str) -> bool {
    let https = |u: &str| u.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://"));
    https(block) && !https(url)
}

/// An endpoint the token server named for the drive: its block endpoint, a node of its list, a
/// failover URL. Only these are followed when an answer names an endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedEndpoint(String);

impl TrustedEndpoint {
    /// The endpoints of a token server's answer: the drive's block endpoint, its nodes' URLs
    /// and its failover URLs.
    #[must_use]
    pub fn from_bundle(bundle: &crate::DriveBundle) -> Vec<TrustedEndpoint> {
        let block = bundle.entry.s3_config().map(|c| c.endpoint);
        block
            .into_iter()
            .chain(bundle.node_urls())
            .map(|url| TrustedEndpoint(normal(&url)))
            .filter(|t| !t.0.is_empty())
            .collect()
    }

    /// The URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.0
    }
}

/// The host of a URL (`n2.example`, an IPv6 address without its brackets), without its port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
            _ => authority,
        }
    };
    Some(host.to_ascii_lowercase()).filter(|h| !h.is_empty())
}

/// Whether `host` is an IP address (it needs no fallback address).
fn is_ip(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

/// Adds the strings of `value` (one string or a list of them), trimmed, to `out` once each.
fn add_strings(out: &mut Vec<String>, value: &Value) {
    let found: Vec<&str> = match value {
        Value::String(one) => vec![one.as_str()],
        Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    for one in found.into_iter().map(str::trim).filter(|a| !a.is_empty()) {
        if !out.iter().any(|a| a == one) {
            out.push(one.to_string());
        }
    }
}

/// One node of the drive, as the credential refresh lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    /// The node's own endpoint (`https://n2.<domain>`); empty for a node the list names only
    /// with its iroh id.
    #[serde(default)]
    pub url: String,
    /// Its IPv4 / IPv6 addresses: where it is reached when its name does not resolve.
    #[serde(default)]
    pub addresses: Vec<String>,
    /// It serves (the token server's health); an unknown state counts as ready.
    #[serde(default)]
    pub ready: bool,
    /// Its iroh endpoint id (the node's Ed25519 key), when the token server names one: the iroh
    /// lane dials it ([`crate::transport::IrohLane`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iroh_id: Option<String>,
    /// Its iroh sockets (`ip:port`, the node's fixed UDP port): where the lane dials it, without
    /// any discovery or DNS.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub iroh_addrs: Vec<String>,
}

impl Node {
    /// A node of the token server's list (`name`, `url` or `public_url`, `ipv4`, `ipv6`, `ready`,
    /// `iroh_id` or `sign_pubkey`, `iroh_addrs`); `None` without a URL or an iroh id.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Node> {
        let url = value["url"]
            .as_str()
            .or_else(|| value["public_url"].as_str())
            .map(normal)
            .unwrap_or_default();
        let iroh_id = value["iroh_id"]
            .as_str()
            .or_else(|| value["sign_pubkey"].as_str())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(String::from);
        if url.is_empty() && iroh_id.is_none() {
            return None;
        }
        let mut addresses: Vec<String> = Vec::new();
        for field in ["ipv4", "ipv6", "addresses"] {
            add_strings(&mut addresses, &value[field]);
        }
        let mut iroh_addrs: Vec<String> = Vec::new();
        add_strings(&mut iroh_addrs, &value["iroh_addrs"]);
        Some(Node {
            name: value["name"].as_str().unwrap_or_default().to_string(),
            url,
            addresses,
            ready: value["ready"].as_bool().unwrap_or(true),
            iroh_id,
            iroh_addrs,
        })
    }

    /// The nodes of a refresh's list: ready ones first, each group in the server's order
    /// (healthiest first); entries without a URL or an iroh id are left out.
    #[must_use]
    pub fn list(nodes: &[Value]) -> Vec<Node> {
        let mut out: Vec<Node> = nodes.iter().filter_map(Node::from_value).collect();
        out.sort_by_key(|node| !node.ready);
        out
    }
}

/// How often one request is tried, and its pauses: the kit's one policy - every endpoint is
/// asked once a round, [`Policy::attempts`] rounds.
pub type Retry = Policy;

/// How a pause is taken (the tests record it instead).
pub use crate::policy::Sleep;
/// Now, in seconds since 1970 (the sticky node's ten minutes).
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// What one answer means for the request.
enum Verdict {
    /// The answer of the request (a success, or one that is not tried again).
    Done,
    /// Ask the next endpoint; after the last one, again after a pause (`Retry-After`, if any).
    Again(Option<Duration>),
    /// Every node says the same (a client asked to slow down, a drive whose deletes are
    /// paused): no other node in this round, again after the pause.
    Wait(Option<Duration>),
    /// The drive is at this endpoint now.
    Moved(String),
}

/// Where each host of a drive is reached when its name does not resolve: each node's own
/// addresses, and the block host at all of them (ready nodes first: the nodes serve the block's
/// name, its certificate covers them). A host that is an IP address needs none.
#[must_use]
pub fn address_book(block: &str, nodes: &[Node]) -> Vec<(String, Vec<String>)> {
    let mut book: Vec<(String, Vec<String>)> = Vec::new();
    let mut every: Vec<String> = Vec::new();
    let ready_first = nodes
        .iter()
        .filter(|n| n.ready)
        .chain(nodes.iter().filter(|n| !n.ready));
    for node in ready_first {
        for address in &node.addresses {
            if !every.contains(address) && every.len() < MAX_ADDRESSES {
                every.push(address.clone());
            }
        }
    }
    if let Some(host) = host_of(block).filter(|h| !is_ip(h)) {
        if !every.is_empty() {
            book.push((host, every));
        }
    }
    for node in nodes.iter().filter(|n| !n.addresses.is_empty()) {
        let Some(host) = host_of(&node.url).filter(|h| !is_ip(h)) else {
            continue;
        };
        if !book.iter().any(|(known, _)| *known == host) {
            let addresses = node.addresses.iter().take(MAX_ADDRESSES).cloned().collect();
            book.push((host, addresses));
        }
    }
    book
}

#[derive(Default)]
struct State {
    nodes: Vec<Node>,
    /// Where each host is reached when its name does not resolve ([`address_book`]).
    book: Vec<(String, Vec<String>)>,
    /// The refresh's failover URLs and the ones a caller added.
    alternatives: Vec<String>,
    /// What answers named in `x-azlin-alt-endpoints`.
    learned: Vec<String>,
    /// The endpoint that answered last, and when.
    sticky: Option<(String, u64)>,
    /// What the token server's answers named besides the block, the nodes and the failover URLs
    /// (a moved drive's new block endpoint).
    trusted: Vec<TrustedEndpoint>,
    /// The epoch of the cluster map the requests name (`x-azlin-map`); none: no header.
    map_epoch: Option<u64>,
    /// An answer named an endpoint or a map the drive's view does not have.
    view_stale: bool,
}

/// The endpoints of one drive and how its requests move between them. Shared by every request
/// of the drive (`Arc`); see the module.
pub struct Failover {
    block: String,
    state: Mutex<State>,
    retry: Mutex<Retry>,
    sleep: Mutex<Sleep>,
    clock: Mutex<Clock>,
    /// The iroh lane (layer 0), when the app dials iroh.
    lane: Mutex<Option<Arc<IrohLane>>>,
}

impl std::fmt::Debug for Failover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failover")
            .field("block", &self.block)
            .field("route", &self.route())
            .field("retry", &*lock(&self.retry))
            .finish()
    }
}

impl Failover {
    /// The failover of a drive whose block endpoint is `block`, with no other endpoint yet.
    #[must_use]
    pub fn new(block: &str) -> Failover {
        Failover {
            block: normal(block),
            state: Mutex::new(State::default()),
            retry: Mutex::new(Policy::NODE),
            sleep: Mutex::new(Arc::new(std::thread::sleep) as Sleep),
            clock: Mutex::new(Arc::new(now_unix) as Clock),
            lane: Mutex::new(None),
        }
    }

    /// The block endpoint.
    #[must_use]
    pub fn block(&self) -> &str {
        &self.block
    }

    /// The node list (of a credential refresh) in its place, and where each host is reached
    /// when its name does not resolve ([`address_book`]): every request tells its transport
    /// before it is sent.
    pub fn set_nodes(&self, nodes: Vec<Node>) {
        let book = address_book(&self.block, &nodes);
        let mut state = lock(&self.state);
        state.nodes = nodes;
        state.book = book;
    }

    /// Where each host of the drive is reached when its name does not resolve.
    #[must_use]
    pub fn addresses(&self) -> Vec<(String, Vec<String>)> {
        lock(&self.state).book.clone()
    }

    /// Sends every request over `lane` first (iroh to the nodes), then the HTTPS layers; `None`:
    /// HTTPS only.
    pub fn set_lane(&self, lane: Option<Arc<IrohLane>>) {
        *lock(&self.lane) = lane;
    }

    /// The iroh lane, if the requests take one.
    #[must_use]
    pub fn lane(&self) -> Option<Arc<IrohLane>> {
        lock(&self.lane).clone()
    }

    /// Tells `transport` where each host is reached when its name does not resolve.
    fn teach(&self, transport: &dyn Transport) {
        let book = lock(&self.state).book.clone();
        for (host, addresses) in &book {
            transport.fallback_addresses(host, addresses);
        }
    }

    /// The node list as it is now.
    #[must_use]
    pub fn nodes(&self) -> Vec<Node> {
        lock(&self.state).nodes.clone()
    }

    /// The failover URLs (of a credential refresh) in place of the ones before.
    pub fn set_alternatives(&self, urls: Vec<String>) {
        let mut state = lock(&self.state);
        state.alternatives.clear();
        Self::add(&self.block, &mut state.alternatives, urls);
    }

    /// Follows the endpoints of a token server's answer when an answer names them.
    pub fn trust(&self, endpoints: Vec<TrustedEndpoint>) {
        lock(&self.state).trusted = endpoints;
    }

    /// The cluster map epoch every request names in `x-azlin-map` (of the last refresh);
    /// `None`: no header.
    pub fn set_map_epoch(&self, epoch: Option<u64>) {
        lock(&self.state).map_epoch = epoch;
    }

    /// The cluster map epoch the requests name.
    #[must_use]
    pub fn map_epoch(&self) -> Option<u64> {
        lock(&self.state).map_epoch
    }

    /// Whether an answer named an endpoint or a newer map than the drive's view has, since the
    /// last time this was asked: the drive then asks the token server for its view again.
    #[must_use]
    pub fn take_view_refresh(&self) -> bool {
        std::mem::take(&mut lock(&self.state).view_stale)
    }

    /// Whether `url` is one the token server named for the drive (and no step down to http).
    fn is_trusted(&self, state: &State, url: &str) -> bool {
        if downgrades(&self.block, url) {
            return false;
        }
        url == self.block
            || state.nodes.iter().any(|node| normal(&node.url) == url)
            || state.alternatives.iter().any(|known| known == url)
            || state.trusted.iter().any(|known| known.0 == url)
    }

    /// Asks `urls` too, after the node list, in their order.
    pub fn add_alternatives(&self, urls: Vec<String>) {
        let mut state = lock(&self.state);
        Self::add(&self.block, &mut state.alternatives, urls);
    }

    fn add(block: &str, list: &mut Vec<String>, urls: Vec<String>) {
        for url in urls.iter().map(|u| normal(u)) {
            if !url.is_empty() && url != block && !list.contains(&url) {
                list.push(url);
            }
        }
    }

    /// The endpoints after the block endpoint that are not nodes: the failover URLs, then what
    /// answers named.
    #[must_use]
    pub fn alternatives(&self) -> Vec<String> {
        let state = lock(&self.state);
        let mut out = state.alternatives.clone();
        for url in &state.learned {
            if !out.contains(url) {
                out.push(url.clone());
            }
        }
        out
    }

    pub fn set_retry(&self, retry: Retry) {
        *lock(&self.retry) = retry;
    }

    #[must_use]
    pub fn retry(&self) -> Retry {
        *lock(&self.retry)
    }

    /// Takes the pauses with `sleep` instead of sleeping.
    pub fn set_sleep(&self, sleep: Sleep) {
        *lock(&self.sleep) = sleep;
    }

    /// Reads the time from `clock` (seconds since 1970) instead of the system's.
    pub fn set_clock(&self, clock: Clock) {
        *lock(&self.clock) = clock;
    }

    fn now(&self) -> u64 {
        let clock = lock(&self.clock).clone();
        clock()
    }

    /// The endpoints one request tries, in order: the one that answered last (for
    /// [`STICKY_SECS`]), the block endpoint, the nodes, the failover URLs, what answers named.
    #[must_use]
    pub fn route(&self) -> Vec<String> {
        let now = self.now();
        let state = lock(&self.state);
        let mut route: Vec<String> = Vec::new();
        let block = self.block.as_str();
        let mut push = |url: &str| {
            let url = normal(url);
            if !url.is_empty() && !route.contains(&url) && !downgrades(block, &url) {
                route.push(url);
            }
        };
        if let Some((sticky, since)) = &state.sticky {
            if now.saturating_sub(*since) < STICKY_SECS {
                push(sticky.as_str());
            }
        }
        push(self.block.as_str());
        for node in &state.nodes {
            push(node.url.as_str());
        }
        for url in state.alternatives.iter().chain(&state.learned) {
            push(url.as_str());
        }
        route
    }

    /// Remembers the endpoints an answer's `x-azlin-alt-endpoints` names that the token server
    /// listed; they, in order. One it did not list is left out and asks for a fresh view.
    fn learn(&self, named: &str) -> Vec<String> {
        let mut state = lock(&self.state);
        let mut out = Vec::new();
        for url in named.split(',').map(normal).filter(|u| !u.is_empty()) {
            if !self.is_trusted(&state, &url) {
                state.view_stale = true;
                continue;
            }
            if url != self.block
                && !state.learned.contains(&url)
                && !state.alternatives.contains(&url)
            {
                state.learned.push(url.clone());
            }
            if !out.contains(&url) {
                out.push(url);
            }
        }
        out
    }

    /// `endpoint` answered: it goes first for a while (the block endpoint needs no stickiness).
    fn answered(&self, endpoint: &str) {
        let now = self.now();
        let mut state = lock(&self.state);
        state.sticky = (endpoint != self.block).then(|| (endpoint.to_string(), now));
    }

    /// The endpoint a moved drive's answer names (a bare host gets the block endpoint's
    /// scheme), when the token server listed it; one it did not is the next refresh's to
    /// confirm.
    fn endpoint_named(&self, named: &str) -> Option<String> {
        let named = named.trim();
        if named.is_empty() {
            return None;
        }
        let url = if named.contains("://") {
            normal(named)
        } else {
            let scheme = self.block.split_once("://").map_or("https", |(s, _)| s);
            normal(&format!("{scheme}://{named}"))
        };
        let mut state = lock(&self.state);
        if self.is_trusted(&state, &url) {
            Some(url)
        } else {
            state.view_stale = true;
            None
        }
    }

    /// Names the map epoch of the drive's view in `call` (none known: no header); that epoch.
    fn stamp_map(&self, call: &mut HttpCall) -> Option<u64> {
        call.headers.retain(|(name, _)| !name.eq_ignore_ascii_case(MAP_HEADER));
        let epoch = self.map_epoch()?;
        call.headers.push((MAP_HEADER.to_string(), epoch.to_string()));
        Some(epoch)
    }

    /// A node's 409 `map_stale` naming a newer epoch than the request named (`sent`): the drive
    /// takes that epoch and asks for a fresh view; whether it did.
    fn newer_map(&self, reply: &HttpReply, sent: Option<u64>) -> bool {
        let stale = reply.status == 409
            && reply
                .header("x-azlin-error")
                .is_some_and(|code| AzlinError::from_wire(code) == AzlinError::MapStale);
        let named = reply.header(MAP_HEADER).and_then(map_epoch);
        let (true, Some(sent), Some(named)) = (stale, sent, named) else {
            return false;
        };
        if named <= sent {
            return false;
        }
        let mut state = lock(&self.state);
        state.map_epoch = Some(state.map_epoch.map_or(named, |known| known.max(named)));
        state.view_stale = true;
        true
    }

    /// A request whose outcome at `endpoint` is unknown, when it may not be sent twice: the
    /// answer it would have had when what it does is found done.
    fn verified(
        &self,
        request: &Routed<'_>,
        transport: &dyn Transport,
        endpoint: &str,
    ) -> Option<HttpReply> {
        let Idempotency::Unsafe(Verify::Completion { etag: Some(etag) }) = request.idempotency()
        else {
            return None;
        };
        let mut head = request.probe_for(endpoint, request.key()?).ok()?;
        crate::hello::stamp(&mut head);
        self.stamp_map(&mut head);
        let reply = transport.send(&head).ok()?;
        let found = reply.header("etag")?.trim().trim_matches('"');
        (reply.is_success() && found.eq_ignore_ascii_case(etag)).then(|| HttpReply {
            status: 200,
            headers: vec![(String::from("etag"), format!("\"{etag}\""))],
            body: Vec::new(),
        })
    }

    /// Tells `transport` the addresses of the node at `endpoint` (layer 4); whether it took them.
    fn connect_by_address(&self, endpoint: &str, transport: &dyn Transport) -> bool {
        let node = lock(&self.state)
            .nodes
            .iter()
            .find(|node| normal(&node.url) == endpoint)
            .cloned();
        let Some(node) = node.filter(|n| !n.addresses.is_empty()) else {
            return false;
        };
        let Some(host) = host_of(&node.url) else {
            return false;
        };
        transport.fallback_addresses(&host, &node.addresses)
    }

    /// What `reply` means for the request about `key`, as the kit reads every answer
    /// ([`crate::answer`]): what passes by itself is asked again (the next node first), a drive
    /// that moved is followed, a damaged object is read once more from another node, and
    /// everything else is the caller's answer.
    fn verdict(&self, reply: &HttpReply, key: Option<&str>, reread: &mut bool) -> Verdict {
        match crate::answer::of_node_reply(reply) {
            Outcome::Done => Verdict::Done,
            Outcome::Transient {
                retry_after,
                elsewhere: true,
                ..
            } => Verdict::Again(retry_after),
            Outcome::Transient { retry_after, .. } => Verdict::Wait(retry_after),
            Outcome::Refused(code) => match code.row().behaviour {
                Behaviour::FollowRedirect => {
                    let DriveError::Service(service) = S3Drive::failure_of(reply, key) else {
                        return Verdict::Done;
                    };
                    match service
                        .endpoint
                        .as_deref()
                        .and_then(|named| self.endpoint_named(named))
                    {
                        Some(moved) => Verdict::Moved(moved),
                        None => Verdict::Again(service.retry_after.map(Duration::from_secs)),
                    }
                }
                Behaviour::RetryOnceThenMark if !*reread => {
                    *reread = true;
                    Verdict::Again(None)
                }
                _ => Verdict::Done,
            },
        }
    }
}

impl Router for Failover {
    fn send(
        &self,
        request: &Routed<'_>,
        transport: &dyn Transport,
    ) -> Result<HttpReply, DriveError> {
        // Layer 4 before anything is sent: a name that does not resolve has its addresses.
        self.teach(transport);
        // Layer 0: iroh to the nodes, when the app dials it.
        let mut lane_reply: Option<HttpReply> = None;
        if let Some(lane) = self.lane() {
            let nodes = self.nodes();
            let key = request.key();
            let mut reread_over_iroh = false;
            let mut is_final = |reply: &HttpReply| {
                matches!(
                    self.verdict(reply, key, &mut reread_over_iroh),
                    Verdict::Done
                )
            };
            match lane.send(request, &self.block, &nodes, &mut is_final) {
                LaneAnswer::Answered(reply) => return Ok(reply),
                LaneAnswer::Busy(reply) if lane.is_required() => return Ok(reply),
                LaneAnswer::Failed(why) if lane.is_required() => {
                    return Err(DriveError::Transport(why));
                }
                LaneAnswer::Busy(reply) => lane_reply = Some(reply),
                LaneAnswer::Failed(_) | LaneAnswer::Skipped => {}
            }
        }
        let retry = self.retry();
        let rounds = retry.attempts.max(1);
        let mut last_reply: Option<HttpReply> = None;
        let mut last_error: Option<DriveError> = None;
        let mut reread = false;
        // a node's newer map gets the request once more, not in a loop
        let mut map_resent = false;
        for round in 1..=rounds {
            let mut route = self.route();
            // The longest pause a node asked for in this round.
            let mut asked: Option<Duration> = None;
            let mut i = 0;
            while i < route.len() {
                let endpoint = route[i].clone();
                i += 1;
                let mut call = match request.signed_for(&endpoint) {
                    Ok(call) => call,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                crate::hello::stamp(&mut call);
                let epoch = self.stamp_map(&mut call);
                let sent = match transport.send(&call) {
                    Err(why)
                        if is_dns_failure(&why)
                            && self.connect_by_address(&endpoint, transport) =>
                    {
                        transport.send(&call)
                    }
                    other => other,
                };
                let reply = match sent {
                    Ok(reply) => reply,
                    Err(why) => {
                        // no answer: a request that may not be sent twice is checked first
                        if let Some(done) = self.verified(request, transport, &endpoint) {
                            self.answered(&endpoint);
                            return Ok(done);
                        }
                        last_error = Some(DriveError::Transport(format!("{endpoint}: {why}")));
                        continue;
                    }
                };
                if !map_resent && self.newer_map(&reply, epoch) {
                    // the same node again, naming its epoch
                    map_resent = true;
                    i -= 1;
                    continue;
                }
                if let Some(named) = reply.header("x-azlin-alt-endpoints") {
                    // Layer 2: the nodes the answer names come next, in this request.
                    let mut at = i;
                    for url in self.learn(named) {
                        if next_in(&mut route, at, url) {
                            at += 1;
                        }
                    }
                }
                match self.verdict(&reply, request.key(), &mut reread) {
                    Verdict::Done => {
                        self.answered(&endpoint);
                        return Ok(reply);
                    }
                    Verdict::Again(pause) => {
                        if outcome_unknown(&reply) {
                            if let Some(done) = self.verified(request, transport, &endpoint) {
                                self.answered(&endpoint);
                                return Ok(done);
                            }
                        }
                        if let Some(pause) = pause {
                            asked = Some(asked.map_or(pause, |a| a.max(pause)));
                        }
                        last_reply = Some(reply);
                    }
                    Verdict::Wait(pause) => {
                        if let Some(pause) = pause {
                            asked = Some(asked.map_or(pause, |a| a.max(pause)));
                        }
                        last_reply = Some(reply);
                        break;
                    }
                    Verdict::Moved(moved) => {
                        next_in(&mut route, i, moved);
                        last_reply = Some(reply);
                    }
                }
            }
            if round == rounds {
                break;
            }
            // Never sooner than a node asked, nor than the backoff: a Retry-After of 0 named
            // another node, not this round's nodes again at once. A longer pause than the
            // policy waits is the caller's to wait out: the node's answer says how long.
            let Some(pause) = retry.pause(asked, round) else {
                break;
            };
            let sleep = lock(&self.sleep).clone();
            sleep(pause);
        }
        match last_reply.or(lane_reply) {
            Some(reply) => Ok(reply),
            None => Err(last_error
                .unwrap_or_else(|| DriveError::Transport(String::from("no endpoint to ask")))),
        }
    }
}

/// Puts `url` at `at` of `route` (moved there when it comes later); whether it is there now. One
/// tried already (before `at`) stays where it was.
fn next_in(route: &mut Vec<String>, at: usize, url: String) -> bool {
    match route.iter().position(|known| *known == url) {
        Some(found) if found < at => false,
        Some(found) => {
            let url = route.remove(found);
            route.insert(at, url);
            true
        }
        None => {
            route.insert(at, url);
            true
        }
    }
}

/// Whether `reply` leaves open whether the request was done: a transient answer that may have
/// done something (a node that is busy, draining or being updated says so and did nothing).
fn outcome_unknown(reply: &HttpReply) -> bool {
    matches!(
        crate::answer::of_node_reply(reply),
        Outcome::Transient {
            spent: Spent::Maybe,
            ..
        }
    )
}

/// Reads a node list kept in `path` (a JSON array of [`Node`]s); empty when there is none.
#[must_use]
pub fn read_nodes(path: &std::path::Path) -> Vec<Node> {
    std::fs::read(path)
        .ok()
        .and_then(|text| serde_json::from_slice(&text).ok())
        .unwrap_or_default()
}

/// Keeps `nodes` in `path` for the next start (best effort: the next refresh lists them again).
pub fn write_nodes(path: &std::path::Path, nodes: &[Node]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_vec_pretty(nodes) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}
