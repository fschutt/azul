//! The shared vocabulary of the skeleton (ids, units, errors, codec table, chunk identity): the
//! behaviour the later lanes build on, pinned so that a change is a red test and not a silent
//! drift. The vocabulary is `ids.rs`, `units.rs`, `error.rs`, `codec.rs` and the types of
//! `chunk.rs`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use azlin_content::limits::{BLOB_OVERHEAD, MAX_BLOB, MAX_CHUNK};
use azlin_content::{
    identity, ChunkError, ChunkId, ChunkLen, ChunkMeta, ChunkRef, Codec, CodecError,
    CompressPolicy, Compressor, FileSize, FileTime, Full, KChunk, PackClass, PackEntry, PackId,
    PackLen, PackName, PackOffset, Percent, SealedChunk, StoredOnly,
};
use azlin_keys::{IdError, Secret};

fn k_chunk(byte: u8) -> KChunk {
    KChunk::from_secret(Secret::from_bytes([byte; 32]))
}

#[test]
fn ids_print_and_parse_as_strict_lowercase_hex() {
    let chunk = ChunkId::from_bytes(core::array::from_fn(|i| i as u8));
    assert_eq!(chunk.to_string(), "000102030405060708090a0b0c0d0e0f");
    assert_eq!(chunk.to_string().parse::<ChunkId>(), Ok(chunk));
    assert_eq!(
        format!("{chunk:?}"),
        "ChunkId(000102030405060708090a0b0c0d0e0f)"
    );

    let pack = PackId::from_bytes([0xfe; 16]);
    assert_eq!(pack.to_string(), "fefefefefefefefefefefefefefefefe");
    assert_eq!(pack.to_string().parse::<PackId>(), Ok(pack));

    let name = PackName::from_bytes([0x0f; 32]);
    assert_eq!(name.to_string(), "0f".repeat(32));
    assert_eq!(name.to_string().parse::<PackName>(), Ok(name));
    assert_eq!(PackName::LEN, 32);
    assert_eq!(ChunkId::LEN, 16);
    assert_eq!(PackId::LEN, 16);

    // Strict: the exact length, lowercase, hex digits and nothing else.
    assert_eq!("".parse::<ChunkId>(), Err(IdError::Length));
    assert_eq!("00".repeat(15).parse::<ChunkId>(), Err(IdError::Length));
    assert_eq!("00".repeat(17).parse::<ChunkId>(), Err(IdError::Length));
    assert_eq!("AB".repeat(16).parse::<ChunkId>(), Err(IdError::BadDigit));
    assert_eq!("0x".repeat(16).parse::<ChunkId>(), Err(IdError::BadDigit));
    assert_eq!(" 0".repeat(16).parse::<ChunkId>(), Err(IdError::BadDigit));
    assert_eq!("gg".repeat(16).parse::<ChunkId>(), Err(IdError::BadDigit));
    assert_eq!("00".repeat(16).parse::<PackName>(), Err(IdError::Length));
}

#[test]
fn ids_are_ordered_and_hashable_for_the_index() {
    let a = ChunkId::from_bytes([1; 16]);
    let b = ChunkId::from_bytes([2; 16]);
    assert!(a < b);
    let by_id: BTreeMap<ChunkId, u32> = [(b, 2), (a, 1)].into_iter().collect();
    assert_eq!(by_id.keys().copied().collect::<Vec<_>>(), vec![a, b]);
    let set: HashSet<PackId> = [PackId::from_bytes([3; 16]), PackId::from_bytes([3; 16])]
        .into_iter()
        .collect();
    assert_eq!(set.len(), 1);
}

