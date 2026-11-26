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
//! is left as it is by default (on): it announces the endpoint (with the short text the app advertises, see
//! [`IrohOptions::advertised`]) and lists the other endpoints that announced themselves. [`IrohLink::discovery`] is that list as a
//! [`Discovery`]: the phone dials a Mac that announces itself by its key, wherever its address is now.

use std::sync::{Arc, Mutex, MutexGuard};

use azul::iroh::{IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohNearbyPeer, IrohPeerStats, IrohRelayMode};
use azul::vec::U8Vec;

use crate::discovery::{Discovery, Nearby, NearbyEvent};
use crate::limits::{ALPN, MAX_FRAME_BYTES};
use crate::link::{Delivery, Link, LinkEvent, LinkStats, Millis, PeerHandle, PERMISSION_PREFIX};

/// The most events one poll takes.
pub const MAX_EVENTS_PER_POLL: usize = 512;
/// The most announcements of the local network kept for [`IrohDiscovery::poll`] (the oldest give way).
pub const MAX_NEWS: usize = 256;

/// How an endpoint is bound.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IrohOptions {
    /// The endpoint's identity as the last run kept it; `None` makes a new one (read it back with [`IrohLink::secret_key`]).
    pub secret_key: Option<[u8; 32]>,
    /// Traffic may go through the public relay servers when there is no direct path. Off: the local network only.
    pub relay: bool,
    /// The short text the endpoint announces on the local network (`advert::encode`: the project, the display name; at most 245 bytes,
    /// never a secret): the phone reads it from the list of neighbours. `None`: nothing is announced but the key and the addresses.
    pub advertised: Option<String>,
}

/// The link over an iroh endpoint.
pub struct IrohLink {
    endpoint: IrohEndpoint,
    /// What the endpoint said about the local network, for [`IrohDiscovery`] (filled by `poll`).
    news: Arc<Mutex<Vec<NearbyEvent>>>,
}

impl IrohLink {
    /// The engine's config for `options` (the protocol's name, the relay, the frame cap, the key, the advertised text): what
    /// [`IrohLink::bind`] binds. An app that wants more of the engine (a port) adds it to this and calls [`IrohLink::bind_config`].
    #[must_use]
    pub fn config(options: &IrohOptions) -> IrohConfig {
        let mut config = IrohConfig::create(ALPN)
            .with_relay_mode(relay_mode(options.relay))
            .with_max_frame_bytes(u32::try_from(MAX_FRAME_BYTES).unwrap_or(u32::MAX));
        if let Some(key) = options.secret_key {
            config = config.with_secret_key(U8Vec::from(key.to_vec()));
        }
        if let Some(text) = &options.advertised {
            config = config.with_advertised(text.as_str());
        }
        config
    }

    /// Binds an endpoint.
    ///
    /// # Errors
    /// The engine could not bind (no engine in this build of the library, no socket): the text says why.
    pub fn bind(options: &IrohOptions) -> Result<IrohLink, String> {
        IrohLink::bind_config(IrohLink::config(options))
    }

    /// Binds an endpoint with a config the app made from [`IrohLink::config`].
    ///
    /// # Errors
    /// As [`IrohLink::bind`].
    pub fn bind_config(config: IrohConfig) -> Result<IrohLink, String> {
        let endpoint = IrohEndpoint::bind(config);
        if !endpoint.is_bound() {
            return Err(endpoint.recv().into_option().map_or_else(|| "the iroh endpoint did not bind".to_string(), |event| failure_text(event.text.as_str())));
        }
        Ok(IrohLink { endpoint, news: Arc::new(Mutex::new(Vec::new())) })
    }

    /// The endpoint's identity, to keep for the next start (`None` when the endpoint gives none).
    #[must_use]
    pub fn secret_key(&self) -> Option<[u8; 32]> {
        let bytes = self.endpoint.secret_key();
        <[u8; 32]>::try_from(bytes.as_ref()).ok()
    }

