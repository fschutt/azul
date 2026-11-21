//! The simulated network (planning/architecture.md section 1.4 rule 2: loss, delay, reordering,
//! duplicates, partitions; rule 4: a bound on what is in flight).
//!
//! [`SimNet`] keeps a bounded map of in-flight messages keyed by `(deliver_at, sequence)`. `send`
//! draws loss, duplication, delay, reordering and corruption from the generator of the step that
//! sends (rule H2); a partition HOLDS the messages between the cut groups until `heal` (the real
//! world delays, and loss is its own fault); a slow node multiplies the delay of the messages to
//! and from it; past the bound a send is refused with [`NetRefusal::Full`] and counted, never an
//! eviction.

use std::collections::BTreeMap;

use azlin_seams::sim::Rng;

use crate::config::{FaultRates, MAX_INFLIGHT};
use crate::draw::between;

/// A message that can be damaged in flight: the copy differs from the original in one byte of a
/// signature or a payload (never in the hash a copy claims).
///
/// Implements planning/architecture.md section 1.4 rule 2 (the simulator injects corruption) and
/// section 5.9 (a corrupt copy is fetched again).
///
/// Caller: `SimNet::send`, `world::Msg`.
pub trait Corrupt: Clone {
    /// A damaged copy of the message.
    fn corrupt(&self, rng: &mut Rng) -> Self;
}

/// A message on its way.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the network).
///
/// Caller: `SimNet::pop_due`, `world::World`.
#[derive(Clone, Debug)]
pub struct Envelope<T> {
    /// The sender (a node index: 0 is the leader).
    pub from: usize,
    /// The receiver.
    pub to: usize,
    /// The message.
    pub msg: T,
    /// The time it falls due (global simulated seconds).
    pub deliver_at: u64,
    /// Whether this copy was damaged in flight.
    pub corrupted: bool,
}

/// Why a send is refused.
///
/// Implements planning/architecture.md section 1.4 rule 4 (a limit is a refusal with a typed
/// error and a counter, never a silent drop).
///
/// Caller: `SimNet::send`, tests/s1_01.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRefusal {
    /// The network holds as many messages as it may.
    Full,
}

error_text!(NetRefusal {
    NetRefusal::Full => "the network is full",
});

/// What the network did, in counters.
///
/// Implements planning/architecture.md section 1.4 rules 2 and 4 (every fault and every refusal
/// is counted).
///
/// Caller: `SimNet::stats`, `Coverage`, tests/s1_01.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetStats {
    /// Sends asked for.
    pub sent: u64,
    /// Messages lost.
    pub lost: u64,
    /// Messages delivered twice.
    pub duplicated: u64,
    /// Messages given extra delay.
    pub reordered: u64,
    /// Messages damaged in flight.
    pub corrupted: u64,
    /// Sends refused because the network was full.
    pub refused_full: u64,
    /// Messages handed to a receiver.
    pub delivered: u64,
}

/// What one send did.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the network).
///
/// Caller: `SimNet::send`, `world::World`, tests/s1_01.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SendReport {
    /// The message was lost.
    pub lost: bool,
    /// How many copies are in flight now (0, 1 or 2).
    pub copies: u8,
    /// The first copy was damaged.
    pub corrupted: bool,
    /// The first copy got extra delay.
    pub reordered: bool,
}

/// The simulated network.
///
/// Implements planning/architecture.md section 1.4 rule 2 and rule 4.
///
/// Caller: `world::World`, tests/s1_01.
#[derive(Clone, Debug)]
pub struct SimNet<T> {
    inflight: BTreeMap<(u64, u64), Envelope<T>>,
    next_seq: u64,
    cut: u16,
    slow: BTreeMap<usize, u8>,
    stats: NetStats,
    limit: usize,
}

impl<T: Corrupt> Default for SimNet<T> {
    fn default() -> SimNet<T> {
        SimNet::new()
    }
}

impl<T: Corrupt> SimNet<T> {
    /// An empty network with the bound [`MAX_INFLIGHT`].
    pub fn new() -> SimNet<T> {
        SimNet::with_limit(MAX_INFLIGHT)
    }

    /// An empty network that holds at most `limit` messages.
    pub fn with_limit(limit: usize) -> SimNet<T> {
        SimNet {
            inflight: BTreeMap::new(),
            next_seq: 0,
            cut: 0,
            slow: BTreeMap::new(),
            stats: NetStats::default(),
            limit,
        }
    }

    /// How many messages are in flight.
    pub fn len(&self) -> usize {
        self.inflight.len()
    }

    /// Whether nothing is in flight.
    pub fn is_empty(&self) -> bool {
        self.inflight.is_empty()
    }

    /// The bound.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The counters.
    pub fn stats(&self) -> NetStats {
        self.stats
    }

    /// The nodes that are cut off (a bit mask).
    pub fn cut_mask(&self) -> u16 {
        self.cut
    }

    /// Cuts the nodes of `mask` off the others: messages between the two sides are held.
    pub fn partition(&mut self, mask: u16) {
        self.cut |= mask;
    }

    /// Joins the nodes of `mask` to the others again.
    pub fn heal(&mut self, mask: u16) {
        self.cut &= !mask;
    }

    /// Joins everything again.
    pub fn heal_all(&mut self) {
        self.cut = 0;
    }

