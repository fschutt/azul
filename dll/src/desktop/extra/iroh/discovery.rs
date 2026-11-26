//! Local-network discovery of the iroh engine: an endpoint finds a peer on the same network by the
//! peer's endpoint id alone, with no saved address and no relay, and it announces itself there.
//!
//! What is announced, to the local link only (multicast, never routed): the endpoint id, a public
//! key, and the network addresses of this endpoint that another computer on the link can use
//! ([`announced_on_the_link`]); the relay is not announced. What is believed: an announcement is
//! anybody's, so every address it names passes [`usable_peer_addr`] and the lookup hands the dial at
//! most [`MAX_ADDRESSES_PER_PEER`] of them ([`LocalLookup`]); the relay a neighbour names is never
//! used. Whatever address a neighbour names, the connection is authenticated by the key of the
//! endpoint id that was dialled, so a forged announcement can waste a dial, not take over a peer.
//!
//! The engine asks for it through [`LocalDiscovery`]: mDNS (`mdns.rs`) everywhere but iOS, where the
//! system's Bonjour answers (`bonjour.rs`), and a test gives its own.

use std::{
    borrow::Cow,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use iroh::{
    address_lookup::{
        AddrFilter, AddressLookup, EndpointData, EndpointInfo, Error as LookupError, Item,
    },
    Endpoint, EndpointId, TransportAddr,
};
use n0_future::{boxed::BoxStream, StreamExt};

use super::{nearby::Nearby, types::IrohConfig};

/// The most addresses of one announcement that a dial gets: a computer has a handful (an IPv4
/// address, a few IPv6 ones with their privacy variants); an announcement of hundreds is hostile.
pub(super) const MAX_ADDRESSES_PER_PEER: usize = 16;

/// Whatever a start of local discovery keeps alive for as long as the engine lives (Android's
/// multicast lock); it is dropped with the engine.
pub(super) type Held = Box<dyn std::any::Any + Send + Sync>;

/// What a config asks local discovery to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wanted {
    /// Nothing: switched off, or `relay_only` (no UDP socket, so nothing to find or to announce).
    Off,
    /// Look for peers, do not announce: an endpoint that accepts nobody is a client.
    Look,
    /// Look for peers and announce this endpoint.
    LookAndAnnounce,
}

/// What `config` asks for.
pub(super) fn wanted(config: &IrohConfig) -> Wanted {
    if !config.local_discovery || config.relay_only {
        return Wanted::Off;
    }
    let accepts_nobody = config
        .allowed_peers
        .as_ref()
        .is_some_and(|peers| peers.as_ref().is_empty());
    if accepts_nobody {
        Wanted::Look
    } else {
        Wanted::LookAndAnnounce
    }
}

/// What local discovery is doing for one engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LocalDiscoveryState {
    /// Not asked for.
    Off,
    /// Running; `announcing` says whether this endpoint announces itself.
    On { announcing: bool },
    /// Asked for, and the network or the OS refused it. The endpoint works without it.
    Unavailable(String),
}

impl LocalDiscoveryState {
    /// The text of `IrohEndpoint::local_discovery_status`: it starts with `off`, `on` or
    /// `unavailable:`. `ignored` counts the announced addresses that were not believed.
    pub(super) fn status(&self, ignored: u64) -> String {
        match self {
            LocalDiscoveryState::Off => "off".to_string(),
            LocalDiscoveryState::On { announcing } => {
                let mut text = if *announcing {
                    "on".to_string()
                } else {
                    "on, looking only (this endpoint announces nothing: it accepts nobody, or the \
                     platform, iOS, never announces)"
                        .to_string()
                };
                if ignored > 0 {
                    text.push_str(&format!("; {ignored} announced addresses were not used"));
                }
                text
            }
            LocalDiscoveryState::Unavailable(reason) => format!("unavailable: {reason}"),
        }
    }
}

/// How an endpoint finds peers on the local network. The engine starts one once its endpoint is
/// bound; a test starts its own.
pub(super) trait LocalDiscovery: Send + Sync {
    /// Whether this platform can announce the endpoint at all. An iPhone does not: it is a client
    /// of the Mac, and Bonjour of an app without the multicast entitlement only browses.
    fn can_announce(&self) -> bool {
        true
    }

