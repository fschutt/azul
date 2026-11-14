//! C8: the limits hold before any recursion and before any allocation: a hostile input is refused
//! with a typed error after reading a few bytes, whatever it claims.

mod common;

use azlin_cbor::{decode, encode, DecodeError, EncodeError, Limits, Value};
use common::{diag, from_hex, gen_value, SplitMix64, BIG};

/// `n` nested containers (n >= 1): n - 1 arrays of one item around an empty array.
fn nested_arrays(n: usize) -> Vec<u8> {
    let mut b = vec![0x81; n - 1];
    b.push(0x80);
    b
}

/// `n` nested maps (n >= 1): n - 1 maps of one pair (key 0) around an empty map.
fn nested_maps(n: usize) -> Vec<u8> {
    let mut b = Vec::new();
    for _ in 1..n {
        b.extend_from_slice(&[0xa1, 0x00]);
    }
    b.push(0xa0);
    b
}

#[test]
fn c8_the_three_constants_are_what_p3_says() {
    // P-3 of the brief and DRIFT X-DELTA-W1S14A-3: the numbers live in limits.rs, in one place.
    assert_eq!(
        Limits::HEADER,
        Limits {
            max_input: 16 * 1024,
            max_depth: 8,
            max_items: 1024,
            max_total_items: 4096,
            max_text: 16 * 1024
        }
    );
    assert_eq!(
        Limits::LOG_RECORD,
        Limits {
            max_input: 64 * 1024,
            max_depth: 8,
            max_items: 1024,
            max_total_items: 16384,
            max_text: 64 * 1024
        }
    );
    assert_eq!(
        Limits::OBJECT,
        Limits {
            max_input: 4 * 1024 * 1024,
            max_depth: 12,
            max_items: 65536,
            max_total_items: 1_048_576,
            max_text: 4 * 1024 * 1024
        }
    );
}

#[test]
fn c8_limits_hold_before_allocation_or_recursion() {
    // 100,000 nested arrays and maps: refused at the 13th level, on a thread whose stack could
    // not hold even a hundred levels of an unbounded recursion.
    for (name, bytes) in [
        ("arrays", nested_arrays(100_000)),
        ("maps", nested_maps(50_000)),
    ] {
        let handle = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || (decode(&bytes, &Limits::OBJECT), decode(&bytes, &BIG)))
            .expect("spawns a thread");
        let (object, big) = handle.join().expect("no stack overflow, no panic");
        assert_eq!(
            object,
            Err(DecodeError::DepthExceeded),
            "{name} under OBJECT"
        );
        assert_eq!(
            big,
            Err(DecodeError::DepthExceeded),
            "{name} under the most permissive depth (255)"
        );
    }
    // The input limit comes first: 100,000 bytes do not fit a header.
    assert_eq!(
        decode(&nested_arrays(100_000), &Limits::HEADER),
        Err(DecodeError::InputTooLarge)
    );
}

#[test]
fn c8_depth_boundaries_are_exact() {
    for max in [0u8, 1, 2, 8, 12] {
        let l = Limits {
            max_depth: max,
            ..Limits::OBJECT
        };
        for n in 1..=16usize {
            let fits = n <= usize::from(max);
            for (name, bytes) in [("arrays", nested_arrays(n)), ("maps", nested_maps(n))] {
                let got = decode(&bytes, &l);
                if fits {
                    assert!(
                        got.is_ok(),
                        "{n} nested {name} under max_depth {max}: {got:?}"
                    );
                } else {
                    assert_eq!(
                        got,
                        Err(DecodeError::DepthExceeded),
                        "{n} nested {name} under max_depth {max}"
                    );
                }
            }
        }
        // A scalar is depth 0: it fits every limit.
        assert!(decode(&[0x00], &l).is_ok());
    }
    // A container inside a key counts too: the map is depth 1, its key [] depth 2, its key [[]] 3.
    let l = Limits {
        max_depth: 2,
        ..Limits::OBJECT
    };
    assert!(decode(&from_hex("a18000"), &l).is_ok());
    assert_eq!(
        decode(&from_hex("a1818000"), &l),
        Err(DecodeError::DepthExceeded)
    );
    // The encoder counts the same: 8 nested containers fit a header, 9 do not.
    let deep = |n: usize| (1..n).fold(Value::Array(vec![]), |inner, _| Value::Array(vec![inner]));
    assert!(encode(&deep(8), &Limits::HEADER).is_ok());
    assert_eq!(
        encode(&deep(9), &Limits::HEADER),
        Err(EncodeError::DepthExceeded)
    );
}