    /// The local network as the engine sees it, for a client that looks for its Mac: who is nearby, and who appeared or went since
    /// the last look. It shares the endpoint, and its news is what [`Link::poll`] of this link took from the engine's events, so it
    /// is only as fresh as the last poll of the link.
    #[must_use]
    pub fn discovery(&self) -> IrohDiscovery {
        IrohDiscovery { endpoint: self.endpoint.clone(), news: self.news.clone() }
    }

    /// What the engine says about itself on the local network (`off`, `on`, or why it is unavailable), for a diagnostics page.
    #[must_use]
    pub fn discovery_status(&self) -> String {
        self.endpoint.local_discovery_status().as_str().to_string()
    }
}

/// The endpoints that announced themselves on the local network, as a [`Discovery`].
pub struct IrohDiscovery {
    endpoint: IrohEndpoint,
    news: Arc<Mutex<Vec<NearbyEvent>>>,
}

impl Discovery for IrohDiscovery {
    fn nearby(&self) -> Vec<Nearby> {
        self.endpoint.nearby_peers().as_slice().iter().map(neighbour).collect()
    }

    fn poll(&mut self, _now: Millis) -> Vec<NearbyEvent> {
        std::mem::take(&mut *lock(&self.news))
    }
}

fn lock(news: &Mutex<Vec<NearbyEvent>>) -> MutexGuard<'_, Vec<NearbyEvent>> {
    news.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A neighbour the engine lists.
#[must_use]
pub fn neighbour(peer: &IrohNearbyPeer) -> Nearby {
    let addresses = peer.addresses.as_slice().iter().map(|a| a.as_str().to_string()).collect();
    Nearby::from_announcement(peer.endpoint_id.as_str(), peer.advertised.as_str(), addresses)
}

/// What an announcement event of the engine says (`addresses` are the ones the engine lists for that endpoint now); `None` for any other
/// event.
#[must_use]
pub fn nearby_event(event: &IrohEvent, addresses: Vec<String>) -> Option<NearbyEvent> {
    match event.kind {
        IrohEventKind::NearbyAppeared => {
            let advertised = String::from_utf8_lossy(event.data.as_ref()).to_string();
            Some(NearbyEvent::Appeared(Nearby::from_announcement(event.text.as_str(), &advertised, addresses)))
        }
        IrohEventKind::NearbyGone => Some(NearbyEvent::Gone(event.text.as_str().to_string())),
        _ => None,
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
    // (iOS says kDNSServiceErr_PolicyDenied when the user denied the Local Network prompt, or the Info.plist keys are missing)
    if ["operation not permitted", "permission denied", "local network", "policydenied", "policy denied"].iter().any(|words| lower.contains(words)) {
        format!("{PERMISSION_PREFIX} {text}")
    } else {
        text.to_string()
    }
}

/// The link event an engine event is; `None` for an event that is not about a connection (the engine's announcements of other endpoints
/// on the local network are [`nearby_event`]s, for [`IrohDiscovery`]).
#[must_use]
pub fn link_event(event: &IrohEvent) -> Option<LinkEvent> {
    Some(match event.kind {
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
        _ => return None,
    })
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
            match event.kind {
                IrohEventKind::NearbyAppeared | IrohEventKind::NearbyGone => {
                    let id = event.text.as_str();
                    let addresses = self
                        .endpoint
                        .nearby_peers()
                        .as_slice()
                        .iter()
                        .find(|peer| peer.endpoint_id.as_str() == id)
                        .map(|peer| neighbour(peer).addresses)
                        .unwrap_or_default();
                    if let Some(news) = nearby_event(&event, addresses) {
                        let mut kept = lock(&self.news);
                        if kept.len() >= MAX_NEWS {
                            kept.remove(0);
                        }
                        kept.push(news);
                    }
                }
                _ => out.extend(link_event(&event)),
            }
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
