//! C2, C3, C6 and the chunker's part of C18: the sizes of the chunks, the equality of the streaming
//! and the one-shot chunker, the bounded memory of the stream, and that odd piece sizes never panic.

mod azl2_common;

use std::ops::Range;

use azl2_common::{
    full_scale, public_reference_table, scaled, stream, SplitMix64, LOOSE, MAX, MIB, MIN,
};
use azlin_content::{Chunker, GearTable};
use zeroize::Zeroizing;

fn chunker() -> Chunker {
    Chunker::new(GearTable::public_v1())
}

/// The chunks of `ranges` are contiguous, non-empty, cover `data_len` bytes, none is above `MAX`
/// and none but the last is below `MIN`.
fn check_chunking(data_len: usize, ranges: &[Range<usize>]) {
    let mut at = 0usize;
    for (i, r) in ranges.iter().enumerate() {
        assert_eq!(r.start, at, "chunk {i} starts where the last one ended");
        assert!(r.end > r.start, "chunk {i} is empty");
        assert!(
            r.len() <= MAX,
            "chunk {i} is {} bytes, above the maximum",
            r.len()
        );
        if i + 1 < ranges.len() {
            assert!(
                r.len() >= MIN,
                "chunk {i} is {} bytes, below the minimum",
                r.len()
            );
        }
        at = r.end;
    }
    assert_eq!(at, data_len, "the chunks cover the input");
}

#[test]
fn c2_chunk_sizes_respect_min_and_max() {
    let c = chunker();
    assert!(c.cut(&[]).is_empty(), "empty input: zero chunks");

    // Random data around every boundary of the rule: the minimum, the maximum and a few between.
    let lengths = [
        1,
        2,
        255,
        MIN - 1,
        MIN,
        MIN + 1,
        MIN + 2,
        MIN + 63,
        MIN + 64,
        MIN + 65,
        2 * MIN,
        MIB,
        3 * MIB + 5,
        MAX - 1,
        MAX,
        MAX + 1,
        MAX + MIN,
        2 * MAX + 17,
    ];
    for (i, len) in lengths.into_iter().enumerate() {
        let data = stream(100 + i as u64, len);
        let ranges = c.cut(&data);
        check_chunking(len, &ranges);
        if len <= MIN {
            let whole = 0..len;
            assert_eq!(
                ranges,
                vec![whole],
                "a stream of {len} bytes (at most the minimum) is one chunk"
            );
        }
    }

    // Constant data never cuts (its fingerprint is the same at every byte, and for all but one in
    // 2^18 byte values that is not a cut), so every chunk but the last is cut by the maximum alone.
    let gear = public_reference_table();
    let byte = (0..=255u8)
        .find(|&b| gear[usize::from(b)].wrapping_neg() & LOOSE != 0)
        .expect("a byte whose constant fingerprint is no cut");
    let one_maximum = 0..MAX;
    for (len, want) in [
        (MAX, vec![one_maximum]),
        (MAX + 1, vec![0..MAX, MAX..MAX + 1]),
        (
            3 * MAX + 5,
            vec![0..MAX, MAX..2 * MAX, 2 * MAX..3 * MAX, 3 * MAX..3 * MAX + 5],
        ),
    ] {
        let data = vec![byte; len];
        assert_eq!(c.cut(&data), want, "{len} bytes of the byte {byte}");
    }
}

/// The chunks a stream makes of `data` fed in pieces of the sizes `next_piece` gives.
fn streamed(
    chunker: &Chunker,
    data: &[u8],
    mut next_piece: impl FnMut() -> usize,
) -> Vec<Zeroizing<Vec<u8>>> {
    let mut stream = chunker.stream();
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < data.len() {
        let n = next_piece().max(1).min(data.len() - at);
        stream.feed(&data[at..at + n], &mut out);
        at += n;
    }
    stream.finish(&mut out);
    out
}

fn assert_same(data: &[u8], ranges: &[Range<usize>], chunks: &[Zeroizing<Vec<u8>>], label: &str) {
    assert_eq!(chunks.len(), ranges.len(), "{label}: the number of chunks");
    for (i, (r, chunk)) in ranges.iter().zip(chunks).enumerate() {
        assert!(
            chunk.as_slice() == &data[r.clone()],
            "{label}: chunk {i} differs"
        );
    }
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    assert_eq!(total, data.len(), "{label}: the chunks hold every byte");
    let mut joined = Vec::with_capacity(data.len());
    for chunk in chunks {
        joined.extend_from_slice(chunk);
    }
    assert!(joined == data, "{label}: the concatenation is the input");
}

