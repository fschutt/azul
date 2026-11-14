//! C5, C6, C13 and C14: reading packs back (planning/architecture.md section 5.7 "a trailer sealed with
//! `K_seal`", "a lost chunk index can be rebuilt from the trailers", "packs below 50 % live are
//! repacked").
//!
//! C5: the trailer is sealed and bound to its pack: a wrong key, space or epoch, another pack's trailer,
//! and a flipped bit anywhere in the pack (exhaustive on a small one) are refused. C6: the index rebuild
//! from the whole pack, and what it says of a truncated or damaged pack. C13: a repack copies the blobs
//! verbatim and needs only `K_seal`. C14: the limits hold before anything is allocated for a claim (the
//! allocation counts themselves are in `pack_alloc.rs`).

mod pack_common;

use azlin_content::pack::{
    read_trailer, rebuild_entries, repack, repack_with, AddOutcome, PackBuilder, PackEntry,
    PackError, PackGeometry, PackHeader, Padding, SealedPack, MAX_CHUNKS,
};
use azlin_content::{ChunkId, KChunk, PackClass, PackLen, SealedChunk};
use azlin_keys::{EntropyError, Epoch, FailingEntropy, SeededEntropy, SpaceId};
use pack_common::{
    build, cbor_head, chunk, k_chunk, ref_seal, scaled, seal_key, small_geometry, space, RefParams,
    SplitMix64,
};

fn tiny_geometry() -> PackGeometry {
    PackGeometry {
        target: PackLen::new(2048),
        hard_max: PackLen::new(3072),
        pad_step: PackLen::new(256),
        max_chunks: 8,
    }
}

/// The header of a pack, and the bytes of its trailer.
fn parts(pack: &SealedPack) -> (PackHeader, Vec<u8>) {
    let header = PackHeader::parse(pack.bytes(), pack.len()).expect("a header");
    let range = header.trailer_range(pack.len()).expect("a trailer range");
    let trailer = pack.bytes()[range.start.as_usize()..range.end.as_usize()].to_vec();
    (header, trailer)
}

