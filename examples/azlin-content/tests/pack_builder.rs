//! C3, C4, C7 and C9: the pack builder (planning/architecture.md section 5.7 "packing into 4 MiB packs",
//! section 5.15 M4 "the last one padded to 256 KiB steps; repository objects to 4 KiB; policy `padding` =
//! none, buckets (the default) or full").
//!
//! C3: when a pack is full, by an independent size formula (`ref_unpadded_len`, written from the layout of
//! the plan, not from the crate's code). C4: the lines of the trailer are the real positions of the
//! blobs. C7: the padding policy, and the padding bytes. C9: the same inputs and the same entropy make the
//! same pack, and another nonce changes the trailer and the padding only.

mod pack_common;

use azlin_content::pack::{
    chunk_range, randomness_check, read_trailer, rebuild_entries, AddOutcome, PackBuilder,
    PackEntry, PackError, PackGeometry, PackHeader, Padding, SealedPack, HARD_MAX, MAX_CHUNKS,
    PAD_STEP, REPO_PAD, TARGET,
};
use azlin_content::{ChunkId, ChunkLen, KChunk, PackClass, PackLen, PackOffset, SealedChunk};
use azlin_keys::{EntropyError, FailingEntropy, SeededEntropy, SpaceId};
use pack_common::{
    build, build_with, chunk, k_chunk, ref_pack, ref_padded_len, ref_unpadded_from_lens,
    ref_unpadded_len, scaled, seal_key, small_geometry, space, FixedNonce, RefChunk, RefParams,
    SplitMix64,
};

/// The epoch and the key bytes of `K_seal` in this file.
const EPOCH: u32 = 1;
const KEY_BYTE: u8 = 7;

fn builder(sp: &SpaceId, class: PackClass, g: PackGeometry, padding: Padding) -> PackBuilder {
    PackBuilder::new(*sp, class, seal_key(EPOCH, KEY_BYTE), g, padding)
}

fn rc(id: &ChunkId, blob: &SealedChunk) -> RefChunk {
    RefChunk {
        id: *id.as_bytes(),
        blob: blob.as_bytes().to_vec(),
    }
}

/// The lengths of the blobs of chunks `indices`.
fn lens_of(chunks: &[(ChunkId, SealedChunk)], indices: &[usize]) -> Vec<usize> {
    indices
        .iter()
        .map(|&i| chunks[i].1.as_bytes().len())
        .collect()
}

/// `n` chunks of random sizes: a third of the sequences are all tiny chunks (so that `max_chunks`, not
/// the size, fills a pack); the others are mostly small, some large, a tenth that does not fit a pack
/// of `small_geometry()` with company (some of those fit none). Every plaintext is 8 bytes or more, so
/// that every chunk has an id of its own.
fn random_chunks(
    rng: &mut SplitMix64,
    sp: &SpaceId,
    k: &KChunk,
    n: usize,
    seed_base: u64,
) -> Vec<(ChunkId, SealedChunk)> {
    let tiny_sequence = rng.below(3) == 0;
    let mut chunks = Vec::new();
    for i in 0..n {
        let plain_len = if tiny_sequence {
            8 + rng.below(53) as usize
        } else {
            match rng.below(10) {
                0..=5 => 8 + rng.below(3_000) as usize,
                6 | 7 => 3_000 + rng.below(9_000) as usize,
                8 => 12_000 + rng.below(18_000) as usize,
                _ => 32_000 + rng.below(13_000) as usize,
            }
        };
        chunks.push(chunk(sp, k, seed_base + i as u64, plain_len));
    }
    chunks
}

fn entropy(seed: u64) -> SeededEntropy {
    SeededEntropy::from_u64(seed)
}

#[derive(Default)]
struct Stats {
    fits: usize,
    full_by_size: usize,
    full_by_count: usize,
    lone_oversize: usize,
    too_big: usize,
    packs: usize,
}