#[test]
fn c3_streaming_equals_one_shot() {
    let c = chunker();
    let big = if full_scale() { 9 * MIB } else { 5 * MIB };
    let sizes = [0, 1, 100, MIN - 1, MIN, MIN + 1, 700_000, 3 * MIB, big];
    for (i, len) in sizes.into_iter().enumerate() {
        let data = stream(300 + i as u64, len);
        let ranges = c.cut(&data);
        check_chunking(len, &ranges);

        // The whole input in one piece.
        assert_same(
            &data,
            &ranges,
            &streamed(&c, &data, || usize::MAX),
            &format!("{len}: one piece"),
        );

        // Pieces of one byte (on the inputs where that is a reasonable number of calls).
        if len <= 700_000 {
            assert_same(
                &data,
                &ranges,
                &streamed(&c, &data, || 1),
                &format!("{len}: one byte at a time"),
            );
        }

        // Random pieces of 1 to 5,000 bytes, and of 1 byte to 2 MiB.
        let mut small = SplitMix64::new(len as u64 + 1);
        assert_same(
            &data,
            &ranges,
            &streamed(&c, &data, || 1 + small.below(5_000) as usize),
            &format!("{len}: pieces of 1 to 5000"),
        );
        let mut large = SplitMix64::new(len as u64 + 2);
        assert_same(
            &data,
            &ranges,
            &streamed(&c, &data, || 1 + large.below(2 * MIB as u64) as usize),
            &format!("{len}: pieces of 1 byte to 2 MiB"),
        );

        // Pieces of exactly the maximum and one more (the edge of the buffer's room).
        for piece in [MAX, MAX + 1, MIN, MIN - 1] {
            assert_same(
                &data,
                &ranges,
                &streamed(&c, &data, || piece),
                &format!("{len}: pieces of {piece}"),
            );
        }
    }
}

#[test]
fn c6_stream_chunker_memory_is_bounded() {
    let total = scaled("c6 bytes fed", 4096 * MIB, 192 * MIB);
    let c = chunker();
    let mut stream = c.stream();
    let mut rng = SplitMix64::new(6);
    let mut out: Vec<Zeroizing<Vec<u8>>> = Vec::new();
    let mut piece = vec![0u8; 3 * MIB];
    let (mut hash_in, mut hash_out) = (blake3::Hasher::new(), blake3::Hasher::new());
    let (mut fed, mut chunks, mut chunk_bytes, mut largest_capacity) =
        (0usize, 0usize, 0usize, 0usize);
    while fed < total {
        let n = (1 + rng.below(3 * MIB as u64) as usize).min(total - fed);
        rng.fill(&mut piece[..n]);
        hash_in.update(&piece[..n]);
        stream.feed(&piece[..n], &mut out);
        fed += n;
        // The memory the stream holds never passes the maximum chunk, whatever the piece.
        assert!(
            stream.buffered_len() <= MAX,
            "buffered {} bytes",
            stream.buffered_len()
        );
        assert!(
            stream.buffered_capacity() <= MAX,
            "buffer capacity {} bytes",
            stream.buffered_capacity()
        );
        largest_capacity = largest_capacity.max(stream.buffered_capacity());
        for chunk in out.drain(..) {
            assert!(
                (MIN..=MAX).contains(&chunk.len()),
                "a cut chunk of {} bytes",
                chunk.len()
            );
            hash_out.update(&chunk);
            chunks += 1;
            chunk_bytes += chunk.len();
        }
    }
    stream.finish(&mut out);
    for chunk in out.drain(..) {
        assert!(
            (1..=MAX).contains(&chunk.len()),
            "the last chunk of {} bytes",
            chunk.len()
        );
        hash_out.update(&chunk);
        chunks += 1;
        chunk_bytes += chunk.len();
    }
    assert_eq!(chunk_bytes, total, "every byte fed came out in a chunk");
    assert!(
        hash_in.finalize() == hash_out.finalize(),
        "the chunks, in order, are the input"
    );
    println!(
        "c6: {total} bytes fed in pieces of up to 3 MiB, {chunks} chunks, the largest buffer capacity {largest_capacity} bytes (maximum chunk {MAX}); {}",
        if full_scale() { "full scale" } else { "debug scale" }
    );
}

#[test]
fn c18_chunker_odd_piece_sizes_never_panic() {
    let c = chunker();
    let mut rng = SplitMix64::new(18);
    // 2,000 small inputs, pieces of 0 to 99 bytes (zero-length pieces included): the buffering path.
    for case in 0..2_000u64 {
        let len = rng.below(4_096) as usize;
        let data = stream(1_000 + case, len);
        let mut sc = c.stream();
        let mut out = Vec::new();
        let mut at = 0;
        while at < len {
            let n = (rng.below(100) as usize).min(len - at);
            sc.feed(&data[at..at + n], &mut out);
            at += n;
        }
        sc.feed(&[], &mut out);
        sc.finish(&mut out);
        let total: usize = out.iter().map(|c| c.len()).sum();
        assert_eq!(total, len, "case {case}");
        assert_eq!(
            c.cut(&data).len(),
            out.len(),
            "case {case}: below the minimum it is one chunk or none"
        );
    }
    // A few larger inputs with piece sizes around the powers of two and the limits.
    let large = if full_scale() { 6 * MIB } else { 2 * MIB };
    let data = stream(1_800, large);
    let ranges = c.cut(&data);
    for piece in [
        1usize,
        2,
        3,
        7,
        4095,
        4096,
        4097,
        65_535,
        65_537,
        MIN + 1,
        MAX - 1,
    ] {
        if piece == 1 && !full_scale() {
            continue; // one-byte pieces over megabytes: C3 does it on 700,000 bytes
        }
        assert_same(
            &data,
            &ranges,
            &streamed(&c, &data, || piece),
            &format!("pieces of {piece}"),
        );
    }
}
