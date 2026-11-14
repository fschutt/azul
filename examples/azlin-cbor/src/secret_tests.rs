//! C16 and C17: the wipe of a decoded secret, counted. The counter is a `#[cfg(test)]` thread-local
//! of the library (`secret::counter`, incremented by the one wipe function), so these are unit tests;
//! the helpers of `tests/common` (the vector file, the seeded generator) are included by path, which
//! is why `lib.rs` names the crate `azlin_cbor` to itself under test.

#[path = "../tests/common/mod.rs"]
mod common;

use azlin_cbor::{
    decode, decode_secret, encode, DecodeError, IntMap, IntMapBuilder, Limits, Map, SecretValue,
    Value,
};

use crate::secret::counter;
use common::{from_hex, gen_value, load_vectors, SplitMix64, BIG};

const L: Limits = Limits::OBJECT;

/// What a wipe must account for: the bytes of every byte string and text string of the tree, map
/// keys included. An oracle of its own: a plain walk, not the wipe.
fn leaf_bytes(v: &Value) -> usize {
    match v {
        Value::Bytes(b) => b.len(),
        Value::Text(s) => s.len(),
        Value::Array(items) => items.iter().map(leaf_bytes).sum(),
        Value::Map(m) => m
            .iter()
            .map(|(k, val)| leaf_bytes(k) + leaf_bytes(val))
            .sum(),
        _ => 0,
    }
}

/// The number of byte string and text string buffers in the tree (empty ones count).
fn leaf_buffers(v: &Value) -> usize {
    match v {
        Value::Bytes(_) | Value::Text(_) => 1,
        Value::Array(items) => items.iter().map(leaf_buffers).sum(),
        Value::Map(m) => m
            .iter()
            .map(|(k, val)| leaf_buffers(k) + leaf_buffers(val))
            .sum(),
        _ => 0,
    }
}

/// Decodes `bytes` as a secret, checks that it reads as `decode` reads it and that reading wipes
/// nothing, drops it, and returns the (bytes, buffers) the drop wiped.
fn dropped(bytes: &[u8], l: &Limits) -> (usize, usize) {
    let plain = decode(bytes, l).expect("valid input");
    let secret = decode_secret(bytes, l).expect("valid as a secret too");
    assert_eq!(secret.as_value(), &plain);
    counter::reset();
    let _ = IntMap::new(secret.as_value());
    let _ = encode(secret.as_value(), l);
    assert_eq!(
        counter::get(),
        (0, 0),
        "reading a SecretValue must wipe nothing"
    );
    drop(secret);
    counter::get()
}

fn hand_cases() -> Vec<(&'static str, Value, (usize, usize))> {
    let text = |s: &str| Value::Text(s.to_string());
    let bytes = |b: &[u8]| Value::Bytes(b.to_vec());
    let uint = Value::Uint;
    let map = |entries: Vec<(Value, Value)>| {
        Value::Map(Map::from_entries(entries).expect("distinct keys"))
    };
    let int_map = map(vec![
        (uint(1), uint(7)),
        (uint(2), Value::Bytes(vec![0xaa; 32])),
        (uint(3), text("hunter2")),
        (uint(5), map(vec![(uint(1), Value::Bytes(vec![0x55; 16]))])),
        (uint(6), Value::Array(vec![bytes(&[1]), text("")])),
    ]);
    vec![
        ("bytes", bytes(&[1, 2, 3]), (3, 1)),
        ("text", text("h\u{e9}llo"), (6, 1)),
        (
            "empty leaves count as buffers",
            Value::Array(vec![bytes(&[]), text("")]),
            (0, 2),
        ),
        (
            "no leaves",
            Value::Array(vec![
                uint(1),
                Value::Nint(2),
                Value::Bool(true),
                Value::Null,
            ]),
            (0, 0),
        ),
        (
            "empty containers",
            Value::Array(vec![Value::Array(vec![]), map(vec![])]),
            (0, 0),
        ),
        (
            "nested arrays and maps",
            Value::Array(vec![
                bytes(&[1, 2]),
                Value::Array(vec![bytes(&[3]), text("ab")]),
                map(vec![(uint(1), bytes(&[4, 5])), (text("k"), text("vv"))]),
            ]),
            (10, 6),
        ),
        (
            "map keys are wiped",
            map(vec![(bytes(&[1, 2, 3]), uint(1)), (text("key"), uint(2))]),
            (6, 2),
        ),
        (
            "an array as a map key",
            map(vec![(
                Value::Array(vec![bytes(&[9, 9]), text("z")]),
                bytes(&[8]),
            )]),
            (4, 3),
        ),
        ("values behind the keys of an IntMap", int_map, (56, 5)),
        (
            "deep",
            (0..10).fold(bytes(&[1, 2, 3, 4]), |inner, _| {
                Value::Array(vec![inner, text("x")])
            }),
            (4 + 10, 11),
        ),
    ]
}

