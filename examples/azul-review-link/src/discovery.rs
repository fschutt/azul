//! How the phone finds the Mac again, behind a narrow seam so that the way can change: the addresses it saved at pairing (the
//! first way, and the only one the tests assume), later what the local network announces (iroh's local discovery on the
//! engine's side, Bonjour on iOS). Nothing here depends on how a way works: [`Discovery`] only says who is nearby (with the
//! text each announced, read through [`crate::advert`]) and who went away, so a mock and the engine's list fit the same shape.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::advert::{parse, Advert};
use crate::link::Millis;

/// The most dialing strings the phone keeps for its Mac (the newest first).
pub const MAX_SAVED_TICKETS: usize = 4;

/// One endpoint that announced itself on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nearby {
    /// Its key.
    pub endpoint_id: String,
    /// What it announced, when that is one of ours (anything else a neighbour writes is not read).
    pub advert: Option<Advert>,
    /// The addresses it can be dialed at.
    pub addresses: Vec<String>,
}

impl Nearby {
    /// A neighbour from the raw announced text (`None` text, or a foreign one, is no advert).
    #[must_use]
    pub fn from_announcement(endpoint_id: &str, text: &str, addresses: Vec<String>) -> Nearby {
        Nearby { endpoint_id: endpoint_id.to_string(), advert: parse(text), addresses }
    }
}

/// What changed in the neighbourhood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NearbyEvent {
    Appeared(Nearby),
    /// The endpoint with this key is gone.
    Gone(String),
}

/// A way to find peers on the local network.
pub trait Discovery {
    /// Who is nearby now.
    fn nearby(&self) -> Vec<Nearby>;

    /// What changed since the last call.
    fn poll(&mut self, now: Millis) -> Vec<NearbyEvent>;
}

/// The Mac's address as the phone saved it: the first way to find it again, no discovery at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SavedAddresses {
    tickets: Vec<String>,
}

impl SavedAddresses {
    /// Nothing saved.
    #[must_use]
    pub fn new() -> SavedAddresses {
        SavedAddresses::default()
    }

    /// Starts from tickets kept before (newest first); more than the cap are left out.
    #[must_use]
    pub fn from_saved(tickets: &[String]) -> SavedAddresses {
        let mut saved = SavedAddresses::new();
        for ticket in tickets.iter().rev() {
            saved.remember(ticket);
        }
        saved
    }

    /// Remembers a ticket as the newest (a ticket seen again moves to the front; the oldest beyond the cap is forgotten).
    pub fn remember(&mut self, ticket: &str) {
        if ticket.is_empty() {
            return;
        }
        self.tickets.retain(|t| t != ticket);
        self.tickets.insert(0, ticket.to_string());
        self.tickets.truncate(MAX_SAVED_TICKETS);
    }

    /// What to dial, the newest first.
    #[must_use]
    pub fn candidates(&self) -> &[String] {
        &self.tickets
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.tickets.clear();
    }
}

/// A discovery that a test steers: neighbours appear and go when it says so. Clones share the neighbourhood, so a test keeps a
/// handle while the client owns another.
#[derive(Debug, Clone, Default)]
pub struct MockDiscovery {
    inner: Arc<Mutex<Neighbourhood>>,
}

#[derive(Debug, Default)]
struct Neighbourhood {
    here: Vec<Nearby>,
    news: Vec<NearbyEvent>,
}

impl MockDiscovery {
    /// Nobody nearby.
    #[must_use]
    pub fn new() -> MockDiscovery {
        MockDiscovery::default()
    }

    fn lock(&self) -> MutexGuard<'_, Neighbourhood> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A neighbour appears (one with the same key is replaced).
    pub fn appear(&self, nearby: Nearby) {
        let mut hood = self.lock();
        hood.here.retain(|n| n.endpoint_id != nearby.endpoint_id);
        hood.here.push(nearby.clone());
        hood.news.push(NearbyEvent::Appeared(nearby));
    }

    /// A neighbour goes away.
    pub fn vanish(&self, endpoint_id: &str) {
        let mut hood = self.lock();
        if hood.here.iter().any(|n| n.endpoint_id == endpoint_id) {
            hood.here.retain(|n| n.endpoint_id != endpoint_id);
            hood.news.push(NearbyEvent::Gone(endpoint_id.to_string()));
        }
    }
}

impl Discovery for MockDiscovery {
    fn nearby(&self) -> Vec<Nearby> {
        self.lock().here.clone()
    }

    fn poll(&mut self, _now: Millis) -> Vec<NearbyEvent> {
        std::mem::take(&mut self.lock().news)
    }
}

#[cfg(test)]
#[path = "discovery_test.rs"]
mod discovery_test;
