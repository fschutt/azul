//! The seam between the protocol and the network. The protocol (host, phone, pairing) talks to a [`Link`]; the
//! tests talk to a [`crate::mock::MockNet`] (an in-process network with latency, loss, reordering and a slow
//! link), the real run to azul's iroh endpoint (`iroh_link.rs`, feature `azul`). No type of the transport leaks
//! through it: bytes, a peer handle and the peer's key (a lowercase hex string).

/// A monotonic clock in milliseconds, handed in by the caller (a real clock in the app, a counter in a test).
pub type Millis = u64;

/// A connection, as the link numbers it.
pub type PeerHandle = u64;

/// How a frame travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// In order and never dropped while the connection lasts.
    Reliable,
    /// Newest wins on this track: a newer frame replaces one that has not left yet, and a slow link drops the
    /// ones in between (the pen state, the gesture updates).
    Latest(u8),
}

/// What happened on the network since the last poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    /// The endpoint knows its addresses: `ticket` is how a peer dials it.
    Ready { ticket: String },
    /// A connection was dialed or accepted; `id` is the peer's key as the transport authenticated it.
    Connected { peer: PeerHandle, id: String },
    Disconnected { peer: PeerHandle, reason: String },
    Received { peer: PeerHandle, bytes: Vec<u8>, delivery: Delivery },
    /// A bind, dial or transport failure.
    Failed { text: String },
}

/// What the transport knows of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkStats {
    pub connected: bool,
    /// Traffic flows over a direct path (the local network), not a relay.
    pub direct: bool,
    /// The transport's own round-trip estimate (0: unknown).
    pub rtt_ms: u32,
    /// Frames of a newest-wins track that a newer frame replaced before they left.
    pub skipped: u64,
    /// Reliable bytes accepted and not yet handed to the network.
    pub queued_bytes: u64,
}

/// The network, as the protocol sees it.
pub trait Link {
    /// This endpoint's key, lowercase hex.
    fn local_id(&self) -> String;

    /// How a peer dials this endpoint now (empty until the endpoint knows its addresses).
    fn ticket(&self) -> String;

    /// Dials `ticket`. The outcome arrives as `Connected` or `Failed`.
    ///
    /// # Errors
    /// The ticket cannot be dialed at all (it does not parse, it is this endpoint's own).
    fn connect(&mut self, now: Millis, ticket: &str) -> Result<(), String>;

    /// Sends `bytes` to `peer`. False when there is no such connection.
    fn send(&mut self, now: Millis, peer: PeerHandle, bytes: &[u8], delivery: Delivery) -> bool;

    /// Closes a connection.
    fn disconnect(&mut self, peer: PeerHandle);

    /// What happened since the last call.
    fn poll(&mut self, now: Millis) -> Vec<LinkEvent>;

    /// What the transport knows of one connection.
    fn stats(&self, peer: PeerHandle) -> LinkStats;

    /// Closes the endpoint and every connection.
    fn close(&mut self);
}

impl Link for Box<dyn Link> {
    fn local_id(&self) -> String {
        (**self).local_id()
    }

    fn ticket(&self) -> String {
        (**self).ticket()
    }

    fn connect(&mut self, now: Millis, ticket: &str) -> Result<(), String> {
        (**self).connect(now, ticket)
    }

    fn send(&mut self, now: Millis, peer: PeerHandle, bytes: &[u8], delivery: Delivery) -> bool {
        (**self).send(now, peer, bytes, delivery)
    }

    fn disconnect(&mut self, peer: PeerHandle) {
        (**self).disconnect(peer);
    }

    fn poll(&mut self, now: Millis) -> Vec<LinkEvent> {
        (**self).poll(now)
    }

    fn stats(&self, peer: PeerHandle) -> LinkStats {
        (**self).stats(peer)
    }

    fn close(&mut self) {
        (**self).close();
    }
}
