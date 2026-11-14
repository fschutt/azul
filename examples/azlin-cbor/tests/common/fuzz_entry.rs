//! The entry functions of the fuzz targets (`examples/azlin-cbor/fuzz/fuzz_targets`), which
//! `tests/fuzz_inputs.rs` also runs on a few thousand fixed inputs: a panic is a failed test here
//! and an abort in the shipped profile. The file is self-contained (it names only `azlin_cbor`)
//! because the fuzz targets include it by `#[path]`.
//!
//! The oracle is the signature rule: what `decode` accepts, `encode` writes again byte for byte.

use azlin_cbor::{decode, encode, IntMap, IntMapBuilder, Limits, ShapeError, Value};

/// Limits so small that most inputs reach one of them: they keep the refusal paths in reach.
pub const TINY: Limits = Limits {
    max_input: 64,
    max_depth: 2,
    max_items: 4,
    max_total_items: 12,
    max_text: 8,
};

/// `decode` under every set of limits; an accepted input must re-encode to itself under the
/// limits that accepted it, and decode again to the same value.
pub fn decode_entry(data: &[u8]) {
    for l in [&Limits::HEADER, &Limits::LOG_RECORD, &Limits::OBJECT, &TINY] {
        if let Ok(v) = decode(data, l) {
            let again =
                encode(&v, l).expect("an accepted value encodes under the limits that accepted it");
            assert_eq!(
                again, data,
                "decode accepted bytes that are not the canonical encoding"
            );
            assert_eq!(
                decode(&again, l),
                Ok(v),
                "the canonical encoding decodes to another value"
            );
        }
    }
}

/// `IntMap` on whatever `decode` accepts (every accessor on every key it holds and on a few it
/// does not), and `IntMapBuilder` round-tripped through `encode`, `decode` and `IntMap`.
pub fn int_map_entry(data: &[u8]) {
    if let Ok(v) = decode(data, &Limits::OBJECT) {
        read_everything(&v);
    }
    builder_round_trip(data);
}

fn read_everything(v: &Value) {
    let Ok(mut m) = IntMap::new(v) else { return };
    let _ = m.crit_keys();
    if let Some(map) = v.as_map() {
        for (k, _) in map.iter() {
            let Some(k) = k.as_uint() else { continue };
            // One accessor per key, chosen by the key, so each key is read once.
            match k % 9 {
                0 => {
                    let _ = m.req_u64(k);
                }
                1 => {
                    let _ = m.opt_bytes(k);
                }
                2 => {
                    let _ = m.req_bytes_n::<32>(k);
                }
                3 => {
                    let _ = m.opt_text(k);
                }
                4 => {
                    let _ = m.req_array(k);
                }
                5 => {
                    let _ = m.opt_bool(k);
                }
                6 => {
                    let _ = m.req_map(k);
                }
                7 => {
                    let _ = m.opt_bytes_n::<4>(k);
                }
                _ => {
                    let _ = m.req_value(k);
                }
            }
        }
    }
    // Reading a key twice, key 0 and keys that are absent are errors or `None`, never panics.
    let _ = m.req_u64(u64::MAX);
    let _ = m.opt_u64(u64::MAX);
    let _ = m.req_u64(0);
    let _ = m.finish();
}

/// The bytes become fields: pairs of (key, value), the key's bit 4 puts it in `crit`.
fn builder_round_trip(data: &[u8]) {
    let mut builder = IntMapBuilder::new();
    let mut fields: Vec<(u64, u64)> = Vec::new();
    let mut crit: Vec<u64> = Vec::new();
    for pair in data.chunks(2).take(24) {
        let first = pair.first().copied().unwrap_or(0);
        let value = u64::from(pair.get(1).copied().unwrap_or(0));
        let key = u64::from(first & 0x0f);
        if first & 0x10 != 0 {
            crit.push(key);
        }
        fields.push((key, value));
        builder = builder.put(key, Value::Uint(value));
    }
    let builder = builder.crit(&crit);
    let has_zero = fields.iter().any(|(k, _)| *k == 0);
    let mut seen = std::collections::BTreeSet::new();
    let has_duplicate = fields.iter().any(|(k, _)| !seen.insert(*k));
    let value = match builder.build() {
        Ok(v) => v,
        Err(_) => {
            assert!(
                has_zero || has_duplicate || crit.contains(&0),
                "the builder refused a valid struct"
            );
            return;
        }
    };
    assert!(
        !has_zero && !has_duplicate && !crit.contains(&0),
        "the builder accepted an invalid struct"
    );
    let bytes = encode(&value, &Limits::OBJECT).expect("a small struct encodes");
    let back = decode(&bytes, &Limits::OBJECT).expect("what encode wrote decodes");
    assert_eq!(back, value);
    let mut m = IntMap::new(&back).expect("a built struct is an IntMap");
    for (key, want) in &fields {
        assert_eq!(m.req_u64(*key).expect("every put key reads back"), *want);
    }
    let all_listed_keys_were_read = crit.iter().all(|k| fields.iter().any(|(f, _)| f == k));
    match m.finish() {
        Ok(unknown) => {
            assert!(
                all_listed_keys_were_read,
                "finish accepted a crit key nobody read"
            );
            assert!(
                unknown.is_empty(),
                "every put key was read, nothing is unknown"
            );
        }
        Err(e) => {
            let missing = crit
                .iter()
                .copied()
                .filter(|k| !fields.iter().any(|(f, _)| f == k))
                .min();
            assert_eq!(
                Some(e),
                missing.map(ShapeError::CritUnknown),
                "finish refused for another reason"
            );
        }
    }
}