#[test]
fn c8_a_claimed_count_is_refused_before_it_is_believed() {
    // An array declaring 2^32 - 1 items in a 10-byte input.
    let ten = from_hex("9affffffff0000000000");
    assert_eq!(ten.len(), 10);
    assert_eq!(
        decode(&ten, &Limits::OBJECT),
        Err(DecodeError::TooManyItems)
    );
    assert_eq!(
        decode(&ten, &Limits::HEADER),
        Err(DecodeError::TooManyItems)
    );
    // Under limits that believe any count: the claim is more than the item budget, still refused.
    assert_eq!(decode(&ten, &BIG), Err(DecodeError::TooManyItems));
    // Claims the budget allows but the input cannot hold: `Truncated`, nothing reserved for them
    // (an allocation proportional to 4,000,000,000 items would abort this test).
    for claim in [
        65_536u32,
        1 << 20,
        1 << 24,
        1 << 30,
        4_000_000_000,
        u32::MAX - 1,
    ] {
        let mut array = vec![0x9a];
        array.extend_from_slice(&claim.to_be_bytes());
        array.extend_from_slice(&[0x00; 5]);
        assert_eq!(
            decode(&array, &BIG),
            Err(DecodeError::Truncated),
            "array of {claim}"
        );
    }
    for claim in [
        65_536u32,
        1 << 20,
        1 << 24,
        1 << 30,
        2_000_000_000,
        i32::MAX as u32,
    ] {
        let mut map = vec![0xba];
        map.extend_from_slice(&claim.to_be_bytes());
        // Three pairs with ascending keys, then the input ends.
        map.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x02, 0x00]);
        assert_eq!(
            decode(&map, &BIG),
            Err(DecodeError::Truncated),
            "map of {claim}"
        );
    }
    // Eight-byte lengths.
    for hex in [
        "9bffffffffffffffff",
        "bbffffffffffffffff",
        "9b0000000100000000",
        "bb0000000100000000",
    ] {
        assert_eq!(
            decode(&from_hex(hex), &BIG),
            Err(DecodeError::TooManyItems),
            "{hex}"
        );
        assert_eq!(
            decode(&from_hex(hex), &Limits::OBJECT),
            Err(DecodeError::TooManyItems),
            "{hex}"
        );
    }
    // Strings: the claim is checked against `max_text`, then against the input that is left.
    for initial in [0x5au8, 0x7a] {
        let mut s = vec![initial];
        s.extend_from_slice(&u32::MAX.to_be_bytes());
        s.extend_from_slice(&[0x61; 5]);
        assert_eq!(
            decode(&s, &BIG),
            Err(DecodeError::Truncated),
            "a string of 4 GiB in 10 bytes"
        );
        assert_eq!(decode(&s, &Limits::OBJECT), Err(DecodeError::InputTooLarge));
    }
    for hex in ["5bffffffffffffffff", "7bffffffffffffffff"] {
        assert_eq!(
            decode(&from_hex(hex), &BIG),
            Err(DecodeError::InputTooLarge),
            "{hex}"
        );
    }
    // A claim inside the limits and above the input: `Truncated`.
    assert_eq!(
        decode(&from_hex("990100"), &Limits::OBJECT),
        Err(DecodeError::Truncated)
    );
    assert_eq!(
        decode(&from_hex("9a00010000"), &Limits::OBJECT),
        Err(DecodeError::Truncated)
    );
}

