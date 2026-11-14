//! C4: the average chunk is about 1 MiB and almost none is cut by the maximum alone; and the model
//! that sets the normal size (DRIFT X-DELTA-W1S14B-6).
//!
//! The measurement runs a seeded stream of 256 MiB through the stream chunker and prints what it
//! finds. The model is the geometric one: the cut test passes with
//! probability 2^-22 per byte below the normal size and 2^-18 per byte from it on, a chunk is at
//! least `MIN` and at most `MAX` bytes, and the expected chunk length is computed in closed form.

mod azl2_common;

use azl2_common::{scaled, SplitMix64, MAX, MIB, MIN, NORMAL};
use azlin_content::{Chunker, GearTable};

/// The expected chunk length of the normalized rule, in closed form: the cut is tested at every
/// length from `min`; it passes with probability `ps` below `normal` and `pl` from there; the chunk
/// is cut at `max` at the latest. A stop after `k` tests means a chunk of `min - 1 + k` bytes.
fn expected_len(min: f64, normal: f64, max: f64, ps: f64, pl: f64) -> f64 {
    let strict_tests = normal - min;
    let loose_tests = max - normal;
    let (qs, ql) = (1.0 - ps, 1.0 - pl);
    let survive_strict = qs.powf(strict_tests);
    let stop_strict = 1.0 - survive_strict;
    // The sum of k * q^(k-1) * p over k = 1 ..= n is (1 - q^n) / p - n * q^n.
    let steps_strict = (1.0 - qs.powf(strict_tests)) / ps - strict_tests * qs.powf(strict_tests);
    let stop_loose = 1.0 - ql.powf(loose_tests);
    let steps_loose = (1.0 - ql.powf(loose_tests)) / pl - loose_tests * ql.powf(loose_tests);
    let forced = survive_strict * ql.powf(loose_tests);
    stop_strict * (min - 1.0)
        + steps_strict
        + survive_strict * (stop_loose * (normal - 1.0) + steps_loose)
        + forced * max
}

#[test]
fn c4_the_normal_size_makes_the_expected_chunk_the_average() {
    let (ps, pl) = (2f64.powi(-22), 2f64.powi(-18));
    let (min, max, mib) = (MIN as f64, MAX as f64, MIB as f64);
    let chosen = expected_len(min, NORMAL as f64, max, ps, pl) / mib;
    println!("expected chunk length at the normal size of 848 KiB: {chosen:.4} MiB");
    assert!(
        (chosen - 1.0).abs() < 0.01,
        "the expected chunk length is {chosen} MiB, not the average of 1 MiB"
    );
    // The other readings of the drift row: a smaller normal size gives a shorter average, a
    // normal size equal to the average one that is too long.
    let kib = 1024.0;
    let at_800 = expected_len(min, 800.0 * kib, max, ps, pl) / mib;
    let at_average = expected_len(min, mib, max, ps, pl) / mib;
    println!("800 KiB: {at_800:.4} MiB; 1 MiB: {at_average:.4} MiB");
    assert!(at_800 < 0.97 && at_800 > 0.96);
    assert!(at_average > 1.13 && at_average < 1.15);
    // The share of chunks that only the maximum cuts is negligible in the model.
    let forced = (1.0 - ps).powf(NORMAL as f64 - min) * (1.0 - pl).powf(max - NORMAL as f64);
    assert!(forced < 1e-5, "forced cuts: {forced}");
}

#[test]
fn c4_average_chunk_size_is_about_one_mib() {
    let total = scaled("c4 bytes", 256 * MIB, 256 * MIB);
    let chunker = Chunker::new(GearTable::public_v1());
    let mut stream = chunker.stream();
    let mut rng = SplitMix64::new(4);
    let mut piece = vec![0u8; 2 * MIB];
    let mut out = Vec::new();
    let mut lengths: Vec<usize> = Vec::new();
    let mut fed = 0usize;
    while fed < total {
        let n = piece.len().min(total - fed);
        rng.fill(&mut piece[..n]);
        stream.feed(&piece[..n], &mut out);
        fed += n;
        lengths.extend(out.drain(..).map(|chunk| chunk.len()));
    }
    // The last chunk of the stream is whatever was left, not a cut: it is not part of the statistics.
    drop(stream);

    let count = lengths.len();
    let mean = lengths.iter().sum::<usize>() as f64 / count as f64;
    let at_max = lengths.iter().filter(|&&len| len == MAX).count();
    let mut sorted = lengths.clone();
    sorted.sort_unstable();
    let quantile = |q: f64| sorted[((count - 1) as f64 * q) as usize];
    println!(
        "c4: {count} chunks over {total} bytes; mean {mean:.0} bytes = {:.4} MiB ({:+.1} % of 1 MiB); shortest {}, p10 {}, median {}, p90 {}, longest {}; {at_max} cut by the maximum ({:.3} %)",
        mean / MIB as f64,
        (mean / MIB as f64 - 1.0) * 100.0,
        sorted[0],
        quantile(0.10),
        quantile(0.50),
        quantile(0.90),
        sorted[count - 1],
        at_max as f64 * 100.0 / count as f64
    );
    assert!(count >= 20, "too few chunks to measure: {count}");
    assert!(
        (mean / MIB as f64 - 1.0).abs() <= 0.15,
        "the mean chunk is {mean} bytes, more than 15 % from 1 MiB"
    );
    assert!(
        (at_max as f64) < count as f64 * 0.01,
        "{at_max} of {count} chunks hit the maximum"
    );
    assert!(sorted[0] >= MIN && sorted[count - 1] <= MAX);
}
