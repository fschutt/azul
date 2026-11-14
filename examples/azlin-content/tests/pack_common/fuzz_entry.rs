//! The entry points of the fuzz targets `pack_header` and `pack_trailer` (`fuzz/fuzz_targets/` include
//! this file by path) and of the seeded run of C15: the same code, so what the fuzzer is told to find,
//! the ordinary tests look for on a few thousand fixed inputs.
//!
//! It uses only the public API of the crate and the entropy double of `azlin-keys`, and holds these rules
//! against every input: nothing here may panic; a header that `PackHeader::parse` accepts is the 24 bytes
//! read and has a trailer range that ends at the declared length; `check_header` and `check_new_pack`
//! never accept what `parse` refuses; a pack the builder made reads back, and any single flipped bit,
//! cut or added byte of it is refused.
#![allow(dead_code)]

use azlin_content::pack::{
    check_header, check_new_pack, randomness_check, read_trailer, rebuild_entries, AddOutcome,
    PackBuilder, PackGeometry, PackHeader, Padding, SealKey,
};
use azlin_content::{ChunkId, PackClass, PackLen, SealedChunk};
use azlin_keys::{Epoch, Secret, SeededEntropy, SpaceId};

fn key() -> SealKey {
    SealKey {
        epoch: Epoch::FIRST,
        key: Secret::from_bytes([9; 32]),
    }
}

fn space() -> SpaceId {
    SpaceId::from_bytes([1; 16])
}

/// One input of `pack_header`: `declared length (4, little-endian) || bytes`. The bytes are the first
/// bytes of a pack of the declared length.
pub fn pack_header(data: &[u8]) {
    let Some((len_bytes, bytes)) = data.split_first_chunk::<4>() else {
        return;
    };
    let total = PackLen::new(u32::from_le_bytes(*len_bytes));
    let parsed = PackHeader::parse(bytes, total);
    let checked = check_header(bytes, total);
    let new_pack = check_new_pack(bytes, total);
    assert_eq!(
        parsed.is_ok(),
        checked.is_ok(),
        "parse and check_header agree"
    );
    if let (Ok(a), Ok(b)) = (&parsed, &checked) {
        assert_eq!(a, b, "the node's check returns the parsed header");
    }
    if new_pack.is_ok() {
        assert!(parsed.is_ok(), "check_new_pack accepted what parse refuses");
    }
    // Any bytes at all are a sample for the randomness test: it never panics.
    let _ = randomness_check(bytes);
    if let Ok(header) = parsed {
        assert_eq!(
            &header.to_bytes()[..],
            &bytes[..PackHeader::LEN],
            "a parsed header is the bytes that were read"
        );
        let trailer = header
            .trailer_range(total)
            .expect("a parsed header has a trailer range");
        assert_eq!(trailer.end.get(), total.get(), "the trailer ends the pack");
        assert!(trailer.start <= trailer.end);
        let body = header.body_range().expect("a parsed header has a body");
        assert!(
            body.end <= trailer.start,
            "the body ends before the trailer"
        );
    }
}

/// One input of `pack_trailer`: `selector (1) || bytes`. The bytes are tried as a whole pack, as a
/// trailer under a header made by hand, and as the content of a real pack that the builder makes and
/// that is then flipped, cut and extended.
pub fn pack_trailer(data: &[u8]) {
    let Some((selector, bytes)) = data.split_first() else {
        return;
    };
    let (space, key) = (space(), key());

    // 1. The bytes as a whole pack: never authentic, never a panic.
    let _ = rebuild_entries(bytes, &space, &key);

    // 2. The bytes as the trailer under a header the selector makes: `read_trailer` refuses them all.
    let header = PackHeader {
        class: PackClass::ALL[usize::from(*selector) % 3],
        key_epoch: Epoch::FIRST,
        chunk_count: 1 + u32::from(*selector % 3),
        body_len: PackLen::new(41 * (1 + u32::from(*selector % 3))),
        pad_len: PackLen::new(u32::from(*selector)),
    };
    let _ = read_trailer(&header, bytes, &space, &key);

    // 3. A real pack made of the bytes: blobs of 41 to 296 bytes, each led by a length byte.
    let geometry = PackGeometry {
        target: PackLen::new(2048),
        hard_max: PackLen::new(4096),
        pad_step: PackLen::new(256),
        max_chunks: 6,
    };
    let padding = [Padding::None, Padding::Buckets, Padding::Full][usize::from(*selector) % 3];
    let class = PackClass::ALL[usize::from(*selector / 3) % 3];
    let mut builder = PackBuilder::new(space, class, key.duplicate(), geometry, padding);
    let mut rest = bytes;
    let mut index = 0u8;
    while let Some((&len_byte, tail)) = rest.split_first() {
        let want = 41 + usize::from(len_byte);
        let Some((blob_bytes, after)) = tail.split_at_checked(want) else {
            break;
        };
        rest = after;
        index = index.wrapping_add(1);
        let Ok(blob) = SealedChunk::from_stored(blob_bytes) else {
            continue;
        };
        let mut id = [0u8; 16];
        id[0] = index;
        id[1] = len_byte;
        match builder.add(ChunkId::from_bytes(id), &blob) {
            Ok(AddOutcome::Fits) | Ok(AddOutcome::TooBigForAnyPack) | Err(_) => {}
            Ok(AddOutcome::Full) => break,
        }
    }
    if builder.is_empty() {
        return;
    }
    let mut entropy = SeededEntropy::from_u64(u64::from(*selector));
    let pack = builder
        .finish(selector.is_multiple_of(2), &mut entropy)
        .expect("a builder that holds a chunk finishes");
    let _ = check_new_pack(
        &pack.bytes()[..pack.bytes().len().min(24 + 4096)],
        pack.len(),
    );
    assert_eq!(
        rebuild_entries(pack.bytes(), &space, &key).expect("a pack the builder made reads back"),
        pack.entries(),
        "the lines read back are the lines written"
    );

    // Where to hit it: the first four bytes of the input say where, the fifth which bit.
    let at = usize::try_from(u32::from_le_bytes([
        bytes.first().copied().unwrap_or(0),
        bytes.get(1).copied().unwrap_or(0),
        bytes.get(2).copied().unwrap_or(0),
        bytes.get(3).copied().unwrap_or(0),
    ]))
    .unwrap_or(0)
        % pack.bytes().len();
    let bit = bytes.get(4).copied().unwrap_or(0) % 8;
    let mut flipped = pack.bytes().to_vec();
    flipped[at] ^= 1 << bit;
    assert!(
        rebuild_entries(&flipped, &space, &key).is_err(),
        "a pack with a flipped bit was accepted (byte {at}, bit {bit})"
    );
    let _ = check_new_pack(&flipped[..flipped.len().min(24 + 4096)], pack.len());
    assert!(
        rebuild_entries(&pack.bytes()[..at], &space, &key).is_err(),
        "a pack cut to {at} bytes was accepted"
    );
    let mut longer = pack.bytes().to_vec();
    longer.push(bytes.get(5).copied().unwrap_or(0));
    assert!(
        rebuild_entries(&longer, &space, &key).is_err(),
        "a pack with a byte added was accepted"
    );
}
