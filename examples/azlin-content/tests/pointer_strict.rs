//! C2 and C12: the strict reader. A pointer arrives from another device, another member or a hostile
//! server, so the reader refuses everything that is not the one deterministic encoding of a valid
//! pointer: every truncation, trailing bytes, every non-canonical form, a wrong type or length at every
//! field and at every leaf, a missing field, a duplicate or unsorted key, a `crit` it cannot honour; and it
//! never calls a newer format damage (version 3 and up is `Decoded::Newer`, whatever follows). C12: the
//! declared sizes and counts are bounded and every sum is checked: the largest numbers are errors, never
//! a wrap.

mod pointer_common;

use azlin_cbor::{DecodeError, Map, ShapeError, Value};
use azlin_content::pointer::{ChunkList, ChunkListRef, Decoded, Part, PointerError, PointerV2};
use azlin_content::{ChunkError, FileSize, FileTime};
use pointer_common::*;

fn full_signed() -> Vec<u8> {
    signed(full()).to_cbor().expect("writes").to_vec()
}

fn read(bytes: &[u8]) -> Result<Decoded, PointerError> {
    PointerV2::from_cbor(bytes, &OBJECT)
}

fn err_of(bytes: &[u8]) -> Option<PointerError> {
    read(bytes).err()
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

/// `bytes` with the first occurrence of `needle` replaced.
fn replace_first(bytes: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    let at = (0..=bytes.len() - needle.len())
        .find(|i| &bytes[*i..*i + needle.len()] == needle)
        .expect("the needle occurs");
    let mut out = bytes[..at].to_vec();
    out.extend_from_slice(with);
    out.extend_from_slice(&bytes[at + needle.len()..]);
    out
}

/// A map value `{1: name, 2: count, 3: total_raw}` for key 7.
fn list_ref_value(name: [u8; 32], count: u64, total: u64) -> Value {
    Value::Map(
        Map::from_entries(vec![
            (Value::Uint(1), Value::Bytes(name.to_vec())),
            (Value::Uint(2), Value::Uint(count)),
            (Value::Uint(3), Value::Uint(total)),
        ])
        .expect("a map"),
    )
}

// ---------------------------------------------------------------------------------------------
// C2
// ---------------------------------------------------------------------------------------------

#[test]
fn c2_strict_decoding_negatives_truncated_at_every_prefix() {
    for p in [full_with_unknown(), minimal_empty()] {
        let bytes = signed(p).to_cbor().expect("writes").to_vec();
        for n in 0..bytes.len() {
            assert_eq!(
                err_of(&bytes[..n]),
                Some(PointerError::Cbor(DecodeError::Truncated)),
                "the first {n} of {} bytes",
                bytes.len()
            );
        }
        assert!(matches!(read(&bytes), Ok(Decoded::Current(_))));
    }
}

#[test]
fn c2_extra_trailing_bytes_are_refused() {
    let bytes = full_signed();
    for tail in [vec![0u8], vec![0xff], vec![0xa0], bytes.clone()] {
        let mut longer = bytes.clone();
        longer.extend_from_slice(&tail);
        assert_eq!(
            err_of(&longer),
            Some(PointerError::Cbor(DecodeError::TrailingBytes))
        );
    }
}

#[test]
fn c2_non_canonical_encodings_are_refused() {
    let hex = concat!(
        "a8",
        "0102",
        "0200",
        "03821a6553f10000",
        "041901a4",
        "055820",
        "1010101010101010101010101010101010101010101010101010101010101010",
        "0780",
        "0850a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
        "0f1a6553f164",
    );
    let minimal = from_hex(hex);
    assert!(matches!(read(&minimal), Ok(Decoded::Current(_))));
    let cases: Vec<(&str, Vec<u8>, PointerError)> = vec![
        (
            "a non-shortest unsigned integer",
            replace_first(&minimal, &[0x02, 0x00, 0x03], &[0x02, 0x18, 0x00, 0x03]),
            PointerError::Cbor(DecodeError::NotShortest),
        ),
        (
            "a non-shortest integer of four bytes",
            replace_first(
                &minimal,
                &[0x02, 0x00, 0x03],
                &[0x02, 0x1a, 0, 0, 0, 0, 0x03],
            ),
            PointerError::Cbor(DecodeError::NotShortest),
        ),
        (
            "a non-shortest byte string length",
            replace_first(&minimal, &[0x58, 0x20], &[0x59, 0x00, 0x20]),
            PointerError::Cbor(DecodeError::NotShortest),
        ),
        (
            "a non-shortest array length",
            replace_first(&minimal, &[0x07, 0x80], &[0x07, 0x98, 0x00]),
            PointerError::Cbor(DecodeError::NotShortest),
        ),
        (
            "a non-shortest map length",
            replace_first(&minimal, &[0xa8], &[0xb8, 0x08]),
            PointerError::Cbor(DecodeError::NotShortest),
        ),
        (
            "an indefinite-length array",
            replace_first(&minimal, &[0x07, 0x80], &[0x07, 0x9f, 0xff]),
            PointerError::Cbor(DecodeError::IndefiniteLength),
        ),
        (
            "a float",
            replace_first(
                &minimal,
                &[0x02, 0x00, 0x03],
                &[0x02, 0xf9, 0x00, 0x00, 0x03],
            ),
            PointerError::Cbor(DecodeError::DisallowedType),
        ),
        (
            "a tag",
            replace_first(&minimal, &[0x02, 0x00, 0x03], &[0x02, 0xc1, 0x00, 0x03]),
            PointerError::Cbor(DecodeError::DisallowedType),
        ),
        (
            "a duplicate key",
            from_hex("a201020102"),
            PointerError::Cbor(DecodeError::DuplicateMapKey),
        ),
        (
            "unsorted keys",
            from_hex("a202000102"),
            PointerError::Cbor(DecodeError::UnsortedMapKeys),
        ),
        (
            "a duplicate key far from the first",
            // keys 1, 2, 3, 2: the repeated 2 is out of order and equal to an earlier key
            from_hex("a40102020003000200"),
            PointerError::Cbor(DecodeError::UnsortedMapKeys),
        ),
    ];
    for (what, bytes, expected) in cases {
        assert_eq!(err_of(&bytes), Some(expected), "{what}");
    }
}

#[test]
fn c2_not_a_map_with_unsigned_keys() {
    for (what, bytes) in [
        ("an unsigned integer", from_hex("00")),
        ("an array", from_hex("80")),
        ("a text string", from_hex("6178")),
        ("a map with a text key", from_hex("a1617800")),
        ("a map with a negative key", from_hex("a12000")),
    ] {
        assert_eq!(
            err_of(&bytes),
            Some(PointerError::Shape(ShapeError::NotAnIntMap)),
            "{what}"
        );
    }
    assert_eq!(
        err_of(&from_hex("a0")),
        Some(PointerError::Shape(ShapeError::Missing(1))),
        "an empty map has no version"
    );
}

#[test]
fn c2_wrong_type_per_top_level_field() {
    let bytes = full_signed();
    // Every key of the pointer: the error a text string where it does not belong gets. The keys read
    // through the typed accessors say `WrongType(key)`; the three whose value is a record say which
    // record.
    let table: Vec<(u64, PointerError)> = vec![
        (1, PointerError::Shape(ShapeError::WrongType(1))),
        (2, PointerError::Shape(ShapeError::WrongType(2))),
        (3, PointerError::BadShape(Part::Mtime)),
        (4, PointerError::Shape(ShapeError::WrongType(4))),
        (5, PointerError::Shape(ShapeError::WrongType(5))),
        (6, PointerError::BadShape(Part::Transform)),
        (7, PointerError::Shape(ShapeError::WrongType(7))),
        (8, PointerError::Shape(ShapeError::WrongType(8))),
        (9, PointerError::Shape(ShapeError::WrongType(9))),
        (10, PointerError::Shape(ShapeError::WrongType(10))),
        (11, PointerError::Shape(ShapeError::WrongType(11))),
        (12, PointerError::Shape(ShapeError::WrongType(12))),
        (13, PointerError::Shape(ShapeError::WrongType(13))),
        (14, PointerError::Shape(ShapeError::WrongType(14))),
        (15, PointerError::Shape(ShapeError::WrongType(15))),
    ];
    assert_eq!(table.len(), 15, "every key of the pointer");
    for (key, expected) in table {
        let wrong = with_entry(&bytes, key, Some(text("x")));
        assert_eq!(err_of(&wrong), Some(expected), "key {key}");
        // A null is a wrong type too (no field is nullable: an absent field is absent).
        let null = with_entry(&bytes, key, Some(Value::Null));
        assert!(err_of(&null).is_some(), "key {key} as null");
    }
}

#[test]
fn c2_wrong_length_fields() {
    let bytes = full_signed();
    for (key, len) in [
        (5u64, 31usize),
        (5, 33),
        (8, 15),
        (8, 17),
        (14, 63),
        (14, 65),
    ] {
        let wrong = with_entry(&bytes, key, Some(Value::Bytes(vec![7; len])));
        assert_eq!(
            err_of(&wrong),
            Some(PointerError::Shape(ShapeError::WrongLength(key))),
            "key {key}, {len} bytes"
        );
    }
}

#[test]
fn c2_wrong_type_at_every_leaf_of_a_fully_populated_pointer() {
    // The tree of the signed fully populated pointer: every leaf is replaced, one at a time, by a value
    // of another type, and every one of those pointers is refused. Not replaced: the parameters of the
    // transform (any CBOR value is a valid parameter).
    let bytes = full_signed();
    let root = tree(&bytes);
    let paths = leaf_paths(&root);
    let mut checked = 0usize;
    for path in &paths {
        // The root map's entries are the keys 1 to 15 in order: entry i is key i + 1.
        let key = path[0] as u64 + 1;
        if key == 6 && path.get(1) == Some(&2) {
            continue;
        }
        let wrong = wrong_type(value_at(&root, path));
        let mutated = bytes_of(&replace_at(&root, path, &wrong));
        assert!(
            err_of(&mutated).is_some(),
            "a leaf of key {key} at {path:?} as {wrong:?} was accepted"
        );
        checked += 1;
    }
    assert!(checked >= 90, "{checked} leaves");
}

#[test]
fn c2_versions() {
    let bytes = full_signed();
    // 0 and 1 are formats this build does not read.
    assert_eq!(
        err_of(&with_entry(&bytes, 1, Some(Value::Uint(0)))),
        Some(PointerError::UnsupportedVersion(0))
    );
    assert_eq!(
        err_of(&with_entry(&bytes, 1, Some(Value::Uint(1)))),
        Some(PointerError::UnsupportedVersion(1))
    );
    // 3 to 255 are newer: never damage, and nothing else is read.
    for v in [3u64, 4, 100, 255] {
        match read(&with_entry(&bytes, 1, Some(Value::Uint(v))))
            .expect("a newer pointer is no error")
        {
            Decoded::Newer(n) => assert_eq!(u64::from(n), v),
            Decoded::Current(_) => panic!("version {v} read as current"),
        }
    }
    // A newer pointer of any shape that is a map with a version: {1: 3, 2: "xxx", 9: [1, 2, 3]}.
    assert!(matches!(
        read(&from_hex("a3010302637878780983010203")),
        Ok(Decoded::Newer(3))
    ));
    // But it is still deterministic CBOR: a newer pointer with a duplicate key is damage.
    assert_eq!(
        err_of(&from_hex("a201030103")),
        Some(PointerError::Cbor(DecodeError::DuplicateMapKey))
    );
    // Above 255 the number does not fit a byte: damage.
    assert_eq!(
        err_of(&with_entry(&bytes, 1, Some(Value::Uint(256)))),
        Some(PointerError::BadVersion)
    );
    assert_eq!(
        err_of(&with_entry(&bytes, 1, Some(Value::Uint(u64::MAX)))),
        Some(PointerError::BadVersion)
    );
    // The version is required and is an unsigned integer.
    assert_eq!(
        err_of(&with_entry(&bytes, 1, None)),
        Some(PointerError::Shape(ShapeError::Missing(1)))
    );
    assert_eq!(
        err_of(&with_entry(&bytes, 1, Some(Value::Nint(0)))),
        Some(PointerError::Shape(ShapeError::WrongType(1)))
    );
    // Version 2 is current.
    assert!(matches!(read(&bytes), Ok(Decoded::Current(_))));
}

#[test]
fn c2_crit_lists_and_unknown_keys() {
    let bytes = full_signed();

    // An unknown key that is not in `crit` is accepted, and kept.
    let with_unknown = with_entry(&bytes, 40, Some(text("a newer field")));
    match read(&with_unknown).expect("an unknown key is ignored") {
        Decoded::Current(p) => {
            assert_eq!(p.unknown.len(), 1);
            assert_eq!(p.unknown.iter().next().expect("one").0, 40);
            assert!(p.crit.is_empty());
        }
        Decoded::Newer(_) => panic!("version 2"),
    }

    // An unknown key that `crit` lists is refused (the writer says the reader must understand it).
    let must = with_entry(&with_unknown, 0, Some(Value::Array(vec![Value::Uint(40)])));
    assert_eq!(
        err_of(&must),
        Some(PointerError::Shape(ShapeError::CritUnknown(40)))
    );
    // ... also when the listed key is absent: this reader does not understand it either.
    let absent = with_entry(&bytes, 0, Some(Value::Array(vec![Value::Uint(99)])));
    assert_eq!(
        err_of(&absent),
        Some(PointerError::Shape(ShapeError::CritUnknown(99)))
    );

    // Known keys in `crit` are fine, and the list is kept.
    let known = with_entry(
        &bytes,
        0,
        Some(Value::Array(vec![
            Value::Uint(2),
            Value::Uint(7),
            Value::Uint(14),
        ])),
    );
    match read(&known).expect("known keys may be listed") {
        Decoded::Current(p) => assert_eq!(p.crit, vec![2, 7, 14]),
        Decoded::Newer(_) => panic!("version 2"),
    }

    // A malformed `crit` is refused: empty, unsorted, repeated, naming 0, not an array, not integers.
    for (what, value) in [
        ("empty", Value::Array(Vec::new())),
        (
            "unsorted",
            Value::Array(vec![Value::Uint(7), Value::Uint(2)]),
        ),
        (
            "repeated",
            Value::Array(vec![Value::Uint(7), Value::Uint(7)]),
        ),
        (
            "naming 0",
            Value::Array(vec![Value::Uint(0), Value::Uint(7)]),
        ),
        ("not an array", Value::Uint(7)),
        ("not integers", Value::Array(vec![text("7")])),
    ] {
        assert_eq!(
            err_of(&with_entry(&bytes, 0, Some(value))),
            Some(PointerError::Shape(ShapeError::BadCrit)),
            "{what}"
        );
    }
}

#[test]
fn c2_missing_fields() {
    let bytes = full_signed();
    // The required keys: a missing one is named.
    for key in [2u64, 3, 4, 5, 7, 8, 15] {
        assert_eq!(
            err_of(&with_entry(&bytes, key, None)),
            Some(PointerError::Shape(ShapeError::Missing(key))),
            "key {key}"
        );
    }
    // The optional keys: the previews, streams, attributes, labels, derived records and the signature
    // may be absent.
    for key in [9u64, 10, 11, 12, 13, 14] {
        assert!(
            matches!(
                read(&with_entry(&bytes, key, None)),
                Ok(Decoded::Current(_))
            ),
            "key {key} is optional"
        );
    }
    // Without the transform the chunks (2,600,000 bytes) no longer add up to the size (3,000,000).
    assert_eq!(
        err_of(&with_entry(&bytes, 6, None)),
        Some(PointerError::SizeMismatch)
    );
}

#[test]
fn c2_an_empty_list_is_absent_never_an_empty_array() {
    let bytes = full_signed();
    for key in [9u64, 10, 11, 12, 13] {
        assert_eq!(
            err_of(&with_entry(&bytes, key, Some(Value::Array(Vec::new())))),
            Some(PointerError::NotCanonical),
            "key {key}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C12
// ---------------------------------------------------------------------------------------------

#[test]
fn c12_declared_size_reasonableness() {
    let one = signed(minimal_one_chunk())
        .to_cbor()
        .expect("writes")
        .to_vec();
    // A size above 2^50 is refused at decode, whatever the chunks say (and at encode).
    for size in [(1u64 << 50) + 1, u64::MAX, u64::MAX - 1] {
        assert_eq!(
            err_of(&with_entry(&one, 2, Some(Value::Uint(size)))),
            Some(PointerError::SizeTooLarge),
            "size {size}"
        );
        let mut p = minimal_one_chunk();
        p.size = FileSize::new(size);
        assert_eq!(
            both(&p),
            (
                Some(PointerError::SizeTooLarge),
                Some(PointerError::SizeTooLarge)
            )
        );
    }
    // 2^50 itself is not too large; it does not add up to one chunk.
    let mut p = minimal_one_chunk();
    p.size = FileSize::new(1 << 50);
    assert_eq!(
        both(&p),
        (
            Some(PointerError::SizeMismatch),
            Some(PointerError::SizeMismatch)
        )
    );

    // A transform's length and a chunk list's total are bounded too.
    let mut p = full();
    p.transform.as_mut().expect("a transform").out_len = FileSize::new(u64::MAX);
    assert_eq!(
        both(&p),
        (
            Some(PointerError::SizeTooLarge),
            Some(PointerError::SizeTooLarge)
        )
    );
}

#[test]
fn c12_chunk_counts_and_the_largest_valid_pointer() {
    let object = |count: u32, total: u64, size: u64| {
        let mut p = minimal_one_chunk();
        p.size = FileSize::new(size);
        p.chunks = ChunkList::Object(ChunkListRef {
            name: [0xCC; 32],
            count,
            total_raw: FileSize::new(total),
        });
        p
    };
    let most = 1u32 << 24; // MAX_FILE_CHUNKS
    let largest = u64::from(most) * 4 * 1024 * 1024; // 2^46: every chunk the longest
                                                     // The largest pointer there can be: 2^24 chunks of 4 MiB. It writes and it reads.
    let p = object(most, largest, largest);
    let bytes = p
        .to_cbor()
        .expect("the largest valid pointer writes")
        .to_vec();
    assert_eq!(bytes, reference_encode(&p, true));
    assert!(matches!(read(&bytes), Ok(Decoded::Current(_))));
    // One chunk more than the cap, or a total that cannot belong to that many chunks: refused.
    for (count, total, size, expected) in [
        (most + 1, largest, largest, PointerError::TooManyChunks),
        (
            u32::MAX,
            u64::from(u32::MAX),
            u64::from(u32::MAX),
            PointerError::TooManyChunks,
        ),
        (1024, 1024, 1024, PointerError::ObjectTooSmall),
        (0, 0, 0, PointerError::ObjectTooSmall),
        (most, largest + 1, largest + 1, PointerError::SizeMismatch),
        (1025, 1024, 1024, PointerError::SizeMismatch),
        (1025, 1025, 1026, PointerError::SizeMismatch),
        (1025, u64::MAX, u64::MAX, PointerError::SizeTooLarge),
    ] {
        let p = object(count, total, size);
        assert_eq!(
            both(&p),
            (Some(expected), Some(expected)),
            "count {count} total {total}"
        );
    }
    // A count that does not fit 32 bits is a count above the cap.
    let wide = with_entry(
        &bytes,
        7,
        Some(list_ref_value([0xCC; 32], 1 << 32, largest)),
    );
    assert_eq!(err_of(&wide), Some(PointerError::TooManyChunks));
    let wide = with_entry(
        &bytes,
        7,
        Some(list_ref_value([0xCC; 32], u64::MAX, largest)),
    );
    assert_eq!(err_of(&wide), Some(PointerError::TooManyChunks));
}

#[test]
fn c12_every_number_of_a_chunk_reference_at_its_extremes() {
    let one = signed(minimal_one_chunk())
        .to_cbor()
        .expect("writes")
        .to_vec();
    let root = tree(&one);
    // chunks[0] = [full, raw_len, pack, offset, stored_len]. This pointer has no transform, so the map's
    // entries are the keys 1, 2, 3, 4, 5, 7, 8, 14, 15: key 7 is entry 5.
    let at = |field: usize, v: Value| {
        let mutated = replace_at(&root, &[5, 0, field], &v);
        err_of(&bytes_of(&mutated))
    };
    let u32max = u64::from(u32::MAX);
    assert_eq!(
        at(1, Value::Uint(0)),
        Some(PointerError::BadChunk(ChunkError::EmptyChunk))
    );
    assert_eq!(
        at(1, Value::Uint(u32max)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        at(1, Value::Uint(u64::MAX)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        at(1, Value::Uint(4 * 1024 * 1024 + 1)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        at(4, Value::Uint(0)),
        Some(PointerError::BadChunk(ChunkError::Truncated))
    );
    assert_eq!(
        at(4, Value::Uint(41)),
        Some(PointerError::BadChunk(ChunkError::Truncated))
    );
    assert_eq!(
        at(4, Value::Uint(u32max)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        at(4, Value::Uint(1042)),
        Some(PointerError::BadChunk(ChunkError::LengthMismatch))
    );
    // offset + stored_len must fit 32 bits: the offset is checked with the stored length.
    assert_eq!(
        at(3, Value::Uint(u32max)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        at(3, Value::Uint(u32max - 1040)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        at(3, Value::Uint(1 << 32)),
        Some(PointerError::BadChunk(ChunkError::TooLong))
    );
    // ... and exactly at the edge it is fine.
    assert!(at(3, Value::Uint(u32max - 1041)).is_none());
}

#[test]
fn c12_times_are_bounded_to_the_years_0_to_9999() {
    // mtime and created_at: the edges are valid, one second past them is not; nanoseconds stay below
    // one second. Written and read alike.
    let min = -62_167_219_200i64;
    let max = 253_402_300_799i64;
    for (secs, nanos, ok) in [
        (min, 0u32, true),
        (min - 1, 0, false),
        (max, 999_999_999, true),
        (max + 1, 0, false),
        (i64::MIN, 0, false),
        (i64::MAX, 0, false),
        (0, 0, true),
        (-1, 999_999_999, true),
    ] {
        let mut p = minimal_one_chunk();
        p.mtime = FileTime::new(secs, nanos).expect("nanoseconds below a second");
        let expected = if ok {
            None
        } else {
            Some(PointerError::BadTime)
        };
        assert_eq!(both(&p), (expected, expected), "mtime {secs}.{nanos:09}");
    }
    for (secs, ok) in [
        (0u64, true),
        (253_402_300_799, true),
        (253_402_300_800, false),
        (u64::MAX, false),
    ] {
        let mut p = minimal_one_chunk();
        p.created_at = azlin_keys::UnixTime::from_secs(secs);
        let expected = if ok {
            None
        } else {
            Some(PointerError::BadTime)
        };
        assert_eq!(both(&p), (expected, expected), "created_at {secs}");
    }
    // Nanoseconds of a second or more are no time at all: the reader refuses them.
    let one = signed(minimal_one_chunk())
        .to_cbor()
        .expect("writes")
        .to_vec();
    let root = tree(&one);
    let bad = bytes_of(&replace_at(&root, &[2, 1], &Value::Uint(1_000_000_000)));
    assert_eq!(err_of(&bad), Some(PointerError::BadTime));
    let huge_secs = bytes_of(&replace_at(&root, &[2, 0], &Value::Uint(u64::MAX)));
    assert_eq!(err_of(&huge_secs), Some(PointerError::BadTime));
    let huge_neg = bytes_of(&replace_at(&root, &[2, 0], &Value::Nint(u64::MAX)));
    assert_eq!(err_of(&huge_neg), Some(PointerError::BadTime));
}
