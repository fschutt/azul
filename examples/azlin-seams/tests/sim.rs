//! The seeded generator `sim::Rng` (known answers, reproducibility, no modulo bias, the edges of
//! `pick`, `shuffle`, `range`, `chance`) and the shrinker `sim::minimize`.
//!
//! Implements planning/architecture.md section 1.4 rules 1 and 2 (a failing seed replays exactly).

use std::cell::Cell;
use std::collections::BTreeSet;

use azlin_seams::sim::{minimize, Rng, MINIMIZE_MAX_CALLS};
use azlin_seams::EntropyError;

mod common;
use common::{CountingEntropy, DeadEntropy};

/// The first five outputs of SplitMix64 for three seeds, computed with an independent
/// implementation (scratch/W1-S17a/splitmix.py; seed 0 is the stream the public-domain C code of
/// Vigna prints).
const KNOWN: [(u64, [u64; 5]); 3] = [
    (
        0,
        [
            0xe220_a839_7b1d_cdaf,
            0x6e78_9e6a_a1b9_65f4,
            0x06c4_5d18_8009_454f,
            0xf88b_b8a8_724c_81ec,
            0x1b39_896a_51a8_749b,
        ],
    ),
    (
        1,
        [
            0x910a_2dec_8902_5cc1,
            0xbeeb_8da1_658e_ec67,
            0xf893_a2ee_fb32_555e,
            0x71c1_8690_ee42_c90b,
            0x71bb_54d8_d101_b5b9,
        ],
    ),
    (
        0xDEAD_BEEF_CAFE_F00D,
        [
            0x901d_4f65_2fb4_72cb,
            0xa7ce_2464_40f7_4527,
            0x19b4_0bbb_b938_0d34,
            0xe7a8_6dc5_be61_8392,
            0x7366_ce94_5d00_e82c,
        ],
    ),
];

/// The chi-square statistic of `draws` draws of `below(n)` into `n` buckets.
fn chi_square(rng: &mut Rng, n: u64, draws: usize) -> f64 {
    let mut counts = vec![0u64; n as usize];
    for _ in 0..draws {
        let value = rng.below(n).expect("n > 0");
        assert!(value < n, "below({n}) gave {value}");
        counts[value as usize] += 1;
    }
    let expected = draws as f64 / n as f64;
    counts
        .iter()
        .map(|&c| {
            let d = c as f64 - expected;
            d * d / expected
        })
        .sum()
}

