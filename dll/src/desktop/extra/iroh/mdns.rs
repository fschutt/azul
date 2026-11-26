//! Local-network discovery through mDNS (`iroh-mdns-address-lookup` on swarm-discovery): every
//! engine target but iOS (`cfg(az_iroh_mdns)`, see build.rs).
//!
//! The service is `irohv1`: an endpoint is announced as `<endpoint id, base32>._irohv1._udp.local`
//! with an SRV and an address record per port and a TXT record, on UDP 5353 of the multicast groups
//! 224.0.0.251 and ff02::fb, every interface. From swarm-discovery 0.6.5 on the announcement carries
//! a TTL and the PTR record DNS-SD browsers need, so the system's own Bonjour (the iOS lookup of
//! `bonjour.rs`) and Android's NsdManager see it too.

use std::{
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use iroh::Endpoint;
use iroh_mdns_address_lookup::{DiscoveryEvent, MdnsAddressLookup};
use n0_future::StreamExt;

use super::{
    discovery::{announce_filter, Held, LocalDiscovery, LocalLookup},
    multicast_lock,
    nearby::Nearby,
};

/// How often an endpoint asks the local network for peers (and so how long a new peer can stay
/// unseen): iroh's default is 700 ms, which keeps a computer on the network talking all day; a
/// peer that was announced is found at once, and one that appears later within this time.
const QUERY_INTERVAL: Duration = Duration::from_secs(5);

/// mDNS on raw multicast sockets.
pub(super) struct MdnsDiscovery;

impl LocalDiscovery for MdnsDiscovery {
    fn start(
        &self,
        endpoint: &Endpoint,
        announce: bool,
        ignored: &Arc<AtomicU64>,
        nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        // Android's Wi-Fi chip drops multicast unless the app holds this lock; nothing to hold
        // elsewhere. Taken first: a refused lock means nothing would arrive.
        let held = multicast_lock::acquire()?;
        let mdns = MdnsAddressLookup::builder()
            .advertise(announce)
            .addr_filter(announce_filter())
            .discovery_cadence(QUERY_INTERVAL)
            .build(endpoint.id())
            .map_err(|e| format!("{e:#}"))?;
        let services = endpoint.address_lookup().map_err(|e| format!("{e:#}"))?;
        // The endpoints of the network as they announce themselves, whether a dial asks for them
        // or not: the lookup tells a subscriber of each new or changed announcement and of each
        // that expired. The task waits on the stream only (the lookup it was made from is not
        // kept), so it ends when the endpoint, and with it the lookup, is gone.
        let feed = mdns.clone();
        let list = nearby.clone();
        tokio::spawn(async move {
            let mut events = feed.subscribe().await;
            drop(feed);
            while let Some(event) = events.next().await {
                match event {
                    DiscoveryEvent::Discovered { endpoint_info, .. } => list.seen(&endpoint_info),
                    DiscoveryEvent::Expired { endpoint_id } => list.gone(&endpoint_id),
                    _ => {}
                }
            }
        });
        services.add(LocalLookup::new(mdns, ignored.clone(), nearby.clone()));
        Ok(held)
    }
}