    /// Whether the two nodes are on different sides of the cut.
    pub fn is_cut(&self, a: usize, b: usize) -> bool {
        let side =
            |node: usize| u32::try_from(node).is_ok_and(|n| n < 16 && (self.cut >> n) & 1 == 1);
        side(a) != side(b)
    }

    /// Multiplies the delays of the messages to and from `node` by `factor` (at least 1).
    pub fn set_slow(&mut self, node: usize, factor: u8) {
        if factor <= 1 {
            self.slow.remove(&node);
        } else {
            self.slow.insert(node, factor);
        }
    }

    /// Removes every slow node.
    pub fn clear_slow(&mut self) {
        self.slow.clear();
    }

    fn factor(&self, from: usize, to: usize) -> u64 {
        let of = |node: usize| u64::from(self.slow.get(&node).copied().unwrap_or(1));
        of(from).max(of(to))
    }

    fn insert(&mut self, envelope: Envelope<T>) {
        let key = (envelope.deliver_at, self.next_seq);
        self.next_seq = self.next_seq.saturating_add(1);
        self.inflight.insert(key, envelope);
    }

    /// Sends a message at global time `now`: loss, duplication, delay, reordering and corruption
    /// are drawn from `rng` in this order. A full network refuses the send.
    pub fn send(
        &mut self,
        now: u64,
        from: usize,
        to: usize,
        msg: T,
        rates: &FaultRates,
        rng: &mut Rng,
    ) -> Result<SendReport, NetRefusal> {
        self.stats.sent = self.stats.sent.saturating_add(1);
        if self.inflight.len() >= self.limit {
            self.stats.refused_full = self.stats.refused_full.saturating_add(1);
            return Err(NetRefusal::Full);
        }
        let (min, max) = (u64::from(rates.delay_secs.0), u64::from(rates.delay_secs.1));
        let lost = rates.loss.hit(rng);
        let duplicate = rates.duplication.hit(rng);
        let delay = between(rng, min, max);
        let reorder = rates.reordering.hit(rng);
        let extra = between(rng, 1, max.saturating_mul(2).max(5));
        let corrupt = rates.corruption.hit(rng);
        let delay_again = between(rng, min, max);
        if lost {
            self.stats.lost = self.stats.lost.saturating_add(1);
            return Ok(SendReport {
                lost: true,
                ..SendReport::default()
            });
        }
        let factor = self.factor(from, to);
        let extra = if reorder { extra } else { 0 };
        let first_at = now.saturating_add(delay.saturating_add(extra).saturating_mul(factor));
        let first = if corrupt {
            msg.corrupt(rng)
        } else {
            msg.clone()
        };
        let mut report = SendReport {
            lost: false,
            copies: 1,
            corrupted: corrupt,
            reordered: reorder,
        };
        if corrupt {
            self.stats.corrupted = self.stats.corrupted.saturating_add(1);
        }
        if reorder {
            self.stats.reordered = self.stats.reordered.saturating_add(1);
        }
        let second = if duplicate {
            if self.inflight.len().saturating_add(1) >= self.limit {
                self.stats.refused_full = self.stats.refused_full.saturating_add(1);
                None
            } else {
                Some(Envelope {
                    from,
                    to,
                    msg: msg.clone(),
                    deliver_at: now.saturating_add(delay_again.saturating_mul(factor)),
                    corrupted: false,
                })
            }
        } else {
            None
        };
        self.insert(Envelope {
            from,
            to,
            msg: first,
            deliver_at: first_at,
            corrupted: corrupt,
        });
        if let Some(copy) = second {
            self.stats.duplicated = self.stats.duplicated.saturating_add(1);
            report.copies = 2;
            self.insert(copy);
        }
        Ok(report)
    }

    /// The earliest message that is due at `now` and not held by a partition, removed from the
    /// network.
    pub fn pop_due(&mut self, now: u64) -> Option<Envelope<T>> {
        let key = self
            .inflight
            .iter()
            .find(|(key, envelope)| key.0 <= now && !self.is_cut(envelope.from, envelope.to))
            .map(|(key, _)| *key)?;
        let envelope = self.inflight.remove(&key)?;
        self.stats.delivered = self.stats.delivered.saturating_add(1);
        Some(envelope)
    }

    /// The time the earliest message in flight falls due, whether or not a partition holds it.
    pub fn next_due(&self) -> Option<u64> {
        self.inflight.keys().next().map(|key| key.0)
    }

    /// The time the earliest message that is not held by a partition falls due.
    pub fn next_deliverable(&self) -> Option<u64> {
        self.inflight
            .iter()
            .find(|(_, envelope)| !self.is_cut(envelope.from, envelope.to))
            .map(|(key, _)| key.0)
    }

    /// Drops the messages in flight to or from `node` (a node that is wiped has no use for them);
    /// the count is returned.
    pub fn drop_for(&mut self, node: usize) -> usize {
        let before = self.inflight.len();
        self.inflight
            .retain(|_, envelope| envelope.from != node && envelope.to != node);
        before.saturating_sub(self.inflight.len())
    }

    /// The messages in flight, earliest first.
    pub fn iter(&self) -> impl Iterator<Item = &Envelope<T>> {
        self.inflight.values()
    }
}
