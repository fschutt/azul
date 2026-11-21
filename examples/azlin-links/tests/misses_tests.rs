//! Check 10: the miss budget (planning/architecture.md section 5.12: "**Bots guessing link ids**:
//! misses at `link_resolve` are budgeted per credential and, at the bridge, per client address in RAM
//! only (R2); a client with many misses gets Retry and slows down"; section 5.15, R1 and R2).
//!
//! Implements planning/architecture.md section 5.12 ("Bots guessing link ids") and section 1.4 (rule
//! 4, limits).

#![allow(dead_code)]

use azlin_links::misses::{
    BudgetKey, MissBudget, MissError, MissTable, MissVerdict, MISS_CAPACITY, MISS_REFILL,
    MISS_TABLE_KEYS,
};
use azlin_seams::sim::Rng;
use azlin_seams::{Span, Stamp};

/// The stamp `secs` seconds after a fixed start (2027-01-15).
fn at(secs: u64) -> Stamp {
    Stamp::from_secs(1_800_000_000 + secs)
}

/// The key of the client `n`.
fn key(n: u8) -> BudgetKey {
    BudgetKey::from_bytes([n; 16])
}

/// A table of `capacity` misses, one back every `refill_secs`, for `max_keys` keys.
fn new_table(capacity: u32, refill_secs: u64, max_keys: usize) -> MissTable {
    let budget = MissBudget::new(capacity, Span::secs(refill_secs)).expect("a budget");
    MissTable::new(budget, max_keys).expect("a table")
}

#[test]
fn miss_budget_refuses_after_capacity_with_retry_after() {
    // Three misses, one coming back every three minutes.
    let mut table = new_table(3, 180, 100);
    let client = key(1);
    assert_eq!(
        table.admit(client, at(0)),
        MissVerdict::Proceed,
        "an unknown key"
    );

    assert_eq!(table.record_miss(client, at(0)), MissVerdict::Proceed);
    assert_eq!(table.record_miss(client, at(0)), MissVerdict::Proceed);
    assert_eq!(
        table.record_miss(client, at(0)),
        MissVerdict::Retry {
            after: Span::minutes(3)
        },
        "the third miss uses the budget up"
    );
    assert_eq!(
        table.admit(client, at(0)),
        MissVerdict::Retry {
            after: Span::minutes(3)
        }
    );

    // The delay counts down, to the second.
    assert_eq!(
        table.admit(client, at(60)),
        MissVerdict::Retry {
            after: Span::secs(120)
        }
    );
    assert_eq!(
        table.admit(client, at(179)),
        MissVerdict::Retry {
            after: Span::secs(1)
        }
    );
    assert_eq!(table.admit(client, at(180)), MissVerdict::Proceed);

    // A client that goes on guessing while it is slowed does not dig itself deeper, and its wait
    // starts again from that miss.
    assert_eq!(
        table.record_miss(client, at(10)),
        MissVerdict::Retry {
            after: Span::secs(180)
        }
    );
    assert_eq!(
        table.admit(client, at(189)),
        MissVerdict::Retry {
            after: Span::secs(1)
        }
    );
    assert_eq!(
        table.admit(client, at(190)),
        MissVerdict::Proceed,
        "three minutes after its last miss, one miss is back"
    );
}

#[test]
fn budget_refills_over_time() {
    let mut table = new_table(3, 180, 100);
    let client = key(1);
    for _ in 0..3 {
        table.record_miss(client, at(0));
    }
    assert!(matches!(
        table.admit(client, at(0)),
        MissVerdict::Retry { .. }
    ));

    // One miss is back after one period: the client may try once, and is slowed again by it.
    assert_eq!(table.admit(client, at(180)), MissVerdict::Proceed);
    assert!(matches!(
        table.record_miss(client, at(180)),
        MissVerdict::Retry { .. }
    ));
    assert!(matches!(
        table.admit(client, at(181)),
        MissVerdict::Retry { .. }
    ));
    // Two periods later two misses are back.
    assert_eq!(table.admit(client, at(180 + 360)), MissVerdict::Proceed);
    assert_eq!(
        table.record_miss(client, at(180 + 360)),
        MissVerdict::Proceed
    );
    assert!(matches!(
        table.record_miss(client, at(180 + 360)),
        MissVerdict::Retry { .. }
    ));

    // The budget never grows past its capacity, however long the client waits.
    let long_after = 1_000_000;
    assert_eq!(
        table.record_miss(client, at(long_after)),
        MissVerdict::Proceed
    );
    assert_eq!(
        table.record_miss(client, at(long_after)),
        MissVerdict::Proceed
    );
    assert!(matches!(
        table.record_miss(client, at(long_after)),
        MissVerdict::Retry { .. }
    ));
}