#[test]
fn c8_max_items_and_max_total_items_are_refused() {
    // max_items: per array and per map (pairs).
    let l = Limits {
        max_items: 3,
        ..Limits::OBJECT
    };
    assert!(decode(&from_hex("83010203"), &l).is_ok());
    assert_eq!(
        decode(&from_hex("8401020304"), &l),
        Err(DecodeError::TooManyItems)
    );
    assert!(decode(&from_hex("a3000001000200"), &l).is_ok());
    assert_eq!(
        decode(&from_hex("a40000010002000300"), &l),
        Err(DecodeError::TooManyItems)
    );
    // max_total_items counts every data item: containers, keys, values, elements.
    // [[1, 2], [3, 4]] is 7 items; {1: 2, 3: 4} is 5.
    let nested = from_hex("82820102820304");
    let map = from_hex("a201020304");
    for (bytes, items) in [(&nested, 7u32), (&map, 5u32)] {
        let at = Limits {
            max_total_items: items,
            ..Limits::OBJECT
        };
        let under = Limits {
            max_total_items: items - 1,
            ..Limits::OBJECT
        };
        assert!(
            decode(bytes, &at).is_ok(),
            "{items} items fit a total of {items}"
        );
        assert_eq!(
            decode(bytes, &under),
            Err(DecodeError::TooManyItems),
            "{items} items under {}",
            items - 1
        );
    }
    // The scalar alone is one item.
    assert!(decode(
        &[0x00],
        &Limits {
            max_total_items: 1,
            ..Limits::OBJECT
        }
    )
    .is_ok());
    assert_eq!(
        decode(
            &[0x00],
            &Limits {
                max_total_items: 0,
                ..Limits::OBJECT
            }
        ),
        Err(DecodeError::TooManyItems)
    );
    // Header-sized totals: 1024 items fit a header, the 1025th does not.
    let mut a = vec![0x99, 0x04, 0x00];
    a.extend_from_slice(&[0x00; 1024]);
    assert!(decode(&a, &Limits::HEADER).is_ok());
    let mut b = vec![0x99, 0x04, 0x01];
    b.extend_from_slice(&[0x00; 1025]);
    assert_eq!(decode(&b, &Limits::HEADER), Err(DecodeError::TooManyItems));
    // Three arrays of 1024 items are 3 * 1025 + 1 = 3076 items; four are 4101 and pass 4096,
    // though 4 * 1027 bytes are far below 16 KiB.
    let one = {
        let mut x = vec![0x99, 0x04, 0x00];
        x.extend_from_slice(&[0x00; 1024]);
        x
    };
    for (count, fits) in [(3u8, true), (4, false)] {
        let mut many = vec![0x80 | count];
        for _ in 0..count {
            many.extend_from_slice(&one);
        }
        let got = decode(&many, &Limits::HEADER);
        if fits {
            assert!(got.is_ok(), "{count} arrays of 1024: {got:?}");
        } else {
            assert_eq!(
                got,
                Err(DecodeError::TooManyItems),
                "{count} arrays of 1024"
            );
        }
    }
}

