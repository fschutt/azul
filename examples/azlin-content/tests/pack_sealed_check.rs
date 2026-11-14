//! C10, C11 and C12: the checks a node runs on a new pack without any key (planning/architecture.md
//! section 2.5 "Nodes check what they can. Every pack must have the AZP2 header (magic, version, class,
//! sizes). The node also tests a few kilobytes of each new pack for randomness ... A pack that fails is
//! refused with `not_sealed`. The check catches mistakes, not malice").
//!
//! C10: no false refusal of a real pack, over 5,000 packs of every class and padding, among them packs of
//! tiny chunks, whose clear codec bytes are the hardest case for a byte histogram. C11: what is not
//! sealed is refused. C12: the threshold is the one the derivation in `sealed_check.rs` gives, recomputed
//! here with an incomplete-gamma routine of the test's own, and the statistic behaves on random data as
//! the chi-square law says.

mod pack_common;

use azlin_content::pack::{
    check_header, check_new_pack, randomness_check, AddOutcome, NotSealed, PackBuilder,
    PackGeometry, PackHeader, Padding, CHI_SQUARE_MAX, MIN_SAMPLE, SAMPLE_LEN,
};
use azlin_content::{ChunkId, PackClass, PackLen, SealedChunk};
use azlin_keys::SeededEntropy;
use pack_common::{
    build, chi2_quantile, chi2_upper_tail, chi_square_of, chunk, chunk_of, k_chunk, scaled,
    seal_key, small_geometry, space, stream, SplitMix64,
};

/// The first bytes a node holds of a pack: the header and up to 4096 bytes after it.
fn first_bytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.len().min(24 + SAMPLE_LEN)]
}

#[test]
fn c10_node_check_accepts_every_real_pack() {
    let sp = space(61);
    let k = k_chunk(62);
    let g = small_geometry();
    // A pool of real chunk blobs: 500 tiny ones (1 to 8 plaintext bytes: blobs of 42 to 49 bytes, a clear
    // codec byte every 42 to 49 bytes), then 1,000 of 1 to 9,000 bytes.
    let mut rng = SplitMix64::new(10);
    let mut pool: Vec<(ChunkId, SealedChunk)> = Vec::new();
    for i in 0..1_500u64 {
        // Every chunk of the pool has a plaintext of its own: equal plaintexts are one chunk, and a
        // chunk twice in one pack is refused.
        pool.push(if i < 256 {
            // One plaintext byte: a blob of 42 bytes, the shortest there is (256 of them).
            chunk_of(&sp, &k, &[i as u8])
        } else if i < 500 {
            // Two plaintext bytes: blobs of 43 bytes.
            chunk_of(&sp, &k, &(i as u16).to_le_bytes())
        } else {
            let plain_len = match rng.below(10) {
                0..=3 => 8 + rng.below(33) as usize,
                4..=6 => 41 + rng.below(360) as usize,
                7 | 8 => 400 + rng.below(2_600) as usize,
                _ => 3_000 + rng.below(6_000) as usize,
            };
            chunk(&sp, &k, 5_000 + i, plain_len)
        });
    }

    // Packs of tiny chunks hold up to 200 lines: 90 to 120 blobs of 42 to 49 bytes fill the 4 KiB sample
    // with about 90 clear codec bytes, the hardest case for the statistic.
    let tiny_geometry = PackGeometry {
        max_chunks: 200,
        ..g
    };
    let packs = 5_000usize;
    let mut refused = 0usize;
    let mut header_only = 0usize;
    let mut tested = 0usize;
    let mut max_x2 = 0.0f64;
    let mut max_at = String::new();
    let mut tiny_heavy = 0usize;
    for n in 0..packs {
        // 30 % of the packs are made of the tiny chunks only.
        let tiny = rng.below(10) < 3;
        let class = PackClass::ALL[rng.below(3) as usize];
        let padding = [Padding::None, Padding::Buckets, Padding::Full][rng.below(3) as usize];
        let last = rng.below(2) == 0;
        let key = seal_key(1 + rng.below(5) as u32, 0x21);
        let (geometry, start, wanted) = if tiny {
            (
                tiny_geometry,
                rng.below(380) as usize,
                90 + rng.below(30) as usize,
            )
        } else {
            (
                g,
                500 + rng.below(1_000) as usize,
                1 + rng.below(30) as usize,
            )
        };
        let mut b = PackBuilder::new(sp, class, key, geometry, padding);
        for j in 0..wanted {
            let (id, blob) = &pool[(start + j) % pool.len()];
            match b.add(*id, blob).expect("adds") {
                AddOutcome::Fits => {}
                AddOutcome::Full | AddOutcome::TooBigForAnyPack => break,
            }
        }
        if b.is_empty() {
            continue;
        }
        let pack = b
            .finish(last, &mut SeededEntropy::from_u64(n as u64))
            .expect("finishes");
        let first = first_bytes(pack.bytes());
        let after_header = pack.bytes().len() - 24;
        match check_new_pack(first, pack.len()) {
            Ok(header) => {
                assert_eq!(header.class, class);
            }
            Err(e) => {
                refused += 1;
                println!(
                    "c10: pack {n} refused: {e:?} ({} bytes)",
                    pack.bytes().len()
                );
            }
        }
        if after_header < MIN_SAMPLE {
            header_only += 1;
        } else {
            tested += 1;
            let x2 = chi_square_of(&first[24..]);
            if x2 > max_x2 {
                max_x2 = x2;
                max_at = format!("pack {n}, {} bytes, {class:?}", pack.bytes().len());
            }
            if tiny && x2 > 500.0 {
                tiny_heavy += 1;
            }
        }
    }
    println!(
        "c10: {packs} packs, false refusals {refused}; {tested} tested by the statistic, {header_only} too short to test; the highest X2 seen was {max_x2:.1} ({max_at}) against the threshold {CHI_SQUARE_MAX}; {tiny_heavy} packs of tiny chunks scored above 500"
    );
    assert_eq!(refused, 0, "false refusals: {refused}");
    assert!(tested > 3_000, "{tested} packs reached the statistic");
    assert!(
        max_x2 < f64::from(CHI_SQUARE_MAX) / 1.5,
        "the worst real pack scored {max_x2}"
    );
    assert!(
        tiny_heavy > 100,
        "the tiny-chunk packs did stress the statistic ({tiny_heavy})"
    );
}