#[test]
fn table_is_bounded_and_evicts_the_oldest() {
    // One miss uses a key's budget up, so a key the table still knows answers `Retry`, and a key it
    // has forgotten answers `Proceed`.
    let mut table = new_table(1, 600, 4);
    for n in 1..=10u8 {
        table.record_miss(key(n), at(u64::from(n)));
        assert!(table.len() <= 4, "the table holds at most max_keys keys");
    }
    assert_eq!(table.len(), 4);
    assert_eq!(table.max_keys(), 4);
    for n in 1..=6u8 {
        assert_eq!(
            table.admit(key(n), at(11)),
            MissVerdict::Proceed,
            "the key {n} was the oldest and is forgotten"
        );
    }
    for n in 7..=10u8 {
        assert!(
            matches!(table.admit(key(n), at(11)), MissVerdict::Retry { .. }),
            "the key {n} is one of the newest and is remembered"
        );
    }

    // A key that misses again becomes the newest: the one that was oldest goes first.
    let mut table = new_table(1, 600, 3);
    for n in 1..=3u8 {
        table.record_miss(key(n), at(u64::from(n)));
    }
    table.record_miss(key(1), at(10));
    table.record_miss(key(4), at(11));
    assert_eq!(table.len(), 3);
    assert_eq!(
        table.admit(key(2), at(12)),
        MissVerdict::Proceed,
        "the key 2 had gone longest without a miss"
    );
    for n in [1u8, 3, 4] {
        assert!(matches!(
            table.admit(key(n), at(12)),
            MissVerdict::Retry { .. }
        ));
    }
}

#[test]
fn admit_changes_nothing_and_creates_no_entry() {
    let table = new_table(3, 180, 10);
    for n in 0..50u8 {
        assert_eq!(table.admit(key(n), at(1)), MissVerdict::Proceed);
    }
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn a_client_with_many_misses_slows_down_others_unaffected() {
    let budget = MissBudget::new(MISS_CAPACITY, MISS_REFILL).expect("the estimated budget");
    let mut table = MissTable::new(budget, MISS_TABLE_KEYS).expect("the estimated table");
    let guesser = key(1);
    let neighbour = key(2);

    // The guesser tries a hundred ids in a burst: after its budget it is slowed on every try.
    let mut slowed_at = None;
    for guess in 0..100u32 {
        match table.record_miss(guesser, at(0)) {
            MissVerdict::Proceed => assert!(slowed_at.is_none()),
            MissVerdict::Retry { .. } => {
                if slowed_at.is_none() {
                    slowed_at = Some(guess);
                }
            }
        }
    }
    assert_eq!(
        slowed_at,
        Some(MISS_CAPACITY - 1),
        "the last miss of the budget slows the client"
    );
    assert!(matches!(
        table.admit(guesser, at(0)),
        MissVerdict::Retry { .. }
    ));
    // The slow client is allowed one more try each refill period: 20 guesses an hour.
    assert_eq!(table.admit(guesser, at(180)), MissVerdict::Proceed);

    // Everyone else is untouched.
    assert_eq!(table.admit(neighbour, at(0)), MissVerdict::Proceed);
    assert_eq!(table.record_miss(neighbour, at(0)), MissVerdict::Proceed);
    assert_eq!(table.admit(key(3), at(0)), MissVerdict::Proceed);
}

#[test]
fn a_budget_and_a_table_refuse_nonsense() {
    assert_eq!(
        MissBudget::new(0, Span::minutes(3)),
        Err(MissError::NoCapacity)
    );
    assert_eq!(MissBudget::new(5, Span::ZERO), Err(MissError::NoRefill));
    assert_eq!(
        MissBudget::new(u32::MAX, Span::secs(u64::MAX)),
        Err(MissError::TooLarge)
    );
    let budget = MissBudget::new(5, Span::minutes(3)).expect("a budget");
    assert_eq!(budget.capacity(), 5);
    assert_eq!(budget.refill_every(), Span::minutes(3));
    assert_eq!(MissTable::new(budget, 0).unwrap_err(), MissError::NoKeys);
    let table = MissTable::new(budget, 1).expect("a table of one key");
    assert_eq!(table.budget(), budget);
}

#[test]
fn the_estimated_numbers_are_twenty_misses_an_hour_and_65536_keys() {
    // 20 misses an hour (one back every three minutes), 65,536 keys.
    assert_eq!(MISS_CAPACITY, 20);
    assert_eq!(MISS_REFILL, Span::minutes(3));
    assert_eq!(MISS_TABLE_KEYS, 65_536);
}

#[test]
fn a_key_and_a_table_print_no_key_bytes() {
    let client = BudgetKey::from_bytes([0xAB; 16]);
    assert_eq!(format!("{client:?}"), "BudgetKey(..)");
    assert_eq!(client.as_bytes(), &[0xAB; 16]);
    let mut table = new_table(3, 180, 10);
    table.record_miss(client, at(0));
    let printed = format!("{table:?}");
    assert!(!printed.contains("171, 171"), "{printed}");
}

/// The table's rules, written plainly with vectors and a linear search: the model the seeded
/// histories are compared with.
struct Model {
    full: u64,
    refill: u64,
    max_keys: usize,
    /// (key, credit, at, sequence number of the last miss)
    entries: Vec<(u8, u64, u64, u64)>,
    seq: u64,
}

impl Model {
    fn credit(&self, entry: &(u8, u64, u64, u64), now: u64) -> u64 {
        (entry.1 + now.saturating_sub(entry.2)).min(self.full)
    }

    fn verdict(&self, credit: u64) -> MissVerdict {
        if credit >= self.refill {
            MissVerdict::Proceed
        } else {
            MissVerdict::Retry {
                after: Span::secs(self.refill - credit),
            }
        }
    }

    fn admit(&self, key: u8, now: u64) -> MissVerdict {
        match self.entries.iter().find(|entry| entry.0 == key) {
            Some(entry) => self.verdict(self.credit(entry, now)),
            None => MissVerdict::Proceed,
        }
    }

    fn miss(&mut self, key: u8, now: u64) -> MissVerdict {
        let (credit, at) = match self.entries.iter().position(|entry| entry.0 == key) {
            Some(index) => {
                let entry = self.entries.remove(index);
                (self.credit(&entry, now), entry.2.max(now))
            }
            None => {
                if self.entries.len() >= self.max_keys {
                    let oldest = self
                        .entries
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, entry)| entry.3)
                        .map(|(index, _)| index)
                        .expect("a full table has entries");
                    self.entries.remove(oldest);
                }
                (self.full, now)
            }
        };
        let left = credit.saturating_sub(self.refill);
        self.seq += 1;
        self.entries.push((key, left, at, self.seq));
        self.verdict(left)
    }
}

