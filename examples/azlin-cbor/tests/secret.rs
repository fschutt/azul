//! C15: `decode_secret` is `decode` with a wipe, nothing else: on every golden vector (the valid ones
//! give the same value, the refused ones the same error) and on seeded random and damaged inputs under
//! five sets of limits. And the shape of `SecretValue`: not `Clone`, a redacted `Debug`, the value
//! given to the readers by `as_value`. The counted wipe (C16, C17) is in `src/secret_tests.rs`: its
//! counter exists only under `cfg(test)` of the library.

mod common;

use azlin_cbor::{
    decode, decode_secret, encode, DecodeError, IntMap, IntMapBuilder, Limits, SecretValue, Value,
};
use common::fuzz_entry::TINY;
use common::{from_hex, gen_value, load_vectors, mutate, SplitMix64, BIG};

/// What `decode` and `decode_secret` said about one input: equal values, or equal errors.
fn assert_same(
    plain: &Result<Value, DecodeError>,
    secret: &Result<SecretValue, DecodeError>,
    what: &str,
) -> bool {
    match (plain, secret) {
        (Ok(v), Ok(s)) => {
            assert_eq!(v, s.as_value(), "{what}: not the same value");
            true
        }
        (Err(a), Err(b)) => {
            assert_eq!(a, b, "{what}: not the same error");
            false
        }
        (Ok(v), Err(e)) => panic!("{what}: decode accepts {v:?}, decode_secret refuses with {e:?}"),
        (Err(e), Ok(_)) => panic!("{what}: decode refuses with {e:?}, decode_secret accepts"),
    }
}

#[test]
fn c15_decode_secret_equals_decode_on_every_golden_vector() {
    let rows = load_vectors();
    let (mut valid, mut refused) = (0usize, 0usize);
    for r in &rows {
        let bytes = from_hex(&r.hex);
        // Both entry points over the same vector file, under the limits the file is written for.
        let plain = decode(&bytes, &Limits::OBJECT);
        let secret = decode_secret(&bytes, &Limits::OBJECT);
        let accepted = assert_same(&plain, &secret, &r.name);
        assert_eq!(accepted, r.valid, "{}: the vector's own verdict", r.name);
        if let Err(e) = &secret {
            assert_eq!(
                format!("{e:?}"),
                r.expect,
                "{}: the error the file names",
                r.name
            );
        }
        if accepted {
            valid += 1;
            // The same value, and the canonical bytes again: the signature rule holds for it.
            let again = encode(
                secret.as_ref().expect("accepted").as_value(),
                &Limits::OBJECT,
            );
            assert_eq!(
                again,
                Ok(bytes),
                "{}: encode(decode_secret(b)) == b",
                r.name
            );
        } else {
            refused += 1;
        }
    }
    assert!(
        valid >= 60 && refused >= 90,
        "{valid} valid and {refused} refused vectors"
    );
}

#[test]
fn c15_decode_secret_equals_decode_on_seeded_inputs() {
    let sets = [
        Limits::HEADER,
        Limits::LOG_RECORD,
        Limits::OBJECT,
        TINY,
        BIG,
    ];
    let mut inputs: Vec<Vec<u8>> = Vec::new();
    let mut rng = SplitMix64::new(0x15);
    // Random bytes behind every initial byte.
    for head in 0..=255u8 {
        for _ in 0..8 {
            let len = rng.below(40) as usize;
            let mut data = vec![head];
            data.extend(rng.bytes(len));
            inputs.push(data);
        }
    }
    // Random trees, whole and damaged, and the vectors damaged.
    for _ in 0..2_500 {
        let v = gen_value(&mut rng, 4);
        let wire = encode(&v, &BIG).expect("encodes");
        for _ in 0..3 {
            inputs.push(mutate(&mut rng, &wire));
        }
        inputs.push(wire);
    }
    for r in load_vectors() {
        let bytes = from_hex(&r.hex);
        for _ in 0..8 {
            inputs.push(mutate(&mut rng, &bytes));
        }
        inputs.push(bytes);
    }
    let (mut accepted, mut refused) = (0usize, 0usize);
    for (i, data) in inputs.iter().enumerate() {
        for l in &sets {
            let plain = decode(data, l);
            let secret = decode_secret(data, l);
            if assert_same(&plain, &secret, &format!("input {i} under {l:?}")) {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    assert!(inputs.len() > 10_000, "{} inputs", inputs.len());
    assert!(
        accepted > 5_000 && refused > 5_000,
        "{accepted} accepted, {refused} refused"
    );
}

#[test]
fn c15_secret_value_is_not_clone_and_shows_no_content() {
    // Compile time: if `SecretValue` were `Clone`, the two impls would both apply and the call of
    // `probe` below would be ambiguous, which does not compile.
    trait AmbiguousIfClone<A> {
        fn probe() {}
    }
    impl<T: ?Sized> AmbiguousIfClone<()> for T {}
    impl<T: Clone> AmbiguousIfClone<u8> for T {}
    let _ = <SecretValue as AmbiguousIfClone<_>>::probe;

    let secret = SecretValue::new(Value::Array(vec![
        Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
        Value::Text("hunter2".into()),
    ]));
    for shown in [format!("{secret:?}"), format!("{secret:#?}")] {
        assert_eq!(shown, "SecretValue(<redacted>)");
    }
}

#[test]
fn c15_as_value_serves_the_readers_and_the_encoder() {
    let built = IntMapBuilder::new()
        .put(1, Value::Uint(2))
        .put(2, Value::Bytes(vec![9; 32]))
        .crit(&[1])
        .build()
        .expect("builds");
    let wire = encode(&built, &Limits::OBJECT).expect("encodes");
    let secret = decode_secret(&wire, &Limits::OBJECT).expect("decodes");
    let mut m = IntMap::new(secret.as_value()).expect("a struct");
    assert_eq!(m.req_u64(1), Ok(2));
    assert_eq!(m.req_bytes(2), Ok(&[9u8; 32][..]));
    assert_eq!(m.finish(), Ok(vec![]));
    // The tree is the one `decode` returns, and encodes back to the bytes it came from.
    assert_eq!(
        secret.as_value(),
        &decode(&wire, &Limits::OBJECT).expect("decodes")
    );
    assert_eq!(encode(secret.as_value(), &Limits::OBJECT), Ok(wire));
}
