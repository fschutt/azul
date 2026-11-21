//! C2 to C7: what the decoder refuses and why, and what `Map` and `encode` guarantee in return.
//! Every refusal is asserted with the exact `DecodeError`, so a check that holds for the wrong
//! reason fails.

mod common;

use azlin_cbor::{decode, encode, DecodeError, EncodeError, Limits, Map, MapError, Value};
use common::{from_hex, load_vectors, to_hex, SplitMix64};

const L: Limits = Limits::OBJECT;

/// The bytes of the shortest head of `v`, apart from the initial byte: 0, 1, 2, 4 or 8.
fn shortest_width(v: u64) -> usize {
    if v < 24 {
        0
    } else if v <= 0xff {
        1
    } else if v <= 0xffff {
        2
    } else if v <= 0xffff_ffff {
        4
    } else {
        8
    }
}

const BOUNDARIES: [u64; 18] = [
    0,
    1,
    22,
    23,
    24,
    25,
    255,
    256,
    257,
    65535,
    65536,
    65537,
    0xffff_fffe,
    0xffff_ffff,
    0x1_0000_0000,
    0x1_0000_0001,
    u64::MAX - 1,
    u64::MAX,
];

// ---------------------------------------------------------------------------------------- C2

#[test]
fn c2_non_shortest_integer_and_length_is_refused() {
    let mut refused = 0;
    // Majors 0 and 1 are integers; 2, 3, 4 and 5 carry a length: the head is checked first, so a
    // head alone is enough to be refused.
    for major in 0u8..=5 {
        for &v in &BOUNDARIES {
            for width in [1usize, 2, 4, 8] {
                if shortest_width(v) >= width {
                    continue;
                }
                let additional = match width {
                    1 => 24,
                    2 => 25,
                    4 => 26,
                    _ => 27,
                };
                let mut bytes = vec![(major << 5) | additional];
                bytes.extend_from_slice(&v.to_be_bytes()[8 - width..]);
                assert_eq!(
                    decode(&bytes, &L),
                    Err(DecodeError::NotShortest),
                    "major {major}, value {v}, {width}-byte head: {}",
                    to_hex(&bytes)
                );
                refused += 1;
            }
        }
    }
    // 35 wider-than-shortest heads per major type (4 + 3 + 2 + 1 widths over the 18 boundaries).
    assert_eq!(refused, 6 * 35, "the loop covered {refused} heads");
    // The examples of the brief, spelled out.
    for hex in [
        "1800",
        "190018",
        "5800",
        "9800",
        "3800",
        "7800",
        "b800",
        "590000",
        "1b0000000000000000",
    ] {
        assert_eq!(
            decode(&from_hex(hex), &L),
            Err(DecodeError::NotShortest),
            "{hex}"
        );
    }
    // Not shortest deep inside a value: as an element, a key, a value and a length.
    assert_eq!(
        decode(&from_hex("82011800"), &L),
        Err(DecodeError::NotShortest)
    );
    assert_eq!(
        decode(&from_hex("a1180001"), &L),
        Err(DecodeError::NotShortest)
    );
    assert_eq!(
        decode(&from_hex("a1011800"), &L),
        Err(DecodeError::NotShortest)
    );
    assert_eq!(
        decode(&from_hex("819800"), &L),
        Err(DecodeError::NotShortest)
    );
}

#[test]
fn c2_the_shortest_forms_are_accepted() {
    for &v in &BOUNDARIES {
        for value in [Value::Uint(v), Value::Nint(v)] {
            let bytes = encode(&value, &L).expect("an integer encodes");
            assert_eq!(
                bytes.len(),
                1 + shortest_width(v),
                "{value:?} is written in its shortest form"
            );
            assert_eq!(decode(&bytes, &L), Ok(value));
        }
    }
    for n in [0usize, 1, 22, 23, 24, 25, 255, 256, 257] {
        let head = 1 + shortest_width(n as u64);
        let bytes = Value::Bytes(vec![7; n]);
        let text = Value::Text("a".repeat(n));
        let array = Value::Array(vec![Value::Uint(0); n]);
        let pairs: Vec<(Value, Value)> = (0..n as u64)
            .map(|i| (Value::Uint(i), Value::Null))
            .collect();
        let map = Value::Map(Map::from_entries(pairs).expect("distinct keys"));
        for (value, content) in [(bytes, n), (text, n), (array, n)] {
            let out = encode(&value, &L).expect("encodes");
            assert_eq!(
                out.len(),
                head + content,
                "the length of {n} is written in its shortest form"
            );
            assert_eq!(decode(&out, &L), Ok(value));
        }
        let out = encode(&map, &L).expect("encodes");
        assert_eq!(out.first().map(|b| b >> 5), Some(5));
        assert_eq!(decode(&out, &L), Ok(map));
    }
}