#[test]
fn units_are_never_zero_where_a_chunk_has_a_length_and_checked_everywhere() {
    // ChunkLen: 1 to MAX_BLOB.
    assert!(ChunkLen::new(0).is_none());
    assert_eq!(ChunkLen::new(1).map(ChunkLen::get), Some(1));
    assert_eq!(ChunkLen::new(MAX_BLOB.get()), Some(MAX_BLOB));
    assert!(ChunkLen::new(MAX_BLOB.get() + 1).is_none());
    assert!(ChunkLen::new(u32::MAX).is_none());
    assert!(ChunkLen::from_len(0).is_none());
    assert_eq!(ChunkLen::from_len(4096).map(ChunkLen::get), Some(4096));
    assert!(ChunkLen::from_len(usize::MAX).is_none());
    assert_eq!(MAX_CHUNK.as_usize(), 4 * 1024 * 1024);
    assert_eq!(ChunkLen::new(7).expect("in range").to_string(), "7");
    assert_eq!(
        format!("{:?}", ChunkLen::new(7).expect("in range")),
        "ChunkLen(7)"
    );

    // PackLen, PackOffset: any u32, sums checked.
    assert_eq!(PackLen::ZERO.get(), 0);
    assert_eq!(
        PackLen::new(10).checked_add(PackLen::new(5)),
        Some(PackLen::new(15))
    );
    assert_eq!(PackLen::new(u32::MAX).checked_add(PackLen::new(1)), None);
    assert_eq!(PackLen::new(10).checked_sub(PackLen::new(11)), None);
    assert_eq!(
        PackLen::new(10).checked_sub(PackLen::new(10)),
        Some(PackLen::ZERO)
    );
    let chunk = ChunkLen::new(1_000).expect("in range");
    assert_eq!(
        PackLen::new(24).checked_add_chunk(chunk),
        Some(PackLen::new(1_024))
    );
    assert_eq!(PackLen::new(u32::MAX).checked_add_chunk(chunk), None);
    assert_eq!(PackLen::from_len(5), Some(PackLen::new(5)));
    assert_eq!(PackLen::from_len(usize::MAX), None);
    assert_eq!(
        PackOffset::ZERO.checked_add(chunk),
        Some(PackOffset::new(1_000))
    );
    assert_eq!(PackOffset::new(u32::MAX).checked_add(chunk), None);
    assert!(PackOffset::new(1) < PackOffset::new(2));

    // FileSize: sums checked, typed.
    assert_eq!(
        FileSize::new(5).checked_add(FileSize::new(6)),
        Some(FileSize::new(11))
    );
    assert_eq!(FileSize::new(u64::MAX).checked_add(FileSize::new(1)), None);
    assert_eq!(
        FileSize::ZERO.checked_add_chunk(chunk),
        Some(FileSize::new(1_000))
    );
    assert_eq!(FileSize::new(u64::MAX).checked_add_chunk(chunk), None);

    // FileTime: before 1970 allowed, nanoseconds below a second, ordered as the time is.
    assert_eq!(FileTime::new(0, 1_000_000_000), None);
    assert_eq!(
        FileTime::new(-5, 999_999_999).map(FileTime::nanos),
        Some(999_999_999)
    );
    let a = FileTime::new(-1, 999_999_999).expect("a time before 1970");
    let b = FileTime::UNIX_EPOCH;
    let c = FileTime::new(0, 1).expect("a time");
    assert!(a < b && b < c);
    assert_eq!(a.secs(), -1);

    // Percent: 0 to 100.
    assert!(Percent::new(100).is_some() && Percent::new(0).is_some());
    assert!(Percent::new(101).is_none());
    assert_eq!(Percent::new(5).expect("a percentage").to_string(), "5%");
}

#[test]
fn full_is_key_material_and_keeps_to_the_rules_of_a_secret() {
    let full = Full::from_bytes([7; 32]);
    assert_eq!(format!("{full:?}"), "Full(***)");
    assert!(full.ct_eq(&full.duplicate()));
    assert!(full == full.duplicate());
    assert!(full != Full::from_bytes([8; 32]));
    assert_eq!(full.expose(), &[7u8; 32]);
    assert_eq!(Full::LEN, 32);
    assert!(Full::from_slice(&[1; 31]).is_none());
    assert!(Full::from_slice(&[1; 33]).is_none());
    let from_slice = Full::from_slice(&[7; 32]).expect("32 bytes");
    assert!(from_slice == full);
    assert!(Full::from_secret(Secret::from_bytes([7; 32])) == full);
    // The id of a chunk is derive_key("azlin chunk id v1", full)[0..16].
    let want = blake3::derive_key("azlin chunk id v1", &[7u8; 32]);
    assert_eq!(full.chunk_id().as_bytes(), &want[..16]);
    // A structure that holds one does not print it either.
    let meta_text = format!(
        "{:?}",
        ChunkRef {
            full: full.duplicate(),
            raw_len: ChunkLen::new(100).expect("in range"),
            pack: PackId::from_bytes([1; 16]),
            offset: PackOffset::new(24),
            stored_len: ChunkLen::new(141).expect("in range"),
        }
    );
    assert!(meta_text.contains("Full(***)"), "{meta_text}");
    assert!(!meta_text.contains("[7"), "{meta_text}");
    assert_eq!(format!("{:?}", k_chunk(1)), "KChunk(***)");
}