#[test]
fn c5_trailer_is_sealed_and_bound_to_its_pack() {
    let sp = space(21);
    let k = k_chunk(22);
    let key = seal_key(3, 0x44);
    let g = tiny_geometry();
    let make = |seed: u64, plain: usize| {
        build(
            &sp,
            PackClass::Content,
            3,
            0x44,
            g,
            Padding::Buckets,
            true,
            &[chunk(&sp, &k, seed, plain)],
            seed,
        )
    };
    let a = make(1, 60);
    let b = make(2, 60); // the same sizes, another chunk: the same header
    let c = make(3, 61); // another body length: another header
    assert_eq!(
        a.len().get(),
        256,
        "one blob of 101 bytes, 31 bytes of padding, a trailer of 100"
    );
    let (header, trailer) = parts(&a);
    let (header_b, trailer_b) = parts(&b);
    let (header_c, trailer_c) = parts(&c);
    assert_eq!(header, header_b, "a and b have one header");
    assert_ne!(header, header_c);
    assert_eq!(
        (trailer.len(), trailer_b.len(), trailer_c.len()),
        (100, 100, 100)
    );

    // The right key, space and epoch open it.
    assert_eq!(
        read_trailer(&header, &trailer, &sp, &key).expect("opens"),
        a.entries()
    );
    assert_eq!(
        rebuild_entries(a.bytes(), &sp, &key).expect("rebuilds"),
        a.entries()
    );

    // A wrong K_seal, space or epoch: Auth, by both readers.
    let wrong_key = seal_key(3, 0x45);
    let other_space = space(23);
    let other_epoch = seal_key(4, 0x44);
    for (what, sp_in, key_in) in [
        ("a wrong K_seal", &sp, &wrong_key),
        ("a wrong space id", &other_space, &key),
        ("a wrong epoch (the same key bytes)", &sp, &other_epoch),
    ] {
        assert_eq!(
            read_trailer(&header, &trailer, sp_in, key_in),
            Err(PackError::Auth),
            "{what}"
        );
        assert_eq!(
            rebuild_entries(a.bytes(), sp_in, key_in),
            Err(PackError::Auth),
            "{what}"
        );
    }

    // A header that says something else about the pack: Auth (the associated data holds the header).
    let mut other_class = header;
    other_class.class = PackClass::Derived;
    assert_eq!(
        read_trailer(&other_class, &trailer, &sp, &key),
        Err(PackError::Auth),
        "another class"
    );
    let mut other_epoch_header = header;
    other_epoch_header.key_epoch = Epoch::new(4).expect("an epoch");
    assert_eq!(
        read_trailer(&other_epoch_header, &trailer, &sp, &key),
        Err(PackError::Auth),
        "another epoch in the header"
    );
    let mut longer_body = header;
    longer_body.body_len = PackLen::new(header.body_len.get() + 1);
    assert_eq!(
        read_trailer(&longer_body, &trailer, &sp, &key),
        Err(PackError::Auth),
        "another body length"
    );
    let mut longer_padding = header;
    longer_padding.pad_len = PackLen::new(header.pad_len.get() + 1);
    assert_eq!(
        read_trailer(&longer_padding, &trailer, &sp, &key),
        Err(PackError::Auth),
        "another padding length"
    );

    // Another pack's trailer spliced in. Under another header it does not open; under the same header
    // it opens (the trailer is sealed on its own, which is what a ranged read needs) and is the OTHER
    // pack's lines, but the whole pack with it spliced in is refused: the body is not the one the
    // trailer sealed.
    assert_eq!(
        read_trailer(&header, &trailer_c, &sp, &key),
        Err(PackError::Auth),
        "a trailer of a pack with another header"
    );
    assert_eq!(
        read_trailer(&header, &trailer_b, &sp, &key).expect("opens on its own"),
        b.entries()
    );
    assert_ne!(b.entries(), a.entries());
    let trailer_at = a.bytes().len() - trailer.len();
    for (what, other) in [
        ("the same header", &trailer_b),
        ("another header", &trailer_c),
    ] {
        let mut spliced = a.bytes().to_vec();
        spliced[trailer_at..].copy_from_slice(other);
        assert_eq!(
            rebuild_entries(&spliced, &sp, &key),
            Err(PackError::Auth),
            "a trailer of a pack with {what} spliced into a"
        );
    }
    // The body of b under the trailer of a: the same.
    let mut body_swapped = a.bytes().to_vec();
    body_swapped[24..24 + 101].copy_from_slice(&b.bytes()[24..24 + 101]);
    assert_eq!(
        rebuild_entries(&body_swapped, &sp, &key),
        Err(PackError::Auth),
        "the body of another pack"
    );

    // Moving a body byte changes the hash of the body and breaks the trailer (for the reader that has
    // the body); `read_trailer`, which has only the trailer, does not see it.
    let (i, j) = (
        30usize,
        (31..120)
            .find(|&j| a.bytes()[j] != a.bytes()[30])
            .expect("two different bytes"),
    );
    let mut moved = a.bytes().to_vec();
    moved.swap(i, j);
    assert_eq!(
        rebuild_entries(&moved, &sp, &key),
        Err(PackError::Auth),
        "two body bytes swapped"
    );

    // Every bit of every byte of the pack, flipped one at a time (2,048 packs): never accepted. A bit in
    // the header is the error of its field or Auth; a bit in the body, the padding or the trailer is Auth.
    let mut kinds = std::collections::BTreeMap::new();
    for byte in 0..a.bytes().len() {
        for bit in 0..8 {
            let mut flipped = a.bytes().to_vec();
            flipped[byte] ^= 1 << bit;
            let err = rebuild_entries(&flipped, &sp, &key).expect_err(&format!(
                "byte {byte} bit {bit}: a damaged pack was accepted"
            ));
            if byte >= 24 {
                assert_eq!(err, PackError::Auth, "byte {byte} bit {bit}");
            } else {
                let allowed = match byte {
                    0..=3 => err == PackError::BadMagic,
                    4 => matches!(err, PackError::UnsupportedVersion(_)),
                    5 => matches!(err, PackError::BadClass(_) | PackError::Auth),
                    6 | 7 => err == PackError::ReservedNotZero,
                    _ => matches!(
                        err,
                        PackError::Auth
                            | PackError::BadEpoch
                            | PackError::NoChunks
                            | PackError::TooManyChunks
                            | PackError::SizesInconsistent
                    ),
                };
                assert!(allowed, "byte {byte} bit {bit}: {err:?}");
            }
            *kinds
                .entry(
                    format!("{err:?}")
                        .split('(')
                        .next()
                        .expect("a name")
                        .to_string(),
                )
                .or_insert(0usize) += 1;
        }
    }
    println!(
        "c5: {} single-bit flips of a {}-byte pack, all refused: {kinds:?}",
        a.bytes().len() * 8,
        a.bytes().len()
    );
    assert!(kinds.get("Auth").copied().unwrap_or(0) > 1_800, "{kinds:?}");

    // And every bit of the trailer alone, for the reader that has only the trailer.
    for byte in 0..trailer.len() {
        for bit in 0..8 {
            let mut flipped = trailer.clone();
            flipped[byte] ^= 1 << bit;
            assert_eq!(
                read_trailer(&header, &flipped, &sp, &key),
                Err(PackError::Auth),
                "trailer byte {byte} bit {bit}"
            );
        }
    }
}