#[test]
fn c3_builder_respects_target_and_counts() {
    let sp = space(1);
    let k = k_chunk(2);
    let g = small_geometry();
    let (target, hard_max, max_chunks) = (
        g.target.as_usize(),
        g.hard_max.as_usize(),
        g.max_chunks as usize,
    );
    let sequences = scaled("c3 sequences", 400, 40) as u64;
    let mut stats = Stats::default();

    for seed in 0..sequences {
        let mut rng = SplitMix64::new(seed);
        let n = 1 + rng.below(120) as usize;
        let chunks = random_chunks(&mut rng, &sp, &k, n, seed * 1_000);
        let mut current = builder(&sp, PackClass::Content, g, Padding::None);
        let mut held: Vec<usize> = Vec::new();
        let mut packs: Vec<(SealedPack, Vec<usize>)> = Vec::new();
        let mut too_big: Vec<usize> = Vec::new();

        for (index, (id, blob)) in chunks.iter().enumerate() {
            // The oracle: the pack this blob would make with the blobs held, and alone.
            let mut with_this = lens_of(&chunks, &held);
            with_this.push(blob.as_bytes().len());
            let would_be = ref_unpadded_from_lens(&with_this);
            let alone = ref_unpadded_from_lens(&with_this[with_this.len() - 1..]);
            match current.add(*id, blob).expect("adds") {
                AddOutcome::TooBigForAnyPack => {
                    assert!(
                        alone > hard_max,
                        "seed {seed}: told too big at {alone} bytes"
                    );
                    too_big.push(index);
                    stats.too_big += 1;
                }
                AddOutcome::Fits => {
                    assert!(
                        alone <= hard_max,
                        "seed {seed}: fits at {alone} bytes alone"
                    );
                    if held.is_empty() {
                        if would_be > target {
                            stats.lone_oversize += 1;
                        }
                    } else {
                        assert!(
                            would_be <= target && held.len() < max_chunks,
                            "seed {seed}: Fits for {would_be} bytes and {} chunks",
                            held.len() + 1
                        );
                    }
                    held.push(index);
                    stats.fits += 1;
                }
                AddOutcome::Full => {
                    assert!(
                        !held.is_empty(),
                        "seed {seed}: an empty builder is never full"
                    );
                    assert!(
                        alone <= hard_max,
                        "seed {seed}: a blob that fits no pack is not Full"
                    );
                    let by_size = would_be > target;
                    let by_count = held.len() + 1 > max_chunks;
                    assert!(
                        by_size || by_count,
                        "seed {seed}: Full for {would_be} bytes and {} chunks",
                        held.len() + 1
                    );
                    stats.full_by_size += usize::from(by_size);
                    stats.full_by_count += usize::from(by_count);
                    // The caller's part: finish this pack, start another, add the blob to it.
                    let closed = std::mem::replace(
                        &mut current,
                        builder(&sp, PackClass::Content, g, Padding::None),
                    );
                    packs.push((
                        closed.finish(false, &mut entropy(seed)).expect("finishes"),
                        std::mem::take(&mut held),
                    ));
                    assert_eq!(
                        current.add(*id, blob).expect("adds"),
                        AddOutcome::Fits,
                        "seed {seed}: a blob that fits an empty pack was refused by one"
                    );
                    if alone > target {
                        stats.lone_oversize += 1;
                    }
                    held.push(index);
                }
            }
        }
        if held.is_empty() {
            assert_eq!(
                current.finish(true, &mut entropy(seed)).unwrap_err(),
                PackError::EmptyPack
            );
        } else {
            packs.push((
                current.finish(true, &mut entropy(seed)).expect("finishes"),
                held,
            ));
        }

        // Every chunk is in exactly one pack, or was too big; each pack is what the layout says.
        let mut seen: Vec<usize> = too_big.clone();
        for (pack, indices) in &packs {
            stats.packs += 1;
            let lens = lens_of(&chunks, indices);
            assert_eq!(
                pack.len().as_usize(),
                ref_unpadded_from_lens(&lens),
                "seed {seed}: the pack is not as long as the layout says"
            );
            if indices.len() > 1 {
                assert!(
                    pack.len() <= g.target,
                    "seed {seed}: a pack of several chunks above target"
                );
            }
            assert!(
                pack.len() <= g.hard_max,
                "seed {seed}: a pack above hard_max"
            );
            assert_eq!(pack.entries().len(), indices.len());
            let mut at = 24usize;
            for (entry, &i) in pack.entries().iter().zip(indices) {
                let blob = chunks[i].1.as_bytes();
                assert_eq!(
                    entry.id, chunks[i].0,
                    "seed {seed}: blobs are in the order they were added"
                );
                assert_eq!(entry.offset.as_usize(), at);
                assert_eq!(entry.length.as_usize(), blob.len());
                assert_eq!(&pack.bytes()[at..at + blob.len()], blob);
                at += blob.len();
                seen.push(i);
            }
            assert_eq!(at, 24 + lens.iter().sum::<usize>());
        }
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..n).collect::<Vec<_>>(),
            "seed {seed}: every chunk is placed once"
        );
    }
    println!(
        "c3: {} sequences, {} packs, {} blobs fitted, Full by size {} and by count {}, lone oversize packs {}, blobs too big for any pack {}",
        sequences, stats.packs, stats.fits, stats.full_by_size, stats.full_by_count, stats.lone_oversize, stats.too_big
    );
    assert!(
        stats.full_by_size > 0 && stats.full_by_count > 0,
        "both causes of Full were met"
    );
    assert!(
        stats.lone_oversize > 0 && stats.too_big > 0,
        "oversize and too-big blobs were met"
    );

    // finish of an empty builder is an error.
    assert_eq!(
        builder(&sp, PackClass::Content, g, Padding::None)
            .finish(true, &mut entropy(1))
            .unwrap_err(),
        PackError::EmptyPack
    );

    // A chunk twice in one pack is refused and changes nothing.
    let (id, blob) = chunk(&sp, &k, 777, 500);
    let mut b = builder(&sp, PackClass::Content, g, Padding::None);
    assert_eq!(b.add(id, &blob), Ok(AddOutcome::Fits));
    assert_eq!(b.add(id, &blob), Err(PackError::DuplicateChunk));
    assert_eq!(b.chunk_count(), 1);
    assert!(!b.is_empty());
    assert!(b.finish(true, &mut entropy(1)).is_ok());

    // A geometry that cannot make a pack is refused by add and by finish.
    let bad_geometries = [
        PackGeometry {
            pad_step: PackLen::ZERO,
            ..g
        },
        PackGeometry {
            target: PackLen::ZERO,
            ..g
        },
        PackGeometry {
            hard_max: PackLen::new(g.target.get() - 1),
            ..g
        },
        PackGeometry {
            hard_max: PackLen::new(HARD_MAX.get() + 1),
            ..g
        },
        PackGeometry { max_chunks: 0, ..g },
        PackGeometry {
            max_chunks: MAX_CHUNKS + 1,
            ..g
        },
    ];
    for bad in bad_geometries {
        let mut b = builder(&sp, PackClass::Content, bad, Padding::None);
        assert_eq!(b.add(id, &blob), Err(PackError::BadGeometry), "{bad:?}");
        assert_eq!(
            b.finish(true, &mut entropy(1)).unwrap_err(),
            PackError::BadGeometry,
            "{bad:?}"
        );
    }
    assert_eq!(PackGeometry::default().validate(), Ok(()));
    assert_eq!(PackGeometry::default().target, TARGET);
    assert_eq!(PackGeometry::default().hard_max, HARD_MAX);
    assert_eq!(PackGeometry::default().pad_step, PAD_STEP);
    assert_eq!(PackGeometry::default().max_chunks, MAX_CHUNKS);
    assert_eq!(MAX_CHUNKS, 16_384);
    assert_eq!(HARD_MAX.get(), 4 * 1024 * 1024 + 64 * 1024);

    // max_chunks is a cause of Full by itself: three tiny blobs fit, the fourth is Full.
    let three = PackGeometry { max_chunks: 3, ..g };
    let mut b = builder(&sp, PackClass::Content, three, Padding::None);
    for i in 0..3u64 {
        let (id, blob) = chunk(&sp, &k, 800 + i, 10);
        assert_eq!(b.add(id, &blob), Ok(AddOutcome::Fits));
    }
    let (id4, blob4) = chunk(&sp, &k, 803, 10);
    assert_eq!(b.add(id4, &blob4), Ok(AddOutcome::Full));
    assert_eq!(b.chunk_count(), 3);

    // The exact boundary of target: the largest second blob that fits, and one byte more.
    let first = 1_000usize;
    let fits = |second: usize| ref_unpadded_from_lens(&[first, second]) <= target;
    let (mut lo, mut hi) = (41usize, target);
    assert!(fits(lo) && !fits(hi));
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (id1, blob1) = chunk(&sp, &k, 900, first - 41);
    let (id2, blob2) = chunk(&sp, &k, 901, lo - 41);
    let (id3, blob3) = chunk(&sp, &k, 902, lo + 1 - 41);
    let mut b = builder(&sp, PackClass::Content, g, Padding::None);
    assert_eq!(b.add(id1, &blob1), Ok(AddOutcome::Fits));
    assert_eq!(
        b.add(id3, &blob3),
        Ok(AddOutcome::Full),
        "one byte past target is Full"
    );
    assert_eq!(
        b.add(id2, &blob2),
        Ok(AddOutcome::Fits),
        "the last blob that fits is not Full"
    );
    let pack = b.finish(false, &mut entropy(2)).expect("finishes");
    assert!(
        pack.len() <= g.target && pack.len().as_usize() + 8 >= target,
        "{} bytes",
        pack.len()
    );
    assert_eq!(
        pack.len().as_usize(),
        ref_unpadded_len(&[rc(&id1, &blob1), rc(&id2, &blob2)])
    );

    // The exact boundary of hard_max for a blob alone: the largest that fits, and one byte more.
    let alone_fits = |len: usize| ref_unpadded_from_lens(&[len]) <= hard_max;
    let (mut lo, mut hi) = (41usize, hard_max);
    assert!(alone_fits(lo) && !alone_fits(hi));
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if alone_fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (id_fit, blob_fit) = chunk(&sp, &k, 910, lo - 41);
    let (id_big, blob_big) = chunk(&sp, &k, 911, lo + 1 - 41);
    let mut b = builder(&sp, PackClass::Content, g, Padding::None);
    assert_eq!(b.add(id_big, &blob_big), Ok(AddOutcome::TooBigForAnyPack));
    assert!(b.is_empty(), "a blob that fits no pack is not kept");
    assert_eq!(
        b.add(id_fit, &blob_fit),
        Ok(AddOutcome::Fits),
        "a lone blob up to hard_max is a pack of its own"
    );
    let pack = b.finish(true, &mut entropy(3)).expect("finishes");
    assert!(
        pack.len() > g.target && pack.len() <= g.hard_max,
        "{} bytes",
        pack.len()
    );
}

