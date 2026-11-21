//! S1-01: the simulated network injects what it says (planning/architecture.md section 1.4 rule 2:
//! loss, delay, reordering, duplicates, partitions; rule 4: bounded). Every test enters through
//! the public API.

use azlin_seams::sim::Rng;
use azlin_sim::config::{FaultRates, Ppm, MAX_INFLIGHT};
use azlin_sim::draw::step_rng;
use azlin_sim::net::{Corrupt, NetRefusal, SimNet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Msg(u32);

impl Corrupt for Msg {
    fn corrupt(&self, _rng: &mut Rng) -> Msg {
        Msg(self.0 ^ 0x8000_0000)
    }
}

fn rates() -> FaultRates {
    FaultRates::none()
}

fn ppm(parts: u32) -> Ppm {
    Ppm::new(parts).expect("a rate of the test is at most one million")
}

#[test]
fn s1_01_loss_duplication_and_corruption_hit_at_their_rates() {
    let mut r = rates();
    r.loss = ppm(50_000);
    r.duplication = ppm(50_000);
    r.corruption = ppm(50_000);
    let mut net: SimNet<Msg> = SimNet::with_limit(100_000);
    let mut lost = 0u64;
    for i in 0..10_000u32 {
        let mut rng = step_rng(0xA11CE, i);
        let report = net
            .send(0, 0, 1, Msg(i), &r, &mut rng)
            .expect("room for 10,000 messages");
        if report.lost {
            lost += 1;
        }
    }
    let stats = net.stats();
    assert_eq!(stats.sent, 10_000);
    assert_eq!(stats.lost, lost);
    for (name, count) in [
        ("lost", stats.lost),
        ("duplicated", stats.duplicated),
        ("corrupted", stats.corrupted),
    ] {
        assert!(
            (400..=600).contains(&count),
            "{name}: {count} of 10,000 at 50,000 ppm (expected 500 +- 100)"
        );
    }
    // Every message that was not lost is in flight, once or twice.
    assert_eq!(net.len() as u64, 10_000 - stats.lost + stats.duplicated);
}

#[test]
fn s1_01_a_rate_of_zero_never_fires_and_a_rate_of_a_million_always_does() {
    let mut net: SimNet<Msg> = SimNet::with_limit(100_000);
    for i in 0..2_000u32 {
        let mut rng = step_rng(7, i);
        net.send(0, 0, 1, Msg(i), &rates(), &mut rng).expect("room");
    }
    let stats = net.stats();
    assert_eq!(
        (
            stats.lost,
            stats.duplicated,
            stats.corrupted,
            stats.reordered
        ),
        (0, 0, 0, 0)
    );
    let mut all = rates();
    all.loss = ppm(1_000_000);
    let mut rng = step_rng(7, 9_999);
    let report = net.send(0, 0, 1, Msg(0), &all, &mut rng).expect("room");
    assert!(report.lost);
}

#[test]
fn s1_01_a_delay_range_is_never_exceeded() {
    let mut r = rates();
    r.delay_secs = (3, 9);
    let mut net: SimNet<Msg> = SimNet::with_limit(100_000);
    for i in 0..3_000u32 {
        let mut rng = step_rng(21, i);
        net.send(100, 0, 1, Msg(i), &r, &mut rng).expect("room");
    }
    let (mut lowest, mut highest) = (u64::MAX, 0);
    for envelope in net.iter() {
        lowest = lowest.min(envelope.deliver_at);
        highest = highest.max(envelope.deliver_at);
    }
    assert!(lowest >= 103, "lowest {lowest}");
    assert!(highest <= 109, "highest {highest}");
    assert!(
        lowest <= 104 && highest >= 108,
        "the whole range is used: {lowest}..{highest}"
    );
}

/// The delivery order of a batch of 200 messages sent at once with one fixed delay, as the numbers
/// in the order they come out.
fn delivery_order(reordering: Ppm, seed: u64) -> Vec<u32> {
    let mut r = rates();
    r.delay_secs = (5, 5);
    r.reordering = reordering;
    let mut net: SimNet<Msg> = SimNet::new();
    for i in 0..200u32 {
        let mut rng = step_rng(seed, i);
        net.send(0, 0, 1, Msg(i), &r, &mut rng)
            .expect("room for 200 messages");
    }
    let mut order = Vec::new();
    while let Some(envelope) = net.pop_due(1_000_000) {
        order.push(envelope.msg.0);
    }
    order
}

#[test]
fn s1_01_reordering_changes_the_order_when_on_and_never_when_off() {
    let off = delivery_order(Ppm::ZERO, 3);
    assert_eq!(
        off,
        (0..200).collect::<Vec<u32>>(),
        "no reordering: the order of sending"
    );
    let on = delivery_order(ppm(100_000), 3);
    assert_eq!(on.len(), 200);
    let mut inverted = 0u32;
    let mut pairs = 0u32;
    for (i, a) in on.iter().enumerate() {
        for b in on.iter().skip(i + 1) {
            pairs += 1;
            if a > b {
                inverted += 1;
            }
        }
    }
    assert!(
        u64::from(inverted) * 100 >= u64::from(pairs) * 5,
        "{inverted} of {pairs} pairs inverted: under 5 %"
    );
}

#[test]
fn s1_01_a_partition_holds_messages_across_the_cut_and_delivers_them_after_the_heal_in_order() {
    let mut net: SimNet<Msg> = SimNet::new();
    let r = rates();
    let mut rng = Rng::new(1);
    // Nodes 0 (leader), 1 and 2 (devices). Node 1 is cut off.
    net.partition(0b010);
    for i in 0..5u32 {
        net.send(0, 0, 1, Msg(i), &r, &mut rng).expect("room");
    }
    net.send(0, 0, 2, Msg(100), &r, &mut rng).expect("room");
    net.send(0, 1, 0, Msg(200), &r, &mut rng).expect("room");
    // Across the cut nothing is delivered, however late it is; on the same side it is.
    let mut got = Vec::new();
    while let Some(envelope) = net.pop_due(1_000) {
        got.push((envelope.to, envelope.msg.0));
    }
    assert_eq!(got, vec![(2, 100)]);
    assert_eq!(
        net.len(),
        6,
        "the held messages are still in flight, not dropped"
    );
    assert!(net.is_cut(0, 1) && net.is_cut(1, 0) && !net.is_cut(0, 2));
    net.heal(0b010);
    let mut got = Vec::new();
    while let Some(envelope) = net.pop_due(1_000) {
        got.push((envelope.to, envelope.msg.0));
    }
    assert_eq!(got, vec![(1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (0, 200)]);
    assert!(net.is_empty());
}

#[test]
fn s1_01_a_slow_node_multiplies_the_delays_to_and_from_it() {
    let mut r = rates();
    r.delay_secs = (2, 2);
    let mut net: SimNet<Msg> = SimNet::new();
    let mut rng = Rng::new(5);
    net.set_slow(1, 4);
    net.send(10, 0, 1, Msg(1), &r, &mut rng).expect("room");
    net.send(10, 1, 0, Msg(2), &r, &mut rng).expect("room");
    net.send(10, 0, 2, Msg(3), &r, &mut rng).expect("room");
    let at: Vec<(u32, u64)> = net.iter().map(|e| (e.msg.0, e.deliver_at)).collect();
    assert!(
        at.contains(&(1, 18)) && at.contains(&(2, 18)) && at.contains(&(3, 12)),
        "{at:?}"
    );
    net.set_slow(1, 1);
    net.send(10, 0, 1, Msg(4), &r, &mut rng).expect("room");
    assert!(net.iter().any(|e| e.msg.0 == 4 && e.deliver_at == 12));
}

#[test]
fn s1_01_past_the_bound_a_send_is_refused_counted_and_never_evicts() {
    let mut net: SimNet<Msg> = SimNet::new();
    assert_eq!(net.limit(), MAX_INFLIGHT);
    let r = rates();
    let mut rng = Rng::new(9);
    for i in 0..MAX_INFLIGHT as u32 {
        net.send(0, 0, 1, Msg(i), &r, &mut rng)
            .expect("under the bound");
    }
    assert_eq!(net.len(), MAX_INFLIGHT);
    assert_eq!(
        net.send(0, 0, 1, Msg(999), &r, &mut rng),
        Err(NetRefusal::Full)
    );
    assert_eq!(
        net.send(0, 0, 1, Msg(1_000), &r, &mut rng),
        Err(NetRefusal::Full)
    );
    assert_eq!(net.stats().refused_full, 2);
    assert_eq!(
        net.len(),
        MAX_INFLIGHT,
        "no message was evicted for the new one"
    );
    assert!(
        net.iter().all(|e| e.msg.0 < MAX_INFLIGHT as u32),
        "the first messages are all still there"
    );
    // A delivered message makes room again.
    assert!(net.pop_due(10).is_some());
    assert!(net.send(0, 0, 1, Msg(1_001), &r, &mut rng).is_ok());
}

#[test]
fn s1_01_the_same_salt_gives_the_same_draws() {
    let mut r = rates();
    r.delay_secs = (0, 30);
    r.loss = ppm(300_000);
    r.reordering = ppm(300_000);
    let run = |seed: u64| -> Vec<(bool, u8, bool, bool)> {
        let mut net: SimNet<Msg> = SimNet::new();
        (0..100u32)
            .map(|i| {
                let mut rng = step_rng(seed, i);
                let report = net.send(0, 0, 1, Msg(i), &r, &mut rng).expect("room");
                (
                    report.lost,
                    report.copies,
                    report.corrupted,
                    report.reordered,
                )
            })
            .collect()
    };
    assert_eq!(run(11), run(11));
    assert_ne!(run(11), run(12));
}