/// A random pack: any class, any padding, 1 to 12 chunks of up to 2,500 bytes.
fn random_pack(
    rng: &mut SplitMix64,
    sp: &SpaceId,
    k: &KChunk,
    epoch: u32,
    key_byte: u8,
    seed: u64,
) -> SealedPack {
    let class = PackClass::ALL[rng.below(3) as usize];
    let padding = [Padding::None, Padding::Buckets, Padding::Full][rng.below(3) as usize];
    let last = rng.below(2) == 0;
    let n = 1 + rng.below(12) as usize;
    let chunks: Vec<(ChunkId, SealedChunk)> = (0..n)
        .map(|i| chunk(sp, k, seed * 100 + i as u64, 1 + rng.below(2_500) as usize))
        .collect();
    build(
        sp,
        class,
        epoch,
        key_byte,
        small_geometry(),
        padding,
        last,
        &chunks,
        seed,
    )
}

#[test]
fn c6_index_rebuild_from_trailers() {
    let sp = space(31);
    let k = k_chunk(32);
    let key = seal_key(2, 0x66);
    let packs = scaled("c6 packs", 150, 40) as u64;
    let mut rng = SplitMix64::new(6);

    // The lines of every random pack rebuild from its bytes.
    let mut all = Vec::new();
    for seed in 0..packs {
        let pack = random_pack(&mut rng, &sp, &k, 2, 0x66, seed);
        assert_eq!(
            rebuild_entries(pack.bytes(), &sp, &key).expect("rebuilds"),
            pack.entries(),
            "pack {seed}"
        );
        all.push(pack);
    }

    // A truncated pack is a typed error, never a partial list: every cut of a small pack, and 60 cuts
    // of each of the others.
    let small = build(
        &sp,
        PackClass::Content,
        2,
        0x66,
        tiny_geometry(),
        Padding::Buckets,
        true,
        &[chunk(&sp, &k, 1, 150)],
        1,
    );
    let mut cuts = 0usize;
    for cut in 0..small.bytes().len() {
        assert!(
            rebuild_entries(&small.bytes()[..cut], &sp, &key).is_err(),
            "cut at {cut}"
        );
        cuts += 1;
    }
    for pack in &all {
        for _ in 0..60 {
            let cut = rng.below(pack.bytes().len() as u64) as usize;
            assert!(
                rebuild_entries(&pack.bytes()[..cut], &sp, &key).is_err(),
                "cut at {cut} of {}",
                pack.bytes().len()
            );
            cuts += 1;
        }
    }
    assert_eq!(rebuild_entries(&[], &sp, &key), Err(PackError::TooShort));

    // A damaged trailer, a trailer of zeros, a pack with bytes added, a pack read under another key.
    for pack in &all {
        let (_, trailer) = parts(pack);
        let trailer_at = pack.bytes().len() - trailer.len();
        for _ in 0..8 {
            let mut damaged = pack.bytes().to_vec();
            let at = trailer_at + rng.below(trailer.len() as u64) as usize;
            damaged[at] ^= 1 << rng.below(8);
            assert_eq!(rebuild_entries(&damaged, &sp, &key), Err(PackError::Auth));
        }
        let mut zeros = pack.bytes().to_vec();
        zeros[trailer_at..].fill(0);
        assert_eq!(rebuild_entries(&zeros, &sp, &key), Err(PackError::Auth));
        let mut longer = pack.bytes().to_vec();
        let extra = 1 + rng.below(40) as usize;
        longer.extend_from_slice(&rng.bytes(extra));
        assert!(rebuild_entries(&longer, &sp, &key).is_err());
        assert_eq!(
            rebuild_entries(pack.bytes(), &sp, &seal_key(2, 0x67)),
            Err(PackError::Auth)
        );
        assert_eq!(
            rebuild_entries(pack.bytes(), &space(32), &key),
            Err(PackError::Auth)
        );
    }
    println!("c6: {packs} packs rebuilt, {cuts} truncations refused");
}