#[test]
fn c8_max_text_and_max_input_are_refused() {
    let l = Limits {
        max_text: 16,
        ..Limits::OBJECT
    };
    let item = |head: u8, n: usize| {
        let mut b = Vec::new();
        if n < 24 {
            b.push(head | u8::try_from(n).expect("short"));
        } else {
            b.extend_from_slice(&[head | 24, u8::try_from(n).expect("short")]);
        }
        b.extend(std::iter::repeat_n(0x61u8, n));
        b
    };
    for head in [0x40u8, 0x60] {
        assert!(
            decode(&item(head, 16), &l).is_ok(),
            "16 bytes fit max_text 16"
        );
        assert_eq!(
            decode(&item(head, 17), &l),
            Err(DecodeError::InputTooLarge),
            "17 bytes do not"
        );
        // The claim alone is enough: no content follows.
        assert_eq!(
            decode(&[head | 24, 30], &l),
            Err(DecodeError::InputTooLarge)
        );
    }
    // A string inside a container, as a key and as a value.
    assert_eq!(
        decode(&[&[0x81u8][..], &item(0x60, 17)].concat(), &l),
        Err(DecodeError::InputTooLarge)
    );
    // max_input: the whole input.
    let exact = vec![0x59, 0x3f, 0xfd]
        .into_iter()
        .chain(std::iter::repeat_n(0u8, 16381))
        .collect::<Vec<u8>>();
    assert_eq!(exact.len(), 16 * 1024);
    assert!(
        decode(&exact, &Limits::HEADER).is_ok(),
        "exactly 16 KiB fits a header"
    );
    let mut over = exact.clone();
    over[2] = 0xfe;
    over.push(0);
    assert_eq!(over.len(), 16 * 1024 + 1);
    assert_eq!(
        decode(&over, &Limits::HEADER),
        Err(DecodeError::InputTooLarge)
    );
    // Before anything is read: 16 KiB + 1 zero bytes are too large, 16 KiB of them are two items.
    assert_eq!(
        decode(&vec![0u8; 16 * 1024 + 1], &Limits::HEADER),
        Err(DecodeError::InputTooLarge)
    );
    assert_eq!(
        decode(&vec![0u8; 16 * 1024], &Limits::HEADER),
        Err(DecodeError::TrailingBytes)
    );
    let none = Limits {
        max_input: 0,
        ..Limits::OBJECT
    };
    assert_eq!(decode(&[], &none), Err(DecodeError::Truncated));
    assert_eq!(decode(&[0x00], &none), Err(DecodeError::InputTooLarge));
    // The encoder writes no more than the limit.
    assert!(encode(&Value::Bytes(vec![0; 16381]), &Limits::HEADER).is_ok());
    assert_eq!(
        encode(&Value::Bytes(vec![0; 16382]), &Limits::HEADER),
        Err(EncodeError::InputTooLarge)
    );
    assert_eq!(
        encode(&Value::Text("a".repeat(17)), &l),
        Err(EncodeError::InputTooLarge)
    );
    assert!(encode(&Value::Text("a".repeat(16)), &l).is_ok());
    assert_eq!(
        encode(&Value::Array(vec![Value::Null; 1025]), &Limits::HEADER),
        Err(EncodeError::TooManyItems)
    );
    assert_eq!(
        encode(
            &Value::Array(vec![Value::Null; 11]),
            &Limits {
                max_total_items: 11,
                ..Limits::OBJECT
            }
        ),
        Err(EncodeError::TooManyItems),
        "12 items (the array and 11 elements) pass a total of 11"
    );
}

#[test]
fn c8_encoder_and_decoder_agree_on_what_fits() {
    let sets = [
        Limits::HEADER,
        Limits {
            max_input: 40,
            max_depth: 3,
            max_items: 4,
            max_total_items: 10,
            max_text: 6,
        },
        Limits {
            max_input: 200,
            max_depth: 5,
            max_items: 8,
            max_total_items: 40,
            max_text: 20,
        },
        Limits {
            max_input: 1000,
            max_depth: 2,
            max_items: 30,
            max_total_items: 100,
            max_text: 300,
        },
        Limits::OBJECT,
    ];
    let mut rng = SplitMix64::new(0xc8c8);
    let (mut fits, mut over) = (0u32, 0u32);
    for case in 0..6000 {
        let v = gen_value(&mut rng, 4);
        let wide = encode(&v, &BIG).expect("the value encodes under no limits");
        for l in &sets {
            let written = encode(&v, l);
            let read = decode(&wide, l);
            assert_eq!(
                written.is_ok(),
                read.is_ok(),
                "case {case}: encode {written:?}, decode {read:?} under {l:?} for {}",
                diag(&v)
            );
            if let (Ok(bytes), Ok(back)) = (&written, &read) {
                assert_eq!(bytes, &wide, "case {case}");
                assert_eq!(back, &v, "case {case}");
                fits += 1;
            } else {
                over += 1;
            }
        }
    }
    assert!(fits > 1000 && over > 1000, "{fits} fit, {over} do not");
}