#[test]
fn c16_dropping_a_secret_value_wipes_every_leaf() {
    for (name, value, want) in hand_cases() {
        // The numbers written above are the oracles' too.
        assert_eq!(
            (leaf_bytes(&value), leaf_buffers(&value)),
            want,
            "{name}: the case's own numbers"
        );
        let wire = encode(&value, &L).expect("encodes");
        assert_eq!(dropped(&wire, &L), want, "{name}: decode_secret");
        // A tree built by hand and wrapped.
        counter::reset();
        drop(SecretValue::new(value));
        assert_eq!(counter::get(), want, "{name}: SecretValue::new");
    }
}

#[test]
fn c16_every_golden_vector_is_wiped_in_full() {
    let mut with_leaves = 0usize;
    for r in load_vectors().iter().filter(|r| r.valid) {
        let bytes = from_hex(&r.hex);
        let plain = decode(&bytes, &L).expect("a valid vector");
        let want = (leaf_bytes(&plain), leaf_buffers(&plain));
        assert_eq!(dropped(&bytes, &L), want, "{}", r.name);
        with_leaves += usize::from(want.1 > 0);
    }
    assert!(
        with_leaves >= 20,
        "{with_leaves} vectors with byte or text strings"
    );
}

#[test]
fn c16_random_trees_are_wiped_in_full() {
    let mut buffers = 0usize;
    for seed in 1..=8u64 {
        let mut rng = SplitMix64::new(seed);
        for case in 0..400 {
            let v = gen_value(&mut rng, 4);
            let wire = encode(&v, &BIG).expect("encodes");
            let want = (leaf_bytes(&v), leaf_buffers(&v));
            assert_eq!(dropped(&wire, &BIG), want, "seed {seed} case {case}");
            buffers += want.1;
        }
    }
    assert!(buffers > 1_000, "{buffers} buffers over all trees");
}

#[test]
fn c16_the_values_behind_int_map_keys_are_wiped() {
    let wire = encode(
        &IntMapBuilder::new()
            .put(1, Value::Uint(7))
            .put(2, Value::Bytes(vec![0xaa; 32]))
            .put(3, Value::Text("hunter2".into()))
            .put(
                5,
                Value::Map(
                    Map::from_entries(vec![(Value::Uint(1), Value::Bytes(vec![0x55; 16]))])
                        .expect("one key"),
                ),
            )
            .crit(&[1, 2])
            .build()
            .expect("builds"),
        &L,
    )
    .expect("encodes");
    let secret = decode_secret(&wire, &L).expect("decodes");
    let mut m = IntMap::new(secret.as_value()).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(7));
    let hash: [u8; 32] = m.req_bytes_n(2).expect("32 bytes");
    assert_eq!(hash, [0xaa; 32]);
    assert_eq!(m.req_text(3), Ok("hunter2"));
    let nested = m.req_map(5).expect("nested");
    assert_eq!(nested.len(), 1);
    assert_eq!(m.finish(), Ok(vec![]));
    counter::reset();
    drop(secret);
    // 32 + 7 + 16 bytes in three buffers.
    assert_eq!(counter::get(), (55, 3));
}