#[test]
fn identity_is_keyed_blake3_and_the_id_of_that_hash() {
    // Recomputed here with blake3 calls only, not with the lane's function.
    let k = [9u8; 32];
    for plaintext in [&b""[..], b"x", b"hello chunk", &[0u8; 10_000][..]] {
        let (full, id) = identity(&k_chunk(9), plaintext);
        let want_full = *blake3::keyed_hash(&k, plaintext).as_bytes();
        assert_eq!(full.expose(), &want_full);
        let want_id = blake3::derive_key("azlin chunk id v1", &want_full);
        assert_eq!(id.as_bytes(), &want_id[..16]);
        assert_eq!(id, full.chunk_id());
    }
    // Inside one space the same bytes are one chunk; two spaces never agree.
    let (full_a, id_a) = identity(&k_chunk(1), b"same");
    let (full_a2, id_a2) = identity(&k_chunk(1), b"same");
    let (full_b, id_b) = identity(&k_chunk(2), b"same");
    let (_, id_other) = identity(&k_chunk(1), b"other");
    assert!(full_a == full_a2 && id_a == id_a2);
    assert!(full_a != full_b && id_a != id_b);
    assert_ne!(id_a, id_other);
}

#[test]
fn chunk_ref_check_holds_the_numbers_to_what_a_chunk_can_be() {
    let make = |raw: u32, stored: u32, offset: u32| ChunkRef {
        full: Full::from_bytes([1; 32]),
        raw_len: ChunkLen::new(raw).expect("in range"),
        pack: PackId::from_bytes([2; 16]),
        offset: PackOffset::new(offset),
        stored_len: ChunkLen::new(stored).expect("in range"),
    };
    // A stored chunk: the blob is the plaintext and the framing.
    let framing = BLOB_OVERHEAD as u32;
    assert_eq!(make(1_000, 1_000 + framing, 24).check(), Ok(()));
    assert_eq!(make(1_000, 500, 24).check(), Ok(())); // compressed
    assert_eq!(make(1, 1 + framing, 0).check(), Ok(())); // one byte
    assert_eq!(
        make(MAX_CHUNK.get(), MAX_CHUNK.get() + framing, 0).check(),
        Ok(())
    );
    // The plaintext is above the largest chunk.
    assert_eq!(
        make(MAX_CHUNK.get() + 1, 2_000, 0).check(),
        Err(ChunkError::TooLong)
    );
    // The blob cannot even hold its framing and one stored byte.
    assert_eq!(make(1_000, framing, 0).check(), Err(ChunkError::Truncated));
    assert_eq!(make(1_000, 5, 0).check(), Err(ChunkError::Truncated));
    // The stored bytes are longer than the plaintext.
    assert_eq!(
        make(1_000, 1_000 + framing + 1, 0).check(),
        Err(ChunkError::LengthMismatch)
    );
    // The blob ends past a u32.
    assert_eq!(
        make(1_000, 600, u32::MAX - 100).check(),
        Err(ChunkError::TooLong)
    );
    assert_eq!(
        make(1_000, 600, u32::MAX - 600).end(),
        Some(PackOffset::new(u32::MAX))
    );
    // duplicate is a second owner, equal to the first.
    let one = make(1_000, 600, 24);
    let two = one.duplicate();
    assert_eq!(one, two);
    assert_eq!(one.id(), two.full.chunk_id());
}

#[test]
fn sealed_chunk_from_stored_checks_the_shape_and_nothing_more() {
    // The framing is 41 bytes: a shorter blob is truncated, a longer one than the largest is too long.
    assert_eq!(SealedChunk::from_stored(&[]), Err(ChunkError::Truncated));
    assert_eq!(
        SealedChunk::from_stored(&[0u8; BLOB_OVERHEAD - 1]),
        Err(ChunkError::Truncated)
    );
    let minimal = SealedChunk::from_stored(&[9u8; BLOB_OVERHEAD]).expect("the framing alone");
    assert_eq!(minimal.as_bytes().len(), BLOB_OVERHEAD);
    assert_eq!(minimal.len(), PackLen::new(BLOB_OVERHEAD as u32));
    assert_eq!(minimal.stored_len().as_usize(), BLOB_OVERHEAD);
    let largest = SealedChunk::from_stored(&vec![1u8; MAX_BLOB.as_usize()]).expect("the largest");
    assert_eq!(largest.stored_len(), MAX_BLOB);
    assert_eq!(
        SealedChunk::from_stored(&vec![1u8; MAX_BLOB.as_usize() + 1]),
        Err(ChunkError::TooLong)
    );
    // The codec byte is not interpreted here (an unknown one surfaces after authentication).
    assert!(SealedChunk::from_stored(&[255u8; 100]).is_ok());
    // The bytes come back as they went in, and the blob never prints them.
    let bytes: Vec<u8> = (0..=99u8).collect();
    let blob = SealedChunk::from_stored(&bytes).expect("a blob");
    assert_eq!(blob.as_bytes(), &bytes[..]);
    assert_eq!(blob.clone().into_bytes(), bytes);
    assert_eq!(format!("{blob:?}"), "SealedChunk(100 bytes)");
}