    /// Starts it for `endpoint`, inside the engine's runtime. `announce`: whether this endpoint
    /// announces itself too; `ignored` counts the announced addresses that were not used;
    /// `nearby` is told of every endpoint that announces itself on the network and of every one
    /// that stops. `Err(reason)`: the network or the OS refused it; the engine keeps the
    /// endpoint without it.
    fn start(
        &self,
        endpoint: &Endpoint,
        announce: bool,
        ignored: &Arc<AtomicU64>,
        nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String>;
}

/// The local-network discovery of this platform: mDNS on raw multicast sockets, and on iOS (where
/// `az_iroh_mdns` is off, see build.rs) the system's Bonjour.
pub(super) fn platform() -> Box<dyn LocalDiscovery> {
    #[cfg(az_iroh_mdns)]
    {
        Box::new(super::mdns::MdnsDiscovery)
    }
    #[cfg(not(az_iroh_mdns))]
    {
        Box::new(super::bonjour::BonjourDiscovery)
    }
}

/// Whether `addr`, taken from an announcement, can be dialled: a port, and an address that is not
/// this computer itself (unspecified, loopback), not a group (multicast, broadcast) and not an
/// IPv6 link-local one (it needs the interface it was heard on, which the announcement does not
/// carry).
pub(super) fn usable_peer_addr(addr: &SocketAddr) -> bool {
    if addr.port() == 0 {
        return false;
    }
    match addr.ip().to_canonical() {
        IpAddr::V4(ip) => {
            !(ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() || ip.is_broadcast())
        }
        IpAddr::V6(ip) => {
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || is_link_local_v6(&ip))
        }
    }
}

/// `fe80::/10`.
fn is_link_local_v6(ip: &Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Whether this endpoint tells the local link about `addr`: an address that another computer on
/// the link can use. A public IPv4 address (what a STUN-like probe reported for the router) is
/// not one; the relay never is.
pub(super) fn announced_on_the_link(addr: &TransportAddr) -> bool {
    match addr {
        TransportAddr::Ip(socket) => {
            usable_peer_addr(socket)
                && match socket.ip().to_canonical() {
                    IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
                    IpAddr::V6(_) => true,
                }
        }
        _ => false,
    }
}

/// The filter the endpoint publishes through to the local link ([`announced_on_the_link`]).
pub(super) fn announce_filter() -> AddrFilter {
    AddrFilter::new(|addrs| {
        Cow::Owned(
            addrs
                .iter()
                .filter(|addr| announced_on_the_link(addr))
                .cloned()
                .collect(),
        )
    })
}

/// What a dial gets of an announcement: the endpoint id, and its direct addresses that
/// [`usable_peer_addr`] accepts, at most [`MAX_ADDRESSES_PER_PEER`] of them. No relay (a neighbour
/// does not choose the relay of a dial) and no user data. `ignored` counts the addresses left out.
pub(super) fn sanitised(item: Item, ignored: &AtomicU64) -> Item {
    let id = item.endpoint_id();
    let provenance = item.provenance();
    let updated = item.last_updated();
    let mut kept: Vec<SocketAddr> = Vec::new();
    let mut left_out = 0u64;
    for addr in item.ip_addrs() {
        if kept.len() < MAX_ADDRESSES_PER_PEER && usable_peer_addr(addr) {
            kept.push(*addr);
        } else {
            left_out += 1;
        }
    }
    if left_out > 0 {
        ignored.fetch_add(left_out, Ordering::Relaxed);
    }
    let mut data = EndpointData::default();
    data.add_ip_addrs(kept);
    Item::new(EndpointInfo::from_parts(id, data), provenance, updated)
}

/// The lookup an endpoint gets for local discovery: `inner` finds the announcements, this keeps
/// what a dial may believe of them ([`sanitised`]).
#[derive(Debug)]
pub(super) struct LocalLookup<L> {
    inner: L,
    ignored: Arc<AtomicU64>,
    nearby: Arc<Nearby>,
}

impl<L> LocalLookup<L> {
    pub(super) fn new(inner: L, ignored: Arc<AtomicU64>, nearby: Arc<Nearby>) -> Self {
        LocalLookup {
            inner,
            ignored,
            nearby,
        }
    }
}

impl<L: AddressLookup> AddressLookup for LocalLookup<L> {
    fn publish(&self, data: &EndpointData) {
        self.inner.publish(data);
    }

    fn resolve(&self, endpoint_id: EndpointId) -> Option<BoxStream<Result<Item, LookupError>>> {
        let ignored = self.ignored.clone();
        let nearby = self.nearby.clone();
        let found = self.inner.resolve(endpoint_id)?;
        Some(Box::pin(found.map(move |found| {
            found.map(|item| {
                // an endpoint that a dial finds is on the network: the list hears it too
                nearby.seen(item.endpoint_info());
                sanitised(item, &ignored)
            })
        })))
    }
}

#[cfg(test)]
#[path = "discovery_test.rs"]
mod tests;
