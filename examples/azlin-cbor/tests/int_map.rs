//! C10 to C12: the reader and the builder of the structs of section 5: `crit` (key 0), the typed
//! accessors and a builder whose output does not depend on the order of its calls.

mod common;

use azlin_cbor::{decode, encode, IntMap, IntMapBuilder, Limits, Map, MapError, ShapeError, Value};
use common::{from_hex, to_hex, SplitMix64};

const L: Limits = Limits::OBJECT;

/// A struct with these unsigned-integer keys, written by hand, key 0 included.
fn st(fields: Vec<(u64, Value)>) -> Value {
    let entries = fields
        .into_iter()
        .map(|(k, v)| (Value::Uint(k), v))
        .collect();
    Value::Map(Map::from_entries(entries).expect("distinct keys"))
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn uints(items: &[u64]) -> Value {
    Value::Array(items.iter().map(|k| Value::Uint(*k)).collect())
}

// ---------------------------------------------------------------------------------------- C10

#[test]
fn c10_int_map_crit_rules() {
    // An unknown key that is not in `crit` is ignored AND returned, in key order.
    let v = st(vec![
        (1, Value::Uint(7)),
        (5, text("x")),
        (9, Value::Bytes(vec![0])),
    ]);
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(7));
    let (five, x, nine, zero) = (
        Value::Uint(5),
        text("x"),
        Value::Uint(9),
        Value::Bytes(vec![0]),
    );
    assert_eq!(m.finish(), Ok(vec![(&five, &x), (&nine, &zero)]));

    // An unknown key that IS in `crit`: refused with that key, present or not.
    let v = st(vec![(0, uints(&[5])), (1, Value::Uint(7)), (5, text("x"))]);
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(7));
    assert_eq!(m.finish(), Err(ShapeError::CritUnknown(5)));
    // The same message is fine for a reader that understands key 5.
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(7));
    assert_eq!(m.req_text(5), Ok("x"));
    assert_eq!(m.finish(), Ok(vec![]));

    // The lowest listed key that was not asked for is the one named.
    let v = st(vec![
        (0, uints(&[4, 6])),
        (4, Value::Null),
        (6, Value::Null),
    ]);
    let m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.finish(), Err(ShapeError::CritUnknown(4)));
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.opt_value(4), Ok(Some(&Value::Null)));
    assert_eq!(m.finish(), Err(ShapeError::CritUnknown(6)));
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.opt_value(6), Ok(Some(&Value::Null)));
    assert_eq!(m.finish(), Err(ShapeError::CritUnknown(4)));
    let mut m = IntMap::new(&v).expect("a struct");
    let _ = m.opt_value(4);
    let _ = m.opt_value(6);
    assert_eq!(m.finish(), Ok(vec![]));

    // A key in `crit` that is absent from the map is accepted, once the reader asked for it.
    let v = st(vec![(0, uints(&[3])), (1, Value::Uint(7))]);
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(7));
    assert_eq!(m.opt_u64(3), Ok(None));
    assert_eq!(m.finish(), Ok(vec![]));
    // ... and refused when the reader does not know the key at all.
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(7));
    assert_eq!(m.finish(), Err(ShapeError::CritUnknown(3)));

    // A malformed `crit` is `BadCrit`: not an array, empty, unsorted, duplicate, with 0, with a
    // value that is not an unsigned integer.
    let bad: Vec<(&str, Value)> = vec![
        ("an integer", Value::Uint(5)),
        ("a negative integer", Value::Nint(5)),
        ("text", text("x")),
        ("bytes", Value::Bytes(vec![5])),
        ("null", Value::Null),
        ("a boolean", Value::Bool(true)),
        ("a map", Value::Map(Map::default())),
        ("an empty array", Value::Array(vec![])),
        ("unsorted", uints(&[2, 1])),
        ("unsorted after a good prefix", uints(&[1, 3, 2])),
        ("a duplicate", uints(&[1, 1])),
        ("a duplicate at the end", uints(&[1, 2, 2])),
        ("only 0", uints(&[0])),
        ("0 first", uints(&[0, 1])),
        ("0 last", uints(&[1, 0])),
        ("a negative item", Value::Array(vec![Value::Nint(0)])),
        ("a text item", Value::Array(vec![Value::Uint(1), text("a")])),
        ("an array item", Value::Array(vec![Value::Array(vec![])])),
        ("a null item", Value::Array(vec![Value::Null])),
    ];
    for (what, crit) in bad {
        let v = st(vec![(0, crit), (1, Value::Uint(7))]);
        let mut m = IntMap::new(&v).expect("still a struct: `crit` is checked by finish");
        assert_eq!(m.req_u64(1), Ok(7), "{what}");
        assert_eq!(m.crit_keys(), Err(ShapeError::BadCrit), "{what}");
        assert_eq!(m.finish(), Err(ShapeError::BadCrit), "{what}");
    }

    // A key that is not an unsigned integer, or no map at all: not a struct.
    for (what, v) in [
        (
            "a text key",
            Value::Map(Map::from_entries(vec![(text("a"), Value::Null)]).expect("one key")),
        ),
        (
            "a negative key",
            Value::Map(Map::from_entries(vec![(Value::Nint(0), Value::Null)]).expect("one key")),
        ),
        (
            "a mixed map",
            Value::Map(
                Map::from_entries(vec![
                    (Value::Uint(1), Value::Null),
                    (text("a"), Value::Null),
                ])
                .expect("keys"),
            ),
        ),
        ("an array", Value::Array(vec![])),
        ("an integer", Value::Uint(1)),
        ("null", Value::Null),
        ("text", text("a")),
    ] {
        assert_eq!(
            IntMap::new(&v).err(),
            Some(ShapeError::NotAnIntMap),
            "{what}"
        );
    }
    // The empty map is a struct with no fields.
    let empty = Value::Map(Map::default());
    assert_eq!(IntMap::new(&empty).expect("a struct").finish(), Ok(vec![]));
}

