//! Local-network discovery of the iroh engine: the shape of it, with bodies that do nothing yet
//! (the tests of `discovery_test.rs` and `engine_test.rs` come first and fail against them).

#![allow(unused_imports, dead_code)]

use std::sync::{atomic::AtomicU64, Arc};

use iroh::{
    address_lookup::{
        AddrFilter, AddressLookup, EndpointData, EndpointInfo, Error as LookupError, Item,
    },
    Endpoint, EndpointId, TransportAddr,
};
use n0_future::boxed::BoxStream;

use super::types::IrohConfig;

pub(super) const MAX_ADDRESSES_PER_PEER: usize = 16;

pub(super) type Held = Box<dyn std::any::Any + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wanted {
    Off,
    Look,
    LookAndAnnounce,
}

pub(super) fn wanted(_config: &IrohConfig) -> Wanted {
    Wanted::Off
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LocalDiscoveryState {
    Off,
    On { announcing: bool },
    Unavailable(String),
}

impl LocalDiscoveryState {
    pub(super) fn status(&self, _ignored: u64) -> String {
        String::new()
    }
}

pub(super) trait LocalDiscovery: Send + Sync {
    fn start(
        &self,
        endpoint: &Endpoint,
        announce: bool,
        ignored: &Arc<AtomicU64>,
    ) -> Result<Option<Held>, String>;
}

struct Nothing;

impl LocalDiscovery for Nothing {
    fn start(
        &self,
        _endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
    ) -> Result<Option<Held>, String> {
        Ok(None)
    }
}

pub(super) fn platform() -> Box<dyn LocalDiscovery> {
    Box::new(Nothing)
}

pub(super) fn usable_peer_addr(_addr: &std::net::SocketAddr) -> bool {
    true
}

pub(super) fn announced_on_the_link(_addr: &TransportAddr) -> bool {
    true
}

pub(super) fn announce_filter() -> AddrFilter {
    AddrFilter::unfiltered()
}

pub(super) fn sanitised(item: Item, _ignored: &AtomicU64) -> Item {
    item
}

#[derive(Debug)]
pub(super) struct LocalLookup<L> {
    inner: L,
    #[allow(dead_code)]
    ignored: Arc<AtomicU64>,
}

impl<L> LocalLookup<L> {
    pub(super) fn new(inner: L, ignored: Arc<AtomicU64>) -> Self {
        LocalLookup { inner, ignored }
    }
}

impl<L: AddressLookup> AddressLookup for LocalLookup<L> {
    fn publish(&self, data: &EndpointData) {
        self.inner.publish(data);
    }

    fn resolve(&self, endpoint_id: EndpointId) -> Option<BoxStream<Result<Item, LookupError>>> {
        self.inner.resolve(endpoint_id)
    }
}

#[cfg(test)]
#[path = "discovery_test.rs"]
mod tests;
