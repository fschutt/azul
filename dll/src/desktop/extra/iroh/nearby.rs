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
        Mutex, MutexGuard,
    },
};

use iroh::{address_lookup::EndpointInfo, EndpointId};

use azul_css::{AzString, StringVec};

use super::{
    discovery::{usable_peer_addr, MAX_ADDRESSES_PER_PEER},
    types::{IrohEvent, IrohEventKind, IrohNearbyPeer},
};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !is_bidi_override(*c))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.len() <= MAX_ADVERTISED_BYTES {
        return Advertised {
            text: cleaned.to_string(),
            cut_from: None,
        };
    }
    let mut end = MAX_ADVERTISED_BYTES;
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    Advertised {
        text: cleaned[..end].trim_end().to_string(),
        cut_from: Some(cleaned.len()),
    }
}

/// The characters that embed, override or isolate a direction of writing (U+202A to U+202E and
/// U+2066 to U+2069): in a name they make the text around it read backwards.
fn is_bidi_override(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
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
        advertised: info
            .data
            .user_data()
            .map(|text| advertised_text(AsRef::<str>::as_ref(text)).text)
            .unwrap_or_default(),
        addresses: info
            .data
            .ip_addrs()
            .filter(|address| usable_peer_addr(address))
            .take(MAX_ADDRESSES_PER_PEER)
            .copied()
            .collect(),
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
        if info.endpoint_id == self.own {
            return;
        }
        let peer = nearby_of(info);
        let mut inner = lock(&self.inner);
        if !inner.peers.contains_key(&peer.id) && inner.peers.len() >= MAX_NEARBY_PEERS {
            self.refused.fetch_add(1, Ordering::Relaxed);
            return;
        }
        inner.peers.insert(peer.id, peer);
    }

    /// The endpoint `id` is no longer announcing.
    pub(super) fn gone(&self, id: &EndpointId) {
        lock(&self.inner).peers.remove(id);
    }

    /// The endpoints on the network now, by endpoint id.
    pub(super) fn list(&self) -> Vec<NearbyPeer> {
        lock(&self.inner).peers.values().cloned().collect()
    }

    /// The next change the app has not been told of: `NearbyAppeared` for an endpoint that is new
    /// or whose announcement changed (once, however often it changed since the app asked last),
    /// `NearbyGone` for one it was told of that vanished. `None` when the app is up to date.
    pub(super) fn take_event(&self) -> Option<IrohEvent> {
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        // an endpoint the app was told of that has vanished
        let vanished = inner
            .reported
            .keys()
            .find(|id| !inner.peers.contains_key(*id))
            .copied();
        if let Some(id) = vanished {
            inner.reported.remove(&id);
            return Some(IrohEvent::new(IrohEventKind::NearbyGone, 0, id.to_string()));
        }
        // one that is new, or whose announcement is not what the app was told
        let changed = inner
            .peers
            .iter()
            .find(|(id, peer)| inner.reported.get(*id) != Some(*peer))
            .map(|(id, peer)| (*id, peer.clone()));
        let (id, peer) = changed?;
        inner.reported.insert(id, peer.clone());
        let mut event = IrohEvent::payload(
            IrohEventKind::NearbyAppeared,
            0,
            0,
            0,
            peer.advertised.into_bytes(),
        );
        event.text = AzString::from_string(id.to_string());
        Some(event)
    }

    /// How many announcements were not listed because the list was full.
    pub(super) fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "nearby_test.rs"]
mod tests;
