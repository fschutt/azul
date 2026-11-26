//! The [`Link`] over azul's iroh endpoint (feature `azul`): QUIC between the phone and the Mac on the local network, the connection
//! dialed by a ticket (the endpoint's id and addresses) and authenticated by the endpoint keys, so [`LinkEvent::Connected`] says who
//! is really there. Reliable frames travel as the endpoint's messages (in order, never dropped while the connection lasts), the
//! newest-wins ones (the pen state, the gesture updates) as its frames on a track of their own (a newer frame replaces one that has
//! not left yet, and the receiver sees the newest of each track).
//!
//! Things this adapter does, so that nothing above it knows about the transport:
//! - the endpoint's events become [`LinkEvent`]s (a frame on track `n` is `Delivery::Latest(n)`; a failure the system caused by not
//!   letting the app use the network starts with [`PERMISSION_PREFIX`]);
//! - the endpoint takes frames and messages of the protocol's own cap ([`MAX_FRAME_BYTES`]) at most, not the engine's 16 MiB;
//! - one poll takes at most [`MAX_EVENTS_PER_POLL`] events, so a flood of frames never holds up the screens;
//! - the identity (the endpoint's 32-byte key) is the caller's: the Mac and the phone know each other by it, so a new key at every
//!   start would be a new device at every start ([`IrohLink::secret_key`] reads the one a first start made).
//!
//! The relay servers are OFF unless asked for: the companion is for a desk, not for the internet. The engine's local-network discovery
//! is left as it is by default (on: it announces the endpoint and finds others, so a Mac whose address changed can be found by its key).

use azul::iroh::{IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohPeerStats, IrohRelayMode};
use azul::vec::U8Vec;

use crate::limits::{ALPN, MAX_FRAME_BYTES};
use crate::link::{Delivery, Link, LinkEvent, LinkStats, Millis, PeerHandle, PERMISSION_PREFIX};

/// The most events one poll takes.
pub const MAX_EVENTS_PER_POLL: usize = 512;

/// How an endpoint is bound.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IrohOptions {
    /// The endpoint's identity as the last run kept it; `None` makes a new one (read it back with [`IrohLink::secret_key`]).
    pub secret_key: Option<[u8; 32]>,
    /// Traffic may go through the public relay servers when there is no direct path. Off: the local network only.
    pub relay: bool,
}

/// The link over an iroh endpoint.
pub struct IrohLink {
    endpoint: IrohEndpoint,
}

impl IrohLink {
    /// Binds an endpoint.
    ///
    /// # Errors
    /// The engine could not bind (no engine in this build of the library, no socket): the text says why.
    pub fn bind(options: &IrohOptions) -> Result<IrohLink, String> {
        let mut config = IrohConfig::create(ALPN)
            .with_relay_mode(relay_mode(options.relay))
            .with_max_frame_bytes(u32::try_from(MAX_FRAME_BYTES).unwrap_or(u32::MAX));
        if let Some(key) = options.secret_key {
            config = config.with_secret_key(U8Vec::from(key.to_vec()));
        }
        let endpoint = IrohEndpoint::bind(config);
        if !endpoint.is_bound() {
            return Err(endpoint.recv().into_option().map_or_else(|| "the iroh endpoint did not bind".to_string(), |event| failure_text(event.text.as_str())));
        }
        Ok(IrohLink { endpoint })
    }

    /// The endpoint's identity, to keep for the next start (`None` when the endpoint gives none).
    #[must_use]
    pub fn secret_key(&self) -> Option<[u8; 32]> {
        let bytes = self.endpoint.secret_key();
        <[u8; 32]>::try_from(bytes.as_ref()).ok()
    }
}

/// Where the endpoint may relay traffic: the public servers when asked for, else nowhere (direct paths only).
#[must_use]
pub fn relay_mode(relay: bool) -> IrohRelayMode {
    if relay {
        IrohRelayMode::Default
    } else {
        IrohRelayMode::Disabled
    }
}

/// What the engine knows of a connection, as the protocol's [`LinkStats`].
#[must_use]
pub fn link_stats(stats: &IrohPeerStats) -> LinkStats {
    LinkStats {
        connected: stats.connected,
        direct: stats.direct,
        rtt_ms: u32::try_from(stats.rtt_us / 1_000).unwrap_or(u32::MAX),
        skipped: stats.frames_skipped,
        queued_bytes: stats.message_bytes_queued,
    }
}

/// What a failure of the engine says, with the permission prefix when the system did not let the app use the network.
#[must_use]
pub fn failure_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("operation not permitted") || lower.contains("permission denied") || lower.contains("local network") {
        format!("{PERMISSION_PREFIX} {text}")
    } else {
        text.to_string()
    }
}

/// The link event an engine event is, if it is one the protocol cares about.
#[must_use]
pub fn link_event(event: &IrohEvent) -> LinkEvent {
    match event.kind {
        IrohEventKind::Ready => LinkEvent::Ready { ticket: event.text.as_str().to_string() },
        IrohEventKind::PeerConnected => LinkEvent::Connected { peer: event.peer, id: event.text.as_str().to_string() },
        IrohEventKind::PeerDisconnected => LinkEvent::Disconnected { peer: event.peer, reason: event.text.as_str().to_string() },
        IrohEventKind::Frame => LinkEvent::Received {
            peer: event.peer,
            bytes: event.data.as_ref().to_vec(),
            // a track the protocol does not have is the last of the tracks, whatever the peer says
            delivery: Delivery::Latest(u8::try_from(event.track).unwrap_or(u8::MAX)),
        },
        IrohEventKind::Message => LinkEvent::Received { peer: event.peer, bytes: event.data.as_ref().to_vec(), delivery: Delivery::Reliable },
        IrohEventKind::Error => LinkEvent::Failed { text: failure_text(event.text.as_str()) },
    }
}

impl Link for IrohLink {
    fn local_id(&self) -> String {
        self.endpoint.endpoint_id().as_str().to_string()
    }

    fn ticket(&self) -> String {
        self.endpoint.ticket().as_str().to_string()
    }

    fn connect(&mut self, _now: Millis, ticket: &str) -> Result<(), String> {
        if ticket.is_empty() {
            return Err("there is no address to dial".to_string());
        }
        if self.endpoint.connect(ticket) {
            Ok(())
        } else {
            Err("the address cannot be dialed (it is no ticket, or the endpoint is closed)".to_string())
        }
    }

    fn send(&mut self, _now: Millis, peer: PeerHandle, bytes: &[u8], delivery: Delivery) -> bool {
        match delivery {
            Delivery::Reliable => self.endpoint.send_message(peer, U8Vec::from(bytes.to_vec())),
            Delivery::Latest(track) => self.endpoint.send_frame(peer, u32::from(track), U8Vec::from(bytes.to_vec())),
        }
    }

    fn disconnect(&mut self, peer: PeerHandle) {
        self.endpoint.disconnect(peer);
    }

    fn poll(&mut self, _now: Millis) -> Vec<LinkEvent> {
        let mut out = Vec::new();
        for _ in 0..MAX_EVENTS_PER_POLL {
            let Some(event) = self.endpoint.recv().into_option() else {
                break;
            };
            out.push(link_event(&event));
        }
        out
    }

    fn stats(&self, peer: PeerHandle) -> LinkStats {
        link_stats(&self.endpoint.peer_stats(peer))
    }

    fn close(&mut self) {
        self.endpoint.close();
    }
}

#[cfg(test)]
#[path = "iroh_link_test.rs"]
mod iroh_link_test;