/// The bytes of a pack's chunk blob, copied (what a ranged read gives).
fn blob_of(pack: &SealedPack, entry: &PackEntry) -> Vec<u8> {
    let from = entry.offset.as_usize();
    pack.bytes()[from..from + entry.length.as_usize()].to_vec()
}

#[test]
fn c13_repack_copies_blobs_verbatim() {
    let sp = space(41);
    let k = k_chunk(42);
    let g = small_geometry();
    let mut rng = SplitMix64::new(13);
    // Three old packs of random chunks, as the pipeline made them (full-size packs); the repack makes
    // small ones, so that the live chunks fill more than one.
    let old: Vec<SealedPack> = (0..3u64)
        .map(|seed| {
            let n = 6 + rng.below(8) as usize;
            let chunks: Vec<(ChunkId, SealedChunk)> = (0..n)
                .map(|i| {
                    chunk(
                        &sp,
                        &k,
                        seed * 100 + i as u64,
                        1 + rng.below(6_000) as usize,
                    )
                })
                .collect();
            build(
                &sp,
                PackClass::Content,
                5,
                0x33,
                PackGeometry::default(),
                Padding::Buckets,
                true,
                &chunks,
                seed,
            )
        })
        .collect();
    let before: Vec<Vec<u8>> = old.iter().map(|p| p.bytes().to_vec()).collect();

    // The live chunks: two of every three lines; the blobs are read from the old packs.
    let mut live_entries: Vec<(PackEntry, Vec<u8>)> = Vec::new();
    for pack in &old {
        for (i, entry) in pack.entries().iter().enumerate() {
            if i % 3 != 1 {
                live_entries.push((*entry, blob_of(pack, entry)));
            }
        }
    }
    let live: Vec<(PackEntry, &[u8])> = live_entries
        .iter()
        .map(|(e, b)| (*e, b.as_slice()))
        .collect();
    assert!(live.len() > 10);

    // The repack needs `K_seal` and nothing else (there is no K_chunk in this call), and copies verbatim.
    let new = repack(
        &live,
        sp,
        PackClass::Content,
        seal_key(5, 0x33),
        g,
        &mut SeededEntropy::from_u64(13),
    )
    .expect("repacks");
    assert!(!new.is_empty());
    let new_key = seal_key(5, 0x33);
    let mut placed = Vec::new();
    for (pi, pack) in new.iter().enumerate() {
        assert_eq!(
            rebuild_entries(pack.bytes(), &sp, &new_key).expect("a repacked pack reads back"),
            pack.entries()
        );
        assert_eq!(pack.class(), PackClass::Content);
        if pi + 1 < new.len() {
            assert_eq!(
                PackHeader::parse(pack.bytes(), pack.len())
                    .expect("a header")
                    .pad_len
                    .get(),
                0,
                "not the last pack: not padded"
            );
        } else {
            assert!(
                pack.len().as_usize().is_multiple_of(4096) || pack.len() == g.target,
                "the last pack is padded by the default policy (buckets)"
            );
        }
        assert!(pack.len() <= g.target || pack.entries().len() == 1);
        for entry in pack.entries() {
            placed.push((*entry, blob_of(pack, entry)));
        }
    }
    // Every live blob, in order, byte for byte, at its new offset.
    assert_eq!(placed.len(), live.len());
    for ((new_entry, new_blob), (old_entry, old_blob)) in placed.iter().zip(&live_entries) {
        assert_eq!(new_entry.id, old_entry.id, "the same chunk");
        assert_eq!(new_entry.length, old_entry.length);
        assert_eq!(new_blob, old_blob, "the blob is copied verbatim");
    }
    assert!(
        placed
            .iter()
            .zip(&live_entries)
            .any(|((n, _), (o, _))| n.offset != o.offset),
        "the lines point at the new offsets, not the old ones"
    );
    // The old packs are untouched (a pack is rewritten, never edited).
    for (pack, bytes) in old.iter().zip(&before) {
        assert_eq!(pack.bytes(), bytes.as_slice());
    }

    // The policy: none leaves the last pack unpadded, full pads it to target.
    let none = repack_with(
        &live,
        sp,
        PackClass::Content,
        seal_key(5, 0x33),
        g,
        Padding::None,
        &mut SeededEntropy::from_u64(13),
    )
    .expect("repacks");
    assert_eq!(none.len(), new.len());
    let last = none.last().expect("a pack");
    let unpadded: usize = 24
        + last
            .entries()
            .iter()
            .map(|e| e.length.as_usize())
            .sum::<usize>();
    let longest_trailer = 80 + 28 * last.entries().len();
    assert!(
        last.len().as_usize() > unpadded && last.len().as_usize() <= unpadded + longest_trailer,
        "no padding: the header, the blobs and a trailer of at most {longest_trailer} bytes"
    );
    let full = repack_with(
        &live,
        sp,
        PackClass::Derived,
        seal_key(5, 0x33),
        g,
        Padding::Full,
        &mut SeededEntropy::from_u64(13),
    )
    .expect("repacks");
    assert_eq!(full.last().expect("a pack").len(), g.target);
    assert_eq!(full.last().expect("a pack").class(), PackClass::Derived);

    // Refusals.
    let one = &live[0];
    // a blob that fits no pack
    let tiny = PackGeometry {
        target: PackLen::new(1024),
        hard_max: PackLen::new(1024),
        pad_step: PackLen::new(256),
        max_chunks: 8,
    };
    let big_blob = vec![7u8; 3_000];
    let big_entry = PackEntry {
        length: azlin_content::ChunkLen::new(3_000).expect("a length"),
        ..one.0
    };
    assert_eq!(
        repack(
            &[(big_entry, big_blob.as_slice())],
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            tiny,
            &mut SeededEntropy::from_u64(1)
        ),
        Err(PackError::TooBigForAnyPack)
    );
    // a line that says another length than the blob has
    let short_entry = PackEntry {
        length: azlin_content::ChunkLen::new(one.0.length.get() + 1).expect("a length"),
        ..one.0
    };
    assert_eq!(
        repack(
            &[(short_entry, one.1)],
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            g,
            &mut SeededEntropy::from_u64(1)
        ),
        Err(PackError::LengthMismatch)
    );
    // a blob shorter than its framing
    let stub = [0u8; 40];
    let stub_entry = PackEntry {
        length: azlin_content::ChunkLen::new(40).expect("a length"),
        ..one.0
    };
    assert_eq!(
        repack(
            &[(stub_entry, stub.as_slice())],
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            g,
            &mut SeededEntropy::from_u64(1)
        ),
        Err(PackError::Chunk(azlin_content::ChunkError::Truncated))
    );
    // a chunk twice
    assert_eq!(
        repack(
            &[*one, *one],
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            g,
            &mut SeededEntropy::from_u64(1)
        ),
        Err(PackError::DuplicateChunk)
    );
    // nothing to repack, no pack
    assert_eq!(
        repack(
            &[],
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            g,
            &mut SeededEntropy::from_u64(1)
        ),
        Ok(vec![])
    );
    // a geometry that cannot make a pack
    let bad = PackGeometry { max_chunks: 0, ..g };
    assert_eq!(
        repack(
            &live,
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            bad,
            &mut SeededEntropy::from_u64(1)
        ),
        Err(PackError::BadGeometry)
    );
    // no entropy, no pack: not even for the first, and not for the second pack of two
    assert_eq!(
        repack(
            &live,
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            g,
            &mut FailingEntropy::after(0)
        ),
        Err(PackError::Entropy(EntropyError::Exhausted))
    );
    assert!(
        new.len() >= 2,
        "the test needs two packs to run out of entropy at the second"
    );
    assert_eq!(
        repack(
            &live,
            sp,
            PackClass::Content,
            seal_key(5, 0x33),
            g,
            &mut FailingEntropy::after(24)
        ),
        Err(PackError::Entropy(EntropyError::Exhausted))
    );
    // the builder itself can be asked the same through its own API: a blob it was given is the blob it keeps
    let mut b = PackBuilder::new(sp, PackClass::Content, seal_key(5, 0x33), g, Padding::None);
    let blob = SealedChunk::from_stored(one.1).expect("a blob");
    assert_eq!(b.add(one.0.id, &blob), Ok(AddOutcome::Fits));
    let single = b
        .finish(true, &mut SeededEntropy::from_u64(2))
        .expect("finishes");
    assert_eq!(blob_of(&single, &single.entries()[0]), one.1);
}

