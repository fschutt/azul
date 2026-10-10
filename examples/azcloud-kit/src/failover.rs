//! The failover of an Azlin drive's requests (D35): four layers, tried in order, and retries by
//! the class of what went wrong.
//!
//! 1. **The block endpoint** - the drive's own (its DNS name balances over the healthy nodes).
//! 2. **The hint**: a node that cannot serve (draining, no majority, overloaded) answers `503`
//!    with `x-azlin-alt-endpoints`; the SAME request goes to the nodes it names at once (with
//!    `Retry-After: 0` there is no pause), and they are remembered for the next requests.
//! 3. **The node list** of the last credential refresh ([`Node::list`]: ready nodes first, in
//!    the server's order), then the refresh's failover URLs. The endpoint that answered stays
//!    first for [`STICKY_SECS`].
//! 4. **The node's addresses**: when a node's name does not resolve, the transport is told its
//!    addresses ([`azul_storage::Transport::fallback_addresses`]) and the request is sent again -
//!    still to the node's name, so TLS verifies the certificate for it.
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
    DriveError, HttpReply, S3Drive, Transport,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::user_errors::{Behaviour, UserError};

/// How long the endpoint that answered stays first (seconds).
pub const STICKY_SECS: u64 = 600;
/// How often the endpoints are asked for one request, by default.
pub const ROUNDS: u32 = 3;
/// The pause after the first round without an answer; it doubles every round.
pub const BACKOFF: Duration = Duration::from_millis(250);
/// The longest `Retry-After` a request waits out itself.
pub const MAX_PAUSE: Duration = Duration::from_secs(30);

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

/// Whether a transport's reason says the name did not resolve.
fn is_dns_failure(why: &str) -> bool {
    let why = why.to_ascii_lowercase();
    [
        "dns",
        "resolve",
        "host not found",
        "lookup",
        "name or service not known",
        "nodename nor servname",
        "no such host",
    ]
    .iter()
    .any(|sign| why.contains(sign))
}

/// One node of the drive, as the credential refresh lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    /// The node's own endpoint (`https://n2.<domain>`).
    pub url: String,
    /// Its IPv4 / IPv6 addresses: where it is reached when its name does not resolve.
    #[serde(default)]
    pub addresses: Vec<String>,
    /// It serves (the token server's health); an unknown state counts as ready.
    #[serde(default)]
    pub ready: bool,
}

impl Node {
    /// A node of the token server's list (`name`, `url` or `public_url`, `ipv4`, `ipv6`, `ready`);
    /// `None` without a URL.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Node> {
        let url = value["url"]
            .as_str()
            .or_else(|| value["public_url"].as_str())
            .map(normal)
            .filter(|url| !url.is_empty())?;
        let mut addresses: Vec<String> = Vec::new();
        for field in ["ipv4", "ipv6", "addresses"] {
            let found: Vec<&str> = match &value[field] {
                Value::String(one) => vec![one.as_str()],
                Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            for address in found.into_iter().map(str::trim).filter(|a| !a.is_empty()) {
                if !addresses.iter().any(|a| a == address) {
                    addresses.push(address.to_string());
                }
            }
        }
        Some(Node {
            name: value["name"].as_str().unwrap_or_default().to_string(),
            url,
            addresses,
            ready: value["ready"].as_bool().unwrap_or(true),
        })
    }

    /// The nodes of a refresh's list: ready ones first, each group in the server's order
    /// (healthiest first); entries without a URL are left out.
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

#[derive(Default)]
struct State {
    nodes: Vec<Node>,
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
        }
    }

    /// The block endpoint.
    #[must_use]
    pub fn block(&self) -> &str {
        &self.block
    }

    /// The node list (of a credential refresh) in its place.
    pub fn set_nodes(&self, nodes: Vec<Node>) {
        lock(&self.state).nodes = nodes;
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
                push(sticky);
            }
        }
        push(&self.block);
        for node in &state.nodes {
            push(&node.url);
        }
        for url in state.alternatives.iter().chain(&state.learned) {
            push(url);
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
        match last_reply {
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
