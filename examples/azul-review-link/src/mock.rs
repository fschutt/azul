//! An in-process network for every test of the protocol: endpoints that find each other by ticket, a latency (with jitter),
//! frames lost on the newest-wins tracks, frames that overtake each other, a slow link (bytes a second), a Mac whose
//! address changed (the old ticket leads nowhere), a link that is down. Time is the caller's: a frame leaves at the `now`
//! it was sent at and arrives when a `poll` at or after its time asks for it, so a test drives a whole conversation with a
//! counter and gets the same run every time (the "randomness" is a seeded generator).
//!
//! Reliable frames are in order and never lost while the connection lasts; a newest-wins frame replaces one of its track
//! that has not arrived yet, and may be lost.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::link::{Delivery, Link, LinkEvent, LinkStats, Millis, PeerHandle};

/// What the network does to traffic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Knobs {
    /// One way, in milliseconds.
    pub latency_ms: u64,
    /// Up to this much more or less, one way (reliable frames still arrive in order).
    pub jitter_ms: u64,
    /// Newest-wins frames lost, in percent (reliable ones never are).
    pub loss_percent: u8,
    /// Newest-wins frames that take longer than the frames after them, in percent.
    pub reorder_percent: u8,
    /// A slow link: bytes a second in each direction (`None`: as fast as you like).
    pub bytes_per_second: Option<u64>,
    /// Nothing gets through: a dial fails and what is sent is dropped without a word.
    pub down: bool,
    /// Seeds the generator behind the loss and the jitter.
    pub seed: u64,
}

impl Default for Knobs {
    fn default() -> Self {
        Knobs { latency_ms: 5, jitter_ms: 0, loss_percent: 0, reorder_percent: 0, bytes_per_second: None, down: false, seed: 1 }
    }
}

/// The network all the endpoints of a test live in.
#[derive(Clone)]
pub struct MockNet {
    inner: Arc<Mutex<Net>>,
}

struct Net {
    knobs: Knobs,
    rng: u64,
    /// Endpoints by key.
    endpoints: BTreeMap<String, Endpoint>,
    next_connection: u64,
    connections: BTreeMap<u64, Connection>,
    /// The latest time any endpoint has told the network.
    last_now: Millis,
    /// Counts the frames in the order they were sent (it breaks ties between frames that arrive together).
    next_order: u64,
}

struct Endpoint {
    /// Counts up each time the endpoint's address changes: a ticket of an older number leads nowhere.
    epoch: u32,
    /// What has arrived and is waiting to be polled, by the time it arrives.
    inbox: Vec<(Millis, u64, LinkEvent)>,
    next_order: u64,
    next_handle: PeerHandle,
    started: bool,
}

struct Connection {
    ends: [String; 2],
    /// The handle each end knows the connection by.
    handles: [PeerHandle; 2],
    /// Frames on their way, per direction (0: from end 0 to end 1).
    flights: [VecDeque<Flight>; 2],
    /// When the last reliable frame of a direction arrives (so the next one cannot overtake it).
    reliable_at: [Millis; 2],
    /// When the link is free again (a slow link serialises what it carries).
    free_at: [Millis; 2],
    skipped: [u64; 2],
    open: bool,
}

struct Flight {
    arrives: Millis,
    order: u64,
    bytes: Vec<u8>,
    delivery: Delivery,
}

fn lock(net: &Mutex<Net>) -> MutexGuard<'_, Net> {
    net.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A key that looks like the transport's: 64 lowercase hex characters made from a name.
fn key_of(name: &str) -> String {
    let mut out = String::new();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for round in 0..4u64 {
        for b in name.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= round;
        h = h.wrapping_mul(0x0100_0000_01b3);
        out.push_str(&format!("{h:016x}"));
    }
    out
}

impl Default for MockNet {
    fn default() -> Self {
        MockNet::new()
    }
}

impl MockNet {
    /// A network with no endpoints and default knobs.
    #[must_use]
    pub fn new() -> MockNet {
        MockNet::with_knobs(Knobs::default())
    }

