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
//!    `Retry-After: 0` there is no pause), and they are remembered for the next requests.
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
//! What happens to an answer is its class in [`crate::user_errors`] (no table here): busy and
//! "something went wrong" are asked again at the next endpoint and, after every endpoint, again
//! after a growing backoff ([`Retry`]); a pause the node asks for (`Retry-After`) is kept, and
//! one longer than [`Retry::max_pause`] is the caller's to wait out (the answer is returned); a
//! drive that moved (`wrong_block`) is followed to the endpoint the answer names; a damaged
//! object is read once more from another node; everything the user must act on (a full drive,
//! a device signed out, a conflict) and every ordinary answer (404, 412, ...) is returned as it
//! is. A request no endpoint answered fails with the last endpoint's reason.
//!
//! [`Failover`] is the [`Router`] the kit's [`crate::bucket::Bucket`] and
//! [`crate::drive::AzlinDrive`] hand their S3 drive: every request - each part of an upload, each
//! range of a download - goes through it on its own.

use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use azul_storage::{
    s3::{Routed, Router},
    time::now_unix,
    transport::is_dns_failure,
    DriveError, HttpReply, S3Drive, Transport,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    transport::{IrohLane, LaneAnswer},
    user_errors::{Behaviour, UserError},
};

/// How long the endpoint that answered stays first (seconds).
pub const STICKY_SECS: u64 = 600;
/// How often the endpoints are asked for one request, by default.
pub const ROUNDS: u32 = 3;
/// The pause after the first round without an answer; it doubles every round.
pub const BACKOFF: Duration = Duration::from_millis(250);
/// The longest `Retry-After` a request waits out itself.
pub const MAX_PAUSE: Duration = Duration::from_secs(30);
/// The most addresses one host is reached at (what azul's HTTP client keeps of a host).
pub const MAX_ADDRESSES: usize = 16;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An endpoint as the failover compares it: trimmed, without a trailing `/`.
fn normal(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
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

/// How often one request is tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    /// Times every endpoint is asked (at least one).
    pub rounds: u32,
    /// The pause after the first round; it doubles every round (with a random part on top).
    pub backoff: Duration,
    /// The longest `Retry-After` waited out here; a longer one returns the node's answer.
    pub max_pause: Duration,
}

impl Default for Retry {
    fn default() -> Retry {
        Retry {
            rounds: ROUNDS,
            backoff: BACKOFF,
            max_pause: MAX_PAUSE,
        }
    }
}

impl Retry {
    /// Every endpoint once, no pause (a probe, a lane that falls back on its own).
    #[must_use]
    pub fn once() -> Retry {
        Retry {
            rounds: 1,
            ..Retry::default()
        }
    }

    /// The pause after `round` (from 1) without an answer: the backoff doubled per round, and up
    /// to half of it again at random (two devices do not come back at the same moment).
    fn pause_after(&self, round: u32) -> Duration {
        let base = self
            .backoff
            .saturating_mul(1u32 << round.saturating_sub(1).min(16));
        let half = u64::try_from(base.as_millis() / 2).unwrap_or(u64::MAX);
        let extra = azul_storage::ids::random_seed() % half.saturating_add(1);
        base.saturating_add(Duration::from_millis(extra))
    }
}

/// How a pause is taken (the tests record it instead).
pub type Sleep = Arc<dyn Fn(Duration) + Send + Sync>;
/// Now, in seconds since 1970 (the sticky node's ten minutes).
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// What one answer means for the request.
enum Verdict {
    /// The answer of the request (a success, or one that is not tried again).
    Done,
    /// Ask the next endpoint; after the last one, again after a pause (`Retry-After`, if any).
    Again(Option<Duration>),
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
            retry: Mutex::new(Retry::default()),
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
        let mut push = |url: &str| {
            let url = normal(url);
            if !url.is_empty() && !route.contains(&url) {
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

    /// Remembers the endpoints an answer's `x-azlin-alt-endpoints` names; they, in order.
    fn learn(&self, named: &str) -> Vec<String> {
        let mut state = lock(&self.state);
        let mut out = Vec::new();
        for url in named.split(',').map(normal).filter(|u| !u.is_empty()) {
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

    /// The endpoint a moved drive's answer names (a bare host gets the block endpoint's scheme).
    fn endpoint_named(&self, named: &str) -> Option<String> {
        let named = named.trim();
        if named.is_empty() {
            return None;
        }
        if named.contains("://") {
            return Some(normal(named));
        }
        let scheme = self.block.split_once("://").map_or("https", |(s, _)| s);
        Some(normal(&format!("{scheme}://{named}")))
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

    /// What `reply` means for the request about `key` (see the module).
    fn verdict(&self, reply: &HttpReply, key: Option<&str>, reread: &mut bool) -> Verdict {
        let azlin = reply
            .header("x-azlin-error")
            .is_some_and(|code| !code.trim().is_empty());
        let redirect = matches!(reply.status, 301 | 307);
        if !azlin && !redirect && reply.status < 500 && reply.status != 429 {
            return Verdict::Done;
        }
        let error = S3Drive::failure_of(reply, key);
        let DriveError::Service(service) = &error else {
            return Verdict::Done;
        };
        if redirect && !azlin {
            // A plain S3 redirect (another region) is the user's setting to fix.
            return Verdict::Done;
        }
        let Some(user) = UserError::from_drive_error(&error) else {
            return Verdict::Done;
        };
        let pause = service.retry_after.map(Duration::from_secs);
        match user.behaviour() {
            Behaviour::FollowRedirect => match service
                .endpoint
                .as_deref()
                .and_then(|named| self.endpoint_named(named))
            {
                Some(moved) => Verdict::Moved(moved),
                None => Verdict::Again(pause),
            },
            Behaviour::RetryWithBackoff | Behaviour::RetryAfterPause | Behaviour::Report => {
                Verdict::Again(pause)
            }
            Behaviour::RetryOnceThenMark if !*reread => {
                *reread = true;
                Verdict::Again(None)
            }
            _ => Verdict::Done,
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
        let rounds = retry.rounds.max(1);
        let mut last_reply: Option<HttpReply> = None;
        let mut last_error: Option<DriveError> = None;
        let mut reread = false;
        for round in 1..=rounds {
            let mut route = self.route();
            // The longest pause a node asked for in this round.
            let mut asked: Option<Duration> = None;
            let mut i = 0;
            while i < route.len() {
                let endpoint = route[i].clone();
                i += 1;
                let call = match request.signed_for(&endpoint) {
                    Ok(call) => call,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
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
                        last_error = Some(DriveError::Transport(format!("{endpoint}: {why}")));
                        continue;
                    }
                };
                if let Some(named) = reply.header("x-azlin-alt-endpoints") {
                    // Layer 2: the nodes the answer names come next, in this request.
                    let mut at = i;
                    for url in self.learn(named) {
                        if !route.contains(&url) {
                            route.insert(at, url);
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
                        if let Some(pause) = pause {
                            asked = Some(asked.map_or(pause, |a| a.max(pause)));
                        }
                        last_reply = Some(reply);
                    }
                    Verdict::Moved(moved) => {
                        if !route.contains(&moved) {
                            route.insert(i, moved);
                        }
                        last_reply = Some(reply);
                    }
                }
            }
            if round == rounds {
                break;
            }
            let pause = match asked {
                // A long pause is the caller's to wait out: the node's answer says how long.
                Some(asked) if asked > retry.max_pause => break,
                Some(asked) => asked,
                None => retry.pause_after(round),
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