#[test]
fn the_table_agrees_with_a_plain_model_over_seeded_histories() {
    for seed in 0..400u64 {
        let mut rng = Rng::new(seed);
        let capacity = 1 + rng.below(5).expect("a range") as u32;
        let refill = 1 + rng.below(300).expect("a range");
        let max_keys = 1 + rng.below(6).expect("a range") as usize;
        let mut table = new_table(capacity, refill, max_keys);
        let mut model = Model {
            full: u64::from(capacity) * refill,
            refill,
            max_keys,
            entries: Vec::new(),
            seq: 0,
        };
        let mut now = 0u64;
        for step in 0..200 {
            // Time goes forward, now and then a little back (a clock glitch).
            if rng.chance(1, 10) {
                now = now.saturating_sub(rng.below(50).expect("a range"));
            } else {
                now += rng.below(400).expect("a range");
            }
            let client = rng.below(9).expect("a range") as u8;
            if rng.chance(1, 3) {
                assert_eq!(
                    table.admit(key(client), at(now)),
                    model.admit(client, now),
                    "seed {seed} step {step}: admit"
                );
            } else {
                assert_eq!(
                    table.record_miss(key(client), at(now)),
                    model.miss(client, now),
                    "seed {seed} step {step}: record_miss"
                );
            }
            assert_eq!(table.len(), model.entries.len(), "seed {seed} step {step}");
            assert!(table.len() <= max_keys);
        }
        // Every key answers as the model does.
        for client in 0..9u8 {
            assert_eq!(
                table.admit(key(client), at(now)),
                model.admit(client, now),
                "seed {seed}: final state of the key {client}"
            );
        }
    }
}

#[test]
fn a_delay_is_never_shorter_than_a_second() {
    let mut table = new_table(2, 7, 10);
    for second in 0..200u64 {
        if let MissVerdict::Retry { after } = table.record_miss(key(1), at(second / 3)) {
            assert!(after >= Span::secs(1));
            assert!(after <= Span::secs(7));
        }
        if let MissVerdict::Retry { after } = table.admit(key(1), at(second / 2)) {
            assert!(after >= Span::secs(1));
            assert!(after <= Span::secs(7));
        }
    }
}

#[test]
fn a_budget_of_the_longest_time_does_not_wrap() {
    let budget = MissBudget::new(1, Span::secs(u64::MAX)).expect("a budget");
    let mut table = MissTable::new(budget, 2).expect("a table");
    let client = key(1);
    assert_eq!(
        table.record_miss(client, at(0)),
        MissVerdict::Retry {
            after: Span::secs(u64::MAX)
        }
    );
    assert_eq!(
        table.admit(client, at(1)),
        MissVerdict::Retry {
            after: Span::secs(u64::MAX - 1)
        }
    );
    let way_later = Stamp::from_secs(u64::MAX);
    assert!(matches!(
        table.admit(client, way_later),
        MissVerdict::Retry { .. }
    ));
}