/// English text, as bytes, `len` of them.
fn english(len: usize) -> Vec<u8> {
    const TEXT: &str = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. How vexingly quick daft zebras jump! Sphinx of black quartz, judge my vow. We promptly judged antique ivory buckles for the next prize. ";
    TEXT.bytes().cycle().take(len).collect()
}

/// JSON records, `len` bytes.
fn json(len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0u32;
    while out.len() < len {
        out.extend_from_slice(
            format!(
                "{{\"id\": {i}, \"name\": \"item number {i}\", \"tags\": [\"alpha\", \"beta\"], \"price\": {}.{:02}}},\n",
                i * 7 % 100,
                i % 100
            )
            .as_bytes(),
        );
        i += 1;
    }
    out.truncate(len);
    out
}

/// A real pack of one 6,000-byte chunk, whose bytes after the header are overwritten by `pattern`.
fn overwritten(pattern: &[u8]) -> (Vec<u8>, PackLen) {
    let sp = space(63);
    let k = k_chunk(64);
    let pack = build(
        &sp,
        PackClass::Content,
        2,
        0x21,
        small_geometry(),
        Padding::None,
        true,
        &[chunk(&sp, &k, 1, 6_000)],
        1,
    );
    let mut bytes = pack.bytes().to_vec();
    assert!(bytes.len() > 24 + SAMPLE_LEN);
    bytes[24..24 + SAMPLE_LEN].copy_from_slice(&pattern[..SAMPLE_LEN]);
    (bytes, pack.len())
}