#[test]
fn c4_entries_equal_real_positions() {
    let sp = space(3);
    let k = k_chunk(4);
    let key = seal_key(EPOCH, KEY_BYTE);
    let sequences = scaled("c4 sequences", 200, 30) as u64;
    let g = small_geometry();
    let mut checked = 0usize;
    for seed in 0..sequences {
        let mut rng = SplitMix64::new(4_000 + seed);
        let n = 1 + rng.below(60) as usize;
        let chunks = random_chunks(&mut rng, &sp, &k, n, 10_000 + seed * 100);
        // One pack: as many of the chunks as fit, from the first, under the buckets.
        let mut b = builder(&sp, PackClass::Content, g, Padding::Buckets);
        let mut added: Vec<usize> = Vec::new();
        for (index, (id, blob)) in chunks.iter().enumerate() {
            match b.add(*id, blob).expect("adds") {
                AddOutcome::Fits => added.push(index),
                AddOutcome::Full | AddOutcome::TooBigForAnyPack => {}
            }
        }
        if added.is_empty() {
            continue;
        }
        let pack = b.finish(true, &mut entropy(seed)).expect("finishes");
        let total = pack.len();
        let header = PackHeader::parse(pack.bytes(), total).expect("a header");
        let trailer_range = header.trailer_range(total).expect("a trailer range");
        let trailer = &pack.bytes()[trailer_range.start.as_usize()..trailer_range.end.as_usize()];

        // Every line is the real position of the blob that was added.
        assert_eq!(pack.entries().len(), added.len());
        for (entry, &i) in pack.entries().iter().zip(&added) {
            let blob = chunks[i].1.as_bytes();
            let from = entry.offset.as_usize();
            assert_eq!(
                &pack.bytes()[from..from + entry.length.as_usize()],
                blob,
                "seed {seed}"
            );
            let range = chunk_range(entry, &header).expect("a range inside the body");
            assert_eq!(
                range,
                entry.offset..PackOffset::new(entry.offset.get() + entry.length.get())
            );
            assert_eq!(
                &pack.bytes()[range.start.as_usize()..range.end.as_usize()],
                blob
            );
            checked += 1;
        }
        // What `read_trailer` reads is what `entries()` says, and so is the whole-pack rebuild.
        assert_eq!(
            read_trailer(&header, trailer, &sp, &key).expect("opens"),
            pack.entries(),
            "seed {seed}"
        );
        assert_eq!(
            rebuild_entries(pack.bytes(), &sp, &key).expect("rebuilds"),
            pack.entries(),
            "seed {seed}"
        );
        // The summary is what the index takes.
        let summary = pack.summary();
        assert_eq!(
            (summary.id, summary.name, summary.len, summary.class),
            (pack.id(), pack.name(), total, PackClass::Content)
        );
        assert_eq!(summary.entries, pack.entries());
    }
    println!("c4: {checked} lines compared with the bytes of their packs");
    assert!(checked > 300, "{checked} lines checked");

    // `chunk_range` refuses a line that does not lie inside the body.
    let (id, blob) = chunk(&sp, &k, 5, 400);
    let pack = build(
        &sp,
        PackClass::Content,
        EPOCH,
        KEY_BYTE,
        g,
        Padding::None,
        true,
        &[(id, blob)],
        1,
    );
    let header = PackHeader::parse(pack.bytes(), pack.len()).expect("a header");
    let body_end = 24 + header.body_len.get();
    let line = |offset: u32, length: u32| PackEntry {
        id,
        offset: PackOffset::new(offset),
        length: ChunkLen::new(length).expect("a length"),
    };
    assert!(
        chunk_range(&line(24, header.body_len.get()), &header).is_ok(),
        "the whole body"
    );
    for (what, entry) in [
        ("starts at 0, inside the header", line(0, 41)),
        ("starts at 23", line(23, 41)),
        ("ends one byte past the body", line(25, body_end - 24)),
        ("starts at the end of the body", line(body_end, 41)),
        ("overflows a u32", line(u32::MAX, 41)),
        ("starts past the pack", line(body_end + 1_000, 41)),
    ] {
        assert_eq!(
            chunk_range(&entry, &header),
            Err(PackError::EntryOutsideBody),
            "{what}"
        );
    }
}