#[test]
fn c16_a_moved_secret_value_is_wiped_once() {
    let wire = encode(&Value::Array(vec![Value::Bytes(vec![1, 2, 3])]), &L).expect("encodes");
    let a = decode_secret(&wire, &L).expect("decodes");
    let b = a;
    let boxed = Box::new(b);
    counter::reset();
    drop(boxed);
    assert_eq!(counter::get(), (3, 1));
}

#[test]
fn c16_a_tree_of_any_depth_is_wiped_without_recursion() {
    // Built by hand, as `decode` never makes one (it stops at max_depth): 100,000 nested arrays
    // with one buffer at the bottom. The wipe walks a work list; plain recursion would overflow
    // the stack of this thread. (The drop of the emptied shell is a plain drop of nothing.)
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut v = Value::Bytes(vec![7; 5]);
            for _ in 0..100_000 {
                v = Value::Array(vec![v]);
            }
            counter::reset();
            drop(SecretValue::new(v));
            counter::get()
        })
        .expect("spawns a thread");
    assert_eq!(handle.join().expect("no overflow"), (5, 1));
}

// ---------------------------------------------------------------------------------------- C17

/// Refused vectors that fail AFTER some byte or text strings were decoded, with the bytes of those
/// strings, counted by hand from the hex. Every other refused vector of the file fails before it has
/// decoded a leaf, so nothing is to be wiped.
const BEFORE_ERROR: &[(&str, usize)] = &[
    // The pre-existing refused vectors with a string before the error: ("a" before the item is refused).
    ("trailing_after_text", 1),
    ("unsorted_keys_text_before_23", 1),
    ("unsorted_keys_aa_before_b", 3),
    ("unsorted_keys_bytes_00_before_empty", 1),
    ("duplicate_key_text", 2),
    // The vectors written for this check.
    ("secret_array_leaves_then_truncated", 3),
    ("secret_nested_arrays_then_reserved_head", 3),
    ("secret_map_duplicate_bytes_key", 3),
    ("secret_map_value_fails_after_key", 1),
    ("secret_map_unsorted_text_keys", 4),
    ("secret_trailing_after_tree", 2),
    ("secret_utf8_after_leaf", 1),
    ("secret_too_many_items_after_leaf", 1),
    ("secret_depth_after_leaf", 1),
    ("secret_size_after_leaf", 1),
    ("secret_indefinite_after_leaf", 1),
    ("secret_break_after_leaf", 1),
    ("secret_not_shortest_after_leaf", 1),
    ("secret_deep_leaves_then_break", 7),
    ("secret_array_key_leaves_then_reserved_value", 3),
    ("secret_second_pair_key_fails_after_a_full_pair", 2),
];

#[test]
fn c17_a_refused_secret_wipes_what_it_decoded_before_the_error() {
    let rows = load_vectors();
    for (name, _) in BEFORE_ERROR {
        let row = rows
            .iter()
            .find(|r| r.name == *name)
            .unwrap_or_else(|| panic!("no vector {name}"));
        assert!(!row.valid, "{name} must be a refused vector");
    }
    let mut with_leaves = 0usize;
    for r in rows.iter().filter(|r| !r.valid) {
        let want = BEFORE_ERROR
            .iter()
            .find(|(n, _)| *n == r.name)
            .map_or(0, |(_, bytes)| *bytes);
        let bytes = from_hex(&r.hex);
        counter::reset();
        let got = decode_secret(&bytes, &L);
        let wiped = counter::get().0;
        match got {
            Ok(_) => panic!("{}: accepted", r.name),
            Err(e) => assert_eq!(format!("{e:?}"), r.expect, "{}: the error", r.name),
        }
        assert_eq!(
            wiped, want,
            "{}: leaf bytes wiped before the error returned",
            r.name
        );
        with_leaves += usize::from(want > 0);
    }
    assert_eq!(with_leaves, BEFORE_ERROR.len());
}

#[test]
fn c17_plain_decode_never_wipes() {
    // `decode` is what it was: it makes no call of the wipe, on any input of the file, valid or not
    // (the count of the wipe function stays 0), and the tree it returns is a plain `Value`.
    for r in load_vectors() {
        let bytes = from_hex(&r.hex);
        counter::reset();
        let got = decode(&bytes, &L);
        drop(got);
        assert_eq!(
            counter::get(),
            (0, 0),
            "{}: plain decode must not wipe",
            r.name
        );
    }
}