#[test]
fn sim_rng_is_reproducible_and_unbiased_below_n() {
    // Known answers: the SplitMix64 stream.
    for (seed, outputs) in KNOWN {
        let mut rng = Rng::new(seed);
        for (index, want) in outputs.iter().enumerate() {
            assert_eq!(rng.next_u64(), *want, "seed {seed:#x}, output {index}");
        }
    }

    // The same seed, the same stream; a clone continues the same stream; another seed differs.
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    let first: Vec<u64> = (0..1000).map(|_| a.next_u64()).collect();
    let second: Vec<u64> = (0..1000).map(|_| b.next_u64()).collect();
    assert_eq!(first, second);
    let mut c = a.clone();
    assert_eq!(a.next_u64(), c.next_u64());
    let mut d = Rng::new(43);
    let other: Vec<u64> = (0..1000).map(|_| d.next_u64()).collect();
    assert_ne!(first, other);
    assert_eq!(
        first.iter().collect::<BTreeSet<_>>().len(),
        1000,
        "no repeated output in the first 1000"
    );
    // `below` is reproducible too: the whole derived stream is a function of the seed.
    let mut x = Rng::new(7);
    let mut y = Rng::new(7);
    for n in [1u64, 2, 3, 10, 1000, u64::MAX] {
        for _ in 0..50 {
            assert_eq!(x.below(n), y.below(n));
        }
    }

    // From the entropy seam: 8 bytes, little-endian; a dead source gives no generator.
    let mut ent = CountingEntropy::new();
    let mut seeded = Rng::from_entropy(&mut ent).expect("the counting source works");
    let mut by_hand = Rng::new(0x0807_0605_0403_0201);
    for _ in 0..10 {
        assert_eq!(seeded.next_u64(), by_hand.next_u64());
    }
    assert_eq!(
        Rng::from_entropy(&mut DeadEntropy).err(),
        Some(EntropyError::Exhausted)
    );

    // `below`: the empty range, the one-point range, the full width.
    let mut rng = Rng::new(1);
    assert_eq!(rng.below(0), None);
    for _ in 0..100 {
        assert_eq!(rng.below(1), Some(0));
        assert!(rng.below(u64::MAX).expect("non-empty") < u64::MAX);
    }
    let two: BTreeSet<u64> = (0..200).filter_map(|_| rng.below(2)).collect();
    assert_eq!(two, BTreeSet::from([0, 1]));

    // Unbiased: chi-square over 100,000 draws. The 99.9% critical values are 27.88 (9 degrees of
    // freedom, n = 10) and 22.46 (6 degrees, n = 7); seeds fixed in advance.
    for seed in [0xC41_5001u64, 0xC41_5002, 0xC41_5003] {
        let mut rng = Rng::new(seed);
        let ten = chi_square(&mut rng, 10, 100_000);
        assert!(ten < 27.88, "n = 10, seed {seed:#x}: chi-square {ten}");
        let seven = chi_square(&mut rng, 7, 100_000);
        assert!(seven < 22.46, "n = 7, seed {seed:#x}: chi-square {seven}");
        // And not a counter: successive values of `below(10)` are not a round robin.
        let run: Vec<u64> = (0..200).filter_map(|_| rng.below(10)).collect();
        assert!(
            run.windows(2).filter(|w| w[0] == w[1]).count() > 5,
            "repeats occur in a random stream"
        );
    }

    // No modulo bias where it would show: n = 3 * 2^62. A modulo reduction makes the remainders
    // below 2^62 twice as likely (a half instead of a third); rejection keeps them a third.
    let n = 3u64 << 62;
    let mut rng = Rng::new(0xB1A5);
    let draws = 30_000usize;
    let low = (0..draws)
        .filter(|_| rng.below(n).expect("non-empty") < (1u64 << 62))
        .count();
    let fraction = low as f64 / draws as f64;
    assert!(
        (0.31..0.355).contains(&fraction),
        "P(below(3 * 2^62) < 2^62) = {fraction}, expected a third (a modulo reduction gives a half)"
    );
}