    /// A network with these knobs.
    #[must_use]
    pub fn with_knobs(knobs: Knobs) -> MockNet {
        let rng = knobs.seed | 1;
        MockNet { inner: Arc::new(Mutex::new(Net { knobs, rng, endpoints: BTreeMap::new(), next_connection: 1, connections: BTreeMap::new(), last_now: 0, next_order: 0 })) }
    }

    /// An endpoint called `name` (its key is made from the name, so a test can say whose key is whose).
    #[must_use]
    pub fn endpoint(&self, name: &str) -> MockLink {
        let id = key_of(name);
        lock(&self.inner).endpoints.entry(id.clone()).or_insert(Endpoint { epoch: 1, inbox: Vec::new(), next_order: 0, next_handle: 1, started: false });
        MockLink { id, net: self.clone() }
    }

    /// The key an endpoint called `name` has.
    #[must_use]
    pub fn key(name: &str) -> String {
        key_of(name)
    }

    /// Changes what the network does.
    pub fn set_knobs(&self, knobs: Knobs) {
        let mut net = lock(&self.inner);
        net.rng = knobs.seed | 1;
        net.knobs = knobs;
    }

    /// What the network does now.
    #[must_use]
    pub fn knobs(&self) -> Knobs {
        lock(&self.inner).knobs.clone()
    }

    /// The endpoint called `name` is now at another address: the tickets it gave before lead nowhere, a new one works.
    pub fn change_address(&self, name: &str) {
        if let Some(e) = lock(&self.inner).endpoints.get_mut(&key_of(name)) {
            e.epoch += 1;
        }
    }

    /// Cuts every connection of the endpoint called `name` at once, as a crash does (nobody says goodbye; both ends are told
    /// after the latency).
    pub fn kill_connections(&self, now: Millis, name: &str) {
        let id = key_of(name);
        let mut net = lock(&self.inner);
        let latency = net.knobs.latency_ms;
        let ids: Vec<u64> = net.connections.iter().filter(|(_, c)| c.open && c.ends.contains(&id)).map(|(k, _)| *k).collect();
        for cid in ids {
            close(&mut net, cid, now + latency, "the connection was cut");
        }
    }

    /// How many connections are open.
    #[must_use]
    pub fn open_connections(&self) -> usize {
        lock(&self.inner).connections.values().filter(|c| c.open).count()
    }
}

/// Closes a connection and tells both ends at `at`.
fn close(net: &mut Net, cid: u64, at: Millis, reason: &str) {
    let Some(conn) = net.connections.get_mut(&cid) else {
        return;
    };
    if !conn.open {
        return;
    }
    conn.open = false;
    let (ends, handles) = (conn.ends.clone(), conn.handles);
    for side in 0..2 {
        if let Some(e) = net.endpoints.get_mut(&ends[side]) {
            let order = e.next_order;
            e.next_order += 1;
            e.inbox.push((at, order, LinkEvent::Disconnected { peer: handles[side], reason: reason.to_string() }));
        }
    }
}

fn next_random(net: &mut Net) -> u64 {
    net.rng ^= net.rng >> 12;
    net.rng ^= net.rng << 25;
    net.rng ^= net.rng >> 27;
    net.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11
}

/// One endpoint of a [`MockNet`].
pub struct MockLink {
    id: String,
    net: MockNet,
}

impl MockLink {
    fn find(&self, net: &Net, peer: PeerHandle) -> Option<(u64, usize)> {
        net.connections.iter().find_map(|(cid, c)| {
            if !c.open {
                return None;
            }
            (0..2).find(|side| c.ends[*side] == self.id && c.handles[*side] == peer).map(|side| (*cid, side))
        })
    }
}

impl Link for MockLink {
    fn local_id(&self) -> String {
        self.id.clone()
    }

    fn ticket(&self) -> String {
        let net = lock(&self.net.inner);
        net.endpoints.get(&self.id).map_or_else(String::new, |e| format!("mt-{}-{}", self.id, e.epoch))
    }