#[test]
fn c10_crit_on_the_wire() {
    // {0: [7], 1: 2, 7: "abc"} as bytes: key 7 must be understood.
    let wire = from_hex(&["a3", "008107", "0102", "0763616263"].concat());
    let v = decode(&wire, &L).expect("canonical");
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(2));
    assert_eq!(m.finish(), Err(ShapeError::CritUnknown(7)));
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(2));
    assert_eq!(m.req_text(7), Ok("abc"));
    assert_eq!(m.crit_keys(), Ok(vec![7]));
    assert_eq!(m.finish(), Ok(vec![]));
    // {0: [7, 1]}: canonical CBOR (an array of two integers), but not a valid `crit`.
    let wire = from_hex("a100820701");
    let v = decode(&wire, &L).expect("canonical bytes");
    assert_eq!(
        IntMap::new(&v).expect("a struct").finish(),
        Err(ShapeError::BadCrit)
    );
}

#[test]
fn c10_a_copy_keeps_the_unknown_fields_and_crit() {
    // The rule that makes "ignore unknown fields" compatible with a signature over the bytes: a
    // structure read and written again by a reader that does not know fields 5 and 9 is the same
    // bytes, `crit` included.
    let original = IntMapBuilder::new()
        .put(1, Value::Uint(7))
        .put(2, Value::Bytes(vec![1, 2, 3]))
        .put(5, text("from a newer writer"))
        .put(9, Value::Array(vec![Value::Uint(1), Value::Null]))
        .crit(&[1, 2])
        .build()
        .expect("builds");
    let wire = encode(&original, &L).expect("encodes");
    let v = decode(&wire, &L).expect("canonical");
    let mut m = IntMap::new(&v).expect("a struct");
    let crit = m.crit_keys().expect("valid crit");
    let one = m.req_u64(1).expect("1");
    let two = m.req_bytes(2).expect("2");
    let unknown = m
        .finish()
        .expect("1 and 2 are understood, 5 and 9 are not critical");
    assert_eq!(crit, vec![1, 2]);
    assert_eq!(unknown.len(), 2);
    let mut copy = IntMapBuilder::new()
        .crit(&crit)
        .put(1, Value::Uint(one))
        .put(2, Value::Bytes(two.to_vec()));
    for (k, val) in unknown {
        copy = copy.put(k.as_uint().expect("an unsigned key"), val.clone());
    }
    let again = encode(&copy.build().expect("builds"), &L).expect("encodes");
    assert_eq!(to_hex(&again), to_hex(&wire));
}

// ---------------------------------------------------------------------------------------- C11