#[test]
fn sim_rng_pick_shuffle_range_edges() {
    let mut rng = Rng::new(0xED6E5);

    // pick: nothing from nothing, the only element, every element.
    let empty: [u32; 0] = [];
    assert_eq!(rng.pick(&empty), None);
    for _ in 0..50 {
        assert_eq!(rng.pick(&[7u32]), Some(&7));
    }
    let five = [10u32, 20, 30, 40, 50];
    let picked: BTreeSet<u32> = (0..1000).filter_map(|_| rng.pick(&five).copied()).collect();
    assert_eq!(picked, five.iter().copied().collect::<BTreeSet<_>>());

    // shuffle: 0 and 1 elements are left alone and consume no draw.
    let mut twin = rng.clone();
    let mut none: [u32; 0] = [];
    rng.shuffle(&mut none);
    let mut one = [9u32];
    rng.shuffle(&mut one);
    assert_eq!(one, [9]);
    assert_eq!(rng.next_u64(), twin.next_u64(), "no draw was consumed");
    // Two elements: both orders occur.
    let orders: BTreeSet<[u32; 2]> = (0..200)
        .map(|_| {
            let mut pair = [1u32, 2];
            rng.shuffle(&mut pair);
            pair
        })
        .collect();
    assert_eq!(orders, BTreeSet::from([[1, 2], [2, 1]]));
    // Four elements: all 24 permutations occur, and every shuffle is a permutation.
    let permutations: BTreeSet<[u32; 4]> = (0..3000)
        .map(|_| {
            let mut four = [1u32, 2, 3, 4];
            rng.shuffle(&mut four);
            let mut sorted = four;
            sorted.sort_unstable();
            assert_eq!(sorted, [1, 2, 3, 4], "a shuffle keeps the elements");
            four
        })
        .collect();
    assert_eq!(permutations.len(), 24);
    // Reproducible from the seed.
    let mut left: Vec<u32> = (0..50).collect();
    let mut right = left.clone();
    Rng::new(5).shuffle(&mut left);
    Rng::new(5).shuffle(&mut right);
    assert_eq!(left, right);
    assert_ne!(left, (0..50).collect::<Vec<u32>>(), "50 elements do move");

    // range: half open; empty and reversed ranges are `None`.
    assert_eq!(rng.range(5, 5), None);
    assert_eq!(rng.range(6, 5), None);
    assert_eq!(rng.range(u64::MAX, 0), None);
    assert_eq!(rng.range(u64::MAX, u64::MAX), None);
    for _ in 0..100 {
        assert_eq!(rng.range(0, 1), Some(0));
        assert_eq!(rng.range(u64::MAX - 1, u64::MAX), Some(u64::MAX - 1));
        let wide = rng.range(0, u64::MAX).expect("non-empty");
        assert!(wide < u64::MAX);
        let offset = rng.range(u64::MAX - 10, u64::MAX).expect("non-empty");
        assert!((u64::MAX - 10..u64::MAX).contains(&offset));
    }
    let ten: BTreeSet<u64> = (0..1000).filter_map(|_| rng.range(10, 20)).collect();
    assert_eq!(ten, (10..20).collect::<BTreeSet<u64>>());

    // chance: the certain and the impossible events, and a half.
    for _ in 0..200 {
        assert!(!rng.chance(0, 5));
        assert!(rng.chance(5, 5));
        assert!(rng.chance(6, 5));
        assert!(rng.chance(u32::MAX, 1));
        assert!(!rng.chance(1, 0), "an event of probability 0 never happens");
        assert!(!rng.chance(0, 0));
    }
    let heads = (0..1000).filter(|_| rng.chance(1, 2)).count();
    assert!(
        (420..580).contains(&heads),
        "a fair coin gave {heads} of 1000"
    );
    let tenth = (0..10_000).filter(|_| rng.chance(1, 10)).count();
    assert!(
        (800..1200).contains(&tenth),
        "one in ten gave {tenth} of 10000"
    );

    // bytes: any length, the same stream, a prefix of the longer.
    for len in [0usize, 1, 7, 8, 9, 15, 16, 17, 100] {
        let bytes = Rng::new(3).bytes(len);
        assert_eq!(bytes.len(), len);
        let mut filled = vec![0xEEu8; len];
        Rng::new(3).fill_bytes(&mut filled);
        assert_eq!(bytes, filled);
        let longer = Rng::new(3).bytes(len + 9);
        assert_eq!(
            &longer[..len],
            bytes.as_slice(),
            "a prefix of the longer stream"
        );
    }
    assert_eq!(
        Rng::new(0).bytes(8),
        0xe220_a839_7b1d_cdafu64.to_le_bytes().to_vec(),
        "the bytes are the little-endian words of the stream"
    );
}

/// Whether the failure is present: the sequence holds all of `needle` (in any positions).
fn holds_all(items: &[u32], needle: &[u32]) -> bool {
    needle.iter().all(|n| items.contains(n))
}

fn is_subsequence(small: &[u32], big: &[u32]) -> bool {
    let mut rest = big.iter();
    small.iter().all(|s| rest.any(|b| b == s))
}