/// A complete bad item (or the start of one) that follows what was decoded, with the error it gets.
const BAD_TAILS: &[(&str, &[u8], DecodeError)] = &[
    ("a float", &[0xf9, 0x3c, 0x00], DecodeError::DisallowedType),
    ("a reserved head", &[0x1c], DecodeError::DisallowedType),
    ("a tag", &[0xc0, 0x00], DecodeError::DisallowedType),
    (
        "an indefinite array",
        &[0x9f],
        DecodeError::IndefiniteLength,
    ),
    ("a break", &[0xff], DecodeError::ReservedBreak),
    (
        "a non-shortest integer",
        &[0x18, 0x00],
        DecodeError::NotShortest,
    ),
    (
        "text that is not UTF-8",
        &[0x61, 0xff],
        DecodeError::InvalidUtf8,
    ),
    (
        "a byte string cut short",
        &[0x43, 0x01],
        DecodeError::Truncated,
    ),
    ("a head cut short", &[0x19, 0x01], DecodeError::Truncated),
    (
        "65537 items claimed",
        &[0x9a, 0x00, 0x01, 0x00, 0x01],
        DecodeError::TooManyItems,
    ),
    (
        "a string over max_text",
        &[0x5a, 0x00, 0x40, 0x00, 0x01],
        DecodeError::InputTooLarge,
    ),
];

#[test]
fn c17_a_random_tree_followed_by_an_error_is_wiped_in_every_position() {
    let mut rng = SplitMix64::new(0x17);
    let mut runs = 0usize;
    let mut wiped_total = 0usize;
    for case in 0..300 {
        let v = gen_value(&mut rng, 3);
        let ev = encode(&v, &L).expect("encodes");
        let want = (leaf_bytes(&v), leaf_buffers(&v));
        for (tail_name, tail, error) in BAD_TAILS {
            let cat = |parts: &[&[u8]]| parts.concat();
            // Each shape puts the bad item after `v` has been decoded: in an array (also nested in
            // one, and deeper in the same one), as a second map key, as a map value, with `v` as a
            // map key held while its value fails, and `v` followed by bytes.
            let shapes: Vec<(&str, Vec<u8>)> = vec![
                ("array", cat(&[&[0x82], &ev, tail])),
                (
                    "array in an array",
                    cat(&[&[0x82, 0x82], &ev, &[0x00], tail]),
                ),
                ("first of a nested array", cat(&[&[0x81, 0x82], &ev, tail])),
                ("map value", cat(&[&[0xa2, 0x00], &ev, &[0x01], tail])),
                ("second map key", cat(&[&[0xa2, 0x00], &ev, tail])),
                (
                    "v as the first map key",
                    cat(&[&[0xa2], &ev, &[0x00], tail]),
                ),
                (
                    "v as a map key held while its value fails",
                    cat(&[&[0xa1], &ev, tail]),
                ),
            ];
            for (shape, bytes) in shapes {
                counter::reset();
                let got = decode_secret(&bytes, &L);
                let now = counter::get();
                match got {
                    Ok(_) => panic!("case {case}, {shape}, {tail_name}: accepted"),
                    Err(e) => assert_eq!(e, *error, "case {case}, {shape}, {tail_name}"),
                }
                assert_eq!(
                    now, want,
                    "case {case}, {shape}, {tail_name}: wiped before the error returned"
                );
                runs += 1;
                wiped_total += now.0;
            }
        }
        // The whole item decoded, then refused for the bytes after it.
        counter::reset();
        let trailing = [ev.as_slice(), &[0x00]].concat();
        assert!(matches!(
            decode_secret(&trailing, &L),
            Err(DecodeError::TrailingBytes)
        ));
        assert_eq!(counter::get(), want, "case {case}, trailing bytes");
    }
    assert!(
        runs > 20_000 && wiped_total > 20_000,
        "{runs} runs wiped {wiped_total} bytes"
    );
}