// ---------------------------------------------------------------------------------------- C3

#[test]
fn c3_indefinite_lengths_and_break_are_refused() {
    for initial in [0x5fu8, 0x7f, 0x9f, 0xbf] {
        assert_eq!(
            decode(&[initial], &L),
            Err(DecodeError::IndefiniteLength),
            "{initial:#x}"
        );
    }
    assert_eq!(decode(&[0xff], &L), Err(DecodeError::ReservedBreak));
    // Whole indefinite-length items, as other encoders write them.
    for hex in [
        "5f4100ff", "7f6161ff", "9f0102ff", "bf0102ff", "5fff", "9fff", "bfff",
    ] {
        assert_eq!(
            decode(&from_hex(hex), &L),
            Err(DecodeError::IndefiniteLength),
            "{hex}"
        );
    }
    // Inside a definite container, as an element, a key and a value.
    assert_eq!(
        decode(&[0x81, 0x5f], &L),
        Err(DecodeError::IndefiniteLength)
    );
    assert_eq!(
        decode(&[0xa1, 0x9f], &L),
        Err(DecodeError::IndefiniteLength)
    );
    assert_eq!(
        decode(&[0xa1, 0x01, 0xbf], &L),
        Err(DecodeError::IndefiniteLength)
    );
    assert_eq!(decode(&[0x81, 0xff], &L), Err(DecodeError::ReservedBreak));
    assert_eq!(
        decode(&[0xa1, 0xff, 0x00], &L),
        Err(DecodeError::ReservedBreak)
    );
    assert_eq!(
        decode(&[0xa1, 0x01, 0xff], &L),
        Err(DecodeError::ReservedBreak)
    );
}

// ---------------------------------------------------------------------------------------- C4

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, first);
            out.push(tail);
        }
    }
    out
}

/// A map with these keys in THIS order, every value 0, assembled by hand (no `Map`).
fn map_bytes_in_order(keys: &[Value]) -> Vec<u8> {
    let mut out = vec![0xa0 | u8::try_from(keys.len()).expect("a small map")];
    for k in keys {
        out.extend(encode(k, &L).expect("a scalar key encodes"));
        out.push(0x00);
    }
    out
}

#[test]
fn c4_map_keys_are_sorted_bytewise_and_unique() {
    // 23 (0x17) before 24 (0x1818) before 256 (0x190100) before "a" (0x6161).
    let canonical = [
        Value::Uint(23),
        Value::Uint(24),
        Value::Uint(256),
        Value::Text("a".into()),
    ];
    let want = map_bytes_in_order(&canonical);
    assert_eq!(to_hex(&want), "a4170018180019010000616100");
    for perm in permutations(&[0, 1, 2, 3]) {
        let keys: Vec<Value> = perm.iter().map(|&i| canonical[i].clone()).collect();
        let bytes = map_bytes_in_order(&keys);
        if perm == [0, 1, 2, 3] {
            assert!(decode(&bytes, &L).is_ok(), "the canonical order is refused");
        } else {
            assert_eq!(
                decode(&bytes, &L),
                Err(DecodeError::UnsortedMapKeys),
                "order {perm:?}"
            );
        }
        // `from_entries` sorts the same way the decoder demands, whatever the order given.
        let entries: Vec<(Value, Value)> = keys.into_iter().map(|k| (k, Value::Uint(0))).collect();
        let map = Map::from_entries(entries).expect("distinct keys");
        assert_eq!(
            encode(&Value::Map(map), &L).expect("encodes"),
            want,
            "order {perm:?}"
        );
    }
    // Duplicates: the decoder refuses them, `from_entries` too, however far apart they sit.
    for key in [
        Value::Uint(1),
        Value::Nint(1),
        Value::Text("a".into()),
        Value::Bytes(vec![1]),
        Value::Array(vec![Value::Uint(1)]),
        Value::Null,
        Value::Bool(true),
    ] {
        let adjacent = map_bytes_in_order(&[key.clone(), key.clone()]);
        assert_eq!(
            decode(&adjacent, &L),
            Err(DecodeError::DuplicateMapKey),
            "{key:?}"
        );
        let apart = vec![
            (key.clone(), Value::Uint(1)),
            (Value::Map(Map::default()), Value::Uint(2)),
            (key.clone(), Value::Uint(3)),
        ];
        assert_eq!(
            Map::from_entries(apart),
            Err(MapError::DuplicateKey),
            "{key:?}"
        );
    }
    // Equal numbers of different type are different keys: 5 and -6 are `05` and `25`.
    let both = Map::from_entries(vec![
        (Value::Nint(5), Value::Null),
        (Value::Uint(5), Value::Null),
    ])
    .expect("distinct");
    assert_eq!(
        both.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        vec![Value::Uint(5), Value::Nint(5)]
    );
}

