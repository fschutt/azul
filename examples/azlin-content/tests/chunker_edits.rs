//! C5: a small edit changes few chunks (the property that makes delta sync worth building,
//! planning/architecture.md section 5.7 "Why": a one-byte edit to a 1 GB file must not upload 1 GB
//! again).
//!
//! A seeded stream of 64 MiB (24 MiB in a debug build) is chunked once. Then, at 20 seeded positions
//! (10 in a debug build) a byte is inserted, and at 20 more a byte is deleted, and the edited stream is
//! chunked again: at most 3 of its chunks may be ones the original does not have (the chunk that holds
//! the edit and, while the boundaries resynchronise, a neighbour or two). Edits at the very start and
//! the very end are tried too. Appending changes only the tail: every chunk but the original's last
//! is still there, in place.

mod azl2_common;

use std::collections::HashSet;

use azl2_common::{k_chunk, scaled, stream, SplitMix64, MIB};
use azlin_content::{Chunker, GearTable};

/// What an edit does.
#[derive(Clone, Copy, Debug)]
enum Edit {
    Insert { at: usize, byte: u8 },
    Delete { at: usize },
}

fn apply(base: &[u8], edit: Edit) -> Vec<u8> {
    match edit {
        Edit::Insert { at, byte } => {
            let mut v = Vec::with_capacity(base.len() + 1);
            v.extend_from_slice(&base[..at]);
            v.push(byte);
            v.extend_from_slice(&base[at..]);
            v
        }
        Edit::Delete { at } => {
            let mut v = Vec::with_capacity(base.len());
            v.extend_from_slice(&base[..at]);
            v.extend_from_slice(&base[at + 1..]);
            v
        }
    }
}

/// The BLAKE3 hash of every chunk, in order.
fn chunk_hashes(chunker: &Chunker, data: &[u8]) -> Vec<[u8; 32]> {
    chunker
        .cut(data)
        .iter()
        .map(|r| *blake3::hash(&data[r.clone()]).as_bytes())
        .collect()
}

#[test]
fn c5_edits_change_few_chunks() {
    let size = scaled("c5 stream bytes", 64 * MIB, 24 * MIB);
    let per_kind = scaled("c5 edits of each kind", 20, 10);
    let base = stream(5, size);
    // The keyed table: the property holds for any table, and this is the one the product will use.
    let chunker = Chunker::new(GearTable::keyed(&k_chunk(5)));
    let original = chunk_hashes(&chunker, &base);
    let known: HashSet<[u8; 32]> = original.iter().copied().collect();
    assert!(
        original.len() >= 5,
        "the base stream has {} chunks",
        original.len()
    );

    // The edits: seeded positions, and the edges of the stream.
    let mut rng = SplitMix64::new(55);
    let mut edits = Vec::new();
    for _ in 0..per_kind {
        let at = rng.below(size as u64) as usize;
        edits.push(Edit::Insert {
            at,
            byte: rng.next_u64().to_le_bytes()[0],
        });
        edits.push(Edit::Delete {
            at: rng.below(size as u64) as usize,
        });
    }
    edits.push(Edit::Insert { at: 0, byte: 0xEE });
    edits.push(Edit::Insert {
        at: size,
        byte: 0xEE,
    });
    edits.push(Edit::Delete { at: 0 });
    edits.push(Edit::Delete { at: size - 1 });

    // Chunk the edited streams side by side, a few at a time (each holds a copy of the stream).
    let workers = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .min(4);
    let results: Vec<(Edit, usize)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                let (edits, base, chunker, known) = (&edits, &base, &chunker, &known);
                scope.spawn(move || {
                    edits
                        .iter()
                        .copied()
                        .enumerate()
                        .filter(|(i, _)| i % workers == w)
                        .map(|(_, edit)| {
                            let edited = apply(base, edit);
                            let changed = chunk_hashes(chunker, &edited)
                                .iter()
                                .filter(|h| !known.contains(*h))
                                .count();
                            (edit, changed)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });

    let worst = results
        .iter()
        .map(|(_, changed)| *changed)
        .max()
        .unwrap_or(0);
    println!(
        "c5: {} chunks in {size} bytes; {} edits; the most chunks any edit changed: {worst}",
        original.len(),
        results.len()
    );
    for (edit, changed) in &results {
        assert!(
            *changed <= 3,
            "{edit:?} changed {changed} of the chunks (at most 3 allowed)"
        );
    }
    assert_eq!(results.len(), edits.len());

    // Appending changes only the tail: every chunk but the original's last one is still there, in
    // place, and what is new is the original's last chunk and the appended bytes, re-cut: at most
    // as many chunks as that region can hold (every chunk but the last is at least the minimum).
    let last_len = chunker.cut(&base).last().map_or(0, |r| r.len());
    for tail_len in [1usize, 1_000, 300_000] {
        let mut appended = base.clone();
        appended.extend_from_slice(&stream(555, tail_len));
        let after = chunk_hashes(&chunker, &appended);
        let keep = original.len() - 1;
        assert!(
            after.len() >= original.len() && after[..keep] == original[..keep],
            "appending {tail_len} bytes moved a chunk before the original's last"
        );
        let changed = after.iter().filter(|h| !known.contains(*h)).count();
        let region = last_len + tail_len;
        assert!(
            changed <= region / azl2_common::MIN + 1,
            "appending {tail_len} bytes changed {changed} chunks, more than the {region} bytes of the tail can hold"
        );
        assert_eq!(
            after.len() - keep,
            changed,
            "appending {tail_len} bytes: exactly the chunks after the kept ones are new"
        );
    }
}