#[test]
fn c11_node_check_refuses_what_is_not_sealed() {
    // The real pack passes, so every refusal below is the pattern's.
    let sp = space(63);
    let k = k_chunk(64);
    let real = build(
        &sp,
        PackClass::Content,
        2,
        0x21,
        small_geometry(),
        Padding::None,
        true,
        &[chunk(&sp, &k, 1, 6_000)],
        1,
    );
    assert!(check_new_pack(first_bytes(real.bytes()), real.len()).is_ok());

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    png.extend_from_slice(&english(SAMPLE_LEN));
    let mut elf = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
    elf.extend_from_slice(&json(SAMPLE_LEN));
    let block_64: Vec<u8> = stream(7, 64).into_iter().cycle().take(SAMPLE_LEN).collect();
    let block_256: Vec<u8> = stream(8, 256)
        .into_iter()
        .cycle()
        .take(SAMPLE_LEN)
        .collect();
    // Random bytes that use only the first 128 (or 160) byte values: ASCII-only, as binary.
    let only = |values: u64, seed: u64| -> Vec<u8> {
        let mut rng = SplitMix64::new(seed);
        (0..SAMPLE_LEN).map(|_| rng.below(values) as u8).collect()
    };
    let patterns = [
        ("zero fill", vec![0; SAMPLE_LEN]),
        ("one repeated byte", vec![0x41; SAMPLE_LEN]),
        ("0xff fill", vec![0xff; SAMPLE_LEN]),
        ("English text", english(SAMPLE_LEN)),
        ("JSON", json(SAMPLE_LEN)),
        ("a PNG header and an ASCII body", png),
        ("an ELF-like header and a JSON body", elf),
        ("a 64-byte random block, repeated", block_64),
        ("a 256-byte random block, repeated", block_256),
        ("random bytes below 128 (7-bit data)", only(128, 1)),
        ("random bytes below 160", only(160, 2)),
        (
            "an alphabet of 16 symbols",
            (0..SAMPLE_LEN)
                .map(|i| b"0123456789abcdef"[i * 7 % 16])
                .collect(),
        ),
    ];
    for (name, pattern) in &patterns {
        let (bytes, total) = overwritten(pattern);
        let x2 = chi_square_of(&bytes[24..24 + SAMPLE_LEN]);
        println!("c11: {name}: X2 {x2:.0}");
        assert_eq!(
            check_new_pack(first_bytes(&bytes), total),
            Err(NotSealed::NotRandom),
            "{name}"
        );
        assert_eq!(
            randomness_check(&bytes[24..24 + SAMPLE_LEN]),
            Err(NotSealed::NotRandom),
            "{name}"
        );
        assert!(x2 > f64::from(CHI_SQUARE_MAX), "{name}: X2 {x2}");
    }

    // A pack whose body IS plaintext, after a header and a trailer the builder made: a client that
    // forgot to encrypt.
    for (name, plain) in [("English text", english(6_000)), ("JSON", json(6_000))] {
        let blob = SealedChunk::from_stored(&plain).expect("a blob-shaped byte string");
        let mut b = PackBuilder::new(
            sp,
            PackClass::Content,
            seal_key(2, 0x21),
            small_geometry(),
            Padding::None,
        );
        assert_eq!(
            b.add(ChunkId::from_bytes([9; 16]), &blob),
            Ok(AddOutcome::Fits)
        );
        let pack = b
            .finish(true, &mut SeededEntropy::from_u64(3))
            .expect("finishes");
        assert_eq!(
            check_new_pack(first_bytes(pack.bytes()), pack.len()),
            Err(NotSealed::NotRandom),
            "a pack whose body is {name}"
        );
    }

    // Data that looks random passes: zstd (a frame magic and bytes a histogram cannot tell from
    // ciphertext), and data that uses 224 of the 256 byte values. Documented: "the check catches
    // mistakes, not malice".
    let mut zstd_like = vec![0x28, 0xb5, 0x2f, 0xfd];
    zstd_like.extend_from_slice(&stream(9, SAMPLE_LEN));
    for (name, pattern) in [
        ("zstd-like data", zstd_like),
        ("224 byte values", only(224, 3)),
    ] {
        let (bytes, total) = overwritten(&pattern);
        assert!(
            check_new_pack(first_bytes(&bytes), total).is_ok(),
            "{name} passes"
        );
    }

    // A header that is not a pack's: the node says which of the three facts it can name.
    let valid = real.bytes().to_vec();
    let with = |edit: &dyn Fn(&mut Vec<u8>)| {
        let mut b = valid.clone();
        edit(&mut b);
        b
    };
    let total = real.len();
    for (name, bytes, want) in [
        (
            "a random pack with the magic of another version (AZP1)",
            with(&|b| b[3] = b'1'),
            NotSealed::BadMagic,
        ),
        (
            "a random pack of version 2",
            with(&|b| b[4] = 2),
            NotSealed::UnsupportedVersion(2),
        ),
        (
            "a random pack of version 0",
            with(&|b| b[4] = 0),
            NotSealed::UnsupportedVersion(0),
        ),
        (
            "a class byte of 3",
            with(&|b| b[5] = 3),
            NotSealed::BadClass(3),
        ),
        (
            "reserved bytes that are not zero",
            with(&|b| b[6] = 9),
            NotSealed::SizesInconsistent,
        ),
        (
            "a body longer than the pack",
            with(&|b| b[16..20].copy_from_slice(&u32::MAX.to_le_bytes())),
            NotSealed::SizesInconsistent,
        ),
        (
            "no chunks",
            with(&|b| b[12..16].fill(0)),
            NotSealed::SizesInconsistent,
        ),
    ] {
        assert_eq!(
            check_new_pack(first_bytes(&bytes), total),
            Err(want),
            "{name}"
        );
        assert_eq!(
            check_header(first_bytes(&bytes), total),
            Err(want),
            "{name}"
        );
    }
    // The same random bytes under a header that is right: accepted.
    assert!(check_header(first_bytes(&valid), total).is_ok());
    // A pack declared longer than the longest pack.
    assert_eq!(
        check_new_pack(
            first_bytes(&valid),
            PackLen::new(azlin_content::pack::HARD_MAX.get() + 1)
        ),
        Err(NotSealed::SizesInconsistent)
    );

    // Short inputs. Fewer than 24 bytes: no header. Fewer bytes than the sample the declared length makes:
    // the caller has not received enough yet.
    assert_eq!(
        check_new_pack(&valid[..23], total),
        Err(NotSealed::TooShort)
    );
    assert_eq!(
        check_new_pack(&valid[..100], total),
        Err(NotSealed::TooShort)
    );
    assert_eq!(check_new_pack(&[], total), Err(NotSealed::TooShort));
    // `randomness_check` itself: 511 bytes are too few to judge, 512 are judged.
    assert_eq!(
        randomness_check(&[0u8; MIN_SAMPLE - 1]),
        Err(NotSealed::TooShort)
    );
    assert_eq!(
        randomness_check(&[0u8; MIN_SAMPLE]),
        Err(NotSealed::NotRandom)
    );
    assert_eq!(randomness_check(&stream(1, MIN_SAMPLE)), Ok(()));
    assert_eq!(randomness_check(&[]), Err(NotSealed::TooShort));
    // More than the sample is not read: zeros after the first 4096 bytes change nothing.
    let mut long = stream(2, SAMPLE_LEN);
    long.extend_from_slice(&[0u8; 100_000]);
    assert_eq!(randomness_check(&long), Ok(()));
    // A pack too short to test (fewer than 512 bytes after the header) is accepted on its header alone:
    // documented, DRIFT X-DELTA-W1S14C-10.
    let tiny = build(
        &sp,
        PackClass::Content,
        2,
        0x21,
        small_geometry(),
        Padding::None,
        true,
        &[chunk(&sp, &k, 2, 1)],
        2,
    );
    assert!(
        tiny.bytes().len() < 24 + MIN_SAMPLE,
        "{} bytes",
        tiny.bytes().len()
    );
    assert!(check_new_pack(tiny.bytes(), tiny.len()).is_ok());
    let mut zeroed = tiny.bytes().to_vec();
    zeroed[24..].fill(0);
    assert!(
        check_new_pack(&zeroed, tiny.len()).is_ok(),
        "too short to test, so the header alone decides"
    );
    let header = check_new_pack(tiny.bytes(), tiny.len()).expect("accepted");
    assert_eq!(
        header,
        PackHeader::parse(tiny.bytes(), tiny.len()).expect("a header")
    );
}