#[test]
fn minimize_returns_a_minimal_failing_subsequence_and_the_input_when_nothing_can_go() {
    // A planted 3-of-40 failure shrinks to those 3 (in their order) and still fails.
    let input: Vec<u32> = (0..40).collect();
    let calls = Cell::new(0usize);
    let needle = [7u32, 19, 33];
    let shrunk = minimize(input.clone(), |candidate| {
        calls.set(calls.get() + 1);
        holds_all(candidate, &needle)
    });
    assert_eq!(shrunk, vec![7, 19, 33]);
    assert!(holds_all(&shrunk, &needle), "the result still fails");
    assert!(
        calls.get() < 400,
        "ddmin needed {} calls for 40 elements",
        calls.get()
    );

    // The same planted failure at many positions and sizes, through a seeded generator: the result
    // is exactly the needle, in order, and 1-minimal.
    let mut rng = Rng::new(0x51114C);
    for _ in 0..200 {
        let len = rng.range(3, 80).expect("non-empty") as usize;
        let items: Vec<u32> = (0..len as u32).collect();
        let k = rng.range(1, 5).expect("non-empty") as usize;
        let mut positions: Vec<u32> = items.clone();
        rng.shuffle(&mut positions);
        positions.truncate(k.min(len));
        positions.sort_unstable();
        let shrunk = minimize(items.clone(), |candidate| holds_all(candidate, &positions));
        assert_eq!(shrunk, positions, "len {len}, needle {positions:?}");
    }

    // A single culprit shrinks to one element; a culprit set that is a pair too.
    assert_eq!(
        minimize((0..100u32).collect(), |c| c.contains(&63)),
        vec![63]
    );
    assert_eq!(
        minimize((0..100u32).collect(), |c| holds_all(c, &[0, 99])),
        vec![0, 99]
    );

    // Nothing can go: the failure needs every element, so the input comes back whole.
    let whole: Vec<u32> = (0..6).collect();
    assert_eq!(minimize(whole.clone(), |c| c.len() == 6), whole);
    assert_eq!(minimize(whole.clone(), |c| holds_all(c, &whole)), whole);
    // A failure that needs the elements to be adjacent and ordered keeps exactly them.
    let strict = minimize((0..30u32).collect(), |c| {
        c.windows(3).any(|w| w == [10, 11, 12])
    });
    assert_eq!(strict, vec![10, 11, 12]);

    // The input that does not fail at all comes back as it is, with one call of the predicate.
    let calls = Cell::new(0usize);
    let passing = minimize(vec![1u32, 2, 3], |_| {
        calls.set(calls.get() + 1);
        false
    });
    assert_eq!(passing, vec![1, 2, 3]);
    assert_eq!(calls.get(), 1);

    // The edges: the empty input, a one-element input, a failure that needs no element.
    assert_eq!(minimize(Vec::<u32>::new(), |_| true), Vec::<u32>::new());
    assert_eq!(minimize(Vec::<u32>::new(), |_| false), Vec::<u32>::new());
    assert_eq!(minimize(vec![5u32], |c| c.contains(&5)), vec![5]);
    assert_eq!(minimize(vec![5u32], |_| true), Vec::<u32>::new());
    assert_eq!(minimize(vec![1u32, 2, 3, 4], |_| true), Vec::<u32>::new());

    // The result is a subsequence of the input (the order is kept) and 1-minimal, for a failure
    // that depends on counts and order, not on a fixed set.
    let mut rng = Rng::new(0xDDDD);
    for _ in 0..100 {
        let len = rng.range(1, 60).expect("non-empty") as usize;
        let items: Vec<u32> = (0..len)
            .map(|_| rng.below(10).unwrap_or(0) as u32)
            .collect();
        // Fails when two 3s are followed (later) by a 7.
        let fails = |c: &[u32]| {
            let mut threes = 0;
            for &x in c {
                if x == 3 {
                    threes += 1;
                }
                if x == 7 && threes >= 2 {
                    return true;
                }
            }
            false
        };
        if !fails(&items) {
            assert_eq!(minimize(items.clone(), fails), items, "a passing input");
            continue;
        }
        let shrunk = minimize(items.clone(), fails);
        assert!(
            fails(&shrunk),
            "the result still fails: {shrunk:?} of {items:?}"
        );
        assert!(is_subsequence(&shrunk, &items));
        assert_eq!(shrunk, vec![3, 3, 7], "the minimal shape of this failure");
        for skip in 0..shrunk.len() {
            let mut smaller = shrunk.clone();
            smaller.remove(skip);
            assert!(!fails(&smaller), "1-minimal: element {skip} can go");
        }
    }

    // The bound is a constant of the crate, and a predicate that fails for everything is cheap.
    assert_eq!(MINIMIZE_MAX_CALLS, 1_000_000);
    let big: Vec<u32> = (0..2000).collect();
    let calls = Cell::new(0usize);
    let nothing = minimize(big, |_| {
        calls.set(calls.get() + 1);
        true
    });
    assert!(nothing.is_empty());
    assert!(calls.get() < 5000, "{} calls", calls.get());
}