/// Keys of every type, in the bytewise order of their encodings.
fn canonical_key_list() -> Vec<Value> {
    vec![
        Value::Uint(1),
        Value::Uint(300),
        Value::Nint(0),
        Value::Nint(5),
        Value::Bytes(vec![]),
        Value::Bytes(vec![0]),
        Value::Bytes(vec![1, 2]),
        Value::Text(String::new()),
        Value::Text("b".into()),
        Value::Text("aa".into()),
        Value::Array(vec![]),
        Value::Array(vec![Value::Uint(1)]),
        Value::Map(Map::default()),
        Value::Bool(false),
        Value::Bool(true),
        Value::Null,
    ]
}

#[test]
fn c4_mixed_key_types_follow_the_encoded_byte_order() {
    let canonical = canonical_key_list();
    let encoded: Vec<Vec<u8>> = canonical
        .iter()
        .map(|k| encode(k, &L).expect("encodes"))
        .collect();
    assert!(
        encoded.windows(2).all(|w| w[0] < w[1]),
        "the list is not in encoded-byte order: {encoded:?}"
    );
    // The text keys "b" before "aa": shorter first, as RFC 8949 4.2.1 says (not alphabetical).
    assert_eq!(to_hex(&encoded[8]), "6162");
    assert_eq!(to_hex(&encoded[9]), "626161");
    let want = map_bytes_in_order(&canonical);
    assert_eq!(
        decode(&want, &L).map(|v| v.as_map().map(Map::len)),
        Ok(Some(canonical.len()))
    );
    let mut rng = SplitMix64::new(0x0c4);
    for round in 0..200 {
        let mut shuffled = canonical.clone();
        for i in (1..shuffled.len()).rev() {
            let j = rng.below(i as u64 + 1) as usize;
            shuffled.swap(i, j);
        }
        let entries: Vec<(Value, Value)> = shuffled
            .iter()
            .cloned()
            .map(|k| (k, Value::Uint(0)))
            .collect();
        let map = Map::from_entries(entries).expect("distinct keys");
        let got: Vec<Value> = map.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(
            got, canonical,
            "round {round}: from_entries did not sort by the encoded key"
        );
        assert_eq!(
            encode(&Value::Map(map), &L).expect("encodes"),
            want,
            "round {round}"
        );
        if shuffled != canonical {
            assert_eq!(
                decode(&map_bytes_in_order(&shuffled), &L),
                Err(DecodeError::UnsortedMapKeys),
                "round {round}"
            );
        }
    }
    // Keys that are themselves maps are compared by their encodings too.
    let inner = |k: u64| {
        Value::Map(Map::from_entries(vec![(Value::Uint(k), Value::Null)]).expect("one key"))
    };
    let map = Map::from_entries(vec![(inner(2), Value::Null), (inner(1), Value::Null)])
        .expect("distinct");
    assert_eq!(
        map.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        vec![inner(1), inner(2)]
    );
    assert_eq!(map.get(&inner(2)), Some(&Value::Null));
    assert_eq!(map.get(&inner(3)), None);
}

// ---------------------------------------------------------------------------------------- C5

#[test]
fn c5_trailing_bytes_and_every_truncation_are_refused() {
    let rows = load_vectors();
    let mut prefixes = 0usize;
    for r in rows.iter().filter(|r| r.valid) {
        let bytes = from_hex(&r.hex);
        assert!(decode(&bytes, &L).is_ok(), "{}", r.name);
        for cut in 0..bytes.len() {
            assert_eq!(
                decode(&bytes[..cut], &L),
                Err(DecodeError::Truncated),
                "{}: the prefix of {cut} of {} bytes",
                r.name,
                bytes.len()
            );
            prefixes += 1;
        }
        for extra in [0x00u8, 0xff, 0x80, 0xf6] {
            let mut longer = bytes.clone();
            longer.push(extra);
            assert_eq!(
                decode(&longer, &L),
                Err(DecodeError::TrailingBytes),
                "{}: one byte {extra:#x} after it",
                r.name
            );
        }
        let mut two = bytes.clone();
        two.extend_from_slice(&bytes);
        assert_eq!(
            decode(&two, &L),
            Err(DecodeError::TrailingBytes),
            "{}: two items",
            r.name
        );
    }
    assert!(prefixes > 1000, "{prefixes} prefixes checked");
}