#[test]
fn c12_randomness_check_threshold_is_derived() {
    assert_eq!(SAMPLE_LEN, 4096);
    assert_eq!(MIN_SAMPLE, 512);

    // The tail of chi-square(255) at 1e-12, found with the test's own incomplete-gamma routine.
    let t0 = chi2_quantile(1e-12, 255.0);
    assert!((t0 - 447.514).abs() < 0.01, "T0 = {t0}");
    let tail = chi2_upper_tail(t0, 255.0);
    assert!(
        (tail / 1e-12 - 1.0).abs() < 1e-6,
        "the tail at T0 is {tail}"
    );
    // Known points of the law, so that the routine itself is held to something outside this file.
    assert!(
        (chi2_upper_tail(255.0, 255.0) - 0.4882).abs() < 0.0005,
        "the median is just under the mean"
    );
    assert!((chi2_upper_tail(300.0, 255.0) - 0.027_73).abs() < 0.000_05);
    assert!((chi2_upper_tail(330.0, 255.0) / 1.0673e-3 - 1.0).abs() < 0.001);
    assert!((chi2_upper_tail(400.0, 255.0) / 1.660_0e-8 - 1.0).abs() < 0.001);

    // The widening for the clear codec bytes of up to 100 blobs in the sample (one per 41 bytes at least).
    let framing = (SAMPLE_LEN as f64 / 41.0).ceil();
    assert_eq!(framing, 100.0);
    let expected = SAMPLE_LEN as f64 / 256.0;
    let t = (t0.sqrt() + framing / expected.sqrt() * (1.0 + 1.0 / 16.0)).powi(2);
    assert!((t - 2276.92).abs() < 0.01, "T = {t}");
    assert_eq!(
        CHI_SQUARE_MAX,
        t.ceil() as u32,
        "the constant is the derived value, rounded up"
    );
    assert_eq!(CHI_SQUARE_MAX, 2277);

    // Random 4 KiB samples: no refusal, the statistic is the chi-square law (mean 255, sd 22.6), and the
    // integer decision of the crate is the float statistic of this file compared with the threshold.
    let n = scaled("c12 samples", 200_000, 20_000);
    let mut rng = SplitMix64::new(12);
    let mut buffer = vec![0u8; SAMPLE_LEN];
    let (mut sum, mut sum_sq, mut max) = (0.0f64, 0.0f64, 0.0f64);
    let mut above_330 = 0usize;
    for i in 0..n {
        rng.fill(&mut buffer);
        let x2 = chi_square_of(&buffer);
        sum += x2;
        sum_sq += x2 * x2;
        max = max.max(x2);
        above_330 += usize::from(x2 > 330.0);
        assert_eq!(
            randomness_check(&buffer),
            Ok(()),
            "random sample {i} (X2 {x2:.1}) was refused"
        );
    }
    let mean = sum / n as f64;
    let sd = (sum_sq / n as f64 - mean * mean).sqrt();
    let want = n as f64 * chi2_upper_tail(330.0, 255.0);
    println!("c12: {n} random samples, no refusal; X2 mean {mean:.2} sd {sd:.2} max {max:.1}; above 330: {above_330} (the law says {want:.1})");
    assert!((mean - 255.0).abs() < 1.0, "mean {mean}");
    assert!((sd - 22.58).abs() < 1.0, "sd {sd}");
    assert!(
        (above_330 as f64 - want).abs() <= 6.0 * want.sqrt() + 3.0,
        "{above_330} samples above 330, the law says {want}"
    );

    // The worst legitimate pack: a clear codec byte (0) at the start of every 41-byte blob of the sample.
    let m = scaled("c12 framed samples", 50_000, 5_000);
    let mut worst = 0.0f64;
    for _ in 0..m {
        rng.fill(&mut buffer);
        for at in (0..SAMPLE_LEN).step_by(41) {
            buffer[at] = 0;
        }
        let x2 = chi_square_of(&buffer);
        worst = worst.max(x2);
        assert_eq!(
            randomness_check(&buffer),
            Ok(()),
            "a real pack of tiny chunks was refused (X2 {x2:.1})"
        );
    }
    println!("c12: {m} framed samples (100 clear bytes each), highest X2 {worst:.1} against {CHI_SQUARE_MAX}");
    assert!(worst < f64::from(CHI_SQUARE_MAX) / 1.5);
}
