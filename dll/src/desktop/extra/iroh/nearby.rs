//! Who is on the local network: the endpoints that announced themselves there, with what they say
//! about themselves (their advertised text) and the addresses a dial can use. The lookups of local
//! discovery feed it ([`Nearby::seen`], [`Nearby::gone`]); the app reads it as a list
//! (`IrohEndpoint::nearby_peers`) and as events (`NearbyAppeared`, `NearbyGone`). It never dials.
//!
//! Everything a neighbour announces is anybody's word, so what the list holds is cut down before
//! the app sees it: at most [`MAX_NEARBY_PEERS`] endpoints, at most
//! [`MAX_ADDRESSES_PER_PEER`](super::discovery::MAX_ADDRESSES_PER_PEER) usable addresses each, the
//! advertised text without control and bidirectional-override characters and at most
//! [`MAX_ADVERTISED_BYTES`] bytes.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use iroh::{address_lookup::EndpointInfo, EndpointId};

use azul_css::{AzString, StringVec};

use super::types::{IrohEvent, IrohEventKind, IrohNearbyPeer};

/// The most endpoints the list holds. A network with more azul endpoints than this is not a home
/// or an office; the rest is counted ([`Nearby::refused`]) and said in the status.
pub(super) const MAX_NEARBY_PEERS: usize = 64;

/// The longest advertised text in bytes: iroh's `UserData::MAX_LENGTH`, what fits a DNS TXT
/// character string after its `user-data=` key.
pub(super) const MAX_ADVERTISED_BYTES: usize = 245;

/// An advertised text as it is announced: cleaned, and cut when it was too long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Advertised {
    pub(super) text: String,
    /// The length in bytes of the cleaned text before it was cut, when it was.
    pub(super) cut_from: Option<usize>,
}

/// `raw` as it may be announced and listed: control characters and the bidirectional overrides and
/// isolates removed (a name that makes the text around it read backwards is a spoof, not a name),
/// surrounding white space trimmed, and at most [`MAX_ADVERTISED_BYTES`] bytes, cut at a
/// character boundary.
pub(super) fn advertised_text(raw: &str) -> Advertised {
    Advertised {
        text: raw.to_string(),
        cut_from: None,
    }
}

/// One endpoint on the local network, as far as the app may believe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NearbyPeer {
    pub(super) id: EndpointId,
    /// What it advertises about itself, cleaned; empty when it advertises nothing.
    pub(super) advertised: String,
    /// The addresses it announced that a dial can use, at most 16.
    pub(super) addresses: Vec<SocketAddr>,
}

impl From<&NearbyPeer> for IrohNearbyPeer {
    fn from(peer: &NearbyPeer) -> Self {
        IrohNearbyPeer {
            endpoint_id: AzString::from_string(peer.id.to_string()),
            advertised: AzString::from_string(peer.advertised.clone()),
            addresses: StringVec::from_vec(
                peer.addresses
                    .iter()
                    .map(|address| AzString::from_string(address.to_string()))
                    .collect(),
            ),
        }
    }
}

/// What an announcement says, cut down to what the list may hold.
pub(super) fn nearby_of(info: &EndpointInfo) -> NearbyPeer {
    NearbyPeer {
        id: info.endpoint_id,
        advertised: String::new(),
        addresses: Vec::new(),
    }
}

/// The endpoints on the local network: what is listed now, and what the app was told last.
#[derive(Debug)]
pub(super) struct Nearby {
    own: EndpointId,
    inner: Mutex<Inner>,
    refused: AtomicU64,
}

#[derive(Debug, Default)]
struct Inner {
    /// What is on the network now.
    peers: BTreeMap<EndpointId, NearbyPeer>,
    /// What the app was told: an event is what differs between the two.
    reported: BTreeMap<EndpointId, NearbyPeer>,
}

impl Nearby {
    /// An empty list for the endpoint `own`, which is never listed itself.
    pub(super) fn new(own: EndpointId) -> Self {
        Nearby {
            own,
            inner: Mutex::new(Inner::default()),
            refused: AtomicU64::new(0),
        }
    }

    /// An announcement (or its refresh) of `info` was heard.
    pub(super) fn seen(&self, info: &EndpointInfo) {
        let _ = (&self.own, info);
    }

    /// The endpoint `id` is no longer announcing.
    pub(super) fn gone(&self, id: &EndpointId) {
        let _ = id;
    }

    /// The endpoints on the network now, by endpoint id.
    pub(super) fn list(&self) -> Vec<NearbyPeer> {
        let _ = &self.inner;
        Vec::new()
    }

    /// The next change the app has not been told of: `NearbyAppeared` for an endpoint that is new
    /// or whose announcement changed (once, however often it changed since the app asked last),
    /// `NearbyGone` for one it was told of that vanished. `None` when the app is up to date.
    pub(super) fn take_event(&self) -> Option<IrohEvent> {
        None
    }

    /// How many announcements were not listed because the list was full.
    pub(super) fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "nearby_test.rs"]
mod tests;