// ---------------------------------------------------------------------------------------- C6

#[test]
fn c6_floats_tags_simple_values_are_refused() {
    // Floats: half, single, double, with their payloads and cut short.
    for hex in [
        "f90000",
        "f93c00",
        "f97e00",
        "fa00000000",
        "fa7f800000",
        "fb0000000000000000",
        "fb7ff0000000000000",
        "f9",
        "fa00",
        "fb000000",
    ] {
        assert_eq!(
            decode(&from_hex(hex), &L),
            Err(DecodeError::DisallowedType),
            "{hex}"
        );
    }
    // Tags 0xc0 to 0xdb (the argument forms), with and without content.
    for initial in 0xc0u8..=0xdb {
        for tail in ["", "00", "0000000000000000"] {
            let hex = format!("{initial:02x}{tail}");
            assert_eq!(
                decode(&from_hex(&hex), &L),
                Err(DecodeError::DisallowedType),
                "{hex}"
            );
        }
    }
    // undefined and the other simple values (0xe0 to 0xf3 are simple 0 to 19; 0xf8 takes a byte).
    assert_eq!(decode(&[0xf7], &L), Err(DecodeError::DisallowedType));
    for initial in 0xe0u8..=0xf3 {
        assert_eq!(
            decode(&[initial], &L),
            Err(DecodeError::DisallowedType),
            "{initial:#x}"
        );
    }
    for argument in [0x00u8, 0x13, 0x14, 0x20, 0xff] {
        assert_eq!(
            decode(&[0xf8, argument], &L),
            Err(DecodeError::DisallowedType),
            "simple {argument}"
        );
    }
    // Inside containers.
    assert_eq!(
        decode(&from_hex("81f93c00"), &L),
        Err(DecodeError::DisallowedType)
    );
    assert_eq!(
        decode(&from_hex("a1c00000"), &L),
        Err(DecodeError::DisallowedType)
    );
    assert_eq!(
        decode(&from_hex("a101f7"), &L),
        Err(DecodeError::DisallowedType)
    );
}

#[test]
fn c6_every_initial_byte_is_classified() {
    // Independent of the decoder: the table of RFC 8949 section 3 for every initial byte, read
    // with nothing after it. A head that needs more bytes is `Truncated`.
    for initial in 0u8..=255 {
        let want: Result<Value, DecodeError> = match initial {
            0x00..=0x17 => Ok(Value::Uint(u64::from(initial))),
            0x18..=0x1b => Err(DecodeError::Truncated),
            0x1c..=0x1f => Err(DecodeError::DisallowedType),
            0x20..=0x37 => Ok(Value::Nint(u64::from(initial - 0x20))),
            0x38..=0x3b => Err(DecodeError::Truncated),
            0x3c..=0x3f => Err(DecodeError::DisallowedType),
            0x40 => Ok(Value::Bytes(vec![])),
            0x41..=0x57 => Err(DecodeError::Truncated),
            0x58..=0x5b => Err(DecodeError::Truncated),
            0x5c..=0x5e => Err(DecodeError::DisallowedType),
            0x5f | 0x7f | 0x9f | 0xbf => Err(DecodeError::IndefiniteLength),
            0x60 => Ok(Value::Text(String::new())),
            0x61..=0x77 => Err(DecodeError::Truncated),
            0x78..=0x7b => Err(DecodeError::Truncated),
            0x7c..=0x7e => Err(DecodeError::DisallowedType),
            0x80 => Ok(Value::Array(vec![])),
            0x81..=0x97 => Err(DecodeError::Truncated),
            0x98..=0x9b => Err(DecodeError::Truncated),
            0x9c..=0x9e => Err(DecodeError::DisallowedType),
            0xa0 => Ok(Value::Map(Map::default())),
            0xa1..=0xb7 => Err(DecodeError::Truncated),
            0xb8..=0xbb => Err(DecodeError::Truncated),
            0xbc..=0xbe => Err(DecodeError::DisallowedType),
            0xc0..=0xf3 => Err(DecodeError::DisallowedType),
            0xf4 => Ok(Value::Bool(false)),
            0xf5 => Ok(Value::Bool(true)),
            0xf6 => Ok(Value::Null),
            0xf7..=0xfe => Err(DecodeError::DisallowedType),
            0xff => Err(DecodeError::ReservedBreak),
        };
        assert_eq!(decode(&[initial], &L), want, "initial byte {initial:#04x}");
    }
}

