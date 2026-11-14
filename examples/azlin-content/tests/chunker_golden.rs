//! C1 and C7: the cut points of fixed streams are pinned, and a keyed table cuts elsewhere.
//!
//! C1: for three deterministic streams (SplitMix64 seeds 1, 2 and 3, 24 MiB each) the cut offsets under
//! `GearTable::public_v1()` and under `GearTable::keyed` of a fixed test `K_chunk` equal the lists in
//! `tests/vectors/chunker_golden.json`: any change to the table, the masks, the normal size, the
//! window or the skipping fails here, and a boundary that moves is a change that re-uploads every
//! file of every user (DRIFT X-DELTA-W1S14B-2). The file is written only under `UPDATE_VECTORS=1`,
//! never when `CI` is set. The lists are also held to a SECOND implementation of the algorithm that
//! lives in this test (`reference_cuts`: per chunk, explicit indices, a gear table recomputed with
//! blake3 calls only), so the file cannot be a snapshot of a bug that two readings of the text would
//! not share.
//!
//! C7: two different `K_chunk` give different cut lists on the same data, the same `K_chunk` the same
//! one, and the public table is independent of any key.

mod azl2_common;

use azl2_common::{
    check_vector_file, field, k_chunk, keyed_reference_table, public_reference_table, read_rows,
    reference_cuts, render_rows, stream, to_hex, MAX, MIB, MIN, NORMAL,
};
use azlin_content::{Chunker, GearTable};

const SEEDS: [u64; 3] = [1, 2, 3];
const STREAM_LEN: usize = 24 * MIB;
/// The test `K_chunk`: 32 bytes of 0x42.
const TEST_K: u8 = 0x42;

/// The cut offsets (the end of each chunk) the library finds.
fn library_cuts(chunker: &Chunker, data: &[u8]) -> Vec<usize> {
    chunker.cut(data).iter().map(|r| r.end).collect()
}

fn list(cuts: &[usize]) -> String {
    cuts.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn c1_chunker_golden_boundaries_pinned() {
    // The six chunkings run side by side: a debug build scans about ten megabytes a second.
    let public_reference = public_reference_table();
    let keyed_reference = keyed_reference_table(&[TEST_K; 32]);
    let results: Vec<(u64, &str, Vec<usize>)> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for seed in SEEDS {
            for (name, keyed) in [("public_v1", false), ("keyed", true)] {
                let reference_table = if keyed {
                    &keyed_reference
                } else {
                    &public_reference
                };
                handles.push(scope.spawn(move || {
                    let data = stream(seed, STREAM_LEN);
                    let table = if keyed {
                        GearTable::keyed(&k_chunk(TEST_K))
                    } else {
                        GearTable::public_v1()
                    };
                    let cuts = library_cuts(&Chunker::new(table), &data);
                    // The second implementation agrees with the library on every cut.
                    assert_eq!(
                        cuts,
                        reference_cuts(&data, reference_table),
                        "seed {seed}, table {name}: the library and the reference disagree"
                    );
                    (seed, name, cuts)
                }));
            }
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });

    // The vector file: a row of parameters, then one row per stream and table.
    let mut rows = vec![format!(
        "{{\"name\": \"parameters\", \"min\": {MIN}, \"normal\": {NORMAL}, \"max\": {MAX}, \"window\": 64, \"mask_strict\": \"fffffc0000000000\", \"mask_loose\": \"ffffc00000000000\", \"k_chunk\": \"{}\", \"stream\": \"splitmix64, 8 little-endian bytes per output\", \"len\": {STREAM_LEN}}}",
        to_hex(&[TEST_K; 32])
    )];
    for (seed, name, cuts) in &results {
        rows.push(format!(
            "{{\"name\": \"seed{seed}_{name}\", \"seed\": {seed}, \"table\": \"{name}\", \"count\": {}, \"cuts\": \"{}\"}}",
            cuts.len(),
            list(cuts)
        ));
    }
    check_vector_file("chunker_golden", &render_rows(&rows));

    // The file's lists are the reference's too, and every list is a valid chunking.
    for row in read_rows("chunker_golden").iter().skip(1) {
        let seed: u64 = field(row, "name")
            .trim_start_matches("seed")
            .split('_')
            .next()
            .expect("a seed")
            .parse()
            .expect("a number");
        let name = field(row, "table");
        let cuts: Vec<usize> = field(row, "cuts")
            .split(',')
            .map(|n| n.parse().expect("an offset"))
            .collect();
        let found = results
            .iter()
            .find(|(s, n, _)| *s == seed && *n == name)
            .expect("a computed stream for the row");
        assert_eq!(
            cuts, found.2,
            "seed {seed}, table {name}: the file is stale"
        );
        assert_eq!(cuts.last(), Some(&STREAM_LEN));
        let mut previous = 0;
        for (i, &end) in cuts.iter().enumerate() {
            let len = end - previous;
            assert!(
                len <= MAX,
                "seed {seed}, table {name}, chunk {i}: {len} bytes"
            );
            if i + 1 < cuts.len() {
                assert!(
                    len >= MIN,
                    "seed {seed}, table {name}, chunk {i}: {len} bytes"
                );
            }
            previous = end;
        }
    }
}

#[test]
fn c7_keyed_table_changes_boundaries() {
    let data = stream(7, 12 * MIB);
    let a = k_chunk(1);
    let b = k_chunk(2);
    let keyed_a = Chunker::new(GearTable::keyed(&a));
    let keyed_a_again = Chunker::new(GearTable::keyed(&a));
    let keyed_b = Chunker::new(GearTable::keyed(&b));
    let public = Chunker::new(GearTable::public_v1());
    let public_again = Chunker::new(GearTable::public_v1());

    let cuts_a = keyed_a.cut(&data);
    assert_eq!(
        cuts_a,
        keyed_a_again.cut(&data),
        "the same K_chunk, the same cuts"
    );
    assert_ne!(cuts_a, keyed_b.cut(&data), "another K_chunk, other cuts");
    assert_ne!(
        cuts_a,
        public.cut(&data),
        "a keyed table is not the public one"
    );
    assert_eq!(
        public.cut(&data),
        public_again.cut(&data),
        "the public table is the same every time"
    );
    // The public table does not depend on any key that happens to exist.
    let _unrelated = k_chunk(9);
    assert_eq!(public.cut(&data), public_again.cut(&data));

    // The tables themselves: equal for the same key, different for another, never the public one.
    assert!(GearTable::keyed(&a) == GearTable::keyed(&a));
    assert!(GearTable::keyed(&a) != GearTable::keyed(&b));
    assert!(GearTable::keyed(&a) != GearTable::public_v1());
    assert!(GearTable::public_v1() == GearTable::public_v1());

    // The tables are what the documentation says: the derive_key-mode XOF, recomputed in this test.
    let reference_public = public_reference_table();
    let reference_keyed_a = keyed_reference_table(&[1; 32]);
    assert_eq!(
        public.cut(&data).iter().map(|r| r.end).collect::<Vec<_>>(),
        reference_cuts(&data, &reference_public)
    );
    assert_eq!(
        cuts_a.iter().map(|r| r.end).collect::<Vec<_>>(),
        reference_cuts(&data, &reference_keyed_a)
    );
}