    fn connect(&mut self, now: Millis, ticket: &str) -> Result<(), String> {
        let parsed = ticket.strip_prefix("mt-").and_then(|rest| rest.rsplit_once('-')).and_then(|(id, epoch)| Some((id.to_string(), epoch.parse::<u32>().ok()?)));
        let Some((target, epoch)) = parsed else {
            return Err(format!("not an endpoint ticket: {ticket:?}"));
        };
        if target == self.id {
            return Err("the ticket belongs to this endpoint".to_string());
        }
        let mut net = lock(&self.net.inner);
        let latency = net.knobs.latency_ms;
        let jitter = if net.knobs.jitter_ms > 0 { next_random(&mut net) % (net.knobs.jitter_ms + 1) } else { 0 };
        let at = now + latency + jitter;
        let reachable = !net.knobs.down && net.endpoints.get(&target).is_some_and(|e| e.epoch == epoch);
        if !reachable {
            // a dial to nowhere takes a while to give up
            let giving_up = at + 400;
            if let Some(e) = net.endpoints.get_mut(&self.id) {
                let order = e.next_order;
                e.next_order += 1;
                e.inbox.push((giving_up, order, LinkEvent::Failed { text: format!("could not connect to {}: no answer", &target[..8]) }));
            }
            return Ok(());
        }
        let cid = net.next_connection;
        net.next_connection += 1;
        let handle_of = |net: &mut Net, id: &str| {
            let e = net.endpoints.get_mut(id).map(|e| {
                let h = e.next_handle;
                e.next_handle += 1;
                h
            });
            e.unwrap_or(0)
        };
        let mine = handle_of(&mut net, &self.id);
        let theirs = handle_of(&mut net, &target);
        net.connections.insert(
            cid,
            Connection {
                ends: [self.id.clone(), target.clone()],
                handles: [mine, theirs],
                flights: [VecDeque::new(), VecDeque::new()],
                reliable_at: [0, 0],
                free_at: [0, 0],
                skipped: [0, 0],
                open: true,
            },
        );
        for (who, handle, other) in [(self.id.clone(), mine, target.clone()), (target, theirs, self.id.clone())] {
            if let Some(e) = net.endpoints.get_mut(&who) {
                let order = e.next_order;
                e.next_order += 1;
                e.inbox.push((at, order, LinkEvent::Connected { peer: handle, id: other }));
            }
        }
        Ok(())
    }

    fn send(&mut self, now: Millis, peer: PeerHandle, bytes: &[u8], delivery: Delivery) -> bool {
        let mut net = lock(&self.net.inner);
        net.last_now = net.last_now.max(now);
        let Some((cid, side)) = self.find(&net, peer) else {
            return false;
        };
        if net.knobs.down {
            // the sender is not told: a link that is down looks like a link that is slow
            return true;
        }
        let knobs = net.knobs.clone();
        let jitter = if knobs.jitter_ms > 0 { next_random(&mut net) % (knobs.jitter_ms + 1) } else { 0 };
        let loss_roll = next_random(&mut net) % 100;
        let reorder_roll = next_random(&mut net) % 100;
        let order = net.next_order;
        net.next_order += 1;
        let transmit = knobs.bytes_per_second.map_or(0, |bps| (bytes.len() as u64 * 1000).div_ceil(bps.max(1)));
        let Some(conn) = net.connections.get_mut(&cid) else {
            return false;
        };
        let start = now.max(conn.free_at[side]);
        conn.free_at[side] = start + transmit;
        let base = start + transmit + knobs.latency_ms + jitter;
        match delivery {
            Delivery::Reliable => {
                let arrives = base.max(conn.reliable_at[side]);
                conn.reliable_at[side] = arrives;
                conn.flights[side].push_back(Flight { arrives, order, bytes: bytes.to_vec(), delivery });
            }
            Delivery::Latest(track) => {
                // a newer frame of the track replaces one that has not arrived yet
                if let Some(at) = conn.flights[side].iter().position(|f| f.delivery == Delivery::Latest(track) && f.arrives > now) {
                    conn.flights[side].remove(at);
                    conn.skipped[side] += 1;
                }
                if loss_roll < u64::from(knobs.loss_percent) {
                    conn.skipped[side] += 1;
                    return true;
                }
                let arrives = if reorder_roll < u64::from(knobs.reorder_percent) { base + knobs.latency_ms * 3 + 7 } else { base };
                conn.flights[side].push_back(Flight { arrives, order, bytes: bytes.to_vec(), delivery });
            }
        }
        true
    }