// ---------------------------------------------------------------------------------------- C7

fn text_item(content: &[u8]) -> Vec<u8> {
    let mut out = vec![0x60 | u8::try_from(content.len()).expect("short")];
    out.extend_from_slice(content);
    out
}

#[test]
fn c7_text_must_be_valid_utf8() {
    let invalid: &[&[u8]] = &[
        &[0xff],
        &[0xfe],
        &[0x80],
        &[0xbf],
        &[0xc0, 0x80],             // overlong NUL
        &[0xc1, 0xbf],             // overlong U+007F
        &[0xe0, 0x80, 0x80],       // overlong
        &[0xe0, 0x9f, 0xbf],       // overlong U+07FF
        &[0xf0, 0x80, 0x80, 0x80], // overlong
        &[0xf0, 0x8f, 0xbf, 0xbf], // overlong U+FFFF
        &[0xed, 0xa0, 0x80],       // U+D800, a surrogate half
        &[0xed, 0xbf, 0xbf],       // U+DFFF
        &[0xf4, 0x90, 0x80, 0x80], // U+110000
        &[0xf5, 0x80, 0x80, 0x80],
        &[0xc3], // truncated sequences
        &[0xe2, 0x82],
        &[0xf0, 0x90, 0x85],
        &[0xc3, 0x28], // a lead byte without its continuation
        &[0xe2, 0x28, 0xa1],
        &[0x61, 0xc3, 0x61],
    ];
    for content in invalid {
        assert_eq!(
            decode(&text_item(content), &L),
            Err(DecodeError::InvalidUtf8),
            "{}",
            to_hex(content)
        );
    }
    let valid: &[&[u8]] = &[
        &[],
        &[0x7f],
        &[0x00],
        &[0xc2, 0x80],             // U+0080
        &[0xdf, 0xbf],             // U+07FF
        &[0xe0, 0xa0, 0x80],       // U+0800
        &[0xed, 0x9f, 0xbf],       // U+D7FF
        &[0xee, 0x80, 0x80],       // U+E000
        &[0xef, 0xbf, 0xbd],       // U+FFFD
        &[0xf0, 0x90, 0x80, 0x80], // U+10000
        &[0xf4, 0x8f, 0xbf, 0xbf], // U+10FFFF
    ];
    for content in valid {
        let bytes = text_item(content);
        let got = decode(&bytes, &L).unwrap_or_else(|e| panic!("{}: {e:?}", to_hex(content)));
        assert_eq!(got.as_text().map(str::as_bytes), Some(*content));
        assert_eq!(encode(&got, &L).expect("encodes"), bytes);
    }
    // A byte string is bytes: it may hold anything.
    assert_eq!(
        decode(&[0x42, 0xff, 0xc0], &L),
        Ok(Value::Bytes(vec![0xff, 0xc0]))
    );
    // Inside a container too, as a key and as a value.
    assert_eq!(
        decode(&[0x81, 0x61, 0xff], &L),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(
        decode(&[0xa1, 0x61, 0xff, 0x00], &L),
        Err(DecodeError::InvalidUtf8)
    );
    assert_eq!(
        decode(&[0xa1, 0x00, 0x61, 0xff], &L),
        Err(DecodeError::InvalidUtf8)
    );
}

#[test]
fn c7_text_is_not_normalized() {
    // U+00E9 (NFC) and U+0065 U+0301 (NFD) are two values, each written and read as itself.
    let nfc = from_hex("62c3a9");
    let nfd = from_hex("6365cc81");
    let a = decode(&nfc, &L).expect("nfc");
    let b = decode(&nfd, &L).expect("nfd");
    assert_ne!(a, b);
    assert_eq!(a.as_text(), Some("\u{e9}"));
    assert_eq!(b.as_text(), Some("e\u{301}"));
    assert_eq!(encode(&a, &L).expect("encodes"), nfc);
    assert_eq!(encode(&b, &L).expect("encodes"), nfd);
    // As keys they are two keys, in the order of their encodings.
    let map = Map::from_entries(vec![
        (b.clone(), Value::Uint(2)),
        (a.clone(), Value::Uint(1)),
    ])
    .expect("distinct keys");
    assert_eq!(map.get(&a), Some(&Value::Uint(1)));
    assert_eq!(map.get(&b), Some(&Value::Uint(2)));
}

// ---------------------------------------------------------------------------------- the rest

#[test]
fn x_encode_writes_the_exact_buffer_once() {
    // Two passes: the buffer is sized by the first, so a value with key material in it is never
    // copied by a growing `Vec` (no stray copy in freed memory).
    let value = Value::Array(
        (0..200u64)
            .map(|i| Value::Bytes(vec![i as u8; (i % 50) as usize]))
            .collect(),
    );
    let out = encode(&value, &L).expect("encodes");
    assert_eq!(out.capacity(), out.len());
    assert_eq!(decode(&out, &L), Ok(value));
}

#[test]
fn x_encode_refuses_what_the_limits_refuse() {
    let deep = |n: usize| (0..n).fold(Value::Array(vec![]), |inner, _| Value::Array(vec![inner]));
    // 9 nested arrays: the empty one at the bottom makes 9 containers (HEADER allows 8).
    assert_eq!(
        encode(&deep(8), &Limits::HEADER),
        Err(EncodeError::DepthExceeded)
    );
    assert!(encode(&deep(7), &Limits::HEADER).is_ok());
    assert_eq!(
        encode(&Value::Array(vec![Value::Null; 1025]), &Limits::HEADER),
        Err(EncodeError::TooManyItems)
    );
    assert_eq!(
        encode(&Value::Bytes(vec![0; 16 * 1024]), &Limits::HEADER),
        Err(EncodeError::InputTooLarge)
    );
}

#[test]
fn x_value_accessors_and_map_api() {
    assert_eq!(Value::Uint(7).as_uint(), Some(7));
    assert_eq!(Value::Nint(7).as_uint(), None);
    assert_eq!(Value::Bytes(vec![1]).as_bytes(), Some(&[1u8][..]));
    assert_eq!(Value::Text("x".into()).as_text(), Some("x"));
    assert_eq!(
        Value::Array(vec![Value::Null]).as_array(),
        Some(&[Value::Null][..])
    );
    assert_eq!(Value::Bool(true).as_bool(), Some(true));
    assert!(Value::Null.is_null() && !Value::Bool(false).is_null());
    assert_eq!(Value::Null.as_map(), None);
    let map = Map::from_entries(vec![
        (Value::Uint(2), Value::Null),
        (Value::Uint(1), Value::Bool(true)),
    ])
    .expect("distinct");
    assert_eq!(map.len(), 2);
    assert!(!map.is_empty() && Map::default().is_empty());
    assert_eq!(map.get(&Value::Uint(1)), Some(&Value::Bool(true)));
    assert_eq!((&map).into_iter().count(), 2);
    let entries = map.clone().into_entries();
    assert_eq!(
        entries,
        vec![
            (Value::Uint(1), Value::Bool(true)),
            (Value::Uint(2), Value::Null)
        ]
    );
    assert_eq!(Map::from_entries(entries), Ok(map));
}

#[test]
fn x_a_key_nested_beyond_any_limit_is_refused_not_recursed() {
    // 300 nested arrays as a key: no `Limits` could encode it (max_depth is a u8).
    let deep = (0..300).fold(Value::Null, |inner, _| Value::Array(vec![inner]));
    assert_eq!(
        Map::from_entries(vec![(deep, Value::Null)]),
        Err(MapError::KeyTooDeep)
    );
}

#[test]
fn x_errors_display_and_are_errors() {
    fn is_error<E: std::error::Error>(_: &E) {}
    let e = DecodeError::NotShortest;
    is_error(&e);
    assert!(!e.to_string().is_empty());
    for e in [
        EncodeError::DepthExceeded,
        EncodeError::TooManyItems,
        EncodeError::InputTooLarge,
    ] {
        is_error(&e);
        assert!(!e.to_string().is_empty());
    }
    for e in [
        MapError::DuplicateKey,
        MapError::ReservedKey,
        MapError::BadCrit,
        MapError::KeyTooDeep,
        MapError::KeyTooLarge,
    ] {
        is_error(&e);
        assert!(!e.to_string().is_empty());
    }
}