/// A pack of chunks of these plaintext lengths (content addressed under fixed keys).
fn pack_with(
    class: PackClass,
    g: PackGeometry,
    padding: Padding,
    last: bool,
    plain_lens: &[usize],
    seed: u64,
) -> (SealedPack, Vec<RefChunk>) {
    let sp = space(9);
    let k = k_chunk(8);
    let chunks: Vec<(ChunkId, SealedChunk)> = plain_lens
        .iter()
        .enumerate()
        .map(|(i, &len)| chunk(&sp, &k, seed + i as u64, len))
        .collect();
    let refs = chunks.iter().map(|(id, blob)| rc(id, blob)).collect();
    (
        build(&sp, class, EPOCH, KEY_BYTE, g, padding, last, &chunks, seed),
        refs,
    )
}

#[test]
fn c7_padding_policy() {
    // Three geometries: 4 KiB steps; 8 KiB steps (the repository's 4 KiB is then not the step);
    // a target that is no multiple of the step, and a hard_max that is none either (the cap).
    let g4 = small_geometry();
    let g8 = PackGeometry {
        pad_step: PackLen::new(8 * 1024),
        ..small_geometry()
    };
    let g3 = PackGeometry {
        target: PackLen::new(30_000),
        hard_max: PackLen::new(36_000),
        pad_step: PackLen::new(4 * 1024),
        max_chunks: 40,
    };
    // Shapes: one small chunk, five of 0.5 to 3 KB, one that nearly fills a pack, one that does not
    // fit a pack of its own within target (a lone oversize pack).
    let shapes: [&[usize]; 4] = [
        &[100],
        &[1_000, 2_000, 3_000, 500, 77],
        &[30_000],
        &[33_000],
    ];
    let policies = [
        ("none", Padding::None),
        ("buckets", Padding::Buckets),
        ("full", Padding::Full),
    ];
    let mut checked = 0usize;
    for (gi, g) in [g4, g8, g3].into_iter().enumerate() {
        for class in PackClass::ALL {
            for (policy_name, policy) in policies {
                for last in [true, false] {
                    for (si, shape) in shapes.iter().enumerate() {
                        let seed = 1_000 * (gi as u64 + 1) + 10 * si as u64;
                        let (pack, refs) = pack_with(class, g, policy, last, shape, seed);
                        let unpadded = ref_unpadded_len(&refs);
                        let want = ref_padded_len(unpadded, class.byte(), policy_name, last, &g);
                        let what = format!(
                            "geometry {gi}, {class:?}, {policy_name}, last {last}, shape {si}"
                        );
                        let len = pack.len().as_usize();
                        assert_eq!(len, want, "{what}");
                        let header = PackHeader::parse(pack.bytes(), pack.len()).expect("a header");
                        assert_eq!(header.pad_len.as_usize(), len - unpadded, "{what}");
                        // What the rules say, stated again without the reference.
                        let (target, hard_max) = (g.target.as_usize(), g.hard_max.as_usize());
                        assert!((unpadded..=hard_max).contains(&len), "{what}: {len} bytes");
                        if !last || policy == Padding::None {
                            assert_eq!(len, unpadded, "{what}: never padded");
                        }
                        if last && policy == Padding::Full {
                            assert_eq!(
                                len,
                                if unpadded <= target { target } else { hard_max },
                                "{what}"
                            );
                        }
                        if last && policy == Padding::Buckets {
                            let step = if class == PackClass::Repository {
                                4096
                            } else {
                                g.pad_step.as_usize()
                            };
                            let cap = if unpadded <= target { target } else { hard_max };
                            assert!(
                                len.is_multiple_of(step) || len == cap,
                                "{what}: {len} is no bucket of {step}"
                            );
                            assert!(
                                len - unpadded < step || len == cap,
                                "{what}: padded by a whole bucket"
                            );
                        }
                        checked += 1;
                    }
                }
            }
        }
    }
    println!("c7: {checked} packs checked against the rules");

    // A few numbers worked out by hand (one blob of 141 bytes: 24 + 141 + a trailer of 100 = 265).
    let (p, _) = pack_with(PackClass::Content, g4, Padding::None, true, &[100], 1);
    assert_eq!(p.len().get(), 265);
    let (p, _) = pack_with(PackClass::Content, g4, Padding::Buckets, true, &[100], 1);
    assert_eq!(p.len().get(), 4096, "to the next 4 KiB");
    let (p, _) = pack_with(PackClass::Content, g8, Padding::Buckets, true, &[100], 1);
    assert_eq!(
        p.len().get(),
        8192,
        "a content pack goes to the step, 8 KiB here"
    );
    let (p, _) = pack_with(PackClass::Repository, g8, Padding::Buckets, true, &[100], 1);
    assert_eq!(p.len().get(), 4096, "a repository pack goes to 4 KiB");
    let (p, _) = pack_with(PackClass::Derived, g8, Padding::Buckets, true, &[100], 1);
    assert_eq!(
        p.len().get(),
        8192,
        "a derived pack is padded like a content pack"
    );
    let (p, _) = pack_with(PackClass::Content, g4, Padding::Full, true, &[100], 1);
    assert_eq!(p.len().get(), 32 * 1024, "full: to target");
    let (p, _) = pack_with(PackClass::Content, g4, Padding::Buckets, false, &[100], 1);
    assert_eq!(p.len().get(), 265, "not the last pack: not padded");
    // The caps of the odd geometry: a pack of 29,466 bytes would go to 32,768, but target is 30,000; a
    // lone oversize pack (33,000 plaintext bytes) would go to 36,864, but hard_max is 36,000.
    let (p, _) = pack_with(PackClass::Content, g3, Padding::Buckets, true, &[29_300], 3);
    assert_eq!(p.len().get(), 30_000, "padded to target, not past it");
    let (p, _) = pack_with(PackClass::Content, g3, Padding::Buckets, true, &[33_000], 3);
    assert_eq!(
        p.len().get(),
        36_000,
        "a pack of its own is padded to hard_max, not past it"
    );
    let (p, _) = pack_with(PackClass::Content, g3, Padding::Full, true, &[33_000], 3);
    assert_eq!(p.len().get(), 36_000);

    // The padding bytes: the keystream of the plan (the vectors pin them byte for byte), random
    // looking, different for every nonce, the same for the same one.
    let sp = space(9);
    let k = k_chunk(8);
    let chunks: Vec<(ChunkId, SealedChunk)> =
        vec![chunk(&sp, &k, 50, 300), chunk(&sp, &k, 51, 700)];
    let nonce = [0x5a; 24];
    let pack = build_with(
        &sp,
        PackClass::Content,
        EPOCH,
        KEY_BYTE,
        g4,
        Padding::Buckets,
        true,
        &chunks,
        &mut FixedNonce(nonce),
    );
    let header = PackHeader::parse(pack.bytes(), pack.len()).expect("a header");
    let pad_from = 24 + header.body_len.as_usize();
    let padding = &pack.bytes()[pad_from..pad_from + header.pad_len.as_usize()];
    assert!(padding.len() > 2_000, "{} bytes of padding", padding.len());
    assert!(padding.iter().any(|&b| b != 0), "padding is never zeros");
    assert_eq!(
        randomness_check(padding),
        Ok(()),
        "the padding passes the node's randomness test"
    );
    let params = RefParams {
        space: [9; 16],
        epoch: EPOCH,
        k_seal: [KEY_BYTE; 32],
        class: 0,
        pad_len: padding.len(),
        nonce,
    };
    let reference = ref_pack(
        &params,
        &chunks
            .iter()
            .map(|(id, blob)| rc(id, blob))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        padding,
        reference.padding.as_slice(),
        "the padding is the keystream of the plan"
    );
    let other = build_with(
        &sp,
        PackClass::Content,
        EPOCH,
        KEY_BYTE,
        g4,
        Padding::Buckets,
        true,
        &chunks,
        &mut FixedNonce([0xa5; 24]),
    );
    let other_padding = &other.bytes()[pad_from..pad_from + padding.len()];
    assert_ne!(padding, other_padding, "another nonce, other padding");
    let again = build_with(
        &sp,
        PackClass::Content,
        EPOCH,
        KEY_BYTE,
        g4,
        Padding::Buckets,
        true,
        &chunks,
        &mut FixedNonce(nonce),
    );
    assert_eq!(again.bytes(), pack.bytes(), "the same nonce, the same pack");

    // The numbers of the plan, with the default geometry: 256 KiB buckets, 4 MiB packs, 4 KiB for the
    // repository.
    assert_eq!(Padding::default(), Padding::Buckets);
    assert_eq!(PAD_STEP.get(), 256 * 1024);
    assert_eq!(REPO_PAD.get(), 4 * 1024);
    assert_eq!(TARGET.get(), 4 * 1024 * 1024);
    let default = PackGeometry::default();
    let (p, _) = pack_with(
        PackClass::Content,
        default,
        Padding::Buckets,
        true,
        &[100, 50_000, 60_000],
        70,
    );
    assert_eq!(
        p.len().get(),
        256 * 1024,
        "the last pack goes to the next 256 KiB step"
    );
    assert!(p.len() < HARD_MAX);
    let (p, _) = pack_with(
        PackClass::Repository,
        default,
        Padding::Buckets,
        true,
        &[100],
        71,
    );
    assert_eq!(p.len().get(), 4096, "repository objects go to 4 KiB");
    let (p, _) = pack_with(PackClass::Content, default, Padding::Full, true, &[100], 72);
    assert_eq!(
        p.len(),
        TARGET,
        "full: every pack the policy pads is a whole 4 MiB"
    );
    let (p, _) = pack_with(PackClass::Content, default, Padding::None, true, &[100], 73);
    assert_eq!(p.len().get(), 265);
}