#[test]
fn c14_limits_hold_before_allocation() {
    let sp = space(51);
    let k = k_chunk(52);
    let key = seal_key(3, 0x44);

    // A header that claims a body near 2^32 on a 1 KiB input, and a count above the cap: refused by the
    // header alone.
    let (id, blob) = chunk(&sp, &k, 1, 60);
    let pack = build(
        &sp,
        PackClass::Content,
        3,
        0x44,
        tiny_geometry(),
        Padding::None,
        true,
        &[(id, blob)],
        1,
    );
    let mut kib = pack.bytes().to_vec();
    kib.resize(1024, 0);
    let mut claims_body = kib.clone();
    claims_body[16..20].copy_from_slice(&(u32::MAX - 7).to_le_bytes());
    assert_eq!(
        PackHeader::parse(&claims_body, PackLen::new(1024)),
        Err(PackError::SizesInconsistent)
    );
    assert_eq!(
        rebuild_entries(&claims_body, &sp, &key),
        Err(PackError::SizesInconsistent)
    );
    let mut claims_count = kib.clone();
    claims_count[12..16].copy_from_slice(&(MAX_CHUNKS + 1).to_le_bytes());
    assert_eq!(
        PackHeader::parse(&claims_count, PackLen::new(1024)),
        Err(PackError::TooManyChunks)
    );
    assert_eq!(
        rebuild_entries(&claims_count, &sp, &key),
        Err(PackError::TooManyChunks)
    );
    // The most the cap allows, on a trailer that is far too short for it.
    let mut claims_cap = kib.clone();
    claims_cap[12..16].copy_from_slice(&MAX_CHUNKS.to_le_bytes());
    assert_eq!(
        rebuild_entries(&claims_cap, &sp, &key),
        Err(PackError::SizesInconsistent)
    );

    // An AUTHENTIC trailer that declares 2^31 lines (and 2^32 - 1, and 2^64 - 1, and one past the cap) in
    // a pack of about 200 bytes: the decoder refuses the claim, nothing is allocated for it.
    let body = {
        let (_, blob) = chunk(&sp, &k, 2, 60);
        blob.as_bytes().to_vec()
    };
    let params = RefParams {
        space: [51; 16],
        epoch: 3,
        k_seal: [0x44; 32],
        class: 0,
        pad_len: 0,
        nonce: [3; 24],
    };
    let hash = *blake3::hash(&body).as_bytes();
    let claims: [Vec<u8>; 5] = [
        vec![0x9a, 0x80, 0, 0, 0],
        vec![0x9a, 0xff, 0xff, 0xff, 0xff],
        vec![0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        vec![0x99, 0x40, 0x01],
        vec![0x85],
    ];
    for claim in claims {
        // `a3 01 <the claim> 02 58 20 <hash> 09 <filler>`: three pairs, the second of which is never reached.
        let mut plain = vec![0xa3, 0x01];
        plain.extend_from_slice(&claim);
        plain.extend_from_slice(&[0x02, 0x58, 0x20]);
        plain.extend_from_slice(&hash);
        plain.push(0x09);
        // The filler brings the plaintext to 63 bytes: a trailer of 103, which fits the header's one line.
        let mut n = 0usize;
        loop {
            let head = cbor_head(2, n as u64);
            if plain.len() + head.len() + n == 63 {
                plain.extend_from_slice(&head);
                plain.extend(std::iter::repeat_n(0u8, n));
                break;
            }
            assert!(
                plain.len() + head.len() + n < 63,
                "the claim of {claim:?} is too long to fill to 63 bytes"
            );
            n += 1;
        }
        let made = ref_seal(&params, 1, &body, &plain);
        assert!(made.bytes.len() < 300);
        assert_eq!(
            rebuild_entries(&made.bytes, &sp, &key),
            Err(PackError::BadTrailer),
            "an authentic trailer that claims {claim:?}"
        );
    }
    // The same plaintext with an honest line is accepted: the harness itself is sound.
    let line = ([7u8; 16], 24u32, body.len() as u32);
    let honest = pack_common::ref_plain(&[line], &hash);
    let made = ref_seal(&params, 1, &body, &honest);
    assert_eq!(
        rebuild_entries(&made.bytes, &sp, &key)
            .expect("an honest pack reads")
            .len(),
        1
    );

    // The decoder's own counts: the header says one line, the plaintext lists two.
    let mut two = vec![0xa2, 0x01, 0x82];
    for _ in 0..2 {
        two.extend_from_slice(&[0x83, 0x50]);
        two.extend_from_slice(&[9u8; 16]);
        two.extend_from_slice(&[0x18, 0x18, 0x18, 0x40]);
    }
    two.extend_from_slice(&[0x02, 0x58, 0x20]);
    two.extend_from_slice(&hash);
    let made = ref_seal(&params, 1, &body, &two);
    assert!(rebuild_entries(&made.bytes, &sp, &key).is_err());
}