#[test]
fn c11_int_map_typed_access() {
    // A hash of 32 bytes: 31 and 33 are refused, with the key.
    for (len, ok) in [
        (0usize, false),
        (31, false),
        (32, true),
        (33, false),
        (64, false),
    ] {
        let v = st(vec![(1, Value::Bytes(vec![7; len]))]);
        let got = IntMap::new(&v).expect("a struct").req_bytes_n::<32>(1);
        let opt = IntMap::new(&v).expect("a struct").opt_bytes_n::<32>(1);
        if ok {
            assert_eq!(got, Ok([7u8; 32]));
            assert_eq!(opt, Ok(Some([7u8; 32])));
        } else {
            assert_eq!(got, Err(ShapeError::WrongLength(1)), "{len} bytes");
            assert_eq!(opt, Err(ShapeError::WrongLength(1)), "{len} bytes");
        }
    }
    // `req_u64` refuses everything but an unsigned integer.
    let others = [
        text("5"),
        Value::Nint(0),
        Value::Nint(4),
        Value::Bytes(vec![5]),
        Value::Array(vec![Value::Uint(5)]),
        Value::Map(Map::default()),
        Value::Bool(true),
        Value::Null,
    ];
    for other in others {
        let v = st(vec![(3, other.clone())]);
        assert_eq!(
            IntMap::new(&v).expect("a struct").req_u64(3),
            Err(ShapeError::WrongType(3)),
            "{other:?}"
        );
        assert_eq!(
            IntMap::new(&v).expect("a struct").opt_u64(3),
            Err(ShapeError::WrongType(3)),
            "{other:?}"
        );
    }
    let v = st(vec![(3, Value::Uint(u64::MAX))]);
    assert_eq!(IntMap::new(&v).expect("a struct").req_u64(3), Ok(u64::MAX));
    // A missing required key is `Missing(k)`, for every accessor; an optional one is `None`.
    let empty = st(vec![(8, Value::Null)]);
    let m = || IntMap::new(&empty).expect("a struct");
    assert_eq!(m().req_u64(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_bytes(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_bytes_n::<4>(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_text(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_array(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_map(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_bool(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().req_value(4), Err(ShapeError::Missing(4)));
    assert_eq!(m().opt_u64(4), Ok(None));
    assert_eq!(m().opt_bytes(4), Ok(None));
    assert_eq!(m().opt_bytes_n::<4>(4), Ok(None));
    assert_eq!(m().opt_text(4), Ok(None));
    assert_eq!(m().opt_array(4), Ok(None));
    assert_eq!(m().opt_map(4), Ok(None));
    assert_eq!(m().opt_bool(4), Ok(None));
    assert_eq!(m().opt_value(4), Ok(None));
    // Every accessor on a value of its own type.
    let inner = st(vec![(1, Value::Uint(1))]);
    let v = st(vec![
        (1, Value::Uint(11)),
        (2, Value::Bytes(vec![1, 2])),
        (3, text("three")),
        (4, Value::Array(vec![Value::Null])),
        (5, inner.clone()),
        (6, Value::Bool(false)),
        (7, Value::Nint(7)),
    ]);
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(11));
    assert_eq!(m.req_bytes(2), Ok(&[1u8, 2][..]));
    assert_eq!(m.req_text(3), Ok("three"));
    assert_eq!(m.req_array(4), Ok(&[Value::Null][..]));
    assert_eq!(m.req_map(5).map(|map| map.len()), Ok(1));
    assert_eq!(m.req_bool(6), Ok(false));
    assert_eq!(m.req_value(7), Ok(&Value::Nint(7)));
    assert_eq!(m.finish(), Ok(vec![]));
    // A nested struct is read with its own reader, and its own `crit`.
    let outer = st(vec![(1, st(vec![(0, uints(&[9])), (1, Value::Uint(1))]))]);
    let mut m = IntMap::new(&outer).expect("a struct");
    let mut nested = IntMap::new(m.req_value(1).expect("nested")).expect("a struct");
    assert_eq!(nested.req_u64(1), Ok(1));
    assert_eq!(nested.finish(), Err(ShapeError::CritUnknown(9)));
    assert_eq!(m.finish(), Ok(vec![]));
}

#[test]
fn c11_a_key_read_twice_is_an_error_not_a_panic() {
    let v = st(vec![(1, Value::Uint(1)), (2, text("a"))]);
    // Every pair of accessors on one key.
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(1));
    assert_eq!(m.req_u64(1), Err(ShapeError::KeyReadTwice));
    assert_eq!(m.opt_u64(1), Err(ShapeError::KeyReadTwice));
    assert_eq!(m.req_bytes(1), Err(ShapeError::KeyReadTwice));
    assert_eq!(m.opt_text(1), Err(ShapeError::KeyReadTwice));
    assert_eq!(m.req_value(1), Err(ShapeError::KeyReadTwice));
    // The reader is not spoiled: another key still reads, and `finish` still runs.
    assert_eq!(m.req_text(2), Ok("a"));
    assert_eq!(m.finish(), Ok(vec![]));
    // An absent key asked for through `opt_` is read too.
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.opt_u64(9), Ok(None));
    assert_eq!(m.opt_u64(9), Err(ShapeError::KeyReadTwice));
    assert_eq!(m.req_u64(9), Err(ShapeError::KeyReadTwice));
    // A failed read counts as a read.
    let mut m = IntMap::new(&v).expect("a struct");
    assert_eq!(m.req_text(1), Err(ShapeError::WrongType(1)));
    assert_eq!(m.req_u64(1), Err(ShapeError::KeyReadTwice));
    assert_eq!(m.req_u64(8), Err(ShapeError::Missing(8)));
    assert_eq!(m.req_u64(8), Err(ShapeError::KeyReadTwice));
    // Key 0 is `crit`'s: no accessor reads it.
    let with_crit = st(vec![(0, uints(&[1])), (1, Value::Uint(1))]);
    let mut m = IntMap::new(&with_crit).expect("a struct");
    assert_eq!(m.req_array(0), Err(ShapeError::ReservedKey));
    assert_eq!(m.opt_u64(0), Err(ShapeError::ReservedKey));
    assert_eq!(m.req_value(0), Err(ShapeError::ReservedKey));
    assert_eq!(m.req_u64(1), Ok(1));
    assert_eq!(m.finish(), Ok(vec![]));
}

// ---------------------------------------------------------------------------------------- C12

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn go(items: &[usize]) -> Vec<Vec<usize>> {
        if items.len() <= 1 {
            return vec![items.to_vec()];
        }
        let mut out = Vec::new();
        for i in 0..items.len() {
            let mut rest = items.to_vec();
            let first = rest.remove(i);
            for mut tail in go(&rest) {
                tail.insert(0, first);
                out.push(tail);
            }
        }
        out
    }
    go(&(0..n).collect::<Vec<_>>())
}

#[test]
fn c12_builder_is_deterministic() {
    let fields: Vec<(u64, Value)> = vec![
        (1, Value::Uint(2)),
        (2, Value::Bytes(vec![1, 2, 3])),
        (7, text("seven")),
        (300, Value::Array(vec![Value::Null])),
        (u64::MAX, Value::Bool(true)),
    ];
    let reference = {
        let mut b = IntMapBuilder::new();
        for (k, v) in &fields {
            b = b.put(*k, v.clone());
        }
        encode(&b.build().expect("builds"), &L).expect("encodes")
    };
    // By hand: a5 | 01 02 | 02 43010203 | 07 65"seven" | 19012c 81f6 | 1b ffffffffffffffff f5
    assert_eq!(
        to_hex(&reference),
        [
            "a5",
            "0102",
            "0243010203",
            "0765736576656e",
            "19012c81f6",
            "1b",
            "ffffffffffffffff",
            "f5"
        ]
        .concat()
    );
    // The same bytes whatever the order of the `put` calls (120 orders).
    for order in permutations(fields.len()) {
        let mut b = IntMapBuilder::new();
        for i in &order {
            let (k, v) = &fields[*i];
            b = b.put(*k, v.clone());
        }
        assert_eq!(
            encode(&b.build().expect("builds"), &L).expect("encodes"),
            reference,
            "order {order:?}"
        );
    }
}

#[test]
fn c12_crit_is_written_only_when_listed_and_in_canonical_form() {
    let base = || {
        IntMapBuilder::new()
            .put(1, Value::Uint(1))
            .put(5, Value::Null)
    };
    let plain = encode(&base().build().expect("builds"), &L).expect("encodes");
    // An empty list writes nothing.
    assert_eq!(
        encode(&base().crit(&[]).build().expect("builds"), &L).expect("encodes"),
        plain
    );
    let v = base().build().expect("builds");
    assert_eq!(IntMap::new(&v).expect("a struct").crit_keys(), Ok(vec![]));
    // The list is sorted and unique, whatever was given, and the calls add up.
    let want = encode(&base().crit(&[1, 5]).build().expect("builds"), &L).expect("encodes");
    for crit in [vec![5, 1], vec![5, 1, 5, 1], vec![1, 1, 5]] {
        let got = encode(&base().crit(&crit).build().expect("builds"), &L).expect("encodes");
        assert_eq!(got, want, "{crit:?}");
    }
    let split =
        encode(&base().crit(&[5]).crit(&[1]).build().expect("builds"), &L).expect("encodes");
    assert_eq!(split, want);
    // By hand: three pairs: 0 -> [1, 5], 1 -> 1, 5 -> null.
    assert_eq!(to_hex(&want), ["a3", "00820105", "0101", "05f6"].concat());
    assert_eq!(to_hex(&plain), ["a2", "0101", "05f6"].concat());
}

#[test]
fn c12_builder_refuses_key_0_duplicates_and_crit_naming_0() {
    let ok = || IntMapBuilder::new().put(1, Value::Null);
    // Key 0 only through `crit()`.
    assert_eq!(
        ok().put(0, Value::Array(vec![Value::Uint(1)])).build(),
        Err(MapError::ReservedKey)
    );
    assert_eq!(
        IntMapBuilder::new().put(0, Value::Null).build(),
        Err(MapError::ReservedKey)
    );
    // A key put twice, whatever the values and the distance.
    assert_eq!(
        ok().put(1, Value::Null).build(),
        Err(MapError::DuplicateKey)
    );
    assert_eq!(
        ok().put(2, Value::Null)
            .put(3, Value::Null)
            .put(1, Value::Uint(1))
            .build(),
        Err(MapError::DuplicateKey)
    );
    // `crit` cannot list itself.
    assert_eq!(ok().crit(&[0]).build(), Err(MapError::BadCrit));
    assert_eq!(ok().crit(&[1, 0]).build(), Err(MapError::BadCrit));
    assert_eq!(ok().crit(&[3]).crit(&[0]).build(), Err(MapError::BadCrit));
    // A listed key that is never put is fine (the receiver must understand it, not receive it).
    assert!(ok().crit(&[3]).build().is_ok());
}

#[test]
fn c12_what_the_builder_writes_the_reader_accepts() {
    let mut rng = SplitMix64::new(0xc12);
    for case in 0..5000 {
        let mut fields: Vec<(u64, u64)> = Vec::new();
        for _ in 0..rng.below(8) {
            let k = 1 + rng.below(12);
            if !fields.iter().any(|(f, _)| *f == k) {
                fields.push((k, rng.next_u64()));
            }
        }
        let crit: Vec<u64> = (0..rng.below(4)).map(|_| 1 + rng.below(14)).collect();
        let mut b = IntMapBuilder::new().crit(&crit);
        for (k, v) in &fields {
            b = b.put(*k, Value::Uint(*v));
        }
        let wire = encode(&b.build().expect("builds"), &L).expect("encodes");
        let v = decode(&wire, &L).unwrap_or_else(|e| {
            panic!("case {case}: the builder wrote bytes decode refuses: {e:?}")
        });
        let mut m = IntMap::new(&v).expect("a struct");
        let mut listed = crit.clone();
        listed.sort_unstable();
        listed.dedup();
        assert_eq!(m.crit_keys(), Ok(listed.clone()), "case {case}");
        for (k, want) in &fields {
            assert_eq!(m.req_u64(*k), Ok(*want), "case {case}");
        }
        let first_unknown_crit = listed.iter().find(|k| !fields.iter().any(|(f, _)| f == *k));
        match (m.finish(), first_unknown_crit) {
            (Ok(rest), None) => assert!(rest.is_empty(), "case {case}"),
            (Err(e), Some(k)) => assert_eq!(e, ShapeError::CritUnknown(*k), "case {case}"),
            (got, want) => {
                panic!("case {case}: finish gave {got:?}, expected an unknown crit key {want:?}")
            }
        }
    }
}