    fn disconnect(&mut self, peer: PeerHandle) {
        let mut net = lock(&self.net.inner);
        if let Some((cid, _)) = self.find(&net, peer) {
            let latency = net.knobs.latency_ms;
            let now = net.connections.get(&cid).map_or(0, |c| c.reliable_at.iter().copied().max().unwrap_or(0));
            close(&mut net, cid, now + latency, "disconnected by the application");
        }
    }

    fn poll(&mut self, now: Millis) -> Vec<LinkEvent> {
        let mut net = lock(&self.net.inner);
        net.last_now = net.last_now.max(now);
        // what has arrived goes to the inbox of the end it was sent to
        let mut arrived: Vec<(String, Millis, u64, LinkEvent)> = Vec::new();
        for conn in net.connections.values_mut() {
            for from in 0..2 {
                let to = 1 - from;
                let mut at = 0;
                while at < conn.flights[from].len() {
                    if conn.flights[from][at].arrives <= now {
                        if let Some(f) = conn.flights[from].remove(at) {
                            arrived.push((
                                conn.ends[to].clone(),
                                f.arrives,
                                f.order,
                                LinkEvent::Received { peer: conn.handles[to], bytes: f.bytes, delivery: f.delivery },
                            ));
                        }
                    } else {
                        at += 1;
                    }
                }
            }
        }
        for (who, at, order, event) in arrived {
            if let Some(e) = net.endpoints.get_mut(&who) {
                e.inbox.push((at, order, event));
            }
        }
        let Some(me) = net.endpoints.get_mut(&self.id) else {
            return Vec::new();
        };
        let mut due: Vec<(Millis, u64, LinkEvent)> = Vec::new();
        let mut at = 0;
        while at < me.inbox.len() {
            if me.inbox[at].0 <= now {
                due.push(me.inbox.remove(at));
            } else {
                at += 1;
            }
        }
        due.sort_by_key(|(when, order, _)| (*when, *order));
        let mut out = Vec::new();
        if !me.started {
            me.started = true;
            out.push(LinkEvent::Ready { ticket: format!("mt-{}-{}", self.id, me.epoch) });
        }
        out.extend(due.into_iter().map(|(_, _, event)| event));
        out
    }

    fn stats(&self, peer: PeerHandle) -> LinkStats {
        let net = lock(&self.net.inner);
        let Some((cid, side)) = self.find(&net, peer) else {
            return LinkStats::default();
        };
        let Some(conn) = net.connections.get(&cid) else {
            return LinkStats::default();
        };
        LinkStats {
            connected: conn.open,
            direct: true,
            rtt_ms: (2 * net.knobs.latency_ms) as u32,
            skipped: conn.skipped[side],
            queued_bytes: conn.flights[side].iter().filter(|f| f.arrives > net.last_now).map(|f| f.bytes.len() as u64).sum(),
        }
    }

    fn close(&mut self) {
        let mut net = lock(&self.net.inner);
        let ids: Vec<u64> = net.connections.iter().filter(|(_, c)| c.open && c.ends.contains(&self.id)).map(|(k, _)| *k).collect();
        for cid in ids {
            close(&mut net, cid, 0, "the endpoint closed");
        }
    }
}

#[cfg(test)]
#[path = "mock_test.rs"]
mod mock_test;