#[test]
fn pack_entries_and_classes() {
    for (class, byte) in PackClass::ALL.into_iter().zip(0u8..) {
        assert_eq!(class.byte(), byte);
        assert_eq!(PackClass::from_byte(byte), Some(class));
    }
    for byte in 3..=255u8 {
        assert_eq!(PackClass::from_byte(byte), None, "class byte {byte}");
    }
    let entry = PackEntry {
        id: ChunkId::from_bytes([1; 16]),
        offset: PackOffset::new(24),
        length: ChunkLen::new(100).expect("in range"),
    };
    assert_eq!(entry.end(), Some(PackOffset::new(124)));
    let last = PackEntry {
        offset: PackOffset::new(u32::MAX),
        ..entry
    };
    assert_eq!(last.end(), None);
    // The classes order as their bytes do.
    let classes: BTreeSet<PackClass> = PackClass::ALL.into_iter().collect();
    assert_eq!(classes.len(), 3);
}

#[test]
fn the_codec_table_is_the_byte_table_of_azul_storage() {
    for byte in 0..=255u8 {
        match byte {
            0 | 1 | 2 | 4 => {
                let codec = Codec::from_byte(byte).expect("a codec byte");
                assert_eq!(codec.byte(), byte);
            }
            other => assert_eq!(
                Codec::from_byte(byte),
                Err(CodecError::Unsupported(other)),
                "byte {byte} is refused by name"
            ),
        }
    }
    assert_eq!(Codec::Stored.byte(), 0);
    assert_eq!(Codec::Zstd.byte(), 1);
    assert_eq!(Codec::Brotli.byte(), 2);
    assert_eq!(Codec::ZstdDict.byte(), 4);
    assert_eq!(azlin_content::CODEC_JPEG_XL, 3, "3 stays free");
    assert!(CodecError::Unsupported(3).to_string().contains("reserved"));
    assert!(CodecError::Unsupported(200).to_string().contains("200"));
    assert!(ChunkError::BadCodec(7).to_string().contains('7'));
}

#[test]
fn the_five_percent_rule_is_exact() {
    let policy = CompressPolicy::plan();
    // Every raw length from 1 to 2,000 and every stored length around the boundary: the policy is
    // `stored * 100 <= raw * 95`, and nothing else.
    let zeros = vec![0u8; 2_010];
    for raw in 1..=2_000usize {
        for stored in 0..=raw + 3 {
            let want = stored * 100 <= raw * 95;
            assert_eq!(
                policy.keeps(&zeros[..stored], &zeros[..raw]),
                want,
                "stored {stored} of raw {raw}"
            );
        }
    }
    assert!(!policy.keeps(&[1], &[]));
}

#[test]
fn stored_only_never_compresses_and_reads_no_compressed_codec() {
    let c = StoredOnly;
    assert_eq!(
        c.compress(b"anything")
            .expect("never fails")
            .map(|(co, _)| co),
        None
    );
    let max = ChunkLen::new(10).expect("in range");
    for codec in [Codec::Zstd, Codec::Brotli, Codec::ZstdDict] {
        let result = c.decompress(codec, b"stream", max);
        assert_eq!(
            result.map(|v| v.len()),
            Err(CodecError::Unsupported(codec.byte()))
        );
    }
}

#[test]
fn chunk_meta_becomes_a_pointer_entry_without_copying_the_hash() {
    let (full, id) = identity(&k_chunk(4), b"some bytes");
    let meta = ChunkMeta {
        full: full.duplicate(),
        id,
        raw_len: ChunkLen::new(10).expect("in range"),
        stored_len: ChunkLen::new(51).expect("in range"),
        codec: Codec::Stored,
    };
    let copy = meta.duplicate();
    assert_eq!(meta, copy);
    let pack = PackId::from_bytes([5; 16]);
    let reference = meta.into_ref(pack, PackOffset::new(48));
    assert!(reference.full == full);
    assert_eq!(reference.id(), id);
    assert_eq!(reference.pack, pack);
    assert_eq!(reference.offset, PackOffset::new(48));
    assert_eq!(reference.raw_len.get(), 10);
    assert_eq!(reference.stored_len.get(), 51);
    assert_eq!(reference.check(), Ok(()));
}