#[test]
fn c9_determinism_and_idempotent_resend() {
    let sp = space(11);
    let k = k_chunk(12);
    let g = small_geometry();
    let chunks: Vec<(ChunkId, SealedChunk)> = (0..6u64)
        .map(|i| chunk(&sp, &k, 500 + i, 50 + 300 * i as usize))
        .collect();
    let make = |seed: u64| {
        build(
            &sp,
            PackClass::Content,
            4,
            KEY_BYTE,
            g,
            Padding::Buckets,
            true,
            &chunks,
            seed,
        )
    };

    // The same inputs and the same entropy: the same pack, name, id and lines.
    let (a1, a2, b) = (make(1), make(1), make(2));
    assert_eq!(a1.bytes(), a2.bytes());
    assert_eq!((a1.name(), a1.id()), (a2.name(), a2.id()));
    assert_eq!(a1.entries(), a2.entries());
    assert_eq!(a1.summary(), a2.summary());

    // Another nonce: the header and the blobs are the same bytes; the padding, the trailer and with
    // them the name differ.
    let header = PackHeader::parse(a1.bytes(), a1.len()).expect("a header");
    assert_eq!(PackHeader::parse(b.bytes(), b.len()), Ok(header));
    let body_end = 24 + header.body_len.as_usize();
    let pad_end = body_end + header.pad_len.as_usize();
    assert!(
        header.pad_len.get() >= 16,
        "the pack has padding to compare"
    );
    assert_eq!(
        &a1.bytes()[..body_end],
        &b.bytes()[..body_end],
        "header and blobs are the same bytes"
    );
    assert_ne!(
        &a1.bytes()[body_end..pad_end],
        &b.bytes()[body_end..pad_end],
        "the padding follows the nonce"
    );
    assert_ne!(
        &a1.bytes()[pad_end..],
        &b.bytes()[pad_end..],
        "the trailer follows the nonce"
    );
    assert_ne!(a1.name(), b.name());
    assert_ne!(a1.id(), b.id());
    assert_eq!(
        a1.entries(),
        b.entries(),
        "the lines do not depend on the nonce"
    );

    // The blobs inside are the ones that were added, byte for byte.
    for ((_, blob), entry) in chunks.iter().zip(a1.entries()) {
        let from = entry.offset.as_usize();
        assert_eq!(
            &a1.bytes()[from..from + blob.as_bytes().len()],
            blob.as_bytes()
        );
    }

    // A pack that is sent again (its bytes kept) has the same name and storage key.
    let resent = a1.clone();
    assert_eq!(resent.name(), a1.name());
    assert_eq!(
        azlin_content::pack::storage_key(&sp, &resent.id()),
        azlin_content::pack::storage_key(&sp, &a1.id())
    );

    // No entropy, no pack: nothing is made with a nonce of zeros.
    for good in [0usize, 23] {
        let mut b = builder(&sp, PackClass::Content, g, Padding::Buckets);
        for (id, blob) in &chunks {
            assert_eq!(b.add(*id, blob), Ok(AddOutcome::Fits));
        }
        assert_eq!(
            b.finish(true, &mut FailingEntropy::after(good))
                .unwrap_err(),
            PackError::Entropy(EntropyError::Exhausted),
            "{good} good bytes are not enough for a 24-byte nonce"
        );
    }
    // And the nonce is the only thing drawn: 24 good bytes are enough.
    let mut b = builder(&sp, PackClass::Content, g, Padding::Buckets);
    for (id, blob) in &chunks {
        assert_eq!(b.add(*id, blob), Ok(AddOutcome::Fits));
    }
    let mut ent = FailingEntropy::after(24);
    assert!(b.finish(true, &mut ent).is_ok());
}
